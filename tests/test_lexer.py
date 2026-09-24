"""Lexer tests: numbers, symbols, names, look-alikes, strings, indentation, errors."""
import pytest

from conftest import run, error_of
from fermium.errors import Diagnostics, FermiumError
from fermium.lexer import tokenize


def toks(src, diags=None):
    """(kind, value) pairs, without the trailing NEWLINE/EOF."""
    out = [(t.kind, t.value) for t in tokenize(src, diags or Diagnostics())]
    while out and out[-1][0] in ("NEWLINE", "EOF", "DEDENT"):
        out.pop()
    return out


def num(src):
    ts = tokenize(src, Diagnostics())
    assert ts[0].kind == "NUM", ts
    return ts[0]


def lex_warnings(src):
    d = Diagnostics()
    tokenize(src, d)
    return [w.message for w in d.warnings]


# ------------------------------------------------------------------ numbers
@pytest.mark.parametrize("src,value,sf", [
    ("3", 3.0, None),
    ("100", 100.0, None),
    ("1_000", 1000.0, None),
    ("3.0", 3.0, 2),
    ("1.20", 1.2, 3),
    ("0.0012", 0.0012, 2),
    ("9.81", 9.81, 3),
    ("1.5e-3", 1.5e-3, 2),
    ("1.0e5", 1e5, 2),
    ("2E3", 2000.0, None),
    ("6.674e-11", 6.674e-11, 4),
    ("6.67×10⁻¹¹", 6.67e-11, 3),
    ("3.00×10⁸", 3.0e8, 3),
    ("6.67×10^-11", 6.67e-11, 3),
    (".5", 0.5, 1),
])
def test_number_values_and_sigfigs(src, value, sf):
    t = num(src)
    assert t.value == pytest.approx(value, rel=1e-15)
    assert t.sigfigs == sf
    assert t.digit is True


@pytest.mark.parametrize("src,value", [("½", 0.5), ("¼", 0.25), ("¾", 0.75), ("⅓", 1 / 3)])
def test_vulgar_fractions(src, value):
    t = num(src)
    assert t.value == pytest.approx(value)
    assert t.digit is False  # units never follow ½ (D7)
    assert t.sigfigs is None


def test_sigfigs_drive_printing():
    assert run("print 1.20 m") == "1.20 m"
    assert run("x = 1.20 m\ny = 2.0 m\nprint x + y") == "3.2 m"
    assert run("print 2 * 3") == "6"


def test_scientific_notation_prints():
    assert run("x = 6.67×10⁻¹¹\nprint x") == "6.67×10⁻¹¹"
    assert run("print 3.00×10⁸ m/s") == "3.00×10⁸ m/s"
    assert run("print ½") == "0.5"


def test_huge_exponent_is_clean_error():
    # must be a FermiumError (or at least not a Python crash): see bugs-tests B7
    try:
        out = run("x = 1e400\nprint x")
    except FermiumError:
        return
    assert "∞" in out or "inf" in out.lower()


# ------------------------------------------------------------------ superscripts & primes
def test_superscripts():
    assert toks("x² y³ z⁻¹") == [("NAME", "x"), ("SUP", 2), ("NAME", "y"), ("SUP", 3),
                                 ("NAME", "z"), ("SUP", -1)]


def test_superscript_equals_caret():
    assert run("x = 3\nprint x², x^2") == "9 9"
    assert run("print 2 m⁻¹ == 2 m^-1") == "true"


def test_bad_superscript_is_error():
    e = error_of("x = 2\nprint x⁻")
    assert "superscript" in e.message
    assert e.line == 2


@pytest.mark.parametrize("src,n", [("x'", 1), ("x''", 2), ("x′", 1), ("x″", 2), ("x'''", 3)])
def test_primes(src, n):
    assert toks(src) == [("NAME", "x"), ("PRIME", n)]


# ------------------------------------------------------------------ names
@pytest.mark.parametrize("a,b", [
    ("ε₀", "ε_0"), ("ε₀", "epsilon_0"), ("theta", "θ"), ("omega_0", "ω₀"), ("omega_0", "ω_0"),
    ("hbar", "ħ"), ("inf", "∞"), ("infinity", "∞"), ("lambda", "λ"), ("Delta", "Δ"),
    ("alpha_1", "α₁"), ("mu_0", "μ₀"), ("x₁", "x_1"),
])
def test_equivalent_spellings(a, b):
    assert toks(a) == toks(b)


def test_multi_digit_subscript():
    assert toks("x₁₂") == toks("x_12")
    assert run("x_12 = 3\nprint x₁₂") == "3"


def test_greek_ascii_names_are_same_variable():
    assert run("ε₀ = 3\nprint ε_0") == "3"
    assert run("theta = 2\nprint θ") == "2"
    assert run("omega_0 = 5\nprint ω₀") == "5"


def test_pi_is_standalone_token():
    assert toks("2πf") == [("NUM", 2.0), ("NAME", "π"), ("NAME", "f")]
    assert run("f = 3\nprint 2πf == 2*pi*f") == "true"


def test_other_greek_letters_join_names():
    assert toks("ωt") == [("NAME", "ωt")]


def test_greek_word_inside_name_is_not_converted():
    # segments are converted, not substrings: 'alphabet' stays a name
    assert toks("alphabet") == [("NAME", "alphabet")]
    assert toks("Delta_E") == [("NAME", "Δ_E")]


def test_keywords_and_aliases():
    assert toks("∫") == [("KW", "integral")]
    assert toks("integral") == [("KW", "integral")]
    assert toks("√") == toks("sqrt") == [("KW", "sqrt")]
    assert toks("∂") == toks("partial")


