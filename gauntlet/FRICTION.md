# Gauntlet friction log

Every rough edge the gauntlet problems hit, deduplicated across topics, and what was done about it.
Per-topic details, with repros, are in `gauntlet/<topic>/FRICTION.md` (IDs like M2, Q1 refer to those logs).
Severity: **W** wrong answer or silent surprise, **B** bug or misleading error, **A** awkward, **C** cosmetic.

| # | Sev | Topics (log IDs) | Friction | Resolution |
|---|---|---|---|---|
| 1 | A | all (T3, Q3, M1) | No root finder: bisection loops by hand | Fixed: `solve lhs = rhs for x from a to b` (D32) |
| 2 | W | quantum (Q1) | Algebraic `solve` can return a later root when the ends already bracket one | Open |
| 3 | A | optics, oscillations, gravitation, mechanics, nuclear (O1, O5, G2, M7, N2) | A prime in an algebraic `solve` (`solve I'(θ) = 0 for θ …`) is read as an ODE | Open |
| 4 | B | mechanics (M2) | "might not have a value" wrongly reported for a variable set again in a second loop | Open |
| 5 | W | oscillations (O1) | `ω in Hz` silently prints ω, not ω/2π | Open |
| 6 | W | thermodynamics (T1) | `(T - 20 °C) in °C` converts a temperature difference as an absolute temperature | Open |
| 7 | A | electromagnetism, mechanics (E3, M3) | `R <cos φ, sin φ, 0>` doesn't parse (`<` is read as less-than) | Open |
| 8 | W | electromagnetism (E5) | An integral's upper limit swallows a following `/ x` | Open |
| 9 | W | special relativity, thermodynamics, electromagnetism, quantum (S1, T4, E4, Q8) | `a/b (c)` silently means a/(b (c)) | Open |
| 10 | A | all (E6, O2, G1, Q9) | `2 L`, `2 m E`, `1.2 T` read as units even when a variable has that name (spec §3.4.2 rule) | Open |
| 11 | W | quantum (Q2) | An `if` on the independent variable in an ODE loses accuracy | Open |
| 12 | A | quantum (Q7) | `solve` can't integrate towards smaller t; the error mentions a `step` never given | Open |
| 13 | B | quantum, oscillations (Q4, O9) | Errors inside functions or integrals report the wrong line, or none | Open |
| 14 | A | special relativity (S3) | `m_π` can't be a variable name | Open |
| 15 | B | nuclear (N3) | `15.3 / min / g` fails with "min is a built-in function" | Open |
| 16 | A | astrophysics (A2) | No `mas`/`μas`; `arcsec` takes no prefixes | Open |
| 17 | C | oscillations, quantum (O3, Q12) | The look-alike-names warning repeats many times, across unrelated functions | Open |
| 18 | A | electromagnetism (E1) | ∇ or d/dx of a function defined by an integral isn't supported | Fixed: the Leibniz rule differentiates under the integral sign, with boundary terms for limits that depend on the variable; `∂/∂x V`, `∇V` and `∇²V` are integrals of the derivative (D36) |
| 19 | A | electromagnetism (E2) | Integrals of vectors aren't supported (one integral per component) | Fixed: a vector integrand gives a vector with units, one adaptive quadrature per component; Biot–Savart (dl × r over r³) works (D35) |
| 20 | B | electromagnetism (E7) | Indefinite integrals that SymPy answers with `asinh(Abs(…))` fail, without a line number | Fixed: asinh/acosh/atanh/abs/sign mapped, safe positive assumptions and abs(b) for even constants, quantities inside the formula, a numerical check of the antiderivative, undefined names first, and every failure has the line (D37) |
| 21 | A | thermodynamics (T2) | `°C` can't be used in compound units (`°C/min`) | Open |
| 22 | A | oscillations (O6) | No matrices or eigenvalues for normal modes | Fixed: matrices, `det`, `solve_linear` (D33); `eigenvalues(M)`, `eigenvectors(M)` and `eigenvalues(K, M)` for K v = ω² M v by Jacobi rotations (D34) |
| 23 | A | quantum (Q6) | No complex numbers | Open |
| 24 | A | quantum (Q5) | Functions can't be passed to functions | Open |
| 25 | A | nuclear (N1) | Stiff decay chains need millions of RK45 steps: no implicit solver | Open |
| 26 | A | nuclear (N4) | `fit` standard errors aren't available as values | Open |
| 27 | A | gravitation (G5) | No `GM_sun` / `GM_earth` constants | Open |
| 28 | C | gravitation (G3) | `h² = …` gets the hint "use == to compare" | Open |
| 29 | C | several (E8, T6, O3, M6, O2) | Derived units print in base SI (`kg/(s³ A)` for V/m²), `1/s` for rad/s | Open |
| 30 | C | quantum (Q14) | List elements lose their significant figures | Open |
| 31 | C | special relativity (S5) | The "(= … SI)" echo appears after an explicit `in` | Open |
| 32 | B | astrophysics (A1) | 0/0 at an ODE's start gives "too many steps (reached t = 0)" | Open |
| 33 | A | mechanics (M1, G4) | `solve` has no "stop when" condition | Open |
| 34 | A | gravitation (G6) | `e`, `h`, `c` can be redefined silently | Open |
| 35 | C | nuclear (N6) | The `fit` report rounds the value more coarsely than its standard error | Open |
| 36 | W | quantum (S2) | A root found in rounding noise is printed without a warning | Open |
| 37 | C | mechanics (M8) | The root takes the unit of the range's start (`141.506 cm`) | By design: the range's unit is the natural display unit |
| 38 | C | oscillations (O8) | `print` always puts a space between items | By design |
