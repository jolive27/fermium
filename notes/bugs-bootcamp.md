# Bugs and confusing behaviour found while writing the bootcamp

Written by the bootcamp agent. Each entry: minimal repro, expected, actual. Workarounds used in the lessons are noted.

## B1. `x^2 L` : a number in an exponent grabs the following name as a unit (ASCII spelling breaks)

```
L = 1.20 m
T = 2.21 s
print 4 pi^2 L / T^2
```
- **Expected:** `9.70 m/s²` — the same as `4π² L / T²` (spec §3.4.3: the ASCII form is first-class).
- **Actual:**
```
e.fm, line 3: an exponent must be a plain number, but this is a quantity with units [m³ s⁴ A²/kg²]
    print 4 pi^2 L / T^2
               ^^^^^^^^^
```
  `^2 L / T^2` is read as `^(2 litres / tesla^2)`. Same for `v^2 m` (→ 2 metres) and `pi^2 L`. `x^2 k` works only because `k` is not a unit.
- **Suggested fix:** a number directly after `^` should never take a unit (the exponent is a bare number; `x^2 L` should be `x² · L`, as DECISIONS D8 says for `x^2y`).
- **Workaround in lessons:** write `4 pi^2 * L / T^2` or use `²`.

## B2. "unit after number" warning fires once per name, on the harmless line, and not on the dangerous one

```
m = 2 kg
v = 3 m/s
E = 0.5 m v^2
print E
```
- **Actual:** one warning on line 2 (`3 m/s`, which obviously means metres per second), then **no warning** on line 3, where `0.5 m v^2` silently means 0.5 metres × v² and prints `4.5 m³/s²`.
- **Expected:** at least warn on line 3 (the case that actually changes meaning). Ideally don't warn for `3 m/s` at all: `m/s` is unambiguous. Every beginner will define `m = ... kg` and then write speeds in m/s, so the current warning trains them to ignore warnings.
- **Workaround in lessons:** teach `½ m v²` / `0.5 * m * v^2`, and the gotcha explicitly.

## B3. `plot f(x) vs x from a to b` fails unless `x` already has a value (docs/reference.md §11 form)

```
f(x) = 50 N/m * x
plot f(x) vs x from 0 m to 1 m to "f.png"
```
- **Expected:** a plot of f over 0–1 m (reference.md §11 lists this form).
- **Actual:** `line 2: x isn't defined` (caret on the `x` inside `f(x)`). Also fails for `plot sin(x) vs x from 0 to 6`. Works only if a variable `x` was defined earlier (`x = 1`), which it shouldn't need.
- **Workaround in lessons:** `xs = linspace(0 m, 1 m, 100)` then `plot f(xs) vs xs`.

## B4. The unit Fermium prints can't always be typed back in: `1/s`

```
print 0.1 1/s
```
- **Actual:** `line 1: s isn't defined` / hint `s is a unit; units go right after a number`. But Fermium itself prints angular frequencies as `10 1/s` (DECISIONS D11), so a beginner copying the output back gets an error. `0.1 / s` gives the same error.
- **Expected:** `0.1 1/s` (or at least `0.1 /s`) accepted, or the hint says "write `0.1 s^-1`".
- **Workaround in lessons:** `s^-1`.

