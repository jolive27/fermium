# Bugs found by adversarial testing

Each entry: minimal repro, expected, actual. Tests are in `tests/test_adversarial.py`
(marked `xfail(strict=True, reason="BUG A<n>: ...")` until fixed).

## A1. [FIXED] `push` on an aliased list reads freed memory (use-after-free, silent garbage)
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

## A2. [FIXED] Integrals can hang forever (no global evaluation budget in the adaptive quadrature)
`fm_qrec` bisects recursively up to depth 40 on *each* branch with no total cap, so the
worst case is ~2^40 GK15 evaluations. Both of these never finish (killed after 30 s):
```
print ∫ exp(-(x-20)^2) dx from -inf to inf     # expected: 1.77245 (√π)
print ∫ sin(x) dx from 0 to inf                # expected: an error (does not converge)
```
(`exp(-(x-30)^2)` also hangs.) Ctrl-C doesn't help either (bootcamp B15). Expected: a
bounded number of subdivisions (e.g. QUADPACK-style global heap with a limit) and a runtime
error "the integral did not converge" when the limit is hit.

## A3. [PARTLY FIXED: x-20, x-30 and the half-line x-100 case work; x-100 over (-inf, inf) and ±1e6 still give 0] Integrals silently return 0 when the first GK15 samples miss a narrow peak
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

## A4. [FIXED] Divergent / singular integrals silently return a finite garbage number
```
print ∫ 1/x dx from 0 to 1            # actual: 34.7577        expected: error (diverges)
print ∫ 1/x^2 dx from 0 to 1          # actual: 7.68126×10¹⁴  expected: error (diverges)
print ∫ 1/x dx from 1 to inf          # actual: 34.7595        expected: error (diverges)
print ∫ 1/(x-0.3) dx from 0 to 1      # actual: -11            expected: error (the PV would be 0.847)
```
Hitting the depth limit (40) in `fm_qrec` is silently accepted. Expected: a runtime error
such as "the integral doesn't converge (the integrand blows up near x = 0)".

## A5. [FIXED] `solve` fails ("step became too small near t = 0") on smooth ODEs that start at rest
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

## A6. [FIXED] `times(sol)` is in seconds even when `t` is a plain number
```
solve x' = 1 with x(0) = 0 for t from 0 to 1
ts = times(x)
print ts[end] + 1     # actual: error "can't add time [s] to a plain number"   expected: 2
print ts              # prints "[...] s"
```
`x(1 s)` on the same solution is (correctly) rejected as "t is a plain number", so the two
disagree.

## A7. [FIXED] `solve ... step 0` silently returns the initial condition
```
solve x' = 1 with x(0) = 0 for t from 0 to 1 step 0
print x(1)            # actual: 0   expected: an error (the step must be non-zero), like `step -0.1`
```

## A8. [FIXED] (HIGH) Negative °C / °F temperatures are wrong: `-40 °C` is -313.15 K
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

## A9. [FIXED] Printing a subnormal number: Python exception leaks / wrong mantissa
```
print 5e-324      # actual: "Exception ignored on calling ctypes callback ... ZeroDivisionError" and an
                  #         empty line; expected 4.94066×10⁻³²⁴ (or 5×10⁻³²⁴)
print 1e-320      # actual: 10.0198×10⁻³²¹   expected: 9.99989×10⁻³²¹ (or 1×10⁻³²⁰)
```
`units.format_number` computes `10**exp` which underflows to 0.0 for exp ≤ -324 (and
loses precision for subnormals).

## A10. [FIXED] Significant figures stick to the *variable*, not to the value assigned
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

## A11. [FIXED] A function that returns on only some paths silently returns 0 on the others
```
f(x) =
    if x > 0
        return 1 m
print f(-2)       # actual: 0 m   expected: a compile error ("f doesn't return a value when the
                  #                 if is false"), or at least a runtime error
```
Same with `while x < 0` / `return 1 m` and `print f(5)` → `0 m`. (A function with *no*
return at all is already rejected: "the function f never returns a value".)

## A12. Failed `assert` prints the line number twice
```
x = 1
assert x > 2, "x too small"
```
`fermium run` prints `as.fm, line 2: line 2: x too small` (and `line 2: line 2: check failed: x > 2`
without a message). The same doubled prefix appears in `fermium build` executables. Expected
`as.fm, line 2: x too small`.

## A13. `fermium build` executables print "runtime error" for a non-converging integral
The C runtime (`fermium/runtime/aot_rt.c`, `fm_error`) has no case for error kind 9, which
`runtime/core.py` now describes ("this integral doesn't converge ...").
```
print ∫ 1/x dx from 0 to 1
```
`fermium run`: `line 1: this integral doesn't converge: ...`;  built executable: `line 1: runtime error`.

