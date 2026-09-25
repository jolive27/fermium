# Friction log: gravitation (first pass)

Problems: `01_orbit_elements.fm`, `02_hohmann_transfer.fm`, `03_mercury_precession.fm`.
Severity scale: blocker / wrong answer / awkward / cosmetic.

## G1. `1.2 T` is 1.2 tesla, even where only a time makes sense (awkward)

T is *the* name for a period in every orbit problem. Integrating for 1.2 periods:

```
for t from 0 s to 1.2 T
```

gives a warning (`'T' right after a number is the unit T`) and then a hard error,
`the range goes from time [s] to magnetic field [T]`. The rule (a unit name right after a number is
a unit, DECISIONS D7) is documented, and the warning is good, but here the unit reading cannot
possibly type-check while the variable reading does. Workaround: `1.2 * T`.
**Fix:** when a number is followed by a name that is both a unit and a user variable, and the unit
reading causes a dimension error in its context while the variable reading does not, take the
variable (keep the warning). Or at least turn the error into "did you mean 1.2*T (your period)?".

## G2. A prime inside an algebraic `solve` is read as an ODE (awkward; also mechanics M7, oscillations O5)

The natural way to find apsides is "radial velocity = 0":

```
solve r(t2) · r'(t2) = 0 km²/s for t2 from 0 s to 3 hr
```

fails with `missing initial condition: r(start)`. Every orbit problem (01, 02, 03) needed the
workaround of defining a helper function first:

```
v_r(τ) = r(τ) · r'(τ) / |r(τ)|
solve v_r(t_aph) = 0 km/s for t_aph from 0 s to T
```

and in 03 even `du(φ) = u'(φ)` just to ask where u' = 0. **Fix:** as in M7: with no `with`, a
primed name that is an existing ODE solution, called at the unknown, is algebraic.

## G3. `h²` can't be a variable name (cosmetic)

The Binet equation is always written with h², and I first wrote `h² = GM a (1 − e²)`. Error:
`can't store a value here: the left side of = must be a variable name`, with the hint
`use == to compare two values`, which is wrong for this case. Wrote `h = √(…)` and used `h²`.
**Fix:** the hint should say "h² is h squared; name the variable h2, or define h and write h²".

## G4. Searching for the *next* apsis needs a hand-picked start (awkward)

`solve … for x from a to b` returns the first root after `a`. Starting at perihelion, r·v = 0 at
t = 0, so the "next perihelion" search must start at some hand-chosen later time
(`t_aph + T/4`, `1.5π`) to skip the root at the start and the aphelion in between. With physics
insight this is fine; a beginner would get t = 0 back. **Fix:** an option to skip a root at the
left end (`from a exclusive`), and/or `solve … for all x from a to b` returning the list of roots
(then `roots[2]` is the next perihelion), and/or a direction filter (`where r·v goes from + to −`).

## G5. Mixed notation for the orbit constants (cosmetic)

The problem statements use GM☉ and GM⊕. `M☉` is a unit and `M_sun` a constant, so `G M_sun` works,
but there is no `GM_sun`/`GM_earth` constant with the (much more precise) IAU values. I typed
the heliocentric and geocentric gravitational constants by hand. **Fix:** add `GM_sun`
(1.32712440018e20 m³/s², IAU) and `GM_earth` (3.986004418e14 m³/s²) as constants.

## G6. Overriding `e`, `h`, `c` silently (cosmetic, maybe fine)

`e = 0.20563` (eccentricity) and `h = …` (angular momentum) silently replace the elementary charge
and Planck's constant; in 03 the same program also uses the built-in `c`. This is documented
(§14) and worked as I wanted. But in a program where `e` is redefined and later `e²/(4π ε₀ r)`
appears, the result would be silently wrong. I'd not change the behaviour, but a note in
`fermium check` output ("e is your eccentricity here, not the elementary charge") would help.

## What worked well

- Orbital mechanics reads like the textbook: `h = r0 × v0` (2-D cross product is a number),
  `ε = ½ |v0|² − GM / |r0|`, `a = −GM / (2 ε)`, `T = 2π √(a³ / GM)`, `∛(GM T² / (4π²))`.
- Vector ODE `r'' = −GM r / |r|³` with `r(t)`, `r'(t)`, `|r(t)|`, `r(t1) × r'(t1)` afterwards.
- The Binet equation with the **angle** as the independent variable, `for φ from 0 to 2.5π`,
  with `u'(0) = 0 /m`, just works, and `tolerance 1e-13` gets Mercury's 43″/century to 6 digits
  from a 5 × 10⁻⁷ rad effect. The Newtonian control gives 10⁻¹⁵ rad.
