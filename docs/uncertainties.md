# Design: uncertainties (`5.0 ± 0.2 m`), planned for a future version

Spec §3.7 deferred error bars. This note records how they can be added **as an extension, not a rewrite**, and what is already in place.

## Current behaviour

`±` and its ASCII spelling `+-` are reserved operators. Any use gives:

```
line 1: uncertainties (±) are planned for a future version of Fermium
    L = 1.20 ± 0.01 m
             ^
  hint: for now write the value without its uncertainty, e.g. 5.0 m
```

## What is already ready for it

| Piece | Where | Why it helps |
|---|---|---|
| `±` token | `lexer.py` (`OP_CANON`: `±` → `+-`) | Both spellings already lex to one operator. |
| AST node | `ast.Uncertain(value, err)` | Reserved in the AST. The parser only needs to build it instead of raising the error. |
| Types are objects | `types.NumTy(dim)` | An uncertain number is a new flavour: `NumTy(dim, uncertain=True)`. The dimension algebra doesn't change: the value and its σ have the same dimension. |
| Per-expression display info | `ir.Expr.sf`, `.hint` | Printing already chooses a unit and significant figures per expression. `±` printing adds "round σ to 2 significant figures and the value to the same decimal place". |
| Code generation by type | `codegen_llvm.lltype` | An uncertain number becomes the LLVM struct `{double value, double sigma}`, like vectors became `<n x double>`. Arithmetic on it is a new branch in `FuncGen.e_IBin`. |
| Fits | `runtime/fitting.py` already computes standard errors | `fit` can return uncertain parameters directly: `g = 9.806 ± 0.017 m/s²`. |

## Proposed semantics

1. **Literals:** `5.0 ± 0.2 m` means value 5.0 m with σ = 0.2 m. The σ must have the same dimension as the value, and that is checked like `+`. Relative form: `5.0 m ± 4 %`.
2. **Propagation:** first order (linear), assuming independent inputs:
   - `a + b` and `a - b`: σ² = σa² + σb²
   - `a·b` and `a/b`: (σ/|f|)² = (σa/a)² + (σb/b)²
   - `f(a)`: σ = |f′(a)| σa, where f′ comes from the existing symbolic differentiator (`calculus.diff`) at compile time. That gives exact first-order propagation through user functions, which is Fermium's advantage over add-on packages.
3. **Correlations:** a variable used twice (`x - x`) must give σ = 0. First version: track each uncertain value by a *source id*, and carry a sparse gradient vector {source → ∂f/∂source} instead of a single σ, as linear-uncertainty libraries do (for example Python's `uncertainties`). The IR type becomes `{value, grad[k]}`, where k is the number of independent uncertain inputs, which is known at compile time for straight-line code. Loops that create new uncertain values fall back to a runtime sparse map.
4. **Printing:** `9.806 ± 0.017 m/s²`, and `(6.674 ± 0.015)×10⁻¹¹ m³/(kg s²)` in scientific form.
5. **Built-ins:** `value(x)` and `sigma(x)` / `σ(x)` read the parts. `fit` returns uncertain parameters. `load` could read a `σT [s]` column.
6. **Zero cost when unused:** plain numbers keep their `double` representation. Only expressions whose type is uncertain pay for the extra doubles.

## Estimated work

- Parser node and checker rules (unify the σ dimension, new type flavour): half a day.
- Codegen for the `{value, sigma}` struct with independent-input propagation: half a day. The correlated version (gradients): 1–2 days.
- Printing rules and tests against the Python `uncertainties` package on random expressions: half a day.
