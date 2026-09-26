"""`fermium fmt --pretty / --ascii`: convert between plain-ASCII and symbol spellings.

Works token by token, copying everything between tokens (spaces, comments)
unchanged, so only spellings change -- never meaning.  Round-trips are tested.
"""
from __future__ import annotations

from .errors import Diagnostics, FermiumError
from .lexer import GREEK, GREEK_TO_ASCII, SUPERS, VULGAR_ASCII
from .parser import parse_tokens
from .units import UNIT_PRETTY, UNIT_ASCII, lookup_unit

SUP_OF = {v: k for k, v in SUPERS.items()}
SUB_DIGITS = str.maketrans("0123456789", "₀₁₂₃₄₅₆₇₈₉")

KW_PRETTY = {"sqrt": "√", "cbrt": "∛", "integral": "∫", "partial": "∂"}
KW_ASCII = {v: k for k, v in KW_PRETTY.items()}
OP_PRETTY = {"*": "·", "<=": "≤", ">=": "≥", "!=": "≠", "+-": "±", "~=": "≈"}
OP_ASCII = {"·": "*", "≤": "<=", "≥": ">=", "≠": "!=", "±": "+-", "≈": "~=", "−": "-", "÷": "/"}


def _wordy(ch):
    return ch.isascii() and (ch.isalnum() or ch == "_") or ch.isalpha()


def _to_sup(n: int) -> str:
    return "".join(SUP_OF[c] for c in str(n))


def _ident_pretty(value: str) -> str:
    parts = value.split("_")
    out = []
    for i, p in enumerate(parts):
        p = GREEK.get(p, p)
        if i > 0 and p.isdigit() and p.isascii():
            out[-1] = out[-1] + p.translate(SUB_DIGITS)
            continue
        out.append(p)
    return "_".join(out)


def _ident_ascii(value: str):
    parts = value.split("_")
    out = []
    ok = True
    for p in parts:
        p2 = GREEK_TO_ASCII.get(p, p)
        if p2 == "∞":
            p2 = "inf"
        if not p2.isascii():
            ok = False
        out.append(p2)
    return "_".join(out), ok


def _unit_pretty(raw: str) -> str:
    if raw in UNIT_PRETTY:
        return UNIT_PRETTY[raw]
    if raw.startswith("u") and len(raw) > 1 and lookup_unit("μ" + raw[1:]) is not None and raw not in (
            "u",) and lookup_unit(raw) is not None and lookup_unit(raw).factor == lookup_unit("μ" + raw[1:]).factor:
        return "μ" + raw[1:]
    return raw


def _unit_ascii(raw: str) -> str:
    if raw in UNIT_ASCII:
        return UNIT_ASCII[raw]
    if raw.startswith(("μ", "µ")):
        return "u" + raw[1:]
    return raw


