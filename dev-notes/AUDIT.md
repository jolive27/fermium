# Fermium audit

Audited 2026-09-25 from 00:05 to 00:25 UTC against `FERMIUM_SPEC_1.md` and the docs as they were on disk at that time. The docs and compiler were being edited during the audit: README, reference.md and DECISIONS.md changed while it ran, and `codegen_llvm.py` changed at 00:15. Line numbers are from about 00:21 UTC.

**How it was checked:** every command, unit, constant, built-in and syntax form below was actually run. Runs that write files (examples, plots, doc blocks) used a copy of the repo in the scratchpad. Nothing in the repo was edited apart from this file.

**Baseline:**
- `ruff` is clean.
- `pytest`: 1382 passed and 19 xfailed (all strict xfails of known bugs), in 90 s.
- Coverage is 90%.
- All 25 examples run, and each one prints the same output under `--interp` as it does compiled.
- All 7 rosetta output boxes match.
- All 25 examples survive `fmt --ascii` and then `fmt --pretty` with identical output.

---

## 1. Spec requirements checklist

### §2 Architecture
| Req | | Evidence |
|---|---|---|
| Python front end: lexer → parser → AST → checker → IR | ✅ | `fermium/{lexer,parser,ast,checker,ir}.py` |
| LLVM back end via llvmlite, JIT for run and the REPL | ✅ | `--emit-llvm` prints the IR. `--time` shows the LLVM+JIT stage. |
| AOT executable (stretch goal) | ✅ | `fermium build` works. Output matches `run` for `11_q_value.fm`. plot/load/fit are refused with a clear message. |
| Values carry a type and dimension, with a slot for uncertainty | ✅ | `types.NumTy`, `ast.Uncertain` and the other references in docs/uncertainties.md all exist. |
| Runtime: numerics native, plot/fit/load through Python | ✅ | D2/D3 |
| `pip install -e .` gives a `fermium` command | ✅ | `/usr/local/bin/fermium`, version 0.1.0 |
| Layout: `stdlib/` holds constants, the units database and a prelude | ⚠️ | `stdlib/` is empty. Constants and units live in `fermium/constants.py` and `fermium/units.py`, and this isn't documented as a deviation. |
| `MORNING_REPORT.md` | ❌ | Missing. It is due at 12:30 UTC under the extended plan in CLAUDE.md. |

### §3.1–3.2 Core language
| Req | | Evidence |
|---|---|---|
| The §3.1 snippet runs | ✅ | It runs once the undefined `A` and `b` are given values. Output: `9.70 m/s²`, `31.8 ft/s²`, `v(t) = -A ω sin(ω t)`, `1.0 J`, a plot, and the fit `g = 9.818 m/s²`. `ω` prints as `10 1/s`, not `10 rad/s` (a documented choice, D6). |
| Variables, one-line and block functions, if/else, for/while, lists | ✅ | All tested |
| Ranges with units and `step` | ✅ | A missing step gives a clear error. |
| `where` clauses | ✅ | `E = ½ m v² where m = 2 kg, v = 3 m/s` prints `9 J`. |
| Static typing with inference | ✅ | Reassigning a variable with other units is rejected. The REPL allows it. |
| `print x in eV`, `to(x, eV)`, significant figures, pretty units | ✅ | |
| Comments with `#` | ✅ | |

### §3.3 Units and dimensions
| Req | | Evidence |
|---|---|---|
| 7 base dimensions with rational exponents | ✅ | `sqrt(8 m³)` gives `m^(3/2)` |
| Angles handled deliberately and documented | ✅ | D6 (angles are dimensionless) |
| eV, MeV, GeV, fm, Å, u/amu, barn | ✅ | All parse. SI values checked. |
| c-relative units | ✅ | `1 amu in MeV/c^2` gives 931.494. `p in GeV/c` works. `0.5 c` works. |
| AU, ly, pc, M☉, erg, K, °, rad, atm, bar, Torr | ✅ | All parse, with correct SI values. |
| **G (gauss)** | ⚠️ | Deliberately spelled `gauss`/`Gs` (D7). `2 G` silently means 2 × the gravitational constant, with no warning. |
| °C affine | ✅ | `20 °C + 5 K` is 25 °C. `°C + °C` and `J/°C` are rejected. `T2 - T1` gives K. |
| Constants c, G, h, ħ, k_B, e, m_e, m_p, m_n, N_A, ε₀, μ₀, σ, α | ✅ | All exist with CODATA 2022 values, which I checked by hand. The source is cited in `constants.py`. |
| Checking at compile time, no unit logic in generated code | ✅ | `fermium check`, and `test_units_erased_same_result`. |
| `solve` checks the dimensions of both sides | ✅ | `solve m x'' = -k` reports "left is force [N], right is spring constant [N/m]". |

