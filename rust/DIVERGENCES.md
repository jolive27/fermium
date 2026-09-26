# Divergences of the Rust implementation from Fermium 1.5

Fermium 1.5 (Python) is the oracle (spec §B2). This file lists each place where the Rust implementation
deliberately behaves differently, with the conformance case ids it affects. Temporary divergences are
marked as such and disappear when the feature is ported.

## The standard library is embedded in the binary

In v1, `import mechanics` finds `fermium/stdlib/mechanics.fm` next to the Python package. The Rust binary
embeds the same Fermium sources when it is built (`fermium-check/build.rs`), so a single binary works with no
files installed. Only the search path differs: the stdlib is searched last as before, but it is shown as
"the standard library" and its files as `stdlib/<name>.fm`, as v1 shows them. Output and messages are the
same as v1's.

## fermium.toml is read by a small TOML reader

v1 reads `fermium.toml` with Python's `tomllib`. The Rust reader accepts what v1's fallback reader
(`modules.py` `_mini_toml`) accepts: `[tables]` and `key = "text"` or `key = ["list", "of", "text"]`, with
`#` comments. For a malformed file, the message is the fallback reader's
(`… isn't a valid fermium.toml (line N: …)`) instead of `tomllib`'s. No conformance case has a malformed
`fermium.toml`.

## Indefinite integrals: native rules instead of SymPy (spec §B6)

- **v1** sent `∫ f dx` without limits to SymPy (`calculus.integrate_symbolic`), checked the answer at random
  points (`_antiderivative_ok`) and printed SymPy's formula.
