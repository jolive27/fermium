"""Lexer: source text -> tokens.

Handles Unicode/ASCII equivalence (θ == theta), subscripts (ε₀ == ε_0),
superscript exponents (x² == x^2), look-alike characters, significant indentation,
and scientific notation written the physics way (6.67×10⁻¹¹).
"""
from __future__ import annotations

import unicodedata
from dataclasses import dataclass, field

from .errors import FermiumError, Diagnostics

KEYWORDS = {
    "if", "else", "elif", "then", "for", "from", "to", "step", "in", "while", "return",
    "break", "continue", "print", "plot", "vs", "solve", "with", "fit", "load", "and", "or",
    "not", "where", "true", "false", "integral", "partial", "sqrt", "cbrt", "assert",
}
# Words that are spelled differently but mean the same keyword/operator.
KEYWORD_ALIASES = {"∫": "integral", "∂": "partial", "√": "sqrt", "∛": "cbrt"}

GREEK = {
    "alpha": "α", "beta": "β", "gamma": "γ", "delta": "δ", "epsilon": "ε", "zeta": "ζ",
    "eta": "η", "theta": "θ", "iota": "ι", "kappa": "κ", "lambda": "λ", "mu": "μ", "nu": "ν",
    "xi": "ξ", "pi": "π", "rho": "ρ", "sigma": "σ", "tau": "τ", "upsilon": "υ", "phi": "φ",
    "chi": "χ", "psi": "ψ", "omega": "ω",
    "Gamma": "Γ", "Delta": "Δ", "Theta": "Θ", "Lambda": "Λ", "Xi": "Ξ", "Pi": "Π",
    "Sigma": "Σ", "Upsilon": "Υ", "Phi": "Φ", "Psi": "Ψ", "Omega": "Ω",
    "hbar": "ħ", "infinity": "∞", "inf": "∞",
}
GREEK_TO_ASCII = {v: k for k, v in GREEK.items() if k != "inf"}

# Characters that are the same letter written with a different code point.
NORMALIZE_CHARS = {
    "µ": "μ",   # micro sign -> Greek mu
    "Ω": "Ω",   # ohm sign -> Greek Omega
    "K": "K",   # Kelvin sign -> K
    "Å": "Å",   # angstrom sign -> Å
    "ϵ": "ε",   # lunate epsilon
    "ϕ": "φ",   # phi symbol
    "ϑ": "θ",   # theta symbol
    "ℏ": "ħ",   # Planck constant over two pi (U+210F) -> ħ (U+0127)
}
# Characters that look identical to Latin letters; these are replaced (with a warning).
LOOKALIKE_REPLACE = {
    # Greek capitals identical to Latin
    "Α": "A", "Β": "B", "Ε": "E", "Ζ": "Z", "Η": "H", "Ι": "I", "Κ": "K", "Μ": "M", "Ν": "N",
    "Ο": "O", "Ρ": "P", "Τ": "T", "Υ": "Y", "Χ": "X",
    # Cyrillic
    "а": "a", "е": "e", "о": "o", "р": "p", "с": "c", "у": "y", "х": "x", "і": "i",
    "А": "A", "В": "B", "Е": "E", "К": "K", "М": "M", "Н": "H", "О": "O", "Р": "P", "С": "C",
    "Т": "T", "Х": "X",
}
# Letters that *look like* others but are legitimately different in physics (ν vs v).
# Using both spellings in one program produces a warning.
SKELETON = str.maketrans({"ν": "v", "ο": "o", "ρ": "p", "ι": "i", "κ": "k", "χ": "x", "ϰ": "k",
                          "ɡ": "g", "ⅼ": "l", "ı": "i"})

PUNCT_NORMALIZE = {
    "−": "-", "–": "-", "÷": "/", "′": "'", "″": "''", "“": '"', "”": '"',
    "’": "'", "⋅": "·", "∙": "·", "∗": "*",
}

DIGITS = "0123456789"
SUPERS = {"⁰": "0", "¹": "1", "²": "2", "³": "3", "⁴": "4", "⁵": "5", "⁶": "6", "⁷": "7", "⁸": "8",
          "⁹": "9", "⁻": "-", "⁺": "+"}
