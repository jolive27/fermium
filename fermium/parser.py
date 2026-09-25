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

CMP_OPS = {"==", "!=", "<", ">", "<=", ">=", "~="}
AUG_OPS = {"+=", "-=", "*=", "/="}


# Unit names that are also built-in functions: after a number (`15.3 / min`) they are the unit.
UNITS_NAMED_LIKE_BUILTINS = {"min"}

VEC_CALC_WORDS ={"grad": "grad", "div": "div", "curl": "curl", "laplacian": "lap"}


class Parser:
    def __init__(self, tokens: list[Token], diags: Diagnostics | None = None, known=None):
        self.toks = tokens
        self.i = 0
        self.diags = diags or Diagnostics()
        self.known = set(known or ())   # names assigned so far (for unit/variable collisions)
        self.abs_depth = 0
        self.no_juxt_names = set()
        self.warned_units = set()
        self.collisions = {}         # line -> [(number, name)]: `2 L` read as a unit though L is a variable
        self.in_integrand = 0
        self.limit_start = None      # token index where an integral's upper limit starts (FRICTION #8)

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
        prog.unit_collisions = self.collisions
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
                raise self.error("uncertainties (±) are planned for a future version of Fermium",
                                 hint="for now write the value without its uncertainty")
            if t.kind == "OP" and t.value == "=":
                hint = "use == to compare two values; = stores a value in a variable"
            if t.kind == "OP" and t.value in ("+", "-") and self.peek().kind == "OP" and self.peek().value == t.value:
                hint = f"Fermium has no {t.value}{t.value}; write  x {t.value}= 1"
            raise self.error(f"didn't expect '{t.raw}' here", hint=hint)

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
                    if self.at_op(","):
                        raise self.error("the entries of a matrix can't be changed one at a time; build the new "
                                         "matrix instead, like M = M + [[1, 0], [0, 0]]")
                    self.expect_op("]")
                    op = self.next().value
                    val = self.expr_where()
                    s = self.span(A.IndexAssign(name.value, idx, val, op), name)
                    if end_line:
                        self.end_statement()
                    return s
        e = self.expr_where()
        if self.at_op("="):
            raise self._assign_to_non_name(t, e)
        s = self.span(A.ExprStmt(e), t)
        if end_line:
            self.end_statement()
        return s

    def _assign_to_non_name(self, start, lhs):
        """`h² = GM a (1 − e²)`: only a name can be stored to; point to `solve` (FRICTION #28)."""
        parts = []
        for j in range(self.toks.index(start), self.i):
            tk = self.toks[j]
            if parts and tk.ws_before:
                parts.append(" ")
            parts.append(tk.raw)
        text = "".join(parts)
        var = None
        if isinstance(lhs, A.BinOp) and lhs.op == "^" and isinstance(lhs.left, A.Name):
            var = lhs.left.name
        else:
            names = [n.name for n in A.walk(lhs) if isinstance(n, A.Name)]
            unknown = [n for n in names if n not in self.known]
            var = (unknown or names or ["x"])[0]
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
            self._warn_where_units(items, binds)
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
        while True:
            st = self.tok
            y = self.expr_full()
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
                continue
            break
        out = None
        opts = {}
        while self.at_kw("to") or self.at_kw("with"):
            if self.at_kw("to"):
                self.next()
                if self.tok.kind != "STR":
                    raise self.error("expected a file name in quotes after 'to', like \"orbit.png\"")
                out = self.next().value
                continue
            self.next()      # with log y / with log / with title "..."
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
                elif w.kind == "NAME" and w.value == "title":
                    self.next()
                    if self.tok.kind != "STR":
                        raise self.error("expected the title in quotes, like title \"Decay of Ba-137m\"")
                    opts["title"] = self.next().value
                else:
                    raise self.error("plot options are:  with log y,  with log x,  with log,  with points,  with title \"...\"")
                if self.at_op(","):
                    self.next()
                    continue
                break
        p = self.span(A.Plot(series, out), t)
        p.options = opts
        return p

    def equation(self):
        st = self.tok
        lhs = self.expr()
        if not self.at_op("="):
            raise self.error("expected '=' in this equation" + self._found())
        self.next()
        rhs = self.expr()
        return self.span(A.Equation(lhs, rhs), st)

    def solve_stmt(self):
        t = self.next()
        eqs = []
        initial = []
        var = lo = hi = step = None
        method = None
        tol_node = [None]
        until = [None]
        if self.tok.kind != "NEWLINE":
            eqs.append(self.equation())
            while self.at_op(",") or self.at_kw("and"):
                self.next()
                eqs.append(self.equation())

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
                self.no_juxt_names = saved | {"tolerance", "using", "method", "until"}
                lo = self.expr()
                self.expect_kw("to")
                hi = self.expr()
                if self.at_kw("step"):
                    self.next()
                    step = self.expr()
                self.no_juxt_names = saved
                if self.tok.kind == "NAME" and self.tok.value == "tolerance":
                    self.next()
                    tol_node[0] = self.expr()
                if self.tok.kind == "NAME" and self.tok.value in ("using", "method"):
                    self.next()
                    method = self.expect_name("a method name (rk4 or rk45)").value
                if self.tok.kind == "NAME" and self.tok.value == "until" and "until" not in self.known:
                    self.next()              # for t from 0 s to 9 s until y = 0 m  (D39)
                    until[0] = self.equation()
                return True
            return False

        while clause():
            pass
        if self.tok.kind == "NEWLINE" and self.peek().kind == "INDENT":
            self.next()
            self.next()
            while self.tok.kind not in ("DEDENT", "EOF"):
                if not clause():
                    eqs.append(self.equation())
                    while self.at_op(",") or self.at_kw("and"):
                        self.next()
                        eqs.append(self.equation())
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
        if until[0] is not None:
            s.until = until[0]
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
            self._warn_where_units([e], binds)
            e = self.span(A.Where(e, binds), t)
        return e

    def _warn_where_units(self, exprs, binds):
        """`0.5 m v² where m = 2 kg`: the m after 0.5 is metres, not the where-variable."""
        names = {b for b, _ in binds}
        for ex in exprs:
            for n in A.walk(ex):
                if isinstance(n, A.BinOp) and n.implicit and isinstance(n.left, A.Quantity) and \
                        not n.left.bracket and len(n.left.unit.factors) == 1:
                    f = n.left.unit.factors[0]
                    if f.name in names:
                        self._note_collision(n.left, f)
                        self.diags.warn(f"'{f.name}' after the number means the unit {f.name}, not the "
                                        f"{f.name} from 'where'", line=f.line, col=f.col, length=len(f.name),
                                        hint=f"write *{f.name} (e.g. 0.5*{f.name}) or ½ {f.name}")

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
        while self.tok.kind == "OP" and self.tok.value in CMP_OPS:
            ops.append(self.next().value)
            operands.append(self.sum())
        if len(ops) == 1:
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

    def sum(self):
        t = self.tok
        e = self.product()
        while self.tok.kind == "OP" and self.tok.value in ("+", "-", "+-"):
            op = self.next()
            if op.value == "+-":
                raise FermiumError("uncertainties (±) are planned for a future version of Fermium",
                                   op.line, op.col, len(op.raw),
                                   hint="for now write the value without its uncertainty, e.g. 5.0 m")
            r = self.product()
            e = self.span(A.BinOp(op.value, e, r), t)
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

    def _check_ambiguous_division(self, left, right, op):
        """Warn about `1/2 m v²` which Fermium reads as 1/(2 m v²)."""
        if isinstance(right, A.BinOp) and right.implicit and not right.paren:
            first = right
            while isinstance(first, A.BinOp) and first.implicit and not first.paren:
                first = first.left
            if isinstance(first, A.Quantity) and isinstance(first.value, A.Num) and not first.bracket:
                first = first.value
            if isinstance(first, A.Num) and isinstance(left, A.Num) and not left.paren:
                a, b = (f"{v:g}" for v in (left.value, first.value))
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
        if t.kind == "NAME" and is_unit_name(t.raw):
            k = self._match(self.i)
            if k is None:
                return True
            return not any(self.toks[m].kind == "OP" and self.toks[m].value == "," for m in range(self.i, k))
        if t.kind == "NUM" and t.value == 1 and self.toks[j + 1].kind == "OP" and self.toks[j + 1].value in ("/", "]"):
            return True
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

    def juxt(self):
        t = self.tok
        e = self.power()
        while True:
            if self.at_op("[") and self.tok.ws_before and self._bracket_is_unit():
                u = self.bracket_unit()
                e = self.span(A.Quantity(e, u, bracket=True), t)
                continue
            if not self._starts_term():
                break
            self._warn_unit_then_term(e)
            ws, ri = self.tok.ws_before, self.i
            r = self.power()
            if isinstance(r, A.Name) and not isinstance(e, A.Num):
                r.unit_left = e               # `A_d u`: if u isn't defined, the hint suggests A_d * 1 u (#55)
            e = self.span(A.BinOp("*", e, r, implicit=True), t)
            e.juxt_ws, e.juxt_i = ws, ri
        self._warn_bare_unit(e)
        return e

    def _note_collision(self, q, f):
        """Remember `2 L` (a unit after a number, while L is also a variable), so that a unit error on
        the same line can say why (FRICTION #10)."""
        num = f"{q.value.value:g}" if isinstance(q.value, A.Num) else None
        if num is not None:
            lst = self.collisions.setdefault(f.line, [])
            if (num, f.name) not in lst:
                lst.append((num, f.name))

    def _warn_bare_unit(self, e):
        """Spec §3.4.2: a bare unit after a number that is also a variable name gets a warning (once per name)."""
        q = e
        while isinstance(q, A.BinOp) and q.implicit and not q.paren:
            q = q.right
        if isinstance(q, A.Quantity) and not q.bracket and len(q.unit.factors) == 1:
            f = q.unit.factors[0]
            if f.name in self.known and f.name != "c":
                self._note_collision(q, f)
            if f.name in self.known and f.name not in self.warned_units and f.name != "c":
                self.warned_units.add(f.name)
                num = f"{q.value.value:g}" if isinstance(q.value, A.Num) else "2"
                self.diags.warn(f"'{f.name}' right after a number is the unit {f.name}, not your variable {f.name}",
                                line=f.line, col=f.col, length=len(f.name),
                                hint=f"that's fine if you meant the unit; to multiply by your variable write "
                                     f"{num}*{f.name}")

    def _warn_unit_then_term(self, e):
        """`0.5 m v²` with a variable m: the m is metres here -- almost certainly a mistake."""
        q = e
        while isinstance(q, A.BinOp) and q.implicit and not q.paren:
            q = q.right
        if isinstance(q, A.Quantity) and not q.bracket and not q.paren and len(q.unit.factors) == 1:
            f = q.unit.factors[0]
            if f.name in self.known and f.exp == 1:
                self._note_collision(q, f)
                self.diags.warn(
                    f"'{f.name}' after the number means the unit {f.name}, not your "
                    f"variable {f.name}", line=f.line, col=f.col, length=len(f.name),
                    hint=f"to multiply by your variable write {q.value.value:g}*{f.name}"
                         if isinstance(q.value, A.Num) else f"write *{f.name} to multiply by your variable")

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
            if isinstance(base, A.Num) and base.digit and self.tok.kind == "NAME" and is_unit_name(self.tok.raw) \
                    and self.tok.value not in self.known and not self._is_call_like():
                u = self.unit_expr(explicit=False)      # 10⁸ m/s, like 10^8 m/s
                r = self.span(A.Quantity(r, u), t)
            return r
        if self.at_op("^"):
            self.next()
            ex = self.exponent()
            r = self.span(A.BinOp("^", base, ex), t)
            if isinstance(base, A.Num) and base.digit and self.tok.kind == "NAME" and is_unit_name(self.tok.raw) \
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
                base = A.BinOp("*", base, A.Name("𝑖").at(base)).at(base)
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
                        is_unit_name(self.tok.raw) and self.tok.value not in self.known:
                    u = self.unit_expr(explicit=False)          # vec(3, 4) m/s
                    e = self.span(A.Quantity(e, u), t)
            elif self.at_op("[") and not self.tok.ws_before:
                self.next()
                idx = self.expr()
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
            elif self.at_op(".") and self.peek().kind == "NAME" and not self.tok.ws_before:
                self.next()
                name = self.next()
                e = self.span(A.Field(e, name.value), t)
            elif self.tok.kind == "PRIME":
                p = self.next()
                e = self.span(A.Prime(e, p.value), t)
            else:
                return e

    def atom(self):
        t = self.tok
        if t.kind in ("NUM", "IMAG"):
            self.next()
            n = A.Num(t.value, t.sigfigs, t.digit)
            n.line, n.col, n.length = t.line, t.col, len(t.raw)
            if t.kind == "IMAG":             # 4i is 4 × 𝑖 (D90); a unit may follow: 4i Ω
                n = A.BinOp("*", n, A.Name("𝑖").at(n)).at(n)
                n.imag_literal = True
            if t.digit:
                if self.at_op("[") and self._bracket_is_unit():
                    u = self.bracket_unit()
                    return self.span(A.Quantity(n, u, bracket=True), t)
                if self.tok.kind == "NAME" and is_unit_name(self.tok.raw) and not self._is_call_like():
                    u = self.unit_expr(explicit=False)
                    q = self.span(A.Quantity(n, u), t)
                    nx = self.tok
                    if nx.kind == "OP" and nx.value in ("[", "(") and not nx.ws_before and len(u.factors) == 1 \
                            and u.factors[0].name in self.known:
                        nm = u.factors[0].name
                        raise self.error(f"'{t.raw} {nm}' means {t.raw} of the unit {nm} (a unit right after a "
                                         f"number), so it can't be followed by '{nx.value}'", tok=nx,
                                         hint=f"to use your variable write {t.raw}*{nm}{nx.value}...")
                    return q
                if self._unit_reciprocal_follows():
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
            if t.value == "∞" and self.peek().kind == "NAME" and is_unit_name(self.peek().raw) and \
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
                if items and all(isinstance(x, A.ListLit) for x in items) and self.tok.kind == "NAME" and \
                        is_unit_name(self.tok.raw) and self.tok.value not in self.known and not self._is_call_like():
                    u = self.unit_expr(explicit=False)          # [[1, 2], [3, 4]] N/m  (a matrix, D29)
                    lst = self.span(A.Quantity(lst, u), t)
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
                raise self.error("uncertainties (±) are planned for a future version of Fermium")
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

    def _unit_reciprocal_follows(self):
        """`0.1 1/s` or `0.1 /s` right after a number."""
        t = self.tok
        if t.kind == "NUM" and t.value == 1 and t.digit and self.peek().kind == "OP" and self.peek().value == "/":
            nx = self.peek(2)
            return nx.kind == "NAME" and is_unit_name(nx.raw)
        if t.kind == "OP" and t.value == "/" and self.peek().kind == "NAME" and is_unit_name(self.peek().raw) and \
                self.peek().value not in self.known and \
                (not self.peek().ws_before or self.peek().value in UNITS_NAMED_LIKE_BUILTINS) and \
                not (self.peek(2).kind == "OP" and self.peek(2).value == "(" and not self.peek(2).ws_before):
            # `15.3 / min / g`: after a number, `min` can only be the minute (min the function is never
            # divided by), so the spaced form is a unit too (FRICTION #15)
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
        if len(items) not in (2, 3, 4):
            raise self.error(f"a vector needs 2, 3 or 4 components, not {len(items)}", tok=t)
        v = self.span(A.VecLit(items), t)
        if self.tok.kind == "NAME" and is_unit_name(self.tok.raw) and not self._is_call_like():
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
        operand = self.power()
        return self.span(A.Deriv(var, order, operand), t)

    def partial_op(self):
        t = self.next()   # ∂
        order = self._deriv_order()      # ∂²/∂x² or partial^2/partial x^2 (what fmt --ascii writes)
        self.expect_op("/", "(write ∂/∂x f)")
        self.expect_kw("partial", "(write ∂/∂x f)")
        v = self.expect_name("the variable to differentiate by")
        o2 = self._deriv_order()
        if o2 != order and o2 != 1:
            raise self.error(f"the orders don't match: ∂{order}/∂{v.value}{o2}", tok=v)
        operand = self.power()
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
        finally:
            self.known -= new
            self.limit_start = saved_limit
            self.in_integrand -= 1
        integrand, var = self._split_dvar(body)
        if var is None:
            raise self.error("this integral is missing its 'dx' (the variable to integrate over)", tok=t,
                             hint="write e.g.  ∫ F(x) dx from 0 m to 1 m")
        lo = hi = None
        if self.at_kw("from"):
            self.next()
            lo = self.sum()
            self.expect_kw("to")
            saved, self.limit_start = self.limit_start, self.i
            try:
                hi = self.sum()
            finally:
                self.limit_start = saved
            if self.at_op("/") and self.tok.ws_before and self.peek().kind in ("NUM", "NAME"):
                self.diags.warn("the ' / ' after the upper limit divides the whole integral, not the limit",
                                tok=self.tok, hint="to divide the limit, write it without spaces (to L/2) or in "
                                                   "parentheses (to (L / 2))")
        return self.span(A.Integral(integrand, var, lo, hi), t)

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
            exp = self.unit_exponent()
            f = A.UnitFactor(name, exp * sign)
            f.line, f.col, f.length = nt.line, nt.col, len(nt.raw)
            factors.append(f)

        def unit_name_here(k=0):
            tk = self.peek(k) if k else self.tok
            return tk.kind == "NAME" and is_unit_name(tk.raw)

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
        while True:
            t = self.tok
            spaced_var = (not explicit and t.kind == "OP" and t.value == "/" and t.ws_before and
                          self.peek().kind == "NAME" and self.peek().value in self.known)
            if t.kind == "OP" and t.value == "/" and not spaced_var and (unit_name_here(1) or (
                    self.peek().kind == "OP" and self.peek().value == "(" and (
                        explicit or unit_name_here(2)))):
                self.next()
                factor(-1)
            elif t.kind == "OP" and t.value == "/" and explicit and self.peek().kind == "NUM":
                break
            elif t.kind == "OP" and t.value == "*" and unit_name_here(1) and (
                    explicit or self.peek().value not in self.known):
                self.next()
                factor(1)
            elif unit_name_here() and (explicit or (t.value not in self.known and not self._is_call_like())):
                if not explicit and self.peek().kind == "OP" and self.peek().value == "(" and not self.peek().ws_before:
                    break
                factor(1)
            else:
                if not explicit and unit_name_here() and t.value in self.known and t.ws_before:
                    self.diags.warn(
                        f"reading '{t.raw}' as your variable {t.raw}, not the unit {t.raw}",
                        tok=t, hint=f"write [{self._unit_text(start)} {t.raw}] if you meant the unit")
                break
        u = A.UnitExpr(factors, ("1" if reciprocal and self._unit_text(start).startswith("/") else "")
                       + self._unit_text(start))
        u.line, u.col = start.line, start.col
        u.length = max(1, self.toks[self.i - 1].end - start.start)
        return u

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
