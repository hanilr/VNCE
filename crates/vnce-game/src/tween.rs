//! Tweens: a value that runs from a start to an end over a time, along an [`ease`] curve,
//! for camera moves, sliding menus, fading colors and growing numbers.
//!
//! [`ease`]: crate::ease

use std::time::Duration;

use vnce_raster::Rgba;

use crate::Timer;
use crate::ease;

/// Values that can be blended: `a.lerp(b, 0.0)` is `a`, `a.lerp(b, 1.0)` is `b`. Curves
/// that overshoot (`ease::out_back`, `ease::out_elastic`) give `t` a little outside
/// `0..=1`; numbers and vectors follow it, colors stop at their ends.
pub trait Lerp: Copy {
    /// The blend of `self` and `other` at `t`.
    #[must_use]
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        ease::lerp(self, other, t)
    }
}

impl<const N: usize> Lerp for [f32; N] {
    fn lerp(self, other: Self, t: f32) -> Self {
        std::array::from_fn(|i| ease::lerp(self[i], other[i], t))
    }
}

impl Lerp for (f32, f32) {
    fn lerp(self, other: Self, t: f32) -> Self {
        (
            ease::lerp(self.0, other.0, t),
            ease::lerp(self.1, other.1, t),
        )
    }
}

impl Lerp for Rgba {
    fn lerp(self, other: Self, t: f32) -> Self {
        Rgba::lerp(self, other, t)
    }
}

/// A value running from `from` to `to` in `duration`, along an easing curve (linear
/// unless [`Tween::with_ease`] picks another). Advance it with the time of each update and
/// read [`Tween::value`].
///
/// ```
/// use std::time::Duration;
/// use vnce_game::{Tween, ease};
///
/// let mut x = Tween::new(0.0, 100.0, Duration::from_millis(400)).with_ease(ease::out_cubic);
/// x.update(Duration::from_millis(200));
/// assert!(x.value() > 50.0); // fast at first
/// x.update(Duration::from_millis(300));
/// assert!(x.is_finished());
/// assert_eq!(x.value(), 100.0);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Tween<T: Lerp> {
    from: T,
    to: T,
    timer: Timer,
    curve: fn(f32) -> f32,
}

impl<T: Lerp> Tween<T> {
    /// A tween from `from` to `to` in `duration`, linear.
    #[must_use]
    pub fn new(from: T, to: T, duration: Duration) -> Self {
        Self {
            from,
            to,
            timer: Timer::once(duration),
            curve: ease::linear,
        }
    }

    /// The same tween along `curve` (one of the [`ease`] functions, or an own one that
    /// maps `0.0` to `0.0` and `1.0` to `1.0`).
    #[must_use]
    pub fn with_ease(mut self, curve: fn(f32) -> f32) -> Self {
        self.curve = curve;
        self
    }

    /// Advances by `dt` and returns the new value.
    pub fn update(&mut self, dt: Duration) -> T {
        self.timer.update(dt);
        self.value()
    }

    /// The value now.
    #[must_use]
    pub fn value(&self) -> T {
        self.from.lerp(self.to, (self.curve)(self.timer.progress()))
    }

    /// Progress in time from `0.0` to `1.0` (before the curve).
    #[must_use]
    pub fn progress(&self) -> f32 {
        self.timer.progress()
    }

    /// Returns `true` once the tween has reached its end.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.timer.is_finished()
    }

    /// Returns `true` if the tween reached its end during the last [`Tween::update`].
    #[must_use]
    pub fn just_finished(&self) -> bool {
        self.timer.just_finished()
    }

    /// The start value.
    #[must_use]
    pub fn from(&self) -> T {
        self.from
    }

    /// The end value.
    #[must_use]
    pub fn to(&self) -> T {
        self.to
    }

    /// Starts again from `from`.
    pub fn reset(&mut self) {
        self.timer.reset();
    }

    /// Runs on from the value now to a new end `to`, taking the whole duration again (a
    /// target that moves while the tween runs, without a jump).
    pub fn retarget(&mut self, to: T) {
        self.from = self.value();
        self.to = to;
        self.timer.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn runs_from_start_to_end() {
        let mut t = Tween::new(10.0_f32, 20.0, 100 * MS);
        assert!((t.value() - 10.0).abs() < 1e-6);
        assert!((t.update(25 * MS) - 12.5).abs() < 1e-4);
        assert!(!t.is_finished() && !t.just_finished());
        assert!((t.update(100 * MS) - 20.0).abs() < 1e-6);
        assert!(t.is_finished() && t.just_finished());
        assert!((t.progress() - 1.0).abs() < 1e-6);
        t.reset();
        assert!((t.value() - 10.0).abs() < 1e-6);
        assert!((t.from() - 10.0).abs() < 1e-6 && (t.to() - 20.0).abs() < 1e-6);
    }

    #[test]
    fn curves_shape_the_way_not_the_ends() {
        let mut t = Tween::new(0.0_f32, 1.0, 100 * MS).with_ease(ease::in_quad);
        assert!((t.update(50 * MS) - 0.25).abs() < 1e-4);
        // An overshooting curve passes the end on the way.
        let mut back = Tween::new(0.0_f32, 1.0, 100 * MS).with_ease(ease::out_back);
        assert!((1..100).any(|_| back.update(MS) > 1.0));
        // ... and stops on it.
        assert!((back.update(100 * MS) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn retarget_continues_without_a_jump() {
        let mut t = Tween::new(0.0_f32, 100.0, 100 * MS);
        t.update(50 * MS);
        t.retarget(0.0);
        assert!((t.value() - 50.0).abs() < 1e-3);
        assert!((t.update(100 * MS)).abs() < 1e-6);
    }

    #[test]
    fn vectors_pairs_and_colors() {
        let mut v = Tween::new([0.0, 0.0, 0.0], [2.0, 4.0, -6.0], 100 * MS);
        assert_eq!(v.update(50 * MS), [1.0, 2.0, -3.0]);
        let mut p = Tween::new((0.0, 10.0), (10.0, 0.0), 100 * MS);
        assert_eq!(p.update(50 * MS), (5.0, 5.0));
        let mut c = Tween::new(Rgba::rgb(0, 0, 0), Rgba::rgb(200, 100, 50), 100 * MS);
        assert_eq!(c.update(50 * MS), Rgba::rgb(100, 50, 25));
        // Zero time: already at the end.
        let z = Tween::new(1.0_f32, 2.0, Duration::ZERO);
        assert!((z.value() - 2.0).abs() < 1e-6);
    }
}
