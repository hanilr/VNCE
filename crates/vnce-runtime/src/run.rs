//! The game loop.

use std::fmt;
use std::io;
use std::time::{Duration, Instant};

use vnce_term::{Backend, Event, KeyCode, KeyModifiers, Terminal};

use crate::Game;
use crate::config::{ConfigError, RunConfig};
use crate::context::Context;
use crate::driver::{Driver, TerminalDriver};

/// An error that stops [`run`].
#[derive(Debug)]
pub enum RunError {
    /// The configuration is invalid.
    Config(ConfigError),
    /// Terminal input or output failed.
    Io(io::Error),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "invalid run configuration: {error}"),
            Self::Io(error) => write!(f, "terminal I/O error: {error}"),
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<ConfigError> for RunError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<io::Error> for RunError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Takes over the terminal and runs `game` until it calls [`Context::quit`] (or `Ctrl+C` is
/// pressed, see [`RunConfig::quit_on_ctrl_c`]). The terminal is restored afterwards, also
/// on errors and panics.
///
/// # Errors
///
/// Returns an error if the configuration is invalid or terminal I/O fails.
pub fn run<G: Game>(game: &mut G, config: RunConfig) -> Result<(), RunError> {
    config.validate()?;
    let terminal = Terminal::enter_with(config.terminal)?;
    run_with(game, config, terminal, TerminalDriver)
}

/// Runs `game` on the given terminal and driver. The terminal is used as it is (it is not
/// entered). With a [`TestBackend`](vnce_term::TestBackend) and a
/// [`ScriptedDriver`](crate::ScriptedDriver) this runs a game deterministically in tests.
///
/// # Errors
///
/// Returns an error if the configuration is invalid or terminal I/O fails.
pub fn run_with<G, B, D>(
    game: &mut G,
    config: RunConfig,
    mut terminal: Terminal<B>,
    mut driver: D,
) -> Result<(), RunError>
where
    G: Game,
    B: Backend,
    D: Driver,
{
    config.validate()?;
    let mut ctx = Context::new(&config, terminal.capabilities(), terminal.size());
    ctx.set_capabilities(terminal.capabilities());
    game.init(&mut ctx);

    let mut previous = driver.now();
    let mut next_frame = previous;
    let mut accumulator = Duration::ZERO;

    while !ctx.quit {
        // 1. Wait until the next frame is due, handling input as it arrives.
        handle_events(game, &mut ctx, &config, &mut driver, next_frame)?;
        if ctx.quit {
            break;
        }

        // 2. Run as many fixed updates as the elapsed time requires.
        let now = driver.now();
        let frame_time = now
            .saturating_duration_since(previous)
            .min(config.max_frame_time);
        previous = now;
        if ctx.paused {
            accumulator = Duration::ZERO;
        } else {
            accumulator += frame_time;
            while accumulator >= ctx.update_interval() {
                let step = ctx.update_interval();
                accumulator -= step;
                ctx.input.begin_update(now);
                game.update(&mut ctx);
                ctx.input.end_update();
                ctx.update_count += 1;
                ctx.elapsed += step;
                ctx.ups_meter.record(now);
                if ctx.quit {
                    break;
                }
            }
        }
        if ctx.quit {
            break;
        }
        ctx.alpha =
            (accumulator.as_secs_f32() / ctx.update_interval().as_secs_f32()).clamp(0.0, 1.0);

        // 3. Draw.
        terminal.autoresize()?;
        ctx.size = terminal.size();
        terminal.draw(|frame| game.render(&ctx, frame))?;
        ctx.record_frame(now, terminal.frame_count());

        // 4. Schedule the next frame. When behind schedule, draw again as soon as possible
        // instead of rendering a burst of frames to catch up.
        next_frame = match ctx.fps_limit().frame_interval() {
            Some(interval) => (next_frame + interval).max(now),
            None => now,
        };
    }
    Ok(())
}

/// Handles input events until `deadline`. Once the deadline has passed, at most
/// `config.max_events_per_frame` pending events are handled so that drawing is never
/// delayed by a flood of events.
fn handle_events<G: Game, D: Driver>(
    game: &mut G,
    ctx: &mut Context,
    config: &RunConfig,
    driver: &mut D,
    deadline: Instant,
) -> io::Result<()> {
    let mut late_events = 0;
    loop {
        let timeout = deadline.saturating_duration_since(driver.now());
        let Some(event) = driver.poll_event(timeout)? else {
            return Ok(());
        };
        let now = driver.now();
        handle_event(game, ctx, config, &event, now);
        if ctx.quit {
            return Ok(());
        }
        if now >= deadline {
            late_events += 1;
            if late_events >= config.max_events_per_frame {
                return Ok(());
            }
        }
    }
}

fn handle_event<G: Game>(
    game: &mut G,
    ctx: &mut Context,
    config: &RunConfig,
    event: &Event,
    now: Instant,
) {
    if config.quit_on_ctrl_c && is_ctrl_c(event) {
        ctx.quit();
        return;
    }
    match event {
        Event::FocusLost if config.pause_on_focus_lost && !ctx.paused => {
            ctx.paused = true;
            ctx.paused_by_focus = true;
        }
        Event::FocusGained if ctx.paused_by_focus => {
            ctx.paused = false;
            ctx.paused_by_focus = false;
        }
        Event::Resize(size) => ctx.size = *size,
        _ => {}
    }
    ctx.input.handle_event(event, now);
    game.event(ctx, event);
}

fn is_ctrl_c(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.is_press()
                && matches!(key.code, KeyCode::Char('c' | 'C'))
                && key.modifiers.contains(KeyModifiers::CONTROL)
    )
}

