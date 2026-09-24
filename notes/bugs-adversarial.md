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
