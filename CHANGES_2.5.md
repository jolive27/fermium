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