### §3.4 Syntax rules
| Req | | Evidence |
|---|---|---|
| 1. Implicit multiplication, `LT` as one name, `1/2x` with a warning | ✅ | `LT` gives an error suggesting `L T`. `1/2x` warns. |
| 1. …but it has correctness holes | ⚠️ | `2(x+1)^2` = 16 for x = 1, and `2½` = 1. Both are silent (see §4). |
| 2. Unit after a number, bracketed units, collision warning | ✅ | `3 m` with a mass `m` warns and suggests `3*m`. |
| 2. `x [m]` bracket on a variable | ⚠️ | Only works as a parameter annotation: `x [m] = 3` is a parse error. |
| 3. Unicode ⇄ ASCII for pi, theta, omega, hbar, sqrt, ^2, integral, partial, +-, deg, * | ✅ | All spellings work. |
| 4. Look-alikes: µ/μ, v/ν, o/ο, Greek Α and Cyrillic а | ✅ | Normalized, or a warning is given. |
| Documented in DECISIONS.md | ✅ | D7–D10 |

### §3.5 Calculus
| Req | | Evidence |
|---|---|---|
| Symbolic derivatives: `x'`, `x''`, `d/dt x`, `dx/dt`, `d²x/dt²` | ✅ | All work, with correct units. |
| Derivatives are one-line functions only | ⚠️ | Multi-line functions are refused with a clear message. `d/dt (3 t^2)` prints the name `d/dt(...)(t)`. |
| Definite integrals, compiled adaptive Gauss–Kronrod, infinite limits | ✅ | …but see the narrow-peak bug in §4. |
| Indefinite integrals through SymPy | ✅ | Prints the odd name `∫dx(x) = x³/3`. |
| ODEs: RK4 and DP45, systems, vector unknowns | ✅ | Solutions work with `plot`, `x(t)`, `x'(t)`, `values`, `times`, `x[end]` and `max`. |
| Partial derivatives (optional) | ⚠️ | `∂/∂x f` works for one-line functions only. |

### §3.6 Data
| Req | | Evidence |
|---|---|---|
| `load` with units from the header | ✅ | |
| `fit` with units and standard errors | ✅ | `fit T² = k L` and `with` guesses also work. |
| `plot` PNG with unit axis labels | ✅ | Checked in `gallery/damped_spring.png`. |

### §3.7 Uncertainties
| Req | | Evidence |
|---|---|---|
| `±` and `+-` give a friendly message | ✅ | |
| Representation ready for them | ✅ | docs/uncertainties.md. Every code reference in it exists. |
| Side effect of reserving ± | ⚠️ | `fermium fmt` refuses to format any file containing ±. |

### §3.8 Tooling
| Req | | Evidence |
|---|---|---|
| REPL `\name<TAB>` | ✅ | Checked through a pty: `\theta`, `\sqrt`, `\int`, `\pi` and `x\^2` complete. `\name` is also replaced on Enter. |
| REPL history | ⚠️ | Up-arrow works within a session. `~/.fermium_history` is never written, so history is lost between sessions (§4, item 7). |
| `fermium run` | ✅ | |
| `fmt --pretty` / `--ascii`, meaning-preserving round trip | ⚠️ | All examples round-trip, but `3*10^8 m/s` → `3·10⁸ m/s` no longer compiles (A14). |
| `fermium doctor` | ✅ | Checks Python, llvmlite, numpy, the optional dependencies, and an end-to-end compile. |
| `doctor` checks what `build` needs | ⚠️ | Doesn't check for a C compiler, which `build` needs. |
| VS Code: highlighting and `\name` completion | ✅ | The symbol table is identical to the REPL's (91 entries) and covers every cheat-sheet name. Not tested in a real VS Code. |

