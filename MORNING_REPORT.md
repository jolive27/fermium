# Morning report: Fermium, the overnight run of Sep 24–25, 2026

> **DRAFT (07:35 UTC).** The final version, with M5 benchmark numbers, red team round 4 and the final test count,
> will be written at 12:30 UTC. Everything below is backed by tests in `tests/` unless it says otherwise.

## In one paragraph

Fermium is a physics programming language:
- units are checked before the program runs, and cost nothing at run time;
- calculus is part of the language;
- programs compile to native code through LLVM.

Overnight it went from a spec to:
- 3403 passing tests;
- 61 + 20 textbook problems;
- 9 research reproductions compared with published numbers;
- 4 red-team rounds;
- all 8 moonshots attempted: 7 done, and M5 (performance) is in progress.

## User requests during the night

| Request | Status | Evidence |
|---|---|---|
| Complex numbers, for quantum mechanics (review priority 1) | **Done** (D90–D94) | `3 + 4i`, `𝑖`, `polar`, complex ODEs, integrals and PDEs, `fermium build`. tests/test_complex.py (106 tests). |
| The unit-after-number ambiguity (`2 g`, `2 m v`) (review priority 2) | **Done** (D7 revised, then D130, D170, D171, D180) | A unit name right after a number that is also your variable is an error when combined with other factors, and a warning when it stands alone. `2 c`, `2 m c²`, `8 /m³` and `36 km/h` are covered too. tests/test_unit_rule.py, test_gauntlet3_fixes.py, test_redteam2/3.py. |
| Warn when an integral may be silently wrong (review priority 3) | **Done, partly** (D110) | A result that is exactly 0 because every sample was 0 warns, in JIT, interpreter and build (tests/test_integral_warnings.py). A peak that is only partly sampled is still not detected (D110, "Not done"). |
| Stale "Partial" section in PROGRESS.md (review priority 4) | **Done** | Rewritten at 03:40 and kept current. |
| **3 significant figures by default when precision is ambiguous or unspecified, display only** | **Done** (D11) | `1/2` → `0.500`, `2π` → `6.28`, `c` → `3.00×10⁸ m/s`. Values with stated precision keep it; whole numbers and literals print exactly; `to N digits` overrides; `1000000/3` → `3.33×10⁵`. The JIT, the interpreter and `fermium build` agree. tests/test_default_sigfigs.py. A test proves it is display only (`x = 1/3; x * 3 - 1` prints `0`). |

## Phase 1: audit
AUDIT.md (00:05–00:25 UTC) listed the spec checklist, false doc claims, stubs and 56 broken things (A1–A56).
- All were fixed or documented by 02:00 UTC.
- The two that remained as documented limitations were later handled by D110 (the zero-integral warning) and D45 (NaN points).

## Phase 2: features (all six done by 02:00 UTC)
Jupyter kernel and notebook; language server with unit hover, live errors and `\name` completion; VS Code extension; vectors and matrices with units and 3-D vector ODEs; ∂/∂x, ∇, div, curl, laplacian; browser playground (Pyodide); `fermium build` (standalone executables, including load/fit/plot to SVG).

## Phase 5: moonshots

| | Moonshot | Status | Evidence |
|---|---|---|---|
| M1 | Natural units (`units natural(ħ = c = 1)`, `nuclear`, `astro`) | Done (D60) | tests/test_natural_units.py; examples/28 |
| M2 | Dimensional analysis (`analyze …: T [s] depends on …`) | Done (D70), bootcamp lesson 11 | tests/test_dimensional_analysis.py |
| M3 | Serious numerics: stiff solvers + stiffness warning, eigenvalue problems, 1-D PDEs (heat, wave, Schrödinger; GIF), FFT, root finding, Monte Carlo, seeded RNG | Done (D42, D80–D83, D131, D160) | tests/test_m3_*.py, test_stiff.py; all validated against SciPy/NumPy or closed forms |
| M4 | Uncertainties (`5.0 ± 0.2 m`, correlations, `propagate montecarlo`, uncertain fit parameters, error bars), bootcamp lesson 12 | Done, **interpreter only** (D120–D124) | tests/test_uncertainty.py, checked against the `uncertainties` package and SciPy `curve_fit` covariances |
| M5 | Performance: `parallel for`, optimisation pass, match Julia on ≥ 2 benchmarks | **TBD at 12:30** | RESULTS.md |
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
| 7 | Rutherford scattering, Monte Carlo | χ² = 35.6 / 35 bins vs exact | Geiger–Marsden 1913 |
| 8 | pp vs CNO crossover | 17.8–18.1 MK | ≈ 17–18 MK |
| 9 | BBN network (stiff, 12 reactions, 10 MeV → 10⁴ s) | Y_p = 0.2423, D/H = 2.60×10⁻⁵, ⁷Li/H = 5.1×10⁻¹⁰ (SciPy agreement 3×10⁻⁶) | Y_p = 0.2471, D/H = 2.51×10⁻⁵ (Fields 2020). Y_p is 1.9 % low: Born weak rates, as expected. |

Honest caveats:
- The BBN rate coefficients and the Geiger–Marsden table were typed from memory of the published forms. They are cross-checked (detailed balance, yields against modern codes) but not proof-read against the papers, since there was no network access.

## Phase 3 and 7: textbook gauntlet
- 81 problems in all: 31 in pass 1, 30 in pass 2 and 20 in pass 3 (graduate: Kapitza pendulum, Hulse–Taylor decay, hydrogen fine structure, deuteron, TOV, Gamow peak, …). Each is checked against closed forms, SciPy or published values.
- Friction: 86 items logged, 74 fixed in the language (gauntlet/FRICTION.md). The remaining open ones: see "Still weak".

## Phase 8: red team (REDTEAM.md)
| Round | Findings | Silent wrong answers | Fixed |
|---|---|---|---|
| 1 (03:35) | 10 | Hz/rpm, the pole returned as a root, coarse RK4, °C sums | all |
| 2 (05:00) | 14 | `2 c` with your own c, Crank–Nicolson coarse step and sawtooth, `std` of one value, cyclotron frequency, Bateman NaN | all 14 |
| 3 (06:30) | 15 | `36 km/h` read as km / Planck's h, °C in products, `°C ± %`, early PDE transients, vector zero integrals | all 15 |
| 4 (07:30) | TBD | TBD | TBD |

## Benchmarks
TBD at 12:30 (M5): measured on a quiet machine, Fermium vs Julia vs Python, same algorithms and tolerances.

## Still weak (honest list)
- **Uncertainties run in the reference interpreter** (slower than native), and aren't available in the REPL, Jupyter or `fermium build`. Vectors of uncertain values are refused.
- **Eigenvalue problems, PDEs, stiff solvers and FFT are Python-backed** (NumPy/SciPy), so `fermium build` refuses the first three.
- **Integration can still miss a narrow feature that is only partly sampled.** Only the all-zero case warns (D110).
- **The unit-after-number rule is still the sharpest edge:** `2 g`, `8 K`, `2 b` and `0.25 T` collide with common variable names. They are caught (errors or warnings), but they cost time: 7 of 20 graduate problems hit one.
- **Matrices stop at 4×4** (a fix is in progress), and there are no lists of vectors.
- **The machine was heavily loaded all night,** so benchmark numbers need the quiet-machine re-run.
