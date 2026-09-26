# Divergences of the Rust implementation from Fermium 1.5

Fermium 1.5 (Python) is the oracle (spec §B2). This file lists each place where the Rust implementation
deliberately behaves differently, with the conformance case ids it affects. Temporary divergences are
marked as such and disappear when the feature is ported.

## `use python` stops with one clear error (temporary)

Spec §B5.14: Python interop loads libpython at run time, and only when a program imports it. It is the last
milestone. Until then the checker handles the placement rules that don't need Python (top level only; not
inside a Fermium module). Any other `use python …` line stops with:

    use python needs Python interop, which this build doesn't have yet
      hint: run it with the Python implementation (legacy/) for now

Affected cases (area python-interop; 22 of 24 fail on purpose): 06c39fca6004 092b2cb0af63 3ff6452a9896
41c01f70ba40 43e3bfbe2b59 51e5813de4ee 5e2299e27fdd 5f1addc03ebf 72ac3f6e78b3 7437a9e2cca4 76e31071c42f
82370562a3c8 8b383857434d 9fc3b3db6503 afeacad54612 c11247eac4c7 d7c13acaf581 d8a5135d4e94 dcbf3421f1b3
e54c838bd4d0 ee9e8f8e830b f50e686a0691.

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

## Special functions and cbrt: the C library, as v1's compiled code

v1's compiled code called the platform C library for erf, erfc, gamma, lgamma, besselj/bessely, asinh/acosh/atanh
and cbrt. The Rust implementation calls the same C library functions (glibc on Linux, libSystem on macOS), so the
results are bit-identical to v1 on the same platform; cbrt is a port of glibc's s_cbrt.c (Rust would otherwise
link compiler-builtins' cbrt), checked bit for bit on 200 000 random doubles. besseli, besselk, ellipk and ellipe
are ports of v1's own algorithms (bit-identical). Like v1, the last bits of the libm functions can differ between
platforms; the conformance goldens come from Linux.

## Implementation differences that are at the rounding level (not user-visible at printed precision)

- Radau/BDF: our LU and sums instead of LAPACK/BLAS; step sequences identical in 15 of 16 test solves.
- Fit: MINPACK lmder port with our QR; parameters agree with SciPy to 1e-9, standard errors to 2e-8.
- Eigenvalues (matrix method): Sturm bisection + inverse iteration instead of LAPACK stebz/stein;
  energies agree to 4e-12 relative.
- FFT: our mixed-radix/Bluestein instead of pocketfft; 6e-16 of the L2 norm.
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
