# Morning report: Fermium, the overnight run of Sep 24–25, 2026

> Final version, 11:45 UTC Sep 25 (branch `claude/lucid-gauss-9y1ov2`, draft PR jolive27/fermium#1). Everything below is backed by
> tests in `tests/` unless it says otherwise; each number was re-checked against the program or file it comes from.

## In one paragraph

Fermium is a physics programming language:
- units are checked before the program runs, and cost nothing at run time;
- calculus is part of the language;
- programs compile to native code through LLVM.

Overnight it went from a spec to:
- 3612 passing tests (plus 2 strict xfails for the two documented red-team limitations);
- 61 + 20 textbook problems;
- 11 research reproductions compared with published numbers;
- 7 red-team rounds with 88 findings: 83 fixed, 2 partly fixed and 3 documented as by design or known limitations;
- all 8 moonshots done (M4 only in the interpreter; M5 matches or beats Julia on 3 of 7 benchmark rows).

## User requests during the night

| Request | Status | Evidence |
|---|---|---|
| Complex numbers, for quantum mechanics (review priority 1) | **Done** (D90–D94) | `3 + 4i`, `𝑖`, `polar`, complex ODEs, integrals and PDEs, `fermium build`. tests/test_complex.py (106 tests). |
| The unit-after-number ambiguity (`2 g`, `2 m v`) (review priority 2) | **Done** (D7 revised, then D130, D170, D171, D180) | A unit name right after a number that is also your variable is an error when combined with other factors, and a warning when it stands alone. `2 c`, `2 m c²`, `8 /m³` and `36 km/h` are covered too. tests/test_unit_rule.py, test_gauntlet3_fixes.py, test_redteam2/3.py. |
| Warn when an integral may be silently wrong (review priority 3) | **Done, partly** (D110) | A result that is exactly 0 because every sample was 0 warns, in JIT, interpreter and build (tests/test_integral_warnings.py). A peak that is only partly sampled is still not detected (D110, "Not done"). |
| Stale "Partial" section in PROGRESS.md (review priority 4) | **Done** | Rewritten at 03:40 and kept current. |
| **3 significant figures by default when precision is ambiguous or unspecified, display only** | **Done** (D11) | `1/2` → `0.500`, `2π` → `6.28`, `c` → `3.00×10⁸ m/s`. Values with stated precision keep it; whole numbers and literals print exactly; `to N digits` overrides; `1000000/3` → `3.33×10⁵`; a sum keeps its most precise operand's figures (`293.15 K + 0.5 K` → `293.65 K`). The JIT, the interpreter and `fermium build` agree. tests/test_default_sigfigs.py. A test proves it is display only (`x = 1/3; x * 3 - 1` prints `0`). |

## Phase 1: audit
AUDIT.md (00:05–00:25 UTC) checked the spec requirements, the doc claims, the stubs and the broken things. It found items A1–A30, and an adversarial pass (notes/bugs-adversarial.md) added A31–A56.
- All were fixed or documented by 02:00 UTC.
- Two remain documented limitations of the quadrature:
  - off-zero singularities (A56);
  - a narrow peak sitting exactly on a bisection point, which gives half the integral.

## Phase 2: features (all six done by 02:00 UTC)
Jupyter kernel and notebook; language server with unit hover, live errors and `\name` completion; VS Code extension; vectors and matrices with units and 3-D vector ODEs; ∂/∂x, ∇, div, curl, laplacian; browser playground (Pyodide); `fermium build` (standalone executables, including load/fit/plot to SVG).

## Phase 5: moonshots

| | Moonshot | Status | Evidence |
|---|---|---|---|
| M1 | Natural units (`units natural(ħ = c = 1)`, `nuclear`, `astro`) | Done (D60) | tests/test_natural_units.py; examples/28 |
| M2 | Dimensional analysis (`analyze …: T [s] depends on …`) | Done (D70), bootcamp lesson 11 | tests/test_dimensional_analysis.py |
| M3 | Serious numerics: stiff solvers + stiffness warning, eigenvalue problems, 1-D PDEs (heat, wave, Schrödinger; GIF), FFT, root finding, Monte Carlo, seeded RNG | Done (D42, D80–D83, D131, D160) | tests/test_m3_*.py, test_stiff.py; all validated against SciPy/NumPy or closed forms |
| M4 | Uncertainties (`5.0 ± 0.2 m`, correlations, `propagate montecarlo`, uncertain fit parameters, error bars), bootcamp lesson 12 | Done, **interpreter only** (D120–D124) | tests/test_uncertainty.py, checked against the `uncertainties` package and SciPy `curve_fit` covariances |
| M5 | Performance: integer loop counters, `parallel for` (pthreads, reproducible sums), fair benchmarks | Done (D150–D152). On a quiet machine it matches or beats Julia on 3 rows (nbody 0.95×, forces 0.65× with 4 threads, spring_adaptive 0.86×) and is slower on 3; the table is below. | RESULTS.md, tests/test_parallel.py |
| M6 | Python interop: `use python numpy as np` with unit contracts; `fermium.compile()` from Python | Done (D140–D142) | tests/test_python_interop.py |
| M7 | Modules (`import`, `from … import`), stdlib (mechanics, em, nuclear, astro, quantum, stats), `fermium.toml` | Done (D100–D103) | tests/test_modules.py, test_stdlib.py; docs/stdlib.md |
| M8 | Self-hosting: the unit database is written in Fermium and generates the compiler's factor table | Done | fermium/selfhost/, tests/test_selfhost.py. It found that M☉ was rounded and that the parsec wasn't the IAU definition. |

## Phase 6: research reproductions (research/)

| # | Reproduction | Fermium | Published |
|---|---|---|---|
| 1 | SEMF fitted to AME2020 (2484 nuclei) | a_V = 15.41, a_S = 16.86, a_C = 0.695, a_A = 22.50, a_P = 12.0 MeV; rms 3.31 MeV; residual peaks at Z, N = 28, 50, 82, 126 | Rohlf/Krane coefficients; magic numbers |
| 2 | Neutron star M–R from TOV (ideal neutron gas) | M_max = 0.7102 M☉, R = 9.16 km | 0.71 M☉ (Oppenheimer & Volkoff 1939) |
| 3 | Lane–Emden → Chandrasekhar mass | ξ₁(n=3) = 6.896849, M_Ch = 5.825/μ_e² M☉ | 6.89685; 5.83 (Chandrasekhar 1939) |
| 4 | U-238 Bateman chain (15 members, radau) | secular equilibrium; Rn-222 99 % in-growth in 25.40 d | closed-form Bateman, all digits |
| 5 | Hydrogen levels (radial Schrödinger) | Lyman α 121.5684 nm | 121.567 nm (NIST; the gap is fine structure) |
| 6 | Friedmann age, Planck 2018 | t₀ = 13.791 Gyr | 13.787 ± 0.020 Gyr |
| 7 | Rutherford scattering, Monte Carlo | χ² = 38.2 / 35 bins vs exact (35 ± 8.4 expected) | Geiger–Marsden 1913 |
| 8 | pp vs CNO crossover | 17.8–18.1 MK | ≈ 17–18 MK |
| 9 | BBN network (stiff, 12 reactions, 10 MeV → 10⁴ s) | Y_p = 0.2423, D/H = 2.60×10⁻⁵, ⁷Li/H = 5.1×10⁻¹⁰ (SciPy agreement 3×10⁻⁶) | Y_p = 0.2471, D/H = 2.51×10⁻⁵ (Fields 2020). Y_p is 1.9 % low: Born weak rates, as expected. |
| 10 | Nuclear shell model (Woods–Saxon + spin–orbit, Bohr & Mottelson parameters, ~450 eigenvalue problems) | the 7 largest shell gaps at N = 2, 8, 20, 28, 50, 82, 126 (without spin–orbit: 2, 8, 20, 40, 70); ²⁰⁸Pb N = 126 gap 3.54 MeV; rms over 13 levels 0.48 MeV | 3.43 MeV measured; the magic numbers |
| 11 | Hydrogen recombination (Saha + Peebles three-level atom, radau) | z_* = 1089.6 (τ = 1); x_e(200) = 3.9×10⁻⁴ | z_* = 1089.92 ± 0.25 (Planck 2018). Part of the agreement is luck: helium and the multi-level atom are left out, and the README says so. |

Honest caveats:
- The BBN rate coefficients, the Geiger–Marsden table and the ²⁰⁸Pb single-particle energies were typed from memory of the published forms. They are cross-checked (detailed balance, yields against modern codes) but not proof-read against the papers, since there was no network access.

## Phase 3 and 7: textbook gauntlet
- 81 problems in all: 31 in pass 1, 30 in pass 2 and 20 in pass 3 (graduate: Kapitza pendulum, Hulse–Taylor decay, hydrogen fine structure, deuteron, TOV, Gamow peak, …). Each is checked against closed forms, SciPy or published values.
- Friction: 96 items logged, 86 fixed in the language (gauntlet/FRICTION.md). The rest are by design, partly fixed, or open: see "Still weak".

### Gauntlet counts per topic
| Topic | Pass 1 | Pass 2 | Pass 3 (graduate) |
|---|---|---|---|
| mechanics | 3 | 3 | 2 |
| oscillations | 3 | 3 | 2 |
| gravitation | 3 | 3 | 2 |
| thermodynamics | 3 | 3 | 2 |
| electromagnetism | 3 | 3 | 2 |
| optics and waves | 3 | 3 | 2 |
| special relativity | 3 | 3 | 2 |
| quantum | 3 | 3 | 2 |
| nuclear | 3 | 3 | 2 |
| astrophysics | 4 | 3 | 2 |
| **total** | **31** | **30** | **20** |

### Top 10 frictions and what was done
All ten were silent wrong answers (severity W in gauntlet/FRICTION.md), and all ten are fixed and tested.
| # | Friction | Resolution |
|---|---|---|
| 43 | `2 g`, `3 V`, `3 b`, `2 l`: a unit name after a number that is also your variable | D7 revised: an error when combined with other factors, a warning when alone; extended by D130, D170, D171, D180, D231 and D232 |
| 66 | `2 m c²` with a variable m read as the compound unit "m c²" | The collision rule also covers the first factor of a compound unit (D170) |
| 67 | `2 ∫ x dx from 0 to 1 - π` took `- π` into the limit | Warns when both readings have consistent units; errors otherwise (D173, D205) |
| 68 | `8 /m³` divided by a variable m | An error that asks for `[1/m³]` or `8/m³` (D171) |
| 8, 61 | An integral's upper limit swallowed a following `/ x` or `/ (1 + z)` | A spaced `/` ends the limit, or gets a warning or error (D34, D112, D205) |
| 5 | `ω in Hz` printed ω, not ω/2π | Warns, with the numbers for the case (D95, and D202 for the Python boundary) |
| 6 | `(T - 20 °C) in °C` converted a difference as an absolute temperature | Temperature differences are tracked (`tdelta`), and °C in products warns (D12, D181) |
| 40 | The adaptive solver's first step was accepted though far too big (w(1 s) = 6.17, not 1) | A dimensionally consistent Hairer–Wanner first step |
| 39 | A nested integral inside a function read garbage for the function's parameter | Captures propagate through enclosing integrands |
| 2 | Algebraic `solve` could return a later root when the ends already bracketed one | The 200-point scan always runs first, and poles are detected (D32, D95) |

## Phase 8: red team (REDTEAM.md)
| Round | Findings | Silent wrong answers | Fixed |
|---|---|---|---|
| 1 (03:35) | 10 | Hz/rpm, the pole returned as a root, coarse RK4, °C sums | 8; #1 (narrow peaks far out on infinite ranges) partly, #7 (sums over-claim figures) partly, see D95/D11 |
| 2 (05:00) | 14 | `2 c` with your own c, Crank–Nicolson coarse step and sawtooth, `std` of one value, cyclotron frequency, Bateman NaN | all 14 |
| 3 (06:30) | 15 | `36 km/h` read as km / Planck's h, °C in products, `°C ± %`, early PDE transients, vector zero integrals | all 15 |
| 4 (07:30) | 18 (6 false positives from tonight's new rules) | `absolute 1e-6 °C` read as 274 K, rpm through a Python unit contract, `1.5 kT` as kilotesla | all 18 (#16 documented: ties round half to even) |
| 5 (08:45) | 19 (tools: REPL, Jupyter, LSP, fmt, build, playground, bootcamp journey) | Jupyter/playground dropping run-time warnings, `d/dt x(2 s)` = 0, `[1, 2, 3] m` with your own m | all 19 (+3 of 4 nits) |
| 6 (10:00) | 7, all silent wrong answers in the newest code | matrix/vector noise-zeroing hid real entries (a Minkowski metric), the integral noise snap zeroed real small integrals, the PDE start-up skip hid a 20 % error, a near-degenerate eigenpair mixed, `d/ds h(2 s)`, `∫ 3 s^2 ds` | all 7: #1–#4 reverted (D230), #5–#7 fixed (D231–D233) |
| 7 (11:15; cut short by a container restart, findings salvaged) | 5 | eigenpair symmetrisation hid a small asymmetry (a tilted double well), `293.15 K + 0.5 K` printed `290 K`, very early PDE times, rounding-level integrals | #1, #3, #4 fixed; #2 and #5 documented as known limitations |

## Benchmarks
The final table was measured at 10:21 UTC with nothing else running. Earlier runs had a stray process of mine using one of the 4 cores since 00:27; it was found and killed. The numbers are medians of 7 interleaved runs, with the same algorithms, tolerances and problem sizes in every language. Full table: benchmarks/RESULTS.md; reproduce with `python3 benchmarks/run.py --interleave -r 7 --langs fermium,fermium-base,julia,python,numpy`.

| Benchmark | Fermium (compute) vs Julia | Before M5 | Pure Python vs Julia |
|---|---:|---:|---:|
| N-body, 1M steps | **0.95×** (matches) | 2.37× | 55× |
| All-pairs forces, N = 2000, 4 threads (`parallel for` vs `Threads.@threads`) | **0.65×** (faster) | 5.74× | 54× (against Julia on 1 thread) |
| the same, 1 thread | 1.09× | 2.40× | |
| Damped spring, adaptive RK45, rtol 10⁻⁶ | **0.86×** (faster; different step controller, similar accuracy) | 0.84× | 20× |
| Blackbody integrals, rtol 10⁻¹⁰ | 1.27× | 1.29× | 20× |
| Damped spring, fixed RK4, 1M steps (Fermium stores the whole 40 MB trajectory) | 1.93× | 2.00× | 21× |
| Loop with units | 1.54× | 1.64× | 79× |

- **Whole-process time, including startup and JIT:** Fermium finishes before Julia on every benchmark (0.18–0.47 s against 0.28–2.9 s, startup included).
- **The M5 goal** (match or beat Julia on ≥ 2 benchmarks) is met on 3.
- **Where it loses:** the 3 slower rows are explained in RESULTS.md and D151: page faults from storing the trajectory, the D44/D45 bookkeeping per quadrature node, and bounds checks in the unit loop.

## Still weak (honest list)
- **Uncertainties run in the reference interpreter** (slower than native), and aren't available in the REPL, Jupyter or `fermium build`. Vectors of uncertain values are refused.
- **Eigenvalue problems, PDEs, stiff solvers and FFT are Python-backed** (NumPy/SciPy), so `fermium build` refuses the first three.
- **Integration can still miss a narrow feature that is only partly sampled.** Only the all-zero case warns (D110). A result at the integrand's rounding level prints more figures than are right (red team 7 #5).
- **1-D PDEs right after a jump** in the initial or boundary data are inaccurate until diffusion reaches one grid cell (red team 7 #2): use a finer `grid`.
- **Significant figures of sums** use the most precise operand, not the decimal-place rule, which would need the magnitudes at compile time. So `1.00 m - 0.999 m` over-claims (D95, D11).
- **The unit-after-number rule is still the sharpest edge:** `2 g`, `8 K`, `2 b` and `0.25 T` collide with common variable names. They are caught (errors or warnings), but they cost time: 7 of 20 graduate problems hit one.
- **Matrices go up to 16×16,** with one unit per matrix. There are no lists of vectors, and `solve` can't take a list of unknowns.

## How to check this report yourself
```
pip install -e .
make check                                   # lint + the whole test suite (~15 min on 4 cores)
fermium run examples/28_natural_units.fm     # or any of the 31 examples
python3 benchmarks/run.py --interleave -r 7  # the benchmark table (needs Julia in .tools/)
python3 -m pytest -q tests/test_research.py  # the 11 research reproductions against SciPy/NumPy
python3 -m pytest -q tests/test_showcase.py  # every SHOWCASE.md snippet prints what the page says
```

## What I would do next
1. **Native uncertainties:** carry the partial derivatives in LLVM code, so `±` programs aren't limited to the interpreter.
2. **A second, independent quadrature check for integrals:** Gauss–Kronrod on a shifted grid. It would catch narrow features that are only partly sampled, the last known way an integral can be silently wrong.
3. **One consistent rule for unit names that are also variables:** the D7 family grew case by case tonight (D130, D170, D171, D180, D203, D222, D231, D232). A single documented rule in the checker would be simpler to learn.
4. **Per-element significant figures for written lists and vectors.**
5. **Lists of vectors, and `solve` with a list of unknowns:** reaction networks and N-body problems written as loops.
