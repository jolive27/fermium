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

## A3. [FIXED for infinite ranges; documented limitation: a width-1 peak in a finite ±1e6 range can still give 0] Integrals silently return 0 when the first GK15 samples miss a narrow peak
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

## A12. [FIXED] Failed `assert` prints the line number twice
```
x = 1
assert x > 2, "x too small"
```
`fermium run` prints `as.fm, line 2: line 2: x too small` (and `line 2: line 2: check failed: x > 2`
without a message). The same doubled prefix appears in `fermium build` executables. Expected
`as.fm, line 2: x too small`.

## A13. [FIXED] `fermium build` executables print "runtime error" for a non-converging integral
The C runtime (`fermium/runtime/aot_rt.c`, `fm_error`) has no case for error kind 9, which
`runtime/core.py` now describes ("this integral doesn't converge ...").
```
print ∫ 1/x dx from 0 to 1
```
`fermium run`: `line 1: this integral doesn't converge: ...`;  built executable: `line 1: runtime error`.

## A14. [FIXED] `fmt --pretty` breaks `3 * 10^8 m/s` (a unit after `10⁸` isn't accepted)
`10^8 m` is 10⁸ metres, but the superscript spelling `10⁸ m` is an error, so `fmt --pretty`
turns a working program into a broken one:
```
print 3 * 10^8 m/s       # works: 3×10⁸ m/s
# fermium fmt --pretty  ->  print 3 · 10⁸ m/s   ->  "line 1: m isn't defined"
print 10⁸ m              # error: m isn't defined     expected 1×10⁸ m (same as 10^8 m)
print 5² m²              # error                       expected 25 m²
print 2⁻¹ m              # error                       expected 0.5 m
print 1.5 × 10³ m        # error (with spaces; `1.5×10³ m` without spaces works)   expected 1500 m
```
Either accept a unit after `number superscript` (like after `number ^ number`) or have
`fmt --pretty` leave `^` alone in that position.

## A15. [FIXED] (HIGH) Adaptive `solve` loses all relative accuracy once a solution decays (absolute error floor)
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

## A16. [FIXED] A function whose only list use is `v[i] = ...` is treated as scalar
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

## A19. [FIXED] (low) Vector ODE solutions: `r''(t)` refused, `r[end]` error has no line number
```
solve r'' = -r with r(0) = <1, 0>, r'(0) = <0, 1> for t from 0 to 3
print r''(3)     # error "can't take that many derivatives of the solution r''"
                 # (scalar x''(t) works, and r.x''(3) works) -- expected <0.989992, -0.14112>
print r[end]     # error "r is a vector; use its components, like r.x" -- no "line 2:" prefix,
                 # and the final vector <cos 3, sin 3> would be a natural answer
```

