# Fermium — Overnight Build Instructions

You are building **Fermium**, a new programming language for physicists, working autonomously overnight. The person you're building it for is a physics undergraduate (nuclear physics & astrophysics interests) who is still learning to code. Nobody will be available to answer questions until morning — make reasonable decisions, write them down, and keep going.

**Work window:** from now until **7:00 AM Eastern (11:00 UTC), Friday Sep 25, 2026**. Check the time with `date -u`. You are not done before then, no matter how much is finished (see *Operating Rules*).

---

## 1. Mission

> **Physics code that reads like physics on paper, a compiler that understands units and calculus, and speed on par with Julia.**

Fermium should be a meaningful upgrade over Julia *for physicists*, specifically in:

| Area | Julia today | Fermium's goal |
|---|---|---|
| Units | Add-on package (Unitful.jl); mismatches surface at runtime | Built into the compiler; unit errors caught **before the program runs**, with **zero runtime cost** (units erased after checking) |
| Calculus | Separate packages with different conventions | Part of the language: `x'`, `d/dt`, `∫ … d…`, `solve … with …` |
| Notation | `4π^2*L/T^2` | `4π² L / T²` |
| Errors | Long stack traces | One clear line in physics terms: *"line 4: can't add length [m] to time [s]"* |
| Startup | Slow "time to first plot" | Fast startup is a measured goal |

**Priority order when goals conflict:** (1) unit safety, (2) readability, (3) calculus, (4) performance. Correctness always beats speed.

---

## 2. Architecture

- **Front end in Python 3** (readable — the user is learning Python): lexer → parser → AST → name resolution → type & **dimension inference/checking** → symbolic calculus passes → typed IR.
- **Back end via LLVM** using `llvmlite`: compile the typed IR to native code (JIT for `fermium run` and the REPL; optional AOT build to a standalone executable is a stretch goal).
- **Numbers are objects in the compiler, not bare floats**: every value carries a type and a dimension. Leave an obvious slot for a future uncertainty component (see §3.7).
- **Runtime library**: numerics that must be fast (ODE steppers, quadrature, loops) are compiled to native code. Non-hot-path features (`plot`, `fit`, `load`, hard symbolic integrals) may call NumPy / SciPy / SymPy / matplotlib.
- Package it installable with `pip install -e .`, exposing a `fermium` command.

Suggested layout (adjust if you have good reason, and document why):

```
fermium/
  fermium/            # compiler + runtime package
    lexer.py parser.py ast.py types.py units.py
    calculus.py codegen_llvm.py runtime/ repl.py cli.py fmt.py
  stdlib/             # constants, units database, prelude written in Fermium where possible
  tests/              # pytest; unit, integration, golden-output, fuzz
  examples/           # *.fm example programs
  benchmarks/         # fermium / julia / python versions + runner + results
  bootcamp/           # beginner course (see §5)
  docs/               # language reference, design decisions
  editors/vscode/     # syntax highlighting + \name<TAB> completion
  PROGRESS.md BACKLOG.md DECISIONS.md MORNING_REPORT.md README.md
```

---

## 3. Language Design

### 3.1 Target feel

```
# Pendulum: measure g
L = 1.20 m
T = 2.21 s
g = 4π² L / T²
print g                      # 9.70 m/s²
print g in ft/s²

# Mass on a spring
k = 50 N/m
m = 0.5 kg
ω = √(k/m)                   # 10 rad/s
x(t) = A cos(ω t)
v = d/dt x                   # symbolic: -A ω sin(ω t), units m/s
a = x''

F(x) = k x
W = ∫ F(x) dx from 0 m to 0.2 m     # 1.0 J

solve m x'' = -k x - b x'
  with x(0) = 0.1 m, x'(0) = 0 m/s
  for t from 0 s to 5 s
plot x vs t

data = load "pendulum.csv"          # headers like: L [m], T [s]
fit T = 2π √(L / g) to data         # reports g with units
```

### 3.2 Core language

