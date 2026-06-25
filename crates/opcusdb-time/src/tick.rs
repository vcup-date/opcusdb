//! The [`Tick`] — discrete simulation time.
//!
//! See `CORE_SPEC.md` §9. All sim time is measured in ticks; there is no
//! wall-clock in sim code (determinism contract §2). A `Tick` is a monotonic
//! counter; durations are plain `u64` tick counts.

use core::ops::Add;

/// A point in simulation time, counted in ticks from the start of the world.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Tick(pub u64);

impl Tick {
    /// The very first tick.
    pub const ZERO: Tick = Tick(0);

    /// The next tick (`self + 1`).
    #[inline]
    pub fn next(self) -> Tick {
        Tick(self.0 + 1)
    }

    /// The raw tick count.
    #[inline]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl Add<u64> for Tick {
    type Output = Tick;
    #[inline]
    fn add(self, rhs: u64) -> Tick {
        Tick(self.0 + rhs)
    }
}
