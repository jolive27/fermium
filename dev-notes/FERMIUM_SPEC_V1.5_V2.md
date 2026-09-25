# Fermium — Spec 1.5 → 2 → 2.5 → 3

You are continuing **Fermium**, a programming language for physicists: units checked at compile time and erased at run time, calculus built into the language, compiled through LLVM. **John** designs Fermium's direction. He is a physics undergraduate (nuclear physics and astrophysics) who is still learning to code. Since the first build he has installed it on his Mac (Apple Silicon), worked through bootcamp Lessons 0–3, and written his own programs. His notes and programs drive this spec.

Nobody will be available to answer questions. Make reasonable decisions, record them in `DECISIONS.md`, and keep going. **Work window:** until the end time in the kickoff message; check `date -u`. The run has four phases in order, each gated on the previous one. Expect Phase B to take more than one run. If you get through everything faster than expected, Phases C and D are there so the work never runs out.

| Phase | Version | What | Gate to move on |
|---|---|---|---|
| A | v1.5 | Fix what real use revealed, in the Python implementation, then freeze the language | §A10 |
| B | v2 | Rewrite the compiler in Rust with zero runtime dependencies | §B8 cutover |
| C | v2.5 | Language growth on the Rust compiler | §C9 |
| D | v3 | Scale and ecosystem: GPUs, clusters, interop, packages | never "done" |

---

## 0. The starting point: know what already exists

The current code is branch `claude/lucid-gauss-9y1ov2`, final commit `af43963`, where `make check` passed with 3612 tests. It is much bigger than its first spec. Before changing anything, read `README.md`, `SHOWCASE.md`, `DECISIONS.md` (D1–D233), `docs/reference.md` (including §19 Known limitations), `docs/stdlib.md`, `docs/uncertainties.md`, `BACKLOG.md`, `gauntlet/FRICTION.md` (96 items), `REDTEAM.md` (7 rounds), `notes/bugs-*.md`, and `research/README.md`.

What already exists:

- **Front end:** a Python front end of about 25,000 lines in 41 modules. It covers the lexer, parser, checker, calculus, the LLVM codegen through llvmlite, and an ahead-of-time build. There is a separate reference interpreter, `interp.py`.
- **Language features:**
  - complex numbers (D90–D94)
  - uncertainties, in the interpreter only (D120–D124)
  - natural, nuclear and astro unit systems (D60)
  - `analyze` dimensional analysis (D70)
  - eigenvalue problems (D82, D190, D233), 1-D PDEs (D83, D131, D206), FFT (D81) and a seeded RNG (D80)
  - stiff solvers, which call SciPy (D42)
  - `parallel for` (D152)
  - matrices up to 16×16 (D195), list slices (D114), `table(...)` (D193)
- **Modules and interop:**
  - modules, a standard library written as Fermium source in `fermium/stdlib/*.fm` (mechanics, em, nuclear, astro, quantum, stats), and `fermium.toml` (D100–D103)
  - Python interop in both directions: `use python numpy as np` with unit contracts (D140), and `fermium.compile()` from Python (D142)
  - a self-hosted unit database written in Fermium (`fermium/selfhost/`, M8)
- **Tools:** the REPL, a Jupyter kernel, a language server built on pygls, a VS Code extension, a Pyodide browser playground (`web/`), `fermium build`, `fmt` and `doctor`.
- **Content:**
  - 31 examples, plus 7 rosetta programs in Fermium, Julia and Python
  - the bootcamp: Lessons 0–12, 2b, a cheat sheet, troubleshooting and solutions
  - 81 gauntlet problems and 11 research reproductions
  - benchmarks against Julia, Python and NumPy

**Rule for every phase:** a feature listed above must never be lost. The Rust rewrite ports all of it (§B5), and later phases build on it rather than re-creating it.

---

## 1. What John says to keep, 100%

These are the reasons Fermium exists. A change that weakens any of them is wrong, even if it simplifies the implementation.

1. **Notebook-like readability.** Greek letters, subscripts, `²`, `√`, `½` and implicit multiplication. In John's words: *"the biggest plus of the program so far by a mile is the readability and appearance of writing equations in a notebook."* These must keep working exactly as written:
   - `R = (v₀)² sin(2θ)/g₀`
   - `g = 4π^2L/T^2`
   - `E = ½ m v²`, with a mass `m` defined
   - `lam = h / m_e v`
   - `ω₀ = √(k/m) where k = 50 N/m, m = 0.5 kg`