- Variables, functions (`f(x) = …` one-liners and multi-line blocks), `if`/`else`, `for`/`while`, lists/arrays, ranges with units (`for t from 0 s to 10 s step 0.1 s`), `where` clauses (`E = ½ m v² where m = 2 kg, v = 3 m/s`), comments with `#`.
- Static typing with inference — users should almost never write a type.
- `print x in eV` / `to(x, eV)` for conversion; output uses sensible significant figures and pretty units (`m/s²`).

### 3.3 Units & dimensions

- Dimensions over the 7 SI base quantities with rational exponents; angles handled deliberately (document the choice).
- Unit database: SI + prefixes, plus physics units (eV, MeV, GeV, fm, Å, u/amu, barn, c-relative units, AU, ly, pc, M☉, erg, G, K, °C handled correctly as affine, °, rad, atm, bar, Torr…).
- Constants library with CODATA values and units: `c, G, h, ħ, k_B, e, m_e, m_p, m_n, N_A, ε₀, μ₀, σ, α, …`. Cite the source in the code.
- All checking at compile time; generated code contains no unit logic.
- Equations like `solve m x'' = -k x` check that both sides have the same dimension.

### 3.4 Syntax rules you must decide, document in `DECISIONS.md`, and test

1. **Implicit multiplication**: `4π² L`, `2x`, `k x` multiply. `LT` is one identifier. Define precedence clearly (e.g. how `1/2x` parses — pick the physicist's expectation and emit a warning where ambiguous).
2. **Units vs. variables** — `m` is both "meters" and "mass". Starting rule (refine if needed): a unit name *immediately following a numeric literal* is a unit (`3 m` = 3 meters); bracketed units are always units (`x [m]`, `3 [m/s]`); everywhere else an identifier is a variable. If a bare unit after a number collides with an in-scope variable, emit a clear warning suggesting `3*m` or `3 [m]`.
3. **Unicode ⇄ ASCII equivalence**: every symbol has an ASCII spelling that means exactly the same thing — `pi`/π, `theta`/θ, `omega`/ω, `hbar`/ħ, `sqrt(x)`/√x, `x^2`/x², `integral`/∫, `partial`/∂, `+-`/± (reserved), `deg`/°, `*`/·. The ASCII form is first-class, not a fallback.
4. **Look-alike characters**: normalize or warn on confusables (micro sign `µ` U+00B5 vs Greek `μ` U+03BC, Latin `v` vs Greek `ν`, Latin `o` vs Greek `ο`, etc.). Never let two visually identical names silently be different variables.

### 3.5 Calculus

- **Derivatives**: symbolic (rule-based, your own implementation), via `x'`, `x''`, `d/dt x`, `dx/dt`; result units = numerator / denominator units. Simplify results enough to be readable.
- **Integrals**: definite integrals numerically (adaptive Gauss–Kronrod or similar), compiled; indefinite/symbolic integrals when possible (SymPy allowed as backend). Units multiply by the integration variable's units.
- **ODEs**: `solve … with … for …` for scalar and small systems of ODEs; compiled RK4 plus an adaptive method (Dormand–Prince RK45); results usable in `plot`, indexing, and further math.
- Partial derivatives (`∂/∂x`) are a Tier 5 backlog item unless time allows.

### 3.6 Data tools

- `load "file.csv"` with units read from column headers (`T [s]`).
- `fit <model> to data` — nonlinear least squares; report fitted parameters with units (and standard errors as plain numbers until §3.7 lands).
- `plot y vs x` — PNG output with labeled axes including units.

### 3.7 Explicitly deferred: uncertainties

Error bars (`5.0 ± 0.2 m`) are **out of scope** tonight. Reserve `±` / `+-`: using them must produce a friendly *"uncertainties are planned for a future version"* message. Keep the value representation ready so adding them later is an extension, not a rewrite.

### 3.8 Tooling

- `fermium` → REPL with history and **`\name<TAB>` → symbol completion** (`\theta` → θ, `\hbar` → ħ, `\int` → ∫, `\sqrt` → √, `\^2` → ², LaTeX-style names).
- `fermium run file.fm`
- `fermium fmt file.fm --pretty` (ASCII → symbols) and `--ascii` (symbols → ASCII). Round-tripping must not change program meaning — test it.
- `fermium doctor` → checks installation (Python version, llvmlite, optional deps) and prints fixes in plain English.
- `editors/vscode/`: syntax highlighting + the same `\name` completion.

---

## 4. Goal Ladder

Work top to bottom. Each item has a concrete "done when". Don't start a tier until the one above is solid enough to build on, but you may interleave tests/docs throughout.

### Tier 1 — Working language (must have)
- Lexer/parser for the syntax in §3, with the rules in §3.4. *Done when:* parser tests cover every construct and every ambiguity rule.
- Dimension checker. *Done when:* ≥ 50 tests of programs that must be **rejected** with a correct, readable message, and ≥ 50 that must be accepted.
- LLVM codegen + `fermium run` + REPL. *Done when:* the §3.1 pendulum snippet runs end-to-end from a `.fm` file and in the REPL.

### Tier 2 — Physics features
- Symbolic derivatives, numeric integrals, ODE solver, constants library, `load`/`fit`/`plot`.
- *Done when:* every snippet in §3.1 runs, and derivative/integral/ODE results match SymPy/SciPy reference values to within stated tolerances in automated tests.

### Tier 3 — Performance vs. Julia
- Install Julia (official binaries) and Python/NumPy/SciPy in this environment.
- Benchmarks, each written idiomatically and fairly in Fermium, Julia, pure Python, and NumPy/SciPy:
  1. N-body orbit simulation (e.g. Sun + planets, fixed steps)
  2. Damped spring ODE (fixed-step RK4 and adaptive)
  3. Numerical integral (e.g. blackbody spectrum)
  4. A tight loop with unit-carrying arithmetic (proves units cost nothing at runtime)
  5. Startup / time-to-first-result for a trivial script
- Report median of repeated runs, with and without compile/JIT time, on the same machine. Record results in `benchmarks/RESULTS.md`.
- Profile and optimize. *Target:* within 2× of Julia on 1–4, far faster than pure Python. **Never fudge benchmarks** — no crippled Julia code, no cherry-picked runs. If Fermium loses, say so and why.

### Tier 4 — Polish
- Error messages: rewrite each into one plain line + a caret pointing at the problem + a suggestion. No Python tracebacks ever reach the user for user errors.
- `docs/reference.md`: complete language reference.
- 25 example programs in `examples/`, each with a header comment explaining the physics. Must include: projectile motion, pendulum, damped spring, Kepler orbit, escape velocity, blackbody peak (Wien), radioactive decay, Bateman decay chain, semi-empirical mass formula / binding energy per nucleon, nuclear radius R = r₀A^(1/3) in fm, Q-value of a reaction, Coulomb barrier, Lane–Emden equation (polytropic stars), hydrostatic equilibrium, relativistic energy/momentum, RC circuit, heat equation (1-D, simple), and a data-fitting example with a CSV.

### Tier 4b — Fermium Bootcamp (for someone who has never coded)
Written like a friendly Python bootcamp, in `bootcamp/`:
- **Lesson 0 — Setup**: installing Fermium on macOS step by step (opening Terminal, installing Python, `pip install`, running `fermium doctor`), running a first program, what to do when something goes wrong.
- **Lessons 1–10**, one idea each, taught through physics:
  1. Numbers & units 2. Variables & formulas 3. Functions 4. Conditions & loops 5. Lists & data 6. Loading lab data & plotting 7. Derivatives 8. Integrals 9. Differential equations (springs, orbits, decay) 10. Final project: simulate a planet's orbit from scratch
- **Symbols section (early, around Lesson 1–2)**: teach ASCII spelling first (`pi`, `theta`, `sqrt`, `^2`), then show both upgrades explicitly with examples:
  - **Tab completion**: type `\theta` then press Tab in the REPL or VS Code → `θ`.
  - **`fermium fmt --pretty`**: write in plain ASCII, then convert the whole file to symbols (and `--ascii` to convert back); show a before/after.
  - Mac Option-key shortcuts (Option+P π, Option+V √, Option+B ∫, Option+D ∂, Option+Shift+= ±).
- Every lesson ends with 3–5 exercises; solutions live in `bootcamp/solutions/`.
- `bootcamp/CHEATSHEET.md`: one printable page of every symbol, its ASCII form, its `\tab` name, and core commands.
- `bootcamp/TROUBLESHOOTING.md`: the 20 most likely errors, what they mean, how to fix them.
- **Every code sample in the bootcamp is executed by the test suite**, so lessons can't drift out of date.
- Tone: encouraging, no jargon without explanation, assume zero prior coding.

### Tier 5 — Never-ending hardening (work here until 7 AM)
Pick the highest-value item each time; add new ideas to `BACKLOG.md` as you find them:
- Cross-check every numeric feature against SciPy/SymPy on randomized inputs.
- Fuzz the lexer/parser (random and mutated programs) — no crashes, only clean errors.
- Property tests for units (e.g. dimension algebra laws, conversion round-trips, `fmt --pretty/--ascii` round-trips).
- Raise test coverage toward 95%.
- More optimization + re-run benchmarks.
- Backlog features: vectors & matrices (`<3, 4> m/s`, `|v|`), partial derivatives, AOT standalone executables, better symbolic simplification, groundwork for §3.7 uncertainties, a browser playground.
- Re-read your own docs and bootcamp as a beginner would and fix anything confusing.

---

## 5. Operating Rules

1. **Not done before 7 AM Eastern (11:00 UTC).** If everything in Tiers 1–4b is done, work on Tier 5. If `BACKLOG.md` is empty, review the project critically and add to it. Going idle early is the main failure mode to avoid.
2. **Git**: initialize a repo; commit after every working step with a clear message. Never commit a broken build to `main` — use a branch if you must experiment.
3. **`PROGRESS.md`**: keep current at all times — done / in progress / next / blocked, with timestamps. Anyone (including a fresh agent resuming your work) should be able to read it and continue immediately.
4. **Resuming**: if you are starting fresh or were interrupted, read `PROGRESS.md`, `DECISIONS.md`, `BACKLOG.md`, and `git log` first, run the test suite, then continue from "next".
5. **Stuck > 30 minutes** on one problem: write what you tried in `PROGRESS.md` under *Blocked*, move to another item, come back later.
6. **Tests are the source of truth.** Never weaken or delete a test to make it pass unless the test itself is wrong — and then explain why in the commit message.
7. **Honesty**: in docs and reports, never claim something works unless a test proves it. Mark partial features as partial.
8. **Record design choices** in `DECISIONS.md` (what, why, alternatives considered).
9. Stay inside this project directory. Network use only for installing packages/tools (pip, Julia binaries).

## 6. Extra Requirements

- **`CLAUDE.md` first**: before writing any code, create `CLAUDE.md` at the repo root summarizing the mission, the operating rules in §5, and "read PROGRESS.md before doing anything". This way any resumed or restarted session follows the same rules.
- **Pacing checkpoints** (guides, not hard limits — they exist to stop perfectionism on early tiers): Tier 1 working end-to-end by ~1:00 AM ET, Tier 2 by ~3:30 AM, Tier 3 benchmarks with first results by ~5:00 AM, Tier 4/4b drafts by ~6:00 AM. If you're far behind, simplify and move on; note what was cut in `BACKLOG.md`.
- **Rosetta page** (`docs/rosetta.md`): 6–8 of the example programs shown side by side in Fermium, Julia, and Python, so the readability claim is visible at a glance. Every snippet must actually run.
- **Demo gallery**: each example that produces a plot saves it to `examples/gallery/`; the README shows the best 3–4 with the code that made them.
- **Keep the build green**: a single `make check` (or `./check.sh`) that runs lint, the full test suite, bootcamp snippets, and examples. Run it before every commit.

## 7. Morning Report (write at ~6:45 AM Eastern, finalize by 7:00)

`MORNING_REPORT.md`, written for a beginner:
1. One-paragraph summary of what Fermium can do right now.
2. **Try it in 5 minutes**: exact commands to install and run three impressive demos.
3. Tier-by-tier checklist: done ✅ / partial ⚠️ / not started ❌.
4. Benchmark table vs. Julia and Python, with an honest paragraph on where Fermium wins and loses.
5. Test counts and coverage.
6. Known bugs and weaknesses.
7. Suggested next steps, including when uncertainties (§3.7) could be added.