SUBS = {"₀": "0", "₁": "1", "₂": "2", "₃": "3", "₄": "4", "₅": "5", "₆": "6", "₇": "7", "₈": "8", "₉": "9"}
VULGAR = {"½": 0.5, "⅓": 1 / 3, "⅔": 2 / 3, "¼": 0.25, "¾": 0.75, "⅕": 0.2, "⅙": 1 / 6, "⅛": 0.125}
VULGAR_ASCII = {"½": "(1/2)", "⅓": "(1/3)", "⅔": "(2/3)", "¼": "(1/4)", "¾": "(3/4)", "⅕": "(1/5)",
                "⅙": "(1/6)", "⅛": "(1/8)"}

# multi-char operators, longest first
OPERATORS = ["+-", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "~=",
             "+", "-", "*", "/", "^", "(", ")", "[", "]", "{", "}", ",", "=", "<", ">", ".", ":",
             "·", "×", "≤", "≥", "≠", "±", "≈", "|", ";"]
OP_CANON = {"·": "*", "×": "*", "≤": "<=", "≥": ">=", "≠": "!=", "±": "+-", "≈": "~="}
IDENT_EXTRA = set("_°☉∞'")  # ' handled separately; kept out below
SPECIAL_STANDALONE = {"π", "∞"}


@dataclass
class Token:
    kind: str          # NUM NAME KW STR OP SUP PRIME NEWLINE INDENT DEDENT EOF
    value: object      # canonical value (float / canonical name / operator / keyword)
    raw: str           # exact source text
    line: int
    col: int
    start: int = 0     # offset in (normalized) source
    end: int = 0
    ws_before: bool = False
    sigfigs: int | None = None  # for NUM: significant figures, None = exact
    digit: bool = False         # NUM written with decimal digits (units may follow)
    role: str = ""              # set by the parser (e.g. 'unit'), used by fmt
    extra: dict = field(default_factory=dict)

    def __repr__(self):
        return f"{self.kind}({self.value!r})@{self.line}:{self.col}"


def canonical_name(raw: str) -> str:
    """Canonical spelling of an identifier: Greek names -> letters, subscripts -> _N."""
    s = raw
    parts = s.split("_")
    parts = [GREEK.get(p, p) for p in parts]
    return "_".join(parts)


def _count_sigfigs(mantissa: str):
    if "." not in mantissa:
        return None
    digits = mantissa.replace(".", "").replace("_", "").lstrip("0")
    if not digits:
        return 1
    return len(digits)


def normalize_source(src: str, diags: Diagnostics | None = None) -> str:
    """Replace look-alike characters.  Keeps length identical where possible."""
    out = []
    line = 1
    col = 1
    for ch in src:
        if ch in NORMALIZE_CHARS:
            ch = NORMALIZE_CHARS[ch]
        elif ch in PUNCT_NORMALIZE and len(PUNCT_NORMALIZE[ch]) == 1:
            ch = PUNCT_NORMALIZE[ch]
        elif ch in LOOKALIKE_REPLACE:
            if diags is not None:
                name = unicodedata.name(ch, "?").title()
                diags.warn(f"replaced look-alike character '{ch}' ({name}) with Latin '{LOOKALIKE_REPLACE[ch]}'",
                           line=line, col=col)
            ch = LOOKALIKE_REPLACE[ch]
        out.append(ch)
        if ch == "\n":
            line += 1
            col = 1
        else:
            col += 1
    return "".join(out)


def _is_ident_start(ch):
    return ch.isalpha() or ch in "_°" or ch == "ħ"


def _is_ident_char(ch):
    return ch.isalnum() and ch not in SUPERS and ch not in VULGAR or ch in "_☉" or ch in SUBS