def test_times_sign_multiplies_numbers():
    # × is its own token (it is the cross product for vectors) but multiplies plain numbers like *
    assert run("print 2 × 3 m") == run("print 2 * 3 m") == "6 m"


@pytest.mark.parametrize("sym,ascii", [("·", "*"), ("≤", "<="), ("≥", ">="),
                                       ("≠", "!="), ("≈", "~="), ("±", "+-")])
def test_operator_spellings(sym, ascii):
    assert toks(f"a {sym} b") == toks(f"a {ascii} b")


# ------------------------------------------------------------------ look-alikes
def test_micro_sign_normalized():
    assert toks("µ") == [("NAME", "μ")]          # U+00B5 -> U+03BC
    assert run("µ = 3\nprint μ") == "3"
    assert run("print 1 µm == 1 μm") == "true"
    assert lex_warnings("µ = 3") == []            # same letter: silent


@pytest.mark.parametrize("ch,to", [("Ω", "Ω"), ("K", "K"), ("Å", "Å"), ("ℏ", "ħ"), ("ϵ", "ε")])
def test_same_letter_normalized(ch, to):
    assert toks(ch) == toks(to)


def test_minus_sign_normalized():
    assert run("print 5 − 3") == "2"


def test_v_and_nu_warns():
    w = lex_warnings("v = 1\nν = 2\nprint v + ν")
    assert any("look almost identical" in m for m in w)
    assert run("v = 1\nν = 2\nprint v + ν") == "3"   # still different variables


def test_no_lookalike_warning_for_one_spelling():
    assert lex_warnings("ν = 2\nprint ν") == []


def test_cyrillic_a_replaced_with_warning():
    src = "а = 3\nprint a"    # Cyrillic а
    assert run(src) == "3"
    w = lex_warnings(src)
    assert len(w) >= 1 and "look-alike" in w[0] and "Cyrillic" in w[0]


def test_greek_capital_alpha_replaced():
    w = lex_warnings("Α = 1")
    assert any("look-alike" in m for m in w)
    assert toks("Α") == [("NAME", "A")]


# ------------------------------------------------------------------ strings
def test_string():
    assert toks('"hi there"') == [("STR", "hi there")]
    assert run('print "hello", 3') == "hello 3"


def test_smart_quotes():
    assert run("print “hello”") == "hello"


def test_unterminated_string():
    e = error_of('print "oops\nprint 1')
    assert "closing quote" in e.message
    assert e.line == 1


# ------------------------------------------------------------------ comments, indentation
def test_comments_ignored():
    assert run("# a comment\nx = 2  # trailing\nprint x   # more") == "2"


def test_indent_dedent():
    kinds = [k for k, _ in toks("if x\n    y\n        z\nw")]
    assert kinds == ["KW", "NAME", "NEWLINE", "INDENT", "NAME", "NEWLINE", "INDENT", "NAME",
                     "NEWLINE", "DEDENT", "DEDENT", "NAME"]


def test_blank_and_comment_lines_do_not_change_indentation():
    src = "if 1 > 0\n    x = 1\n\n# note\n    print x\n"
    assert run(src) == "1"


def test_inconsistent_dedent_is_error():
    e = error_of("if 1 > 0\n    print 1\n  print 2")
    assert "indentation" in e.message
    assert e.line == 3


def test_unexpected_indent_is_error():
    e = error_of("x = 1\n    print x")
    assert "indented" in e.message
    assert e.line == 2


def test_tabs_count_as_indentation():
    assert run("if 1 > 0\n\tprint 1") == "1"


# ------------------------------------------------------------------ line continuation
@pytest.mark.parametrize("op", ["+", "-", "*", "/", ","])
def test_continuation_after_trailing_operator(op):
    src = f"x = [4 {op}\n   2]" if op == "," else f"x = 4 {op}\n    2\nprint x"
    if op == ",":
        assert run(src + "\nprint x") == "[4, 2]"
    else:
        expected = {"+": "6", "-": "2", "*": "8", "/": "2"}[op]
        assert run(src) == expected


def test_continuation_inside_parentheses():
    assert run("x = (1\n  + 2)\nprint x") == "3"


def test_continuation_with_backslash():
    assert run("x = 1 \\\n  + 2\nprint x") == "3"


def test_newline_without_operator_ends_statement():
    assert toks("x = 1\n+ 2").count(("NEWLINE", "\n")) == 1


# ------------------------------------------------------------------ errors
@pytest.mark.parametrize("ch", ["$", "@", "‽", "`"])
def test_unknown_character_error(ch):
    e = error_of(f"x = 3\ny = x {ch} 2")
    assert "unexpected character" in e.message and ch in e.message
    assert e.line == 2
    assert e.col == 7


def test_unknown_character_error_is_fermium_error_from_lexer():
    with pytest.raises(FermiumError):
        tokenize("x = 3 $ 4", Diagnostics())


def test_leading_bom_ignored():
    assert run("﻿x = 3\nprint x") == "3"


def test_crlf_line_endings():
    assert run("x = 3\r\nprint x\r\n") == "3"


@pytest.mark.parametrize("latin,greek", [("v", "ν"), ("o", "ο"), ("p", "ρ"), ("k", "κ")])
def test_lookalike_pairs_warn_but_stay_different(latin, greek):
    src = f"{latin} = 1\n{greek} = 2\nprint {latin}, {greek}"
    assert any("look almost identical" in m for m in lex_warnings(src))
    assert run(src) == "1 2"
