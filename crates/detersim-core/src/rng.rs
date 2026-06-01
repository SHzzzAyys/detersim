//! Deterministic randomness.
//!
//! The whole framework's determinism guarantee depends on every random draw
//! being reproducible. Never use `rand::thread_rng`, `rand::random`, or any
//! OS-entropy source in framework or SUT code — use this trait.

/// A deterministic, reproducible random source.
///
/// Implementations MUST be pure functions of their seed/state: the same seed
/// must always produce the same sequence, on the same platform.
pub trait Rng {
    /// Next 64 bits of output.
    fn next_u64(&mut self) -> u64;

    /// Returns `true` with probability `p` (clamped to `[0, 1]`).
    fn gen_bool(&mut self, p: f64) -> bool {
        let p = p.clamp(0.0, 1.0);
        // 53-bit mantissa -> uniform f64 in [0, 1).
        let x = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        x < p
    }

    /// Uniform-ish integer in the half-open range `[lo, hi)`.
    ///
    /// Uses modulo reduction (slightly biased for large ranges); good enough
    /// for a simulation. Swap for Lemire's method if unbiasedness matters.
    fn gen_range_u64(&mut self, lo: u64, hi: u64) -> u64 {
        assert!(lo < hi, "gen_range_u64 requires lo < hi (got {lo}..{hi})");
        lo + self.next_u64() % (hi - lo)
    }

    /// Derive a statistically independent, deterministic sub-stream.
    ///
    /// Used to give each node/task its own stream so that adding a task
    /// elsewhere does not perturb this stream — which keeps shrinking stable.
    fn fork(&mut self) -> Self
    where
        Self: Sized;
}

/// SplitMix64 — a tiny, fast, fully deterministic generator.
///
/// Not cryptographic. For DST that is fine; if you want higher statistical
/// quality later, swap in ChaCha8 (it implements the same `Rng` trait).
#[derive(Clone, Copy, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }
}

impl Rng for SplitMix64 {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn fork(&mut self) -> Self {
        SplitMix64::new(self.next_u64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = SplitMix64::new(123);
        let mut b = SplitMix64::new(123);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn fork_is_independent_but_deterministic() {
        let mut a = SplitMix64::new(7);
        let mut a2 = SplitMix64::new(7);
        let mut fa = a.fork();
        let mut fa2 = a2.fork();
        for _ in 0..100 {
            assert_eq!(fa.next_u64(), fa2.next_u64());
        }
        // The parent stream still advances deterministically after a fork.
        assert_eq!(a.next_u64(), a2.next_u64());
    }
}
