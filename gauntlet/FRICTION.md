# Gauntlet friction log

Every rough edge the gauntlet problems hit, deduplicated across topics, and what was done about it.
Per-topic details, with repros, are in `gauntlet/<topic>/FRICTION.md` (IDs like M2, Q1 refer to those logs).
Severity: **W** wrong answer or silent surprise, **B** bug or misleading error, **A** awkward, **C** cosmetic.

| # | Sev | Topics (log IDs) | Friction | Resolution |
|---|---|---|---|---|
| 1 | A | all (T3, Q3, M1) | No root finder: bisection loops by hand | Fixed: `solve lhs = rhs for x from a to b` (D32) |
| 2 | W | quantum (Q1) | Algebraic `solve` can return a later root when the ends already bracket one | Fixed: the 200-point scan from `a` always runs, so the first root after `a` is returned (`sin x = 0` from 1 to 10 gives π) (D32) |
| 3 | A | optics, oscillations, gravitation, mechanics, nuclear (O1, O5, G2, M7, N2) | A prime in an algebraic `solve` (`solve I'(θ) = 0 for θ …`) is read as an ODE | Fixed: a called prime of an already-defined function or ODE solution (`I'(θ)`, `r(t)·r'(t)`) is a value; only undefined names make an ODE (D32) |
| 4 | B | mechanics (M2) | "might not have a value" wrongly reported for a variable set again in a second loop | Fixed: an assignment earlier in the same loop body or branch counts as definite for later reads there |
| 5 | W | oscillations (O1) | `ω in Hz` silently prints ω, not ω/2π | Fixed: `ω in Hz` (a name starting with ω/omega, or a value shown in rad/s) warns and suggests `ω/(2π) in Hz` |
| 6 | W | thermodynamics (T1) | `(T - 20 °C) in °C` converts a temperature difference as an absolute temperature | Fixed: a difference of temperatures converted to °C/°F is shown without the offset, with a warning; `2 T` of a °C value warns |
| 7 | A | electromagnetism, mechanics (E3, M3) | `R <cos φ, sin φ, 0>` doesn't parse (`<` is read as less-than) | Fixed: `R <…>` with a space before `<` and a closing `>` on the line multiplies by a vector (D34) |
| 8 | W | electromagnetism (E5) | An integral's upper limit swallows a following `/ x` | Fixed: a `/` with a space before it ends the upper limit; `to 1/2` still means ½ (D34) |
| 9 | W | special relativity, thermodynamics, electromagnetism, quantum (S1, T4, E4, Q8) | `a/b (c)` silently means a/(b (c)) | Fixed: warning when the writing suggests (a/b) c, or when dividing by an ODE unknown (D34) |
| 10 | A | all (E6, O2, G1, Q9) | `2 L`, `2 m E`, `1.2 T` read as units even when a variable has that name (spec §3.4.2 rule) | Fixed: the reading stays (spec §3.4.2), but a unit error on that line now names the cause and suggests `2*L` (D34) |
| 11 | W | quantum (Q2) | An `if` on the independent variable in an ODE loses accuracy | Fixed: the step control finds a jump in t to rounding precision and restarts on its far side; the tolerance holds at 1e-6 to 1e-12 (D35) |
| 12 | A | quantum (Q7) | `solve` can't integrate towards smaller t; the error mentions a `step` never given | Fixed: a decreasing range integrates backwards (rk45 and rk4); `x(t)`, `x[end]`, `times` and `plot` follow; an empty range has its own error (D34) |
| 13 | B | quantum, oscillations (Q4, O9) | Errors inside functions or integrals report the wrong line, or none | Fixed: kernels report the calling line and units, errors inside functions their own line, one-line functions their definition line (D36) |
| 14 | A | special relativity (S3) | `m_π` can't be a variable name | Fixed: `π` after `_` is part of the name (`m_π` = `m_pi`) |
| 15 | B | nuclear (N3) | `15.3 / min / g` fails with "min is a built-in function" | Fixed: after a number, `/ min` is the minute even with spaces (D34) |
| 16 | A | astrophysics (A2) | No `mas`/`μas`; `arcsec` takes no prefixes | Fixed: `mas`, `μas` (`uas`) units; `arcsec` takes SI prefixes |
| 17 | C | oscillations, quantum (O3, Q12) | The look-alike-names warning repeats many times, across unrelated functions | Fixed: warned once per pair of names |
| 18 | A | electromagnetism (E1) | ∇ or d/dx of a function defined by an integral isn't supported | Fixed: the Leibniz rule differentiates under the integral sign, with boundary terms for limits that depend on the variable; `∂/∂x V`, `∇V` and `∇²V` are integrals of the derivative (D36) |
| 19 | A | electromagnetism (E2) | Integrals of vectors aren't supported (one integral per component) | Fixed: a vector integrand gives a vector with units, one adaptive quadrature per component; Biot–Savart (dl × r over r³) works (D35) |
| 20 | B | electromagnetism (E7) | Indefinite integrals that SymPy answers with `asinh(Abs(…))` fail, without a line number | Fixed: asinh/acosh/atanh/abs/sign mapped, safe positive assumptions and abs(b) for even constants, quantities inside the formula, a numerical check of the antiderivative, undefined names first, and every failure has the line (D37) |
| 21 | A | thermodynamics (T2) | `°C` can't be used in compound units (`°C/min`) | Fixed: in a compound unit a degree is a K-sized step: `2 °C/min`, `4.18 J/(g °C)` |
| 22 | A | oscillations (O6) | No matrices or eigenvalues for normal modes | Fixed: matrices, `det`, `solve_linear` (D33); `eigenvalues(M)`, `eigenvectors(M)` and `eigenvalues(K, M)` for K v = ω² M v by Jacobi rotations (D34) |
| 23 | A | quantum (Q6) | No complex numbers | Open |
| 24 | A | quantum (Q5) | Functions can't be passed to functions | Open |
| 25 | A | nuclear (N1) | Stiff decay chains need millions of RK45 steps: no implicit solver | Open |
| 26 | A | nuclear (N4) | `fit` standard errors aren't available as values | Fixed: `err(g)` gives a fitted parameter's standard error, with units |
| 27 | A | gravitation (G5) | No `GM_sun` / `GM_earth` constants | Fixed: `GM_sun` (`GM☉`, IAU nominal) and `GM_earth` constants |
| 28 | C | gravitation (G3) | `h² = …` gets the hint "use == to compare" | Fixed: the error says only a name can be assigned and suggests `solve h² = … for h from … to …` (D34) |
| 29 | C | several (E8, T6, O3, M6, O2) | Derived units print in base SI (`kg/(s³ A)` for V/m²), `1/s` for rad/s | Fixed: composite display units (V/m², T m, W/(m² K), …) instead of base SI; `1/s` stays |
| 30 | C | quantum (Q14) | List elements lose their significant figures | Partly fixed: `for E in [0.50 eV, 0.75 eV]` keeps the elements' precision when they share it |
| 31 | C | special relativity (S5) | The "(= … SI)" echo appears after an explicit `in` | Fixed: no "(= … SI)" echo for a value printed `to N digits` |
| 32 | B | astrophysics (A1) | 0/0 at an ODE's start gives "too many steps (reached t = 0)" | Fixed: a NaN/∞ derivative at the start is reported as such, with the equation's own variable and units (D36) |
| 33 | A | mechanics (M1, G4) | `solve` has no "stop when" condition | Fixed: `until lhs = rhs` stops at the first crossing, located on the dense output; the solution ends there; never crossing is an error (D34); G4 (skipping a root at `a`) is still open |
| 34 | A | gravitation (G6) | `e`, `h`, `c` can be redefined silently | Fixed: redefining a constant that the program already used as the constant warns (a fresh `h = 10 m` doesn't) |
| 35 | C | nuclear (N6) | The `fit` report rounds the value more coarsely than its standard error | Fixed: fitted values are shown to the second digit of their standard error (`A = 1001.7 (standard error 1.5)`) |
| 36 | W | quantum (S2) | A root found in rounding noise is printed without a warning | Fixed: a run-time warning when lhs − rhs is rounding noise compared with the terms it adds up, near the root (D32) |
| 37 | C | mechanics (M8) | The root takes the unit of the range's start (`141.506 cm`) | By design: the range's unit is the natural display unit |
| 38 | C | oscillations (O8) | `print` always puts a space between items | By design |