class Lexer:
    def __init__(self, source: str, diags: Diagnostics | None = None):
        self.diags = diags or Diagnostics()
        src = source.replace("\r\n", "\n").replace("\r", "\n")
        src = src.replace("″", "''")
        self.src = normalize_source(src, self.diags)
        self.pos = 0
        self.line = 1
        self.col = 1
        self.tokens: list[Token] = []
        self.paren = 0
        self.indents = [0]

    def error(self, msg, hint=None, length=1):
        return FermiumError(msg, self.line, self.col, length, hint)

    def adv(self, n=1):
        for _ in range(n):
            if self.pos < len(self.src):
                if self.src[self.pos] == "\n":
                    self.line += 1
                    self.col = 1
                else:
                    self.col += 1
                self.pos += 1

    def peek(self, k=0):
        p = self.pos + k
        return self.src[p] if p < len(self.src) else ""

    def add(self, kind, value, start, line, col, ws, **kw):
        t = Token(kind, value, self.src[start:self.pos], line, col, start, self.pos, ws, **kw)
        self.tokens.append(t)
        return t

    def tokenize(self) -> list[Token]:
        at_line_start = True
        ws = False
        while self.pos < len(self.src):
            ch = self.peek()
            if at_line_start:
                at_line_start = False
                # measure indentation
                width = 0
                p = self.pos
                while p < len(self.src) and self.src[p] in " \t":
                    width += 4 if self.src[p] == "\t" else 1
                    p += 1
                rest = self.src[p] if p < len(self.src) else "\n"
                if rest in "\n#" or self.paren > 0:
                    # blank/comment line or inside brackets: no indentation change
                    self.adv(p - self.pos)
                    ws = True
                    continue
                self.adv(p - self.pos)
                if self.tokens and self.tokens[-1].kind != "NEWLINE" and self.tokens[-1].kind != "INDENT":
                    pass
                if width > self.indents[-1]:
                    self.indents.append(width)
                    self.add("INDENT", width, self.pos, self.line, self.col, True)
                else:
                    while width < self.indents[-1]:
                        self.indents.pop()
                        self.add("DEDENT", width, self.pos, self.line, self.col, True)
                    if width != self.indents[-1]:
                        raise self.error("this line's indentation doesn't match any block above it",
                                         hint="line up the start of the line with the lines above it")
                ws = True
                continue
            if ch in " \t":
                self.adv()
                ws = True
                continue
            if ch == "\\" and self.peek(1) == "\n":
                self.adv(2)
                ws = True
                continue
            if ch == "#":
                while self.pos < len(self.src) and self.peek() != "\n":
                    self.adv()
                continue
            if ch == "\n":
                if self.paren == 0 and not self._continues():
                    if self.tokens and self.tokens[-1].kind not in ("NEWLINE",):
                        self.add("NEWLINE", "\n", self.pos, self.line, self.col, ws)
                    self.adv()
                    at_line_start = True
                else:
                    self.adv()
                ws = True
                continue
            line, col, start = self.line, self.col, self.pos
            if ch in DIGITS or (ch == "." and self.peek(1) in DIGITS and self.peek(1) != ''):
                self._number(ws)
            elif ch in VULGAR:
                self.adv()
                self.add("NUM", VULGAR[ch], start, line, col, ws, sigfigs=None, digit=False)
            elif ch == '"':
                self._string(ws)
            elif ch in SUPERS:
                self._superscript(ws)
            elif ch == "'":
                n = 0
                while self.peek() == "'":
                    self.adv()
                    n += 1
                self.add("PRIME", n, start, line, col, ws)
            elif ch in SPECIAL_STANDALONE:
                self.adv()
                self.add("NAME", ch, start, line, col, ws)
            elif ch in KEYWORD_ALIASES:
                self.adv()
                self.add("KW", KEYWORD_ALIASES[ch], start, line, col, ws)
            elif _is_ident_start(ch):
                self._ident(ws)
            else:
                for op in OPERATORS:
                    if self.src.startswith(op, self.pos):
                        self.adv(len(op))
                        canon = OP_CANON.get(op, op)
                        if canon in "([{":
                            self.paren += 1
                        elif canon in ")]}":
                            self.paren = max(0, self.paren - 1)
                        self.add("OP", canon, start, line, col, ws)
                        break
                else:
                    name = unicodedata.name(ch, "unknown character")
                    raise self.error(f"unexpected character '{ch}' ({name.title()})",
                                     hint="remove it, or check the cheat sheet for the symbols Fermium understands")
            ws = False
        if self.tokens and self.tokens[-1].kind not in ("NEWLINE",):
            self.add("NEWLINE", "\n", self.pos, self.line, self.col, True)
        while len(self.indents) > 1:
            self.indents.pop()
            self.add("DEDENT", 0, self.pos, self.line, self.col, True)
        self.add("EOF", None, self.pos, self.line, self.col, True)
        self._check_lookalikes()
        return self.tokens

    def _continues(self):
        """A line ending in a binary operator or comma continues on the next line."""
        if not self.tokens:
            return False
        t = self.tokens[-1]
        return t.kind == "OP" and t.value in ("+", "-", "*", "/", "^", ",", "+-", "==", "<", ">", "<=", ">=", "!=")

    def _number(self, ws):
        line, col, start = self.line, self.col, self.pos
        p = self.pos
        s = self.src
        while p < len(s) and ((s[p] in DIGITS) or (s[p] == "_" and p + 1 < len(s) and (s[p + 1] in DIGITS))):
            p += 1
        if p < len(s) and s[p] == "." and p + 1 < len(s) and (s[p + 1] in DIGITS):
            p += 1
            while p < len(s) and ((s[p] in DIGITS) or s[p] == "_"):
                p += 1
        elif p < len(s) and s[p] == "." and not (p + 1 < len(s) and (s[p + 1].isalpha() or s[p + 1] == ".")):
            p += 1  # "100." trailing point
        mantissa = s[start:p]
        exp = 0
        # exponent: e-11 / E+3
        if p < len(s) and s[p] in "eE":
            q = p + 1
            if q < len(s) and s[q] in "+-":
                q += 1
            if q < len(s) and (s[q] in DIGITS):
                while q < len(s) and (s[q] in DIGITS):
                    q += 1
                exp = int(s[p + 1:q])
                p = q
        # ×10⁻¹¹  or ×10^-11
        elif s.startswith("×10", p) or s.startswith("*10^", p) and False:
            q = p + 3
            if q < len(s) and s[q] in SUPERS:
                r = q
                while r < len(s) and s[r] in SUPERS:
                    r += 1
                exp = int("".join(SUPERS[c] for c in s[q:r]))
                p = r
            elif q < len(s) and s[q] == "^":
                r = q + 1
                if r < len(s) and s[r] in "+-":
                    r += 1
                while r < len(s) and (s[r] in DIGITS):
                    r += 1
                exp = int(s[q + 1:r])
                p = r
        self.adv(p - self.pos)
        clean = mantissa.replace("_", "")
        value = float(clean) * (10.0 ** exp) if exp else float(clean)
        if exp:
            value = float(f"{clean}e{exp}")
        self.add("NUM", value, start, line, col, ws, sigfigs=_count_sigfigs(mantissa), digit=True)

    def _string(self, ws):
        line, col, start = self.line, self.col, self.pos
        self.adv()
        buf = []
        while self.pos < len(self.src) and self.peek() != '"':
            if self.peek() == "\n":
                raise FermiumError("this text (string) is missing its closing quote \"", line, col)
            buf.append(self.peek())
            self.adv()
        if self.pos >= len(self.src):
            raise FermiumError("this text (string) is missing its closing quote \"", line, col)
        self.adv()
        self.add("STR", "".join(buf), start, line, col, ws)

    def _superscript(self, ws):
        line, col, start = self.line, self.col, self.pos
        buf = []
        while self.peek() in SUPERS and self.peek():
            buf.append(SUPERS[self.peek()])
            self.adv()
        text = "".join(buf)
        try:
            val = int(text)
        except ValueError:
            raise FermiumError(f"can't read the superscript exponent '{self.src[start:self.pos]}'", line, col)
        self.add("SUP", val, start, line, col, ws)

    def _ident(self, ws):
        line, col, start = self.line, self.col, self.pos
        if self.peek() == "°":
            self.adv()
            if self.peek() in ("C", "F") and not _is_ident_char(self.peek(1) or " "):
                self.adv()
            self.add("NAME", self.src[start:self.pos], start, line, col, ws)
            return
        while self.pos < len(self.src):
            c = self.peek()
            if c in SPECIAL_STANDALONE:
                if c == "∞" and self.src[self.pos - 1] == "_":   # R_∞
                    self.adv()
                    continue
                break
            if _is_ident_char(c):
                self.adv()
            else:
                break
        raw = self.src[start:self.pos]
        text = "".join("_" + SUBS[c] if c in SUBS else c for c in raw)
        text = text.replace("__", "_")
        if text in KEYWORDS:
            self.add("KW", text, start, line, col, ws)
            return
        self.add("NAME", canonical_name(text), start, line, col, ws)

    def _check_lookalikes(self):
        seen = {}
        for t in self.tokens:
            if t.kind != "NAME":
                continue
            sk = t.value.translate(SKELETON)
            other = seen.setdefault(sk, t.value)
            if other != t.value:
                self.diags.warn(
                    f"'{t.value}' and '{other}' look almost identical but are different names",
                    tok=t, hint="rename one of them so they can't be confused (e.g. nu vs v)")


def tokenize(source: str, diags: Diagnostics | None = None) -> list[Token]:
    return Lexer(source, diags).tokenize()