## A20. [FIXED] Deep or infinite recursion segfaults the whole process (also kills the REPL)
```
f(x) = x * f(x - 1)          # forgot the base case
print f(3)                   # actual: "Segmentation fault" (exit 139)   expected: a one-line runtime error
```
```
f(n) = if n <= 0 then 0 else 1 + f(n - 1)
print f(10000000)            # actual: Segmentation fault   (f(100000) works)
```
Suggestion: a depth counter in each user function (runtime error "f called itself more than
N times -- is a base case missing?"), or a guard page/stack check. Related: `f(x) = f(x)`
(never called) fails to compile with "this program is nested too deeply" and no line number.

## A21. [FIXED] Calling a function before its definition silently uses a constant of the same name
```
print h(2)
h(x) = x^2
# actual: 1.32521×10⁻³³ J s   (Planck constant × 2)     expected: 4, or an error "h is defined
# as a function on line 2, after this line"
```
Same for any name that is also a constant (`c(2)`, `e(1)`, `G(3)`, `k_B(...)`), silently.

## A22. [FIXED] Negating an absolute °C temperature (variable or parenthesised) gives nonsense
After the A8 fix `-5 °C` is right, but:
```
T = 20 °C
print -T              # actual: -566.3 °C (= -293.15 K)   expected: an error, like 20 °C + 10 °C
x = -(5 °C)
print x               # actual: -551.3 °C                 expected: -5 °C or an error
```

## A23. (low) `plot a vs b, c vs d` with different units puts both on one mislabelled axis
```
xs = [1 m, 2 m]
ys = [1 s, 2 s]
plot ys vs xs, xs vs ys to "h.png"
```
Saves a plot whose x axis says `xs [m]` and y axis `ys [s], xs [m]`; the second series' x
values are seconds drawn on the metres axis. Expected: an error ("all series in one plot need
the same x units"), like the unit checker does everywhere else. (Also: `plot ys vs xs` with
lists of different lengths raises the error but still leaves an empty c.png behind.)

## A24. (low) A CSV with an empty cell leaks a Python traceback before the error
`gap.csv`:
```
x [m], y [m]
1, 2
2,
3, 6
```
```
d = load "gap.csv"
print d.y
```
stderr shows `Exception ignored on calling ctypes callback function ... KeyError: 0` (from
`Runtime.column`) and then the proper error `gap.csv, line 3: not a number: ['2', '']`. The
column callback runs after the failed load; it should not be called (or should fail quietly).

## A25. [FIXED] `gamma(x)` (listed in reference §13) is unusable: the lexer turns it into the Greek name γ
```
print gamma(5)      # actual: "line 1: γ isn't defined"   expected: 24
print γ(5)          # same
```
The ASCII→Greek rule (D9) rewrites the builtin's name. (The derivative code knows about
γ(...) -- "can't differentiate γ(...) symbolically" -- but calling it fails.)

## A26. Real odd roots of negatives: `x^(1/3)` works but its derivative and `x^(2/3)` give NaN
```
print (-8)^(1/3)          # -2   (real cube root, good)
print (-8)^(2/3)          # NaN  expected 4 (or consistently NaN for all of them)
f(x) = x^(1/3)
g = f'
print g                   # g(x) = 1/(3 x^0.666666666667)   (exponent printed as a float, not 2/3)
print g(-8)               # NaN  expected 0.0833333 (= 1/12), consistent with f(-8) = -2
```
Only an exponent exactly equal to 1/3 gets the real-root treatment.

