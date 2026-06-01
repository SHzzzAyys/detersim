//! Logical simulation time.

use std::time::Duration;

/// Logical time, measured in nanoseconds since the start of a simulation run.
///
/// Within a single run `SimTime` is monotonic. It only advances when the
/// scheduler decides to (by popping the next event), never via a wall clock.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SimTime(pub u64);

impl SimTime {
    pub const ZERO: SimTime = SimTime(0);

    #[inline]
    pub fn from_nanos(n: u64) -> Self {
        SimTime(n)
    }

    #[inline]
    pub fn as_nanos(self) -> u64 {
        self.0
    }

    /// Add a [`Duration`], saturating at `u64::MAX` nanoseconds.
    #[inline]
    pub fn saturating_add(self, dur: Duration) -> Self {
        let add = dur.as_nanos().min(u64::MAX as u128) as u64;
        SimTime(self.0.saturating_add(add))
    }
}
