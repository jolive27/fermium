# Bugs found by the test suite

**Status (checked against the suite):** B1–B16 are FIXED (their tests pass, xfail markers
removed). **Open: B17 (`dx/dt` notation), B18 (parser crash on `d^2/dt^* x`).**

Each entry: minimal repro, expected vs actual, and the test that covers it (marked
`xfail(strict=True)` until fixed -- when you fix one, the xfail will turn into XPASS and
fail the suite; just delete the `xfail` marker).

## B1. [FIXED] Arithmetic on absolute °C temperatures gives nonsense
```
print 20 °C - 10 °C      # actual: -263.15 °C   expected: 10 K (a difference), or an error
print 20 °C + 10 °C      # actual: 303.15 °C    expected: an error (can't add two absolute temperatures)
```
D12 says °C values are absolute temperatures and the spec says "°C handled correctly as
affine". The difference of two absolute temperatures is a temperature *difference*, which must
not be displayed as °C (the -273.15 offset gets applied). Tests: `test_dimensions.py::test_celsius_difference_is_not_celsius`,
`test_celsius_plus_celsius_rejected`.

## B2. [FIXED] A non-integer list index is silently accepted
```
xs = [1 m, 2 m]
print xs[1.5]            # actual: 2 m   expected: an error (index must be a whole number)
```
Test: `test_parser.py::test_fractional_index_is_an_error`.

## B3. [FIXED] `plot f(x) vs x from a to b` (documented in reference §11) doesn't work
```
k = 2 N/m
F(x) = k x
plot F(x) vs x from 0 m to 1 m to "F.png"
# actual: line 3: x isn't defined     expected: plots the formula, prints "plot saved to F.png"
```
Test: `test_parser.py::test_plot_formula_from_to`.

## B4. [FIXED] No "a/(b c)" ambiguity warning for `1/2 m v²` when m, v are variables
```
m = 2 kg
v = 3 m/s
E = 1/2 m v²
```
Reference §3 says "`1/2 m v²` means 1/(2 m v²), and Fermium warns you". Line 3 produces no
warning at all (the "m after a number is the unit m" collision warning was already spent on line 2,
and `_check_ambiguous_division` only looks for a plain `Num` at the start of the denominator, not a
`Quantity` like `2 m`). Actual output of `print E` is `0.0555556 s²/m³` silently.
Test: `test_parser.py::test_one_half_m_v_squared_warns`.

## B5. [FIXED] Unit errors caused by a call are reported only at the function definition
```
f(x) = x + 1 m
print f(2 s)
# actual: line 1: can't add time [s] to length [m]
# expected: the error points at (or at least mentions) the call on line 2, which is where the mistake is
```
Same for `x(t) = A cos(ω t)` then `print x(3 m)`: the error is on line 3 "cos needs a plain number,
but got speed [m/s]", with no mention of the call. Test: `test_dimensions.py::test_bad_call_points_at_call_site`.

## B6. [FIXED] Runtime errors carry no line number
```
xs = [1 m, 2 m, 3 m]
print xs[4]
# actual: FermiumRuntimeError with line=None: "index 4 is out of range: ..."
# expected: line 2 (spec Tier 4: one plain line + a caret pointing at the problem)
```
Test: `test_parser.py::test_index_out_of_range_has_line`.

## B7. [FIXED] A huge number literal crashes the lexer (Python OverflowError)
```
x = 1e400
# actual: OverflowError (34, 'Numerical result out of range') from lexer._number (10.0 ** exp)
# expected: a FermiumError ("this number is too large") -- or at least ∞, never a traceback
```
Test: `test_lexer.py::test_huge_exponent_is_clean_error`.

## B8. [FIXED] Multi-digit subscripts don't match their ASCII spelling: `x₁₂` is `x_1_2`, not `x_12`
```
x_12 = 3
print x₁₂        # actual: "x_1_2 isn't defined"   expected: 3
```
D9 says "Subscript digits become _N". Worse, `fermium fmt --pretty` turns `x_12` into `x₁₂`,
so a program that mixes the two spellings changes meaning after formatting (`x_10` in ASCII
and `x₁₀` elsewhere). Tests: `test_lexer.py::test_multi_digit_subscript`,
`test_fmt.py::test_pretty_multi_digit_subscript_roundtrip`.

## B9. [FIXED] A UTF-8 byte-order mark at the start of a file is an error
```
"﻿x = 3\nprint x"     # actual: unexpected character '﻿' (Zero Width No-Break Space)
```
Windows Notepad and some editors save files with a BOM. Expected: ignore a leading BOM.
Test: `test_lexer.py::test_leading_bom_ignored`.