2. **The core unit rule:** a unit comes right after a number, and everywhere else a name is a variable. It works better in practice than the docs suggest.
3. **`\name` + Tab completion** and **ASCII ⇄ symbol equivalence** (`theta` is `θ`).
4. **`fermium fmt --pretty / --ascii`**, which never changes meaning.
5. **Speed.** Compiling and running should feel instant. **Errors** are one line with a caret and a hint.
6. **One-line and block functions, `where` clauses,** compile-time unit checking, natural units, and the constants library.

---

# PHASE A: v1.5, in the Python implementation

Work on branch `claude/v1.5`, created from `af43963`. Write tests first for every item, and keep `make check` green.

## A0. Triage everything that is still open (do this first)

Collect every open, partly fixed or "by design" item from these sources into `dev-notes/OPEN_ITEMS.md`:

- `BACKLOG.md` (some of it is stale; confirm each item against the code)
- `gauntlet/FRICTION.md`
- `REDTEAM.md`
- `notes/bugs-*.md`: the review items R1–R7, the bootcamp B-items, the examples items #7, #12, #14 and #15, and the tests items B17–B18
- `docs/reference.md` §19

Mark each one as **fix in A**, **fix natively in B**, **C/D feature**, or **by design (with the reason)**. Re-run every repro from those notes so you know what still reproduces. This file tells the rest of the run what is left.

## A1. One unit-name rule, stated in three sentences

The collision handling between units and variables grew case by case. It now includes D7 and its descendants: at least D130, D163, D164, D170, D171, D180, D184 (the bare `i`), D192, D199, D203, D204, D207, D211, D215, D222, D231 and D232. On top of that there is a spacing-sensitive `/` rule (D7 rule 4, D34), and Lesson 1 has to teach a "Big Gotcha." Replace all of it with one rule that does not depend on spacing:

> 1. Right after a number comes a unit: `3 m`, `9.81 m/s²`, `50 N/m`.
> 2. If that unit is a **single name** that is also one of your variables (`2 g` with your own `g`, `0.1 m` with a mass `m`), Fermium stops and asks which you mean: `2*g` for your variable, `2 [g]` for the unit.
> 3. In a **compound unit** (`3 m/s`, `2 kg m²`) the first name is always a unit. Any **later** name that is also your variable is an error that asks the same question.

Brackets are always units (`2 [g]`, `x [m]`). A name that doesn't come right after a number is always a variable. Spaces never change meaning.

**Required behavior. Make each row a test:**