#[cfg(test)]
mod tests {
    use vnce_core::Style;
    use vnce_term::input::{MouseEvent, MouseEventKind};
    use vnce_term::{Capabilities, Frame, KeyEvent, KeyEventKind, TestBackend};

    use super::*;
    use crate::config::FpsLimit;
    use crate::driver::ScriptedDriver;

    const MS: Duration = Duration::from_millis(1);

    /// A test game that records what it observes and quits after the number of 60 Hz
    /// updates that fit in `run_for`.
    #[derive(Default)]
    struct Probe {
        max_updates: u64,
        updates: u64,
        frames: u64,
        updates_since_frame: u64,
        max_updates_per_frame: u64,
        pressed: u64,
        released: u64,
        down: u64,
        events: u64,
        alphas: Vec<f32>,
    }

    impl Probe {
        fn new(run_for: Duration) -> Self {
            let millis = u64::try_from(run_for.as_millis()).expect("short test durations");
            Self {
                max_updates: millis * 60 / 1000,
                ..Self::default()
            }
        }
    }

    impl Game for Probe {
        fn event(&mut self, _ctx: &mut Context, _event: &Event) {
            self.events += 1;
        }

        fn update(&mut self, ctx: &mut Context) {
            self.updates += 1;
            self.updates_since_frame += 1;
            let a = KeyCode::Char('a');
            self.pressed += u64::from(ctx.input().was_pressed(a));
            self.released += u64::from(ctx.input().was_released(a));
            self.down += u64::from(ctx.input().is_down(a));
            if self.updates >= self.max_updates {
                ctx.quit();
            }
        }

        fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
            self.frames += 1;
            self.max_updates_per_frame = self.max_updates_per_frame.max(self.updates_since_frame);
            self.updates_since_frame = 0;
            self.alphas.push(ctx.alpha());
            let text = ctx.update_count().to_string();
            frame.buffer_mut().set_string(0, 0, &text, Style::new());
        }
    }

    fn terminal(key_release_events: bool) -> Terminal<TestBackend> {
        let capabilities = Capabilities {
            key_release_events,
            ..Capabilities::default()
        };
        Terminal::new(TestBackend::new(20, 5), capabilities).expect("test backend has a size")
    }

    fn key(code: KeyCode, kind: KeyEventKind, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
        })
    }

    fn run_probe(probe: &mut Probe, config: RunConfig, driver: ScriptedDriver, releases: bool) {
        run_with(probe, config, terminal(releases), driver).expect("the run succeeds");
    }

    #[test]
    fn updates_run_at_a_fixed_rate_independent_of_fps() {
        for fps in [15, 30, 60, 144] {
            let mut probe = Probe::new(Duration::from_secs(1));
            let config = RunConfig {
                fps: FpsLimit::Fixed(fps),
                ..RunConfig::default()
            };
            run_probe(&mut probe, config, ScriptedDriver::new(), true);
            assert_eq!(probe.updates, 60, "fps {fps}");
            // One second of frames, plus the first frame at time zero.
            let expected = u64::from(fps);
            assert!(
                probe.frames.abs_diff(expected) <= 2,
                "fps {fps}: {} frames",
                probe.frames
            );
        }
    }

    #[test]
    fn event_floods_do_not_add_frames() {
        let mut driver = ScriptedDriver::new();
        for i in 0..1000 {
            let event = Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                column: 1,
                row: 1,
                modifiers: KeyModifiers::NONE,
            });
            driver.push_event(MS * i, event);
        }
        let mut probe = Probe::new(Duration::from_secs(1));
        run_probe(&mut probe, RunConfig::default(), driver, true);
        assert_eq!(probe.events, 1000);
        assert!(probe.frames <= 61, "{} frames", probe.frames);
        assert_eq!(probe.updates, 60);
    }

    #[test]
    fn long_stalls_are_clamped() {
        let mut driver = ScriptedDriver::new();
        driver.push_stall(MS * 500, Duration::from_secs(5));
        let mut probe = Probe::new(Duration::from_secs(1));
        run_probe(&mut probe, RunConfig::default(), driver, true);
        // 250 ms / 16.67 ms = 15 updates at most, plus one from the leftover time.
        assert!(
            probe.max_updates_per_frame <= 16,
            "{}",
            probe.max_updates_per_frame
        );
        assert_eq!(probe.updates, 60);
    }

    #[test]
    fn alpha_interpolates_between_updates() {
        // 120 FPS with 60 UPS: every other frame sits halfway between two updates.
        let mut probe = Probe::new(Duration::from_millis(500));
        let config = RunConfig {
            fps: FpsLimit::Fixed(120),
            ..RunConfig::default()
        };
        run_probe(&mut probe, config, ScriptedDriver::new(), true);
        assert!(probe.alphas.iter().all(|a| (0.0..1.0).contains(a)));
        assert!(probe.alphas.iter().any(|a| (a - 0.5).abs() < 0.05));
    }

    #[test]
    fn unlimited_fps_draws_more_frames_than_updates() {
        let mut driver = ScriptedDriver::new();
        driver.set_min_wait(MS);
        let mut probe = Probe::new(Duration::from_millis(500));
        let config = RunConfig {
            fps: FpsLimit::Unlimited,
            ..RunConfig::default()
        };
        run_probe(&mut probe, config, driver, true);
        assert_eq!(probe.updates, 30);
        assert!(probe.frames > 400, "{} frames", probe.frames);
    }

    #[test]
    fn key_press_is_seen_once_with_release_events() {
        let mut driver = ScriptedDriver::new();
        let a = KeyCode::Char('a');
        driver.push_event(MS * 100, key(a, KeyEventKind::Press, KeyModifiers::NONE));
        driver.push_event(MS * 300, key(a, KeyEventKind::Release, KeyModifiers::NONE));
        let mut probe = Probe::new(Duration::from_secs(1));
        run_probe(&mut probe, RunConfig::default(), driver, true);
        assert_eq!(probe.pressed, 1);
        assert_eq!(probe.released, 1);
        // Held for 200 ms: about 12 updates.
        assert!(probe.down.abs_diff(12) <= 1, "{}", probe.down);
    }

    #[test]
    fn key_hold_times_out_without_release_events() {
        let mut driver = ScriptedDriver::new();
        let a = KeyCode::Char('a');
        driver.push_event(MS * 100, key(a, KeyEventKind::Press, KeyModifiers::NONE));
        let mut probe = Probe::new(Duration::from_secs(1));
        run_probe(&mut probe, RunConfig::default(), driver, false);
        assert_eq!(probe.pressed, 1);
        assert_eq!(probe.released, 1);
        // Held until the 500 ms timeout: about 30 updates.
        assert!(probe.down.abs_diff(30) <= 2, "{}", probe.down);
    }

    #[test]
    fn ctrl_c_quits_unless_disabled() {
        let ctrl_c = key(
            KeyCode::Char('c'),
            KeyEventKind::Press,
            KeyModifiers::CONTROL,
        );

        let mut driver = ScriptedDriver::new();
        driver.push_event(MS * 100, ctrl_c.clone());
        let mut probe = Probe::new(Duration::from_secs(10));
        run_probe(&mut probe, RunConfig::default(), driver, true);
        assert!(probe.updates <= 7, "{}", probe.updates);
        assert_eq!(probe.events, 0);

        let mut driver = ScriptedDriver::new();
        driver.push_event(MS * 100, ctrl_c);
        let mut probe = Probe::new(Duration::from_millis(500));
        let config = RunConfig {
            quit_on_ctrl_c: false,
            ..RunConfig::default()
        };
        run_probe(&mut probe, config, driver, true);
        assert_eq!(probe.updates, 30);
        assert_eq!(probe.events, 1);
    }

    #[test]
    fn focus_loss_pauses_when_configured() {
        let mut driver = ScriptedDriver::new();
        driver.push_event(MS * 200, Event::FocusLost);
        driver.push_event(MS * 700, Event::FocusGained);
        let mut probe = Probe::new(Duration::from_secs(1));
        let config = RunConfig {
            pause_on_focus_lost: true,
            ..RunConfig::default()
        };
        run_with(&mut probe, config, terminal(true), &mut driver).unwrap();
        // 500 ms of pause: one second of game time takes about 1.5 s.
        let elapsed = driver.elapsed();
        assert!(elapsed >= MS * 1480 && elapsed <= MS * 1540, "{elapsed:?}");
    }

    #[test]
    fn invalid_config_is_rejected() {
        let mut probe = Probe::new(Duration::from_secs(1));
        let config = RunConfig {
            update_rate: 0,
            ..RunConfig::default()
        };
        let result = run_with(&mut probe, config, terminal(true), ScriptedDriver::new());
        assert!(matches!(
            result,
            Err(RunError::Config(ConfigError::InvalidUpdateRate(0)))
        ));
        assert_eq!(probe.frames, 0);
    }

    #[test]
    fn resize_is_visible_to_the_game() {
        struct SizeProbe(Vec<vnce_core::Size>);
        impl Game for SizeProbe {
            fn update(&mut self, ctx: &mut Context) {
                if ctx.update_count() >= 10 {
                    ctx.quit();
                }
            }
            fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
                self.0.push(ctx.size());
                frame.buffer_mut().set_string(0, 0, "x", Style::new());
            }
        }
        let mut game = SizeProbe(Vec::new());
        let mut terminal = terminal(true);
        terminal.backend_mut().resize(30, 8);
        run_with(
            &mut game,
            RunConfig::default(),
            terminal,
            ScriptedDriver::new(),
        )
        .unwrap();
        assert!(game.0.iter().all(|s| *s == vnce_core::Size::new(30, 8)));
    }
}
