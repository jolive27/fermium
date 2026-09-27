# What changes in Fermium 2.5 (in progress)

Fermium 2.5 grows the language on the Rust compiler of 2.0 (spec Phase C). Programs that ran with 2.0 print the
same, except the documented divergences below: the conformance suite still holds every program to Fermium 1.5's
output (3324 pass, 42 documented divergences). This file lists each Phase C item as it lands.

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

- **Conditions on the unknowns are located** (D296): an `if` in the equations of a solve that depends on the
  unknowns (`M x'' = if x > 0 m then -k1 x else -k2 x`, or through a one-line function) is frozen during each RK45
  step and its switch found by root finding on the dense output, so the tolerance holds across it (a piecewise
  spring: 1.5×10⁻⁹ m after three periods instead of 6×10⁻⁸ m). Sliding modes (dry friction at rest, a block on a
  conveyor belt) slide along the switching surface (Filippov), where 2.0 stopped with "the step became too small".
  One conformance program (a white dwarf) changes by 2 units in its 8th digit, towards the converged value (a
  documented divergence).
- **`when` events** (D297): `when y = 0 m: y' = -0.9 y'` in a solve changes the state where the condition is met,
  located on the dense output (a bouncing ball matches the analytic bounces to 10⁻¹³ m); `when y < 0 m` fires only
  on the way down. An event that accumulates (a Zeno point) is an error naming the time. RK45 only; not with
  uncertain values yet.

```text
g = 9.81 m/s²
solve y'' = -g
  with y(0 s) = 10 m, y'(0 s) = 0 m/s
  for t from 0 s to 12 s
  when y = 0 m: y' = -0.9 y'
```

- **Printed derivatives are tidier** (D298): a derivative prints in its tidy form when that is at least a fifth
  shorter: `g''(x) = 2 cos(x) - x sin(x)` (2.0: `cos(x) + (cos(x) - x sin(x))`), `E'(x) = x exp(x)` (2.0:
  `(1 + x - 1)·exp(x)`), `q''(x) = 1/(1 + x²)^(3/2)`, `∂V/∂x = -x/(x² + y² + z²)^(3/2)`. Values are computed as
  before. One conformance program prints `(1 - x²)/(1 + x²)²` instead of `(1 + x² - 2x²)/(1 + x²)²` (documented).
- **Parameter sweeps** (D299): `sweep k in [1, 2, 4] N/m` + a block is a for loop whose plots draw one curve
  per value in one figure, labelled `k = 1 N/m`, … and saved when the sweep ends (a `for` loop's plots overwrite
  each other). `sweep L from 0.5 m to 2 m step 0.5 m` works too.

```text
sweep k in [1, 2, 4] N/m
    solve M x'' = -k x
      with x(0 s) = 1 m, x'(0 s) = 0 m/s
      for t from 0 s to 3 s
    plot x vs t
```

- Docs: docs/reference.md §8 *Derivatives of functions written over several lines* and §10 (*An `if` on the
  unknowns*, *`when`*, *`sweep`*); DECISIONS D295–D299; tests: rust/c-cases/c2 (run by
  `rust/crates/fermium-cli/tests/c2_cases.rs`); divergences: rust/DIVERGENCES.md *v2.5: derivatives of
  multi-line functions (C2)* and *v2.5: conditions on the unknowns in solve are located (C2)* and *v2.5: printed derivatives in their tidy form
  when it is clearly shorter (C2)*.