### §4 Tiers
| Req | | Evidence |
|---|---|---|
| T1: ≥50 rejected and ≥50 accepted dimension tests | ✅ | 84 rejected and 94 accepted (`tests/test_dimensions.py`). |
| T1: pendulum from a file and in the REPL | ✅ | Piped REPL prints `9.70 m/s²`. |
| T2: cross-checks against SymPy/SciPy | ✅ | `test_numerics.py`, `test_differential.py` |
| T3: 5 benchmarks × 4 languages, medians, with and without JIT | ✅ | benchmarks/ |
| T3: within 2× of Julia on 1–4 | ✅ | RESULTS.md at 00:09 shows nbody 1.28×, RK4 1.82×, blackbody 0.91×, unit loop 1.09×. |
| T3: spring_adaptive gives the same result | ❌ | Results disagree (✗) in RESULTS.md: Fermium's x(100 s) = −2.34e-9 m against −1.95e-10 m for Julia and SciPy. The analytic value is about 1e-12. |
| T4: errors are one line, a caret and a hint | ✅ | Mostly. See the EOF line-number and assert bugs. |
| T4: no Python tracebacks | ❌ | Non-UTF-8 files and directories produce tracebacks. Runaway recursion segfaults. |
| T4: `docs/reference.md` | ✅ | Some gaps (§2). |
| T4: 25 examples with header comments, including the required list | ✅ | All 18 required topics are present (`01`…`18`). The CSV fit is `18_fit_decay_data.fm`. |
| T4b: bootcamp lessons 0–10, symbols lesson, Option keys, fmt before/after | ✅ | Lesson list matches the spec. The Option-key table includes ⌥⇧= for ±. |
| T4b: 3–5 exercises per lesson, with solutions | ✅ | 4–5 per lesson. Lesson 10 has 6 "project extensions". |
| T4b: cheat sheet and 20 troubleshooting entries | ✅ | |
| T4b: every bootcamp sample executed by tests | ⚠️ | Every ```` ```fermium ```` block runs, but the output boxes are **not** compared, and 13 of them are stale (§2). |
| T5: fuzzing, property tests, coverage | ✅ | Coverage is 90% (target 95%). |
| T5: vectors | ✅ | |
| T5: matrices | ❌ | |
| T5: AOT | ✅ | |
| T5: browser playground | ❌ | |
| §6: CLAUDE.md, `make check`, rosetta (7 programs), gallery (20 PNGs, 4 in the README) | ✅ | |

---

## 2. Doc claims that are false or stale (most severe first)

1. **README.md:117.** The claim: "Damped spring, adaptive RK45, **same accuracy**, ~1.1× Julia (spot check)".
   - **What happens:** RESULTS.md (00:09) has 0.41× and **✗ results disagree**. Fermium takes 2066 steps against Julia's 3713, and its error is about 12× larger.
   - **Fix:** say "results currently disagree (see RESULTS.md)" until a full run shows agreement.
   - Separately, RESULTS.md predates the 00:15 codegen change, so the whole table may be **stale**.
   - With the ✗ in place, `run.py` (and `make bench`) exits with code 1.
2. **bootcamp/README.md:65.** The claim: "the examples **and outputs** you see here really work".
   - **What happens:** `test_docs.py` only checks that blocks run. It never compares the `<!-- output -->` boxes, and **13 of 154 boxes differ** from the real output:
     - lesson01:250 and lesson02:148: the warning hint text changed.
     - lesson07:11, :31, :52 and :143: the derivative formatting (`m/s²` instead of `m/s^2`, `9.81 m/s² t`) and the significant figures (`10.2` against `10.19`, `0.380` against `0.38`).
     - lesson09:97 (energy 0.249999940 against …794) and :147 (542.848 against 542.865).
     - lesson10:262 (−0.291056 against −0.291036).
     - solutions/lesson06:34, lesson07:7, lesson07:63 and lesson09:96.
   - **Fix:** run `bootcamp/update_outputs.py`, and add a test that compares the boxes (ignoring warning order).
3. **The fmt round-trip claim is false in one case** (docs/reference.md:385, bootcamp/lesson02b_symbols.md:132 "never changes what the program does", DECISIONS.md:76).
   - **What happens:** `v = 3*10^8 m/s` pretty-prints to `3·10⁸ m/s`, which fails with "m isn't defined". This is known bug A14 (xfail).
   - **Fix:** fix the lexer, or qualify the claim.
4. **docs/reference.md:321.** The claim: `gamma` is a built-in.
   - **What happens:** `gamma(0.5)` fails with "γ isn't defined". `Γ(0.5)` fails too (A25).
   - **Fix:** fix the rewrite, or remove it from the table.
5. **docs/reference.md:384.** The claim: "It keeps history."
   - **What happens:** only within one session. `cli.entry` exits with `os._exit`, so the `atexit` handler that writes `~/.fermium_history` never runs.
   - **Fix:** call the save explicitly before `os._exit`, or say "within a session".
6. **README.md:12.** The claim: `# line 6: can't add length…`.
   - **What happens:** that statement is on line 7 of the snippet, and the real message says `line 7`.