- **v2** finds the antiderivative natively (`fermium-sym/src/integrate.rs`), with no Python:
  - linearity; constant factors; the power rule for x and for a linear base (`(a x + b)^p`, `1/(a x + b)` → ln);
    `c^u`;
  - a table for functions of a linear argument: sin, cos, tan, cot, sec, csc, exp, sinh, cosh, tanh, ln/log,
    log10, log2, sqrt, cbrt, asin, acos, atan, asinh, atanh, erf, sign, |u| (as `if u <= 0 then … else …`,
    SymPy's Piecewise); squares of the trigonometric and hyperbolic functions; `1/cos²`, `1/sin²`, …;
  - quadratics by completing the square: `1/Q`, `1/√Q`, `√Q` (atan, atanh, asinh, acosh, asin, and the
    `u √Q` forms), and `(m x + n) Q^p` for p = −1, ±½; `exp(−a x² + …)` → erf;
  - substitution `∫ F(u) u' dx` for u the argument of a function, a power's base, a root or a denominator;
  - integration by parts: a polynomial times exp, sin, cos, sinh or cosh of a linear argument (tabular), and
    times ln, atan, asin, acos, asinh, atanh;
  - `e^(ax) sin(bx)`, `e^(ax) cos(bx)`;
  - rational functions with numeric coefficients: polynomial division and partial fractions over the real
    linear (also repeated) and quadratic factors of the denominator;
  - positivity as in v1 (D37): built-in constants > 0, program variables only ever given positive literal
    values, and literal quantities with a positive number; a square root of `b²` is `b` for a positive b and
    `|b|` otherwise (v1's "evens" trick).
- Every formula is verified like v1 did: its derivative (by fermium-sym) is compared with the integrand at 12
  pseudo-random points; a formula that fails is refused ("Fermium's formula for this integral isn't right for
  every value of the constants in it …").
- **The printed formulas** match v1's for the programs in the conformance suite (`x³/3`, `x²/2`, `asinh(s/a)`,
  `-𝑖 exp(𝑖 x)`, `if x <= 0 then -x²/2 else x²/2`); for other integrands the formula may be written
  differently from SymPy's (an equivalent expression; the values agree).
- **Coverage vs v1:** SymPy's Risch-based integrator finds more antiderivatives (for example
  `∫ exp(sin(x)) cos(x)²…` style mixtures, products of several transcendental functions, rational functions
  with symbolic coefficients of degree > 2 in the denominator). For those v2 stops with
  "Fermium couldn't find a formula for this integral" and the hint "give limits (from a to b) to compute it
  numerically" (v1: "SymPy couldn't find a formula …" with the same hint). Integrals that need a special
  function Fermium doesn't have keep v1's message word for word, so programs and tests see the same error:
  "SymPy's formula for this integral uses the function Ei, which Fermium doesn't have yet: Ei(s)" (also Si,
  Ci, Shi, Chi, li, erfi).
- Tests: `fermium-sym/src/tests.rs` (`antiderivatives_print_like_v1`, `antiderivatives_are_right`,
  `non_elementary_integrals_are_refused`); conformance cases in `integrals/` (e.g. `cd3a0d2250de`,
  `ccf49cc58559`, `71a54bb44659`, `a5b1891791c9`).

## Display of ∇ results and of Leibniz-rule integrands: native tidying instead of SymPy's simplify

- **v1** printed the result of ∇f, ∇·F, ∇×F, ∇²f and the ∂/∂x integrand of a derivative under the integral sign
  (D36) through `sympy_tidy`: SymPy's `simplify`, kept only when shorter.
- **v2** (`fermium-sym/src/tidy.rs`) does the part of that which these formulas need: SymPy's automatic
  canonical form (equal bases combined, whole powers of products distributed, like terms collected), exact
  cancellation of a sum over a common denominator (∇²(1/r) = 0), SymPy's sign convention for sums
  (`signsimp`: `λ·(s - x)` rather than `-λ·(x - s)`), and SymPy's argument order (Basic.compare) when the
  formula is written back; like v1, the tidied formula is used only when it is shorter.
- The printed formulas match v1's for every ∇ and Leibniz program in the conformance suite (e.g.
  `-q x/(4π ε_0 (x² + y² + z²)^(3/2))`, `∫ λ·(s - x)/(4π ε_0 (y² + z² + (s - x)²)^(3/2)) ds`). SymPy's
  `simplify` also tries trigonometric identities, `cancel` and `together`; for formulas that need those, v2
  prints a different, equivalent formula, sometimes longer (∇² of the Yukawa potential `A exp(-a r)/r`: v1
  `A a² exp(-a r)/r`), sometimes shorter (∇·∇ of `G M / r` and ∇² of `x/r³`: v2 `0`, where v1 printed a long
  expression SymPy couldn't reduce; ∂/∂x of `x/r³`: v2 `(y² + z² - 2x²)/r⁵`, v1 `1/r³ - 3x²/r⁵`). Of 16
  textbook potentials (∇, ∇², ∇×, ∇· each), 11 print exactly as v1; about a third of random formulas print
  differently; derivatives (`f'`, `d/dx`,
  `∂/∂x`) don't go through this step and printed identically in 370 random formulas. Values are the same
  either way.
- Tests: `fermium-sym/src/tests.rs` (`tidy_like_sympy`); conformance cases `f6f6b826b670`, `71b03deee8c1`,
  `7c0c11b0d9a1`.

## Quadrature: narrow peaks and half peaks (spec B2; OPEN_ITEMS RT1-1, BL-1, BL-2, L-3)

- v1: `∫ exp(-((x - 1)/1e-6)^2) dx from 0 to ∞` gave 8.86×10⁻⁷ (half the true 1.77×10⁻⁶), no warning;
  `∫ exp(-(x-1000)^2*100) dx from 0 to 2000` gave 0.0886 (true 0.177); `∫ exp(-x²) dx from -1e6 to 1e6`
  gave 0 with the D110 warning.
- v2: all three give the true value (relative errors 8e-12, 1e-13, 5e-12). Method: panel-end sentinels, a second, independent
  sample of the integrand at the panel ends (which Gauss–Kronrod never uses), checked once the
  adaptive loop has converged; a panel whose end value is over 10× its largest node value, and
  large enough to matter at the tolerance, gets its error raised and is refined. Sentinels stop
  on panels narrower than 10⁻¹² of the range, so a value at one isolated point (not a peak) still
  gives v1's 0 (with the D110 warning).
- Tests: `quad.rs` cases `rt1_1`, `bl1`, `bl2`; unit test `isolated_point_is_not_a_peak`.
- Still possible (as with any quadrature): a peak narrower than the node spacing that sits neither
  at a panel end nor near a node is missed.

## Quadrature: strong singularities away from 0 (spec B2; OPEN_ITEMS BL-3, L-4)

- v1: `∫ abs(x - 0.3)^(-0.6) dx from 0 to 1` stopped with "the integrand is infinite at x = 0.300";
  `^(-0.8)` with "couldn't compute this integral".
- v2: 3.71210253732 (true 3.71210253737, rel 1.3e-11) and 8.58576502 (true 8.58576500346, rel 1.9e-9). Method: when v1's split
  at the middle of the stuck panel still fails, the point where |f| is largest nearby is located
  (golden-section search to rounding level) and each side is integrated from there; if a side
  still fails, because near c ≠ 0 the integrand is lost to rounding (x − c is quantised at ulp(c)),
  its value is the limit of ∫ from c ± d for d = d₀, d₀/2, d₀/4 (d₀ ≈ 10⁻⁹ of the scale, a power of
  two) by Aitken's Δ² (exact for a power law; QUADPACK's qags extrapolates similarly).
  Accuracy: ~1e-11 relative for α = −0.6, ~2e-9 for α = −0.8 (the closer α is to −1, the more the
  extrapolation amplifies rounding).
