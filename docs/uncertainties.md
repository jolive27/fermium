# Uncertainties (`5.0 ± 0.2 m`): how they are implemented

User documentation: [reference §21](reference.md#21-uncertainties--error-propagation-monte-carlo) and [bootcamp Lesson 12](../bootcamp/lesson12_lab_report.md). Design decisions: DECISIONS.md D120–D124. Tests: `legacy/tests/test_uncertainty.py`.

## Pieces

| Piece | Where | What it does |
|---|---|---|
| `±` / `+-` | `lexer.py` (`OP_CANON`), `parser.py` (`pm_term`) | Binds tighter than `+`/`-`, looser than `*`/`/`. `5.0 ± 0.2 m` gives the unit to the bare 5.0; `(5.0 ± 0.2) m` is a quantity; `x ± 3%` is relative. |
| AST | `ast.Uncertain(value, err)`, `ast.Propagate(samples, body)` | |
| Checker | `checker.e_Uncertain`, `unc_part`, `s_Propagate` | Unifies the two dimensions (so units stay checked), builds `IBuiltin("pm" / "pm_rel" / "unc_value" / "unc_uncertainty" / "unc_rel")` and `SPropagate`, and sets `CheckedModule.uses_unc`. |
| Values | `uncertain.py::UFloat` | Nominal value + `{source id: ∂f/∂xᵢ·σᵢ}`; Python operators do first-order propagation with exact correlations. `__float__` raises `UncertainUse`, so nothing drops an uncertainty silently. |
| Running | `driver.Program` → `interp.Interpreter` | A program with `uses_unc` runs in the reference interpreter (not LLVM). `fermium build` and the REPL refuse it. |
| Monte Carlo | `interp.s_SPropagate`, `_Sampler` | Standard normals per source from the seeded generator (D80); the block runs once on NumPy arrays, or once per sample if it can't; results = mean ± std, linked back to the sources by regression. |
| Printing | `uncertain.format_pm`, `runtime/core.py` `print_num`/`print_list` | σ to 2 significant figures, the value to the same decimal place; a shared power of ten when needed. |
| Fit | `runtime/fitting.py` (covariance), `interp.s_SFit`, `uncertain.correlated` | In a `uses_unc` program the parameters get the full covariance (one source per eigenvector). |
| Plot | `interp.s_SPlot`, `runtime/core.py` `make_plot` | Error bars for uncertain lists; a ±1σ band for an uncertain curve. |

## Not done (yet)

- Native code: uncertain values would need a run-time sparse map in LLVM IR and in the C runtime of `fermium build`.
- The REPL and the Jupyter kernel (their variables live in a native arena of doubles).
- Vectors/matrices of uncertain values; integrals, ODEs and `solve … for x` with uncertain inputs outside `propagate montecarlo`.
- Weighted fits (σ of each data point) and reading an uncertainty column from a CSV header automatically (write `data.T ± data.dT` instead).
