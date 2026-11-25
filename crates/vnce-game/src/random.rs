//! Random numbers for games: [`Rng`], a small, fast and seeded generator.
//!
//! The same seed always gives the same numbers, so levels can be generated again from a
//! seed, and bugs can be reproduced. It is not suitable for cryptography.

use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};

/// A small deterministic random generator (xorshift64*).
///
/// ```
/// use vnce_game::Rng;
///
/// let mut rng = Rng::new(42);
/// let roll = rng.range(1..7); // a die
/// assert!((1..7).contains(&roll));
///
/// // The same seed gives the same numbers.
/// let mut again = Rng::new(42);
/// assert_eq!(again.range(1..7), roll);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator started from `seed`. Every seed (also `0`) is valid.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        // Spread the seed bits; the state must never be zero.
        Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    /// A generator seeded from the system clock: different numbers on every run.
    #[must_use]
    pub fn from_time() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        #[expect(
            clippy::cast_possible_truncation,
            reason = "only the fast-changing low bits are needed"
        )]
        let seed = nanos as u64;
        Self::new(seed)
    }

    /// The current state, to save the generator and continue later with
    /// [`Rng::from_state`].
    #[must_use]
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// Continues a generator from a saved [`Rng::state`].
    #[must_use]
    pub const fn from_state(state: u64) -> Self {
        // A zero state would only give zeros.
        Self { state: state | 1 }
    }

    /// The next random 64-bit number.
    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// The next random 32-bit number.
    pub fn next_u32(&mut self) -> u32 {
        // The high bits are the best ones of xorshift64*.
        (self.next_u64() >> 32) as u32
    }

    /// A number in `0..n` (`0` when `n` is `0`).
    pub fn below(&mut self, n: u32) -> u32 {
        // Multiply and keep the high half: no division, and an even spread.
        let wide = u128::from(self.next_u64()) * u128::from(n);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the high half of a 64 x 32 bit product fits in 32 bits"
        )]
        let value = (wide >> 64) as u32;
        value
    }

    /// A number in `range` (`range.start` when the range is empty).
    pub fn range(&mut self, range: Range<i32>) -> i32 {
        if range.end <= range.start {
            return range.start;
        }
        let span = u32::try_from(i64::from(range.end) - i64::from(range.start)).unwrap_or(u32::MAX);
        let value = i64::from(range.start) + i64::from(self.below(span));
        i32::try_from(value).unwrap_or(range.start)
    }

    /// A float in `0.0..1.0`.
    pub fn unit(&mut self) -> f32 {
        // 24 bits: exactly the precision of an `f32`.
        #[expect(
            clippy::cast_precision_loss,
            reason = "the value has 24 bits, which an f32 holds exactly"
        )]
        let value = (self.next_u64() >> 40) as f32 / (1_u32 << 24) as f32;
        value
    }

    /// `true` with probability `p` (`0.0` = never, `1.0` = always).
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// A random element of `items` (`None` when it is empty).
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        let len = u32::try_from(items.len()).unwrap_or(u32::MAX);
        items.get(self.below(len) as usize)
    }

    /// Puts `items` in a random order (Fisher–Yates).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let bound = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let j = self.below(bound) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_numbers() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn zero_seed_works() {
        let mut rng = Rng::new(0);
        assert_ne!(rng.next_u64(), 0);
        let mut rng = Rng::from_state(0);
        assert_ne!(rng.next_u64(), 0);
    }

    #[test]
    fn state_round_trip() {
        let mut rng = Rng::new(3);
        rng.next_u64();
        let mut copy = Rng::from_state(rng.state());
        assert_eq!(rng.next_u64(), copy.next_u64());
    }

    #[test]
    fn below_stays_in_bounds_and_covers_them() {
        let mut rng = Rng::new(1);
        let mut seen = [false; 6];
        for _ in 0..1000 {
            let value = rng.below(6);
            assert!(value < 6);
            seen[value as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert_eq!(rng.below(0), 0);
        assert_eq!(rng.below(1), 0);
    }

    #[test]
    fn range_stays_in_bounds() {
        let mut rng = Rng::new(2);
        for _ in 0..1000 {
            assert!((-5..5).contains(&rng.range(-5..5)));
        }
        assert_eq!(rng.range(3..3), 3);
        // A reversed range is empty too.
        let (high, low) = (5, 1);
        assert_eq!(rng.range(high..low), 5);
        let full = rng.range(i32::MIN..i32::MAX);
        assert!(full < i32::MAX);
    }

    #[test]
    fn unit_and_chance() {
        let mut rng = Rng::new(4);
        for _ in 0..1000 {
            assert!((0.0..1.0).contains(&rng.unit()));
            assert!(!rng.chance(0.0));
            assert!(rng.chance(1.0));
        }
    }

    #[test]
    fn pick_and_shuffle() {
        let mut rng = Rng::new(5);
        assert_eq!(rng.pick::<u8>(&[]), None);
        assert_eq!(rng.pick(&[9]), Some(&9));

        let mut items: Vec<u32> = (0..50).collect();
        rng.shuffle(&mut items);
        assert_ne!(items, (0..50).collect::<Vec<_>>());
        items.sort_unstable();
        assert_eq!(items, (0..50).collect::<Vec<_>>());
    }
}