- Tests: `quad.rs` cases `bl3_06`, `bl3_08`.

## Integrals at the rounding level print only their meaningful figures (spec B2; OPEN_ITEMS L-2, RT7-5)

- v1: `print ∫ 1e6 sin(x) + 4e-9 dx from -1 to 1` printed `7.93×10⁻⁹` (exact 8×10⁻⁹): the sine part cancels,
  and summing it leaves a rounding error of about ε ∫|f| ≈ 2×10⁻¹⁰, so only the first figure is right. A
  symmetric zero printed rounding noise with three figures (`∫ sin(x) dx from -1 to 1` → `2.78×10⁻¹⁷`).
- v2: the quadrature returns ∫|f| with the value; when ε ∫|f| exceeds the relative tolerance's reach
  (10⁻¹⁰ |value|), the printed value keeps floor(log10(|value| / (ε ∫|f|))) significant figures, at least one:
  `8×10⁻⁹`, `3×10⁻¹⁷`. Every integral that converged to its tolerance prints exactly as in v1 (the limit never
  applies when ∫|f| is within 4.5×10⁵ of |value|, and the quadrature's own error estimate isn't used, since
  after the B2 extrapolations it is pessimistic: `∫ 1/√|x − 0.3| dx` still prints 12 correct digits). It
  applies to an integral printed directly, alone or scaled by a constant (`2 ∫ …`, a unit conversion), including
  with `to N digits`; a value stored in a variable first prints as before.
- Tests: `fermium-codegen/src/eval_calc.rs` (`rounding_level_integrals_keep_only_their_meaningful_figures`);
  conformance cases 173e2d6f8f97 3605dc185659 67451d15fe04 9bef8c6f60d5 a1b56d5b320c a504c61d2cbc
  ca02b7679e45 e7ffc7d1ff73 eaf1d936eb6c 02e450290630 (recorded divergences).

## Sums of measured values print by the decimal-place rule (spec B2; DECISIONS D95, red team 7 #4)

- v1 gave a sum or difference the significant figures of its most precise operand (D95: the textbook rule
  counts decimal places, which needs the magnitudes, known only at run time): `293.15 K + 0.5 K` printed
  `293.65 K`, `1.20 m + 2.0 m` printed `3.20 m`, `0.1 + 0.2` printed `0.30`.
- v2 applies the textbook rule when a printed sum's operands all carry measured precision (significant figures
  from written values; whole literals like `1` or `300` are exact, so `1 - r` keeps v1's rule): at run time each
  operand's last significant decimal place is found in the unit the result prints in, the sum keeps the
  coarsest one, and that sets its figures (at least one): `293.6 K` (293.65 is 293.6499… in binary),
  `940.5 MeV` for `938.272 MeV + 2.2 MeV`, `3.2 m`, `0.3`. `to N digits` always wins, and a sum stored in a
  variable first prints as before. The LLVM back end leaves such a print to the tree-walker, so both print the
  same.
- Measured on the whole conformance suite before adopting it: 4 cases change, no others: the three above, and
  a first-law check `W − (Q_h + Q_c)` whose rounding-noise result now shows one figure (`-7×10⁻¹⁵ μJ`, v1
  `-6.78×10⁻¹⁵ μJ`).
  An earlier version without the exact-literal and `to N digits` exceptions changed 28 cases, many for the
  worse (`1 - r^(1-γ) = 0.56`), and was not adopted.