7. **README.md:134.** The claim: "including every code block in the docs and bootcamp".
   - **What happens:** only ```` ```fermium ```` blocks run. These plain blocks are never run: README:5–13, reference §11 (the load/fit/plot block, which does work), the cheat-sheet one-pager, and all output boxes.
   - **Fix:** say "every `fermium` code block".
8. **The reference is missing syntax the code uses:**
   - `solve … tolerance 1e-10`, `using rk4|rk45` / `method …` (parser.py:414–427), and `clock()`.
   - `benchmarks/fermium/spring_adaptive.fm` uses `tolerance` and `clock()`.
   - **Fix:** add them to reference §10, §13 and the §18 grammar.
9. **docs/reference.md:56.** The claim: "Units in brackets are always units: … `x [m]`".
   - **What happens:** `x [m] = 3` is a parse error. It only works as a parameter annotation, `f(x [m])`.
10. **PROGRESS.md is stale:**
    - The header "Last updated 23:40" sits above a 00:05 entry.
    - "1066 tests passing" (line 39): now 1382 pass and 19 xfail.
    - "Coverage 87%" (line 31): now 90%.
    - "Next" (line 49) still lists AOT `fermium build`, which is done.
    - "`fit` gives g = 9.806" (line 22): `examples/data/pendulum.csv` gives 9.818 and `bootcamp/data` gives 9.856.
    - It doesn't list the 19 known xfail bugs, although README.md says PROGRESS lists "the known issues".
11. **BACKLOG.md is stale:**
    - Still unchecked but done: vectors (line 14; matrices aren't done), AOT (16), fuzzing (7, `test_fuzz.py`), property tests (8, `test_properties.py`), and `print … to N digits` (21).
    - Line 24 says "There is a warning" for `2 g h`. There is one only if the user has defined `g`.
    - `2 G` and `2 h` give no warning at all, and silently mean 2 × the gravitational constant and 2 × Planck's constant.
12. **DECISIONS.md:**
    - Line 120 says "Revised at **00:30**", but the file was saved at 00:15.
    - D8 says `1/2 m` warns. With `m` as the unit (no variable `m`) it silently gives `0.5 1/m`.
    - D2 says a C compiler isn't available on a beginner's Mac, while D25 (`build`) requires one. Say "optional".
13. **README.md:121.** "0.1–0.6 s against 1–3 s" is fine for the benchmarks, but the Julia startup benchmark is 0.28 s. Say "against 0.3–3 s".
14. **editors/vscode/README.md:3.** The claim: "units after numbers are coloured separately from variables".
    - **What happens:** the regex colours *any* name after a number as a unit, so the `x` in `2 x` is coloured as a unit. Cosmetic.
15. **docs/uncertainties.md:** every code reference in it exists. `9.806 ± 0.017` is only illustrative, so no change is needed.
16. **CHEATSHEET.md and docs/rosetta.md:** every claim I checked holds: symbols, `\names`, commands, constants and units. All 7 rosetta output boxes are exact.

---

## 3. Stubbed or partial features

- **Uncertainties:** only reserved (`±` gives an error). This is by design.
- **Matrices and lists of vectors:** not implemented. `[<1,2>, <3,4>]` is rejected, as documented.
- **Derivatives and ∂:** only of one-line functions.
- **`fermium build`:** no plot, load or fit. `doctor` doesn't check for `cc`.
- **Browser playground and LSP (hover, live errors):** not started. Both are Phase 2 in CLAUDE.md.
- **`stdlib/`:** empty.
- **MORNING_REPORT.md:** not written yet.
- **Gauss:** only as `gauss`/`Gs`, not the spec's `G`.
- **REPL history:** per session only.
- **`:vars`:** shows `v: vec` and `xs: list` without values or units.
- **The 19 strict-xfail known bugs** (`tests/test_adversarial.py`): A3, A12, A14, A15 ×2, A16, A17, A18, A19, A20, A21, A22, A25, A26, A27 ×3, A28, A29 and A30. Several of them produce silent wrong answers (§4).

---

## 4. Broken things (most severe first; each has a minimal repro)

**Silent wrong answers**

1. **A variable assigned only in a branch or loop that never ran still has a value** (A27):
   - `if 1 > 2` / `    y = 5 m` / `print y` prints `5 m`.
   - `for i from 1 to 0` / `    z = 3 m` / `print z` prints `3 m`.
   - It should be a compile error: "y may not be defined".
2. **`2(x+1)^2`** with `x = 1` prints `16`; the right answer is 8 (A29). The `^` applies to the whole juxtaposed product.
3. **`print 2½`** prints `1`, not 2.5 (A28).
4. **Using a function before its definition picks up the constant** (A21): `print h(2)` / `h(x) = 3 x` prints `1.32521×10⁻³⁴ J s`.
5. **A narrow peak exactly at a bisection point gives exactly half the integral, with no warning:**
   - `print ∫ exp(-(x-1000)^2*100) dx from 0 to 2000` prints `0.0886227`. The true value is 0.177245.
   - Peaks at 500 and 1500 give the same half. A peak at 1000.5 is correct.
   - It looks like a kernel bug, not ordinary quadrature failure.
6. **`2 G` and `2 h`** silently mean 2 × G_Newton and 2 × Planck, with no warning. For a student who writes `2 G` meaning gauss, the result is 1.3e-10 m³/(kg s²).
7. **`floor`, `ceil` and `round` on quantities with units work in SI:** `floor(270 cm)` gives `200 cm`, and `round(1.5 km)` gives `1.5 km`. Document this, or reject non-dimensionless arguments.
8. **`sqrt(-1)` gives `NaN` and `factorial(-1)` gives `∞`,** silently.

**Crashes and tracebacks**

9. **Runaway recursion segfaults** (A20): `f(n) = 1 + f(n+1)` / `print f(1)` ends in `Segmentation fault`, exit code 139. The tail-recursive form `f(n) = f(n+1)` hangs forever.
10. **Python tracebacks:**
    - `printf '\xff\xfe' > b.fm; fermium run b.fm` gives a `UnicodeDecodeError` traceback, and so do `fmt` and `check`.
    - `mkdir d.fm; fermium run d.fm` gives an `IsADirectoryError` traceback.
    - The fix is in `cli._read`.
11. **The spring_adaptive benchmark result disagrees with Julia and SciPy** (see §2, item 1).

**Wrong behaviour and polish**

12. **`fmt --pretty` breaks programs:** `v = 3*10^8 m/s` → `3·10⁸ m/s` fails with "m isn't defined" (A14).
13. **REPL history isn't saved.** Run `fermium`, type something, `:quit`: no `~/.fermium_history` is created. The cause is `os._exit` in `cli.py:162`, which skips `atexit`.
14. **`gamma(0.5)`** fails with "γ isn't defined" (A25).
15. **A parse error at the end of the file reports a line that doesn't exist:** `print 1 +` (a one-line file) gives `line 2: this line ended before…`, with an empty source line under it.
16. **An assert message repeats the line** (A12): `assert 1 m > 2 m, "bad"` prints `b.fm, line 2: line 2: bad`.
17. **`fermium missing.fm`** gives an argparse error ("invalid choice"), not the friendly "can't find the file" message that `fermium run missing.fm` gives.
18. **`fermium fmt` refuses any file that contains `±`,** so a file that uses it can't be formatted at all.
19. **Minor:**
    - AOT runtime errors say "the list has 1 elements", where the JIT says "1 element".
    - `print "hi" to 3 digits` silently ignores `to 3 digits`.
    - Indefinite integrals and `d/dt (formula)` print odd names: `∫dx(x)` and `d/dt(...)(t)`.
