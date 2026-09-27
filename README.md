# Fermium

**Experimental prototype. Designed by John Oliver (physics, UTK) and built with Claude Code. Expect rough edges; feedback welcome.**

See [Known limitations](docs/reference.md#19-known-limitations) for what's partial, and [dev-notes/](dev-notes/) for the development logs. MIT licensed ([LICENSE](LICENSE)).

**Physics code that reads like physics on paper. The compiler understands units and calculus, and the code runs at native speed.**

```fermium
L = 1.20 m
T = 2.21 s
g = 4π² L / T²
print g                 # 9.70 m/s²
print g in ft/s²        # 31.8 ft/s²
```

Add `y = L + T` as line 6 of that file (a length plus a time, by mistake), and the program doesn't run at all. The compiler stops it first:

```
pendulum.fm, line 6: can't add length [m] to time [s]
    y = L + T
        ^^^^^
  hint: both sides of + and - must have the same units
```

Fermium is a small programming language for physicists.
- **Units are part of the language.** The compiler checks them *before the program runs*, and they cost nothing at run time: they are erased before code generation.
- **Calculus is built in:** `x'`, `d/dt`, `∫ … dx from a to b`, and `solve m x'' = -k x with …`.
- **Programs compile to native code** through LLVM, built into the `fermium` binary: one file, nothing else to install.
- **Errors are one line in physics terms**, with a caret and a hint.
- **Plain ASCII works too.** You can type `pi` or `π`, `sqrt` or `√`, `x^2` or `x²`. `fermium fmt --pretty` / `--ascii` converts between the two.

**What else is in the box** (each item is tested; details in the [reference](docs/reference.md)):
- **Vectors, matrices and ∇:** `<1 m, 2 m, 0 m>`, `r × v`, `grad(φ)`, `div`, `curl`, `laplacian`, eigenvalues, and 3-D vector ODEs.
- **Complex numbers:** `(3 + 4i) Ω`, `exp(𝑖 π)`, complex ODEs such as `1i ħ ψ' = E ψ`.
- **Uncertainties:** `L = 1.000 ± 0.002 m` propagates with correlations (`x - x` is `0 ± 0`), plus `propagate montecarlo`, uncertain fit parameters and error bars.
- **Serious numerics:** stiff solvers (`using radau`), eigenvalue problems (`solve -ħ²/(2 m_e) * ψ'' + V(x) ψ = E ψ … lowest 3`), 1-D PDEs (heat, wave, Schrödinger; animated GIFs), FFT, seeded random numbers and Monte Carlo, root finding.
- **Natural units:** `units natural(ħ = c = 1)`, `units nuclear`, `units astro`. **Dimensional analysis:** `analyze pendulum: T [s] depends on L [m], m [kg], g [m/s²]` gives T ∝ √(L/g).
- **Modules and a standard library:** `import mechanics`, `from nuclear import semf_binding`, with `fermium.toml` ([stdlib](docs/stdlib.md)).
- **Tools:** REPL, Jupyter kernel, language server (hover shows units), VS Code extension, browser playground, and `fermium build` for standalone executables.
- **Research reproductions** in [research/](research/README.md): the SEMF fitted to AME2020, the neutron-star mass–radius relation (TOV), the Chandrasekhar mass, the U-238 chain, hydrogen levels, the age of the universe (Planck 2018), Rutherford scattering by Monte Carlo, the pp/CNO crossover. Each is compared with published numbers.
- **Printing:** a result shows as many significant figures as its inputs justify, and 3 when that is unspecified (`print x to 6 digits` for more). This is display only.

## Install

Fermium is one program, `fermium`, with everything inside it: the compiler, LLVM, the units, the numerics, the plots, the REPL, the language server and the Jupyter kernel. Download the file for your computer from the Releases page (`fermium-macos-arm64` or `fermium-linux-x86_64`), put it on your PATH as `fermium`, and check it (the v2.0 binaries aren't on the Releases page yet: until they are, build it with `make install`, below):

```
fermium doctor                   # checks the installation and explains fixes
fermium run pendulum.fm
```

New to programming? Start with the **[Fermium Bootcamp](bootcamp/README.md)**. Lesson 0 walks through the install on a Mac or Linux PC, step by step.

From a checkout, build and install it with Rust and the LLVM 18 development files ([rust/BUILD.md](rust/BUILD.md) lists what to install):

```
git clone <this repository>
cd fermium
make install                     # cargo install --locked --path rust/crates/fermium-cli  → ~/.cargo/bin/fermium
```

Fermium 2 (the Rust implementation in [rust/](rust/)) replaced Fermium 1.5 (Python) in v2.0: see [CHANGES_2.0.md](CHANGES_2.0.md). Fermium 1.5 is deprecated and kept in [legacy/](legacy/README.md) for one more phase; `python3 -m pip install -e ".[full]"` installs it as `fermium-legacy`.

## A 30-second tour

```fermium
# Mass on a spring
k = 50 N/m
m = 0.5 kg
ω = √(k/m)
print ω in rad/s                      # 10 rad/s

A = 10 cm                             # cm, not 0.1 m: here m is also the mass (Fermium would warn)
x(t) = A cos(ω t)
v = d/dt x
print v                               # v(t) = -A ω sin(ω t)   [m/s, for t in s]

F(x) = k x
W = ∫ F(x) dx from 0 cm to 20 cm
print W                               # 1 J

b = 0.2 kg/s
solve m x'' = -k x - b x'
  with x(0) = 10 cm, x'(0) = 0 m/s
  for t from 0 s to 5 s
print x(5 s)                          # 3.52 cm (to 6 digits: 3.52006 cm)
```

Also:
- `data = load "pendulum.csv"` reads a CSV whose headers carry units, like `L [m], T [s]`.
- `fit T = 2π √(L / g) to data` fits the model and reports g in m/s². The left side may be a formula of a column: `fit T² = k L to data`.
- `plot x vs t` saves a PNG with labelled axes.
- Vectors: `<3, 4> m/s`, `|v|`, `v.x`, `a · b`, `a × b`. ODEs can have vector unknowns: `solve r'' = -G M r / |r|³ with …`.
- Leibniz notation: `dx/dt` works for functions and in `solve`.
- `fermium build prog.fm` makes a standalone executable. The linker is built into `fermium`, so Linux needs no C compiler (a Mac needs Apple's Command Line Tools for the system library). A program it can't compile yet says so: run that one with `fermium run`.

## Gallery

Every example program in [`examples/`](examples) is commented and tested. Here are some of their plots.

**Kepler orbits** ([04_kepler_orbit.fm](examples/04_kepler_orbit.fm)):
```
solve x'' = -GM x / (x² + y²)^(3/2),
      y'' = -GM y / (x² + y²)^(3/2)
  with x(0) = r_peri, y(0) = 0 m,
       x'(0) = 0 m/s, y'(0) = v_peri
  for t from 0 s to 1 yr
plot y in AU vs x in AU, comet_y in AU vs comet_x in AU to "gallery/kepler_orbit.png"
```
![Kepler orbits](examples/gallery/kepler_orbit.png)

**Binding energy per nucleon**, from the semi-empirical mass formula ([09_binding_energy.fm](examples/09_binding_energy.fm)):
```
B(Z, A) = a_V A - a_S A^(2/3) - a_C Z (Z - 1) / A^(1/3) - a_A (A - 2Z)² / A + pairing(Z, A)
...
plot BperA in MeV vs As to "gallery/binding_energy.png"
```
![Binding energy per nucleon](examples/gallery/binding_energy.png)

**The Bateman decay chain Mo-99 → Tc-99m → Tc-99** ([08_bateman_chain.fm](examples/08_bateman_chain.fm)):
```
solve N_Mo' = -λ_Mo N_Mo,
      N_Tc' = λ_Mo N_Mo - λ_Tc N_Tc,
      N_99' = λ_Tc N_Tc
  with N_Mo(0) = N0, N_Tc(0) = 0, N_99(0) = 0
  for t from 0 hr to 240 hr
plot N_Mo vs t, N_Tc vs t, N_99 vs t to "gallery/bateman_chain.png"
```
![Bateman chain](examples/gallery/bateman_chain.png)

**Planck's law for the Sun** ([06_blackbody.fm](examples/06_blackbody.fm)):
```
flux = π * ∫ B(λ) dλ from 0 nm to ∞        # equals σT⁴ to 10 digits
plot B(λ) vs λ from 50 nm to 3000 nm to "gallery/blackbody.png"
```
![Blackbody spectrum](examples/gallery/blackbody.png)

## Speed

Fermium 2.5 (the Rust binary, release build), measured 27 Sep 2026 09:52 UTC on one 4-core machine (Intel Xeon
@ 2.10 GHz, a shared VM, load average 0.9–1.3 during the run), median of 7 interleaved runs. The full table,
methods and caveats are in [benchmarks/RESULTS.md](benchmarks/RESULTS.md);
`python benchmarks/run.py --interleave -r 7 --langs fermium,fermium-1.5,julia,python,numpy` reproduces every number.
The Fermium 1.5 column (the Python implementation, LLVM through llvmlite) is from the same run, for reference;
the back end's own measurements and what is still slower are in
[rust/crates/fermium-codegen/PERF.md](rust/crates/fermium-codegen/PERF.md). Ratios near 1 move by 5–10% from run
to run on this machine.

| Benchmark | Fermium 2.5 (compute) | Fermium 1.5 (compute) | Julia (compute) | Pure Python |
|---|---|---|---|---|
| N-body, 1M steps | **0.86× Julia** (faster) | 1.06× Julia | 1× | ~54× Julia |
| Damped spring, RK4, 1M steps (Fermium also stores the whole trajectory: 40 MB, D151) | **1.95× Julia** (slower) | 2.22× Julia | 1× | ~24× Julia |
| Damped spring, adaptive RK45, both at pure-relative rtol 10⁻⁶ (errors vs exact: Fermium 2.4×10⁻⁴, Julia 1.5×10⁻⁴; Fermium takes 12% fewer steps) | 1.16× Julia (1.3× slower than Fermium 1.5) | 0.88× Julia | 1× | ~22× Julia |
| Blackbody integrals, both at rtol 10⁻¹⁰ (same number of integrand evaluations) | **1.98× Julia** (slower; 1.5× Fermium 1.5) | 1.29× Julia | 1× | ~21× Julia |
| Loop with units | 1.00× Julia | 1.00× Julia | 1× | ~74× Julia |
| All-pairs gravity, N = 2000: `parallel for` vs `Threads.@threads`, 4 threads (D152) | **0.29× Julia** (faster; Julia's 4-thread time was noisy in this run: 6.2–9.8 ms across three runs, Fermium 2.7–2.9 ms) | 0.60× Julia | 1× | ~93× Julia's 4-thread time (Python runs one thread) |
| the same, 1 thread | **0.50× Julia** (faster) | 1.08× Julia | 1× | |

Counting startup and compilation, Fermium 2.5 finishes every benchmark program far sooner than Julia (14–72 ms
against 0.9–2.8 s for the whole process) and than Fermium 1.5 (0.18–0.43 s), because it starts in milliseconds,
compiles quickly and reuses its compiled code on a repeat run (the JIT cache, D317), while Julia spends that time
loading its runtime and packages, running the untimed warm-up call each benchmark makes, and JIT-compiling. A
program that computes one number takes 15 ms in Fermium 2.5, 0.18 s in Fermium 1.5 and 0.29 s in Julia (the
`startup` row in RESULTS.md).

## Gauntlet

Textbook problems from ten fields of physics, solved in Fermium and checked against closed forms or SciPy ([gauntlet/](gauntlet/README.md)). Every rough edge they hit is logged in [gauntlet/FRICTION.md](gauntlet/FRICTION.md) and fixed in the language where possible.

<!-- gauntlet-table -->
| Topic | Pass 1 | Pass 2 | Pass 3 (graduate) |
|---|---|---|---|
| mechanics | 3 | 3 | 2 |
| oscillations | 3 | 3 | 2 |
| gravitation | 3 | 3 | 2 |
| thermodynamics | 3 | 3 | 2 |
| electromagnetism | 3 | 3 | 2 |
| optics waves | 3 | 3 | 2 |
| special relativity | 3 | 3 | 2 |
| quantum | 3 | 3 | 2 |
| nuclear | 3 | 3 | 2 |
| astrophysics | 4 | 3 | 2 |
| **total** | **31** | **30** | **20** |

Friction items logged: 96; fixed in the language: 86.
<!-- /gauntlet-table -->

## Browser playground

`web/` is a static site that runs Fermium in the browser: an editor with `\name` + Tab completion, Run (Ctrl+Enter / Cmd+Enter), errors in the usual one-line form, plots, and a menu with every code block of the bootcamp lessons and every program in `examples/`. Nothing is sent to a server. It runs the Rust compiler built to WebAssembly (a 3.6 MB module, `rust/crates/fermium-wasm`) with its interpreter, so it is slower than the desktop `fermium run`; its output matches `fermium run`.

```
rustup target add wasm32-unknown-unknown
python3 web/build.py                  # fermium.wasm, examples and symbols -> web/gen/
python3 -m http.server -d web 8000    # then open http://localhost:8000/
```

`web/gen/` is a build output and is not committed. [web/README.md](web/README.md) has the details and the tests.

## Documentation
- [Language reference](docs/reference.md)
- [Rosetta page](docs/rosetta.md): the same programs in Fermium, Julia and Python
- [Design decisions](DECISIONS.md): why the language is the way it is
- [Bootcamp](bootcamp/README.md): a course for people who have never programmed
- [Cheat sheet](bootcamp/CHEATSHEET.md)
- [Standard library](docs/stdlib.md) and [uncertainties](docs/uncertainties.md)
- [Research reproductions](research/README.md), [textbook gauntlet](gauntlet/FRICTION.md), [red team log](dev-notes/REDTEAM.md)
- [VS Code extension](editors/vscode/README.md)

## Development

```
python3 -m pip install -e ".[full,dev]"   # Fermium 1.5 (legacy): the conformance oracle and its test suite
make check           # legacy lint + tests, then the Rust build, cargo test and the conformance suite
make build           # rust/target/fast/fermium, for iterating
python3 benchmarks/run.py
```

`make check` runs the Rust binary on the whole conformance suite (`conformance/`): every example, rosetta, gauntlet and research program, every ```fermium code block of the docs and bootcamp, and every test program, each with the output Fermium 1.5 gives. `FERMIUM_SKIP_RUST=1 make check` skips the Rust part and says so.

Project layout ([docs/architecture.md](docs/architecture.md) is the guide for contributors):
- `rust/`: Fermium 2, the compiler (a Cargo workspace; `rust/crates/fermium-cli` is the `fermium` binary). [rust/BUILD.md](rust/BUILD.md) explains the build; [rust/DIVERGENCES.md](rust/DIVERGENCES.md) lists where it deliberately differs from 1.5.
- `conformance/`: the conformance suite and its runner; the score is in [CONFORMANCE.md](CONFORMANCE.md).
- `legacy/`: Fermium 1.5 (Python), deprecated: `legacy/fermium/` (the package, still imported as `fermium`) and `legacy/tests/` (its pytest suite).
- `examples/`, `bootcamp/`, `docs/`, `gauntlet/`, `research/`, `benchmarks/`: programs and documentation.
- `web/`: the browser playground; `editors/vscode/`: the VS Code extension.
