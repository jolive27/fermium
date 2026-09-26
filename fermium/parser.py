"""Parser: tokens -> AST.

Precedence, lowest to highest (see DECISIONS.md, "Implicit multiplication"):

    expr_full  := expr ['in' unit]                 conversion for display
    expr       := 'if' expr 'then' expr 'else' expr | or
    or         := and ('or' and)*
    and        := not ('and' not)*
    not        := 'not' not | compare
    compare    := sum [cmp_op sum]
    sum        := product (('+' | '-') product)*
    product    := unary (('*' | '/') unary)*         explicit * and /
    unary      := '-' unary | '+' unary | juxt
    juxt       := power power*                       implicit multiplication binds TIGHTER than / and *
    power      := postfix ['^' exponent | superscript]   (right-assoc)
    postfix    := atom ( '(' args ')' | '[' index ']' | '.' name | ' )*   (no space before ( and [)
    atom       := number [unit] | name | string | '(' expr_full ')' | '[' list ']' | '|' expr '|'
                | √ power | ∫ ... | d/dt power | ∂/∂x power | load "file" | true | false

So `h c / λ k_B T` = (h c)/(λ k_B T), and `1/2 m v²` = 1/(2 m v²) (with a warning).
"""
from __future__ import annotations

from fractions import Fraction

from . import ast as A
from .errors import FermiumError, Diagnostics
from .lexer import Token, tokenize, canonical_name
from .units import is_unit_name


def _is_constant(name):
    """Is this a built-in constant's name (c, AU, M☉ are units too, with the same value)?"""
    from .constants import CONSTANTS
    if not hasattr(_is_constant, "names"):
        _is_constant.names = set(CONSTANTS) | {a for v in CONSTANTS.values() for a in v[3]}
    return name in _is_constant.names

CMP_OPS = {"==", "!=", "<", ">", "<=", ">=", "~="}
AUG_OPS = {"+=", "-=", "*=", "/="}


# Unit names that are also built-in functions: after a number (`15.3 / min`) they are the unit.
UNITS_NAMED_LIKE_BUILTINS = {"min"}

VEC_CALC_WORDS ={"grad": "grad", "div": "div", "curl": "curl", "laplacian": "lap"}


# keywords that are also the ASCII spelling of a symbol: `integral(b, T) = …` gets a clear error (#78)
KEYWORD_SPELLED = {"integral": "∫", "partial": "∂", "sqrt": "√", "cbrt": "∛", "nabla": "∇"}


UNIT_WORDS = {"g": "grams", "m": "metres", "s": "seconds", "L": "litres", "l": "litres", "V": "volts",
              "T": "tesla", "b": "barns", "A": "amperes", "K": "kelvin", "N": "newtons", "J": "joules",
              "W": "watts", "C": "coulombs", "F": "farads", "H": "henries", "Pa": "pascals", "u": "atomic mass units",
              "d": "days", "min": "minutes", "yr": "years", "h": "hours", "t": "tonnes", "au": "AU", "pc": "parsecs"}


