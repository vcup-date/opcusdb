//! Deterministic pseudo-random number generation (`CORE_SPEC.md` §2).
//!
//! Sim code must never use ambient randomness; all randomness comes from a
//! seeded, explicitly-advanced generator whose seed lives in the event log, so
//! replay is exact. [`Rng`] is **PCG32** (PCG-XSH-RR 64/32) — small, fast, and
//! well-distributed — and is `Clone + Eq` so it snapshots and compares cleanly as
//! part of world state.

/// The PCG32 multiplier (the LCG constant).
const PCG_MULT: u64 = 6_364_136_223_846_793_005;
/// A fixed default stream increment (must be odd). Distinct seeds still diverge.
const DEFAULT_INC: u64 = 1_442_695_040_888_963_407;

/// A deterministic PCG32 generator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    /// Seed a generator on the default stream. Same seed ⇒ same sequence forever.
    pub fn seed(seed: u64) -> Self {
        Self::seed_with_stream(seed, 0)
    }

    /// Seed with an explicit stream selector, so independent streams can run from
    /// related seeds without correlating (e.g. per-shard or per-system streams).
    pub fn seed_with_stream(seed: u64, stream: u64) -> Self {
        // Standard PCG seeding: set the (odd) increment, then mix in the seed.
        let inc = (stream << 1) | 1 | DEFAULT_INC;
        let mut rng = Self { state: 0, inc };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// The next 32-bit value, advancing the state.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(PCG_MULT).wrapping_add(self.inc);
        // PCG-XSH-RR output permutation.
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// The next 64-bit value (two 32-bit draws, high word first).
    pub fn next_u64(&mut self) -> u64 {
        let hi = self.next_u32() as u64;
        let lo = self.next_u32() as u64;
        (hi << 32) | lo
    }

    /// A uniform value in `0..bound` with no modulo bias (Lemire's method).
    /// Returns 0 if `bound` is 0.
    pub fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        let mut x = self.next_u32();
        let mut m = (x as u64) * (bound as u64);
        let mut low = m as u32;
        if low < bound {
            // Reject the few values that would bias the result.
            let threshold = bound.wrapping_neg() % bound;
            while low < threshold {
                x = self.next_u32();
                m = (x as u64) * (bound as u64);
                low = m as u32;
            }
        }
        (m >> 32) as u32
    }

    /// A uniform value in `lo..hi`. Returns `lo` if the range is empty.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + self.below(hi - lo)
    }

    /// A boolean true with probability `num/den` (e.g. `chance(1, 10)` ≈ 10%).
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        if den == 0 {
            return false;
        }
        self.below(den) < num
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::seed(42);
        let mut b = Rng::seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        assert_eq!(a, b, "states stay in lockstep");
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::seed(1);
        let mut b = Rng::seed(2);
        let sa: Vec<u32> = (0..8).map(|_| a.next_u32()).collect();
        let sb: Vec<u32> = (0..8).map(|_| b.next_u32()).collect();
        assert_ne!(sa, sb);
    }

    #[test]
    fn clone_continues_identically() {
        let mut a = Rng::seed(7);
        for _ in 0..10 {
            a.next_u32();
        }
        let mut b = a.clone();
        assert_eq!(a.next_u64(), b.next_u64());
        assert_eq!(a.below(1000), b.below(1000));
    }

    #[test]
    fn below_is_in_range() {
        let mut r = Rng::seed(99);
        for _ in 0..10_000 {
            assert!(r.below(7) < 7);
        }
        assert_eq!(r.below(0), 0, "zero bound is safe");
        assert_eq!(r.below(1), 0, "bound 1 is always 0");
    }

    #[test]
    fn range_respects_bounds() {
        let mut r = Rng::seed(123);
        for _ in 0..10_000 {
            let v = r.range(10, 20);
            assert!((10..20).contains(&v));
        }
        assert_eq!(r.range(5, 5), 5, "empty range returns lo");
        assert_eq!(r.range(9, 3), 9, "inverted range returns lo");
    }

    #[test]
    fn below_is_roughly_uniform() {
        // Rough chi-square-free sanity: counts over 6 buckets stay near the mean.
        let mut r = Rng::seed(2024);
        let mut counts = [0u32; 6];
        let n = 60_000;
        for _ in 0..n {
            counts[r.below(6) as usize] += 1;
        }
        let mean = n / 6;
        for c in counts {
            let dev = (c as i64 - mean as i64).unsigned_abs();
            assert!(dev < mean as u64 / 10, "bucket {c} too far from mean {mean}");
        }
    }

    #[test]
    fn chance_probability_is_about_right() {
        let mut r = Rng::seed(555);
        let n = 100_000;
        let hits = (0..n).filter(|_| r.chance(1, 4)).count();
        let expected = n / 4;
        let dev = (hits as i64 - expected as i64).unsigned_abs();
        assert!(dev < expected as u64 / 10, "~25% expected, got {hits}/{n}");
    }
}