- Tests: `fermium-codegen/src/eval_calc.rs` (`sums_keep_the_coarsest_decimal_place`); conformance cases
  89747a9eb10a 664786f7f30f 09fe8fafb686 47680448a9da (recorded divergences).

## Special functions and cbrt: the C library, as v1's compiled code

v1's compiled code called the platform C library for erf, erfc, gamma, lgamma, besselj/bessely, asinh/acosh/atanh
and cbrt. The Rust implementation calls the same C library functions (glibc on Linux, libSystem on macOS), so the
results are bit-identical to v1 on the same platform; cbrt is a port of glibc's s_cbrt.c (Rust would otherwise
link compiler-builtins' cbrt), checked bit for bit on 200 000 random doubles. besseli, besselk, ellipk and ellipe
are ports of v1's own algorithms (bit-identical). Like v1, the last bits of the libm functions can differ between
platforms; the conformance goldens come from Linux.

## The browser playground (fermium-wasm): playground only, not `fermium run`

The playground (`web/`, spec B5.12) runs the same parser, checker and tree-walking back end, compiled to
`wasm32-unknown-unknown` (`crates/fermium-wasm`). Every example of the page prints exactly what `fermium run
--backend interp` prints (`web/test/compare_native.js`, run by CI: 142 of 142 at 90a44c4). On the conformance
suite (`conformance/run --impl rust --bin web/test/fermium-wasm`) the module scored 2679/3037 against 2713 for
the native binary of the same commit (0f4840d, tree-walker); all 34 differences are floating-point: 23 in the
last digits of numbers printed to 12–17 digits, 11 in values at rounding level (a 10⁻¹⁷ or 10⁻²² that should be 0,
or a quantity found by cancellation). What differs, in the browser only:

