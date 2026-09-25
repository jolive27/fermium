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
