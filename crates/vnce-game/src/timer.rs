//! Time helpers: [`Timer`] for cooldowns and repeating events, [`ease`] curves for
//! animating values.

use std::time::Duration;

/// Counts game time: a one-shot delay (cooldowns, invincibility) or a repeating event
/// (spawning enemies). Advance it with the time of each update.
///
/// ```
/// use std::time::Duration;
/// use vnce_game::Timer;
///
/// let mut spawn = Timer::repeating(Duration::from_millis(500));
/// assert_eq!(spawn.update(Duration::from_millis(1200)), 2); // fired twice
/// assert_eq!(spawn.remaining(), Duration::from_millis(300));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Timer {
    duration: Duration,
    elapsed: Duration,
    repeating: bool,
    just_finished: bool,
}

impl Timer {
    /// A timer that finishes once after `duration`.
    #[must_use]
    pub const fn once(duration: Duration) -> Self {
        Self {
            duration,
            elapsed: Duration::ZERO,
            repeating: false,
            just_finished: false,
        }
    }

    /// A timer that fires every `duration`.
    #[must_use]
    pub const fn repeating(duration: Duration) -> Self {
        Self {
            repeating: true,
            ..Self::once(duration)
        }
    }

    /// Advances by `dt`; returns how many times the timer finished during this step (at
    /// most 1 for a one-shot timer).
    pub fn update(&mut self, dt: Duration) -> u32 {
        let was_finished = self.is_finished();
        self.elapsed = self.elapsed.saturating_add(dt);
        let fired = if self.repeating {
            if self.duration.is_zero() {
                u32::from(!dt.is_zero())
            } else {
                let count = self.elapsed.as_nanos() / self.duration.as_nanos();
                let whole = self
                    .duration
                    .saturating_mul(u32::try_from(count).unwrap_or(u32::MAX));
                self.elapsed = self.elapsed.saturating_sub(whole);
                u32::try_from(count).unwrap_or(u32::MAX)
            }
        } else {
            u32::from(!was_finished && self.is_finished())
        };
        self.just_finished = fired > 0;
        fired
    }

    /// Returns `true` if the timer finished during the last [`Timer::update`].
    #[must_use]
    pub fn just_finished(&self) -> bool {
        self.just_finished
    }

    /// Returns `true` once a one-shot timer has run out (never for repeating timers).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !self.repeating && self.elapsed >= self.duration
    }

    /// Time left until the timer finishes (next time, for repeating timers).
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.duration.saturating_sub(self.elapsed)
    }

    /// Progress from `0.0` (just started) to `1.0` (finished).
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        (self.elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0)
    }

    /// Starts again from zero.
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.just_finished = false;
    }

    /// The duration of one run.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

/// Easing curves: they map a progress `t` from `0.0` to `1.0` onto a value from `0.0` to
/// `1.0` (some overshoot slightly), for movement that starts or stops smoothly. Inputs
/// outside `0..=1` are clamped.
///
/// ```
/// use vnce_game::ease;
///
/// let x = ease::lerp(10.0, 50.0, ease::out_cubic(0.5));
/// assert!(x > 30.0); // fast at first, slow at the end
/// ```
pub mod ease {
    /// Linear interpolation from `a` to `b`.
    #[must_use]
    pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    fn clamp(t: f32) -> f32 {
        if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }
    }

    /// No easing.
    #[must_use]
    pub fn linear(t: f32) -> f32 {
        clamp(t)
    }

    /// Starts slowly.
    #[must_use]
    pub fn in_quad(t: f32) -> f32 {
        let t = clamp(t);
        t * t
    }

    /// Ends slowly.
    #[must_use]
    pub fn out_quad(t: f32) -> f32 {
        let t = clamp(t);
        t * (2.0 - t)
    }

    /// Starts and ends slowly.
    #[must_use]
    pub fn in_out_quad(t: f32) -> f32 {
        let t = clamp(t);
        if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        }
    }

    /// Starts slowly (stronger than quadratic).
    #[must_use]
    pub fn in_cubic(t: f32) -> f32 {
        clamp(t).powi(3)
    }

    /// Ends slowly (stronger than quadratic).
    #[must_use]
    pub fn out_cubic(t: f32) -> f32 {
        1.0 - (1.0 - clamp(t)).powi(3)
    }

    /// Starts and ends slowly (stronger than quadratic).
    #[must_use]
    pub fn in_out_cubic(t: f32) -> f32 {
        let t = clamp(t);
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    /// Overshoots a little past `1.0`, then settles (a springy stop).
    #[must_use]
    pub fn out_back(t: f32) -> f32 {
        const C1: f32 = 1.701_58;
        const C3: f32 = C1 + 1.0;
        let t = clamp(t) - 1.0;
        1.0 + C3 * t.powi(3) + C1 * t.powi(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn one_shot_timer() {
        let mut timer = Timer::once(100 * MS);
        assert_eq!(timer.update(60 * MS), 0);
        assert!(!timer.is_finished() && !timer.just_finished());
        assert!((timer.progress() - 0.6).abs() < 1e-6);
        assert_eq!(timer.update(60 * MS), 1);
        assert!(timer.is_finished() && timer.just_finished());
        // It fires only once.
        assert_eq!(timer.update(60 * MS), 0);
        assert!(!timer.just_finished());
        assert_eq!(timer.remaining(), Duration::ZERO);
        timer.reset();
        assert!(!timer.is_finished());
        assert_eq!(timer.duration(), 100 * MS);
    }

    #[test]
    fn repeating_timer_catches_up() {
        let mut timer = Timer::repeating(100 * MS);
        assert_eq!(timer.update(250 * MS), 2);
        assert_eq!(timer.update(40 * MS), 0);
        assert_eq!(timer.update(10 * MS), 1);
        assert!(timer.just_finished() && !timer.is_finished());
        let mut zero = Timer::repeating(Duration::ZERO);
        assert_eq!(zero.update(MS), 1);
        assert!((Timer::once(Duration::ZERO).progress() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn easing_curves_start_at_zero_and_end_at_one() {
        let curves: [fn(f32) -> f32; 8] = [
            ease::linear,
            ease::in_quad,
            ease::out_quad,
            ease::in_out_quad,
            ease::in_cubic,
            ease::out_cubic,
            ease::in_out_cubic,
            ease::out_back,
        ];
        for curve in curves {
            assert!(curve(0.0).abs() < 1e-5);
            assert!((curve(1.0) - 1.0).abs() < 1e-5);
            assert!((curve(2.0) - 1.0).abs() < 1e-5, "clamped");
            assert!(curve(f32::NAN).abs() < 1e-5);
        }
        assert!(ease::in_quad(0.5) < 0.5 && ease::out_quad(0.5) > 0.5);
        assert!((ease::in_out_cubic(0.5) - 0.5).abs() < 1e-5);
        assert!((0..=100_u8).any(|i| ease::out_back(f32::from(i) / 100.0) > 1.0));
        assert!((ease::lerp(2.0, 4.0, 0.25) - 2.5).abs() < f32::EPSILON);
    }
}