- **Math functions:** there is no C library in the browser. The functions v1 took from the C library (erf, erfc,
  gamma, lgamma, besselj/bessely, asinh/acosh/atanh) already fall back to fermium-runtime's pure-Rust ports on
  non-Unix targets (`eval_core.rs`, `cmath`), and Rust's `sin`, `exp`, `powf`, … come from the pure-Rust libm
  (musl's algorithms) instead of glibc. So a number printed to 16–17 digits can differ in its last digits from
  Linux; at the default 3 significant figures, or `to 12 digits`, nothing changes in practice.
- **Recursion depth:** no threads, so no 512 MB program thread: programs run on the browser's call stack
  (about 450 levels of a simple recursive function in Chromium's worker, about 800 in Node). Deeper recursion
  stops with "this program recurses or nests too deeply for the browser playground" (the v1 Pyodide page said the
  same). The evaluator's own recursion check (`STACK_LIMIT`) is set for the module's 32 MB shadow stack.
- **Files:** `load` reads from an in-memory file system holding the bootcamp's and examples' data files; plots are
  written there and handed to the page. `import` of your own `.fm` files isn't possible (the standard library
  works: it is embedded).
- **`clock()`** is `performance.now()`.
- Errors and warnings are shown without the file name (`line 5: …`), as v1's playground showed them.

## Implementation differences that are at the rounding level (not user-visible at printed precision)

- Radau/BDF: LAPACK's LU and OpenBLAS's products modelled with their rounding (`numerics/npblas.rs`), so
  Radau's steps are v1's step for step. BDF's step-size factors use `error_norms ** (-1/k)`, which
  NumPy evaluates with its vectorised pow (not libm's; it differs in the last bit about 5% of the time),
  so a long BDF solve can drift from v1 at the rounding level; it shows only when the results are printed to
  16–17 digits (case 1954d43c8916).
- Fit: MINPACK lmder port with our QR; parameters agree with SciPy to 1e-9, standard errors to 2e-8.
- Eigenvalues (matrix method): LAPACK dstebz/dstein, dgbtf2/dgbtrs and the BLAS calls are transcribed,
  so energies and eigenfunctions are v1's to the last bit, except after the near-degenerate-pair fix,
  which uses a 2×2 eigenproblem instead of NumPy's SVD (last-digit differences in ψ). Orthogonality integrals
  such as ∫ψ₁ψ₂ are rounding noise (about 10⁻¹⁷) in both implementations and differ there (cases 1fc80f4b748e,
  92190636e7d6, 02e450290630; recorded under the rounding-level integrals section, which prints them with one
  figure).
- FFT: a literal port of pocketfft (numpy.fft's library): bit-identical to NumPy on every fixture value.
- PDE: a tridiagonal LU instead of SuperLU; 3e-13.

## Plots: native SVG/PNG/GIF instead of matplotlib (spec B6)

- v1 drew `plot` with matplotlib (PNG by default) and `plot … animate` with matplotlib + pillow; `fermium build`
  had its own SVG plotter (aot_data.c). v2 draws everything natively (`src/plot/`): SVG, PNG (own rasterizer
  and deflate encoder) and animated GIF (own LZW), with text in DejaVu Sans (the font matplotlib uses; outlines
  embedded from `rust/tools/font_subset.py`, Bitstream Vera licence reproduced in `font_data.rs`).
- Same semantics as v1: file names, the messages "plot saved to <absolute path>", "animation saved to … (N
  frames)" and "animation saved as N PNG frames in …_frames/", axis labels with units in brackets (`axis_label`,
  `y_axis_label` with D253's formula rule), markers for data, error bars and ±1σ bands (D124), log axes,
  xlim/ylim (D161), reversed axes, equal aspect for orbits, the 6-time PDE plot and the time label of animation
  frames.
- Not pixel-identical to matplotlib: the layout is v1's native plotter's (770×495, matplotlib's colours and
  5 % margins, 1-2-2.5-5 ticks, the legend in the corner with the fewest data points, like loc="best" restricted
  to corners); log axes have no minor ticks; `plot … animate` without a .gif path writes PNG frames (v1 did so
  only without pillow).
- A GIF uses one 256-colour palette (the most frequent colours; antialiasing blends map to the nearest).
- Tests: `tests/plot.rs` (files, messages, PNG/GIF structure; decoded by PIL once by hand), unit tests for
  deflate (round trip), LZW (round trip), labels and number format.

## Lists are freed (spec B2; OPEN_ITEMS BL-9, BL-18)

v1's compiled code never freed lists (a documented trap: a long loop that builds lists grows without bound).
In the Rust implementation a list is a reference-counted value (`Rc<RefCell<Vec<f64>>>` in the tree-walker),
freed when the last variable holding it goes away; list aliasing semantics (D26: `ys = xs` shares the list) are
unchanged, so no program prints anything different.

## parallel for: the first failing iteration's error is reported

When a run-time error happens in several iterations of a `parallel for`, v1's compiled code reported the error
of whichever thread stopped last, so the message depended on thread timing (e.g. "index 12 is out of range"
for a loop where iterations 11 and 12 both fail). The Rust implementation reports the error of the first failing
iteration in block order, the same on every run and machine. Case: fe2e353a26d0.

## PDEs right after a jump: the grid check (spec B2; OPEN_ITEMS RT7-2, L-1)

- v1: the heat equation after a jump (`D = 1e-4 m²/s`, `u(x, 0 s) = 0 K`, `u(0 m, t) = 80 K`, `u(1 m, t) = 0 K`,
  grid 400) prints `u(0.5 mm, 0.002 s)` = 55.0 K and `u(1 mm, 0.01 s)` = 42.2 K where 80 K erfc(x / (2√(D t)))
  gives 34.3 K and 38.4 K, with no warning. v1 controls the time step (D130) but not the grid: before the
  diffusion length √(D t) spans a few grid cells, the value between the boundary and the first nodes is an
  interpolation.
- v2: the values are the same, but a first-order PDE whose Dirichlet boundary value differs from its initial value
  at t0 (the D206 jump) is solved a second time on a grid half as fine, with the same time steps. `u(x, t)` compares
  the two there, and where they differ by more than 3·10⁻³ of the solution's range (a fine-grid error of 10⁻³ of the
  range for the second-order scheme, the tolerance of the time-step control), it warns once per solve:

      warning: line 5: the grid is too coarse for this PDE at x = 0.000500 m, t = 0.00200 s: the value there changes
      by 14% of the solution's range when the grid is made half as fine (a sharp front, a short wavelength, or the
      time right after a jump needs more grid points); raise  grid  (it is 400)

  The check costs about one more march on half the grid. PDEs without a jump (wave packets, smooth initial values)
  are not checked, and ∂u/∂x, ∂u/∂t are not checked.
