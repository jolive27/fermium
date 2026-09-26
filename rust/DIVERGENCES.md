# Intentional differences between Fermium 2 (Rust) and Fermium 1.5 (the oracle)

Spec §B2 and §B6: the language is frozen, but documented v1 limitations are fixed natively and Python
libraries are replaced by native code. Each difference below says what v1 did, what v2 does, and which test
proves it. (The numerics drafts for the quadrature and special functions are in
`crates/fermium-runtime/NOTES.md`.)

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