def format_source(source: str, mode: str, diags: Diagnostics | None = None) -> str:
    """mode: 'pretty' (ASCII -> symbols) or 'ascii' (symbols -> ASCII)."""
    diags = diags or Diagnostics()
    try:
        _, toks = parse_tokens(source, diags)
    except FermiumError as e:
        if "uncertainties" not in e.message:
            raise
        from .lexer import tokenize        # still format a file that uses the reserved ±
        toks = tokenize(source, diags)
    src = toks[0].extra.get("src") if toks and toks[0].extra else None
    from .lexer import Lexer
    norm = Lexer(source).src if src is None else src
    out = []
    pos = 0
    closers = {}   # token index -> text to add after it
    last_changed = False
    skip = set()
    i = 0
    n = len(toks)
    while i < n:
        t = toks[i]
        if t.kind in ("NEWLINE", "INDENT", "DEDENT", "EOF"):
            i += 1
            continue
        gap = norm[pos:t.start]
        if mode == "pretty" and gap == " " and t.kind == "NAME" and i >= 2 and toks[i - 1].raw == "partial" \
                and toks[i - 2].raw == "/":
            gap = ""                # partial^2/partial x^2 -> ∂²/∂x², not ∂²/∂ x²
        out.append(gap)
        pos = t.end
        if i in skip:
            i += 1
            continue
        text = t.raw
        if mode == "pretty" and i >= 1 and toks[i - 1].kind == "OP" and toks[i - 1].raw == "." and not t.ws_before:
            text = t.raw            # np.sqrt, sp.gamma: a Python name after '.' keeps its spelling (D140)
        elif mode == "pretty":
            text = _pretty_token(toks, i, skip)
        elif t.kind == "KW" and t.value == "nabla":
            text = _nabla_ascii(toks, i, skip)
        elif t.kind == "NAME" and t.value == "𝑖" and _imag_literal_ok(toks, i):
            out[-1] = ""                # 2𝑖 -> 2i, not 2 1i (red team 5 nit)
            text = "i"
        else:
            text = _ascii_token(toks, i, closers, diags)
        # never glue two words together that were separate tokens (2πf -> "2 pi f", not "2pif")
        prev = "".join(out)[-1:] if out else ""
        if not gap and prev and text and (text != t.raw or last_changed) and _wordy(prev) and _wordy(text[0]) \
                and not (prev.isdigit() and not text[0].isdigit()):
            out.append(" ")
        last_changed = text != t.raw
        out.append(text)
        if i in closers:
            out.append(closers.pop(i))
        i += 1
    out.append(norm[pos:])
    return "".join(out)


def _pretty_token(toks, i, skip):
    t = toks[i]
    if t.kind == "NAME":
        if t.role == "unit":
            return _unit_pretty(t.raw)
        if t.value == "∞" and t.raw in ("inf", "infinity"):
            return "∞"
        return _ident_pretty(t.value) if t.raw.isascii() else t.raw
    if t.kind == "KW":
        return KW_PRETTY.get(t.raw, t.raw)
    if t.kind == "OP":
        if t.raw == "^":
            # x^2 -> x², x^-1 -> x⁻¹ (integer exponents only)
            j = i + 1
            sign = ""
            if toks[j].kind == "OP" and toks[j].raw == "-":
                sign = "-"
                j += 1
            nt = toks[j]
            after = toks[j + 1] if j + 1 < len(toks) else None
            if nt.kind == "NUM" and nt.raw.isdigit() and not (after is not None and after.kind == "OP" and
                                                              after.raw in ("^",)) and \
                    not (after is not None and after.kind == "NUM" and not after.ws_before):
                for k in range(i + 1, j + 1):
                    skip.add(k)
                return _to_sup(int(sign + nt.raw))
            return t.raw
        return OP_PRETTY.get(t.raw, t.raw)
    return t.raw


def _nabla_ascii(toks, i, skip):
    """∇f, ∇·F, ∇×F, ∇²f -> grad(f), div(F), curl(F), laplacian(f)."""
    j = i + 1
    word = "grad"
    t = toks[j]
    if t.kind == "SUP" and t.value == 2:
        word, j = "laplacian", j + 1
    elif t.kind == "OP" and t.raw == "^" and toks[j + 1].kind == "NUM" and toks[j + 1].value == 2:
        word, j = "laplacian", j + 2
    elif t.kind == "OP" and t.value == "*":
        word, j = "div", j + 1
    elif t.kind == "OP" and t.raw == "×":
        word, j = "curl", j + 1
    if toks[j].kind != "NAME":
        return "nabla"
    skip.update(range(i + 1, j + 1))
    name, ok = (toks[j].raw, True) if toks[j].raw.isascii() else _ident_ascii(toks[j].value)
    return f"{word}({name if ok else toks[j].raw})"


def _imag_literal_ok(toks, i):
    """Can `2𝑖` be written as the literal `2i`?  Only after a plain number (not an exponent: 2^3𝑖 is (2³)𝑖, but
    2^3i would be 2^(3i)), with nothing that could read as a unit after it."""
    if i < 1 or toks[i - 1].kind != "NUM" or not all(c.isdigit() or c == "." for c in toks[i - 1].raw):
        return False
    if i >= 2 and toks[i - 2].kind == "OP" and toks[i - 2].raw in ("^", "**"):
        return False
    nxt = toks[i + 1] if i + 1 < len(toks) else None
    return not (nxt is not None and nxt.kind in ("NAME", "NUM"))