## B10. [FIXED] Deeply nested / very long expressions crash with RecursionError
```
print ((((...80 levels...(1)...)))       -> RecursionError from run_source
print 1+1+1+...  (1000 terms)            -> RecursionError
```
`fermium run` catches RecursionError and prints a message, but `run_source` (and therefore
the REPL, which prints "internal error in Fermium") does not raise a FermiumError. Expected:
a FermiumError such as "this expression is nested too deeply". A 1000-term sum is plausible in
generated code. Test: `test_parser.py::test_long_sum_no_crash`, `test_deep_nesting_clean_error`.

## B11. [FIXED] A non-numeric rational unit exponent crashes the parser (ValueError)
```
print 3 [m^(1/x)]
# actual: ValueError: invalid literal for int() with base 10: 'x'  (parser.unit_exponent)
# expected: FermiumError "expected a number in the unit's exponent"
```
Test: `test_parser.py::test_bad_unit_exponent_clean_error`.

## B12. [FIXED] `fmt --ascii` glues a spelled-out symbol onto the next name, changing meaning
```
f = 3
ω = 2πf          # fmt --ascii -> "omega = 2pif"   ("pif isn't defined")
print ∂/∂x g     # fmt --ascii -> "print partial/partialx g"   (doesn't parse)
```
π and ∂ are single-character tokens, so `πf` / `∂x` need a space when spelled `pi` / `partial`.
Expected: `2pi f`/`2 pi f` and `partial/partial x`. Test: `test_fmt.py::test_ascii_pi_before_name_keeps_meaning`,
`test_ascii_partial_keeps_meaning`.

## B13. [FIXED] REPL: a block ends as soon as it parses, so if/else and multi-line functions can't be typed
```
fm> if g > 1 m/s²
...     print "big"
fm> else                  <- the REPL already ran the if; 'else' is now an error ("didn't expect 'else'")
```
Same for a function with two body lines:
```
fm> f(x) =
...     y = 2 x          <- the REPL runs "f(x) =\n    y = 2 x" here: "the function f never returns a value"
```
`repl.needs_more` stops reading once the text parses. Expected (like Python): after a line that
opens a block, keep reading `...` lines until an empty line. Tests:
`test_repl.py::test_multiline_function_two_body_lines`, `test_if_else_block`.

## B14. [FIXED] `print "label", x where x = ...` fails with "can't print this"
```
print "E =", E where E = 3 J      # actual: line 1: can't print this   expected: E = 3 J
```
`print_stmt` wraps every item (including the string) in a `Where`, and the checker can't print a
`Where(Str)`. Test: `test_parser.py::test_print_label_with_where`.

## B15. [FIXED] The `%` unit (reference §15: "rad sr ° arcmin arcsec rev %") is rejected by the lexer
```
print 50 %        # actual: unexpected character '%' (Percent Sign)   expected: 50 % (or 0.5)
x = 0.5
print x in %      # expected: 50 %
```
Test: `test_dimensions.py::test_percent_unit`.

## B16. [FIXED] Printed units echo the source spelling, so `fmt` changes a program's output
```
g = 9.81 m/s²
print g in ft/s^2     # prints "32.2 ft/s^2"
print g in ft/s²      # prints "32.2 ft/s²"
print 20 °C in degF   # prints "68 degF"  (vs "68 °F")
G_N = 6.674e-11 N m^2/kg^2 ; print G_N   # "N m^2/kg^2"
```
Spec §3.4.3: the ASCII form "means exactly the same thing" and §3.8: round-tripping `fmt` "must
not change program meaning". Today `fermium fmt --ascii` changes what the program prints.
Expected: the display unit is canonical (pretty: `ft/s²`, `°F`, `m²`) whichever way it was spelled.
Tests: `test_fmt.py::test_unit_display_does_not_depend_on_spelling` and the `*_output` round-trip
tests p00, p08, p12, doc03, a00.

## B17. `dx/dt` derivative notation (spec §3.5) isn't supported
```
x(t) = 3 m/s * t
v = dx/dt
print v(1 s)        # actual: line 2: dx isn't defined (hint: give it a value first)   expected: 3 m/s
```
Spec §3.5: "Derivatives: symbolic ..., via `x'`, `x''`, `d/dt x`, `dx/dt`". (When the user has
variables called `dx` and `dt`, `dx/dt` must stay a division -- that works today and is tested.)
At minimum the error should suggest `d/dt x` or `x'`. Test: `test_parser.py::test_dx_dt_notation`.

## B18. Malformed `d/dt^…` crashes the parser (ValueError)
```
x(t) = t
print d^2/dt^* x      # ValueError: invalid literal for int() with base 10: '*'
print d/dt ^ * x      # same
```
`deriv_op` does `int(self.next().value)` after `^` without checking the token is a number.
Expected: a FermiumError. Found by fuzzing. Test: `test_parser.py::test_malformed_derivative_order`.
