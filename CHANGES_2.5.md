# What changes in Fermium 2.5 (in progress)

Fermium 2.5 grows the language on the Rust compiler of 2.0 (spec Phase C). Programs that ran with 2.0 print
exactly the same: the conformance suite still holds every program to Fermium 1.5's output (3334 pass, 32
documented divergences). This file lists each Phase C item as it lands.

## Several versions of one function: multiple dispatch (C5, DECISIONS D285)

One function name can have several versions, and each call uses the one its arguments fit, by their number,
units and kind. The choice is made when the program is checked, so it costs nothing at run time:

```text
energy(m [kg], v [m/s]) = ½ m v²
energy(λ [m]) = h c / λ
energy(f [Hz]) = h f
print energy(2 kg, 3 m/s), energy(500 nm) in eV, energy(1 GHz)   # 9 J 2.48 eV 6.63×10⁻²⁵ J

size(r: vector) = |r|          # a parameter can name a kind: number, vector, list or complex
size(xs: list) = len(xs)
```

- The most specific version wins (an annotated parameter beats an unannotated one). No fitting version, or two
  that fit equally well, is a one-line error before the program runs that lists the versions with their lines.
- Works in generic functions (each call chooses again), with derivatives (`energy'`, `d/dx U`), `∫`, `plot`,
  functions passed to functions and modules (`photons.energy(500 nm)`); the editor's hover shows the version a
  call uses.
- A definition with the *same* signature still replaces the earlier one, so every existing program prints the
  same. Reference: [docs/reference.md](docs/reference.md), *Several versions of one function*.
- Not yet: differentiating a formula that calls a function with versions, adding versions to an imported
  function, the Python API (uses the last version), Fermium 1.5.

## C++ interop (C4, DECISIONS D290)

Fermium calls C++ functions, with the units checked at every call before the program runs, through a small
`extern "C"` wrapper it generates and compiles with the system's C++ compiler (cached, so only the first run
compiles):

```text
import cpp "libkinematics.so" header "kinematics.hpp":
    kin::TwoBody::momentum(M [MeV/c²], m1 [MeV/c²], m2 [MeV/c²]) -> [MeV/c]   # a static member function
    kin::invariant_mass(E [MeV], p [MeV/c]) -> [MeV/c²] as mass_of           # one overload …
    kin::invariant_mass(E1 [MeV], p1 [MeV/c], E2 [MeV], p2 [MeV/c], cosθ) -> [MeV/c²] as pair_mass  # … another
import cpp header "cmath":
    std::tgamma(x) -> number
print momentum(139.57039 MeV/c², m_μ, 0 MeV/c²), tgamma(5)    # 29.79 MeV/c 24
```

- The signatures are those of `import c`, with namespaced names; the declared signature picks the overload
  (or instantiates a function template), and `as` names it in the program. Static member functions work;
  ordinary member functions are refused with a message that says why.
- A C++ exception stops the program with its message; compile-time problems (a misspelt function, no overload
  with the declared types, a symbol the library doesn't define, a missing header, no C++ compiler) are one line
  with a caret on the signature.
- The compiler is `$CXX` or the first of c++, g++, clang++; wrappers are cached in `~/.cache/fermium/cpp` (or
  `$FERMIUM_CACHE_DIR/cpp`) and rebuilt when a header or the library changes.
- Worked example: [examples/cpp_interop/](examples/cpp_interop/) computes two-body decay momenta (π⁺ → μ⁺ ν:
  29.79 MeV/c, as the PDG gives), decay lengths and the invariant mass of the Λ in C++. Reference:
  [docs/reference.md](docs/reference.md), *C++ interop*.
- Not yet: objects and ordinary member functions, references, `std::vector` and strings, types other than
  `double`, `int` and arrays of doubles, functions returning nothing, explicit template arguments. C++ calls
  take about 0.6 µs each (they go through the run time to check for exceptions). Tested on x86-64 Linux with
  g++; macOS is untested.

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
