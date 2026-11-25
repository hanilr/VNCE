//! `--bench`: plays the island without a screen, walking a fixed route and turning the
//! camera, and prints the time and output per frame for several display methods.

use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vnce::vnce_core::Size;
use vnce::vnce_game::{SaveFile, SceneStack};
use vnce::vnce_raster::PixelMode;
use vnce::vnce_runtime::{Context, FpsLimit, Game, RunConfig, ScriptedDriver, run_with};
use vnce::vnce_term::{
    Backend, Capabilities, Event, Frame, GraphicsMode, GraphicsProtocol, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers, Terminal, TerminalOptions,
};

use crate::Shared;
use crate::play::Play;

/// Runs a game for a fixed number of frames, then quits.
pub struct Limited<G> {
    pub game: G,
    pub frames: u64,
}

impl<G: Game> Game for Limited<G> {
    fn init(&mut self, ctx: &mut Context) {
        self.game.init(ctx);
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        self.game.event(ctx, event);
    }

    fn update(&mut self, ctx: &mut Context) {
        self.game.update(ctx);
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        self.game.render(ctx, frame);
        if ctx.frame_count() + 1 >= self.frames {
            ctx.quit();
        }
    }
}

/// A terminal that throws the output away and counts its bytes.
struct Sink {
    size: Size,
    bytes: Rc<Cell<usize>>,
}

impl Backend for Sink {
    fn size(&self) -> io::Result<Size> {
        Ok(self.size)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.bytes.set(self.bytes.get() + bytes.len());
        std::hint::black_box(bytes);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn enter(&mut self, _options: &TerminalOptions) -> io::Result<()> {
        Ok(())
    }

    fn leave(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A route over the island with two camera turns and a zoom: (key, milliseconds held).
pub const ROUTE: [(KeyCode, u64); 9] = [
    (KeyCode::Up, 1500),
    (KeyCode::Char('e'), 60),
    (KeyCode::Left, 1400),
    (KeyCode::Up, 1200),
    (KeyCode::Char('+'), 60),
    (KeyCode::Right, 1600),
    (KeyCode::Char('q'), 60),
    (KeyCode::Down, 1200),
    (KeyCode::Char('-'), 60),
];

/// Holds the keys of [`ROUTE`] one after another, starting at `start` milliseconds.
pub fn walk(driver: &mut ScriptedDriver, start: u64) {
    let mut at = start;
    for (code, hold) in ROUTE {
        let key = |kind| {
            Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind,
            })
        };
        driver.push_event(Duration::from_millis(at), key(KeyEventKind::Press));
        driver.push_event(Duration::from_millis(at + hold), key(KeyEventKind::Release));
        at += hold + 150;
    }
}

/// Plays the route in every display method and prints the time per frame, the part of it
/// spent drawing, and the output per frame.
pub fn run(gpu: bool) {
    const FRAMES: u64 = 600;
    let modes = [
        GraphicsMode::Cells(PixelMode::HalfBlock),
        GraphicsMode::Cells(PixelMode::Sextant),
        GraphicsMode::Cells(PixelMode::Braille),
        GraphicsMode::Protocol(GraphicsProtocol::Kitty),
        GraphicsMode::Protocol(GraphicsProtocol::Sixel),
    ];
    println!("Isle, {FRAMES} frames at 200×60 cells, at dusk in the rain, walking and turning:");
    let mut ocean = String::new();
    for mode in modes {
        let shared = Shared::new(SaveFile::new(), gpu, false);
        ocean = shared.visuals.borrow().describe();
        let mut game = Limited {
            game: SceneStack::new(Play::bench(3), shared),
            frames: FRAMES,
        };
        let bytes = Rc::new(Cell::new(0));
        let capabilities = Capabilities {
            key_release_events: true,
            ..Capabilities::default()
        };
        let backend = Sink {
            size: Size::new(200, 60),
            bytes: Rc::clone(&bytes),
        };
        let Ok(mut terminal) = Terminal::new(backend, capabilities) else {
            continue;
        };
        terminal.set_graphics_mode(mode);
        let mut driver = ScriptedDriver::new();
        walk(&mut driver, 200);
        let config = RunConfig {
            fps: FpsLimit::Fixed(60),
            ..RunConfig::default()
        };
        let start = Instant::now();
        if let Err(error) = run_with(&mut game, config, terminal, driver) {
            println!("  {mode}: {error}");
            continue;
        }
        let elapsed = start.elapsed();
        let (render, frames) = game.game.data().render_time.get();
        let frames = f64::from(frames.max(1));
        println!(
            "  {:<12} {:>6.2} ms per frame (render {:>5.2} ms) {:>8.1} KB per frame",
            mode.to_string(),
            elapsed.as_secs_f64() * 1000.0 / FRAMES as f64,
            render / frames,
            bytes.get() as f64 / FRAMES as f64 / 1000.0,
        );
    }
    println!("Ocean: {ocean}");
}
