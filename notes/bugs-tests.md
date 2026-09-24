# Bugs found by the test suite

Each entry: minimal repro, expected vs actual, and the test that covers it (marked
`xfail(strict=True)` until fixed -- when you fix one, the xfail will turn into XPASS and
fail the suite; just delete the `xfail` marker).

## B1. Arithmetic on absolute °C temperatures gives nonsense
```
print 20 °C - 10 °C      # actual: -263.15 °C   expected: 10 K (a difference), or an error
print 20 °C + 10 °C      # actual: 303.15 °C    expected: an error (can't add two absolute temperatures)
```
D12 says °C values are absolute temperatures and the spec says "°C handled correctly as
affine". The difference of two absolute temperatures is a temperature *difference*, which must
not be displayed as °C (the -273.15 offset gets applied). Tests: `test_dimensions.py::test_celsius_difference_is_not_celsius`,
`test_celsius_plus_celsius_rejected`.

## B2. A non-integer list index is silently accepted
```
xs = [1 m, 2 m]
print xs[1.5]            # actual: 2 m   expected: an error (index must be a whole number)
```
Test: `test_parser.py::test_fractional_index_is_an_error`.

## B3. `plot f(x) vs x from a to b` (documented in reference §11) doesn't work
```
k = 2 N/m
F(x) = k x
plot F(x) vs x from 0 m to 1 m to "F.png"
# actual: line 3: x isn't defined     expected: plots the formula, prints "plot saved to F.png"
```
Test: `test_parser.py::test_plot_formula_from_to`.

## B4. No "a/(b c)" ambiguity warning for `1/2 m v²` when m, v are variables
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

## B5. Unit errors caused by a call are reported only at the function definition
```
f(x) = x + 1 m
print f(2 s)
# actual: line 1: can't add time [s] to length [m]
# expected: the error points at (or at least mentions) the call on line 2, which is where the mistake is
```
Same for `x(t) = A cos(ω t)` then `print x(3 m)`: the error is on line 3 "cos needs a plain number,
but got speed [m/s]", with no mention of the call. Test: `test_dimensions.py::test_bad_call_points_at_call_site`.

## B6. Runtime errors carry no line number
```
xs = [1 m, 2 m, 3 m]
print xs[4]
# actual: FermiumRuntimeError with line=None: "index 4 is out of range: ..."
# expected: line 2 (spec Tier 4: one plain line + a caret pointing at the problem)
```
Test: `test_parser.py::test_index_out_of_range_has_line`.
