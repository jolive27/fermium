# fermium-runtime numerics: methods and accuracy

Fermium 1.5 (`fermium/`, Python) is the oracle. Where v1 implements an algorithm itself, the Rust
code ports it operation for operation, so results agree to rounding (usually bit for bit). Where
v1 called SciPy/NumPy, the Rust code is a native method validated against SciPy/NumPy.

Fixtures: `python3 rust/tools/numerics_fixtures.py` (Python only generates them; nothing at run time
needs Python) writes `tests/fixtures/*.txt`; `cargo test -p fermium-runtime` compares.

| Module | Method | Reference | Measured agreement |
|---|---|---|---|
| `ode` | v1's RK4 (+ step-doubling check, kind-7 warning) and Dormand–Prince 5(4) (Hairer–Wanner first step, D17 norm, jump location D40, stiffness detection, `until` events on DOPRI5 dense output, D39), port of `fm_rk4`/`fm_dp45`; `Sol` (cubic Hermite, `extreme` = `fm_sol_ext`) | `fermium.interp.dp45`/`rk4`/`rk4_error`/`Sol`/`sol_ext` on 18 cases (decay, oscillators, Van der Pol, Kepler, Lorenz, backwards, events, a jump in t, absolute tolerances, stiffness warning after 100 000 steps, errors) | all 281 numbers (step counts, end states, interpolated values and slopes, max/min, warnings, error kinds and places) bit-identical |
| `stiff` | native Radau IIA (order 5) and BDF (orders 1–5): ports of SciPy's `Radau`/`BDF` (Newton iteration, error estimate, step/order control, `num_jac`, dense output) with our partial-pivoting LU (real and complex), under a port of v1's `stiff_solve` (tolerance mapping D17/D42/D160, events, error kinds) | v1's `stiff_solve` over SciPy 1.17 on 10 cases × 2 methods (Robertson at rtol 1e-6 and 1e-9, Van der Pol μ = 1000, stiff linear, decay chain, event, backwards, absolute tolerance, blow-up, missing event) | same step count in 15 of 16 solved cases (Van der Pol/Radau: 1342 vs 1343); largest difference in the solution relative to its scale 5e-10 (stiff linear/Radau, rtol 1e-8), typically 1e-16 … 1e-12; errors: same kind, same place to 1e-5 |
| `quad` | v1's adaptive Gauss–Kronrod 7-15 with smoothstep / u/(1−u) maps, D44/D45/D110 (port of `fm_quadcore`) | `fermium.interp.quad` on 31 integrands (finite, infinite, singular, oscillatory, NaN/∞, errors) | `quad_v1`: every value bit-identical to v1 (27/27), every error the same kind and place (4/4). `quad` (with B2): bit-identical on all 26 cases v1 got right; the 5 v1 failures give the truth to 1e-13 … 2e-9 |

## quad

- `quad(f, a, b, rtol, atol, name)`: v1's defaults are rtol 1e-10, atol 0. Returns the value, the
  error estimate, ∫|g| (for the D110 "exactly 0" warning) and `all_zero`.
- `quad_v1`: v1 exactly, without the B2 improvements (see NOTES.md for the intentional differences).
- Libm: Rust's `exp`, `sin`, `powf`, … call the platform libm like CPython, so integrands give the
  same values; bit-identical results were observed on Linux/glibc.

## ode

- `rk4(f, y0, t0, t1, h0, event, opts)`, `dp45(f, y0, t0, t1, event, opts)`; `f(t, y, out)` writes y'.
  `OdeOpts { rtol, atol, tname, evtext, tdep }` carries v1's options and the text ids for errors.
- Warnings v1 raises while solving are returned in `Sol::warnings` as (kind, a): kind 2 (looks
  stiff: after `STIFF_AFTER` = 100 000 steps, Hairer's test 15 times in a row), kind 7 (RK4 too
  coarse: estimated relative error > 1e-3 by step doubling).
- Python's `max`/`min` keep the first argument on ties and NaN; `pymax`/`pymin` reproduce that, so
  NaN paths behave as in v1.

## stiff

- `stiff_solve(f, y0, t0, t1, StiffMethod::Radau | Bdf, event, opts)`.
- Differences from SciPy are only in the order of floating-point operations (our LU vs LAPACK's
  getrf, sums vs BLAS dot); they are at the rounding level and only rarely change a step decision.
