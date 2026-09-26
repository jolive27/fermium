# fermium-runtime notes

## Draft for `rust/DIVERGENCES.md`: intentional differences from v1.5 in the numerics

Each item only changes a result v1 got wrong or refused; every result v1 got right is unchanged
(tested: `tests/quad.rs::quad_b2_keeps_v1_results_and_fixes_its_failures` requires bit-identical
values on every other fixture case). `quad_v1` keeps v1's exact behaviour for comparison.

### Quadrature: narrow peaks and half peaks (spec B2; OPEN_ITEMS RT1-1, BL-1, BL-2, L-3)

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

### Quadrature: strong singularities away from 0 (spec B2; OPEN_ITEMS BL-3, L-4)

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