- Tests: `pde.rs` `heat_step_very_early_time_is_accurate_or_warned` (the four points of red team round 7 #2 against
  erfc: each is within 2·10⁻³ of the range or flagged; t = 50 s is accurate and not flagged; no check without a jump).
- Affected cases (a warning v1 didn't give): 36c9c5b55398 (the red team's 55.004 K), 6781a0d7f2f0 (34.239 K where
  erfc gives 34.34 K: off by 1.3·10⁻³ of the range, over the tolerance).

## The command-line tool: doctor, --help, --time, build (fermium-cli)

The subcommands, argument errors, help texts, messages and exit codes are v1's (`fermium/cli.py`), compared
with `python3 -m fermium …` for `check` (ok, error, warnings, parse error, missing file, a folder, extra
arguments), `run`, `fmt`, every `-h`, a bad subcommand and a missing file argument. The differences:

- `fermium doctor` (spec §B7): Fermium 2 is one self-contained program, so doctor no longer checks Python,
  llvmlite, NumPy, SciPy, SymPy, matplotlib, pygls, ipykernel or a C compiler. It reports the version (and
  where the program is), the LLVM version built into it, the platform, that nothing else is needed, that the
  REPL, language server and Jupyter kernel are built in, that `fermium build` isn't there yet, and it still
  compiles and runs the test program (`g = 9.70 m/s²`). Same ✓/✗ layout and the same closing lines.
- `fermium run` also takes `--backend auto|llvm|interp` and `--base-dir DIR` (the conformance runner uses
  them); `--interp` is `--backend interp`. Its help and usage list them.
- `fermium run --time` prints `time: parse … ms, check … ms, run … ms (codegen, JIT and running)`: the back end
  is one number (v1 split it into codegen, LLVM+JIT and run).
- `fermium build` (milestone B5.10, not done yet) says so and exits with 1; its help says "not yet in this
  version" instead of "needs a C compiler".
- `fermium --version` prints `fermium 2.0.0-dev (Rust)`.

## The REPL (fermium-repl)

The prompt loop is repl.py's, line for line: the banner, `fm> ` and `... `, blocks (a blank line ends one at a
terminal; indentation, `else`/`elif` and unfinished input continue one from a pipe), `:help`, `:vars`,
`:quit`, the terminal-command hint, `\name` expansion on Enter, errors in the one-line form with a caret, and a
failed input leaving no names behind (D220: the checker is rolled back to a copy). 53 scripted sessions (every
session of tests/test_repl.py plus 30 more) print exactly what v1 prints
(`cargo test -p fermium-repl`, fixtures from rust/tools/repl_sessions.py). Differences:

- Line editing and history are a small editor of Fermium's own over the terminal (termios through the libc
  crate) instead of GNU readline/libedit: arrows, Home/End, Ctrl-A/E/K/U/W/L, Up/Down history saved in
  `~/.fermium_history` (v1's file; a libedit file from macOS is read too), at most 1000 entries kept.
  Tab completes `\name` (a unique match is replaced, a common prefix is extended, several matches are listed
  as `\varphi φ`); Tab on a blank line indents by four spaces (readline did nothing). Long lines that wrap
  past the terminal width are redrawn less neatly than readline does.
- Inputs run on the tree-walker (the LLVM back end doesn't compile arena variables yet), so an input's
  output is the same as `fermium run --backend interp` would print.
- `±` is refused by the checker as in `fermium run` (v1's REPL refused it with its own message).

## The language server (fermium-lsp)

`fermium lsp` is lsp.py's server without pygls: the same diagnostics (errors and warnings, UTF-16 ranges, the
hint on a second line), hover (variables with their units, functions with the units of their result, ODE
solutions, modules, constants, units, keywords; everything above an error still hovers), completion (`\name`
symbols, names in scope with their hover as detail, a module's members after `name.`, keywords) and the A1
quick fix (the edit of `fermium fmt --fix`). Four scripted sessions (the two of tests/test_lsp.py plus
warnings, astral characters, incremental edits, a parse error and modules) get exactly v1's replies
(`cargo test -p fermium-lsp`, fixtures from rust/tools/lsp_session.py). Differences:

- The initialize reply lists only the capabilities the server has (pygls lists more), with
  `textDocumentSync` incremental as pygls's default.
- After `shutdown` then `exit` the process exits with 0, as the protocol says (pygls exits with 1).
- A file:// URI with %-escapes (a folder name with spaces) is decoded before imports are looked up there.

## Function instances are keyed by the shape and units of vector and matrix arguments

v1 made one instance of a user function per argument type, but it keyed a vector or matrix argument by its
kind alone. So `f(v) = |v|` called first with `<3, 4> m` and then with `<1, 2, 2> s` reused the first instance:
v1's compiled code stopped with an internal error (`TypeError: Type of #1 arg mismatch: <2 x double> !=
<3 x double>`), and a straight port printed `3 m` for the second call. The Rust checker keys these arguments by
their length (or rows and columns) and by the dimension of each component, so each call gets its own instance
(`5 m 3 s`). No conformance case is affected (v1 crashed on every such program).

## The Jupyter kernel (fermium-jupyter)

v1's kernel ran on ipykernel (Python, pyzmq, libzmq). The Rust kernel has its own ZMTP 3.1 (TCP, NULL
mechanism, ROUTER/PUB/REP) and HMAC-SHA256, so it needs nothing installed beside `fermium`. It behaves as
kernel.py: one REPL session for all cells, stdout streamed, warnings on stderr, errors in the one-line form
on stderr with an error reply, "plot saved to …" lines replaced by the inline picture (PNG, or SVG), Tab
completion of `\name`, names, keywords and module members, and is_complete for unfinished blocks.
Checked end to end with jupyter_client (rust/tools/jupyter_e2e.py: 14 checks) and with nbclient on
examples/notebook.ipynb (every cell prints what v1's kernel prints, except the `plot` cell, which the Rust
checker doesn't compile yet); `cargo test -p fermium-jupyter` plays a client over TCP. Differences:

- kernel.json starts `fermium jupyter kernel -f {connection_file}` and says `"interrupt_mode": "message"`;
  an interrupt is acknowledged but doesn't stop a running cell yet (the kernel ignores SIGINT rather than
  die of it). v1's ipykernel could interrupt a cell.
- `fermium jupyter install --sys-prefix` installs into the active conda or virtual environment (or
  python3's prefix); `--prefix DIR` installs into DIR/share/jupyter (v1's install(prefix=…)).
- Run-time warnings (they go to the process's stderr) are shown after the cell's printed output; v1 showed
  them in the order they happened.
- stdin (`input`) and comms are not used by Fermium; history, inspect and comm_info get empty replies.

## Python interop (use python, and calling Fermium from Python)

`use python` behaves as in v1 (conformance area python-interop: 24 of 24). Differences in how it is set up, and
in the API for calling Fermium from Python (D142), which no conformance case covers:

- The binary loads libpython with dlopen the first time a program says `use python` (spec §B5.14), from the
  `python3` found on the PATH, then /usr/local/bin/python3 and /usr/bin/python3, or from `FERMIUM_PYTHON` /
  `FERMIUM_LIBPYTHON`. v1 *was* Python, so it used its own interpreter. A Python built without its shared
  library can't be used; the error at the `use python` line says so.
- Calling Fermium from Python is the module `fermium2` (rust/crates/fermium-pyapi/python), a ctypes wrapper
  of the library libfermium_pyapi, instead of `fermium.compile` / `fermium.load` in the `fermium` package.
  It has v1's API (compile, load, Module, Quantity, Q, QuantityArray, ComplexQuantity; the same errors and
  warnings); python/test_fermium2.py ports v1's tests of it.
- It runs programs on the tree-walker, not LLVM: a loop-heavy function is roughly as fast as pure Python
  (Leibniz series, 2·10⁶ terms: 1.07 s vs Python's 0.78 s on the shared test machine) where v1's JIT was ~10×
  faster. The LLVM back end doesn't keep top-level variables between inputs (REPL-style) yet.
