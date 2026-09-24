# Bugs found by adversarial testing

Each entry: minimal repro, expected, actual. Tests are in `tests/test_adversarial.py`
(marked `xfail(strict=True, reason="BUG A<n>: ...")` until fixed).

## A1. `push` on an aliased list reads freed memory (use-after-free, silent garbage)
Lists are `{data, len, cap}` structs copied by value, so `ys = xs`, passing a list to a
function, or `for x in xs` share `data`; `push` then `realloc`s it and the other copy keeps
the freed pointer.
```
xs = [1, 2, 3]
ys = xs
push(ys, 4)
print xs          # actual: [4.3056×10⁻³¹⁵, 9.23612×10⁻¹⁸⁴, 3]   expected: [1, 2, 3] (or [1, 2, 3, 4])
```
```
a = [1.0, 2.0]
f(v) =
    push(v, 3.0)
    push(v, 4.0)
    push(v, 5.0)
    v[1]
print f(a)
print a           # actual: [3.6×10⁻³¹⁵, -2.8×10⁻¹²²]   expected: [1.0, 2.0] (or the pushed list)
```
```
xs = [1, 2, 3]
for x in xs
    push(xs, x)
print xs          # actual: [1, 2, 3, 1, -2.79078×10⁻¹²², 3]   expected: [1, 2, 3, 1, 2, 3]
```
Also inconsistent semantics: `ys[1] = 100` through an alias *does* change `xs`, but `push`
through an alias changes only one length. Pick value or reference semantics and make push
consistent (e.g. copy on assignment/call, or a heap list header).

## A2. Integrals can hang forever (no global evaluation budget in the adaptive quadrature)
`fm_qrec` bisects recursively up to depth 40 on *each* branch with no total cap, so the
worst case is ~2^40 GK15 evaluations. Both of these never finish (killed after 30 s):
```
print ∫ exp(-(x-20)^2) dx from -inf to inf     # expected: 1.77245 (√π)
print ∫ sin(x) dx from 0 to inf                # expected: an error (does not converge)
```
(`exp(-(x-30)^2)` also hangs.) Ctrl-C doesn't help either (bootcamp B15). Expected: a
bounded number of subdivisions (e.g. QUADPACK-style global heap with a limit) and a runtime
error "the integral did not converge" when the limit is hit.

## A3. Integrals silently return 0 when the first GK15 samples miss a narrow peak
```
print ∫ exp(-(x-100)^2) dx from -inf to inf    # actual: 0   expected: 1.77245
print ∫ exp(-(x-100)^2) dx from 0 to inf       # actual: 0   expected: 1.77245
print ∫ exp(-x^2) dx from -1e6 to 1e6          # actual: 0   expected: 1.77245
```
The whole-interval estimate is 0 with error estimate 0, so `err <= tol` accepts it at once.
(Physics case: a Gaussian wave packet centred at x₀ = 100 integrated over all space.)
Suggestion: always subdivide a few levels (or start from several panels) before trusting a
zero error estimate, and never accept `err == 0 && res == 0` on the first panel for infinite
ranges.

## A4. Divergent / singular integrals silently return a finite garbage number
```
print ∫ 1/x dx from 0 to 1            # actual: 34.7577        expected: error (diverges)
print ∫ 1/x^2 dx from 0 to 1          # actual: 7.68126×10¹⁴  expected: error (diverges)
print ∫ 1/x dx from 1 to inf          # actual: 34.7595        expected: error (diverges)
print ∫ 1/(x-0.3) dx from 0 to 1      # actual: -11            expected: error (the PV would be 0.847)
```
Hitting the depth limit (40) in `fm_qrec` is silently accepted. Expected: a runtime error
such as "the integral doesn't converge (the integrand blows up near x = 0)".

