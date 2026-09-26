# Friction log: mechanics (first pass)

Problems: `01_projectile_drag.fm`, `02_incline_pulley.fm`, `03_sounding_rocket.fm`.
Severity scale: blocker / wrong answer / awkward / cosmetic.

## M1. No "stop when" / event detection in `solve` (awkward, the biggest one)

Every problem needed the time at which something happens: the ball lands (y = 0), reaches the apex
(y' = 0), the block reaches x = 0.50 m, the rocket reaches its apex (v = 0). What I wanted:

```
solve r'' = <0 m/s², -g> - k |r'| r'
  with r(0) = <0, 0> m, r'(0) = v0 * <cos(θ), sin(θ)>
  until r.y < 0 m                  # or: for t from 0 s until r.y = 0 m
t_land = t_end                     # or: last(times(r))
```

What I had to write, four times across three files (10 lines each):

```
lo = 0 s
hi = T_vac
mid = 0 s
while hi - lo > 1e-9 s
    mid = (lo + hi) / 2
    if r(mid).y > 0 m
        lo = mid
    else
        hi = mid
t_land = lo
```

I also had to guess an end time that is long enough (the vacuum flight time for the ball; the
rocket coast was first given 200 s, which was too short: bisection silently returned the end of the
range, 200.000 s, as the "apex time"). A physicist would not notice that unless they checked.
**Fix:** `until <condition>` on `solve` (event location on the dense output, as SciPy's `events`),
setting the end time; and/or a root finder, e.g. `t_land = root r(t).y = 0 m for t from t_a to t_b`
(bracketed Brent). Either removes all four loops.

**Update (during this pass):** the algebraic `solve lhs = rhs for x from a to b` landed on the
branch, and all four loops are now one line each, e.g.
`solve r(t_land).y = 0 m for t_land from t_apex to T_vac`. What remains: (i) you still have to
guess an end time for the ODE that is long enough; (ii) "first root after a" means the landing
search must start after the apex, because y = 0 at t = 0 as well (a physicist has to know to do
that); (iii) M7 below. An `until r.y < 0 m` clause on the ODE would still read better for
"integrate until it lands".

## M2. False "might not have a value" when a loop variable is reused in a second loop (wrong error, bug)

Two bisection loops that both use a scratch variable `mid`. The second loop sets `mid` before
reading it, yet the checker refuses the program. Minimal repro:

```
n = 0
while n < 3
    mid = n
    n = mid + 1
while n < 6
    mid = n
    n = mid + 1          # line 7: "mid might not have a value here: it is only set inside
print n                  #          the while loop on line 2"
```

The same program with the first loop removed runs. (Hit again in oscillations 01, with `w`,
before root finding existed. No longer in the committed problems since `solve … for` replaced
the loops, but the bug is still there.) The error message even points at the *first*
loop, which has nothing to do with line 7. Workaround used: `mid = 0 s` before the first loop
(`01_projectile_drag.fm`). **Fix:** the definite-assignment analysis must treat an assignment
earlier in the same loop body as definite for reads later in that body, regardless of whether the
name was "maybe-assigned" by an earlier loop.

## M3. `v0 <cos(θ), sin(θ)>` does not parse (awkward; bad error message)

Wanted the textbook `v₀ (cos θ, sin θ)`, i.e. `r'(0) = v0 <cos(θ), sin(θ)>`. The `<` is read as
"less than" and the error is `this line ended before the expression was complete`, pointing at the
end of the line, which does not explain anything. Had to write `v0 * <cos(θ), sin(θ)>`.
**Fix:** allow implicit multiplication by a vector literal when `<` follows a name/number and a
matching `>` closes it on the same line (the same rule that already makes `<1, 0> AU` work), or at
least make the error say "to multiply by a vector write `v0 * <…>`".

## M4. The speed at a given *position* needs an inverse (awkward)

Problem 02(d): "check the ODE speed when x = 0.50 m". The solution is a function of t, so I had to
invert x(t) by bisection (M1 again). A `x' at x = 0.50 m` or `when x = 0.50 m` query on an ODE
solution would read like the problem statement. Same fix as M1 (root finder).
**Update:** now `solve x(t_05) = 0.50 m for t_05 from 0 s to t_half` then `x'(t_05)`. Two lines,
fine.

## M5. Two-phase motion: the natural way works, but it is undocumented (cosmetic / docs)

For the rocket coast phase I wanted initial conditions at burnout, not at 0:
`with vc(t_b) = vp(t_b), yc(t_b) = yp(t_b) for t from t_b to t_b + 400 s`. This works, and
reusing the unknown names of an earlier `solve` also works (it shadows the old solution), and a
piecewise force `burn(t) = if t < t_b then ṁ0 else 0 kg/s` inside the equations works too.
None of the three is mentioned in docs/reference.md §10; I only found them by trying.
**Fix:** document "initial conditions at t₀ ≠ 0", "re-solving with the same names" and
"piecewise forces with `if … then … else`" in §10, with a two-stage example.

## M6. ODE results print in SI although the problem is in km/s (cosmetic)

`v_b` from the closed form prints `2.46125 km/s` (because `v_e` was written in km/s), but the same
speed from `solve` prints `2461.25 m/s` (because the initial condition was `0 m/s`). Harmless, but
the two lines of the same table look inconsistent; I had to remember which is which in the test.
**Fix:** none needed in the language; maybe prefer the unit of the largest-magnitude input in the
equation when printing a solution. Low priority.

## M7. A prime inside an algebraic `solve` is read as an ODE (awkward, confusing error)

Wanted: `solve r'(t_apex).y = 0 m/s for t_apex from 0 s to T_vac` (when is the vertical velocity
zero?). Got: `missing initial condition: r(start)` with the hint to add `with x(0) = …`. The prime
makes `solve` think this is a differential equation. Workaround in `01_projectile_drag.fm`:

```
v_y(τ) = r'(τ).y
solve v_y(t_apex) = 0 m/s for t_apex from 0 s to T_vac
```

**Fix:** an equation with no `with` whose primed names are existing ODE solutions called at the
unknown is algebraic. At least the error should say "to find when r' is zero, define
`v(t) = r'(t)` first".

## M8. The root of `solve … for x from a to b` gets the unit of `a` (cosmetic)

`solve W(s_stop) = 0 J for s_stop from 1 cm to 10 m` prints `141.506 cm`. I started the range at
1 cm only to skip the trivial root at 0; the answer is naturally in metres. Had to add `in m`.
**Fix:** use the unit of `b` (or of the larger end), or the unit of the other side of the equation's
variable if it has one.

## What worked well (for balance)

- Vector ODE with drag `r'' = <0 m/s², -g> - k |r'| r'` is exactly the textbook equation.
- A mixed literal `<0 m/s², -g>` is accepted; `r(t).x`, `|r'(t)|` read naturally.
- `M v' = v_e ṁ − M g` with the unknown mass multiplying the highest derivative is solved for v'.
- `W(s) = ∫ F(x) dx from 0 m to s` (an integral with a variable limit, as a function) just works,
  and `W(x_stop)` printed `-3.1×10⁻¹⁵ J`, i.e. zero.
- `β = 0.400 /m`, `(1 - x/R) in %`, `a ~= b` all did what I expected.

## Second pass

Problems: `21_intermediate_axis.fm` (Euler's equations as one vector ODE, Landau's elliptic
solution), `22_double_pendulum.fm` (Lagrange's equations, normal modes, energy, Lyapunov growth),
`23_brachistochrone.fm` (a functional, its stationarity by differentiating under the integral sign,
rolling). Tests: `tests/test_gauntlet2_mechanics.py`.

### M9. Two second derivatives in one equation are refused, with a misleading error (bug)

Lagrange's equations for the double pendulum come out with θ₁'' and θ₂'' in *both* equations (a
mass matrix). Written as on paper:

```
solve a'' + 0.5 b'' = -a / (1 s²),
      b'' + 0.5 a'' = -b / (1 s²)
  with a(0) = 1 m, b(0) = 0 m, a'(0) = 0 m/s, b'(0) = 0 m/s
  for t from 0 s to 1 s
```

gives `line 1: ' (prime) means a derivative; it only works on functions and ODE solutions`, pointing
at `0.5 b''`. `b` *is* an unknown of this very `solve`; the real restriction is "each equation must
contain exactly one highest derivative". Workaround used: solve the 2×2 linear system for the
accelerations with `solve_linear` inside a helper function, and make θ = <θ₁, θ₂> one vector
unknown (`solve a'' = acc(a, a')`). That works nicely, but a physicist should not have to invert the
mass matrix by hand. **Fix:** the equations are linear in the highest derivatives, so collect them
into M(q, q') q'' = f(q, q') and solve that linear system in every right-hand-side call; at minimum,
say "a'' and b'' both appear in equation 1: write each equation for one highest derivative, or solve
for them with solve_linear".

### M10. `2 g` is two grams, again (wrong answer with only a warning; repeat of #10)

`√(2 g yc(θ))` in the brachistochrone functional: a warning only, the program went on, and the unit
error came three lines later on `print T_pert(0) - T_cycloid` (units `m^(1/2)/kg^(1/2)`), without the
"'2 g' here is 2 grams" note, because the note is only attached to an error on the same line. The
two `print`s before it would have printed nonsense with odd units. Across this pass the same trap
hit `2 g`, `2 m`, `2 V`, `27 b²`, `3 b` and `3 V` (oscillations O11, gravitation G7, thermodynamics
T7 and T11): six times in twelve problems. **Fix:** when a number is followed by a unit name that is
also a user variable *and* the product is multiplied by further user variables (`2 g yc(θ)`,
`2 V v_inf`, `2 m m2`), make it an error asking for `2*g` (or `(2 g)` for the unit); or carry the
"unit right after a number" provenance with the value so that a later unit error can name it.

### M11. d/dε of an integral fails with "doesn't converge" when the integrand is 0/0 at an end (bug)

`T_pert(ε) = ∫ √(xc'(θ)² + …) / √(2*g (yc(θ) + ε a sin(πθ/θf)²)) dθ from 0 to θf` with
`yc(θ) = a (1 - cos(θ))`, then `dT = d/dε T_pert` and `print dT(0)`:

```
this integral doesn't converge: the integrand may blow up (like 1/x at 0) or keep oscillating
(like sin(x) up to ∞) -- the estimate was 0.000164321 ± 2.29482×10⁻¹³ in SI units
```

The integrand of dT/dε is finite (each term tends to a constant as θ → 0), but it is a difference
of 0/0 terms, and `1 − cos θ` loses all its digits near 0. T_pert itself integrates fine. Three
problems: (i) the refusal contradicts its own estimate (± 2e-13); (ii) in the full program the error
named the line of a *different* integral (the straight-line one, several lines earlier); in a small
repro it named no line at all (known #13); (iii) the fix a physicist has to find is numerical:
`yc(θ) = 2 a sin(θ/2)²`. With it, dT/dε(0) = 3e-17 s and d²T/dε² = 0.164943 s, matching SciPy to
1e-4. **Fix:** when the integrand is NaN at a few nodes next to an end point but the estimate has
converged, drop those nodes (or evaluate slightly inside) instead of refusing; and let SymPy rewrite
`1 - cos(x)` as `2 sin(x/2)²` when simplifying derivatives.

### M12. `<0, 0> /s` is not a vector with a unit (awkward)

`a'(0) = <0, 0> /s` → `s isn't defined … s is a unit; units go right after a number`. `0 /s` works
for a number and `<0, 0> rad/s` works, so the space-slash form after `>` is the odd one out.
**Fix:** after a vector literal's `>`, accept `/unit` exactly as after a number.

### M13. The printed derivative formula shows `2 g·(…)` (cosmetic, but a trap)

`print dT` shows `… √(2 g·(yc(θ) + ε a sin(π θ/θf)²)) …`: the variable `g` printed in a way that,
pasted back into a program, means two grams. **Fix:** print `2*g` or `2·g` when a variable's name is
also a unit.

### What helped from the first pass

- The algebraic `solve … for` (#1): flip times, the cycloid's θ_f, the quarter period of the slow
  mode, all one line each, on ODE solutions or on formulas.
- The Leibniz rule (#18): `d/dε` and `d²/dε²` of a function defined by an integral, exactly what a
  first-variation argument needs.
- The `a/b (c)` warning (#9) caught a real slip: `√(g / l1 (2 - √2))`.
- Vector ODEs with a user function on the right (`solve ω' = euler(ω)`), vectors of angles, and
  `solve_linear` inside that function all compile; 20 s of chaotic double pendulum conserves energy
  to 1e-9, and the separation of neighbouring trajectories matches SciPy to 3 %.
- `eigenvalues(K, M)` / `eigenvectors(K, M)` (#22) give the double pendulum's (2 ∓ √2) g/l at once.
- Still open and still felt: M1/#33 (no "stop when"; every event needed a hand-picked bracket).

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#74 (A)** The unit-after-number rule (#10) with standard symbols: `2 Ω` (the Rabi frequency) is 2 ohms, `8 K` (the EOS constant) 8 kelvin, `2 b` 2 barns, `0.25 T` (a period) 0.25 tesla, `2 l²` (a length) 2 litres², `2 m` with a mass m; 7 of 20 problems hit it (all caught, as errors or with the #10 note)
- **#76 (C)** `Ωπ = …` (Ω for the pendulum about θ = π) is Ω × π, and the error "can't store a value in Ωπ" suggests `solve Ωπ = … for Ω`; it could say that `Ωπ` is read as Ω times π and suggest `Ω_π`
- **#80 (C)** Display units: a conductivity ε₀ω_p²/γ prints as F/(m s), not S/m; a speed squared a²ω² as J/kg
