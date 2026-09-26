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
| `fit` | v1's `fitting.py` flow (power-of-ten and 1-2-5 guess scans, two starts, covariance (JᵀJ)⁻¹·rss/dof, D262 degeneracy test) over a port of MINPACK `lmder` (SciPy 1.17's `least_squares(method='lm')`, x_scale = abs(p0), tolerances 1e-14) with SciPy's 2-point Jacobian | v1's `least_squares_fit` on 10 data sets (line, exponential with and without guesses, Gaussian + offset, power law, SI-scale 1e-19/1e-6, degenerate A·B, damped cosine, exact fit, n = k) | parameters within 1.2e-9 relative, standard errors within 1.9e-8, rms within 1e-9; degenerate/exact cases identical |
| `roots` | v1's algebraic `solve` (`fm_root`: first sign change in 200 sub-intervals, Illinois, pole and rounding-noise detection); SciPy's `brentq` | `fermium.interp.root`, `scipy.optimize.brentq` on 12 cases | bit-identical (roots, warnings, errors) |
| `eigen` | port of v1's `eigen.py` (matrix method with Richardson and Numerov inverse iteration + extrapolation; shooting with Numerov + Sturm bracketing + brentq; jumps; degenerate pairs D233) with native Sturm bisection + inverse iteration (for `eigh_tridiagonal`) and banded LU (for `solve_banded`) | v1's `eigen_solve` on 7 problems × 2 methods (oscillator, box, finite well, Coulomb, symmetric/tilted double well, Morse) | shooting: energies bit-identical, states within 2e-11 of max; matrix: energies within 4e-12 of max abs(E), states within 1e-11 |
| `linalg` | v1's `linalg.py` (to 4×4) and `linalg_big.py` (to 16×16): det, solve, inverse, Jacobi eigen, generalized eigen | FloatOps on 12 matrices 2×2 … 16×16 | bit-identical |
| `fft` | mixed-radix Cooley–Tukey (4, 2, 3, 5, odd primes ≤ 97) and Bluestein, octant-reduced twiddles; `spectrum` = v1's `spectral.py` kinds 0–8 | numpy.fft (pocketfft) via v1's `spectrum`, 20 lengths 1 … 4096 incl. primes 1031, 2003 × 9 kinds | worst difference 6.3e-16 of the output's L2 norm |
| `special` | besseli, besselk, ellipk/ellipe: v1's own algorithms; erf, erfc, gamma, lgamma, jn, yn: the pure-Rust `libm` crate (musl/fdlibm) | glibc (what compiled v1 calls) and v1's special.py; SciPy as an independent check; 838 points | I, K, K(m), E(m): bit-identical (358 values); libm functions: within 1e-15 relative of glibc (J, Y: 4.3e-16 absolute near zeros), 441 of 463 bit-identical; vs SciPy ≤ 2e-14 |
| `rng` | v1's xoshiro256** + splitmix64, rand/randn (Box–Muller)/seed | `fermium/rng.py`, 11 seeds × 60 draws | bit-identical stream |
| `pde` | port of v1's `pde.py` (θ-method with SDIRK2 start, step doubling D130/D131, leapfrog waves, jump averaging, Neumann ghost points); complex tridiagonal LU for `splu` | v1's `pde_solve` on 9 problems + 1 error (heat CN/jump/Neumann+warning/implicit with source/explicit, Schrödinger free/barrier, wave, damped wave) | same snapshot counts and warnings; u within 2.5e-13 of max abs(u), u_t within 3e-13; messages identical |
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

## fit

- `least_squares_fit(resid, n, guess)`: `resid(p, out)` writes model − left side; `guess[i] = None` for no
  starting guess. Returns params, standard errors (None where they can't be estimated, as v1), rms,
  covariance. `fit_sigfigs` is v1's digits rule for printing.
- The optimum is defined to about 1e-10 by the tolerances (v1 itself differs by 1e-10 between two starts).

## eigen, linalg

- `eigen_solve(rhs, a, b, N, grid, Matrix | Shooting)` with `rhs(x, ψ, ψ', E) = ψ''`; `tridiag_lowest`
  and `band_solve` are usable on their own.
- `linalg`: `det`, `solve`, `inverse`, `eigen_sym`, `eigen_general` (with v1's error kinds 15, 21, 22),
  and the raw `jacobi_eigen` / `generalized_eigen`.

## fft, special, rng, pde

- `fft`, `fft_backward`, `ifft` on `C64` slices; `spectrum(kind, a, b, dt)` for the built-ins.
- `special`: plain f64 functions; orders are f64, NaN unless whole (v1's `order_ok`).
- `rng::Rng`: `default()` is seed(0); `seed`, `rand`, `rand_range`, `randn`, `randn_ms`.
- `pde_solve(probe, xa, xb, t0, t1, PdeOpts)`; warnings (8: step too coarse, 9: step cap) in the result.
