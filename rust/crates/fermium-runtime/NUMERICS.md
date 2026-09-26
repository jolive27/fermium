# fermium-runtime numerics: methods and accuracy

Fermium 1.5 (`fermium/`, Python) is the oracle. Where v1 implements an algorithm itself, the Rust
code ports it operation for operation, so results agree to rounding (usually bit for bit). Where
v1 called SciPy/NumPy, the Rust code is a native method validated against SciPy/NumPy.

Fixtures: `python3 rust/tools/numerics_fixtures.py` (Python only generates them; nothing at run time
needs Python) writes `tests/fixtures/*.txt`; `cargo test -p fermium-runtime` compares.

| Module | Method | Reference | Measured agreement |
|---|---|---|---|
| `quad` | v1's adaptive Gauss–Kronrod 7-15 with smoothstep / u/(1−u) maps, D44/D45/D110 (port of `fm_quadcore`) | `fermium.interp.quad` on 31 integrands (finite, infinite, singular, oscillatory, NaN/∞, errors) | `quad_v1`: every value bit-identical to v1 (27/27), every error the same kind and place (4/4). `quad` (with B2): bit-identical on all 26 cases v1 got right; the 5 v1 failures give the truth to 1e-13 … 2e-9 |

## quad

- `quad(f, a, b, rtol, atol, name)`: v1's defaults are rtol 1e-10, atol 0. Returns the value, the
  error estimate, ∫|g| (for the D110 "exactly 0" warning) and `all_zero`.
- `quad_v1`: v1 exactly, without the B2 improvements (see NOTES.md for the intentional differences).
- Libm: Rust's `exp`, `sin`, `powf`, … call the platform libm like CPython, so integrands give the
  same values; bit-identical results were observed on Linux/glibc.
