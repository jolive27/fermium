# What changes in Fermium 2.5 (in progress)

Fermium 2.5 grows the language on the Rust compiler of 2.0 (spec Phase C). Programs that ran with 2.0 print
exactly the same: the conformance suite still holds every program to Fermium 1.5's output (3326 pass, 40
documented divergences). This file lists each Phase C item as it lands.

## C and Fortran interop (C3, DECISIONS D275)

Fermium calls functions in C and Fortran shared libraries through the C ABI, with the units checked at every
call before the program runs:

```text
import c "libphys.so":
    kinetic_energy(m [kg], v [km/s]) -> [J]
    twice(n: int) -> int
    sum_sq(x: list [m], n: len(x)) -> [m²]
import fortran "libnuclear.so":
    binding_energy(Z: int, A: int) -> [MeV]                      # the symbol binding_energy_
    neutron_separation(Z: int, A: int) -> [MeV] bind(C, name="semf_sn")
print kinetic_energy(2 kg, 3000 m/s), binding_energy(26, 56)
```

- The signatures are those of `use python`, plus arrays (`x: list [m], n: len(x)`) and Fortran's `bind(C)`.
  Arguments are passed in the declared units and the result converted back; `kinetic_energy(2 kg, 3 s)` is a
  compile-time error with a caret and a hint. Fortran arguments go by reference, with gfortran's name mangling.
- The library and every function are looked up when the program is checked: a missing library or a misspelt
  function is a compile error that says how to fix it.
- A function of doubles is called directly from the compiled code (about 5 ns per call); `fermium build`
  executables can call C and Fortran too. There is no libffi or C compiler involved.
- Worked example: [examples/c_interop/](examples/c_interop/) calls a Fortran semi-empirical mass formula and a
  C Gamow-peak routine (6.09 keV for p + p at 15.7 MK). Reference: [docs/reference.md](docs/reference.md),
  *C and Fortran interop*.
- Not yet: output arrays, `float`/`long`, structs, strings, callbacks, functions returning nothing, `import c`
  in modules or calls inside `units natural`, the browser playground.

## Uncertainties everywhere (spec C7)

- **Vectors and matrices of uncertain values** print and work: `<1.0 ± 0.1, 2.0 ± 0.2> m` prints
  `<1.00 ± 0.10, 2.00 ± 0.20> m`; `|v|`, `unit`, `abs`, `≈`, components, `·`, `×`, `det`, `inverse` and matrix
  products propagate with correlations. In 2.0 building or printing one stopped with "needs a plain number" or
  "vectors and matrices of uncertain values aren't supported yet".
- **Lists of uncertain values** work in `std` and `interp` too (the other list functions already did).
- **Integrals with uncertain parameters or limits** give an uncertain result, propagated exactly (the derivative
  under the integral sign and through the limits). 2.0: "an integral can't use uncertain values (±) yet".
- **ODE solutions with uncertain starting values, start time or parameters**: the sensitivity equations are solved
  alongside, so `y(t)` prints value ± uncertainty with its correlations kept. 2.0: "a starting value of solve can't
  be uncertain (±) yet" / "a differential equation (solve) can't use uncertain values (±) yet".
- **Monte Carlo when linear isn't valid:** each error source is tested at ±1σ; an integral or solve that isn't
  close to linear there is computed by Monte Carlo instead (seeded, with a warning).
- Still errors: `solve … for x`, eigenvalue problems and PDEs with uncertain inputs (use `propagate montecarlo`).
- Docs: docs/reference.md §21; DECISIONS D276–D279; tests: rust/c-cases/c7 (run by
  `rust/crates/fermium-cli/tests/c7_cases.rs`).

## Calculus reach (C2, DECISIONS D295)

- **Derivatives of functions written over several lines** (automatic differentiation, D295): `f'`, `f''`,
  `d/dx f`, `∂/∂v E`, `∇φ` and `∇²φ` work through assignments, `if`/`else`, `for` and `while`, exact to rounding.
  A one-line function that calls a multi-line one can be differentiated too. 2.0: "can't differentiate through g:
  it's defined over several lines". Not differentiated: lists whose elements depend on the variable, and `solve`,
  `plot`, `fit` or `propagate` inside the function (errors naming the statement); `∇·` and `∇×` still need a
  one-line vector formula.

```text
root(a) =
    r = a
    for i from 1 to 30
        r = (r + a/r)/2      # Newton's iteration for √a
    r
print root'(4), root''(4)    # 0.250 -0.0312: 1/(2√a) and its derivative
```

- Docs: docs/reference.md §8 *Derivatives of functions written over several lines*; tests: rust/c-cases/c2 (run
  by `rust/crates/fermium-cli/tests/c2_cases.rs`); divergences: rust/DIVERGENCES.md *v2.5: derivatives of
  multi-line functions (C2)*.
