//! Deterministic fixed-point arithmetic (`CORE_SPEC.md` §3 determinism toolkit).
//!
//! Floating-point results can differ across platforms/compilers, which breaks
//! lockstep and replay. [`Fx`] is a **Q16.16** fixed-point number (16 integer
//! bits, 16 fractional) backed by `i32` with `i64` intermediates, pure integer
//! math, so every platform computes the exact same bits. Use it for positions,
//! velocities, and physics in deterministic (lockstep) worlds; convert to `f64`
//! only at the rendering edge, never inside the sim.

use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Number of fractional bits (Q16.16).
const FRAC: u32 = 16;
/// `1.0` in raw bits.
const ONE_BITS: i32 = 1 << FRAC;

/// Integer square root of a non-negative `i64` (Newton's method).
fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// A Q16.16 fixed-point number. `Copy`, totally ordered, and deterministic.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Fx(i32);

impl Fx {
    /// The value `0`.
    pub const ZERO: Fx = Fx(0);
    /// The value `1`.
    pub const ONE: Fx = Fx(ONE_BITS);

    /// From a whole number. Debug-asserts it fits the integer range (±32767).
    #[inline]
    pub fn from_int(i: i32) -> Fx {
        debug_assert!((-(1 << 15)..(1 << 15)).contains(&i), "Fx integer overflow: {i}");
        Fx(i << FRAC)
    }

    /// From a fraction `num/den` (exact integer construction, no float).
    #[inline]
    pub fn frac(num: i32, den: i32) -> Fx {
        debug_assert!(den != 0, "Fx::frac division by zero");
        Fx((((num as i64) << FRAC) / den as i64) as i32)
    }

    /// From raw Q16.16 bits.
    #[inline]
    pub fn from_bits(bits: i32) -> Fx {
        Fx(bits)
    }

    /// The raw Q16.16 bits.
    #[inline]
    pub fn to_bits(self) -> i32 {
        self.0
    }

    /// From an `f64`, for construction/tests only, never inside the sim.
    #[inline]
    pub fn from_num(f: f64) -> Fx {
        Fx((f * ONE_BITS as f64).round() as i32)
    }

    /// To `f64`, for rendering only, never feed back into the sim.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / ONE_BITS as f64
    }

    /// Truncate toward negative infinity to a whole number (floor).
    #[inline]
    pub fn floor_int(self) -> i32 {
        self.0 >> FRAC
    }

    /// Absolute value.
    #[inline]
    pub fn abs(self) -> Fx {
        Fx(self.0.abs())
    }

    /// `-1`, `0`, or `1` as an `Fx`.
    #[inline]
    pub fn signum(self) -> Fx {
        Fx::from_int(self.0.signum())
    }

    /// Square root (deterministic integer sqrt; non-negative inputs).
    #[inline]
    pub fn sqrt(self) -> Fx {
        // sqrt(v) in bits = isqrt(bits * ONE): real r = sqrt(self/ONE), r*ONE = sqrt(self*ONE).
        Fx(isqrt(self.0 as i64 * ONE_BITS as i64) as i32)
    }

    /// Clamp to `[lo, hi]`.
    #[inline]
    pub fn clamp(self, lo: Fx, hi: Fx) -> Fx {
        Fx(self.0.clamp(lo.0, hi.0))
    }
}

impl Add for Fx {
    type Output = Fx;
    #[inline]
    fn add(self, r: Fx) -> Fx {
        Fx(self.0 + r.0)
    }
}
impl Sub for Fx {
    type Output = Fx;
    #[inline]
    fn sub(self, r: Fx) -> Fx {
        Fx(self.0 - r.0)
    }
}
impl Neg for Fx {
    type Output = Fx;
    #[inline]
    fn neg(self) -> Fx {
        Fx(-self.0)
    }
}
impl Mul for Fx {
    type Output = Fx;
    /// Fixed-point multiply (`i64` intermediate, then rescale).
    #[inline]
    fn mul(self, r: Fx) -> Fx {
        Fx(((self.0 as i64 * r.0 as i64) >> FRAC) as i32)
    }
}
impl Div for Fx {
    type Output = Fx;
    /// Fixed-point divide.
    #[inline]
    fn div(self, r: Fx) -> Fx {
        debug_assert!(r.0 != 0, "Fx divide by zero");
        Fx((((self.0 as i64) << FRAC) / r.0 as i64) as i32)
    }
}
impl AddAssign for Fx {
    #[inline]
    fn add_assign(&mut self, r: Fx) {
        self.0 += r.0;
    }
}
impl SubAssign for Fx {
    #[inline]
    fn sub_assign(&mut self, r: Fx) {
        self.0 -= r.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn int_roundtrip() {
        for i in [-1000, -1, 0, 1, 42, 1000] {
            assert_eq!(Fx::from_int(i).floor_int(), i);
        }
    }

    #[test]
    fn add_sub_are_exact() {
        let a = Fx::from_int(7);
        let b = Fx::from_int(3);
        assert_eq!((a + b).floor_int(), 10);
        assert_eq!((a - b).floor_int(), 4);
        assert_eq!((-a).floor_int(), -7);
        // fractions add exactly
        assert_eq!(Fx::frac(1, 2) + Fx::frac(1, 2), Fx::ONE);
    }

    #[test]
    fn mul_div() {
        let two_half = Fx::from_num(2.5);
        let four = Fx::from_int(4);
        assert!(approx((two_half * four).to_f64(), 10.0, 1e-4));
        assert!(approx((Fx::from_int(10) / four).to_f64(), 2.5, 1e-4));
        // exact fraction multiply
        assert_eq!(Fx::frac(1, 2) * Fx::from_int(8), Fx::from_int(4));
    }

    #[test]
    fn sqrt_values() {
        assert_eq!(Fx::from_int(16).sqrt(), Fx::from_int(4)); // exact
        assert!(approx(Fx::from_int(2).sqrt().to_f64(), 2f64.sqrt(), 1e-3));
        assert_eq!(Fx::ZERO.sqrt(), Fx::ZERO);
    }

    #[test]
    fn deterministic_known_bits() {
        // Pure-integer ops -> identical bits everywhere. Pin a known result.
        let x = Fx::frac(3, 4); // 0.75 -> 49152 bits
        assert_eq!(x.to_bits(), 49152);
        let y = x * x; // 0.5625 -> 36864 bits
        assert_eq!(y.to_bits(), 36864);
    }

    #[test]
    fn ordering_and_clamp() {
        assert!(Fx::from_int(-2) < Fx::from_int(3));
        assert_eq!(
            Fx::from_int(10).clamp(Fx::ZERO, Fx::from_int(5)),
            Fx::from_int(5)
        );
        assert_eq!(Fx::from_int(2).abs(), Fx::from_int(2));
        assert_eq!(Fx::from_int(-2).abs(), Fx::from_int(2));
    }

    #[test]
    fn add_is_commutative_and_associative() {
        let (a, b, c) = (Fx::frac(1, 3), Fx::frac(1, 7), Fx::from_int(5));
        assert_eq!(a + b, b + a);
        assert_eq!((a + b) + c, a + (b + c)); // exact for add
    }
}
