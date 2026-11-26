//! The game context: time, input, settings and control of the loop.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use vnce_core::Size;
use vnce_term::{Capabilities, GraphicsMode, TerminalInfo, WindowRequest, WindowState};

use crate::config::{self, ConfigError, FpsLimit, RunConfig};
use crate::input::InputState;
use crate::meter::RateMeter;

/// Everything a [`Game`](crate::Game) can read or change while it runs.
///
/// The methods that request a change ([`Context::quit`], [`Context::set_paused`],
/// [`Context::set_fps_limit`], [`Context::set_update_rate`],
/// [`Context::set_graphics_mode`] and the window requests such as
/// [`Context::set_fullscreen`]) take `&self`, so they can be called while the input from
/// [`Context::input`] is still in use:
///
/// ```
/// use vnce_runtime::{Context, Game};
/// use vnce_term::{Frame, KeyCode};
///
/// struct Demo;
///
/// impl Game for Demo {
///     fn update(&mut self, ctx: &mut Context) {
///         let input = ctx.input();
///         if input.was_pressed(KeyCode::Char('p')) {
///             ctx.set_paused(!ctx.is_paused());
///         }
///         if input.was_pressed(KeyCode::Char('q')) {
///             ctx.quit();
///         }
///     }
///
///     fn render(&mut self, _ctx: &Context, _frame: &mut Frame<'_>) {}
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Context {
    pub(crate) quit: Cell<bool>,
    pub(crate) paused: Cell<bool>,
    /// Set when the game was paused because the window lost focus.
    pub(crate) paused_by_focus: Cell<bool>,
    update_rate: Cell<u32>,
    update_interval: Cell<Duration>,
    fps: Cell<FpsLimit>,
    pub(crate) elapsed: Duration,
    pub(crate) alpha: f32,
    pub(crate) update_count: u64,
    pub(crate) frame_count: u64,
    pub(crate) fps_meter: RateMeter,
    pub(crate) ups_meter: RateMeter,
    pub(crate) input: InputState,
    pub(crate) capabilities: Capabilities,
    pub(crate) size: Size,
    pub(crate) terminal_info: TerminalInfo,
    pub(crate) graphics_mode: GraphicsMode,
    /// A graphics mode change requested by the game, applied before the next frame.
    pub(crate) graphics_request: Cell<Option<GraphicsMode>>,
    /// Requests to the native window, handed to the backend before the next frame.
    pub(crate) window_requests: RefCell<Vec<WindowRequest>>,
    /// The native window's state after the last frame (`None` in a terminal).
    pub(crate) window_state: Option<WindowState>,
}

impl Context {
    pub(crate) fn new(
        config: &RunConfig,
        capabilities: Capabilities,
        size: Size,
        terminal_info: TerminalInfo,
        graphics_mode: GraphicsMode,
    ) -> Self {
        Self {
            quit: Cell::new(false),
            paused: Cell::new(false),
            paused_by_focus: Cell::new(false),
            update_rate: Cell::new(config.update_rate),
            update_interval: Cell::new(config::interval(config.update_rate)),
            fps: Cell::new(config.fps),
            elapsed: Duration::ZERO,
            alpha: 0.0,
            update_count: 0,
            frame_count: 0,
            fps_meter: RateMeter::default(),
            ups_meter: RateMeter::default(),
            input: InputState::new(capabilities.key_release_events, config.key_hold_timeout),
            capabilities,
            size,
            terminal_info,
            graphics_mode,
            graphics_request: Cell::new(None),
            window_requests: RefCell::new(Vec::new()),
            window_state: None,
        }
    }

    /// Stops the loop after the current callback. [`run`](crate::run) then returns.
    pub fn quit(&self) {
        self.quit.set(true);
    }

    /// Returns `true` once [`Context::quit`] was called.
    #[must_use]
    pub fn is_quitting(&self) -> bool {
        self.quit.get()
    }

    /// Length of one update step.
    #[must_use]
    pub fn dt(&self) -> Duration {
        self.update_interval.get()
    }

    /// Length of one update step in seconds, for physics.
    #[must_use]
    pub fn dt_secs(&self) -> f32 {
        self.update_interval.get().as_secs_f32()
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
        self.fps.get()
    }

    /// Changes the frame limit, effective from the next frame.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidFps`] for an out-of-range fixed limit.
    pub fn set_fps_limit(&self, fps: FpsLimit) -> Result<(), ConfigError> {
        fps.validate()?;
        self.fps.set(fps);
        Ok(())
    }

    /// The number of updates per second.
    #[must_use]
    pub fn update_rate(&self) -> u32 {
        self.update_rate.get()
    }

