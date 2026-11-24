//! The game context: time, input, settings and control of the loop.

use std::time::{Duration, Instant};

use vnce_core::Size;
use vnce_term::Capabilities;

use crate::config::{self, ConfigError, FpsLimit, RunConfig};
use crate::input::InputState;
use crate::meter::RateMeter;

/// Everything a [`Game`](crate::Game) can read or change while it runs.
#[derive(Debug, Clone)]
pub struct Context {
    pub(crate) quit: bool,
    pub(crate) paused: bool,
    /// Set when the game was paused because the window lost focus.
    pub(crate) paused_by_focus: bool,
    update_rate: u32,
    update_interval: Duration,
    fps: FpsLimit,
    pub(crate) elapsed: Duration,
    pub(crate) alpha: f32,
    pub(crate) update_count: u64,
    pub(crate) frame_count: u64,
    pub(crate) fps_meter: RateMeter,
    pub(crate) ups_meter: RateMeter,
    pub(crate) input: InputState,
    pub(crate) capabilities: Capabilities,
    pub(crate) size: Size,
}

impl Context {
    pub(crate) fn new(config: &RunConfig, capabilities: Capabilities, size: Size) -> Self {
        Self {
            quit: false,
            paused: false,
            paused_by_focus: false,
            update_rate: config.update_rate,
            update_interval: config::interval(config.update_rate),
            fps: config.fps,
            elapsed: Duration::ZERO,
            alpha: 0.0,
            update_count: 0,
            frame_count: 0,
            fps_meter: RateMeter::default(),
            ups_meter: RateMeter::default(),
            input: InputState::new(capabilities.key_release_events, config.key_hold_timeout),
            capabilities,
            size,
        }
    }

    /// Stops the loop after the current callback. [`run`](crate::run) then returns.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    /// Returns `true` once [`Context::quit`] was called.
    #[must_use]
    pub fn is_quitting(&self) -> bool {
        self.quit
    }

    /// Length of one update step.
    #[must_use]
    pub fn dt(&self) -> Duration {
        self.update_interval
    }

    /// Length of one update step in seconds, for physics.
    #[must_use]
    pub fn dt_secs(&self) -> f32 {
        self.update_interval.as_secs_f32()
    }

    /// Game time: the sum of all update steps. It does not advance while paused.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Interpolation factor in `0.0..1.0`: how far the current time is between the last
    /// update and the next one. Draw positions as
    /// `previous + (current - previous) * alpha` for smooth movement.
    #[must_use]
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Number of updates run so far.
    #[must_use]
    pub fn update_count(&self) -> u64 {
        self.update_count
    }

    /// Number of frames drawn so far.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Measured frames per second (over the last second).
    #[must_use]
    pub fn fps(&self) -> f64 {
        self.fps_meter.rate()
    }

    /// Measured updates per second (over the last second).
    #[must_use]
    pub fn ups(&self) -> f64 {
        self.ups_meter.rate()
    }

    /// The current frame limit.
    #[must_use]
    pub fn fps_limit(&self) -> FpsLimit {
        self.fps
    }

    /// Changes the frame limit, effective from the next frame.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidFps`] for an out-of-range fixed limit.
    pub fn set_fps_limit(&mut self, fps: FpsLimit) -> Result<(), ConfigError> {
        fps.validate()?;
        self.fps = fps;
        Ok(())
    }

    /// The number of updates per second.
    #[must_use]
    pub fn update_rate(&self) -> u32 {
        self.update_rate
    }

    /// Changes the number of updates per second, effective from the next update.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidUpdateRate`] for an out-of-range rate.
    pub fn set_update_rate(&mut self, rate: u32) -> Result<(), ConfigError> {
        config::validate_update_rate(rate)?;
        self.update_rate = rate;
        self.update_interval = config::interval(rate);
        Ok(())
    }

    /// Returns `true` while updates are paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pauses or resumes updates. Events and rendering continue while paused.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.paused_by_focus = false;
    }

    /// Keyboard and mouse state.
    #[must_use]
    pub fn input(&self) -> &InputState {
        &self.input
    }

    /// What the terminal supports.
    #[must_use]
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// The terminal size in cells.
    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    /// Changes the key hold timeout used when the terminal does not report key releases.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::ZeroKeyHoldTimeout`] for a zero timeout.
    pub fn set_key_hold_timeout(&mut self, timeout: Duration) -> Result<(), ConfigError> {
        if timeout.is_zero() {
            return Err(ConfigError::ZeroKeyHoldTimeout);
        }
        self.input.set_hold_timeout(timeout);
        Ok(())
    }

    pub(crate) fn update_interval(&self) -> Duration {
        self.update_interval
    }

    pub(crate) fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.capabilities = capabilities;
        self.input
            .set_key_release_events(capabilities.key_release_events);
    }

    pub(crate) fn record_frame(&mut self, now: Instant, frame_count: u64) {
        self.frame_count = frame_count;
        self.fps_meter.record(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context::new(
            &RunConfig::default(),
            Capabilities::default(),
            Size::new(80, 24),
        )
    }

    #[test]
    fn settings_are_validated() {
        let mut ctx = context();
        assert_eq!(ctx.set_fps_limit(FpsLimit::Fixed(144)), Ok(()));
        assert_eq!(ctx.fps_limit(), FpsLimit::Fixed(144));
        assert_eq!(
            ctx.set_fps_limit(FpsLimit::Fixed(0)),
            Err(ConfigError::InvalidFps(0))
        );
        assert_eq!(ctx.fps_limit(), FpsLimit::Fixed(144));

        assert_eq!(ctx.set_update_rate(120), Ok(()));
        assert_eq!(ctx.update_rate(), 120);
        assert_eq!(ctx.dt(), Duration::from_nanos(8_333_333));
        assert_eq!(
            ctx.set_update_rate(5000),
            Err(ConfigError::InvalidUpdateRate(5000))
        );
        assert_eq!(
            ctx.set_key_hold_timeout(Duration::ZERO),
            Err(ConfigError::ZeroKeyHoldTimeout)
        );
    }

    #[test]
    fn quit_and_pause() {
        let mut ctx = context();
        assert!(!ctx.is_quitting());
        ctx.quit();
        assert!(ctx.is_quitting());
        ctx.set_paused(true);
        assert!(ctx.is_paused());
    }
}
