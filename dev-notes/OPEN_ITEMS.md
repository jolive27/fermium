# Open items (spec A0 triage)

Written 2026-09-25 for spec item A0 (`dev-notes/FERMIUM_SPEC_V1.5_V2.md`). It collects every open, partly fixed,
"by design" or "documented" item from `BACKLOG.md`, `gauntlet/FRICTION.md`, `dev-notes/REDTEAM.md`,
`dev-notes/notes/bugs-*.md`, `docs/reference.md` §19 and the workaround decisions in `DECISIONS.md`.

**Method.** Each repro was re-run with `fermium run` on the current working tree (commit `25adb47`, with the
staged A9 moves; `fermium` resolves to `/home/user/fermium/fermium`). The repro files were in `/tmp/claude-0/a0/`
and are not in the repo. "Prints now" is the first line or two of the current output. **Not re-run** means that
the item was not tested here, and the row says why. The full test suite was not run for this triage.

**Classification** (exactly one per row):
- **A1…A9**: fix in Phase A, under that spec item.
- **B**: fix natively in the Rust compiler (spec B2's list). Keep the v1.5 behaviour as the oracle and log the
  difference in `DIVERGENCES.md`.
- **C/D**: a later-phase feature (the C or D item is named).
- **design**: by design, with the reason given.
- **closed**: it no longer reproduces. Mark it fixed in its source file (for BACKLOG, tick the box).

## Summary

| Classification | Rows |
|---|---|
| Fix in A (A1–A9) | 22 |
| Fix natively in B | 23 |
| C/D feature | 20 |
| By design | 17 |
| Closed (no longer reproduces; update the source) | 29 |
| **Total rows** | **111** |

**The Fix-in-A rows by item:** A1: 7, A2: 2, A3: 4, A4: 5, A5: 1, A6: 1, A8: 2. No rows fall under A7 or A9, but
the stale bootcamp text and one hygiene note are listed at the end. The spec's own A3/A4/A6 items that were
confirmed here are listed separately. There is **one new crash** (NEW-1, below). Some rows repeat the same
problem from another source (for example BL-2, AD-A3 and L-3), so the counts are rows, not distinct bugs. Several items were fixed
without their notes being updated; those are the **closed** rows.

## New finding (during this triage)

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| NEW-1 | A vector times an uncertain value crashes | **yes**: `L = 1.20 ± 0.01 m` then `v = <1, 2> * L` gives `internal error in Fermium: TypeError: type UFloat doesn't define __round__ method` | A3 | `<L, L>` and `∫ L dx` already give the clean "can't use uncertain values (±) yet … value(x)" error. `<1, 2> m * L` crashes too. Use the same error here and add a test |

## BACKLOG.md

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| BL-1 | A narrow peak exactly at a subdivision point gives half the integral | yes: `∫ exp(-(x-1000)^2*100) dx from 0 to 2000` gives `0.0886` (true value 0.177), no warning | B | B2: quadrature half-peak |
| BL-2 | A narrow peak in a huge finite range gives 0 (A3 leftover) | yes: `∫ exp(-x²) dx from -1e6 to 1e6` gives `0`, with the D110 warning | B | B2: narrow peaks |
| BL-3 | A strong singularity away from 0 is rejected (A56 leftover) | yes: `abs(x - 0.3)^(-0.6)` and `^(-0.8)` stop with "the integrand is infinite at x = 0.300" | B | B2: ε-algorithm |
| BL-4 | `sqrt(-1)` gives NaN and `factorial(-1)` gives ∞ silently | yes: `NaN`, `∞` (and `1/0` gives `∞`) | A3 | Decide and test. Julia raises DomainError for `sqrt(-1.0)` and `factorial(-1)` and returns Inf for `1/0`. Same item as bootcamp B10 |
| BL-5 | Re-run the benchmarks (the adaptive spring disagreed with Julia) | no: benchmarks/RESULTS.md shows spring_adaptive ✓ after the red team round 1 #3 fix (not re-run here) | closed | |
| BL-6 | Raise test coverage toward 95% | not measured (last measurement 90%, per MORNING_REPORT) | B | B3's conformance suite is the measure from here on |
| BL-7 | Better symbolic simplification | partly: `f''` gives `(4x² - 2)·exp(-x²)` and `N'` is tidy; the derivative of a constant loses its units (BC-B23) | C/D | C2: better symbolic simplification |
| BL-8 | DP45 dense output should use the method's 4th-order interpolant | no: `codegen_llvm.py:1077` uses Hermite plus DOPRI5's 4th-order term | closed | |
| BL-9 | Garbage collection or reference counting for lists | yes (documented in §19) | B | B2: memory never freed (and C1) |
| BL-10 | Matrices with units; lists of vectors | partly: `[[1, 2], [3, 4]] m` works; `[<1, 2> m, <3, 4> m]` gives "a list element must be a number, but it is a 2-vector" | C/D | C1 |
| BL-11 | Uncertainties: native code, REPL/Jupyter, weighted fits, uncertain vectors | yes: `∫ L dx` is refused cleanly; see NEW-1 for vectors | C/D | C7 |
| BL-12 | Browser playground | no: `web/` exists (D29) | closed | |
| BL-13 | Derivatives of multi-line functions | yes: "can only differentiate one-line functions like f(x) = ..." | C/D | C2 |
| BL-14 | Stiff solver; events in `solve` | stiff is done (D42) and `until` is done (D39); a jump that depends on the unknowns isn't located (§19) | C/D | C2: events on the unknowns |
| BL-15 | `solve` with a parameter sweep | not implemented (not tested) | C/D | C2 |
| BL-16 | `stdlib/` as the spec lays it out, or record the deviation | recorded in D103 (`fermium/stdlib/*.fm`) | design | An **empty, untracked** top-level `stdlib/` folder exists in the checkout; remove it under A9 hygiene |
| BL-17 | Known trap: `2 g h` means 2 grams times h | no longer silent: with your own g it is the error "'2 g' is ambiguous … write 2*g … or 2 [g]" | A1 | The trap disappears once A1's rule is taught; the BACKLOG text is stale |
| BL-18 | Known trap: lists are never freed | yes (same as BL-9) | B | B2 |

## gauntlet/FRICTION.md (the 9 rows not marked fixed, plus the leftovers of fixed rows #33, #48 and #94)

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| F-30 | List elements lose their significant figures | partly: `for E in [0.50 eV, 0.75 eV, 1 eV]` prints `0.5 eV 1 eV`, `0.75 eV 1.50 eV`, … | A4 | A4.3 (same as examples #15, red team round 4's °C list) |
| F-33 | Leftover G4: `solve … for x` returns a root at the left end | yes: `solve sin(t) = 0 for t from 0 to 4` gives `0` | C/D | C2: `from a exclusive` or "all roots" |
| F-37 | The root takes the unit of the range's start | yes: `141 cm` | design | The range's unit is the natural display unit |
| F-38 | `print` always puts a space between items | yes: `1 2` | design | |
| F-48 | A function can't return an ODE solution | yes: "a function can't return the ODE solution y yet; return a number made from it" | C/D | C1/C2: needs a solution type. Related to A8.2 (D48 pointer) |
| F-51 | No unit per matrix entry | yes: "all entries of a matrix need the same units" | C/D | C1 |
| F-56 | Unknowns can't be lists | yes; the message misleads: "xs holds a list of a plain number; it can't now hold a plain number" | C/D | C1: `solve` with a list of unknowns |
| F-59 | Leftover cosmetics | partly. S13: `[[1.2, -0.75, 0], …, [0, 0, 1.0]]` (a γ of 1.25 from b = 0.6 prints 1.2, and an exact 1 prints 1.0). `2 rad/s + 3 1/s` gives `5 rad/s` silently. "Unit-free error texts" has no concrete repro | A4 | A4.3 for S13. Mixing rad/s and 1/s is by design (D6: angles are plain numbers) |
| F-73 | Fraction coefficients `73/24 e²`, `π²/12 t²` | yes: a warning, then `8.4` (= 73/(24 e²)) | A2 | |
| F-74 | Unit-after-number with standard symbols (Ω, K, b, T, l, m) | partly: `2 Ω t` is an error; `0.25 T` with a period T only warns and prints `0.25 T` (tesla) | A1 | A1's table requires an error |
| F-94 | No `global` declaration | by design: `clear(xs)`, plus a warning | design | Documented (D216) |
| F-96a | An if-expression can't hold `in MeV to 3 digits` | yes: "expected 'else' … but found 'in'" | design | Formatting belongs to `print`; print inside the branches |
| F-96b | No text labels on plot points (level schemes) | yes (not implemented) | C/D | D6/D8 (plotting polish) |

## dev-notes/REDTEAM.md (not fully fixed findings, plus "not reported/by design" notes)

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| RT1-1 | A narrow peak far from the start of an infinite range is missed | **yes, silently**: `∫ exp(-((x - 1)/1e-6)^2) dx from 0 to ∞` gives `8.86×10⁻⁷` (true 1.77×10⁻⁶, half), no warning | B | B2: narrow peaks. Worse than §19 says (half, not missed) |
| RT1-7 | Differences over-claim figures (D95 "not changed") | yes: `1.00 m - 0.999 m` gives `0.00100 m` | B | B2: sig figs of sums |
| RT2-n1 | FFT in `fermium build` differs from NumPy by 10⁻¹⁶ | not re-run | design | Rounding level |
| RT2-n2 | A 1 fm decay integrated over 1 m gives 0 | yes (same as BL-2) | B | B2 |
| RT3-n1 | `floor`/`round` of an uncertain value drop σ | not re-run | design | The derivative is 0 almost everywhere |
| RT3-n2 | `1i^k` in a loop shows rounding noise | not re-run | design | Honest noise (D230) |
| RT4-16 | Exact ties round half to even | documented (D11); `print 0.125` gives `0.125` (a literal prints as written) | design | Same as printf, NumPy and Julia |
| RT4-n1 | `x = 1/2 m` is 0.500 1/m | yes: `0.500 1/m`; `1/2 kg` gives `0.500 1/kg` | A2 | Spec: `1/2 kg` = 0.5 kg |
| RT4-n2 | `[20 °C, 30 °C] in K` prints `[293, 303] K` | yes | A4 | A4.3 |
| RT4-n3 | `np.sin(x [deg])` passes degrees to a function that takes radians | not re-run | design | The user's declaration is wrong |
| RT5-nit | `3𝑖 V`: a unit can't follow an imaginary literal | yes: "V isn't defined", with the hint `3i * 1 V or 3i [V]` | design | Won't fix; the hint gives the fix |
| RT6-n | README comment `# 3.52006 cm` vs the printed `3.52 cm` | no: README:77 now reads `3.52 cm (to 6 digits: 3.52006 cm)` | closed | |
| RT7-2 | Heat equation after a jump: very early times are 60 % wrong, silently | yes: `55.0 K`, `42.2 K` (erfc gives 34.3 K and 38.4 K) | B | B2: PDE after a jump (strict xfail) |
| RT7-5 | An integral at rounding level prints 3 figures, 1 of them right | yes: `7.93×10⁻⁹` (exact 8×10⁻⁹) | B | B2 (strict xfail) |
| RT7-n1 | ⟨H⟩ of the quadruple well: an error on a converged integral | not re-run (needs the red team's program) | B | B2: quadrature error estimate |
| RT7-n2 | `\|[3.0 m/s, 4.0 m/s]\|` is element-wise | yes: `[3.0, 4.0] m/s`; `\|<3, 4> m/s\|` gives `5 m/s` | design | `abs` of a list is element-wise; a vector uses `<…>` |
| RT7-n3 | `recombination.fm` warns "h (Planck's constant) is now your variable" on every run | yes: `h = 0.674` warns | design | D213. The research program could rename h (optional) |

## dev-notes/notes/bugs-review.md (R1–R7)

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| R1 | `d²x/dt²` not parsed | no: `print d²y/dt²` gives `-9.81 m/s²`; in `solve`, x(3 s) = `-0.990 m` (= cos 3) | closed | TROUBLESHOOTING:533 already says it works |
| R2 | `d/dt (dx/dt)` in `solve` is order 1 | no: `-0.990 m` | closed | |
| R3 | `dx/dt(0 s) = …` as an initial condition | no: `-0.990 m` | closed | |
| R4 | The hint always says `2*m`; the warning fires on a harmless `A = 0.1 m` | partly: the hint now uses the literal (`0.1*m`, `0 [m]`), but the warning still fires on `A = 0.1 m` and on `from 0 m to 20 cm` | A1 | Under A1 rule 2, `0.1 m` with a mass m is an error with both fixes. Audit the hint wording under A3.2 |
| R5 | Out-of-range solution error shows bare SI numbers | no: "asked for the solution at 30 s, outside the range … (it ends at 20 s)" | closed | |
| R6 | `from 1 to e`: the hint is about something else | no: hint "e is the elementary charge in Fermium; for Euler's number write exp(1)" | closed | |
| R7 | `vec(3, 4) m/s` fails | no: `<3, 4> m/s` | closed | |

## dev-notes/notes/bugs-bootcamp.md (items its status line lists as open)

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| BC-B9 | The "isn't defined" hint for typos and calls | partly: `pritn` and `furlongs` are fixed; `print foo(3)` hints "did you mean floor?"; `omegat` with `omega` defined hints `omegat = 1.0 m` | A3 | A3.2: audit "did you mean" hints (no `ω t` suggestion; `foo(…)` should suggest defining a function) |
| BC-B10 | Small oddities | mostly no: `3 m m` gives `3 m²`; `3.0.1` is an error; run-time errors carry lines; `(` is reported on line 1; `∫ x² dx = x³/3`; a reserved-word hint exists. Still: `sqrt(-1)` gives `NaN` and `1/0` gives `∞` silently | A3 | Same as BL-4 |
| BC-B12 | `1 AU / c` echoes the unit | no: `1 AU/c (= 499 s)` | closed | |
| BC-B13 | `where` bypasses the warning | no: `0.5 m … where m = 2 kg` is the "ambiguous" error | closed | |
| BC-B14 | A function result loses eV | no: `-13.6 eV -3.40 eV`; `10°` has no space | closed | |
| BC-B15 | Ctrl+C doesn't stop a loop | no: SIGINT prints `stopped by Ctrl+C` (driver.py SIGINT handler) | closed | TROUBLESHOOTING's Control+\ advice may be stale (A7) |
| BC-B16 | Text in a variable can't be printed | no: `hi` | closed | |
| BC-B17 | `%` for remainder | no: hint "for the remainder of a division use mod(n, 2)" | closed | |
| BC-B18 | `20 m/s / g` divides by grams, silently | partly: spaced `20 m/s / g` gives `2.04 s`; Wien's `m K / T` is fixed (501.6 nm); **`20 m/s/g` still gives `20 m/(s g)` silently** | A1 | A1 table row: an error either way, since spacing never matters |
| BC-B19 | `len` prints `5.00` | no: `5` | closed | |
| BC-B20 | `ys = xs` shares the list | yes: `[5, 2] m` | design | D26 (Python semantics). Slices copy (`xs[1:end]`); no `copy()` builtin |
| BC-B21 | Lists print with 1 figure | no: `[0, 8.8, 15, 19, 20] m`, `[4.5, 16, 25] J`, `1.00 AU` | closed | Still shows `1.00` for a written `1.000` (A4.3) |
| BC-B22 | Plot/fit polish | mostly no: loaded data plots as markers; `fit T^2 = k L` works; `τ` prints `20.0 min`. The long two-series axis label (`data.T [s], …`) remains | A6 | A6.4: axis labels |
| BC-B23 | The derivative of a constant loses its units | partly: `r(t) = 1 AU` gives `r'(t) = 0` with no units; the other printed derivatives are tidy now (`x'(t) = 9 m/s³ t²`, `∂f/∂x`) | A4 | Display |
| BC-B24 | `inf s` as a limit | no: `1 s` | closed | |
| BC-B25 | `to 2 m / M` divides the limit | no: `/ M` now divides the integral (`1 m`) | closed | |
| BC-B26 | Orbit plots from lists are stretched | no: equal aspect when both axes have one dimension (core.py:821) | closed | |
| BC-B27 | `with` indented less than the equations | no: `0.607` | closed | |
| BC-B28 | `N = 2 N` hint says "that's usually what you want" | partly: the hint now says "that's fine if you meant the unit", followed by the error. `2^20` prints `1048576` | A1 | Under A1 rule 2 it is one error |
| BC-B29 | Mixed-unit list prints in the first unit | yes, and worse: `0.0416666666666667 day`, `0.00694444444444444 day` (16 figures) | A4 | A4 display (sig figs of converted list elements) |

## dev-notes/notes/bugs-examples.md (#7, #12, #13, #14, #15) and bugs-tests.md (B17, B18)

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| EX-7 | No lists of text | no: `names = ["a", "b"]` then `print names` gives `[a, b]`; `names[2]` works | closed | A5's "lists of text" is already done: check docs and tests |
| EX-12 | `fit … with` can't go on the next line | no: works (`g = 9.78 m/s²`) | closed | A5 bullet already done: add a test if missing |
| EX-13 | `plot` draws lines only; no log axis | no: data as markers, `with points`, `log y` (D161, core.py:802) | closed | |
| EX-14 | Look-alike normalisation rewrites strings | no: `"Pound–Rebka"` prints with the en dash | closed | A4.4 already done: add a test if missing |
| EX-15 | List elements lose significant figures | no: `1.0×10⁷ kg/m³ 2.0×10⁷ kg/m³` | closed | The F-30 variant (mixed precisions) remains |
| EX-obs | The unit-after-number rule bit the examples 7 times | caught by errors now | A1 | Motivation for A1 |
| TS-B17 | `dx/dt` notation | no: `v = dx/dt` gives `v(1 s)` = `3 m/s` | closed | |
| TS-B18 | Malformed `d^2/dt^*` crashes | no: "the order of a derivative must be a whole number" | closed | |

## dev-notes/notes/bugs-adversarial.md (the three entries not marked [FIXED])

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| AD-A3 | A width-1 peak in a ±1e6 finite range gives 0 | yes (= BL-2) | B | B2 |
| AD-A46 | `1 rev/min in Hz` = 0.10472 Hz | yes, with a tailored warning (D27/D95); `60 rpm in rev/s` gives `1 rev/s` | design | D6/D27: rad = 1 |
| AD-A56 | Strong singularities away from 0 | yes (= BL-3) | B | B2 |

## docs/reference.md §19 "Known limitations"

| ID | Description | Reproduces now? | Class | Notes |
|---|---|---|---|---|
| L-1 | PDE at very early times after a jump | yes (= RT7-2) | B | B2 |
| L-2 | An integral at rounding level | yes (= RT7-5) | B | B2 |
| L-3 | A narrow peak in a huge range; a peak in the middle gives half | yes (= BL-1, BL-2, RT1-1) | B | B2. §19 should also say that the half-peak case happens on infinite ranges (RT1-1) |
| L-4 | Strong blow-ups away from 0 | yes (= BL-3) | B | B2 |
| L-5 | No garbage collection | yes (not stress-tested) | B | B2 |
| L-6 | Derivatives only of one-line functions | yes (= BL-13) | C/D | C2 |
| L-7 | An ODE jump on the unknowns isn't located | the result is right on a simple case (`x(2 s)` = `3.00 m`); accuracy not measured | C/D | C2: events on the unknowns |
| L-8 | No lists of vectors, matrices or complex numbers; `eigenvalues` needs a symmetric matrix | yes: "a list element must be a number, but it is a complex number" | C/D | C1. The symmetric-matrix restriction is by design (Jacobi) |
| L-9 | Uncertainties: interpreter only, no build/REPL/vectors/ODEs, unweighted fit | yes (plus NEW-1) | C/D | C7 |
| L-10 | Modules aren't cached; the REPL keeps an old module | not re-run | C/D | C6: cache compiled modules |
| L-11 | `fermium build` writes SVG, reads data relative to the current folder | not re-run | design | D31: no zlib/rasteriser in C; executables are moved around |

## DECISIONS.md: workarounds from before a feature existed (spec A5, last bullet)

| ID | Workaround | Status now | Class | Notes |
|---|---|---|---|---|
| D81 | FFT returns `fft_re`/`fft_im` because there were no complex numbers | yes: `fft(xs)` "isn't defined (did you mean ifft?)"; `fft_re` works | A5 | Return complex values; keep the old names as deprecated aliases for one version |
| D18 | Fit standard errors are "plain numbers until uncertainties exist" | partly superseded by D124: with a ± in the program, `fit` gives `9.7846 ± 0.0039 m/s²`; without one, plain numbers | C/D | C7. Always-uncertain fits would force the interpreter (D122) |
| D21 | `≈` is "equal within 10⁻⁶ relative" | yes: `1e-12 m/s ≈ 0 m/s` is `false` | A8 | A8.1 |
| D48 | ODE solution captured as a pointer stored in a double | yes (code unchanged) | A8 | A8.2 |
| D83 | "Complex without complex numbers": PDE `i` found by probing with i = 1, −1, 2 | still the implementation; ψ is a complex value since D184 | B | Port with a real complex type |
| D184 | A bare `i` is kept as the imaginary unit for legacy TDSE programs | yes | A1 | Listed in A1's superseded set; decide under the one rule |
| D26 | No GC "yet"; `push` never frees the old block | yes | B | B2 |
| D122 | Uncertain programs run only in the interpreter | yes | C/D | C7 |
| D82 | Eigenproblems: no ψ′ term ("a Sturm–Liouville transform later"), N ≤ 500, real only | not re-run | C/D | C2 |
| D42/D82/D83/D122 | `fermium build` refuses radau/bdf, eigenproblems, PDEs and ± because they call SciPy/Python | yes (per the decisions; not re-run) | B | B6: native replacements |
| D29 | The playground runs the reference interpreter because Pyodide can't run llvmlite | yes | B | B7/D6: a WebAssembly build |
| D30/D35/D52 | "Left for later": ∇ of an expression, a shared-node vector quadrature, non-integer Bessel orders and incomplete elliptic integrals | not implemented | C/D | C2 (∇, Bessel), C6 (quadrature) |
| D230 | Noise-zeroing reverted; "a smarter noise test may come later" | yes: honest noise | design | Hiding real values is worse |

## Spec items confirmed while triaging (not in the sources above; checked for the implementer)

A3.1: `ħ = c = 1` still hints "use == to compare". A3.2: in a program with no eigenvalue problem, `print g`
gives "g isn't defined: the eigenvalue problem's states are g₀". A3.3: `print h c in J/m` gives "can't show …
[N m²] in J/m", with a generic hint (and the `N m²` display is A4.1). A3.4: `fermium fmt` prints no note on
stderr. A3.5: typing `fermium run x.fm` at the REPL hints `fermium = 1.0 m`. A4.2: `fmt --pretty` leaves `(1/2)`
unchanged. A6.2: `plot … to "p.png"` prints the relative `plot saved to p.png`. Stale docs for A7:
docs/reference.md:142 (`2 g` "would mean 2 grams!"), bootcamp Lesson 1 "The big gotcha" (line 214),
CHEATSHEET:108 and TROUBLESHOOTING:254 (the spacing-dependent `/` rule, which A1 removes).

## Fix in A, grouped by A-item

- **A1 (one unit-name rule):** F-74 (`0.25 T` with a period T only warns), BC-B18 (`20 m/s/g` still silently per
  gram), R4 (warning on a harmless `A = 0.1 m`), BC-B28 (`N = 2 N` warning plus error), BL-17 (the `2 g h` trap
  text), EX-obs (the examples' 7 collisions), D184 (bare `i`). Also migrate the stale bootcamp and reference text
  listed above.
- **A2 (fraction coefficients):** F-73 (`73/24 e²` gives 8.4), RT4-n1 (`x = 1/2 m` gives 0.500 1/m and
  `1/2 kg` gives 0.500 1/kg).
- **A3 (error messages):** NEW-1 (vector × ± crash), BL-4 and BC-B10 (`sqrt(-1)` gives NaN, `factorial(-1)` and
  `1/0` give ∞, silently), BC-B9 (hints "did you mean floor?" for `foo(3)`; `omegat` gets `omegat = 1.0 m`), plus the
  spec's A3.1–A3.5 confirmed above. R1–R3 and R5–R7 already pass (A3.6); add regression tests.
- **A4 (display):** F-30 (`[0.50 eV, …]` prints 0.5 eV), F-59/S13 (matrix entries 1.25 → 1.2, exact 1 → 1.0),
  RT4-n2 (`[20 °C, 30 °C] in K` gives `[293, 303] K`), BC-B29 (mixed-unit list prints `0.0416666666666667 day`),
  BC-B23 (`r'(t) = 0` loses units), plus A4.1 (`h c` shown as N m²) and A4.2 (½ in `--pretty`). EX-14 (strings
  untouched) and EX-15 are already done: add tests.
- **A5 (API consistency):** D81 (complex `fft`, deprecated `fft_re`/`fft_im`). EX-12 (`fit … with` on the next
  line) and EX-7 (lists of text) already work: add tests and close them. Workarounds found in DECISIONS: D81, D18,
  D83, D184, D48, D21, D26, D122, D82, D29, D30/D35/D52 (their classes are in the table above).
- **A6 (install, plots):** BC-B22 (axis label `data.T [s], …`: A6.4), plus A6.2 (relative "plot saved to" path).
- **A7 (bootcamp):** the stale text listed in "Spec items confirmed" (Lesson 1 gotcha, CHEATSHEET:108,
  TROUBLESHOOTING:254, reference:142), and BC-B15's Control+\ advice (Ctrl+C works now).
- **A8 (review items):** D21 (`≈` near zero: `1e-12 m/s ≈ 0 m/s` is false), D48 (pointer as a double).
  A8.3 (research data citations) wasn't in these sources and wasn't checked here.
- **A9 (hygiene):** remove the empty untracked top-level `stdlib/` folder (BL-16).