class Parser:
    def __init__(self, tokens: list[Token], diags: Diagnostics | None = None, known=None):
        self.toks = tokens
        self.i = 0
        self.diags = diags or Diagnostics()
        self.known = set(known or ())   # names assigned so far (for unit/variable collisions)
        self.module_names = set()        # names bound by `import m [as a]` (for the hint on `m.x = …`)
        self.abs_depth = 0
        self.no_juxt_names = set()
        self.warned_units = set()
        self.in_integrand = 0
        self.limit_start = None      # token index where an integral's upper limit starts (FRICTION #8)
        self._named = None           # every name the program assigns, defines or loops over (D215)
        self.deriv_vars = set()      # variables of the d/ds, ∂/∂s whose operand is being parsed (D231)
        self.solve_unknowns = set()  # the unknowns of the solve being parsed: names written with a prime (D211)
        self.fix_mode = False        # `fmt --fix`: rewrite unit/variable collisions instead of stopping (D235)
        self.fixes = []              # fix mode: (start, end, replacement) edits in the source
        self._fix_whole = []         # fix mode: units to bracket whole once read (v1 read on through a collision)

    # ------------------------------------------------------------ helpers
    @property
    def tok(self) -> Token:
        return self.toks[self.i]

    def peek(self, k=1) -> Token:
        j = min(self.i + k, len(self.toks) - 1)
        return self.toks[j]

    def next(self) -> Token:
        t = self.toks[self.i]
        if self.i < len(self.toks) - 1:
            self.i += 1
        return t

    def at_op(self, *ops):
        return self.tok.kind == "OP" and self.tok.value in ops

    def at_kw(self, *kws):
        return self.tok.kind == "KW" and self.tok.value in kws

    def expect_op(self, op, what=None):
        if not self.at_op(op):
            raise self.error(f"expected '{op}'" + (f" {what}" if what else "") + self._found())
        return self.next()

    def expect_kw(self, kw, what=None):
        if not self.at_kw(kw):
            raise self.error(f"expected '{kw}'" + (f" {what}" if what else "") + self._found())
        return self.next()

    def expect_name(self, what="a name"):
        if self.tok.kind != "NAME":
            raise self.error(f"expected {what}" + self._found())
        return self.next()

    def _found(self):
        t = self.tok
        if t.kind == "NEWLINE":
            return " but the line ended"
        if t.kind == "EOF":
            return " but the program ended"
        if t.kind in ("INDENT", "DEDENT"):
            return " but the indentation changed"
        return f" but found '{t.raw}'"

    def error(self, msg, tok=None, hint=None):
        t = tok or self.tok
        if t.kind in ("EOF", "NEWLINE", "DEDENT") and tok is None:
            j = self.i - 1
            while j > 0 and self.toks[j].kind in ("EOF", "NEWLINE", "DEDENT", "INDENT"):
                j -= 1
            prev = self.toks[j]
            return FermiumError(msg, prev.line, prev.col + len(prev.raw), 1, hint)
        return FermiumError(msg, t.line, t.col, max(1, len(t.raw)), hint)

    def node(self, cls, tok, *args, **kw):
        n = cls(*args, **kw)
        n.line, n.col = tok.line, tok.col
        n.length = max(1, self.toks[self.i - 1].end - tok.start) if self.i > 0 and \
            self.toks[self.i - 1].line == tok.line else max(1, len(tok.raw))
        return n

    def span(self, n, tok):
        """Set n's position to run from tok to the previous token."""
        n.line, n.col = tok.line, tok.col
        prev = self.toks[self.i - 1]
        n.length = max(1, prev.end - tok.start) if prev.line == tok.line else max(1, len(tok.raw))
        return n

    def skip_newlines(self):
        while self.tok.kind == "NEWLINE":
            self.next()

    # ------------------------------------------------------------ program
    def parse_program(self) -> A.Program:
        body = []
        self.skip_newlines()
        while self.tok.kind != "EOF":
            if self.tok.kind == "INDENT":
                raise self.error("this line is indented but isn't inside a block",
                                 hint="remove the spaces at the start of the line")
            if self.tok.kind == "DEDENT":
                self.next()
                continue
            body.append(self.statement())
            self.skip_newlines()
        prog = A.Program(body)
        return prog

    def block(self):
        """NEWLINE INDENT stmt* DEDENT, or a single statement on the same line after ':'."""
        if self.at_op(":"):
            self.next()
        if self.tok.kind != "NEWLINE":
            return [self.simple_statement_only()]
        self.next()
        self.skip_newlines()
        if self.tok.kind != "INDENT":
            raise self.error("expected an indented block here",
                             hint="indent the lines that belong to this block (e.g. 4 spaces)")
        self.next()
        stmts = []
        while self.tok.kind not in ("DEDENT", "EOF"):
            stmts.append(self.statement())
            self.skip_newlines()
        if self.tok.kind == "DEDENT":
            self.next()
        return stmts

    def simple_statement_only(self):
        s = self.statement(end_line=False)
        return s

    def end_statement(self):
        if getattr(self, "_stmt_done", False):
            self._stmt_done = False
            return
        if self.tok.kind == "NEWLINE":
            self.next()
        elif self.tok.kind in ("EOF", "DEDENT"):
            pass
        elif self.at_op(";"):
            self.next()
        else:
            t = self.tok
            hint = None
            if t.kind == "OP" and t.value == "+-":
                raise self.error("± needs a value on its left, like  L = 1.20 ± 0.01 m")
            if t.kind == "OP" and t.value == "=":
                hint = "use == to compare two values; = stores a value in a variable"
                chain = self._chained_assignment(t)
                if chain is not None:
                    raise chain
            if t.kind == "OP" and t.value in ("+", "-") and self.peek().kind == "OP" and self.peek().value == t.value:
                hint = f"Fermium has no {t.value}{t.value}; write  x {t.value}= 1"
            raise self.error(f"didn't expect '{t.raw}' here", hint=hint)

    NATURAL_CONSTANTS = {"ħ", "hbar", "c", "k_B", "kB", "G", "ε₀", "ε_0", "epsilon_0", "e", "μ₀", "μ_0"}

    def _chained_assignment(self, t):
        """`ħ = c = 1` (spec A3.1): setting constants to 1 is natural units; `a = b = 1` assigns one at a time."""
        line = [tk for tk in self.toks if tk.line == t.line and tk.kind not in ("NEWLINE", "INDENT", "DEDENT", "EOF")]
        names, j = [], 0
        while j + 1 < len(line) and line[j].kind == "NAME" and line[j + 1].kind == "OP" and line[j + 1].value == "=":
            names.append(line[j])
            j += 2
        if len(names) < 2 or t not in line[:j]:
            return None
        text = " = ".join(n.raw for n in names) + " = " + "".join(
            (" " if tk.ws_before and k else "") + tk.raw for k, tk in enumerate(line[j:]))
        if all(n.raw in self.NATURAL_CONSTANTS or n.value in self.NATURAL_CONSTANTS for n in names):
            return self.error(f"'{text}' sets physical constants to 1: that's natural units", tok=t,
                              hint=f"write  units natural({text})  (or just  units natural  for ħ = c = 1)")
        return self.error(f"'{text}': Fermium gives one variable a value at a time", tok=t,
                          hint="write each on its own line, like  " + "  and  ".join(
                              f"{n.raw} = …" for n in names[:2]) + "  (== compares two values)")

    # ------------------------------------------------------------ statements
    def statement(self, end_line=True):
        t = self.tok
        if t.kind == "NAME" and t.value in ("def", "function", "fn") and self.peek().kind == "NAME":
            nm = self.peek().raw
            raise self.error(f"Fermium doesn't use '{t.raw}': a function is written like a formula",
                             hint=f"write  {nm}(x) = 2 x   (or put the body on the indented lines after {nm}(x) =)")
        if t.kind == "KW" and self.peek().kind == "OP" and self.peek().value in ({"="} | AUG_OPS) and \
                t.value not in ("print",):
            raise self.error(f"'{t.raw}' is a reserved word in Fermium, so it can't be a variable name",
                             hint=f"pick another name, e.g. {t.raw}_ or my_{t.raw}")
        if t.kind == "KW":
            kw = t.value
            handler = {
                "print": self.print_stmt, "plot": self.plot_stmt, "solve": self.solve_stmt,
                "fit": self.fit_stmt, "if": self.if_stmt, "for": self.for_stmt,
                "while": self.while_stmt, "return": self.return_stmt, "assert": self.assert_stmt,
            }.get(kw)
            if kw in ("break", "continue"):
                self.next()
                s = self.span(A.Break() if kw == "break" else A.Continue(), t)
                if end_line:
                    self.end_statement()
                return s
            if handler:
                s = handler()
                if end_line and not isinstance(s, (A.If, A.For, A.ForIn, A.While, A.Solve)):
                    self.end_statement()
                return s
        if (t.kind == "NAME" and t.value == "import" or t.kind == "KW" and t.value == "from") and \
                self.peek().kind in ("NAME", "STR"):
            s = self.import_stmt()
            if end_line:
                self.end_statement()
            return s
        if t.kind == "NAME" and t.value == "use" and self.peek().kind == "NAME" and self.peek().value == "python":
            return self.use_python_stmt(end_line)
        if t.kind == "NAME" and t.value == "parallel" and self.peek().kind == "KW" and self.peek().value == "for":
            self.next()
            s = self.for_stmt()
            if isinstance(s, A.ForIn):
                raise self.error("parallel for works with a range of numbers: write  parallel for i from 1 to n",
                                 t, hint="loop over the indexes: parallel for i from 1 to len(xs), then use xs[i]")
            s.parallel = True
            s.length = (s.col - t.col) + 3 if s.line == t.line else 8     # underline "parallel for"
            s.col = t.col
            return s
        if t.kind == "NAME" and t.value == "analyze" and self._is_analyze():
            s = self.analyze_stmt()
            if end_line:
                self.end_statement()
            return s
        if t.kind == "NAME" and t.value == "propagate" and self.peek().kind == "NAME" and \
                self.peek().value in ("montecarlo", "monte_carlo", "MonteCarlo"):
            return self.propagate_stmt()
        if t.kind == "NAME" and t.value == "units" and self.peek().kind == "NAME" and \
                self.peek().value in ("natural", "nuclear", "astro", "SI"):
            return self.units_stmt(end_line)
        if t.kind == "NAME":
            nxt = self.peek()
            if nxt.kind == "OP" and nxt.value == "=":
                s = self.assign_stmt()
                if end_line:
                    self.end_statement()
                return s
            if nxt.kind == "OP" and nxt.value in AUG_OPS:
                name = self.next()
                op = self.next().value
                val = self.expr_where()
                s = self.span(A.Assign(name.value, val, op), name)
                if end_line:
                    self.end_statement()
                return s
            if nxt.kind == "OP" and nxt.value == "(" and not nxt.ws_before and self._is_funcdef():
                s = self.funcdef()
                return s
            if nxt.kind == "OP" and nxt.value == "[" and not nxt.ws_before:
                j = self._match(self.i + 1)
                if j is not None and self.toks[j + 1].kind == "OP" and self.toks[j + 1].value in ({"="} | AUG_OPS):
                    name = self.next()
                    self.next()
                    idx = self.expr()
                    idx2 = None
                    if self.at_op(","):                  # M[i, j] = … (D195)
                        self.next()
                        idx2 = self.expr()
                    if self.at_op(":"):
                        raise self.error("a slice xs[a:b] can be read but not assigned to; set the elements "
                                         "one at a time, like  for i from a to b  then  xs[i] = ...")
                    self.expect_op("]")
                    op = self.next().value
                    val = self.expr_where()
                    s = self.span(A.IndexAssign(name.value, idx, val, op, idx2), name)
                    if end_line:
                        self.end_statement()
                    return s
        if t.kind == "KW" and t.value in KEYWORD_SPELLED and t.raw == t.value and (
                self.peek().kind == "OP" and self.peek().value == "=" or
                self.at_op_at(self.i + 1, "(") and not self.peek().ws_before and self._is_funcdef()):
            raise self._keyword_as_name(t)
        e = self.expr_where()
        if self.at_op("="):
            raise self._assign_to_non_name(t, e)
        s = self.span(A.ExprStmt(e), t)
        if end_line:
            self.end_statement()
        return s

    def propagate_stmt(self):
        """propagate montecarlo [N [samples]] + an indented block (or ':' and one formula) (D123)."""
        t = self.next()
        self.next()
        n = None
        if not (self.at_op(":") or self.tok.kind in ("NEWLINE", "EOF")):
            if self.tok.kind == "NUM" or self.tok.kind == "NAME" and self.peek().kind == "NAME":
                k = self.next()        # `100000 samples`, `N samples`: not a number with a unit
                n = A.Num(k.value, k.sigfigs).at(k) if k.kind == "NUM" else A.Name(k.value).at(k)
            else:
                n = self.sum()
            if self.tok.kind == "NAME" and self.tok.value in ("samples", "sample"):
                self.next()
        if not (self.at_op(":") or self.tok.kind == "NEWLINE"):
            raise self.error("write  propagate montecarlo 100000 samples  and put the formulas on the indented "
                             "lines below it")
        body = self.block()
        return self.span(A.Propagate(n, body), t)

    def units_stmt(self, end_line=True):
        """units natural(ħ = c = 1) | units nuclear | units astro | units SI, optionally with ':' + a block (D60)."""
        t = self.next()
        system = self.next().value
        consts = []
        if self.at_op("("):
            self.next()
            while True:
                names = [self.expect_name("a constant, like ħ or c").value]
                while True:
                    self.expect_op("=", "(write it like  units natural(ħ = c = 1))")
                    if self.tok.kind == "NUM":
                        break
                    names.append(self.expect_name("a constant, like ħ or c").value)
                num = self.next()
                if float(num.value) != 1:
                    raise self.error("natural units set constants to 1, like  units natural(ħ = c = 1)", num)
                consts += names
                if self.at_op(","):
                    self.next()
                    continue
                break
            self.expect_op(")")
        s = self.span(A.Units(system, consts), t)
        if self.at_op(":"):
            s.body = self.block()
        elif end_line:
            self.end_statement()
        return s

    def at_op_at(self, j, value):
        tk = self.toks[j] if j < len(self.toks) else None
        return tk is not None and tk.kind == "OP" and tk.value == value

    def _keyword_as_name(self, t):
        """`integral(b, T) = …`: integral is the ASCII spelling of ∫ (gauntlet #78)."""
        sym = KEYWORD_SPELLED[t.value]
        what = f"the ASCII spelling of {sym}" if sym else "a Fermium keyword"
        return self.error(f"{t.raw} is {what}, so it can't be the name of a function or variable; pick another name",
                          tok=t, hint=f"for example {t.raw}_ or I_{t.raw[:3]}" if t.value == "integral" else
                          f"for example {t.raw}_")

    def _assign_to_non_name(self, start, lhs):
        """`h² = GM a (1 − e²)`: only a name can be stored to; point to `solve` (FRICTION #28)."""
        parts = []
        for j in range(self.toks.index(start), self.i):
            tk = self.toks[j]
            if parts and tk.ws_before:
                parts.append(" ")
            parts.append(tk.raw)
        text = "".join(parts)
        run = self.toks[self.toks.index(start):self.i]
        if len(run) > 1 and all(tk.kind == "NAME" for tk in run) and not any(tk.ws_before for tk in run[1:]) and \
                any(tk.value == "π" for tk in run):
            # `Ωπ = 2`: π always ends a name, so this is Ω × π (gauntlet #76)
            prod = " × ".join(tk.raw for tk in run)
            return self.error(f"can't store a value in {text}: it is read as {prod} (π is always the number π, "
                              f"even written next to a letter)",
                              hint=f"name it with an underscore instead, like  {'_'.join(tk.raw for tk in run)} = …")
        var = None
        if isinstance(lhs, A.BinOp) and lhs.op == "^" and isinstance(lhs.left, A.Name):
            var = lhs.left.name
        else:
            names = [n.name for n in A.walk(lhs) if isinstance(n, A.Name)]
            unknown = [n for n in names if n not in self.known]
            var = (unknown or names or ["x"])[0]
        if isinstance(lhs, A.Field) and isinstance(lhs.target, A.Name):
            owner = lhs.target.name
            if owner in self.module_names:          # nuclear.a_V = 16 MeV (red team round 2 #13)
                return self.error(f"can't change {text}: a module's names can't be changed from outside it",
                                  hint=f"make your own copy and use it instead:  {lhs.name} = …  (the module's own "
                                       f"functions keep using {text})")
            return self.error(f"can't store a value in {text}: the left side of = must be a variable name",
                              hint=f"{text} is a part of {owner}, which can be read but not changed; store it in a "
                                   f"name of its own:  {lhs.name} = …")
        if any(isinstance(n, (A.Prime, A.Deriv)) for n in A.walk(lhs)):
            hint = f"a differential equation is solved with solve:  solve {text} = … with (starting values) " \
                   f"for t from 0 s to 10 s"
        else:
            hint = f"you can only assign to a name; to solve {text} = … for {var}, write  " \
                   f"solve {text} = … for {var} from {var}_min to {var}_max"
            if isinstance(lhs, A.BinOp) and lhs.op == "^" and isinstance(lhs.left, A.Name):
                hint += f", or store {var} = √(…) and write {text} where you need it"
            else:
                hint += "; to compare two values use =="
        return self.error(f"can't store a value in {text}: the left side of = must be a variable name", hint=hint)

    def _match(self, j):
        """Index of the bracket matching the one at j."""
        depth = 0
        opening = self.toks[j].value
        closing = {"(": ")", "[": "]", "{": "}"}[opening]
        while j < len(self.toks):
            t = self.toks[j]
            if t.kind == "OP" and t.value == opening:
                depth += 1
            elif t.kind == "OP" and t.value == closing:
                depth -= 1
                if depth == 0:
                    return j
            elif t.kind in ("EOF",):
                return None
            j += 1
        return None

    def _is_funcdef(self):
        j = self._match(self.i + 1)
        if j is None:
            return False
        after = self.toks[j + 1]
        return after.kind == "OP" and after.value == "="

    def assign_stmt(self):
        name = self.next()
        self.next()  # =
        val = self.expr_where()
        self.known.add(name.value)
        return self.span(A.Assign(name.value, val), name)

    def funcdef(self):
        name = self.next()
        self.expect_op("(")
        params = []
        saved = set(self.known)
        while not self.at_op(")"):
            pt = self.expect_name("a parameter name")
            unit = None
            if self.at_op("["):
                unit = self.bracket_unit()
            params.append(self.span(A.Param(pt.value, unit), pt))
            self.known.add(pt.value)
            if self.at_op(","):
                self.next()
            elif not self.at_op(")"):
                raise self.error("expected ',' or ')' in the list of parameters" + self._found())
        self.expect_op(")")
        self.expect_op("=")
        self.known.add(name.value)
        if self.tok.kind == "NEWLINE":
            body = self.block()
            f = A.FuncDef(name.value, params, body)
        else:
            e = self.expr_where()
            f = A.FuncDef(name.value, params, e)
            self.end_statement()
        self.known = saved | {name.value}
        f.line, f.col, f.length = name.line, name.col, len(name.raw)
        return f

    def print_stmt(self):
        t = self.next()
        items = []
        if self.tok.kind not in ("NEWLINE", "EOF", "DEDENT"):
            items.append(self.print_item())
            while self.at_op(","):
                self.next()
                items.append(self.print_item())
        if self.at_kw("where"):
            binds = self.where_bindings()
            self._check_where_collisions(items, {b for b, _ in binds})
            items = [it if isinstance(it, A.Str) else A.Where(it, binds).at(it) for it in items]
        return self.span(A.Print(items), t)

    def print_item(self):
        t = self.tok
        e = self.expr_full()
        if self.at_kw("to") and self.peek().kind == "NUM" and self.peek(2).kind == "NAME" and \
                self.peek(2).value in ("digits", "digit"):
            self.next()
            n = self.next()
            self.next()
            e = self.span(A.Digits(e, int(n.value)), t)
        return e

    def plot_stmt(self):
        t = self.next()
        series = []
        saved_nj = self.no_juxt_names
        # plot y vs x title "…": `x title` isn't a product (#65); plot u vs x animate over t (D83)
        self.no_juxt_names = saved_nj | {w for w in ("title", "animate") if w not in self.known}
        try:
            bare_opts = self._plot_series(series)
        finally:
            self.no_juxt_names = saved_nj
        out = None
        opts = {}

        def animate_next():
            return self.tok.kind == "NAME" and self.tok.value == "animate" and "animate" not in self.known

        def options(bare_opts):
            nonlocal out
            while bare_opts or self.at_kw("to") or self.at_kw("with") or animate_next():
                if not bare_opts and self.at_kw("to"):
                    self.next()
                    if self.tok.kind != "STR":
                        raise self.error("expected a file name in quotes after 'to', like \"orbit.png\"")
                    out = self.next().value
                    # `to "a.png" title "…"` / `to "a.png", log y`: options may follow the file name without `with`,
                    # as they may follow the last series (§11, red team 5 #17)
                    if self.at_op(","):
                        self.next()
                        bare_opts = True
                    elif self._plot_option_ahead():
                        bare_opts = True
                    continue
                if not bare_opts and not animate_next():
                    self.next()      # with log y / with log / with title "..."   (`animate over t` needs no `with`)
                bare_opts = False
                while True:
                    w = self.tok
                    if w.kind == "NAME" and w.value == "log":
                        self.next()
                        axes = "xy"
                        if self.tok.kind == "NAME" and self.tok.value in ("x", "y"):
                            axes = self.next().value
                        for a in axes:
                            opts["log" + a] = True
                    elif w.kind == "NAME" and w.value in ("points", "dots", "markers"):
                        self.next()
                        opts["points"] = True        # scatter: markers, no lines (research/semf_ame2020)
                    elif w.kind == "NAME" and w.value == "animate":
                        self.next()             # with animate over t [frames 60]  (D83)
                        if not (self.tok.kind == "NAME" and self.tok.value == "over"):
                            raise self.error("write  with animate over t  (the variable that changes from frame to frame)")
                        self.next()
                        opts["animate"] = self.expect_name("the variable to animate over, like t").value
                        if self.tok.kind == "NAME" and self.tok.value == "frames" and self.peek().kind == "NUM":
                            self.next()
                            opts["frames"] = self.next().value
                    elif w.kind == "NAME" and w.value == "title":
                        self.next()
                        if self.tok.kind != "STR":
                            raise self.error("expected the title in quotes, like title \"Decay of Ba-137m\"")
                        opts["title"] = self.next().value
                    elif w.kind == "NAME" and w.value in ("xlabel", "ylabel"):
                        self.next()             # with xlabel "T [MeV]", ylabel "mass fraction"  (D161)
                        if self.tok.kind != "STR":
                            raise self.error(f"expected the axis label in quotes, like  {w.value} \"mass fraction\"")
                        opts[w.value] = self.next().value
                    elif w.kind == "NAME" and w.value in ("x", "y") and self.peek().kind == "KW" and \
                            self.peek().value == "from":
                        self.next()             # with y from 1e-12 to 1  (D161)
                        self.next()
                        lo = self.expr()
                        self.expect_kw("to", f"(write:  {w.value} from 1e-12 to 1)")
                        opts[w.value + "range"] = (lo, self.expr())
                    elif w.kind == "NAME" and w.value == "reversed":
                        self.next()             # with reversed x  (D161)
                        if not (self.tok.kind == "NAME" and self.tok.value in ("x", "y")):
                            raise self.error("write  with reversed x  (or  reversed y)")
                        opts["rev" + self.next().value] = True
                    else:
                        raise self.error("plot options are:  with log y,  with log x,  with log,  with points,  with title \"...\",  "
                                         "with xlabel \"...\",  with ylabel \"...\",  with y from a to b,  with x from a to b,  "
                                         "with reversed x,  with animate over t")
                    if self.at_op(","):
                        self.next()
                        continue
                    break

        options(bare_opts)
        if self.tok.kind == "NEWLINE" and self.peek().kind == "INDENT":
            # plot … continued on indented lines: more series, `with …` options, `to "f.png"` (D216)
            self.next()
            self.next()
            while self.tok.kind not in ("DEDENT", "EOF"):
                if self.at_op(",") or self.at_kw("and"):
                    self.next()
                if self.at_kw("with") or self.at_kw("to") or animate_next():
                    options(False)
                elif self._plot_option_ahead():
                    options(True)
                else:
                    self.no_juxt_names = saved_nj | {w for w in ("title", "animate") if w not in self.known}
                    try:
                        options(self._plot_series(series))
                    finally:
                        self.no_juxt_names = saved_nj
                if self.tok.kind not in ("DEDENT", "EOF"):
                    self.end_statement()
                self.skip_newlines()
            if self.tok.kind == "DEDENT":
                self.next()
            self._stmt_done = True
        p = self.span(A.Plot(series, out), t)
        p.options = opts
        return p

    def _plot_series(self, series):
        """The `y vs x [from a to b]` series of a plot, separated by ',' or 'and'. True when plot options
        follow without `with`: `plot y vs x, title "…"` or `plot y vs x title "…"` (#65)."""
        while True:
            st = self.tok
            y = self.expr_full()
            if not self.at_kw("vs") and (self.tok.kind == "STR" or (self.tok.kind == "NAME" and
                                                                    self.tok.value == "title")):
                raise self.error("expected 'vs' after the quantity to plot; a title goes after the series, "
                                 "like  plot y vs x, title \"Orbit\"  (or  with title \"Orbit\")")
            self.expect_kw("vs", "(write: plot y vs x)")
            x = self.expr_full()
            lo = hi = None
            if self.at_kw("from"):
                self.next()
                lo = self.expr()
                self.expect_kw("to")
                hi = self.expr()
            series.append(self.span(A.PlotSeries(y, x, lo, hi), st))
            if self.at_op(",") or self.at_kw("and"):
                self.next()
                if self._plot_option_ahead():
                    return True
                continue
            return self.tok.kind == "NAME" and self.tok.value == "title" and self._plot_option_ahead()

    def _plot_option_ahead(self):
        """After a ',' in a plot: does a plot option (title "…", log [x|y], points) follow rather than
        another series? Only when the word isn't one of the program's names and its shape fits."""
        w, nx = self.tok, self.peek()
        if w.kind == "NAME" and w.value in ("x", "y") and nx.kind == "KW" and nx.value == "from":
            return True          # `y from 1e-12 to 1`: no series starts like that, even with a variable y (D161)
        if w.kind != "NAME" or w.value in self.known:
            return False
        ends = nx.kind in ("NEWLINE", "EOF") or (nx.kind == "OP" and nx.value == ",") or \
            (nx.kind == "KW" and nx.value in ("to", "with"))
        if w.value in ("title", "xlabel", "ylabel"):
            return nx.kind == "STR"
        if w.value == "reversed":
            return nx.kind == "NAME" and nx.value in ("x", "y")
        if w.value == "log":
            return ends or (nx.kind == "NAME" and nx.value in ("x", "y"))
        return w.value in ("points", "dots", "markers") and ends

    def equation(self):
        st = self.tok
        lhs = self.expr()
        if not self.at_op("="):
            raise self.error("expected '=' in this equation" + self._found())
        self.next()
        rhs = self.expr()
        return self.span(A.Equation(lhs, rhs), st)

    def _unit_tok(self, tk):
        """Is this token a unit name?  In a solve, the unknown written with a prime (`u''`) never is: a prime
        on a unit means nothing, so `eV nm² * u''` and `0.5 u''` hold the unknown u, not the atomic mass unit
        (D211)."""
        return is_unit_name(tk.raw) and not getattr(tk, "unknown_prime", False)

    def _solve_unknowns(self):
        """Before parsing a solve: the names written with a prime in it (`u''`, `x'`), its unknowns (D211).
        Scans to the end of the statement, including an indented block of equations and clauses."""
        names, indep = set(), set()     # indep: the independent variables (`for s from …`), red team 6 #7
        depth = 0
        j = self.i
        while j < len(self.toks):
            tk = self.toks[j]
            if tk.kind == "EOF":
                break
            if tk.kind == "INDENT":
                depth += 1
            elif tk.kind == "DEDENT":
                depth -= 1
                if depth <= 0:
                    break
            elif tk.kind == "NEWLINE" and depth == 0 and self.toks[j + 1].kind != "INDENT":
                break
            elif tk.kind == "NAME" and self.toks[j + 1].kind == "PRIME" and not self.toks[j + 1].ws_before:
                names.add(tk.value)
                if is_unit_name(tk.raw):
                    tk.unknown_prime = True
            elif tk.kind == "NAME" and j > 0 and self.toks[j + 1].kind == "KW" and self.toks[j + 1].value == "from" \
                    and ((self.toks[j - 1].kind == "KW" and self.toks[j - 1].value == "for") or
                         (self.toks[j - 1].kind == "OP" and self.toks[j - 1].value == ",")):
                indep.add(tk.value)
            j += 1
        self.solve_independents = indep
        return names

    def _solve_equation(self, unknowns):
        """An equation of a solve: its unknowns count as your variables while it is parsed, so the D7 rule
        applies to them (`nm² * u''` isn't continued by u, `2 u * V` is ambiguous) (D211)."""
        saved_known, saved_unk = self.known, self.solve_unknowns
        # the independent variable (`for s from …`) is your variable too: `y' = 3 s^2` is not 3 square seconds
        # without a word (red team round 6 #7, D232)
        self.known = saved_known | unknowns | getattr(self, "solve_independents", set())
        self.solve_unknowns = unknowns
        try:
            return self.equation()
        finally:
            self.known, self.solve_unknowns = saved_known, saved_unk

    def solve_stmt(self):
        t = self.next()
        eqs = []
        initial = []
        unknowns = self._solve_unknowns()
        var = lo = hi = step = None
        method = None
        tol_node = [None]
        abs_nodes = [None]        # `absolute a[, b …]`: absolute tolerances, one per unit (D160)
        until = [None]
        m3 = {}                   # `lowest N` / `grid N` of an eigenvalue problem or a PDE (D82, D83)
        if self.tok.kind != "NEWLINE":
            eqs.append(self._solve_equation(unknowns))
            while self.at_op(",") or self.at_kw("and"):
                self.next()
                eqs.append(self._solve_equation(unknowns))

        def clause():
            nonlocal var, lo, hi, step, method
            if self.at_kw("with"):
                self.next()
                initial.append(self.equation())
                while self.at_op(",") or self.at_kw("and"):
                    self.next()
                    initial.append(self.equation())
                return True
            if self.at_kw("for"):
                self.next()
                vt = self.expect_name("the time variable, e.g. 'for t from 0 s to 5 s'")
                var = vt.value
                self.expect_kw("from")
                saved = self.no_juxt_names
                self.no_juxt_names = saved | {"tolerance", "absolute", "using", "method", "until", "lowest", "grid"}
                lo = self.expr()
                self.expect_kw("to")
                hi = self.expr()
                if self.at_kw("step"):
                    self.next()
                    step = self.expr()
                # `tolerance`, `using`/`method` and `until` may follow in any order; the tolerance
                # expression is parsed with the option words still excluded from juxtaposition, so
                # `tolerance 1e-11 using radau` does not read `1e-11 using` as a product (D111).
                seen = set()
                while self.tok.kind == "NAME":
                    word = self.tok.value
                    if word == "tolerance":
                        key = "tolerance"
                    elif word == "absolute" and "absolute" not in self.known:
                        key = "absolute"
                    elif word in ("using", "method"):
                        key = "using"
                    elif word == "until" and "until" not in self.known:
                        key = "until"
                    else:
                        break
                    if key in seen:
                        raise self.error(f"'{word}' is given twice in this solve")
                    seen.add(key)
                    self.next()
                    if key == "tolerance":
                        tol_node[0] = self.expr()
                    elif key == "absolute":
                        # absolute 1e-16  or  absolute 1e-16, 1e-20 MeV  (one value per unit, D160)
                        abs_nodes[0] = [self.expr()]
                        while self.at_op(",") and self.peek().kind == "NUM":
                            self.next()
                            abs_nodes[0].append(self.expr())
                    elif key == "using":
                        method = self.expect_name("a method name (rk4, rk45, radau or bdf)").value
                    else:
                        until[0] = self.equation()   # for t from 0 s to 9 s until y = 0 m  (D39)
                self.no_juxt_names = saved
                if self.at_op(",") and self.peek().kind == "NAME" and self.peek(2).kind == "KW" and \
                        self.peek(2).value == "from":
                    self.next()             # a second range: a PDE in x and t (D83)
                    m3["var2"] = self.next().value
                    self.expect_kw("from")
                    self.no_juxt_names = saved | {"tolerance", "absolute", "using", "method", "until", "lowest", "grid"}
                    m3["lo2"] = self.expr()
                    self.expect_kw("to")
                    m3["hi2"] = self.expr()
                    if self.at_kw("step"):
                        self.next()
                        m3["step2"] = self.expr()
                    self.no_juxt_names = saved
                # options after a PDE's second range (the loop above handles them after the first)
                if self.tok.kind == "NAME" and self.tok.value == "tolerance":
                    self.next()
                    tol_node[0] = self.expr()
                if self.tok.kind == "NAME" and self.tok.value in ("using", "method"):
                    self.next()
                    method = self.expect_name("a method name (rk4, rk45, radau or bdf)").value
                if self.tok.kind == "NAME" and self.tok.value == "until" and "until" not in self.known:
                    self.next()              # for t from 0 s to 9 s until y = 0 m  (D39)
                    until[0] = self.equation()
                return True
            if self.tok.kind == "NAME" and self.tok.value in ("lowest", "grid") and self.tok.value not in self.known \
                    and self.peek().kind == "NUM":
                word = self.next().value      # lowest 3 [states]  /  grid 400  (D82, D83)
                saved = self.no_juxt_names
                self.no_juxt_names = saved | {"states", "levels", "state", "grid", "lowest", "using", "method"}
                m3[word] = self.expr()
                self.no_juxt_names = saved
                if self.tok.kind == "NAME" and self.tok.value in ("states", "levels", "state") and word == "lowest":
                    self.next()
                if self.tok.kind == "NAME" and self.tok.value in ("using", "method") and method is None:
                    self.next()
                    method = self.expect_name("a method name (matrix or shooting)").value
                return True
            # `tolerance 1e-12` / `absolute 1e-16` on a line of its own, like `using bdf` (red team round 3 #13)
            if self.tok.kind == "NAME" and self.tok.value in ("tolerance", "absolute") and \
                    self.tok.value not in self.known and not (self.peek().kind == "OP" and self.peek().value == "="):
                word = self.tok.value
                if (tol_node if word == "tolerance" else abs_nodes)[0] is not None:
                    raise self.error(f"'{word}' is given twice in this solve")
                self.next()
                saved = self.no_juxt_names
                self.no_juxt_names = saved | {"tolerance", "absolute", "using", "method", "until", "lowest", "grid"}
                if word == "tolerance":
                    tol_node[0] = self.expr()
                else:
                    abs_nodes[0] = [self.expr()]
                    while self.at_op(",") and self.peek().kind == "NUM":
                        self.next()
                        abs_nodes[0].append(self.expr())
                self.no_juxt_names = saved
                return True
            # `using shooting` / `using explicit` on a line of its own, like `grid 400` (red team round 2 #11)
            if self.tok.kind == "NAME" and self.tok.value in ("using", "method") and \
                    self.tok.value not in self.known and self.peek().kind == "NAME":
                if method is not None:
                    raise self.error(f"'{self.tok.value}' is given twice in this solve")
                self.next()
                method = self.expect_name("a method name (rk4, rk45, radau, bdf, matrix, shooting, crank_nicolson, "
                                          "implicit or explicit)").value
                return True
            return False

        while clause():
            pass
        if self.tok.kind == "NEWLINE" and self.peek().kind == "INDENT":
            self.next()
            self.next()
            while self.tok.kind not in ("DEDENT", "EOF"):
                if not clause():
                    eqs.append(self._solve_equation(unknowns))
                    while self.at_op(",") or self.at_kw("and"):
                        self.next()
                        eqs.append(self._solve_equation(unknowns))
                while clause():
                    pass
                self.end_statement()
                self.skip_newlines()
            if self.tok.kind == "DEDENT":
                self.next()
            # continuation clauses indented less than the equations (but still indented)
            if self.tok.kind == "INDENT" and self.peek().kind == "KW" and self.peek().value in ("with", "for"):
                self.next()
                while self.tok.kind not in ("DEDENT", "EOF"):
                    if not clause():
                        raise self.error("expected 'with ...' or 'for ...' here" + self._found())
                    while clause():
                        pass
                    self.end_statement()
                    self.skip_newlines()
                if self.tok.kind == "DEDENT":
                    self.next()
        else:
            self.end_statement()
        if not eqs:
            raise self.error("this solve has no equation", tok=t, hint="write e.g. solve x' = -x with x(0) = 1 for t from 0 to 5")
        if var is None:
            raise FermiumError("solve needs a range for the independent variable", t.line, t.col, 5,
                               hint="add e.g.  for t from 0 s to 10 s")
        self._warn_divide_by_unknown(eqs)
        s = A.Solve(eqs, initial, var, lo, hi, step, method, tol_node[0])
        s.absolute = abs_nodes[0]
        if until[0] is not None:
            s.until = until[0]
        s.lowest = m3.get("lowest")
        s.grid = m3.get("grid")
        s.var2, s.lo2, s.hi2, s.step2 = m3.get("var2"), m3.get("lo2"), m3.get("hi2"), m3.get("step2")
        s.line, s.col, s.length = t.line, t.col, 5
        for eq in eqs:
            for n in A.walk(eq.lhs):
                if isinstance(n, A.Name):
                    self.known.add(n.name)
        return s

    def _warn_divide_by_unknown(self, eqs):
        """`ψ'' = -2 m_e E / ħ² ψ` divides by ψ (D8); dividing by the unknown of an ODE is rare and
        usually a precedence slip, so warn (FRICTION #9)."""
        unknowns = set()
        for eq in eqs:
            for n in A.walk(eq.lhs):
                if isinstance(n, (A.Prime, A.Deriv)):
                    base = n.target if isinstance(n, A.Prime) else n.operand
                    if isinstance(base, A.Call):
                        base = base.func
                    if isinstance(base, A.Name):
                        unknowns.add(base.name)
        for eq in eqs:
            for n in A.walk(eq.rhs):
                info = getattr(n, "div_info", None)
                if info is None or info["warned"]:
                    continue
                for k, (f, _, ws) in enumerate(info["factors"]):
                    g = f.func if isinstance(f, A.Call) else f
                    if k > 0 and isinstance(g, A.Name) and g.name in unknowns:
                        self._warn_juxt_denominator(info, k, why=f", including the unknown {g.name}")
                        break

    def import_stmt(self):
        """import mechanics [as m] | import "path/file.fm" [as m] | from mechanics import a [as b], c  (D100)"""
        t = self.next()
        frm = t.value == "from"
        mt = self.next()
        module, is_path = (mt.value, True) if mt.kind == "STR" else (mt.raw, False)
        if not frm:
            alias = None
            if self.tok.kind == "NAME" and self.tok.value == "as":
                self.next()
                alias = self.expect_name("a name after 'as' (like  import astro as a)").value
            if self.at_op(","):
                raise self.error("import one module per line", hint="write each on its own line:  import mechanics")
            self.known.add(alias or mt.value)
            self.module_names.add(alias or mt.value)
            return self.span(A.Import(module, is_path, alias, None), t)
        if not (self.tok.kind == "NAME" and self.tok.value == "import"):
            raise self.error(f"expected 'import' after 'from {mt.raw}'" + self._found(),
                             hint=f"write  from {mt.raw} import name1, name2")
        self.next()
        names = []
        while True:
            nt = self.expect_name(f"a name to import from {mt.raw} (like  from nuclear import semf_binding)")
            alias = None
            if self.tok.kind == "NAME" and self.tok.value == "as":
                self.next()
                alias = self.expect_name("a name after 'as'").value
            names.append((nt.value, alias))
            self.known.add(alias or nt.value)
            if not self.at_op(","):
                break
            self.next()
        return self.span(A.Import(module, is_path, None, names), t)

    def use_python_stmt(self, end_line=True):
        """use python numpy [as np] [: signatures]   (D140).  Signatures, one per line in an indented block
        (or on the same line after ':', separated by ';'):  f(x [m], n: int) -> list [J]"""
        t = self.next()
        self.next()                                  # python
        parts = [self.expect_name("the name of a Python module (like  use python numpy as np)").raw]
        while self.at_op(".") and not self.tok.ws_before:
            self.next()
            parts.append(self.expect_name("the rest of the Python module's name (like scipy.special)").raw)
        module = ".".join(parts)
        alias = None
        if self.tok.kind == "NAME" and self.tok.value == "as":
            self.next()
            alias = self.expect_name("a name after 'as' (like  use python numpy as np)").value
        elif len(parts) > 1:
            raise self.error(f"give the Python module {module} a short name with as",
                             hint=f"write  use python {module} as {parts[-1][:2]}")
        self.known.add(alias or module)
        sigs = []
        if self.at_op(":"):
            self.next()
            if self.tok.kind != "NEWLINE":
                sigs.append(self.py_signature())
                while self.at_op(";"):
                    self.next()
                    sigs.append(self.py_signature())
            else:
                self.next()
                self.skip_newlines()
                if self.tok.kind != "INDENT":
                    raise self.error("expected the signatures of the Python functions, indented on the next lines",
                                     hint="like\n    use python mylib as ml:\n        energy(m [kg], v [m/s]) -> [J]")
                self.next()
                while self.tok.kind not in ("DEDENT", "EOF"):
                    sigs.append(self.py_signature())
                    if self.tok.kind == "NEWLINE":
                        self.next()
                    elif self.tok.kind not in ("DEDENT", "EOF"):
                        raise self.error("expected one signature per line" + self._found())
                    self.skip_newlines()
                if self.tok.kind == "DEDENT":
                    self.next()
                return self.span(A.UsePython(module, alias, sigs), t)
        if end_line:
            self.end_statement()
        return self.span(A.UsePython(module, alias, sigs), t)

    def py_signature(self):
        """f(x [m], xs [s], n: int) -> list [J]   (the result part is optional)."""
        nt = self.tok
        if nt.kind not in ("NAME", "KW"):
            raise self.error("expected a Python function's signature, like  energy(m [kg], v [m/s]) -> [J]" +
                             self._found())
        self.next()
        self.expect_op("(", f"after {nt.raw} (write the signature like  {nt.raw}(x [m]) -> [J])")
        params = []
        while not self.at_op(")"):
            pt = self.expect_name("a parameter name")
            unit, is_int = None, False
            if self.at_op("["):
                unit = self.bracket_unit()
            elif self.at_op(":"):
                self.next()
                kt = self.expect_name("int after ':' (a whole number passed to Python as an int)")
                if kt.value != "int":
                    raise self.error(f"a parameter can be marked  : int  (a whole number), not : {kt.raw}", kt,
                                     hint="give a unit in brackets instead, like  x [m]")
                is_int = True
            params.append((pt.raw, unit, is_int))
            if self.at_op(","):
                self.next()
            elif not self.at_op(")"):
                raise self.error("expected ',' or ')' in the list of parameters" + self._found())
        self.expect_op(")")
        shape, runit = None, None
        if self.at_op("-") and self.peek().kind == "OP" and self.peek().value == ">":
            self.next()
            self.next()
            if self.tok.kind == "NAME" and self.tok.value in ("list", "number"):
                shape = self.next().value
            if self.at_op("["):
                runit = self.bracket_unit()
            elif shape is None:
                raise self.error("expected the result's unit in brackets, or list / number, after ->" + self._found(),
                                 hint=f"like  {nt.raw}(x [m]) -> [J]   or  -> list [m]")
        return self.span(A.PySig(nt.raw, params, shape, runit), nt)

    def _is_analyze(self):
        """`analyze [title:] T depends on ...`: 'depends' follows on the same line (so `analyze` stays a name)."""
        j = self.i + 1
        while self.toks[j].kind not in ("NEWLINE", "EOF"):
            if self.toks[j].kind == "NAME" and self.toks[j].value == "depends":
                return True
            j += 1
        return False

    def analyze_stmt(self):
        """analyze [title:] T [unit] depends on a [unit], b, c  (D70)"""
        t = self.next()
        title = None
        raw = {}
        if self.tok.kind == "NAME" and self.peek().kind == "OP" and self.peek().value == ":":
            title = self.next().value
            self.next()

        def quantity(what):
            nt = self.tok
            if nt.kind != "NAME":
                raise self.error(f"expected {what}" + self._found(),
                                 hint="write  analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]")
            self.next()
            unit = self.bracket_unit() if self.at_op("[") else None
            raw[nt.value] = nt.raw
            return self.span(A.Param(nt.value, unit), nt)
        target = quantity("the quantity to analyze (like T)")
        if not (self.tok.kind == "NAME" and self.tok.value == "depends"):
            raise self.error("expected 'depends on' after the quantity to analyze" + self._found(),
                             hint="write  analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]")
        self.next()
        if not (self.tok.kind == "NAME" and self.tok.value == "on"):
            raise self.error("expected 'on' after 'depends'" + self._found())
        self.next()
        inputs = [quantity("a quantity after 'depends on'")]
        while self.at_op(","):
            self.next()
            inputs.append(quantity("a quantity after ','"))
        if title:
            self.known.add(title)
        return self.span(A.Analyze(title, target, inputs, raw), t)

    def fit_stmt(self):
        t = self.next()
        model = self.equation_until_to()
        self.expect_kw("to", "(write: fit y = model to data)")
        data = self.expr()
        guesses = []
        indented = False
        if self.tok.kind == "NEWLINE" and self.peek().kind == "INDENT" and self.peek(2).kind == "KW" and \
                self.peek(2).value == "with":
            self.next()
            self.next()
            indented = True
        if self.at_kw("with") or self.at_kw("starting"):
            self.next()
            while True:
                nt = self.expect_name("a parameter name")
                self.expect_op("=")
                guesses.append((nt.value, self.expr()))
                if self.at_op(","):
                    self.next()
                    continue
                break
        f = self.span(A.Fit(model, data, guesses), t)
        if indented:
            self.skip_newlines()
            if self.tok.kind != "DEDENT":
                raise self.error("expected the end of the indented 'with' line" + self._found())
            self.next()
            self._stmt_done = True     # the NEWLINE/DEDENT were consumed here
        return f

    def equation_until_to(self):
        return self.equation()

    def if_stmt(self):
        t = self.next()
        cond = self.expr()
        if self.at_kw("then"):
            self.next()
        then = self.block()
        other = None
        self.skip_newlines_if_else()
        if self.at_kw("else"):
            et = self.next()
            if self.at_kw("if"):
                other = [self.if_stmt()]
            else:
                other = self.block()
            del et
        elif self.at_kw("elif"):
            other = [self.if_stmt()]
        return self.span(A.If(cond, then, other), t)

    def skip_newlines_if_else(self):
        j = self.i
        while self.toks[j].kind == "NEWLINE":
            j += 1
        if self.toks[j].kind == "KW" and self.toks[j].value in ("else", "elif"):
            self.i = j

    def for_stmt(self):
        t = self.next()
        vt = self.expect_name("a loop variable name")
        self.known.add(vt.value)
        if self.at_kw("in"):
            self.next()
            it = self.expr()
            body = self.block()
            return self.span(A.ForIn(vt.value, it, body), t)
        self.expect_kw("from", "(write: for i from 1 to 10)")
        lo = self.expr()
        self.expect_kw("to")
        hi = self.expr()
        step = None
        if self.at_kw("step"):
            self.next()
            step = self.expr()
        body = self.block()
        n = A.For(vt.value, lo, hi, step, body)
        n.line, n.col, n.length = t.line, t.col, 3
        return n

    def while_stmt(self):
        t = self.next()
        cond = self.expr()
        body = self.block()
        n = A.While(cond, body)
        n.line, n.col, n.length = t.line, t.col, 5
        return n

    def return_stmt(self):
        t = self.next()
        val = None
        if self.tok.kind not in ("NEWLINE", "EOF", "DEDENT"):
            val = self.expr_where()
        return self.span(A.Return(val), t)

    def assert_stmt(self):
        t = self.next()
        cond = self.expr()
        msg = None
        if self.at_op(","):
            self.next()
            if self.tok.kind != "STR":
                raise self.error("expected a message in quotes after the comma")
            msg = self.next().value
        return self.span(A.Assert(cond, msg), t)

    # ------------------------------------------------------------ expressions
    def expr_where(self):
        t = self.tok
        e = self.expr_full()
        if self.at_kw("where"):
            binds = self.where_bindings()
            self._check_where_collisions([e], {b for b, _ in binds})
            e = self.span(A.Where(e, binds), t)
        return e

    def where_bindings(self):
        self.next()
        binds = []
        while True:
            nt = self.expect_name("a name after 'where'")
            self.expect_op("=")
            binds.append((nt.value, self.expr()))
            if self.at_op(","):
                self.next()
                continue
            break
        return binds

    def expr_full(self):
        t = self.tok
        e = self.expr()
        if self.at_kw("in"):
            self.next()
            u = self.unit_expr(explicit=True)
            e = self.span(A.Convert(e, u), t)
        return e

    def expr(self):
        if self.at_kw("if"):
            t = self.next()
            c = self.expr()
            self.expect_kw("then", "(write: if condition then a else b)")
            a = self.expr()
            if not self.at_kw("else") and self.tok.kind in ("NEWLINE", "EOF", "DEDENT"):
                raise self.error("expected 'else' (an if-expression needs an else part) but the line ended",
                                 hint="to continue on the next line, indent the line that starts with else, "
                                      "or put the whole right side in brackets ( … )")
            self.expect_kw("else", "(an if-expression needs an else part)")
            b = self.expr()
            return self.span(A.IfExpr(c, a, b), t)
        return self.or_expr()

    def or_expr(self):
        t = self.tok
        e = self.and_expr()
        while self.at_kw("or"):
            self.next()
            e = self.span(A.Logic("or", e, self.and_expr()), t)
        return e

    def and_expr(self):
        t = self.tok
        e = self.not_expr()
        while self.at_kw("and") and not self._and_is_separator():
            self.next()
            e = self.span(A.Logic("and", e, self.not_expr()), t)
        return e

    def _and_is_separator(self):
        # In `solve a = b and c = d` / `plot y vs x and z vs x`, 'and' separates items.
        # We treat 'and' as logical only if no '=' / 'vs' follows at the same nesting level.
        depth = 0
        j = self.i + 1
        while j < len(self.toks):
            t = self.toks[j]
            if t.kind in ("NEWLINE", "EOF"):
                return False
            if t.kind == "OP" and t.value in "([{":
                depth += 1
            elif t.kind == "OP" and t.value in ")]}":
                if depth == 0:
                    return False
                depth -= 1
            elif depth == 0 and ((t.kind == "OP" and t.value == "=") or (t.kind == "KW" and t.value == "vs")):
                return True
            elif depth == 0 and t.kind == "OP" and t.value == ",":
                return False
            j += 1
        return False

    def not_expr(self):
        if self.at_kw("not"):
            t = self.next()
            return self.span(A.Not(self.not_expr()), t)
        return self.compare()

    def compare(self):
        t = self.tok
        e = self.sum()
        if not (self.tok.kind == "OP" and self.tok.value in CMP_OPS):
            return e
        # a < x < b means a < x and x < b, with x evaluated once (D50)
        operands, ops = [e], []
        within = "within" not in self.known
        while self.tok.kind == "OP" and self.tok.value in CMP_OPS:
            op_tok = self.next()
            ops.append(op_tok.value)
            saved = self.no_juxt_names
            if op_tok.value == "~=" and within:
                self.no_juxt_names = saved | {"within"}    # `x ≈ 0 m/s within 1e-9 m/s` (D260)
            try:
                operands.append(self.sum())
            finally:
                self.no_juxt_names = saved
        if len(ops) == 1:
            if ops[0] == "~=":
                return self._approx(operands[0], operands[1], t, op_tok, within)
            return self.span(A.Compare(ops[0], operands[0], operands[1]), t)
        if any(op in ("==", "!=", "~=") for op in ops) and not all(op == "==" for op in ops):
            raise self.error("a chain of comparisons can only use <, <=, > and >= (like a < x < b), or only ==",
                             hint="write the comparisons separately, joined with and")
        binds = []
        for k in range(1, len(operands) - 1):
            x = operands[k]
            if not isinstance(x, (A.Name, A.Num)):
                self.chain_count = getattr(self, "chain_count", 0) + 1
                nm = f"·chain{self.chain_count}"
                binds.append((nm, x))
                operands[k] = A.Name(nm).at(x)
        e = None
        for k, op in enumerate(ops):
            lo, hi = operands[k], operands[k + 1]
            c = A.Compare(op, lo, hi).at(lo)
            if hi.line == lo.line and getattr(hi, "length", None):
                c.length = max(1, hi.col + hi.length - lo.col)
            e = c if e is None else A.Logic("and", e, c).at(e)
        e = self.span(e, t)
        if binds:
            e = self.span(A.Where(e, binds), t)
        return e

    @staticmethod
    def _zero_literal(n):
        """A literal zero: `0`, `0.0 m/s`, `0 [m]`, `-0 J`, `<0, 0> m/s` (not `0 °C`, which is 273.15 K).
        Returns the unit text to suggest for the tolerance ('' for a pure number), or None (D260)."""
        from .units import _AFFINE
        while isinstance(n, A.Neg):
            n = n.operand
        unit = ""
        if isinstance(n, A.Quantity):
            if any(f.name in _AFFINE for f in n.unit.factors):
                return None
            txt = n.unit.text.strip()
            unit = " " + (f"[{txt}]" if n.bracket else txt)
            n = n.value
        if isinstance(n, A.VecLit):
            zeros = [Parser._zero_literal(x) for x in n.items]
            if not zeros or any(z is None for z in zeros):
                return None
            inner = {z for z in zeros if z}
            return unit or (next(iter(inner)) if len(inner) == 1 else "")
        if isinstance(n, A.Num) and n.value == 0:
            return unit
        return None

    def _approx(self, left, right, t, op_tok, within):
        """`a ≈ b [within tol]` (D260): |a − b| ≤ max(atol, rtol·max(|a|, |b|)).  Without `within` the
        tolerance is 10⁻⁶ relative, so comparing with a literal zero is an error that shows the fix."""
        tol = None
        if within and self.tok.kind == "NAME" and self.tok.value == "within":
            wt = self.next()
            if self.tok.kind in ("NEWLINE", "EOF", "DEDENT") or self.at_op(")") or self.at_op(","):
                raise self.error("'within' needs a tolerance after it, like  x ≈ 0 m/s within 1e-9 m/s", wt)
            tol = self.sum()
            v = tol.value if isinstance(tol, A.Quantity) else tol
            if isinstance(v, A.Neg) or (isinstance(v, A.Num) and v.value < 0):
                raise self.error("a tolerance can't be negative", wt,
                                 hint=f"write the size of the allowed difference, like within {self._src_of(tol).lstrip('-− ')}")
        zero_unit = self._zero_literal(right)
        other = left
        if zero_unit is None:
            zero_unit, other = self._zero_literal(left), right
        relative = isinstance(tol, A.Quantity) and tol.unit.text.strip() in ("%", "percent")
        if zero_unit is not None and self._zero_literal(other) is None and (tol is None or relative):
            op = op_tok.raw if op_tok.raw in ("≈", "~=") else "≈"
            written = f"{self._src_of(left)} {op} {self._src_of(right)}"
            fix = f"{written} within 1e-9{zero_unit}"
            if tol is None:
                msg = (f"'{written}' is true only when {self._src_of(other)} is exactly 0: ≈ allows a difference "
                       f"of 10⁻⁶ × the larger size, which is 0 here")
            else:
                msg = (f"'{written} within {self._src_of(tol)}' is true only when {self._src_of(other)} is "
                       f"exactly 0: a percentage of 0 is 0")
            raise FermiumError(msg, op_tok.line, op_tok.col, max(1, len(op_tok.raw)),
                               f"give an absolute tolerance: write {fix} (the difference you can accept)")
        c = self.span(A.Compare("~=", left, right, tol), t)
        return c

    def sum(self):
        t = self.tok
        e = self.pm_term()
        while self.tok.kind == "OP" and self.tok.value in ("+", "-"):
            op = self.next()
            r = self.pm_term()
            e = self.span(A.BinOp(op.value, e, r), t)
        return e

    def pm_term(self):
        """a ± b binds tighter than + and - and looser than * and / (D120): 2 x ± 0.1 is (2 x) ± 0.1."""
        t = self.tok
        e = self.product()
        while self.tok.kind == "OP" and self.tok.value == "+-":
            op = self.next()
            if self.tok.kind in ("NEWLINE", "EOF", "DEDENT") or self.at_op(")") or self.at_op(","):
                raise self.error("± needs the uncertainty after it, like  L = 1.20 ± 0.01 m", op)
            r = self.product()
            if isinstance(e, A.Uncertain) and not e.paren:
                raise self.error("a value can only have one ±; to add a second (independent) uncertainty, write  "
                                 "(a ± b) ± c", op)
            # `5.0 ± 0.2 m`: the unit written after the uncertainty belongs to both numbers
            # (also through a minus sign on either number: `-5.0 ± 0.2 m`, and `5.0 ± -0.2 m` reaches the
            # "can't be negative" check; red team round 3 #8)
            ev = e.operand if isinstance(e, A.Neg) and not e.paren else e
            rq = r.operand if isinstance(r, A.Neg) and not r.paren else r
            if isinstance(ev, A.Num) and not ev.paren and isinstance(rq, A.Quantity) and not rq.paren and \
                    isinstance(rq.value, A.Num) and not rq.bracket and rq.unit.text.strip() != "%":
                q = A.Quantity(ev, rq.unit).at(ev)
                e = A.Neg(q).at(e) if ev is not e else q
            e = self.span(A.Uncertain(e, r), t)
        return e

    def product(self):
        t = self.tok
        e = self.unary()
        while self.tok.kind == "OP" and self.tok.value in ("*", "/", "×"):
            if self.tok.value == "/" and self._ends_upper_limit():
                break
            op = self.next()
            den_start = self.i
            tight = not op.ws_before and not self.tok.ws_before
            r = self.unary()
            info = None
            if op.value == "/":
                last = e
                while isinstance(last, (A.BinOp, A.Neg)) and not last.paren and \
                        (isinstance(last, A.Neg) or last.implicit):
                    last = last.operand if isinstance(last, A.Neg) else last.right
                if isinstance(last, A.Integral) and getattr(last, "div_info", None) is not None and \
                        last.div_info["tok"] is op and "divisor" not in last.div_info:
                    last.div_info["divisor"] = r               # `∫ … to E / (2 P0)`: for the checker (D205)
                    r.limit_div_of = last
                    last.div_info["div_text"] = self._text(den_start, self.i)
                coef = self._fraction_coefficient(e, r, op)
                if coef is not None:            # `73/24 e²` is (73/24)·e², `1/2 kg` is 0.5 kg (A2, D236)
                    e = coef
                    continue
                warned = self._check_ambiguous_division(e, r, op)
                info = self._juxt_denominator(r, op, den_start, tight, warned)
            e = self.span(A.BinOp(op.value, e, r), t)
            if info is not None:
                e.div_info = info
        return e

    def _text(self, i, j):
        """Source text of tokens i..j-1."""
        parts = []
        for k in range(i, j):
            tk = self.toks[k]
            if parts and tk.ws_before:
                parts.append(" ")
            parts.append(tk.raw)
        return "".join(parts)

    def _juxt_denominator(self, den, op, den_start, tight, warned):
        """`c²/g (√(1 + x) − 1)` is c²/(g (…)): implicit multiplication binds tighter than '/' (D8).
        Warn when the way it's written suggests (a/b) c (FRICTION #9, D34):
          - a tight '/' (no spaces around it) followed by factors separated by spaces:
            `n R/(γ−1) (T3−T2)`, `μ₀ I/(4π) dl`;
          - a bracketed factor next to another factor with a space between: `/ g (…)`, `/ (4π) dl`.
        `h c / λ k_B T`, `a/(b c)`, `1/2π` and `G M m / r²` are not warned about.
        Returns the denominator's factors, for the solve-unknown check in solve_stmt."""
        if not (isinstance(den, A.BinOp) and den.implicit and not den.paren):
            return None
        factors = []      # (node, first token index, space before it)
        n = den
        while isinstance(n, A.BinOp) and n.implicit and not n.paren:
            factors.append((n.right, n.juxt_i, n.juxt_ws))
            n = n.left
        factors.append((n, den_start, False))
        factors.reverse()
        end = self.i
        if self.limit_start is None and self.in_integrand:
            # `∫ 1/u du`: the trailing differential isn't part of the denominator
            while len(factors) > 1 and isinstance(factors[-1][0], A.Name) and factors[-1][0].name.startswith("d") \
                    and len(factors[-1][0].name) > 1:
                end = factors.pop()[1]
            if len(factors) == 1:
                return None
        info = {"op": op, "start": den_start, "end": end, "factors": factors, "warned": warned}
        if warned:
            return info
        spaced = [k for k in range(1, len(factors)) if factors[k][2]]
        bracketed = [k for k in spaced if factors[k][0].paren or factors[k - 1][0].paren]
        if factors[0][0].paren and 1 in spaced:
            # `2/(3H₀√Ω) asinh(…)`: the brackets say the denominator ends there, the rule says it doesn't.
            # Too likely to be a slip to guess either way (gauntlet #58): make the writer choose.
            bounds = [f[1] for f in factors] + [info["end"]]
            first = self._text(bounds[0], bounds[1])
            rest = self._text(bounds[1], bounds[-1]).strip()
            raise self.error(f"'/{first} {rest}' is ambiguous: implicit multiplication binds tighter than '/', so "
                             f"this would divide by all of '{first} {rest}'", tok=self.toks[bounds[0]],
                             hint=f"write  /{first} * {rest}  to multiply by {rest}, or  /({first} {rest})  to "
                                  f"divide by both")
        if (tight and spaced) or bracketed:
            k = spaced[0] if tight and spaced else bracketed[0]
            self._warn_juxt_denominator(info, k)
        return info

    def _warn_juxt_denominator(self, info, k, why=""):
        factors, op = info["factors"], info["op"]
        bounds = [f[1] for f in factors] + [info["end"]]
        texts = []
        for j, (f, _, ws) in enumerate(factors):
            tx = self._text(bounds[j], bounds[j + 1])
            if f.paren and len(tx) > 12:
                tx = "(…)"
            texts.append((" " if ws and j else "") + tx)
        head, rest = "".join(texts[:k]), "".join(texts[k:]).lstrip()
        den = "".join(texts)
        info["warned"] = True
        self.diags.warn(f"this divides by all of '{den}'{why}: implicit multiplication binds tighter than '/'",
                        tok=op, hint=f"write …/{head} * {rest} if only {head} is below the line, "
                                     f"or …/({den}) if all of it is")

    @classmethod
    def _pure(cls, n):
        """A pure number as written: digits, π, √ and powers of them, and products of those (A2, D236)."""
        if isinstance(n, A.Num):
            return True
        if isinstance(n, A.Name):
            return n.name == "π" and not getattr(n, "imag_literal", False)
        if isinstance(n, A.Neg):
            return cls._pure(n.operand)
        if isinstance(n, A.Sqrt):
            return cls._pure(n.operand)
        if isinstance(n, A.BinOp):
            if n.op == "^":
                return cls._pure(n.left) and cls._pure(n.right)
            if n.implicit or n.paren:
                return cls._pure(n.left) and cls._pure(n.right)
        return False

    def _fraction_coefficient(self, left, right, op):
        """A2: a fraction of pure numbers is one coefficient.  `73/24 e²` is (73/24)·e², `π²/12 t²` is (π²/12)·t²
        and `1/2 kg` is 0.5 kg, although implicit multiplication binds tighter than '/' (D8: `h / m_e v` is
        unchanged, its denominator starts with a name).  Returns the rewritten product, or None."""
        if not self._pure(left):
            return None
        path, n = [], right
        while isinstance(n, A.BinOp) and n.implicit and not n.paren:
            path.append(n)
            n = n.left
        quantity = None
        if isinstance(n, A.Quantity) and not n.bracket and not n.paren and isinstance(n.value, A.Num):
            quantity, leaf = n, n.value
        else:
            leaf = n
        if not isinstance(leaf, (A.Num, A.BinOp, A.Sqrt, A.Name)) or not self._pure(leaf) or \
                (isinstance(leaf, A.BinOp) and leaf.op != "^" and not leaf.paren) or \
                (isinstance(leaf, A.Name) and not leaf.paren):
            return None                              # `1/(2π) √(k/m)`: a bracketed pure number counts too
        if not path and quantity is None:
            return None                              # a plain a/b
        if isinstance(leaf, A.Num) and leaf.value == 1:
            return None                              # `0.04 / 1 s` is 0.04 per second: dividing by one is no coefficient
        if path and quantity is None and self._pure(path[-1].right) and not leaf.paren:
            # `4/3 π r³` (a sphere) and `1/2π √(k/m)` have the same shape: which one?
            a, b, c = (self._src_of(x) for x in (left, leaf, path[-1].right))
            raise self.error(f"'{a}/{b} {c}' is ambiguous: is it ({a}/{b})·{c} or {a}/({b} {c})?", tok=op,
                             hint=f"write  ({a}/{b}) {c}  or  {a}/({b} {c})")
        frac = A.BinOp("/", left, leaf)
        frac.line, frac.col = left.line, left.col
        frac.length = max(1, (leaf.col or 0) + (leaf.length or 1) - (left.col or 0)) if leaf.line == left.line else 1
        frac.paren = True
        frac.coefficient = True
        if quantity is not None:
            nq = A.Quantity(frac, quantity.unit)
            nq.line, nq.col, nq.length = frac.line, frac.col, quantity.length
            new_leaf = nq
        else:
            new_leaf = frac
        if not path:
            return new_leaf
        path[-1].left = new_leaf
        for b in path:                               # the product now starts where the numerator starts
            b.col = left.col if b.line == left.line else b.col
        return right

    def _src_of(self, n):
        """The source text of a node on one line (for messages)."""
        if isinstance(n, A.Num):
            return A.num_text(n)
        if isinstance(n, A.Name):
            return n.name
        k = next((j for j, tk in enumerate(self.toks) if tk.line == n.line and tk.col == n.col), None)
        if k is None:
            return "…"
        j, end = k, (n.col or 0) + (n.length or 1)
        while j + 1 < len(self.toks) and self.toks[j + 1].line == n.line and self.toks[j + 1].col < end:
            j += 1
        return self._text(k, j + 1)

    def _check_ambiguous_division(self, left, right, op):
        """Warn about `1/2 m v²` which Fermium reads as 1/(2 m v²)."""
        if isinstance(right, A.BinOp) and right.implicit and not right.paren:
            first = right
            while isinstance(first, A.BinOp) and first.implicit and not first.paren:
                first = first.left
            if isinstance(first, A.Quantity) and isinstance(first.value, A.Num) and not first.bracket:
                first = first.value
            if isinstance(first, A.Num) and isinstance(left, A.Num) and not left.paren:
                a, b = (A.num_text(v) for v in (left, first))
                self.diags.warn(
                    f"this is read as a/(b c), i.e. {a}/({b} ...): implicit multiplication binds tighter than '/'",
                    tok=op, hint=f"if you meant ({a}/{b}) times the rest, write ({a}/{b}) with parentheses "
                                 f"(or ½ for one half)")
                return True
        return False

    def unary(self):
        if self.at_op("-"):
            t = self.next()
            return self.span(A.Neg(self.unary()), t)
        if self.at_op("+"):
            self.next()
            return self.unary()
        return self.juxt()

    def _bracket_is_unit(self):
        """At '[': is this a unit in brackets ([m/s]) rather than a list ([1, 2])?"""
        j = self.i + 1
        t = self.toks[j]
        if t.kind == "NAME" and self._unit_tok(t):
            k = self._match(self.i)
            if k is None:
                return True
            return not any(self.toks[m].kind == "OP" and self.toks[m].value == "," for m in range(self.i, k))
        if t.kind == "NUM" and t.value == 1 and self.toks[j + 1].kind == "OP" and self.toks[j + 1].value in ("/", "]"):
            return True
        nx = self.toks[j + 1]
        if t.kind == "NAME" and t.raw == "h" and "h" not in self.known and (
                nx.kind == "SUP" or (nx.kind == "OP" and nx.value in ("]", "/", "^"))):
            return True        # `2 [h]` means hours, not a list holding Planck's constant: the unit check says so (D180)
        return False

    def _starts_term(self):
        t = self.tok
        if t.kind == "OP" and t.value == "[" and t.ws_before and not self._bracket_is_unit():
            return True
        if t.kind in ("NUM", "IMAG"):
            return True
        if t.kind == "NAME":
            return t.value not in self.no_juxt_names
        if t.kind == "KW":
            return t.value in ("sqrt", "cbrt", "integral", "partial", "nabla")
        if t.kind == "OP":
            if t.value == "(":
                return True
            if t.value == "|" and self.abs_depth == 0:
                return True
            if t.value == "<" and self._vector_after_space():
                return True
        return False

    def _vector_after_space(self):
        """At '<': is this `R <cos φ, sin φ, 0>`, a vector literal multiplied by what came before?

        Only when `<` has a space before it and none after, and a matching `>` (with no space before
        it) follows on the same line with a comma between them at the top level (FRICTION #7). `a < b` and `if x <y` stay
        comparisons.
        """
        t = self.tok
        if not t.ws_before or self.peek().ws_before:
            return False
        depth, comma, j = 0, False, self.i + 1
        while j < len(self.toks):
            tk = self.toks[j]
            if tk.kind in ("NEWLINE", "EOF", "INDENT", "DEDENT") or (tk.kind == "OP" and tk.value in ("=", "<")
                                                                     and depth == 0):
                return False
            if tk.kind == "OP" and tk.value in "([{":
                depth += 1
            elif tk.kind == "OP" and tk.value in ")]}":
                if depth == 0:
                    return False
                depth -= 1
            elif depth == 0 and tk.kind == "OP" and tk.value == ",":
                comma = True
            elif depth == 0 and tk.kind == "OP" and tk.value == ">":
                return comma and not tk.ws_before
            j += 1
        return False

    def named_anywhere(self):
        """Every name the program assigns (`m = …`, `m += …`), loops over (`for m in …`) or defines as a
        function or parameter (`f(m, g) = …`), wherever it is: a unit name after `)` is a unit only if none of
        these holds, so a variable set further down (or in a loop) is never silently read as a unit (D215)."""
        if self._named is None:
            out = set()
            toks = self.toks
            for j, tk in enumerate(toks):
                if tk.kind != "NAME" or j + 1 >= len(toks):
                    continue
                nx = toks[j + 1]
                prev = toks[j - 1] if j > 0 else None
                if nx.kind == "OP" and nx.value in ("=", "+=", "-=", "*=", "/=", "^=") or \
                        prev is not None and prev.kind in ("KW", "NAME") and prev.value in ("for", "as", "import"):
                    out.add(tk.value)
                elif nx.kind == "OP" and nx.value == "(" and not nx.ws_before and \
                        (prev is None or prev.kind in ("NEWLINE", "INDENT", "DEDENT") or
                         prev.kind == "KW" and prev.value in ("def", "function")):
                    k = self._match(j + 1)          # f(a, b) = …: the parameters are names too
                    if k is not None and k + 1 < len(toks) and toks[k + 1].kind == "OP" and toks[k + 1].value == "=":
                        out.add(tk.value)
                        out.update(p.value for p in toks[j + 2:k] if p.kind == "NAME")
            self._named = out
        return self._named

    def _unit_after_paren(self, e, t):
        """`(51 - 33 (N - Z)/A) MeV`: a unit right after a bracketed expression multiplies it by 1 unit, as
        after a number (D7, D215).  When the name is also one of your variables (anywhere in the program),
        `(a + b) m` keeps its old meaning, your variable: tested programs write `(…) m`, `(…) u`, `(…) g`,
        `(4/3) T` for their own m, u, g, T (D215)."""
        tk = self.tok
        if not (e.paren and not isinstance(e, A.Uncertain) and tk.kind == "NAME" and self._unit_tok(tk) and
                tk.value not in self.no_juxt_names and not _is_constant(tk.value) and not self._is_call_like()):
            return None
        if tk.value in self.known or tk.value in self.named_anywhere():
            return None
        u = self.unit_expr(explicit=False)
        q = self.span(A.Quantity(e, u), t)
        q.times_unit = True
        return q

    @staticmethod
    def _number_first(e):
        """Is e a product written after a number, like `100 h`?"""
        while isinstance(e, A.BinOp) and e.implicit and not e.paren:
            e = e.left
        return isinstance(e, A.Num) and e.digit and not e.paren

    def _free_unit_here(self):
        """A unit name here that no variable, function, parameter or constant of the program shares (D215)."""
        tk = self.tok
        return tk.kind == "NAME" and self._unit_tok(tk) and tk.value not in self.known and \
            tk.value not in self.named_anywhere() and tk.value not in self.no_juxt_names and \
            not _is_constant(tk.value) and not self._is_call_like()

    def juxt(self):
        t = self.tok
        e = self.power()
        if isinstance(e, A.Uncertain) and e.paren and self.tok.kind == "NAME" and self._unit_tok(self.tok) and \
                self.tok.value not in self.known and not self._is_call_like():
            u = self.unit_expr(explicit=False)          # (5.0 ± 0.2) m: the unit applies to both (D120)
            e = self.span(A.Quantity(e, u), t)
        q = self._unit_after_paren(e, t)
        if q is not None:
            e = q
        while True:
            if self.at_op("[") and self.tok.ws_before and self._bracket_is_unit():
                u = self.bracket_unit()
                e = self.span(A.Quantity(e, u, bracket=True), t)
                continue
            if not self._starts_term():
                break
            ws, ri = self.tok.ws_before, self.i
            r = self.power()
            if isinstance(r, A.Name) and not isinstance(e, A.Num):
                r.unit_left = e               # `A_d u`: if u isn't defined, the hint suggests A_d * 1 u (#55)
            e = self.span(A.BinOp("*", e, r, implicit=True), t)
            e.juxt_ws, e.juxt_i = ws, ri
            if r.paren:                       # `2 (a + b) MeV` (D215)
                q = self._unit_after_paren(r, t)
                if q is not None:
                    e = self.span(A.Quantity(e, q.unit), t)
                    e.times_unit = True
            elif isinstance(r, A.Name) and r.name in self.known and self._number_first(e) and \
                    self._free_unit_here():
                # `100 h km/s/Mpc` with your h: a compound unit (two names or more) after `number variable` is
                # a unit; a single one (`2 a b²`) stays a name, as before (D215)
                j0 = self.i
                roles = [getattr(tk, "role", None) for tk in self.toks[j0:j0 + 16]]
                u = self.unit_expr(explicit=False)
                if len(u.factors) >= 2:
                    e = self.span(A.Quantity(e, u), t)
                    e.times_unit = True
                else:
                    for tk, rl in zip(self.toks[j0:j0 + 16], roles):
                        tk.role = rl
                    self.i = j0
        return e

    def _num_text(self, q, default="2"):
        """The number of a quantity as it was written (`8.5e28`, not `8.5e+28`), for messages (round 4 #9)."""
        n = q.value if isinstance(q, A.Quantity) else q
        return A.num_text(n) if isinstance(n, A.Num) else default

    def _drop_warnings_between(self, line, col0, col1):
        """An error about a token replaces the warnings the parser gave on the way to it (a lone-unit warning,
        'reading L as your variable'), which would contradict it (red team round 4 #10, D207)."""
        ws = self.diags.warnings
        keep = [w for w in ws if not (w.line == line and w.col is not None and col0 <= w.col <= col1)]
        if len(keep) != len(ws):
            ws[:] = keep

    # ------------------------------------------------------------ the unit-name rule (A1, D235)
    # 1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
    # 2. If that unit is a single name that is also one of your variables, Fermium stops and asks which you mean.
    # 3. In a compound unit the first name is always a unit; any later name that is also your variable is an error.
    # Brackets are always units; a name that doesn't come right after a number is a variable; spaces never matter.
    # In fix mode (`fermium fmt --fix`, the language server's quick fix) each collision is rewritten as a bracketed
    # unit that keeps what v1 did there, and parsing goes on with v1's reading.

    def _whose(self, name, where=False):
        if where:
            return f"the {name} from 'where'"
        if name in self.deriv_vars:
            return f"the variable {name} you differentiate by"
        if name in self.solve_unknowns:
            return f"the unknown {name} of this solve"
        return f"your variable {name}"

    def _number_before(self, start):
        k = self.toks.index(start)
        prev = self.toks[k - 1] if k > 0 else None
        return prev.raw if prev is not None and prev.kind == "NUM" else "2"

    def _add_fix(self, fix):
        """Record a fix-mode edit; an edit inside one already recorded (or the same) adds nothing."""
        a, b, _ = fix
        self.fixes[:] = [f for f in self.fixes if not (a <= f[0] and f[1] <= b)]
        if not any(f[0] <= a and b <= f[1] for f in self.fixes):
            self.fixes.append(fix)

    def _unit_span_text(self, a, b):
        """The unit written by tokens a..b, with spaces only between names (`m/s/g`, `kg m`, `J/(kg K)`)."""
        out = []
        for j in range(a, b + 1):
            tk = self.toks[j]
            pv = self.toks[j - 1] if j > a else None
            tight = pv is None or tk.kind in ("SUP", "PRIME") or (tk.kind == "OP" and tk.value in "/*^)") or \
                (pv.kind == "OP" and pv.value in "/*^(-")
            out.append(("" if tight or not tk.ws_before else " ") + tk.raw)
        return "".join(out)

    def _bracket_fix(self, first, last):
        """The edit that writes the unit from token `first` to token `last` in brackets."""
        text = self._unit_span_text(self.toks.index(first), self.toks.index(last))
        return (first.start, last.end, f"[{text}]")

    def _unit_end_from(self, j):
        """The last token of a unit that starts at token j, reading every unit name (for a quick fix)."""
        last = j
        k = j + 1
        while k < len(self.toks):
            tk = self.toks[k]
            if tk.kind == "SUP":
                last = k
            elif tk.kind == "OP" and tk.value == "^" and k + 1 < len(self.toks):
                k += 1
                if self.toks[k].kind == "OP" and self.toks[k].value in ("-", "("):
                    m = self._match(k) if self.toks[k].value == "(" else k + 1
                    k = m if m is not None else k
                last = k
            elif tk.kind == "OP" and tk.value in ("/", "*") and k + 1 < len(self.toks) and \
                    self.toks[k + 1].kind == "NAME" and self._unit_tok(self.toks[k + 1]):
                k += 1
                last = k
            elif tk.kind == "NAME" and self._unit_tok(tk) and tk.ws_before and \
                    not (k + 1 < len(self.toks) and self.toks[k + 1].kind == "OP" and self.toks[k + 1].value == "("
                         and not self.toks[k + 1].ws_before):
                last = k
            else:
                break
            k += 1
        return self.toks[last]

    def _later_collision(self, start, nt, old_continues):
        """Sentence 3: a later name of a compound unit (`20 m/s/g`, `2 kg m`) that is also your variable.
        Returns True to go on reading the unit (fix mode, where v1 did), False to stop before the name (fix mode)."""
        from .units import lookup_unit, dim_name
        so_far_last = self.toks[self.i - 1]
        if self.fix_mode:
            if old_continues:
                self._fix_whole.append(start)
                return True
            self._add_fix(self._bracket_fix(start, so_far_last))
            return False
        num = self._number_before(start)
        a = self.toks.index(start)
        so_far = self._unit_span_text(a, self.i - 1)
        full_last = self._unit_end_from(self.toks.index(nt))
        full = self._unit_span_text(a, self.toks.index(full_last))
        u = lookup_unit(nt.raw)
        what = UNIT_WORDS.get(nt.value) or (dim_name(u.dim).split(" [")[0] if u is not None else "a unit")
        whose = self._whose(nt.value)
        op = self.tok.value if self.tok.kind == "OP" else " "
        use = f"({num} {so_far})/{nt.raw}  to divide by {whose}" if op == "/" else \
            f"({num} {so_far}) {nt.raw}  for {num} {so_far} × {whose}"
        e = self.error(f"'{num} {full}' is ambiguous: right after a number, {full} is one unit ({nt.raw} is {what} "
                       f"there), but {nt.raw} is also {whose}", tok=nt,
                       hint=f"write  {use}, or  {num} [{full}]  for the unit")
        e.fix = [self._bracket_fix(start, full_last) if old_continues else self._bracket_fix(start, so_far_last)]
        raise e

    def _single_collision(self, q, start_tok, where=False):
        """Sentence 2: `2 g`, `0.1 m`, `2 T` right after a number, when that single name is also your variable."""
        from .units import lookup_unit, dim_name
        f = q.unit.factors[0]
        first = next(tk for tk in self.toks if tk.line == f.line and tk.col == f.col)
        last = self._factor_last(self.toks.index(first))
        fix = self._bracket_fix(first, last)
        if self.fix_mode:
            self._add_fix(fix)
            return
        num = self._num_text(q)
        ut = q.unit.text.strip() or f.name
        u = lookup_unit(f.name)
        what = UNIT_WORDS.get(f.name) or (dim_name(u.dim).split(" [")[0] if u is not None else "a unit")
        whose = self._whose(f.name, where)
        hint = f"write  {num}*{ut}  for {num} × {whose}, or  {num} [{ut}]  for the unit"
        k = self.toks.index(first) - 1                   # `1/2 m v²` with a mass m: suggest ½ m v² (A2)
        if k >= 2 and self.toks[k - 1].kind == "OP" and self.toks[k - 1].value == "/" and \
                self.toks[k - 2].kind == "NUM" and self.toks[k].kind == "NUM":
            a, b = self.toks[k - 2].raw, self.toks[k].raw
            frac = {("1", "2"): "½", ("1", "3"): "⅓", ("1", "4"): "¼", ("3", "4"): "¾"}.get((a, b), f"({a}/{b})")
            hint = f"write  {frac} {ut}  for {a}/{b} × {whose}, or  {a}/{b} [{ut}]  for the unit"
        e = self.error(f"'{num} {ut}' is ambiguous: right after a number, {f.name} is a unit ({what}), but "
                       f"{f.name} is also {whose}", tok=first, hint=hint)
        e.fix = [fix]
        raise e

    def _list_collision(self, open_tok):
        """`[1, 2, 3] m` when m is also your variable: a unit follows a list literal as it follows a number (D192), so
        sentence 2 applies.  Fermium 1 multiplied by your variable (D222); fix mode keeps that with a `*`."""
        from .units import lookup_unit, dim_name
        f = self.tok
        prev = self.toks[self.i - 1]
        if self.fix_mode:
            self._add_fix((prev.end, f.start, "*"))
            return
        j = self.toks.index(open_tok)
        text = self._text(j, self.i)
        if len(text) > 24:
            text = "[…]"
        u = lookup_unit(f.value)
        what = UNIT_WORDS.get(f.value) or (dim_name(u.dim).split(" [")[0] if u is not None else "a unit")
        e = self.error(f"'{text} {f.raw}' is ambiguous: after a list, {f.raw} is a unit ({what}), but {f.raw} is also "
                       f"your variable {f.raw}", tok=f,
                       hint=f"write  {text}*{f.raw}  for your variable, or  {text} [{f.raw}]  for the unit")
        e.fix = [(prev.end, f.start, "*")]
        raise e

    def _check_where_collisions(self, exprs, names):
        """The rule for the names a `where` defines (`0.1 m where m = 2 kg`): they are your variables in the
        expression before `where`, which was read before they were known."""
        for ex in exprs:
            for n in A.walk(ex):
                if not (isinstance(n, A.Quantity) and not n.bracket and not n.paren and isinstance(n.value, A.Num)):
                    continue
                fs = n.unit.factors
                if len(fs) == 1 and fs[0].name in names:
                    self._single_collision(n, None, where=True)
                for f in fs[1:]:
                    if f.name in names:
                        first = next(tk for tk in self.toks if tk.line == fs[0].line and tk.col == fs[0].col)
                        nt = next(tk for tk in self.toks if tk.line == f.line and tk.col == f.col)
                        last = self._unit_end_from(self.toks.index(first))
                        fix = self._bracket_fix(first, last)
                        if self.fix_mode:
                            self._add_fix(fix)
                            break
                        num = self._num_text(n)
                        e = self.error(f"'{num} {n.unit.text}' is ambiguous: right after a number, {n.unit.text} is "
                                       f"one unit, but {f.name} is also the {f.name} from 'where'", tok=nt,
                                       hint=f"write  {num} [{n.unit.text}]  for the unit, or put the number and "
                                            f"its unit in brackets before using {f.name}")
                        e.fix = [fix]
                        raise e

    def _factor_last(self, j):
        """The last token of the unit factor that starts at token j: its exponent (`m²`, `m^-3`, `m^(1/2)`)."""
        k = j + 1
        if k < len(self.toks) and self.toks[k].kind == "SUP":
            return self.toks[k]
        if k < len(self.toks) and self.toks[k].kind == "OP" and self.toks[k].value == "^":
            k += 1
            if self.toks[k].kind == "OP" and self.toks[k].value == "-":
                k += 1
            if self.toks[k].kind == "OP" and self.toks[k].value == "(":
                k = self._match(k) or k
            return self.toks[k]
        return self.toks[j]

    def power(self):
        t = self.tok
        base = self.postfix()
        if self.tok.kind == "SUP":
            s = self.next()
            e = A.Num(float(s.value), None, False).at(s)
            e.extra_int = s.value
            if self.tok.kind in ("SUP",):
                raise self.error("two exponents in a row")
            r = self.span(A.BinOp("^", base, e), t)
            if isinstance(base, A.Num) and base.digit and self.tok.kind == "NAME" and self._unit_tok(self.tok) \
                    and self.tok.value not in self.known and not self._is_call_like():
                u = self.unit_expr(explicit=False)      # 10⁸ m/s, like 10^8 m/s
                r = self.span(A.Quantity(r, u), t)
            return r
        if self.at_op("^"):
            self.next()
            ex = self.exponent()
            r = self.span(A.BinOp("^", base, ex), t)
            if isinstance(base, A.Num) and base.digit and self.tok.kind == "NAME" and self._unit_tok(self.tok) \
                    and self.tok.value not in self.known and not self._is_call_like():
                u = self.unit_expr(explicit=False)      # 10^8 m/s
                r = self.span(A.Quantity(r, u), t)
            return r
        return base

    def exponent(self):
        """The thing after ^ : a signed atom, possibly itself raised (right-assoc)."""
        t = self.tok
        if self.at_op("-"):
            self.next()
            return self.span(A.Neg(self.exponent()), t)
        if self.at_op("+"):
            self.next()
            return self.exponent()
        if self.tok.kind in ("NUM", "IMAG"):
            nt = self.next()
            base = A.Num(nt.value, nt.sigfigs, nt.digit)
            base.line, base.col, base.length = nt.line, nt.col, len(nt.raw)
            if nt.kind == "IMAG":
                base = A.Name("𝑖").at(base) if nt.value == 1 and nt.sigfigs is None else \
                    A.BinOp("*", base, A.Name("𝑖").at(base)).at(base)
        else:
            base = self.postfix()
        if self.at_op("^"):
            self.next()
            return self.span(A.BinOp("^", base, self.exponent()), t)
        if self.tok.kind == "SUP":
            s = self.next()
            return self.span(A.BinOp("^", base, A.Num(float(s.value), None, False).at(s)), t)
        return base

    def nabla_op(self):
        """∇f, ∇·f, ∇×f, ∇²f (ASCII: nabla f, nabla*F, nabla×F, nabla^2 f)."""
        t = self.next()
        kind = "grad"
        if self.tok.kind == "SUP" and self.tok.value == 2:
            self.next()
            kind = "lap"
        elif self.at_op("^") and self.peek().kind == "NUM" and self.peek().value == 2:
            self.next()
            self.next()
            kind = "lap"
        elif self.at_op("*"):
            self.next()
            kind = "div"
        elif self.at_op("×"):
            self.next()
            kind = "curl"
        if self.tok.kind != "NAME":
            raise self.error("∇ needs the name of a function after it, like ∇φ, ∇·E, ∇×B or ∇²φ" + self._found())
        n = self.next()
        name = A.Name(n.value)
        name.line, name.col, name.length = n.line, n.col, len(n.raw)
        return self.span(A.VecCalc(kind, name), t)

    def postfix(self):
        t = self.tok
        e = self.atom()
        while True:
            # (x+1)(x-1) is a product, but (∂/∂x f)(1, 2) and (f')(3) are calls (A42)
            if self.at_op("(") and not self.tok.ws_before and not isinstance(e, (A.Num, A.Quantity)) and \
                    (not e.paren or isinstance(e, (A.Deriv, A.Prime))):
                if isinstance(e, A.Name) and e.name == "table" and "table" not in self.known and \
                        self.peek().kind == "NAME" and self.at_op_at(self.i + 2, "="):
                    e = self.table_args(e, t)
                    continue
                self.next()
                args = []
                self.skip_newlines()
                while not self.at_op(")"):
                    args.append(self.expr_full())
                    if self.at_op(","):
                        self.next()
                    elif not self.at_op(")"):
                        raise self.error("expected ',' or ')' in the list of arguments" + self._found())
                self.next()
                e = self.span(A.Call(e, args), t)
                if isinstance(e.func, A.Name) and e.func.name in VEC_CALC_WORDS and e.func.name not in self.known \
                        and len(args) == 1 and isinstance(args[0], A.Name):     # grad(f) is ∇f
                    e = self.span(A.VecCalc(VEC_CALC_WORDS[e.func.name], args[0]), t)
                if isinstance(e.func, A.Name) and e.func.name == "vec" and self.tok.kind == "NAME" and \
                        self._unit_tok(self.tok) and self.tok.value not in self.known:
                    u = self.unit_expr(explicit=False)          # vec(3, 4) m/s
                    e = self.span(A.Quantity(e, u), t)
            elif self.at_op("[") and not self.tok.ws_before:
                st = self.next()
                idx = None if self.at_op(":") else self.expr()
                if self.at_op(":"):                     # xs[a:b], xs[:b], xs[a:] (D114)
                    self.next()
                    hi = None if self.at_op("]") else self.expr()
                    idx = self.span(A.Slice(idx, hi), st)
                e = self.span(A.Index(e, idx), t)
                if self.at_op(","):                     # M[i, j] is M[i][j] (a matrix entry, D29)
                    self.next()
                    e = self.span(A.Index(e, self.expr()), t)
                self.expect_op("]")
            elif self.at_op("ᵀ"):                       # Mᵀ is transpose(M)
                self.next()
                e = self.span(A.Call(A.Name("transpose").at(e), [e]), t)
            elif self.at_op("[") and self.tok.ws_before and not isinstance(e, A.Num) and self._bracket_is_unit():
                u = self.bracket_unit()
                e = self.span(A.Quantity(e, u, bracket=True), t)
            elif self.at_op(".") and not self.tok.ws_before and (self.peek().kind == "NAME" or (
                    self.peek().kind == "KW" and self.peek(2).kind == "OP" and self.peek(2).value == "("
                    and not self.peek(2).ws_before)):      # np.sqrt(x): a Python function named like a keyword
                self.next()
                name = self.next()
                e = self.span(A.Field(e, name.value), t)
                e.raw = name.raw             # the spelling as written (np.pi, not np.π) for Python calls (D140)
            elif self.tok.kind == "PRIME":
                p = self.next()
                e = self.span(A.Prime(e, p.value), t)
            else:
                return e

    def table_args(self, e, t):
        """table(x = xs, y = ys): named columns (D193)."""
        self.next()
        names, items = [], []
        self.skip_newlines()
        while not self.at_op(")"):
            nt = self.expect_name("a column name, like  table(x = xs, y = ys)")
            if nt.value in names:
                raise self.error(f"the column {nt.value} appears twice in this table", tok=nt)
            self.expect_op("=", "after the column name (write: table(x = xs, y = ys))")
            names.append(nt.value)
            items.append(self.expr_full())
            self.skip_newlines()
            if self.at_op(","):
                self.next()
                self.skip_newlines()
            elif not self.at_op(")"):
                raise self.error("expected ',' or ')' in this table" + self._found())
        self.next()
        return self.span(A.Table(names, items), t)

    def atom(self):
        t = self.tok
        if t.kind in ("NUM", "IMAG"):
            self.next()
            n = A.Num(t.value, t.sigfigs, t.digit)
            n.line, n.col, n.length = t.line, t.col, len(t.raw)
            if t.kind == "NUM":
                n.raw = t.raw                 # quoted as written in warnings (#81)
            if t.kind == "IMAG":             # 4i is 4 × 𝑖 and 1i is 𝑖 (D90); a unit may follow: 4i Ω
                n = A.Name("𝑖").at(n) if t.value == 1 and t.sigfigs is None else \
                    A.BinOp("*", n, A.Name("𝑖").at(n)).at(n)
                n.imag_literal = True
            if t.digit:
                if self.at_op("[") and self._bracket_is_unit():
                    u = self.bracket_unit()
                    return self.span(A.Quantity(n, u, bracket=True), t)
                if self.tok.kind == "NAME" and self._unit_tok(self.tok) and not self._is_call_like():
                    ustart = self.tok
                    u = self.unit_expr(explicit=False)
                    q = self.span(A.Quantity(n, u), t)
                    if len(u.factors) == 1 and u.factors[0].name in self.known:
                        self._single_collision(q, ustart)       # `0.1 m` with your m: which one? (A1, D235)
                    return q
                self._check_mixed_reciprocal(t.raw)
                if self._unit_reciprocal_follows() or self._bracketed_reciprocal_unit():
                    u = self.unit_expr(explicit=True, reciprocal=True)
                    return self.span(A.Quantity(n, u), t)
            return n
        if t.kind == "STR":
            self.next()
            return self.span(A.Str(t.value), t)
        if t.kind == "NAME":
            if t.value == "d" and self._is_deriv_op():
                return self.deriv_op()
            if t.value == "d":
                lz = self._leibniz_higher()
                if lz is not None:
                    return lz
            if t.value == "∞" and self.peek().kind == "NAME" and self._unit_tok(self.peek()) and \
                    self.peek().value not in self.known:
                self.next()
                n = self.span(A.Name("∞"), t)
                u = self.unit_expr(explicit=False)
                return self.span(A.Quantity(n, u), t)
            if t.value in ("Σ", "sum") and self.peek().kind == "OP" and self.peek().value == "(":
                j = self._sum_for_index()
                if j is not None:
                    return self.sum_expr(j)
            self.next()
            if t.value == "end" and self.in_index():
                return self.span(A.End(), t)
            return self.span(A.Name(t.value), t)
        if t.kind == "KW":
            if t.value == "to" and self.peek().kind == "OP" and self.peek().value == "(" and not self.peek().ws_before:
                self.next()
                return self.span(A.Name("to"), t)
            if t.value in ("true", "false"):
                self.next()
                return self.span(A.Bool(t.value == "true"), t)
            if t.value in ("sqrt", "cbrt"):
                self.next()
                start_i = self.i
                operand = self.power()
                t.extra["operand_end"] = self.i - 1
                st = self.toks[start_i]
                t.extra["operand_paren"] = st.kind == "OP" and st.value == "(" and \
                    self._match(start_i) == self.i - 1
                return self.span(A.Sqrt(operand, 2 if t.value == "sqrt" else 3), t)
            if t.value == "integral":
                if t.raw == "integral" and self.at_op_at(self.i + 1, "(") and not self.peek().ws_before:
                    k = self._match(self.i + 1)
                    depth = 0
                    for m in range(self.i + 2, k or self.i + 2):
                        tk = self.toks[m]
                        if tk.kind == "OP" and tk.value in "([{":
                            depth += 1
                        elif tk.kind == "OP" and tk.value in ")]}":
                            depth -= 1
                        elif depth == 0 and tk.kind == "OP" and tk.value == ",":
                            raise self._keyword_as_name(t)          # integral(1, 2): a call (#78)
                return self.integral()
            if t.value == "partial":
                return self.partial_op()
            if t.value == "nabla":
                return self.nabla_op()
            if t.value == "load":
                self.next()
                if self.tok.kind != "STR":
                    raise self.error("expected a file name in quotes after load, like load \"data.csv\"")
                p = self.next()
                return self.span(A.Load(p.value), t)
        if t.kind == "OP":
            if t.value == "(":
                self.next()
                saved_abs = self.abs_depth
                self.abs_depth = 0
                self.skip_newlines()
                e = self.expr_full()
                self.skip_newlines()
                self.abs_depth = saved_abs
                if not self.at_op(")"):
                    if self.tok.kind in ("NEWLINE", "EOF", "INDENT", "DEDENT"):
                        raise self.error("this '(' is never closed", tok=t, hint="add the missing ')'")
                    raise self.error("expected ')' to close '('" + self._found())
                self.next()
                e.paren = True
                return e
            if t.value == "[":
                self.next()
                items = []
                while not self.at_op("]"):
                    items.append(self.expr())
                    if self.at_op(","):
                        self.next()
                    elif not self.at_op("]"):
                        raise self.error("expected ',' or ']' in this list" + self._found())
                self.next()
                lst = self.span(A.ListLit(items), t)
                if items and self.tok.kind == "NAME" and \
                        self._unit_tok(self.tok) and self.tok.value not in self.known and not self._is_call_like():
                    # [[1, 2], [3, 4]] N/m  (a matrix, D29) and [1, 2, 3] m  (a list, D192)
                    u = self.unit_expr(explicit=False)
                    lst = self.span(A.Quantity(lst, u), t)
                elif items and self.tok.kind == "NAME" and self._unit_tok(self.tok) and not self._is_call_like():
                    self._list_collision(t)       # [1, 2, 3] m with your m: a unit follows a list as a number (D235)
                return lst
            if t.value == "<":
                return self.vector_literal()
            if t.value == "|":
                self.next()
                self.abs_depth += 1
                e = self.expr()
                self.abs_depth -= 1
                self.expect_op("|", "to close the absolute value |x|")
                return self.span(A.Abs(e), t)
            if t.value == "+-":
                raise self.error("± needs a value on its left, like  L = 1.20 ± 0.01 m")
        if t.kind == "NEWLINE" or t.kind == "EOF":
            prev = self.toks[self.i - 1] if self.i > 0 else None
            pp = self.toks[self.i - 2] if self.i > 1 else None
            if prev is not None and pp is not None and prev.kind == pp.kind == "OP" and prev.value == pp.value \
                    and prev.value in ("+", "-"):
                raise self.error("this line ended before the expression was complete",
                                 hint=f"Fermium has no {prev.value}{prev.value}; write  x {prev.value}= 1")
            raise self.error("this line ended before the expression was complete")
        if t.kind == "OP" and t.value == "=":
            raise self.error("unexpected '='", hint="use == to compare two values")
        raise self.error(f"didn't expect '{t.raw}' here")

    def _sum_for_index(self):
        """At `Σ (` or `sum (`: the index of a `for` at the top level inside the brackets, or None."""
        depth = 0
        j = self.i + 1
        while j < len(self.toks):
            tk = self.toks[j]
            if tk.kind == "EOF":
                return None
            if tk.kind == "OP" and tk.value in "([{":
                depth += 1
            elif tk.kind == "OP" and tk.value in ")]}":
                depth -= 1
                if depth == 0:
                    return None
            elif depth == 1 and tk.kind == "KW" and tk.value == "for":
                return j
            j += 1
        return None

    def sum_expr(self, j):
        """Σ(k² for k from 1 to 10) or sum(f(k) for k from 1 to N step 2): a one-line sum (#49, D51)."""
        t = self.next()
        self.next()                                   # (
        if self.toks[j + 1].kind == "NAME":
            self.known.add(self.toks[j + 1].value)    # the summation variable is a variable, not a unit
        body = self.expr()
        if not self.at_kw("for"):
            raise self.error("expected 'for' in this sum (write: Σ(k² for k from 1 to 10))" + self._found())
        self.next()
        vt = self.expect_name("the summation variable (write: Σ(k² for k from 1 to 10))")
        self.expect_kw("from", "(write: Σ(k² for k from 1 to 10))")
        lo = self.expr()
        self.expect_kw("to", "(write: Σ(k² for k from 1 to 10))")
        hi = self.expr()
        step = None
        if self.at_kw("step"):
            self.next()
            step = self.expr()
        if not self.at_op(")"):
            raise self.error("expected ')' to close this sum" + self._found())
        self.next()
        return self.span(A.Sum(body, vt.value, lo, hi, step), t)

    def _unit_words(self, name):
        from .units import lookup_unit, dim_name
        words = {"g": "grams", "m": "metres", "s": "seconds", "L": "litres", "l": "litres", "V": "volts",
                 "T": "tesla", "b": "barns", "A": "amperes", "K": "kelvin", "N": "newtons", "J": "joules",
                 "W": "watts", "C": "coulombs", "F": "farads", "H": "henries", "Pa": "pascals",
                 "u": "atomic mass units", "d": "days", "min": "minutes", "yr": "years", "h": "hours",
                 "t": "tonnes", "au": "AU", "pc": "parsecs"}
        u = lookup_unit(name)
        return words.get(name) or (dim_name(u.dim).split(" [")[0] if u is not None else "a unit")

    def _check_mixed_reciprocal(self, num):
        """`0.300 /(m s²)` right after a number, with your own m: the bracket holds only unit names, some of them your
        variables and some not, so it is neither the unit 1/(m s²) nor a division by variables: ask (D235)."""
        if not (self.at_op("/") and self.at_op_at(self.i + 1, "(") and self._bracket_all_units(self.i + 1)):
            return
        k = self._match(self.i + 1)
        names = [tk for tk in self.toks[self.i + 2:k] if tk.kind == "NAME"]
        yours = [tk for tk in names if tk.value in self.known]
        if not yours or len(yours) == len(names):
            return
        text = self._unit_span_text(self.i, k)
        v = yours[0].raw
        raise self.error(f"'{num} {text}' is ambiguous: right after a number, {text} is the unit 1{text}, but {v} is "
                         f"also your variable {v}", tok=yours[0],
                         hint=f"write  {num} [1{text}]  for the unit, or give the units their own number to divide "
                              f"by your {v}")

    def _bracketed_reciprocal_unit(self):
        """`0.300 /(m s²)` right after a number: '/' then brackets holding only unit names and exponents, none of
        them your variable (gauntlet #72; spaces don't matter, D235)."""
        t = self.tok
        if not (t.kind == "OP" and t.value == "/" and self.at_op_at(self.i + 1, "(")):
            return False
        k = self._match(self.i + 1)
        if k is None or k == self.i + 2:
            return False
        first = self.toks[self.i + 2]
        if first.kind != "NAME":
            return False
        prev = None
        for m in range(self.i + 2, k):
            tk = self.toks[m]
            if tk.kind == "NAME":
                if not self._unit_tok(tk) or tk.value in self.known:
                    return False
            elif tk.kind == "SUP":
                pass
            elif tk.kind == "NUM":
                if not (prev is not None and ((prev.kind == "OP" and prev.value in ("^", "-", "(", "/")))):
                    return False
            elif tk.kind == "OP" and tk.value in ("^", "/", "*", "(", ")", "-"):
                pass
            else:
                return False
            prev = tk
        nx = self.toks[k + 1] if k + 1 < len(self.toks) else None
        if nx is not None and nx.kind == "OP" and nx.value == "(" and not nx.ws_before:
            return False
        return True

    def _unit_reciprocal_follows(self):
        """`0.1 1/s`, `0.1 /s` or `0.1/s` right after a number: a unit name after '/' that isn't your variable (a name
        that is your variable is divided by: it doesn't come right after the number; spaces don't matter, D235)."""
        t = self.tok
        if t.kind == "NUM" and t.value == 1 and t.digit and self.peek().kind == "OP" and self.peek().value == "/":
            nx = self.peek(2)
            return nx.kind == "NAME" and self._unit_tok(nx) and nx.value not in self.known
        if t.kind == "OP" and t.value == "/" and self.peek().kind == "NAME" and self._unit_tok(self.peek()) and \
                self.peek().value not in self.known and \
                not (self.peek(2).kind == "OP" and self.peek(2).value == "(" and not self.peek(2).ws_before):
            return True
        return False

    def vector_literal(self):
        """<a, b>, <a, b, c> or <a, b, c, d>, optionally followed by a unit: <3, 4> m/s."""
        t = self.next()
        items = []
        while True:
            items.append(self.sum())
            if self.at_op(","):
                self.next()
                continue
            break
        if not self.at_op(">"):
            raise self.error("expected '>' to close this vector (written <x, y> or <x, y, z>)" + self._found())
        self.next()
        if not 2 <= len(items) <= 16:
            raise self.error(f"a vector needs 2 to 16 components, not {len(items)}", tok=t)
        v = self.span(A.VecLit(items), t)
        if self.tok.kind == "NAME" and self._unit_tok(self.tok) and not self._is_call_like():
            u = self.unit_expr(explicit=False)
            v = self.span(A.Quantity(v, u), t)
        elif self._unit_reciprocal_follows():
            # `<0, 0> /s` and `<1, 2> 1/s`, as after a number (#55)
            u = self.unit_expr(explicit=True, reciprocal=True)
            v = self.span(A.Quantity(v, u), t)
        return v

    def in_index(self):
        # crude: are we inside [...] after a name? look back for an unmatched '['
        depth = 0
        for j in range(self.i - 1, -1, -1):
            tk = self.toks[j]
            if tk.kind == "OP" and tk.value == "]":
                depth += 1
            elif tk.kind == "OP" and tk.value == "[":
                if depth == 0:
                    return True
                depth -= 1
            elif tk.kind in ("NEWLINE",):
                return False
        return False

    def _is_call_like(self):
        # `3 m(…)` is not sensible; treat NAME( right after number as unit anyway unless it's a call
        nxt = self.peek()
        return nxt.kind == "OP" and nxt.value == "(" and not nxt.ws_before

    # ------------------------------------------------------------ calculus syntax
    def _is_deriv_op(self):
        # d/dt X   or   d²/dt² X   or d^2/dt^2 X
        j = self.i + 1
        order = 1
        if self.toks[j].kind == "SUP":
            order = self.toks[j].value
            j += 1
        elif self.toks[j].kind == "OP" and self.toks[j].value == "^" and self.toks[j + 1].kind == "NUM":
            j += 2
        if not (self.toks[j].kind == "OP" and self.toks[j].value == "/"):
            return False
        v = self.toks[j + 1]
        if v.kind != "NAME" or not v.raw.startswith("d") or len(v.raw) < 2:
            return False
        k = j + 2
        if self.toks[k].kind == "SUP" or (self.toks[k].kind == "OP" and self.toks[k].value == "^"):
            k += 1 if self.toks[k].kind == "SUP" else 2
        nxt = self.toks[k]
        del order
        # an operand must follow on the same line
        return nxt.kind in ("NAME", "NUM") or (nxt.kind == "OP" and nxt.value in ("(", "|")) or \
            (nxt.kind == "KW" and nxt.value in ("sqrt", "cbrt"))

    def _leibniz_higher(self):
        """d²x/dt² or d^2x/dt^2 -> Deriv(t, 2, x); returns None if the tokens don't match."""
        j = self.i + 1
        tk = self.toks
        if tk[j].kind == "SUP":
            order, j = tk[j].value, j + 1
        elif tk[j].kind == "OP" and tk[j].value == "^" and tk[j + 1].kind == "NUM" and tk[j + 1].value == int(
                tk[j + 1].value) and not tk[j + 2].ws_before:
            order, j = int(tk[j + 1].value), j + 2
        else:
            return None
        if tk[j].kind != "NAME" or tk[j].ws_before:
            return None
        xname = tk[j]
        if not (tk[j + 1].kind == "OP" and tk[j + 1].value == "/"):
            return None
        v = tk[j + 2]
        if v.kind != "NAME" or not v.raw.startswith("d") or len(v.raw) < 2:
            return None
        k = j + 3
        if tk[k].kind == "SUP":
            o2, k = tk[k].value, k + 1
        elif tk[k].kind == "OP" and tk[k].value == "^" and tk[k + 1].kind == "NUM":
            o2, k = int(tk[k + 1].value), k + 2
        else:
            return None
        if o2 != order:
            raise self.error(f"the orders don't match in this derivative (d{order}.../d...{o2})", tok=v)
        start = self.tok
        self.i = k
        x = A.Name(xname.value)
        x.line, x.col, x.length = xname.line, xname.col, len(xname.raw)
        return self.span(A.Deriv(canonical_name(v.raw[1:]), order, x), start)

    def _deriv_order(self):
        if self.tok.kind == "SUP":
            return self.next().value
        if self.at_op("^"):
            self.next()
            n = self.next()
            if n.kind != "NUM" or n.value != int(n.value) or not 1 <= n.value <= 10:
                raise self.error("the order of a derivative must be a whole number like d²/dt²", tok=n)
            return int(n.value)
        return 1

    def deriv_op(self):
        t = self.next()  # d
        order = self._deriv_order()
        self.expect_op("/")
        v = self.next()
        var = canonical_name(v.raw[1:])
        o2 = self._deriv_order()
        if o2 != order:
            raise self.error(f"the orders don't match: d{order}/d{var}{o2}", tok=v)
        operand = self._deriv_operand(var)
        return self.span(A.Deriv(var, order, operand), t)

    def _deriv_operand(self, var):
        """The operand of d/ds or ∂/∂s.  Its variable counts as your variable for D7, and a lone unit of that name
        right after a number (`d/ds h(2 s)`) is an error: the reader means 2s, the unit rule says 2 seconds, and
        D221 would silently give h' at 2 seconds (red team round 6 #6, D231)."""
        saved_known, saved_dv = self.known, self.deriv_vars
        self.known = saved_known | {var}
        self.deriv_vars = saved_dv | {var}
        try:
            return self.power()
        finally:
            self.known, self.deriv_vars = saved_known, saved_dv

    def partial_op(self):
        t = self.next()   # ∂
        order = self._deriv_order()      # ∂²/∂x² or partial^2/partial x^2 (what fmt --ascii writes)
        if self.tok.kind == "NAME" and self.peek().kind == "OP" and self.peek().value == "/" and \
                self.peek(2).kind == "KW" and self.peek(2).value == "partial":
            # Leibniz form ∂u/∂t, ∂²u/∂x² (D83): the derivative of the function u
            fn = self.next()
            self.next()
            self.next()
            v = self.expect_name("the variable to differentiate by")
            o2 = self._deriv_order()
            if o2 != order and o2 != 1:
                raise self.error(f"the orders don't match: ∂{order}{fn.value}/∂{v.value}{o2}", tok=v)
            return self.span(A.Deriv(v.value, order, self.span(A.Name(fn.value), fn), partial=True), t)
        self.expect_op("/", "(write ∂/∂x f)")
        self.expect_kw("partial", "(write ∂/∂x f)")
        v = self.expect_name("the variable to differentiate by")
        o2 = self._deriv_order()
        if o2 != order and o2 != 1:
            raise self.error(f"the orders don't match: ∂{order}/∂{v.value}{o2}", tok=v)
        operand = self._deriv_operand(v.value)
        return self.span(A.Deriv(v.value, order, operand, partial=True), t)

    def integral(self):
        t = self.next()
        # The integration variable is a variable inside the integrand: `∫ 1/u du` is 1/u, not
        # "per atomic mass unit" (A54). Look ahead for the trailing `du` before reading the integrand.
        new = self._integration_vars() - self.known
        self.known |= new
        saved_limit, self.limit_start = self.limit_start, None
        self.in_integrand += 1
        try:
            body = self.sum()
            integrand, var = self._split_dvar(body)
        finally:
            self.known -= new
            self.limit_start = saved_limit
            self.in_integrand -= 1
        if var is None:
            raise self.error("this integral is missing its 'dx' (the variable to integrate over)", tok=t,
                             hint="write e.g.  ∫ F(x) dx from 0 m to 1 m")
        lo = hi = None
        if self.at_kw("from"):
            self.next()
            lo = self.sum()
            self.expect_kw("to")
            saved, self.limit_start = self.limit_start, self.i
            hi_start = self.i
            try:
                hi = self.sum()
            finally:
                self.limit_start = saved
            sum_info = self._sum_in_limit(hi_start, hi)
            div_tok = None
            if self.at_op("/") and self.tok.ws_before and self._divisor_follows() and \
                    not self._limit_is_infinite(hi_start):
                div_tok = self.tok
            node = self.span(A.Integral(integrand, var, lo, hi), t)
            # Both limit warnings (D112, D173) are decided by the checker, which knows whether each reading
            # has consistent units (red team round 4 #6, #7, D205)
            node.sum_info = sum_info
            if div_tok is not None:
                node.div_info = {"tok": div_tok, "hi_text": self._text(hi_start, self.i)}
            return node
        return self.span(A.Integral(integrand, var, lo, hi), t)

    def _sum_in_limit(self, start, hi):
        """`2 ∫ x dx from 0 to 1 - π`: the ' - π' is part of the upper limit (the integral goes up to 1 - π),
        which on paper usually means (2 ∫ …) - π.  The parse is kept (`to L - a` is a limit); a spaced binary
        + or - at the top level of the upper limit is recorded here, and the checker warns when both readings
        have consistent units (gauntlet #67, D173; red team round 4 #6, D205).  Returns None or a dict with
        the token, the texts, and the head and first rest term of the limit's AST."""
        depth = bars = 0
        for j in range(start, self.i):
            tk = self.toks[j]
            if tk.kind == "OP" and tk.value in "([{":
                depth += 1
            elif tk.kind == "OP" and tk.value in ")]}":
                depth -= 1
            elif tk.kind == "OP" and tk.value == "|":
                bars += 1
            elif tk.kind == "OP" and tk.value in ("+", "-") and j > start and depth == 0 and bars % 2 == 0 \
                    and tk.ws_before and j + 1 < self.i and self.toks[j + 1].ws_before:
                pv = self.toks[j - 1]
                if pv.kind == "OP" and pv.value not in (")", "]", "}", "|"):
                    continue                              # `to 2 * -1`: a sign, not a sum
                # the first split of the left spine of the sum: head = its left, rest = its right
                split, n = None, hi
                while isinstance(n, A.BinOp) and n.op in ("+", "-") and not n.paren and not n.implicit:
                    split, n = n, n.left
                return {"tok": tk, "limit": self._text(start, self.i), "rest": self._text(j, self.i),
                        "head": self._text(start, j), "op": tk.value,
                        "head_node": split.left if split is not None else None,
                        "rest_node": split.right if split is not None else None}
        return None

    def _divisor_follows(self):
        """After a spaced '/' that ended an upper limit: is a divisor next (a number, a name, or a
        bracketed expression like `/ (1 + z)`)? (FRICTION #8, and research: #61)"""
        nx = self.peek()
        return nx.kind in ("NUM", "NAME") or (nx.kind == "OP" and nx.value in ("(", "[", "|")) or \
            (nx.kind == "KW" and nx.value in ("sqrt", "cbrt"))

    def _limit_is_infinite(self, start):
        """Is the upper limit starting at token `start` ±∞ (possibly with a unit)? Then ∞ / x is ∞
        and both readings of `to ∞ / (μ₀ I)` agree, so the '/' dividing the integral needs no warning."""
        j = start
        while j < self.i and self.toks[j].kind == "OP" and self.toks[j].value in "+-":
            j += 1
        return j < self.i and self.toks[j].kind == "NAME" and self.toks[j].value == "∞"

    def _ends_upper_limit(self):
        """At '/': does it end an integral's upper limit? A '/' with a space before it, outside any
        brackets opened in the limit, ends it: `∫ f dx from 0 to ∞ / (μ₀ I)` divides the integral,
        while `from 0 to 1/2` has the limit ½ (FRICTION #8, D34)."""
        if self.limit_start is None or not self.tok.ws_before:
            return False
        depth = bars = 0
        for j in range(self.limit_start, self.i):
            tk = self.toks[j]
            if tk.kind == "OP" and tk.value in "([{":
                depth += 1
            elif tk.kind == "OP" and tk.value in ")]}":
                depth -= 1
            elif tk.kind == "OP" and tk.value == "|":
                bars += 1
        return depth == 0 and bars % 2 == 0

    def _integration_vars(self):
        """Names v of the `dv` tokens that end the integrand starting at the current token."""
        depth, j, last = 0, self.i, []
        while j < len(self.toks):
            t = self.toks[j]
            if t.kind in ("NEWLINE", "EOF") or (depth == 0 and t.kind == "KW") or \
                    (depth == 0 and t.kind == "OP" and t.value in (",", "=")):
                break
            if t.kind == "OP" and t.value in "([{":
                depth += 1
            elif t.kind == "OP" and t.value in ")]}":
                if depth == 0:
                    break
                depth -= 1
            if depth == 0 and t.kind == "NAME" and t.value.startswith("d") and len(t.value) > 1:
                last.append(canonical_name(t.value[1:]))
            elif not (t.kind == "OP" and t.value in ")]}"):
                last = []
            j += 1
        return set(last)

    def _split_dvar(self, e):
        """Pull a trailing `dx` out of the integrand: `F(x) dx` -> (F(x), 'x')."""
        if isinstance(e, A.Name) and e.name.startswith("d") and len(e.name) > 1 and not e.paren:
            return A.Num(1.0, None, False).at(e), canonical_name(e.name[1:])
        if isinstance(e, A.Quantity) and not e.bracket and not e.paren and e.unit.factors:
            # `∫ 2 dm from 0 kg to 1 kg`: after a number `dm` lexes as decimetres, but at the end of an
            # integral that has no other differential, d + a unit name is the differential (D27)
            f = e.unit.factors[-1]
            if f.exp == 1 and len(f.name) > 1 and f.name.startswith("d") and is_unit_name(f.name[1:]) \
                    and e.unit.text.endswith(f.name):
                var = canonical_name(f.name[1:])
                if len(e.unit.factors) == 1:
                    return e.value, var
                u = A.UnitExpr(e.unit.factors[:-1], e.unit.text[:-len(f.name)].rstrip()).at(e.unit)
                return A.Quantity(e.value, u, e.bracket).at(e), var
        if isinstance(e, A.Neg) and not e.paren:
            # `∫ x * -2 dx`: the dx sits inside the negated factor (red team round 4 #5)
            inner, var = self._split_dvar(e.operand)
            if var is not None:
                n = A.Neg(inner)
                n.line, n.col, n.length = e.line, e.col, e.length
                return n, var
            return e, None
        if isinstance(e, A.BinOp) and not e.paren:
            if e.implicit and isinstance(e.right, A.Name) and e.right.name.startswith("d") \
                    and len(e.right.name) > 1:
                return e.left, canonical_name(e.right.name[1:])
            if e.op in ("+", "-", "*", "/") or e.implicit:
                inner, var = self._split_dvar(e.right)
                if var is not None:
                    n = A.BinOp(e.op, e.left, inner, e.implicit)
                    n.line, n.col, n.length = e.line, e.col, e.length
                    return n, var
        return e, None

    # ------------------------------------------------------------ units
    def bracket_unit(self):
        self.expect_op("[")
        u = self.unit_expr(explicit=True)
        self.expect_op("]", "to close the unit brackets")
        return u

    def unit_expr(self, explicit, reciprocal=False):
        """Parse a unit expression like `kg m/s²`.

        explicit=True: inside [...] or after `in` -- every name is a unit.
        explicit=False: right after a number -- the first name is a unit; later names
        are units only if they aren't variables you've defined (see DECISIONS.md).
        """
        start = self.tok
        factors = []
        first = True

        def factor(sign):
            if self.at_op("("):
                self.next()
                inner = self.unit_expr(explicit=True)
                self.expect_op(")")
                exp = self.unit_exponent()
                for f in inner.factors:
                    factors.append(A.UnitFactor(f.name, f.exp * exp * sign).at(f))
                return
            if self.tok.kind == "NUM" and self.tok.value == 1.0 and explicit:
                self.next()
                return
            if self.tok.kind != "NAME":
                raise self.error("expected a unit name" + self._found())
            nt = self.next()
            nt.role = "unit"
            name = nt.raw
            if not explicit:
                self._check_prefix_split(nt)
            exp = self.unit_exponent()
            f = A.UnitFactor(name, exp * sign)
            f.line, f.col, f.length = nt.line, nt.col, len(nt.raw)
            factors.append(f)

        def unit_name_here(k=0):
            tk = self.peek(k) if k else self.tok
            return tk.kind == "NAME" and self._unit_tok(tk)

        # first factor
        if reciprocal:
            if self.tok.kind == "NUM":
                self.next()
            self.next()   # /
            factor(-1)
        else:
            factor(1)
        first = False
        del first
        juxt_join = False
        while True:
            # The unit goes on through `/`, `*` and spaces while unit names follow (spaces never matter).
            # Right after a number (not explicit), a later name that is also your variable is an error that
            # asks which you mean (the A1 rule, sentence 3; D235).
            t = self.tok
            self._no_hour_h(t, explicit, start)
            if t.kind == "OP" and t.value == "/" and unit_name_here(1):
                if not explicit and self.peek().value in self.known:
                    if self._later_collision(start, self.peek(), old_continues=not t.ws_before):
                        self.next()
                        factor(-1)
                        continue
                    break
                self.next()
                factor(-1)
            elif t.kind == "OP" and t.value == "/" and self.peek().kind == "OP" and self.peek().value == "(" and (
                    explicit or (unit_name_here(2) and self._bracket_all_units(self.i + 1))):
                clash = [] if explicit else [tk for tk in self.toks[self.i + 2:self._match(self.i + 1) or self.i]
                                             if tk.kind == "NAME" and tk.value in self.known]
                if clash and not self._later_collision(start, clash[0], old_continues=not t.ws_before):
                    break
                self.next()
                factor(-1)
            elif t.kind == "OP" and t.value == "/" and explicit and self.peek().kind == "NUM":
                break
            elif t.kind == "OP" and t.value == "*" and unit_name_here(1):
                if not explicit and self.peek().value == "c":
                    break                 # `2 kg * c²`: the constant c multiplies (D235)
                if not explicit and self.peek().value in self.known:
                    break                 # `1.2 fm * A^(1/3)`: an explicit * before your variable multiplies (D235)
                self.next()
                factor(1)
                juxt_join = True
            elif unit_name_here() and (explicit or not self._is_call_like()):
                if not explicit and self.peek().kind == "OP" and self.peek().value == "(" and not self.peek().ws_before:
                    break
                if not explicit and t.value == "c":
                    break                 # `2 m c²`: c continues a unit only after '/' (`MeV/c²`), D235
                if not explicit and self.in_integrand and t.raw.startswith("d") and len(t.raw) > 1 and \
                        canonical_name(t.raw[1:]) in self.known:
                    break                 # `∫ 2 [m] dm`: the trailing dm is the differential, not decimetres
                if not explicit and t.value in self.known:
                    if not self._later_collision(start, t, old_continues=False):
                        break
                factor(1)
                juxt_join = True
            else:
                break
        u = A.UnitExpr(factors, ("1" if reciprocal and self._unit_text(start).startswith("/") else "")
                       + self._unit_text(start))
        u.line, u.col = start.line, start.col
        u.length = max(1, self.toks[self.i - 1].end - start.start)
        u.juxt_join = juxt_join
        if start in self._fix_whole:
            self._fix_whole.remove(start)
            self._add_fix(self._bracket_fix(start, self.toks[self.i - 1]))
        return u

    # Prefixed units a physics course uses all the time: never read as two of your variables (D203)
    COMMON_PREFIXED = frozenset("""
        kg mg μg ug km cm mm μm um nm pm fm dm ms μs us ns ps fs kHz MHz GHz THz mV kV MV μV uV mA μA uA nA pA
        kW MW GW mW μW kJ MJ GJ mJ μJ keV MeV GeV TeV meV kPa MPa GPa hPa mL μL mmol kmol μmol mT μT uT nT μF uF
        nF pF mF mH μH uH kΩ MΩ mΩ kohm Mohm μC uC nC pC mC mK μK nK kN mN kBq MBq GBq mGy mSv μSv uSv mbar kcal
        Myr Gyr kyr kpc Mpc Gpc mrad μrad krad dB""".split())

    def _check_prefix_split(self, nt):
        """`E = 1.5 kT` with your own k and T is the unit kilotesla, while `k T` was meant: a prefixed unit
        that spells two of your variables (prefix and unit) is almost never meant as the unit, unless it is
        a unit everybody uses (`500 nm` next to a refractive index n and an order m).  Warn, like a lone
        unit that is also your variable (D7 rule 5; red team round 4 #3, D203)."""
        from .units import lookup_unit, PREFIXES, _UNITS
        name = nt.raw
        if name in _UNITS or name in self.known or name in self.COMMON_PREFIXED or lookup_unit(name) is None:
            return
        for p in sorted(PREFIXES, key=len, reverse=True):
            rest = name[len(p):]
            if name.startswith(p) and rest and p in self.known and rest in self.known:
                if name in self.warned_units:
                    return
                self.warned_units.add(name)
                k = self.toks.index(nt)
                num = self.toks[k - 1].raw if k > 0 and self.toks[k - 1].kind == "NUM" else "2"
                u = lookup_unit(name)
                from .units import dim_name
                what = dim_name(u.dim).split(" [")[0]
                self.diags.warn(f"'{num} {name}' is the unit {name} (the prefix {p} on the unit {rest}: a "
                                f"{what}), not your {p} times your {rest}", tok=nt,
                                hint=f"write  {num} {p} {rest}  (with a space) for {num} × {p} × {rest}, or  "
                                     f"{num} [{name}]  if you mean the unit")
                return

    def _bracket_all_units(self, j):
        """At the '(' at token j after a unit and '/': does the bracket hold only unit names (and exponents, `*`,
        `/`)?  `3 J/(kg K)` continues the unit; `60 s / (m c_w)` divides (c_w isn't a unit; red team 4 #4).
        A name in it that is your variable is the A1 collision error (D235)."""
        k = self._match(j)
        if k is None:
            return True          # let the unit parser report the missing ')'
        for m in range(j + 1, k):
            tk = self.toks[m]
            if tk.kind == "NAME":
                if not is_unit_name(tk.raw):
                    return False
            elif tk.kind == "OP" and tk.value not in ("^", "/", "*", "(", ")", "-"):
                return False
            elif tk.kind not in ("NAME", "OP", "SUP", "NUM"):
                return False
        return True

    def _no_hour_h(self, t, explicit, start):
        """`36 km/h` and `[km/h]`: h is Planck's constant, not the hour, so dividing a unit by it is almost always
        a mistake for km/hr; it is an error rather than a silent 5×10³⁷ s/(kg m) (red team round 3 #1, D180).
        Spaces don't matter (D235): `2 eV / h` is the same error; `(2 eV)/h` divides by Planck's constant."""
        if not (t.kind == "OP" and t.value == "/"):
            return
        h = self.peek()
        if h.kind != "NAME" or h.raw != "h":
            return
        after = self.peek(2)
        if after.kind == "OP" and after.value == "(" and not after.ws_before:
            return                                          # km/h(x): a call of your function h
        unit = self._unit_text(start)
        num = self._number_before(start) + " " if not explicit else ""
        if explicit:
            raise self.error(f"'{unit}/h' isn't a unit: in Fermium h is Planck's constant, not the hour", tok=h,
                             hint=f"write {unit}/hr for {unit} per hour")
        who = "your h" if "h" in self.known else "Planck's constant"
        e = self.error(f"'{num}{unit}/h': in Fermium h is Planck's constant, not the hour, so this would divide "
                       f"by Planck's constant", tok=h,
                       hint=f"write {num}{unit}/hr for {unit} per hour, or  ({num}{unit})/h  to divide by {who}")
        if self.fix_mode and (t.ws_before or h.ws_before):      # v1 divided when a space was there
            self._add_fix(self._bracket_fix(start, self.toks[self.i - 1]))
            return
        raise e

    def _unit_text(self, start_tok):
        prev = self.toks[self.i - 1]
        # reconstruct from token raws
        parts = []
        for j in range(self.toks.index(start_tok), self.i):
            tk = self.toks[j]
            if parts and tk.ws_before:
                parts.append(" ")
            parts.append(tk.raw)
        del prev
        return "".join(parts)

    def unit_exponent(self):
        if self.tok.kind == "SUP":
            return Fraction(self.next().value)
        if self.at_op("^"):
            self.next()
            neg = 1
            if self.at_op("-"):
                self.next()
                neg = -1
            if self.at_op("("):
                self.next()
                sgn = 1
                if self.at_op("-"):
                    self.next()
                    sgn = -1
                a = self.next()
                if a.kind != "NUM":
                    raise self.error("expected a number in the unit's exponent", tok=a)
                if self.at_op("/"):
                    self.next()
                    b = self.next()
                    if b.kind != "NUM" or b.value == 0 or a.value != int(a.value) or b.value != int(b.value):
                        raise self.error("a unit's exponent must be a fraction of whole numbers, like m^(1/2)",
                                         tok=b)
                    p = Fraction(int(a.value), int(b.value))
                else:
                    p = Fraction(a.value).limit_denominator(1000)
                self.expect_op(")")
                return p * sgn * neg
            a = self.next()
            if a.kind != "NUM":
                raise self.error("expected a number after ^ in this unit", tok=a)
            return Fraction(a.value).limit_denominator(1000) * neg
        return Fraction(1)


def parse(source: str, diags: Diagnostics | None = None, known=None) -> A.Program:
    diags = diags or Diagnostics()
    toks = tokenize(source, diags)
    return Parser(toks, diags, known).parse_program()


def parse_tokens(source: str, diags=None, known=None):
    """Parse and also return the token list (for the formatter)."""
    diags = diags or Diagnostics()
    toks = tokenize(source, diags)
    p = Parser(toks, diags, known)
    return p.parse_program(), toks