def _ascii_token(toks, i, closers, diags):
    t = toks[i]
    if t.kind == "NAME":
        if t.role == "unit":
            return _unit_ascii(t.raw)
        if t.raw.isascii():
            return t.raw
        if t.value == "∞":
            return "inf"
        if t.value == "𝑖":           # the imaginary unit: 1i in ASCII (D90); (1i) before a name, which
            nxt = toks[i + 1] if i + 1 < len(toks) else None       # could otherwise read as a unit (1i hbar)
            return "(1i)" if nxt is not None and nxt.kind == "NAME" else "1i"
        text, ok = _ident_ascii(t.value)
        if not ok:
            diags.warn(f"'{t.raw}' has no plain-ASCII spelling, so it was left as is", tok=t,
                       hint="rename it (e.g. ΔE -> Delta_E) if you need pure ASCII")
            return t.raw
        return text
    if t.kind == "KW":
        if t.raw in ("√", "∛"):
            word = KW_ASCII[t.raw]
            end = t.extra.get("operand_end")
            nxt = toks[i + 1]
            if nxt.kind == "OP" and nxt.raw == "(" and t.extra.get("operand_paren"):
                return word
            if end is not None:
                closers[end] = closers.get(end, "") + ")"
                return word + "("
            return word
        return KW_ASCII.get(t.raw, t.raw)
    if t.kind == "OP":
        if t.raw == "×":
            diags.warn("× (cross product) has no ASCII operator, so it was left as is", tok=t,
                       hint="write cross(a, b) if you need pure ASCII")
        if t.raw == "ᵀ":
            diags.warn("ᵀ (transpose) has no ASCII operator, so it was left as is", tok=t,
                       hint="write transpose(M) if you need pure ASCII")
        return OP_ASCII.get(t.raw, t.raw)
    if t.kind == "SUP":
        v = t.value
        return f"^{v}" if v >= 0 else f"^{v}"
    if t.kind == "NUM":
        raw = t.raw
        if raw in VULGAR_ASCII:
            return VULGAR_ASCII[raw]
        if "×10" in raw:
            mant, _, ex = raw.partition("×10")
            ex = ex.lstrip("^")
            ex = "".join(SUPERS.get(c, c) for c in ex)
            return f"{mant}e{ex}"
        return raw
    return t.raw


def fix_source(source: str, rounds: int = 20) -> tuple[str, int]:
    """The fixed source and the number of edits (see fix_source_report)."""
    new, n, _ = fix_source_report(source, rounds)
    return new, n


def fix_source_report(source: str, rounds: int = 20):
    """`fermium fmt --fix`: rewrite every unit/variable collision (the A1 rule, D235) as a bracketed unit that keeps
    what Fermium 1 did there: `x(0) = 0.1 m` next to a mass m becomes `0.1 [m]`, `20 m/s / g` becomes `20 [m/s] / g`
    and `20 m/s/g` becomes `20 [m/s/g]`.  A collision Fermium 1 already stopped on (`0.5 m v²` with a mass m) had
    no meaning to keep, so it is left for you.  Returns (new source, number of edits, the error still left or None)."""
    from .lexer import tokenize, Lexer
    from .parser import Parser
    total = 0
    for _ in range(rounds):
        d = Diagnostics()
        toks = tokenize(source, d)
        p = Parser(toks, d)
        p.fix_mode = True
        err = None
        try:
            p.parse_program()
        except FermiumError as e:
            err = e
        if not p.fixes:
            if err is not None and total == 0:
                raise err
            try:                           # what the fixed program still stops on, if anything
                from .parser import parse as _parse
                _parse(source, Diagnostics())
                left = None
            except FermiumError as e:
                left = e
            return source, total, left
        norm = Lexer(source).src
        text = source if len(norm) == len(source) else norm
        for a, b, rep in sorted(p.fixes, reverse=True):
            text = text[:a] + rep + text[b:]
        source = text
        total += len(p.fixes)
    return source, total, None
