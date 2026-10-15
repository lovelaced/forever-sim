//! Deterministic random number streams.
//!
//! Every replication gets its own seed, derived from the master seed and the
//! replication index with SplitMix64. Inside a replication every actor gets
//! independent streams ("purposes"), so that changing one actor's talents does
//! not shift the random numbers another actor sees. That keeps common random
//! numbers (CRN) aligned between builds being compared, which is what makes
//! paired comparisons much tighter than independent runs.
//!
//! The generator is xoshiro256++ (Blackman & Vigna), implemented here so the
//! sequence never changes with a dependency upgrade.

#[inline]
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hash a list of keys into a seed (order-sensitive).
pub fn derive_seed(master: u64, keys: &[u64]) -> u64 {
    let mut s = master ^ 0xD1B5_4A32_D192_ED03;
    let mut out = splitmix64(&mut s);
    for &k in keys {
        s ^= k.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17);
        out ^= splitmix64(&mut s);
        s = out;
    }
    splitmix64(&mut s)
}

/// Stable 64-bit FNV-1a hash of a string, used to turn names into seed keys.
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut s = [0u64; 4];
        for v in s.iter_mut() {
            *v = splitmix64(&mut x);
        }
        if s == [0; 4] {
            s[0] = 1;
        }
        Rng { s }
    }

    pub fn from_keys(master: u64, keys: &[u64]) -> Self {
        Rng::new(derive_seed(master, keys))
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1) with 53 bits of precision.
    #[inline]
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    #[inline]
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        if hi <= lo {
            lo
        } else {
            lo + (hi - lo) * self.f64()
        }
    }

    /// Bernoulli trial with probability `p` (clamped to [0, 1]).
    #[inline]
    pub fn chance(&mut self, p: f64) -> bool {
        if p <= 0.0 {
            false
        } else if p >= 1.0 {
            true
        } else {
            self.f64() < p
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = Rng::from_keys(42, &[1, 2, 3]);
        let mut b = Rng::from_keys(42, &[1, 2, 3]);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn streams_differ() {
        let mut a = Rng::from_keys(42, &[1, 2]);
        let mut b = Rng::from_keys(42, &[2, 1]);
        let same = (0..100).filter(|_| a.next_u64() == b.next_u64()).count();
        assert_eq!(same, 0);
    }

    #[test]
    fn uniform_moments() {
        // mean 1/2, variance 1/12; 200k draws gives tight bounds
        let mut r = Rng::new(7);
        let n = 200_000;
        let (mut s, mut s2) = (0.0, 0.0);
        for _ in 0..n {
            let x = r.f64();
            assert!((0.0..1.0).contains(&x));
            s += x;
            s2 += x * x;
        }
        let mean = s / n as f64;
        let var = s2 / n as f64 - mean * mean;
        assert!((mean - 0.5).abs() < 0.005, "mean {mean}");
        assert!((var - 1.0 / 12.0).abs() < 0.002, "var {var}");
    }

    #[test]
    fn chance_rate() {
        let mut r = Rng::new(11);
        let n = 100_000;
        let hits = (0..n).filter(|_| r.chance(0.3)).count() as f64 / n as f64;
        // 4 sigma ~ 0.0058
        assert!((hits - 0.3).abs() < 0.006, "{hits}");
    }
}
