//! Seeded random numbers (D80): v1's `fermium/rng.py` (the same stream as its JIT, `fermium build`
//! and interpreter): xoshiro256** seeded through splitmix64.
//!
//! ```text
//! let mut r = Rng::default();     // as if seed(0) had been called
//! r.seed(42.0);                   // seed(s): s truncated to a whole number (|s| >= 9.2e18 or NaN: 0)
//! r.rand();                       // uniform in [0, 1): top 53 bits × 2⁻⁵³
//! r.rand_range(a, b);             // a + (b − a)·rand()
//! r.randn();                      // standard normal: sqrt(−2 ln(1 − u₁)) cos(2π u₂)
//! r.randn_ms(mu, sigma);          // mu + sigma·randn()  (sigma < 0: v1's error kind 26)
//! ```

use super::Fail;

const GOLDEN: u64 = 0x9E3779B97F4A7C15;
const SM1: u64 = 0xBF58476D1CE4E5B9;
const SM2: u64 = 0x94D049BB133111EB;
const TWO_M53: f64 = 1.0 / 9007199254740992.0;
const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
/// v1's ERR_NEG_SIGMA: "randn(μ, σ): σ is a standard deviation, so it can't be negative"
pub const ERR_NEG_SIGMA: i64 = 26;

/// The generator state: four 64-bit words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    pub s: [u64; 4],
}

impl Default for Rng {
    /// A program that never calls seed starts as if it had called seed(0).
    fn default() -> Self {
        Rng { s: state_for(0.0) }
    }
}

/// The whole number a seed value stands for (fptosi in v1's compiled code).
pub fn seed_int(s: f64) -> u64 {
    if !(s == s) || s.abs() >= 9.2e18 {
        return 0;
    }
    (s as i64) as u64
}

/// The state seed(s) sets, by splitmix64.
pub fn state_for(s: f64) -> [u64; 4] {
    let mut x = seed_int(s);
    let mut out = [0u64; 4];
    for w in out.iter_mut() {
        x = x.wrapping_add(GOLDEN);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(SM1);
        z = (z ^ (z >> 27)).wrapping_mul(SM2);
        *w = z ^ (z >> 31);
    }
    out
}

impl Rng {
    pub fn new(seed: f64) -> Self {
        Rng { s: state_for(seed) }
    }

    pub fn seed(&mut self, s: f64) {
        self.s = state_for(s);
    }

    /// xoshiro256**
    pub fn next_u64(&mut self) -> u64 {
        let [mut s0, mut s1, mut s2, mut s3] = self.s;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s1 << 17;
        s2 ^= s0;
        s3 ^= s1;
        s1 ^= s2;
        s0 ^= s3;
        s2 ^= t;
        s3 = s3.rotate_left(45);
        self.s = [s0, s1, s2, s3];
        result
    }

    /// uniform in [0, 1)
    pub fn rand(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * TWO_M53
    }

    /// uniform in [a, b): a + (b − a)·rand()
    pub fn rand_range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.rand()
    }

    /// a standard normal (Box–Muller)
    pub fn randn(&mut self) -> f64 {
        let u1 = self.rand();
        let u2 = self.rand();
        (-2.0 * (1.0 - u1).ln()).sqrt() * (TWO_PI * u2).cos()
    }

    /// mu + sigma·randn(); Err(kind 26) for a negative sigma
    pub fn randn_ms(&mut self, mu: f64, sigma: f64) -> Result<f64, Fail> {
        if sigma < 0.0 {
            return Err(Fail::new(ERR_NEG_SIGMA, sigma, 0.0));
        }
        Ok(mu + sigma * self.randn())
    }
}