## A14. `fmt --pretty` breaks `3 * 10^8 m/s` (a unit after `10⁸` isn't accepted)
`10^8 m` is 10⁸ metres, but the superscript spelling `10⁸ m` is an error, so `fmt --pretty`
turns a working program into a broken one:
```
print 3 * 10^8 m/s       # works: 3×10⁸ m/s
# fermium fmt --pretty  ->  print 3 · 10⁸ m/s   ->  "line 1: m isn't defined"
print 10⁸ m              # error: m isn't defined     expected 1×10⁸ m (same as 10^8 m)
print 5² m²              # error                       expected 25 m²
print 2⁻¹ m              # error                       expected 0.5 m
```
Either accept a unit after `number superscript` (like after `number ^ number`) or have
`fmt --pretty` leave `^` alone in that position.

## A15. (HIGH) Adaptive `solve` loses all relative accuracy once a solution decays (absolute error floor)
After the A5 fix (absolute floor ≈ max|dy/dt| × span × 1e-3 × rtol) — and to a lesser degree
with the older `1e-3·max|y| so far` floor — a decaying solution is only accurate to an
*absolute* ~1e-12 of its peak, so exponential decay (radioactivity!) is silently wrong:
```
solve x' = -x with x(0) = 1 for t from 0 to 30
print x(30)            # actual: 8.29301×10⁻¹³   expected: 9.35762×10⁻¹⁴ (e⁻³⁰)
solve x' = -x with x(0) = 1 for t from 0 to 60
print x(60)            # actual: 2.27807×10⁻¹¹   expected: 8.75651×10⁻²⁷
λ = 1 1/s
solve N' = -λ N with N(0) = 1e20 for t from 0 s to 50 s
print N(50 s)          # actual: 3.7168×10⁹      expected: 0.0192875 (1e20·e⁻⁵⁰)
solve x'' = -x - 0.2 x' with x(0) = 1, x'(0) = 0 for t from 0 to 200
print x(200)           # actual: -6.88365×10⁻¹⁰  expected: -1.15908×10⁻⁹ (scipy, atol=1e-30)
```
It also made the stiff Van der Pol case worse: `μ = 1000`, `x'' = μ (1 - x^2) x' - x`,
x(0)=2, x'(0)=0, to t=3000 now gives -1.51035 (before: -1.51061; scipy Radau/LSODA: -1.5106069).
A decay printed with 6 significant figures and the wrong value is the worst kind of
answer. Suggestion: keep a *relative* tolerance on each component and only use an absolute
floor while the component is still ≈0 at the start (e.g. floor from |h·f| of the current step,
or floor = rtol·|y| with |y| replaced by |h·y'| when y == 0), or expose `atol`.
(Kept as xfail in the tests: returning a confident `0` for ∫ of a positive function is a
silent wrong answer; at minimum the quadrature could refuse to accept a zero estimate with
zero error on an infinite or very wide interval without sampling further, or warn.)

## A16. A function whose only list use is `v[i] = ...` is treated as scalar
```
f(v) =
    v[1] = 42
    0
xs = [1, 2, 3]
print f(xs), xs      # actual: error "v isn't a list, so you can't set v[...]" (with v = a plain number)
                     # expected: 0 [42, 2, 3]
```
D14's list-parameter detection counts indexing, looping and list functions but not index
*assignment*; adding any read such as `len(v)` makes it work.

## A17. (low, misleading error) `∫ 2 dm ...`, `∫ 1 dV ...`, `∫ 3 dT ...` read `dm`/`dV`/`dT` as units
After a number literal, `dm` is decimetres, `dV` decivolts, `dT` decitesla, `dg` decigrams, so a
constant integrand gives "this integral is missing its 'dx'":
```
print ∫ 2 dm from 0 kg to 1 kg      # expected 2 kg
print ∫ 1 dV from 0 m^3 to 2 m^3    # expected 2 m³
print ∫ 3 dT from 0 K to 2 K        # expected 6 K
```
(`∫ 1 dt` and `∫ 1 dx` work.) Inside `∫ … d<name>`, a `d<name>` token right before `from`
(or the end of the integral) should be the differential.

## A18. Differentiating the length of a vector gives an internal-looking error about `sign`
```
r(t) = <t^2, t^3, 1>
s(t) = |r(t)|
g = s'
print g(1)       # expected 2.88675 (= 10/(2√3))
# actual: line 2: the argument of sign must be a number, but it is a 3-vector ...
```
d|u|/dt is implemented as sign(u)·u' (scalar rule) — for a vector it should be (u·u')/|u|.
The user never wrote `sign`. Related gaps (clear error, but documented operations):
`d/dt (r(t) × a)`, `d/dt r(t).y`, `d/dt unit(...)` all say "can't differentiate this
expression symbolically".

## A19. (low) Vector ODE solutions: `r''(t)` refused, `r[end]` error has no line number
```
solve r'' = -r with r(0) = <1, 0>, r'(0) = <0, 1> for t from 0 to 3
print r''(3)     # error "can't take that many derivatives of the solution r''"
                 # (scalar x''(t) works, and r.x''(3) works) -- expected <0.989992, -0.14112>
print r[end]     # error "r is a vector; use its components, like r.x" -- no "line 2:" prefix,
                 # and the final vector <cos 3, sin 3> would be a natural answer
```