## A5. `solve` fails ("step became too small near t = 0") on smooth ODEs that start at rest
The scale-free error norm `sc = rtol·(max(|y|,|y_new|) + 1e-3·max|y| so far)` is 0 for a
component that is still exactly 0 at the first steps, so any non-zero error estimate is
rejected until h underflows.
```
solve x' = t^4 with x(0) = 0 for t from 0 to 1
print x(1)                  # expected 0.2; actual: runtime error "step became too small near t = 0"
```
```
solve x'' = -x + sin(t)^3 with x(0) = 0, x'(0) = 0 for t from 0 to 3
print x(3)                  # expected 1.16631 (scipy); actual: same error
```
```
F0 = 1 N
m = 1 kg
ω = 1 1/s
solve m x'' = F0 (ω t)^4 with x(0) = 0 m, x'(0) = 0 m/s for t from 0 s to 1 s
```
(also `z' = t - sin(t)`, and a driven oscillator with `z' = x y` added.) A driven system
starting from rest at the origin is extremely common. With `x(0) = 1e-30` it works.
Suggestion: floor the scale with something like the step's own |h·f| or a tiny absolute
tolerance per component (e.g. relative to the largest |y| of *any* component with the same
units, or 1e-3·|Δy| accumulated).

## A6. `times(sol)` is in seconds even when `t` is a plain number
```
solve x' = 1 with x(0) = 0 for t from 0 to 1
ts = times(x)
print ts[end] + 1     # actual: error "can't add time [s] to a plain number"   expected: 2
print ts              # prints "[...] s"
```
`x(1 s)` on the same solution is (correctly) rejected as "t is a plain number", so the two
disagree.

## A7. `solve ... step 0` silently returns the initial condition
```
solve x' = 1 with x(0) = 0 for t from 0 to 1 step 0
print x(1)            # actual: 0   expected: an error (the step must be non-zero), like `step -0.1`
```

## A8. (HIGH) Negative °C / °F temperatures are wrong: `-40 °C` is -313.15 K
Unary minus is applied to the absolute value in kelvin, i.e. `-(40 °C)` = −313.15 K.
```
T = -40 °C
print T in K, T in °F, T      # actual: -313.15 K -1023.34 °F -586.3 °C
                              # expected: 233.15 K -40 °F -40 °C
print -40 °F in °C            # actual: -550.744 °C   expected: -40 °C
T = [-10 °C, 10 °C]
print T in K                  # actual: [-283.15, 283.15] K   expected: [263.15, 283.15] K
```
A negative literal directly followed by °C/°F must be converted as (−40 + 273.15) K. (`0 - 10 °C`
is already rejected, which is fine.) Any winter/cryogenic program is silently wrong.

## A9. Printing a subnormal number: Python exception leaks / wrong mantissa
```
print 5e-324      # actual: "Exception ignored on calling ctypes callback ... ZeroDivisionError" and an
                  #         empty line; expected 4.94066×10⁻³²⁴ (or 5×10⁻³²⁴)
print 1e-320      # actual: 10.0198×10⁻³²¹   expected: 9.99989×10⁻³²¹ (or 1×10⁻³²⁰)
```
`units.format_number` computes `10**exp` which underflows to 0.0 for exp ≤ -324 (and
loses precision for subnormals).

## A10. Significant figures stick to the *variable*, not to the value assigned
```
x = 1.20 m
x = 2.123456 m
print x           # actual: 2.12 m    expected: 2.123456 m (the literal as written)
x = 1.20 m
x = 2 m
print x           # actual: 2.00 m    expected: 2 m
```
The REPL shows the same (`x = 1.20 m` then `x = 2 m` prints `2.00 m`). Reassignment silently
throws away precision the user typed.

## A11. A function that returns on only some paths silently returns 0 on the others
```
f(x) =
    if x > 0
        return 1 m
print f(-2)       # actual: 0 m   expected: a compile error ("f doesn't return a value when the
                  #                 if is false"), or at least a runtime error
```
Same with `while x < 0` / `return 1 m` and `print f(5)` → `0 m`. (A function with *no*
return at all is already rejected: "the function f never returns a value".)
