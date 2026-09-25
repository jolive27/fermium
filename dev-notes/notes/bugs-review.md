# Bugs found while re-reading the docs as a beginner (review pass, 2026-09-24)

Each entry: minimal repro, expected, actual. The docs describe the current behaviour honestly where a student would hit it.
(Older, still-open items are in `notes/bugs-bootcamp.md`; re-checked in this pass: B27 (`with` indented less than the
equations) and B23 (untidy printed derivatives, e.g. `x'(t) = 3 t²·3 m/s^3`) still reproduce.)

## R1. Leibniz second derivatives `d²x/dt²` aren't parsed, and the errors mislead

```
y(t) = 20 m/s * t - ½ * 9.81 m/s^2 * t^2
print d²y/dt²            # same with d^2y/dt^2
```
- **Actual:** `line 2: d isn't defined` / `hint: give it a value first, e.g.  d = 1.0 m`.
- Inside `solve`:
  ```
  solve d²x/dt² = -x / (1 s)^2 with x(0 s) = 1 m, x'(0 s) = 0 m/s for t from 0 s to 3 s
  ```
  gives `this solve has no derivatives in it, so there's no differential equation to solve`, which is wrong from the user's point of view.
- **Expected:** `d²x/dt²` = `x''` (first-order `dx/dt` already works, as do `d²/dt² x` and `x''`). At minimum, a hint: "write x'' or d²/dt² x".
- **Docs:** lesson 7, lesson 9, reference §8/§10 and TROUBLESHOOTING say `d²x/dt²` isn't supported yet.

## R2. `d/dt (dx/dt)` in `solve` is treated as a first-order equation

```
solve d/dt (dx/dt) = -x / (1 s)^2 with x(0 s) = 1 m, x'(0 s) = 0 m/s for t from 0 s to 3 s
```
- **Actual:** `x'(…) isn't needed: the equation for x is order 1`.
- **Expected:** order 2 (same as `x''`), or an error saying nested `d/dt` isn't supported.

## R3. `dx/dt(0) = …` isn't accepted as an initial condition

```
solve x'' = -x / (1 s)^2 with x(0 s) = 1 m, dx/dt(0 s) = 0 m/s for t from 0 s to 3 s
```
- **Actual:** `initial conditions look like  x(0) = 1 m  or  x'(0) = 0 m/s` (clear, at least).
- **Expected:** now that `dx/dt` works in equations, users will write the initial condition the same way. Low priority.

## R4. The "unit after a number" warning's hint always says `2*m`, whatever the number

```
m = 0.5 kg
A = 0.1 m
```
- **Actual:** `warning: line 2: 'm' right after a number is the unit m, not your variable m` / `hint: to multiply by the variable write 2*m; ...`. Same for `∫ ... from 0 m to 20 cm` (hint `2*m`).
- **Expected:** `0.1*m` (use the literal that's actually there), like the other variant of this warning does (`hint: ... write *m (e.g. 0.5*m)`, printed for `print 0.5 m v^2`). There are two differently-worded versions of the same warning; they should match.
- Also (B2 again): here the warning fires on the harmless `A = 0.1 m` (clearly 0.1 metres), and the README tour tripped over it. The README now uses `A = 10 cm`.

## R5. Runtime error for a solution outside its range shows bare SI numbers

```
solve N' = -N / 5 s with N(0) = 1000 for t from 0 s to 20 s
print N(30 s)
```
- **Actual:** `asked for the solution at 30 (SI units), outside the range it was solved for (it ends at 20)`.
- **Expected:** `asked for the solution at t = 30 s, but it was only solved from 0 s to 20 s` (with units; "SI units" is jargon for a beginner). Shown verbatim in TROUBLESHOOTING.

## R6. `from 1 to e` in an integral: the hint is about something else

```
print integral 1 / x dx from 1 to e
```
- **Actual:** `the limits of this integral are a plain number (no units) and charge [C]; they need the same units` / `hint: if you divide or multiply the integral by something, put the integral in parentheses`.
- **Expected:** when a limit is the constant `e`, the hint should say "e is the elementary charge in Fermium; for Euler's number write exp(1)". (Lesson 8 mentions this exact mistake.)

## R7. `vec(3, 4) m/s` fails though `<3, 4> m/s` works

```
print vec(3, 4) m/s
```
- **Actual:** `m isn't defined` / `hint: m is a unit; units go right after a number, like 1 m, or in brackets [m]`.
- **Expected:** either accept a unit after `vec(...)` (DECISIONS D24 says `vec(a, b, c)` "is the same thing" as `<a, b, c>`), or a hint "write vec(3 m/s, 4 m/s) or <3, 4> m/s". Low priority.