- Kepler's equation with the textbook names, `solve E - e sin(E) = M for E from 0 to 2π`, works.
- `in arcsec`, `in AU`, `in km²/s²`, `in hr` all as expected.

## Second pass

Problems: `21_restricted_three_body.fm` (rotating-frame vector ODE with Ω × r', Lagrange points,
Routh's criterion, Jacobi constant), `22_jupiter_slingshot.fm` (a hyperbolic flyby set up from its
orbital elements and integrated in the inertial frame with a moving planet),
`23_perturbed_precession.fm` (exact 1/r³ precession, the apsidal-angle integral with 1/√ blow-ups at
both ends, the ODE, and a 1/r⁴ force). Tests: `tests/test_gauntlet2_gravitation.py`.

### G7. `2 V v_inf` is two volts (awkward; repeat of #10, see mechanics M10)

`u_out = √(V² + v_inf² + 2 V v_inf sin(δ))` with `V` = Jupiter's orbital speed. This time the
first-pass fix worked as intended: the error was on the same line and carried the note `'2 V' here
is 2 V, voltage [V] (a unit right after a number); for 2 × your variable V write 2*V`. Still, a
planet's speed called V is textbook notation. **Fix:** see M10.

### G8. No event location, so every apsis needs a bracket (awkward; M1/#33 still open)

The flyby's periapsis: `solve radial(t_p) = 0 m²/s for t_p from 0 s to t_end` with a hand-written
`radial(t) = (r(t) - V_J t) · (r'(t) - V_J)`. The next perihelion: bracket 0.75 P to 1.25 P, because
the aphelion is also a root of r · r' = 0. It works, but "stop at the next perihelion" is what one
means. **Fix:** `solve … until r · r' = 0` with a direction (as SciPy's `events`).

### G9. No `angle(a, b)` between vectors (cosmetic)

The flyby's turning angle is `acos(rel_in · rel_out / (|rel_in| |rel_out|))`. An `angle(a, b)`
(computed as atan2(|a × b|, a · b), accurate near 0 and π) would read better.

### Physics note (not a language problem)

Starting the flyby on the exact hyperbola at 1000 r_p, the heliocentric speed gain between the two
mirror points is 4.520 km/s, 0.5 % more than the asymptotic 4.497 km/s: at 1000 r_p the probe is
still 0.36 % faster than v∞ relative to Jupiter. The program prints both; the ODE matches the
finite-distance value to 7 digits.

### What helped from the first pass

- `GM_sun` (#27): the precession problem uses the exact IAU value, no G·M round-off.
- 3-D vector ODEs with cross products in the rotating frame, `-2 Ωv × v - Ωv × (Ωv × r)`, as in
  Murray & Dermott; the Jacobi constant is conserved to 12 digits over 360 days.
- A matrix times a vector in initial conditions (`r(0) = rot pos_pf`, rot a 2×2 rotation matrix).
- An explicitly time-dependent force in a vector ODE (`r - V_J t`, a moving planet).
- The apsidal-angle integral with inverse-square-root blow-ups at **both** ends, its upper end
  found by an algebraic `solve`: 7 digits with no substitution. (The SciPy reference needed one.)
- Algebraic solves (L1, apsides, periapsis time) are one line each.

## Third pass (graduate, files 31_… and 32_…)

Details, repros and workarounds are in the problem files and in `gauntlet/FRICTION.md`:

- **#67 (W)** `2 ∫ x dx from 0 to 1 - π` silently takes `1 - π` as the upper limit (prints 4.59, where `(2 ∫ x dx from 0 to 1) - π` is −2.14): a spaced `/` after the limit ends it or warns (#8, #61), a spaced `+`/`-` doesn't. δ = 2∫…du − π is how the deflection integral is written. Workaround: brackets
- **#73 (A)** Textbook coefficients `73/24 e²`, `37/96 e⁴`, `121/304 e²`, `π²/12 t²`, `π⁴/80 t⁴` read as a/(b x) (D8): 9 warnings in two problems, each fixed with brackets. The warning works, but Peters' and Sommerfeld's formulas are written this way on paper
- **#74 (A)** The unit-after-number rule (#10) with standard symbols: `2 Ω` (the Rabi frequency) is 2 ohms, `8 K` (the EOS constant) 8 kelvin, `2 b` 2 barns, `0.25 T` (a period) 0.25 tesla, `2 l²` (a length) 2 litres², `2 m` with a mass m; 7 of 20 problems hit it (all caught, as errors or with the #10 note)
- **#77 (C)** The 2022 prefixes make short names units: a parameter `rg` gave "'2 rg' is ambiguous … rg is a unit (mass)" (the rontogram). Few physicists know ronto-/quecto-; the message could name the prefix