    /// Changes the number of updates per second, effective from the next update.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidUpdateRate`] for an out-of-range rate.
    pub fn set_update_rate(&self, rate: u32) -> Result<(), ConfigError> {
        config::validate_update_rate(rate)?;
        self.update_rate.set(rate);
        self.update_interval.set(config::interval(rate));
        Ok(())
    }

    /// Returns `true` while updates are paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused.get()
    }

    /// Pauses or resumes updates. Events and rendering continue while paused.
    pub fn set_paused(&self, paused: bool) {
        self.paused.set(paused);
        self.paused_by_focus.set(false);
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

    /// What the terminal reported about itself (name, version, graphics support, cell
    /// size in pixels).
    #[must_use]
    pub fn terminal_info(&self) -> &TerminalInfo {
        &self.terminal_info
    }

    /// How images are shown by [`Frame::draw_image`](vnce_term::Frame::draw_image).
    #[must_use]
    pub fn graphics_mode(&self) -> GraphicsMode {
        self.graphics_requests_or_current()
    }

    /// Changes how images are shown, from the next frame on.
    pub fn set_graphics_mode(&self, mode: GraphicsMode) {
        self.graphics_request.set(Some(mode));
    }

    /// Returns `true` when the game runs in a native window (`vnce-window`) rather than a
    /// terminal; settings screens show the window's rows only then.
    #[must_use]
    pub fn is_windowed(&self) -> bool {
        self.terminal_info.native_window
    }

    /// The native window's state (fullscreen, cursor, vsync, scaling) including the
    /// requests made since the last frame, or `None` in a terminal.
    #[must_use]
    pub fn window_state(&self) -> Option<WindowState> {
        let state = self.window_state?;
        Some(
            self.window_requests
                .borrow()
                .iter()
                .fold(state, WindowState::with),
        )
    }

    /// Asks the native window to do something, from the next frame on. Terminals ignore
    /// it. The setters below are shortcuts for the usual requests.
    pub fn request_window(&self, request: WindowRequest) {
        self.window_requests.borrow_mut().push(request);
    }

    /// Switches the native window's borderless fullscreen on or off (the player can also
    /// press F11 or Alt+Enter).
    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.request_window(WindowRequest::Fullscreen(fullscreen));
    }

    /// Shows or hides the mouse cursor over the native window.
    pub fn set_cursor_visible(&self, visible: bool) {
        self.request_window(WindowRequest::CursorVisible(visible));
    }

    /// Changes the native window's title.
    pub fn set_title(&self, title: impl Into<String>) {
        self.request_window(WindowRequest::Title(title.into()));
    }

    /// Makes the native window wait for the screen's refresh (no tearing) or not (lower
    /// latency, frames as fast as the frame limit allows).
    pub fn set_vsync(&self, vsync: bool) {
        self.request_window(WindowRequest::Vsync(vsync));
    }

    /// Scales pictures in the native window smoothly (`true`) or with sharp pixels.
    pub fn set_smooth_scaling(&self, smooth: bool) {
        self.request_window(WindowRequest::SmoothScaling(smooth));
    }

    /// Rumbles the connected gamepads at `strength` (`0.0..=1.0`) for `duration`, where
    /// the pad and the system can.
    pub fn rumble(&self, strength: f32, duration: Duration) {
        self.request_window(WindowRequest::Rumble { strength, duration });
    }

    fn graphics_requests_or_current(&self) -> GraphicsMode {
        self.graphics_request.get().unwrap_or(self.graphics_mode)
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
        self.update_interval.get()
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
            TerminalInfo::default(),
            GraphicsMode::Protocol(vnce_term::GraphicsProtocol::Kitty),
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
    fn graphics_mode_requests() {
        let ctx = context();
        let sixel = GraphicsMode::Protocol(vnce_term::GraphicsProtocol::Sixel);
        ctx.set_graphics_mode(sixel);
        assert_eq!(ctx.graphics_mode(), sixel);
        assert_eq!(ctx.graphics_request.get(), Some(sixel));
    }

    #[test]
    fn window_requests_wait_for_the_loop() {
        let mut ctx = context();
        assert!(!ctx.is_windowed());
        assert_eq!(ctx.window_state(), None, "a terminal has no window");
        ctx.window_state = Some(WindowState {
            cursor_visible: true,
            ..WindowState::default()
        });
        ctx.set_fullscreen(true);
        ctx.set_cursor_visible(false);
        ctx.set_title("Game");
        ctx.rumble(0.5, Duration::from_millis(100));
        let state = ctx.window_state().unwrap();
        assert!(state.fullscreen && !state.cursor_visible);
        assert_eq!(ctx.window_requests.borrow().len(), 4);
    }

    #[test]
    fn quit_and_pause() {
        let ctx = context();
        assert!(!ctx.is_quitting());
        ctx.quit();
        assert!(ctx.is_quitting());
        ctx.set_paused(true);
        assert!(ctx.is_paused());
    }
}
