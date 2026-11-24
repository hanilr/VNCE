//! Run configuration.

use std::fmt;
use std::time::Duration;

use vnce_term::TerminalOptions;

/// The highest supported update rate and frame limit, per second.
pub const MAX_RATE: u32 = 1000;

/// How often frames are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FpsLimit {
    /// At most this many frames per second (1 to [`MAX_RATE`]).
    Fixed(u32),
    /// As many frames as possible.
    Unlimited,
}

impl FpsLimit {
    /// Time between two frames, or `None` for [`FpsLimit::Unlimited`].
    #[must_use]
    pub fn frame_interval(self) -> Option<Duration> {
        match self {
            Self::Fixed(fps) => Some(interval(fps)),
            Self::Unlimited => None,
        }
    }

    /// Checks that a fixed limit is between 1 and [`MAX_RATE`].
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidFps`] for an out-of-range fixed limit.
    pub fn validate(self) -> Result<(), ConfigError> {
        match self {
            Self::Fixed(fps) if fps == 0 || fps > MAX_RATE => Err(ConfigError::InvalidFps(fps)),
            _ => Ok(()),
        }
    }
}

impl Default for FpsLimit {
    /// 60 frames per second.
    fn default() -> Self {
        Self::Fixed(60)
    }
}

impl fmt::Display for FpsLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(fps) => write!(f, "{fps}"),
            Self::Unlimited => f.write_str("unlimited"),
        }
    }
}

/// Time between two events at `rate` per second. `rate` must not be zero.
pub(crate) fn interval(rate: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(rate.max(1)))
}

/// Checks that an update rate is between 1 and [`MAX_RATE`].
pub(crate) fn validate_update_rate(rate: u32) -> Result<(), ConfigError> {
    if rate == 0 || rate > MAX_RATE {
        Err(ConfigError::InvalidUpdateRate(rate))
    } else {
        Ok(())
    }
}

/// Configuration of [`run`](crate::run).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunConfig {
    /// Updates per second (1 to [`MAX_RATE`]). Default: 60.
    pub update_rate: u32,
    /// Frame limit. Default: 60 FPS.
    pub fps: FpsLimit,
    /// Longest frame time taken into account. After a longer stall (for example when the
    /// process was suspended), the game skips time instead of running many updates at once.
    /// Default: 250 ms.
    pub max_frame_time: Duration,
    /// Quit when `Ctrl+C` is pressed. Default: `true`.
    pub quit_on_ctrl_c: bool,
    /// Pause updates while the terminal window is not focused. Default: `false`.
    pub pause_on_focus_lost: bool,
    /// On terminals that do not report key releases, a key counts as released when no
    /// press or repeat arrived for this long. It must be longer than the operating system's
    /// key repeat delay. Default: 500 ms.
    pub key_hold_timeout: Duration,
    /// Most events handled per frame once the frame is due, so that a flood of events (such
    /// as fast mouse movement) cannot delay drawing. Default: 1024.
    pub max_events_per_frame: usize,
    /// Terminal modes. By default focus events and keyboard enhancement are enabled.
    pub terminal: TerminalOptions,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            update_rate: 60,
            fps: FpsLimit::default(),
            max_frame_time: Duration::from_millis(250),
            quit_on_ctrl_c: true,
            pause_on_focus_lost: false,
            key_hold_timeout: Duration::from_millis(500),
            max_events_per_frame: 1024,
            terminal: TerminalOptions {
                focus_events: true,
                keyboard_enhancement: true,
                ..TerminalOptions::default()
            },
        }
    }
}

impl RunConfig {
    /// Checks every value.
    ///
    /// # Errors
    ///
    /// Returns the first invalid value found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_update_rate(self.update_rate)?;
        self.fps.validate()?;
        if self.max_frame_time.is_zero() {
            return Err(ConfigError::ZeroMaxFrameTime);
        }
        if self.key_hold_timeout.is_zero() {
            return Err(ConfigError::ZeroKeyHoldTimeout);
        }
        if self.max_events_per_frame == 0 {
            return Err(ConfigError::ZeroMaxEventsPerFrame);
        }
        Ok(())
    }
}

/// An invalid configuration value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The update rate is 0 or above [`MAX_RATE`].
    InvalidUpdateRate(u32),
    /// The fixed frame limit is 0 or above [`MAX_RATE`].
    InvalidFps(u32),
    /// [`RunConfig::max_frame_time`] is zero.
    ZeroMaxFrameTime,
    /// [`RunConfig::key_hold_timeout`] is zero.
    ZeroKeyHoldTimeout,
    /// [`RunConfig::max_events_per_frame`] is zero.
    ZeroMaxEventsPerFrame,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUpdateRate(rate) => {
                write!(f, "invalid update rate {rate} (expected 1 to {MAX_RATE})")
            }
            Self::InvalidFps(fps) => {
                write!(f, "invalid FPS limit {fps} (expected 1 to {MAX_RATE})")
            }
            Self::ZeroMaxFrameTime => f.write_str("max frame time must not be zero"),
            Self::ZeroKeyHoldTimeout => f.write_str("key hold timeout must not be zero"),
            Self::ZeroMaxEventsPerFrame => f.write_str("max events per frame must not be zero"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert_eq!(RunConfig::default().validate(), Ok(()));
    }

    #[test]
    fn invalid_values_are_rejected() {
        let config = |f: fn(&mut RunConfig)| {
            let mut config = RunConfig::default();
            f(&mut config);
            config.validate()
        };
        assert_eq!(
            config(|c| c.update_rate = 0),
            Err(ConfigError::InvalidUpdateRate(0))
        );
        assert_eq!(
            config(|c| c.update_rate = MAX_RATE + 1),
            Err(ConfigError::InvalidUpdateRate(MAX_RATE + 1))
        );
        assert_eq!(
            config(|c| c.fps = FpsLimit::Fixed(0)),
            Err(ConfigError::InvalidFps(0))
        );
        assert_eq!(config(|c| c.fps = FpsLimit::Unlimited), Ok(()));
        assert_eq!(
            config(|c| c.max_frame_time = Duration::ZERO),
            Err(ConfigError::ZeroMaxFrameTime)
        );
        assert_eq!(
            config(|c| c.key_hold_timeout = Duration::ZERO),
            Err(ConfigError::ZeroKeyHoldTimeout)
        );
        assert_eq!(
            config(|c| c.max_events_per_frame = 0),
            Err(ConfigError::ZeroMaxEventsPerFrame)
        );
    }

    #[test]
    fn intervals() {
        assert_eq!(
            FpsLimit::Fixed(50).frame_interval(),
            Some(Duration::from_millis(20))
        );
        assert_eq!(FpsLimit::Unlimited.frame_interval(), None);
        assert_eq!(interval(1000), Duration::from_millis(1));
        assert_eq!(FpsLimit::Fixed(144).to_string(), "144");
        assert_eq!(FpsLimit::Unlimited.to_string(), "unlimited");
    }
}