| Code | Your variables | Must do |
|---|---|---|
| `v = 3 m/s` | `m = 2 kg` | 3 m/s (John's ke.fm) |
| `E = ½ m v²` | `m`, `v` | ½·m·v² (no number comes right before `m`) |
| `KE = (1/2) m v²` | `m`, `v` | ½·m·v² |
| `print 20 m/s/g` | `g` | **error** asking which. Today it silently means "per gram" (John's note; bootcamp B18) |
| `print 20 m/s / g` | `g` | the same error, because spacing never matters. Fix: `(20 m/s)/g` |
| `print (20 m/s)/g` | `g` | divides by your `g` |
| `print 2 g h` | `g`, `h` | error, showing both fixes |
| `x(0) = 0.1 m` | mass `m` | error. Fix: `0.1 [m]` |
| `print 2 kg m` | `m` | error, because a later name is your variable |
| `B = 2 T` | `T = 2.21 s` | error. Fix: `2 [T]` |
| `2 Ω t`, `8 K`, `2 b`, `0.25 T`, `2 l²` | Ω, K, b, T, l defined | error, showing both fixes (friction #74) |
| `print 2 c` | your own `c` | error (D130) |
| `print 9.81 m/s² * 3 s` | none | 29.4 m/s |
| `θ = 45 °`, `theta = 30 deg` | none | angles, as today |

Constants that are not units (`G`, `h`, `c`) still work by implicit multiplication: `2 h` is 2 × Planck's constant. Keep the existing warning for those.

You may refine the rule if a case forces it, but it must stay:

- **at most 3 short sentences** in the bootcamp,
- **independent of spacing**,
- **passing every row above, every program in Appendix 1, and every gauntlet and research program** after migration.

Mark every superseded decision in `DECISIONS.md` as "superseded by D-new", and delete the code that implements them.

**Migration:**

- Add `fermium fmt --fix`, which rewrites collisions as bracketed units. Offer the same fix as a quick fix in the language server.
- Use it to migrate the examples, bootcamp, gauntlet, research, rosetta, stdlib and tests.
- When a golden output changes only because of the migration, update it. **If a value changes, stop and investigate.**

## A2. Fraction coefficients (friction #73)

D8 makes implicit multiplication bind tighter than `/`. That is right for `h c / λ k_B T` and for John's `lam = h / m_e v`. But it reads the textbook coefficients `73/24 e²`, `π²/12 t²` and `π⁴/80 t⁴` as 73/(24 e²), and it produced nine warnings in two graduate problems.

**New rule:** a fraction of pure numbers (digits, `π`, `√` or powers of numbers) is a single coefficient:

- `73/24 e²` = (73/24)·e²
- `π²/12 t²` = (π²/12)·t²
- `1/2 kg` = 0.5 kg. Today this gives 0.5 /kg; list it as a deliberate change in `CHANGES_1.5.md`.
- `h / m_e v` is unchanged, because its denominator contains a name.
- `1/2 m v²` with a mass `m` falls under A1 rule 2 and asks which you mean. The suggested fix is `½ m v²`.

Record the decision, update the warnings, and add tests.

## A3. Error messages John hit, and similar ones

1. **`ħ = c = 1`** currently gives the hint "use == to compare." It should recognize the physics intent and suggest `units natural` or `units natural(ħ = c = 1)`.
2. **An undefined `g`** currently gives *"g isn't defined: the eigenvalue problem's states are g₀"*, in a program with no eigenvalue problem. Fix the leak:
   - A hint must only mention features the program actually uses.
   - The eigenvalue "ground state" name `g₀` must not shadow, or be confused with, the constant `g_0`.
   - The new message: *"g isn't defined. For standard gravity use g_n (9.80665 m/s²), or define your own: g = 9.81 m/s²."*
   - Then **audit every "did you mean" hint for similar leaks**, including R4 (the hint always says `2*m`) and B9 (typos, unknown units).
3. **Conversions to the wrong kind of unit** (`print h c in J/m`, `in J`, `in eV/nm`) should suggest 2–3 units of the right kind: *"h c is energy × length; try `in J m` or `in eV nm`."*
4. **`fermium fmt`** prints formatted source that looks like program output. After printing to stdout, add a one-line note on stderr: `formatted ke.fm (not run — use fermium run)`.
5. **Typing a shell command at the REPL** (`fm> fermium run …`) should hint: *"this is the Fermium prompt; type :quit to go back to the terminal first."*
6. Re-check review items **R1–R3** (`d²x/dt²` in `solve`, `d/dt (dx/dt)`, `dx/dt(0)` initial conditions) and **R5–R7**. Fix any that still reproduce.

## A4. Display and the formatter

1. **Preferred display units:** energy × length shows as `J m`, not `N m²`. Add composite preferences for common physics products: J m, eV nm, MeV fm, J s, and N m for torque when written that way. `print h c in eV nm` → 1240 eV nm must keep working.
2. **Fractions in `--pretty`:** turn `(1/2)` into `½` (and ⅓, ¼, ¾, and so on). Turn `(0.5)` into `½` **only if** that doesn't change any printed output: 0.5 is a literal with measured precision, while ½ is exact. Otherwise leave it and document why.
3. The 3-significant-figure default (D11) stays as implemented, display only. Close the open list-element significant-figure items (examples #15, friction #30) if that's cheap.
4. Examples #14: the look-alike character normalization must not rewrite text inside strings.

## A5. API consistency before the freeze

Anything designed before a later feature existed should be made consistent now. After the freeze it would have to be carried forward forever.

- **FFT (D81)** returns `fft_re`/`fft_im` because it predates complex numbers. Make it return complex values; keep the old names as deprecated aliases for one version.
- **`fit … with`** should continue onto the next line like `solve … with` does (examples #12).
- **Lists of text** (examples #7), if they're cheap. Otherwise schedule them for Phase C.
- Search `DECISIONS.md` for other "because X didn't exist yet" workarounds and list them in `OPEN_ITEMS.md`.

## A6. Install, plots and examples (found on macOS)

1. **`fermium doctor`:** when optional packages are missing, print **one** copy-pasteable fix: `python3 -m pip install -e ".[full]"`. `CLAUDE.md` still says `pip install -e .`; correct it everywhere.
2. **"plot saved to …"** must print the **absolute** path. It printed `gallery/…`, but the file was in `examples/gallery/`.
3. **`examples/01_pendulum.fm`** must draw the fitted curve over the data.
4. **Axis labels** use the column or variable name (`T [s]`), not `data.T [s]`.
5. **CI:** add GitHub Actions on **`macos-14` (arm64) and `ubuntu-latest`**, running `make check` from a fresh `pip install -e ".[full,dev]"`. Watch the runs with `gh` and fix failures. macOS has never been tested in CI.

## A7. Bootcamp refresh

In John's words, the bootcamp is *"overly cautious… the language is farther along than the bootcamp indicates."*

1. **Lesson 1 must not teach a "Big Gotcha."** After A1, the rule is a short, calm, three-sentence paragraph, placed where it's first needed (likely Lesson 2), with one example of each fix.
2. **Lead with the most natural, notebook-like syntax:** `½ m v²`, `(h c)/λ`, `4π² L / T²`. Remove every `*` and workaround that's no longer needed. Re-check each "careful" or "gotcha" note against the current language and delete the stale ones.
3. **Teach gravity explicitly:** put `g = 9.81 m/s²` in your file, or use `g_n` for standard gravity (`g_0` also works).
4. **Explain `fmt` vs `run`** in Lesson 2b.
5. **Keep everything tested:** all code blocks stay tested, and the output boxes are regenerated and compared.

## A8. Review items (from an outside code review)

1. **`≈` near zero.** With a relative-only tolerance, `v ≈ 0 m/s` is never true.
   - Adopt Julia's `isapprox` semantics: `|a−b| ≤ max(atol, rtol·max(|a|,|b|))`.
   - Add syntax for an explicit tolerance: `x ≈ 0 m/s within 1e-9 m/s`.
   - Comparing to exactly zero without a tolerance is a compile error that shows the fix.
2. **D48 stores a pointer as a double** (`ptrtoint`/bitcast). Replace it with a properly typed handle. Flush-to-zero or fast-math settings could turn a pointer that looks like a subnormal into 0. Add a test.
3. **Research inputs typed from memory:** the BBN rate coefficients, the Geiger–Marsden table and the ²⁰⁸Pb single-particle energies. Using network access:
   - check each one against a cited primary source,
   - record the citation next to the data,
   - fix any discrepancies,
   - re-run the affected reproductions and update `research/README.md`.

## A9. Repo hygiene for outside testers

1. Add an **MIT `LICENSE`**.
2. Put this status note at the top of `README.md`: *Experimental prototype. Designed by John Oliver (physics, UTK) and built with Claude Code. Expect rough edges; feedback welcome.*
3. Move the development logs into `dev-notes/`: `MORNING_REPORT.md`, `PROGRESS.md`, `AUDIT.md`, `REDTEAM.md`, `FERMIUM_SPEC_1.md`, `notes/`, and this spec once Phase A starts. Keep `README.md`, `docs/`, `bootcamp/`, `examples/`, `research/`, `gauntlet/`, `DECISIONS.md` and `SHOWCASE.md` at the top level. Update links.
4. Make sure no large binaries, tool installs (such as `.tools/`), secrets or `.DS_Store` are committed, and extend `.gitignore`.
5. **Never add, edit or commit John's personal files:** `my_notes.md`, `hello.fm`, `me.fm`, `lesson*.fm`, `ke.fm`. Copy their programs into tests instead (Appendix 1).

## A10. Freeze v1.5 (the gate)

When A0–A9 are done and CI is green on both platforms:

1. Write `CHANGES_1.5.md`: what changed and why, readable by a beginner.
2. Tag `v1.5` and open a pull request from `claude/v1.5`.
3. **The v1.5 language is now frozen.** Phase B must match it.

---

# PHASE B: v2, the compiler in Rust

Start only after A10. Work on branch `claude/v2-rust`, created from the `v1.5` tag.

## B1. Principle

> **Zero required dependencies for users. Optional bridges to everything.**

- Users install one `fermium` binary: no Python, pip packages, C compiler, LLVM or Rust.
- Rust crates and LLVM (including lld) are compiled and linked into the binary.
- Python, Rust and the LLVM dev libraries are fine as **developer and test tooling**.
- Bridges to Python, C, Fortran and C++ load only when a program imports them.

Fermium is **an evolution of Julia for physicists**: its own language, not a layer on Python.

## B2. v1.5 is the oracle

- Move the Python implementation to `legacy/`. Keep it runnable and in CI, and use it as the reference.
- **The language is frozen:** the same syntax, rules, results and warnings.
- **Fix documented v1 limitations natively** rather than reproducing them:
  - the quadrature's narrow-peak and half-peak cases (use a second, independent estimate or a QUADPACK-style ε-algorithm)
  - strong singularities away from 0
  - integrals at rounding level printing too many figures
  - PDE accuracy right after a jump
  - significant figures of sums
  - memory that is never freed
- Log each intentional difference in `DIVERGENCES.md`, with a test.

## B3. The conformance suite first

`conformance/` holds `.fm` programs, each with:

- golden stdout,
- expected diagnostics (kind, line and column, key content),
- an exit code.

Compare numeric outputs with documented tolerances.

Harvest programs from:

- every test, example, rosetta program and bootcamp block
- every gauntlet problem and research reproduction
- the benchmarks and the rejection tests
- every repro in `notes/`, `REDTEAM.md` and `FRICTION.md`
- **Appendix 1 (John's programs), which is mandatory**

The runner, `conformance/run --impl legacy|rust`, writes `CONFORMANCE.md`: pass/fail for each program, grouped by feature area, with an overall percentage. That file is the honest scoreboard. The suite is ready when legacy passes 100%.

## B4. Architecture

A Cargo workspace. Adjust the split if you have reasons, and record them:

| Crate | Contents |
|---|---|
| `fermium-syntax` | lexer, parser, spans, Unicode/ASCII, look-alike characters |
| `fermium-units` | dimensions with rational exponents, unit database, CODATA constants. **Keep the unit database in Fermium source (M8) and load it at build time.** |
| `fermium-check` | names, types, dimensions, diagnostics |
| `fermium-sym` | derivatives, simplification, native integration |
| `fermium-ir` | typed IR: the boundary for back ends |
| `fermium-codegen` | LLVM via `inkwell` or `llvm-sys`, one pinned version, statically linked, **behind a backend trait** |
| `fermium-runtime` | native numerics, CSV, plotting, uncertainties |
| `fermium-fmt`, `fermium-cli`, `fermium-lsp`, `fermium-jupyter` | the tools |

The **standard library stays Fermium source** (`stdlib/*.fm`). Target well under 50 ms for startup and for `fermium check` on small programs, and measure it.

## B5. Parity milestones

Work through these in order. Each is done when its conformance group passes.

1. **Syntax,** including `fmt` round-trips.
2. **Units and checking:** the A1 rule, A2 coefficients, every rejection test, natural/nuclear/astro units, and `analyze`.
3. **Core execution** through the LLVM JIT:
   - functions and control flow
   - lists and slices, vectors, matrices, `table`
   - `where`
   - printing with significant figures and display units, and `in`
4. **Calculus:**
   - every derivative notation, including ∂ and ∇
   - integrals and algebraic `solve`
   - ODE `solve`, including `until`, backwards ranges, vector and complex unknowns
   - stiff solvers
   - eigenvalue problems (Numerov and shooting)
   - PDEs
   - FFT
5. **Complex numbers.**
6. **Uncertainties, natively in compiled code,** including the REPL, Jupyter and `fermium build`; v1 had them in the interpreter only.
7. **Data:** `load`, `fit` (with `err()`, and weighted fits), `plot` to PNG and SVG.
8. **Modules and stdlib, `parallel for`, and the RNG.**
9. **CLI and REPL:** history and `\name` + Tab.
10. **`fermium build`** with **bundled lld**. Linux must work fully. On macOS arm64, link against libSystem; if that's impossible without the Xcode Command Line Tools, document exactly what's needed.
11. **Language server:** units on hover, live errors, completion, and the A1 quick fix.
12. **Playground via WebAssembly,** replacing Pyodide.
13. **Jupyter kernel** written in Rust.
14. **Python interop** in both directions (D140, D142), with libpython loaded at run time and only when imported.

## B6. Native replacements for Python libraries

| v1 used | v2 replacement |
|---|---|
| llvmlite | inkwell or llvm-sys |
| NumPy | Fermium arrays |
| SciPy quadrature | adaptive Gauss–Kronrod plus the B2 improvements |
| SciPy stiff ODE solvers | native Radau IIA and BDF |
| SciPy fitting | native Levenberg–Marquardt with covariance |
| SciPy root finding | bracketing plus Brent |
| NumPy eigenvalues | native Jacobi and QR |
| SciPy/NumPy PDEs and FFT | native |
| SymPy indefinite integrals | native rule tables, substitution, integration by parts and partial fractions. If no antiderivative is found, give a clear error suggesting a definite integral. Record coverage vs v1 in `DIVERGENCES.md`. |
| matplotlib | native SVG and PNG plots with unit-labelled axes, markers and log axes |

Validate every method against v1 and SciPy reference values stored as fixtures. Python may generate the fixtures, but must never be needed at run time.

## B7. Distribution and CI

- **CI:** GitHub Actions on `ubuntu-latest` and `macos-14`, building and running the conformance suite for both implementations.
- **Release:** one `fermium` binary per platform.
- **Lesson 0:** update it for the new install (download one file, put it on your PATH, run `fermium doctor`). Keep it beginner-friendly and tested.
- **`fermium doctor`** reports the version, the embedded LLVM version and the platform, and that nothing external is needed.

## B8. Cutover (the gate)

When `CONFORMANCE.md` reaches 100%, or every failure is a documented divergence:

1. Make the Rust binary *the* `fermium`, and update the README, docs, bootcamp, examples and benchmarks.
2. Keep `legacy/` in CI for one more phase, marked deprecated.
3. Re-run the benchmarks against Julia and Python: compile time, startup and compute. Never fudge them.
4. Write `docs/architecture.md` for future contributors, and tag `v2.0`.

---

# PHASE C: v2.5, language growth (only after B8)

Branch `claude/v2.5`. Each item gets tests, docs, a bootcamp or reference section, and conformance programs.

- **C1. Memory and data structures.** N-dimensional arrays with units. Real memory management (reference counting or a garbage collector). Lists of vectors, matrices, complex numbers and text. `solve` with a list of unknowns, for reaction networks and N-body problems written as loops.
- **C2. Calculus reach.** Derivatives of multi-line functions (automatic differentiation). Events in `solve` that depend on the unknowns (`if x > 0 m` located accurately). Better symbolic simplification. Parameter sweeps over `solve`.
- **C3. Unit-checked C and Fortran interop through the C ABI.**
  - `import c "libfoo.so" function energy(m: kg, v: m/s) -> J`
  - `import fortran "libnuclear.so" function binding_energy(Z, A) -> MeV`, using `bind(C)` and name-mangling conventions
  - Units are checked at every call; this is Fermium's headline interop feature.
  - Add a worked example that calls a real, small open-source physics routine.
- **C4. C++ interop** through generated C wrappers.
- **C5. Multiple dispatch,** Julia's core idea: one function name with versions chosen by argument types **and dimensions**.
- **C6. Performance.** SIMD-friendly codegen and loop vectorization. Beat Julia on at least half of the benchmark rows, measured honestly. Cache compiled modules.
- **C7. Uncertainties everywhere:** vectors of uncertain values, and uncertainty through ODEs and integrals (linear propagation where valid, Monte Carlo otherwise).
- **C8. The research track.** Add 10 more research reproductions in nuclear physics and astrophysics, **each with data downloaded from a cited source** and compared with published values. Examples:
  - neutron star cooling
  - r-process abundance patterns
  - nuclear charge radii against the Angeli–Marinova tables
  - the Gamow window for key stellar reactions
  - white dwarf cooling ages
  - the CMB blackbody from FIRAS data
- **C9. The gate:** tag `v2.5` when C1–C7 are done, CI is green, and the conformance suite has grown with every feature.

---

# PHASE D: v3, scale and ecosystem (only after C9; open-ended)

Branch `claude/v3`. These are large; do them in order, and get each to an honest, tested state before starting the next.

- **D1. A formal language specification** (`docs/spec/`): the grammar in EBNF, the typing and dimension rules, the unit rule, the numeric semantics. This is what makes a "1.0" possible. Keep it consistent with the conformance suite.
- **D2. GPU back ends** behind the backend trait: LLVM NVPTX (NVIDIA) and AMDGPU (AMD; Frontier at ORNL uses AMD). Start with `parallel for` loops and element-wise array math, plus host↔device memory management.
  - **Honesty rule:** there may be no GPU in the cloud machine. Test the code generation (IR and PTX/GCN output, unit tests) and run the same kernels on the CPU back end for correctness.
  - Mark the GPU paths **"not yet run on GPU hardware"** until they have been. Never claim GPU speedups you didn't measure.
- **D3. Distributed runs with MPI:** bindings, domain decomposition helpers, and parallel HDF5 input and output with units in the metadata. Test with multiple local processes (`mpirun -np 4`) on the CI machine.
- **D4. A package manager** (`fermium add`, a lock file, a registry format), building on `fermium.toml`.
- **D5. More self-hosting.** Rewrite more of the compiler in Fermium (the formatter, then the unit checker), compiled by the Rust compiler and passing the same tests. Document what that proved.
- **D6. A documentation website:** static, built from `docs/`, the bootcamp and the examples, with the WebAssembly playground embedded.
- **D7. Validation against established codes:** compare Fermium implementations with trusted results, such as a MESA stellar model profile, a published nuclear mass model table, or a standard cosmology code's distances. Write up agreements and disagreements honestly.
- **D8. Keep going:** red-team, optimize, add conformance programs, and add research reproductions. Log new ideas in `BACKLOG.md`.

---

## E. Operating rules (every phase)

1. **Don't finish before the end time.** If every listed item is done, add to `BACKLOG.md` and continue. There is always a next item.
2. **Commit after every working step, and push at least every 30 minutes.** Never leave a branch broken.
3. **Keep `dev-notes/PROGRESS.md` current:** done, in progress, next and blocked, with a timestamped line every hour. After any context compaction, re-read `CLAUDE.md`, `PROGRESS.md` and this spec. Update `CLAUDE.md` for this plan at the start: the commands, the phases, and the `.[full]` install.
4. **Stuck for more than 30 minutes:** log it under Blocked, move on, and come back later.
5. **Tests, CI and `CONFORMANCE.md` are the source of truth.** Never weaken a test or golden output unless it was wrong, and explain why in the commit.
6. **Honesty:** never claim a feature works unless a test proves it. Label stubs as stubs. Keep benchmarks fair.
7. **Subagents:** use them for parallel work, and merge only through tests and conformance. Before any full test run, make sure the installed `fermium` points at the main checkout. The first night lost two hours to an editable install that pointed at an agent's worktree.
8. **Red team:** run a pass with an independent subagent reviewer at least every 3 hours, looking for wrong physics, wrong units, misleading docs and unfair benchmarks. Fix what it finds and log it in `REDTEAM.md`.

## F. Report (write it 30 minutes before the end time)

`dev-notes/RUN_REPORT.md`, written for a beginner:

1. One paragraph on where Fermium stands now, and which phase the run reached.
2. **Phase A:** each item A0–A10 marked done, partial or not started, with evidence. Include the final three-sentence unit rule exactly as the bootcamp states it, and the `OPEN_ITEMS.md` triage summary.
3. **Phase B:** the `CONFORMANCE.md` summary (overall % and per area), what a user must install today (the goal is nothing), and before/after numbers for startup and compute.
4. **Phases C and D,** if reached: status per item, with evidence.
5. Every divergence from v1.5, and why.
6. Exact steps for John to try the latest build on his Mac.
7. Honest weaknesses, and the recommended next steps.

---

## Appendix 1: John's programs (mandatory regression tests)

Each must run unchanged in v1.5, v2 and later, with these values. The display format may follow D11, but the numbers must match. Add them to the tests and to `conformance/`.

**hello.fm**
```fermium
print "Hello Physics!"
print 9.81 m / s^2 * 3 s        # 29.4 m/s
```

**ke.fm** (a mass `m` is defined, then `3 m/s` is used; this is A1's first row)
```fermium
m = 2 kg
v = 3 m/s
E = ½ m v²
print E                          # 9 J
```

**lesson2.fm**
```fermium
v₀ = 20 m/s
θ = 45 °
R = (v₀)² sin(2θ)/g₀
print R                          # 40.8 m

m = 1500 kg
v = 100 km/hr
KE = (1/2) m v²
print KE in kJ                   # 579 kJ

v = √((2 G M_earth)/R_earth)
print v in km/s                  # 11.2 km/s

a = 3 m
b = 5 m
temp = a
a = b
b = temp
print a, b                       # 5 m 3 m

v = 0.01 · c
lam = h / m_e  v
print lam in nm                  # 0.24 nm  (λ = h/(m_e v) ≈ 0.243 nm)
```

**lesson2b.fm**
```fermium
ω₀ = √(k/m) where k = 50 N/m, m = 0.5 kg
print ω₀ in rad/s                # 10 rad/s
print 2π √(1.0 m / 9.81 m/s²)    # 2.0 s
theta = 30 deg
print sin(θ)                     # 0.500 (theta and θ are the same name)
```

**lesson3.fm**
```fermium
F_grav(m1, m2, r) = G m1 m2 / r^2
print F_grav(M_earth, 70 kg, R_earth)          # 686 N

R(A) = r₀ A^(1/3) where r₀ = 1.2 fm
print R(12), R(56), R(238)                     # 2.75 fm, 4.59 fm, 7.44 fm

proj_range(v₀, θ) = v₀² sin(2θ)/g where g = 9.81 m/s^2
print proj_range(20 m/s, 30 deg), proj_range(20 m/s, 45 deg), proj_range(20 m/s, 60 deg)
                                               # 35.3 m, 40.8 m, 35.3 m

peak(T) = b / T where b = 2.898e-3 m K
print peak(5778 K) in nm, peak(5778 K) in um, peak(310 K) in nm, peak(310 K) in um
                                               # 502 nm, 0.502 um, 9.35×10³ nm, 9.35 um

fall_time(h) =
    g = 9.81 m/s^2
    t = √(2 h / g)
    return t

print fall_time(330 m)                         # 8.20 s
```

**REPL checks:**

- `print 2 m + 30 cm` → 2.30 m
- `print h c in eV nm` → 1240 eV nm
- `print h c in J m` → 1.99×10⁻²⁵ J m
- `print 0.5 * 2 kg * 3 m/s^2` → 3.0 N. This is the bootcamp's "spot the bug" exercise: the unit shows the bug.

## Appendix 2: John's notes, verbatim (Lessons 0–3)

> Readability through lesson 0 and lesson 1 so far is exactly how I designed, very easy to learn and use.
> 20 m/s/g should suggest error in compiler instead of running as is.
> \hbar = c = 1 should suggest units natural(code) instead of the use == hint it currently gives.
> Need to fix so that lesson 1 doesn't teach a "Big Gotcha".
> Bootcamp is overly cautious on rules (telling to use *), which is good, but the language is farther along than bootcamp indicates.
> Lesson 2: Using g (gravity) is clunky, should suggest g_n or tell you to establish g = 9.81 m/s^2 before running instead of leaking in eigenvalue problem's.
> Greek letters and subscripts work beautifully: keep no matter what, biggest plus of the program so far by a mile is the readability and appearance of writing equations in a notebook.
> Smart compiler, very quick runtime, and supreme readability.
> Lesson 2b: Symbol translation (\… tab) is fully functional, another thing that must be kept. Running pretty should turn (1/2) or (0.5) to the ½ character. All the math checks out, compiler working well, readability is incredible.
> Lesson 3: Functions run without any hiccups, good feature.