## A27. [FIXED] (HIGH) Variables assigned only inside a branch/loop that didn't run: garbage or a made-up value
The checker accepts reading a variable whose only assignment is inside an `if`/`for`/`while`
that never executed; codegen reads an uninitialised slot (LLVM then feels free to use `undef`):
```
x = 1
if x > 2
    y = 3 m
print y              # actual: 3 m   (never assigned!)     expected: an error
for i from 1 to 0
    z = 5
print z              # actual: 5
while false
    w = 1
print w              # actual: 6.90361×10⁻³¹⁰  (garbage)
x = 1
if x > 2
    ys = [1, 2]
print ys             # actual: "runtime error" (no message, no line)
if x > 2
    solve q' = -q with q(0) = 1 for t from 0 to 1
print q(0.5)         # actual: "runtime error"
```
Expected: a compile-time error ("y might not have a value here: it is only set inside the if
on line 2") -- definite-assignment analysis -- or at least a runtime error naming y.
Same inside functions:
```
f(x) =
    if x > 0
        y = 2 x
    y
print f(-1)          # actual: 6.90214×10⁻³¹⁰   expected: an error
```
(Globals defined *after* the call are correctly rejected: "z isn't defined".)

## A29. [FIXED] (HIGH) `2(x+1)^2` squares the 2 as well: juxtaposition with a parenthesis binds tighter than `^`
`number(expr)` / `name(expr)` (multiplication, D8) is parsed like a call, a postfix, so a
following `^` applies to the whole product:
```
x = 3
print 2(x+1)^2          # actual: 64   expected: 32   (2 (x+1)^2 with a space gives 32)
print ½(x+1)^2          # actual: 4    expected: 8
print 0.5(x - 1)^2      # actual: 1.0  expected: 2.0
k = 2
print k(x+1)^2          # actual: 64   expected: 32
print 3(x)²             # actual: 81   expected: 27
x = 3 m
print 2(x)^2            # actual: 36 m²  expected: 18 m²
```
D8 says powers bind tighter than juxtaposition (`4π² L` = 4·π²·L), and the spaced form follows
that; the unspaced form silently doesn't. `½(x - x₀)^2`-style formulas are everywhere in physics.
Also: `(x+1)(x-1)^2` with x = 3 gives 64 (expected 16), and `xs = [1, 2]; print 2(xs)^2`
gives [4, 16] (expected [2, 8]). Separately, `v = <1, 2> m; print 2(v)` is rejected ("this
value must be a number, but it is a 2-vector") although `2 v` works.

## A28. [FIXED] (low) `2½` is 2 × ½ = 1, not the mixed number 2.5
```
print 2½          # actual: 1     expected: 2.5, or an error suggesting 2.5 / 5/2
print 1½ m        # actual: error "m isn't defined"
```

**A15 addendum — long integrations.** Because the floor scales with the *span*, long runs get
looser:
```
solve x'' = -x with x(0) = 1, x'(0) = 0 for t from 0 to 100000
print x(100000)     # actual: -0.996591   expected: -0.999361 (cos 1e5)
```
scipy RK45 at the same rtol=1e-9 (atol=1e-12) gives -0.999349 (error 1.2e-5, vs Fermium's
2.8e-3, i.e. ~200× worse), with Fermium printing 6 significant figures.

## A30. [FIXED] `x'(t)` from a `solve` is only first-order accurate between steps (linear interpolation of slopes)
`fm_sol_eval` evaluates `x'(t)` by *linear* interpolation of the stored derivatives, so its
error is O(h²) — thousands of times larger than the solver's tolerance, while printed to 6 s.f.:
```
solve x' = cos(t) with x(0) = 0 for t from 0 to 20
# max over step midpoints tm of |x'(tm) - cos(tm)| = 0.0020349     (x(tm): 6.4e-6)
solve x' = y, y' = -x with x(0) = 0, y(0) = 1 for t from 0 to 20
# max |x'(tm) - cos(tm)| = 0.000370 while max |y(tm) - cos(tm)| = 2.9e-8 (y *is* x')
```
(e.g. `print x'(1.23)` can be wrong in the 4th digit.) Expected: differentiate the Hermite
cubic (O(h³)), or better, evaluate the right-hand side at the interpolated state (exact to
solver accuracy for first-order unknowns); consider DP45's own 4th/5th-order dense output for
`x(t)` too (midpoint errors are ~1000× the node errors now).

## A31. [FIXED] (low) A `where` binding silently shadows a function parameter
```
f(x) = 2 x where x = 5 s
print f(1 s)        # 10 s -- the argument is ignored, no warning
```

## A32. [FIXED] Python crash (OverflowError) for an infinite constant index
```
xs = [1, 2, 3]
print xs[inf]
```
Traceback from `checker.index_expr`: `int(idx.value)` → `OverflowError: cannot convert float
infinity to integer`. Expected the usual "index ∞ is out of range" error. (`xs[0/0]`, `xs[1e19]`
don't crash the checker but see A33.)

## A33. [FIXED] Runtime errors with no message: "runtime error"
All of these stop with just `runtime error` (no line, no explanation):
```
xs = [1, 2, 3]
print xs[1e19]              # (also i = 18446744073709551617; xs[i], and xs[1e19] = 5)
print xs[0/0]
n = 0
for i from 1 to inf         # (with a break inside -- arguably should just loop)
    n += 1
    if n > 5
        break
for i from 1 to 0/0
    n += 1
print len(zeros(inf))
```
Also after A27: reading a list/solution that was never assigned gives the same bare message.

## A34. [FIXED] Huge list sizes segfault (malloc failure not checked)
```
xs = zeros(3e9)
xs[1] = 2
print sum(xs)               # Segmentation fault (exit 139)
```
`zeros(1e12)` then `print xs[5]` prints `0`, and `print len(linspace(0, 1, 1e12))` prints
`1×10¹²`, so the allocation result is never checked. Expected: "not enough memory for a list of
3×10⁹ numbers". (Minor: `sort([3, 0/0, 1, 2])` returns `[3, NaN, 1, 2]` unsorted.)
A30 shows up in ordinary programs:
```
solve x' = -x with x(0) = 1 for t from 0 to 2
print ∫ x'(s) ds from 0 to 2     # actual: -0.864908   expected: -0.864665 (= x(2) - x(0))
f(s) = x(s)^2
h = f'
print h(1)                       # actual: -0.270784   expected: -0.270671 (= -2e⁻²)
```

## A35. Indefinite integrals with a parameter fail: SymPy's Piecewise leaks out
```
ω = 2 1/s
F = ∫ cos(ω t) dt       # error: SymPy returned something Fermium can't use yet:
                        #   Piecewise((sin(t*ω)/ω, (ω > 0) | (ω < 0)), (t, True))
k = 3
F = ∫ exp(-k x) dx      # same, Piecewise((-exp(-k*x)/k, ...
a = 2
F = ∫ x^a dx            # same
F = ∫ abs(x) dx         # same
```
These are the most common textbook antiderivatives. Fix: declare the SymPy symbols for
parameters as `nonzero=True`/`positive=True` (or take the generic branch of a Piecewise whose
condition is "parameter ≠ 0"). The error also has no line number and names SymPy/Python syntax.
Minor, same area: `print F` shows `∫dx(x) = x³/3` rather than `F(x) = x³/3`, and `∫ 1/x dx`
gives ln(x), so `F(-1)` is NaN (ln|x| expected).

## A36. [FIXED] (HIGH) `std`, `diff`, `sum`, `cumsum` of a °C list print nonsense in °C
The B1 fix (difference of two absolute temperatures is in K) doesn't reach the list functions:
```
T = [10 °C, 20 °C]
print std(T)            # actual: -266.079 °C   expected: 7.07107 K  (`std(T) in K` is right)
T = [0 °C, 100 °C]
print diff(T)           # actual: [-173.15] °C  expected: [100] K
print cumsum(T)         # actual: [0, 373.15] °C  expected: an error (sum of absolute temperatures)
T = linspace(0 °C, 100 °C, 3)
print sum(T)            # actual: 696.3 °C      expected: an error, or 969.45 K
```
Realistic case: `d = load "data.csv"` with a `T [°C]` column, then `print std(d.T)`.
(`mean`, `min`, `max`, `interp`, `trapz` are right.)

## A37. (low, after the A15 fix) Decay below ~1e-300 stalls
```
solve x' = -x with x(0) = 1 for t from 0 to 700
print x(700)            # actual: 7.99895×10⁻³⁰²   expected: 9.85968×10⁻³⁰⁵
solve x' = -x with x(0) = 1e-300 for t from 0 to 30
print x(30)             # actual: 3.79017×10⁻³⁰¹   expected: 9.35762×10⁻³¹⁴ (subnormal)
```
Looks like a fixed tiny absolute floor (~1e-300) in the error norm. Only matters at the edge
of double range; everything down to ~1e-290 is right now.

## A38. `fmt --ascii` turns `∂²/∂x² f` into `partial^2/partial x^2 f`, which doesn't parse
```
f(x, y) = x^2 y^2
g = ∂²/∂x² f
print g(1, 2)          # 8
```
`fermium fmt --ascii` gives `g = partial^2/partial x^2 f` → "expected '/' (write ∂/∂x f) but
found '^'". Either accept `partial^2/partial x^2` in the parser (like `d^2/dt^2`) or emit
another spelling.

## A39. [FIXED] CLI: Python traceback for a file that isn't UTF-8, or a directory
```
printf 'x = 5 \xb5m\nprint x\n' > latin1.fm     # a Latin-1 "µm", e.g. saved by an old Windows editor
fermium run latin1.fm      # Traceback ... UnicodeDecodeError: 'utf-8' codec can't decode byte 0xb5
fermium fmt --pretty latin1.fm   # same
mkdir dir.fm; fermium run dir.fm   # Traceback ... IsADirectoryError
```
Expected one-line errors ("latin1.fm isn't a UTF-8 text file (byte 0xB5 on line 1); save it as
UTF-8", "dir.fm is a folder, not a file"). `cli._read` only handles FileNotFoundError.

## A40. [FIXED] (low, cosmetic) The unit of an *argument* is lost through a function call
```
f(x) = 2 x
print 2 * (1 km), f(1 km)      # 2 km 2000 m
print 2 (1 eV), f(1 eV)        # 2 eV 3.20435×10⁻¹⁹ J
f(E) = E
print f(3 MeV)                 # 4.80653×10⁻¹³ J    expected 3 MeV
```
Units written in the function body are kept (bugs-examples #4), but not the argument's. For a
nuclear-physics user every `f(E)` needs `in MeV`. Display hints are per call site, so each call
could carry its argument's hint (the instance is shared per dimension, so this must be done at
the call, not in the monomorphised body).

## A41. [FIXED] (HIGH) Symbolic derivative drops a factor: `exp(sin(exp(x)))''` is wrong
Found by fuzzing f, f', f'' against mpmath (tests/test_adversarial.py has the fuzz cases).
```
f(x) = exp(sin(exp(x)))
g = f''
print g(13/10)       # actual: 0.856062     expected: 8.25545 (sympy / mpmath)
print g              # g(x) = (cos(exp(x))² - sin(exp(x)) + cos(exp(x)))·exp(sin(exp(x)))·exp(x)
                     # correct: (exp(x)·(cos(exp(x))² - sin(exp(x))) + cos(exp(x)))·exp(sin(exp(x)))·exp(x)
```
Smallest first-derivative repro:
```
f(x) = exp(sin(exp(x))) * cos(exp(x)) * exp(x)
g = f'
print g(13/10)       # actual 0.856062, expected 8.25545
```
Also `f(x) = sin(exp(sin(x)))^2`: f''(1.3) = 2.20714, expected 2.51432.
Likely cause: `calculus.factor_common` builds `{key(f): f for f in fs}` per term, so a factor
that occurs twice in a term (here `exp(x)·exp(x)`) is collapsed; then
`remaining = [f for f in fs if key(f) not in common]` removes *every* copy while only one copy
is multiplied back in front. Count multiplicities (take the minimum count across terms, remove
that many).

## A42. [FIXED] (regression from the A29 fix) `(∂/∂x f)(1, 2)` no longer parses
```
f(x, y) = x^2 y
print (∂/∂x f)(1, 2)     # now: "expected ')' to close '(' but found ','"   (worked before; expected 4)
```
`(d/dx f)(3)` still works; a parenthesised expression followed by a multi-argument `(a, b)`
must still be a call when the parenthesised thing is a function. (This form was in
tests/test_adversarial.py FMT_PROGRAMS, which is how it was caught.)

## A43. [FIXED] (A27 leftover) The loop variable after a loop that never ran is garbage
```
for i from 1 to 0
    print 5
print i              # actual: 6.92797×10⁻³¹⁰
for x in []
    print 5
print x              # actual: 6.92365×10⁻³¹⁰
f(n) =
    for i from 1 to n
        print 1
    i
print f(0)           # actual: 6.90166×10⁻³¹⁰
```
The A27 definite-assignment check doesn't cover the loop variable itself. Either treat it
like other loop-body assignments ("i might not have a value here") or give it a defined value
(e.g. `i` = start value) -- but then document what `i` is after a loop that ran (currently the
last value, 3 for `from 1 to 3`).

## A44. [FIXED] (HIGH, likely regression from the A4 fix) Convergent integrals with 1/√ endpoint singularities are rejected as "doesn't converge"
```
print ∫ 1/sqrt(1 - x^2) dx from -1 to 1          # expected π; actual: "this integral doesn't converge"
print ∫ 1/sqrt(1 - x^2) dx from 0 to 1           # expected π/2; same error
print ∫ 1/sqrt(0.25 - x^2) dx from -0.5 to 0.5   # expected π
print ∫ 1/sqrt(cos(x) - cos(1)) dx from -1 to 1  # expected 4.7376 (pendulum period integral, θ₀ = 1)
print ∫ 1/sqrt(abs(x - 0.3)) dx from 0 to 1      # expected 2.76877 (interior singularity)
```
These are the textbook physics integrals (period of a pendulum / of any 1-D oscillator
between turning points, arcsine distribution). Note `∫ x^(-0.95) dx from 0 to 1` = 20 works,
so the singularity at the lower limit is handled but at the upper limit (1 - x → 0) or at an
interior point it isn't -- maybe the √ of a catastrophically cancelled `1 - x^2` becomes
sqrt(0) = 0 → 1/0 = ∞ at a node, which the divergence check then treats as blow-up. An
endpoint node should never be evaluated (GK nodes are interior) -- but after bisection at depth
~50 a node can round onto the endpoint.

(Retracted: I also reported ~1e-4 inaccuracy for `∫ (x^(-2) + 1)^0.4 dx from 0 to 1` = 5.16031;
that was my reference being wrong -- scipy at epsrel 1e-13 and mpmath after substituting x = u^5
give 5.1603067686, and 13.7312084 for the -2..3 variant, matching Fermium.)

## A45. [FIXED] `fit N = A exp(-t/τ)` without a guess reports a garbage fit as if it were fine
`decay.csv` (t in ms, N = 1000·exp(-t/3 ms) + small noise, 11 points):
```
d = load "decay.csv"
fit N = A exp(-t/τ) to d
# actual:  A = 312.6 (standard error could not be estimated)
#          τ = -4.885×10⁹ ms (standard error could not be estimated), rms residual = 299
# expected: A = 1002 ± 1.5, τ = 2.993 ± 0.0075 ms (scipy curve_fit), rms 1.56
```
The same data fits fine as `A exp(-λ t)`, as `A exp(-t/τ) + B`, or with `with τ = 2 ms`.
Exponential decay with a time constant is the most common fit in a physics lab.
Expected: better automatic starting guesses (e.g. try the column scales: τ ~ span of t), and
when the fit clearly failed (no standard errors, negative time constant, rms ≈ std of the data)
say so ("the fit didn't converge; give a starting guess with `with τ = ...`") instead of
printing the numbers as results.

## A46. (design consequence of D6, but silent) `1 rev/min in Hz` = 0.10472 Hz, not 1/60 Hz
```
print 1 rev/min in Hz     # 0.10472 Hz   -- a rotation frequency of 1 rpm is 0.016667 Hz
print 60 rpm              # 'rpm' isn't a unit
```
With rad = 1 and rev = 2π, converting a rotation rate to Hz is off by 2π. D6 accepts that Hz and
rad/s can't be told apart, but here the user wrote `rev`, whose only purpose is counting turns.
Suggestion: refuse `in Hz` for a value whose written unit contains rad/rev/° (or warn), and add
`rpm` = 1/60 Hz (turns per minute), documenting the choice.

## A47. [FIXED] `T - 20 °C` is rejected unless T was also written in °C (blocks Newton's law of cooling)
```
print 300 K - 20 °C            # error "can't subtract an absolute temperature (°C) from this"
                               # expected 6.85 K (both are absolute temperatures)
Ta = 20 °C
solve T' = -(T - Ta) / (10 min) with T(0 s) = 90 °C for t from 0 min to 30 min
print T(30 min)                # same error; expected 23.4851 °C (20 + 70 e⁻³)
Ta = 20 °C
f(T) = T - Ta
print f(30 °C)                 # same error (the instance sees T as "temperature [K]")
```
`T = 30 °C` then `print T - 20 °C` works (10 K). An absolute temperature in K minus one in
°C is a legitimate difference; only °C + °C, k·°C, etc. are meaningless. The ODE unknown gets
its units from `T(0 s) = 90 °C`, so it should count as absolute too.

## A48. [FIXED] Mapping a vector-valued function over a list crashes codegen (Python TypeError)
```
f(x) = <x, 2x>
print f([1, 2])
```
Traceback from `codegen_llvm.map_list`: `TypeError: cannot store <2 x double> to double*`.
Expected the checker's "lists of vectors aren't supported (yet)" error (D24 says lists of vectors
are not done).

## A49. (low) `g = d/dt (a t^2) where a = 3` is rejected
```
g = d/dt (a t^2) where a = 3
print g(1)        # error "d/dt(...) is a function; give it an argument, like d/dt(...)(x)"
                  # expected 6 (works without `where`, with a = 3 on its own line)
```

## A50. (low / latent) A loaded column is a list header over NumPy's buffer; `push` reallocs it
`e_IColumn` wraps the pointer returned by `fm_column` (NumPy's array data) in a list header
with cap = len, so
```
d = load "pend.csv"
xs = d.L
xs[1] = 7 m          # changes d.L too (shared, fine under D26)
push(xs, 5 m)        # realloc()s memory owned by NumPy -- undefined behaviour; d.L still has 4 values
print xs, d.L        # [700, 20, 40, 80, 500] cm [700, 20, 40, 80] cm
```
I couldn't make it crash (the datasets seem to stay alive for the whole run), but a
realloc/free of a NumPy-owned block is heap corruption waiting to happen (REPL sessions,
future GC of datasets). Copy the column into a malloc'd buffer (or give it cap = -1 so push
copies first), and decide whether `push(xs, …)` on a column alias should affect `d.L`.

## A51. [FIXED] (HIGH) Integrals to ∞ are wrong whenever the physical scale isn't ~1 in SI units
The ∞ transform `x = a + u/(1-u)` assumes the integrand lives on a scale of ~1 SI unit. With
nuclear, atomic, or astronomical scales the result is silently 0, or a false "doesn't converge":
```
a = 1 fm
print ∫ exp(-r/a) dr from 0 m to ∞                             # actual: 0 m     expected: 1 fm
a = 0.529e-10 m
print ∫ 4π r² exp(-2 r/a) / (π a³) dr from 0 m to ∞            # actual: 0       expected: 1 (hydrogen 1s normalisation!)
σ = 1 fm
print ∫ exp(-x^2/(2 σ^2)) dx from -∞ to ∞ in fm                # actual: 0 fm    expected: 2.50663 fm
E0 = 1 MeV
print ∫ exp(-E/E0) dE from 0 J to ∞ in MeV                     # actual: 0 MeV   expected: 1 MeV
print ∫ exp(-t/(1 ns)) dt from 0 s to ∞ in ns                  # actual: 0 ns    expected: 1 ns
a = 1 AU
print ∫ exp(-r/a) dr from 0 m to ∞ in AU                       # actual: "doesn't converge ... estimate
                                                               #  1.49598×10¹¹ ± 158.646"  expected: 1 AU
print ∫ exp(-t/(1 Gyr)) dt from 0 s to ∞                       # actual: "doesn't converge ... NaN ± NaN"
```
(Dimensionless ones like ∫ x³/(eˣ-1) dx = 6.49394 are fine.) This hits exactly this user's
fields (nuclear physics, astrophysics). Suggestion: pick the transform scale L from the problem
-- e.g. the magnitude of the finite limit if non-zero, else scan the integrand at x = 10^k (k from
-40 to 40) for where it's largest / where it decays, and use x = a + L·u/(1-u); or split
[a, ∞) into [a, a + L] + [a + L, ∞). The divergence heuristic also needs to be scale-aware
(the AU case had the right value, 1.496e11 m, but was rejected).

## A52. Symbolic derivatives overflow to NaN (∞/∞) where the true value is ~0 -- Fermi function
```
kT = 0.025 eV
μ = 5 eV
f(E) = 1/(exp((E - μ)/kT) + 1)          # Fermi-Dirac occupation
g = f'
print g(30 eV)      # actual: NaN   expected: -0 (≈ -1.4e-435 → 0)
f(x) = tanh(x)
h = f''             # h(x) = -2 sinh(x)/cosh(x)³
print h(1000)       # NaN, expected 0
f(x) = 1/cosh(x)
print f'(800)       # NaN (g(x) = -sinh(x)/cosh(x)²), expected 0
```
The derivative is formally right, but exp(x)/(exp(x)+1)², sinh/cosh³ etc. are evaluated as
∞/∞. Plotting or integrating such a derivative over a range (e.g. -∂f/∂E over the band) gives
NaN. Possible fixes: rewrite quotients of exponentials (divide through by the dominant term),
use sech/tanh forms, or evaluate `a/b` as 0 when |b| = ∞ and a is finite... at least for
the standard functions' derivative rules (d tanh = 1 - tanh², d sech = -sech·tanh).
(Also a display oddity: the unit of g is shown as s²/(kg m²) rather than 1/J or 1/eV.)

## A53. [FIXED] Divergent integral with a pole at a *transcendental* interior point still returns a number
After the A4 fix, poles at "nice" points are caught, but:
```
print ∫ 1/sin(x)^2 dx from 1 to 5     # actual: 6.30687×10¹⁶   expected: "doesn't converge" (pole at π)
print ∫ 1/sin(x)^3 dx from 1 to 5     # actual: 4.82374×10³²   expected: same error
```
(`1/(x - 3.14159)^2` and `1/sin(x)` are correctly rejected.) Probably sin(π) evaluates to
1.2e-16 rather than 0, so the integrand stays finite (~1e32..1e48) and the estimate "converges"
to a huge value. A relative-magnitude check (result ≫ integrand scale × interval, or error
estimate not shrinking with depth) would catch it.

## A54. Inside `∫ … du`, `1/u` is "per atomic mass unit", not 1/(the integration variable)
Variables and parameters named like units win over the unit (`s = 2; print 1/s` → 0.5,
`f(s) = 1/s` → 0.5), but the integration variable isn't known yet when the integrand is read:
```
print ∫ 1/u du from 1 to 2      # actual: 6.02214×10²⁶ 1/kg   (∫ 1 u⁻¹ du)   expected: 0.693147
f(x) = ∫ 1/u du from 0 to x     # the divergent integral is silently a straight line in 1/kg
```
`u` is the most common substitution variable. (`∫ 1/s ds` and `∫ 2/L dL` fail with "missing its
'dx'", see A17.) The integration variable should be in scope while the integrand is parsed
(or resolved after `d<name>` is seen), with the usual collision warning.

Related: a runtime error inside `plot f(x) vs x from ...` (e.g. a divergent integral in f) is
swallowed: the plot is saved and the program continues.

## A55. `max(x)` / `min(x)` of an ODE solution only look at the solver's step points
```
solve x' = cos(t) with x(0) = 0 for t from 0 to 20
print max(x), min(x)        # actual: 0.999848 -0.999824   expected: 1 -1 (to ~1e-9)
solve x'' = -x with x(0) = 0, x'(0) = 1 for t from 0 to 20 step 0.5
print max(x)                # actual: 0.99713   expected: 1
```
Printed to 6 s.f., so "maximum height of a projectile" style answers are wrong in the 4th
digit. Refine around the best sample with the dense interpolant (solve x'(t) = 0 in the
neighbouring steps, e.g. a few Newton/bisection steps on the Hermite cubic / RHS).

## A56. (regression in the rewritten quadrature) Interior |x|^-p singularities with p ≥ ~0.8 are rejected
Endpoint singularities work (`∫ x^(-0.9) dx from 0 to 1` = 10) and interior 1/√ works, but an
interior singularity a bit stronger than 1/√ is now reported as divergent:
```
print ∫ abs(x)^(-0.8) dx from -1 to 1          # "doesn't converge ... estimate 9.99352 ± 0.002"   expected 10
print ∫ abs(x)^(-0.8) dx from -2 to 3          # expected 11.9721 (worked before the rewrite)
print ∫ abs(x - 3/10)^(-0.8) dx from 0 to 1    # expected 8.58577
print ∫ abs(x)^(-0.9) dx from -1 to 2          # expected 20.7177
print ∫ ((1/x)^2 + 1)^(2/5) dx from -2 to 3    # expected 13.7312 (worked before the rewrite)
```
The estimates are close and the error estimates small, so the convergence test is too strict
for interior algebraic singularities (the endpoint path apparently handles them better -- maybe
split at the detected singular point and treat both halves as endpoint singularities).
