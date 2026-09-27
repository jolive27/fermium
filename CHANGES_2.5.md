# What changes in Fermium 2.5 (in progress)

Fermium 2.5 grows the language on the Rust compiler of 2.0 (spec Phase C). Programs that ran with 2.0 print the
same, except the documented divergences below: the conformance suite still holds every program to Fermium 1.5's
output (3324 pass, 42 documented divergences). This file lists each Phase C item as it lands.

## Performance (C6, DECISIONS D310)

Compiled programs are faster where the loop vectorizer can now work, and a program runs from a cache of its
compiled code when neither it nor its modules changed. Every printed number stays bit for bit what it was: no
fast-math, no reassociation.

- **Vectorized loops, exactly** (D311, D312, D313). A loop whose floating-point sum must keep its order
  (`U += …`) is vectorized with the sum still added term by term in the original order; an `if` inside a loop
  that only assigns numbers is compiled without a branch (`v + (c ? e : −0)`, exact for every value, ±0 and NaN
  included); loops that only read lists no longer stop at a collector safe point. The all-pairs `forces`
  benchmark's inner loop now runs 4 pairs at a time: about 2× faster on one thread and on four (A/B on a shared
  machine; the quiet-machine table in benchmarks/RESULTS.md is the reference).
- **Fixed-step RK4 with a stored solution** (D316): the solution's sample arrays are mapped in one go (huge
  pages where Linux has them) instead of one page fault per 4 KiB, which cost as much as the steps themselves in
  `spring_rk4` (≈ 20 % faster there, median).
- **Smaller things** (D314, D315): functions that call no function skip the runaway-recursion check (so they
  inline as plain arithmetic); `exp`, `ln`, `sin`, `cos` call the C library directly (the same functions, so the
  same numbers).
- **The compile cache** (D317). `fermium run` keeps the machine code of a program in `~/.cache/fermium/jit`
  (`$FERMIUM_CACHE_DIR/jit`, `$XDG_CACHE_HOME/fermium/jit`), keyed by the program's text, the `fermium` binary
  and the CPU, and checked against every module file and `fermium.toml` the compilation read or looked for. The
  next run of the same program skips parsing, checking and LLVM: a program that imports the whole standard
  library takes 8–9 ms from start to finish instead of 25–28 ms (the rest is starting the binary). A changed module, a module added where an import looks first,
  a damaged entry or a new `fermium` binary make it compile again. Not cached: programs that use Python, C or C++,
  read data files when checked, or have constructs the tree-walker runs. `FERMIUM_NO_CACHE=1` turns it off.
- Not done: blackbody stays ≈ 1.5× Julia (its time is the quadrature's bookkeeping around each integrand call and
  glibc's `exp`; a batched integrand is in BACKLOG), and `fermium build` doesn't use these caches.

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
- A definition replaces every earlier version it covers (the same number of parameters, each at least as broad:
  `f(x) = 2 x` then `f(x) = 3 x`, or `force(x [m]) = …` then `force(x) = …`), so every existing program prints the
  same; a more specific definition written later adds a version. Replacing `E(f [Hz])` by `E(ω [rad/s])` (the
  same dimension, a different meaning) warns. A parameter declared `: list` takes the list whole. Reference: [docs/reference.md](docs/reference.md), *Several versions of one function*.
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
  function is a compile error that says how to fix it. `fermium check` and the editor's language server read the
  function names from the library file without loading it, so no library code runs just from checking.
- Arguments are plain numbers: an uncertain value (±) is an error, as for `use python` (write `value(x)`, or call
  the function inside `propagate montecarlo`, which calls it once per sample).
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
  close to linear there (including a jump at a measured value, `∫ (if x < a then 1 else 0) dx`) is computed by
  Monte Carlo instead (seeded, with a warning). The value shown is the one at the measured inputs; all values of a
  Monte Carlo solution share its samples.
- **`use python`** refuses an uncertain argument again, as Fermium 1.5 does ("this operation needs a plain
  number, but got an uncertain value (±)"); 2.0 had passed the value alone.
- Still errors: `solve … for x`, `solve` with a list of unknowns, eigenvalue problems and PDEs with uncertain
  inputs (use `propagate montecarlo`); arrays (`fill`) of uncertain values.
- Docs: docs/reference.md §21; DECISIONS D276–D279; tests: rust/c-cases/c7 (run by
  `rust/crates/fermium-cli/tests/c7_cases.rs`).

## C1. Memory and data structures

- **Lists are freed in compiled code too.** The LLVM back end and `fermium build` executables used to keep every
  list until the program ended (as Fermium 1.5's compiled code did), so a long loop that made lists grew without
  bound. They now run a collector at the top of loop iterations that make lists (D280): a loop that makes a million
  lists of 100 numbers stays under 100 MB instead of 800 MB; texts made in a loop (`"run " + str(i)`) and arrays are
  freed the same way. `FERMIUM_GC_STATS=1` reports it. (The tree-walker's
  lists were already reference counted.)
- **Lists of vectors, matrices, complex numbers and text** (D281): `[<1, 2> m, <3, 4> m]`, a list of matrices
  (`[[[1, 0], [0, 1]], [[0, 1], [1, 0]]] N/m`, or built with `push`), `[1 + 2i, 3i]`, and `[]` that becomes the kind
  of the first value pushed onto it. Indexing, `xs[i] = …`, `len`, `for … in`, `clear`, `print`, a number times the
  list, `sum` and `mean` work; units and sizes are checked per element. An N-body step can be written as loops over
  lists of position and velocity vectors ([docs/reference.md](docs/reference.md#lists-of-vectors-matrices-complex-numbers-and-text-fermium-25)).
  Text lists gained `names[i] = "…"`.
- **`solve` with a list of unknowns** (D282): an unknown whose initial value is a list (`N(0) = [1e6, 0, 0, 0]`) or
  a list of vectors (`r(0) = r0`) is sized when the program runs, so reaction networks and N-body problems are
  written as loops in a function (`solve N' = rates(N) …`, `solve r'' = accel(r) …`). `N(t)` is the list at t;
  `until`, `absolute`, and all four methods work, with ordinary unknowns alongside.
- **Arrays of 2 to 4 dimensions with units** (D283): `θ = fill(300 K, 50, 50)`, `θ[i, j]`, `θ[i, j] = …`, `A[i, j, k]`,
  entry-by-entry arithmetic with units checked, `size`, `sum`, `mean`, `max`, `min`, `abs`, `copy`; printed in full up
  to 64 entries, otherwise as shape and range. For fields on a grid (a heat-equation step is in the reference).
- Tests: `rust/c-cases/c1/` (each program on both back ends, and under `FERMIUM_GC_STRESS=1`, which collects at
  every safe point), run by `cargo test` (`crates/fermium-cli/tests/c1_cases.rs`).

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
  on the way down. An event that accumulates (a Zeno point) is an error naming the time. RK45 only; with
  uncertain values the uncertainty comes from Monte Carlo.

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