(B1 addendum: the REPL's own `:help` text says `(or ASCII: g = 4 pi^2 L / T^2)`, which is exactly the line that fails.)

## B5. REPL runs a block after its first indented line, so multi-line blocks and `else` don't work

Typed into `fermium` (the REPL):
```
fm> for i from 1 to 3
...     print i
1
2
3
fm>     print i^2
line 1: this line is indented but isn't inside a block
```
and
```
fm> if x > 2
...     print "big"
fm> else
line 1: didn't expect 'else' here
```
- **Expected:** like Python, keep reading `...` lines until an empty line, then run the whole block.
- **Workaround in lessons:** lessons put loops/ifs in `.fm` files; the REPL is used for one-liners only.

(B2 addendum: `E = 0.5 m v^2 where m = 2 kg, v = 3 m/s` silently prints `4.5 m³/s²` with **no warning at all** — the `where` variable `m` isn't known yet when the unit rule runs.)

## B6. A Python warning leaks out of `fit` when no starting guesses are given

With `bootcamp/data/decay.csv` (header `t [min], counts`):
```
d = load "data/decay.csv"
fit counts = N0 exp(-t / tau) to d
```
- **Actual:** the fit is right, but first prints
```
/home/user/fermium/fermium/runtime/fitting.py:16: RuntimeWarning: overflow encountered in dot
  v = float(np.dot(r, r))
```
- **Expected:** no Python internals on screen (spec: no tracebacks/Python noise for users). Suppress with `np.errstate(over="ignore")` in the powers-of-ten scan.
- **Workaround in lessons:** give starting guesses: `... to d with N0 = 1000, tau = 10 min`.
- Minor: τ is reported as `1206 s` although the column is in minutes; `min` would be friendlier.

## B7. `round` and `floor` print with decimals

```
print round(2.567), floor(2.7)
```
- **Actual:** `3.000 2.0`
- **Expected:** `3 2` — rounding to a whole number and then printing `3.000` confuses beginners (it looks like it didn't round). Similarly `factorial`-style integer results print as `3.6288×10⁶` instead of `3628800` (`fact(n) = if n <= 1 then 1 else n * fact(n - 1); print fact(10)`).

## B8. `%` is listed as a unit (reference.md §15) but is rejected

```
x = 5 %
```
- **Actual:** `line 1: unexpected character '%' (Percent Sign)`.
- **Expected:** 0.05 (or remove `%` from the reference).

## B9. "isn't defined" hint is unhelpful for typos, unknown units and functions

- `pritn 5` → `pritn isn't defined` / `hint: give it a value first, e.g.  pritn = 1.0 m`. Expected: "did you mean print?".
- `print 2 furlongs` → hint suggests `furlongs = 1.0 m`. Expected: "furlongs isn't a unit Fermium knows" (it's right after a number, so it was surely meant as a unit).
- `print foo(3)` → hint suggests `foo = 1.0 m`; for a call it should suggest defining a function `foo(x) = ...`.
- `omega = 2` then `print omegat` (or `ωt`) → no "did you mean ω t?" hint, although DECISIONS D9 says the error suggests it (it works for `LT`).

## B10. Small oddities (low priority)

- `print 3 m m` prints `3 m m` (not `3 m²`).
- `print 3.0.1` prints `0.30` (typo silently read as 3.0 × .1).
- Runtime errors have no line number: `xs = [1,2,3]` / `print xs[4]` → `index 4 is out of range: ...` (compile errors do have `file, line N:`). Same for `these two lists have different lengths (3 and 2)`.
- `print (2 + 3` reports the error at `line 2` (a line that doesn't exist in a 1-line file) with an empty source line.
- `print integral x^2 dx` prints `∫dx(x) = 0.333333333333 x³` (odd name, 12 digits); `print d/dx (x^2)` prints `d/dx(...)(x) = 2x`.
- `print sqrt(-1)` prints `NaN` and `print 1/0` prints `∞` with no error or warning.
- The warning for `2 g h` says "the unit g (metres, grams, ...)" — "metres" is wrong for g.
- `vs`, `step`, `to`, `from`, `in`, `with`, `where`, `fit`, `load`, `plot`, `solve` are reserved words; `vs = [3.0 m/s, 4.0 m/s]` gives just `didn't expect 'vs' here` — a hint "vs is a reserved word, pick another name" would help.
- REPL `:vars` lists names only, not values.

## B11. `3 * 10^8 m/s` fails (after the B1 fix)

```
print 3 * 10^8 m/s
```
- **Actual:** `line 1: m isn't defined` with hint `m is a unit; units go right after a number, like 1 m` — but it *is* right after a number, so the hint is baffling.
- `3×10^8 m/s` and `3e8 m/s` work. Physicists will write `3*10^8 m/s` all the time.
- **Expected:** either accept it (a unit after a `^`-exponent literal belongs to the whole product, as for `3×10^8 m/s`), or a hint "write 3e8 m/s or 3×10^8 m/s".

## B12. `c` right after a number is the speed-of-light *unit*, which surprises

```
print 1 AU / c          # prints: 1 AU / c
print 1 kg * c^2        # prints: 1 kg * c^2
```
- Correct by the rules (`AU/c` is a unit of time), but a beginner expects a number of seconds / joules. The printout looks like the program was echoed back. Maybe warn when a unit expression is printed unconverted and contains `c`? Or print `1 AU/c (499.005 s)`.
- **In lessons:** taught as part of the "unit right after a number" gotcha; `print 1 AU / c in s` works.

## B13. `where` still bypasses the new B2 warning

`E = 0.5 m v^2 where m = 2 kg, v = 3 m/s` → prints `4.5 m³/s²`, no warning (B2 fix works for ordinary variables).

## B14. A function's result loses the unit you wrote (eV → J)

```
energy(n) = -13.6 eV / n^2
print energy(1), energy(2)
print -13.6 eV / 2^2
```
- **Actual:** `-2.18×10⁻¹⁸ J -5.45×10⁻¹⁹ J` then `-3.4 eV`.
- **Expected:** `-13.6 eV -3.4 eV` (DECISIONS D11: the written unit is kept through scaling by plain numbers; it works outside a function but not through a function call).
- **Workaround in lessons:** `print energy(2) in eV`.
- Similar: `10°` prints as `10 °` (with a space), and `fmt --pretty` turns `10 deg` into `10 °`.

## B15. Control+C doesn't stop an infinite loop

```
x = 1
while x > 0
    x += 1
```
Run with `fermium run`, then press Ctrl+C (SIGINT): **nothing happens**; the process keeps using 100% CPU (tested with `timeout -s INT 3`; it was still running 2 minutes later). Control+\ (SIGQUIT) does kill it. Beginners *will* write infinite `while` loops. Expected: Ctrl+C stops the program with a one-line message ("stopped by Ctrl+C"). (Probably the JIT code never returns to Python so the KeyboardInterrupt is never raised; a signal handler that sets a flag checked on loop back-edges, or restoring SIG_DFL for SIGINT while native code runs, would fix it.)
- **In lessons:** TROUBLESHOOTING tells people to press Control+\ (or close the Terminal window) if Control+C doesn't work.

## B16. Text can be stored in a variable but not printed

```
label = "hi"
print label
```
- **Actual:** `line 2: can't print this` (also for `label = if x > 1 m then "long" else "short"`).
- **Expected:** `hi` — or an error at line 1 saying text can't be stored in variables yet. The message "can't print this" doesn't say why.

## B17. `%` for remainder gives "unexpected character"

`if n % 2 == 0` → `unexpected character '%' (Percent Sign)` / `hint: remove it, or check the cheat sheet`. Python users will try this; a hint "for the remainder use mod(n, 2)" would help (and see B8: `%` is also documented as a unit).

## B18. `20 m/s / g` divides by *grams*, silently (HIGH: wrong physics, no warning)

```
g = 9.81 m/s^2
print 20 m/s / g        # prints: 20 m/s / g
print 2 * 20 m/s / g    # prints: 40 m/s / g
x = 20 m / g            # even with spaces around /
print x                 # prints: 20 m / g
```
- Wanted: v₀/g = 2.04 s. Got: a speed per gram. DECISIONS D7 rule 4 says "`/` followed by a unit name continues the unit even if a variable has that name. **You get a warning when it does**" — but no warning is printed in any of these cases.
- This is the most natural way to write t = v₀/g (or `2 v0/g` with numbers plugged in). I hit it by accident in Lesson 4.
- **Expected:** at minimum the promised warning. Better: when a variable with that name exists and there are spaces around `/` (`20 m / g`), treat it as division by the variable.
- **Workaround in lessons:** `(20 m/s) / g`, or a variable `v0 = 20 m/s` then `v0 / g`. Taught as part of the gotcha.

## B19. `len` of a list of decimals prints as `5.00`

```
times = [2.21 s, 2.19 s, 2.24 s, 2.20 s, 2.23 s]
print len(times)
```
- **Actual:** `5.00` (significant figures of the elements leak into the count). `len([1, 2])` prints `2`.
- **Expected:** `5` — a count is exact.

## B20. (confusing, maybe by design) `ys = xs` makes both names refer to the same list

```
xs = [1 m, 2 m]
ys = xs
ys[1] = 5 m
print xs        # [5, 2] m
```
Same as Python, but a surprise for a beginner who thinks of `=` as "copy the value" (which is what it does for numbers). A `copy(xs)` builtin (or copy-on-assign semantics) would be good; I mention it in Lesson 5.
