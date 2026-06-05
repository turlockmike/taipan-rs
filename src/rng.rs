//! Deterministic pseudo-random number generator.
//!
//! We roll our own xorshift64* generator instead of pulling in the `rand`
//! crate so the project stays dependency-free and, more importantly, so the
//! sequence is fully reproducible from a `--seed`. Same seed in, same game out.

/// A seedable xorshift64* PRNG.
///
/// Not cryptographically secure — it just needs to be fast, deterministic, and
/// well-distributed enough for game events. xorshift64* passes BigCrush-ish
/// quality for our purposes.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Create a generator from a seed. A seed of 0 would make xorshift collapse
    /// to all-zeroes forever, so we substitute a non-zero constant in that case.
    pub fn new(seed: u64) -> Self {
        let state = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Rng { state }
    }

    /// Current internal state — for serializing a game save. Pair with
    /// [`Rng::from_state`] to restore an RNG mid-sequence so a reloaded game
    /// produces the exact same future draws.
    pub fn state(&self) -> u64 {
        self.state
    }

    /// Reconstruct an RNG from a previously saved [`Rng::state`]. Unlike
    /// `new`, this does not apply the seed-zero substitution — the value is a
    /// live state, already guaranteed non-zero by construction.
    pub fn from_state(state: u64) -> Self {
        Rng { state }
    }

    /// Advance the state and return the next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Uniform integer in `[low, high]` inclusive on both ends.
    ///
    /// Panics if `low > high` — that's a programming error, not user input.
    pub fn range(&mut self, low: u32, high: u32) -> u32 {
        assert!(low <= high, "range: low ({low}) > high ({high})");
        let span = (high - low) as u64 + 1;
        low + (self.next_u64() % span) as u32
    }

    /// Return `true` with probability `numerator / denominator`.
    pub fn chance(&mut self, numerator: u32, denominator: u32) -> bool {
        assert!(denominator > 0, "chance: denominator must be > 0");
        self.range(1, denominator) <= numerator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_differ() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        // Vanishingly unlikely the first value collides for distinct seeds.
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn seed_zero_does_not_collapse() {
        let mut r = Rng::new(0);
        let first = r.next_u64();
        let second = r.next_u64();
        assert_ne!(first, 0);
        assert_ne!(first, second);
    }

    #[test]
    fn range_is_inclusive_and_bounded() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let v = r.range(5, 10);
            assert!((5..=10).contains(&v), "value {v} out of [5,10]");
        }
    }

    #[test]
    fn range_single_value() {
        let mut r = Rng::new(99);
        assert_eq!(r.range(3, 3), 3);
    }

    #[test]
    fn range_covers_both_endpoints() {
        let mut r = Rng::new(123);
        let mut saw_low = false;
        let mut saw_high = false;
        for _ in 0..10_000 {
            match r.range(0, 1) {
                0 => saw_low = true,
                1 => saw_high = true,
                other => panic!("unexpected {other}"),
            }
        }
        assert!(saw_low && saw_high, "did not see both endpoints");
    }

    #[test]
    fn chance_zero_never_true() {
        let mut r = Rng::new(5);
        for _ in 0..1000 {
            assert!(!r.chance(0, 10));
        }
    }

    #[test]
    fn chance_full_always_true() {
        let mut r = Rng::new(5);
        for _ in 0..1000 {
            assert!(r.chance(10, 10));
        }
    }
}
