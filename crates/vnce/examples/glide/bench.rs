//! `--bench`: flies a course with the autopilot without a screen and prints the time and
//! output per frame for several display methods, on the GPU and the CPU.

use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::time::Instant;

use vnce::vnce_core::Size;
use vnce::vnce_game::{SaveFile, SceneStack};
use vnce::vnce_raster::PixelMode;
use vnce::vnce_runtime::{Context, FpsLimit, Game, RunConfig, ScriptedDriver, run_with};
use vnce::vnce_term::{
    Backend, Capabilities, Event, Frame, GraphicsMode, GraphicsProtocol, Terminal, TerminalOptions,
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

/// Flies the course in every display method, on the GPU (when `gpu` and there is one)
/// and on the CPU, and prints the time per frame, the part of it spent drawing the world,
/// and the output per frame.
pub fn run(gpu: bool) {
    const FRAMES: u64 = 480;
    let modes = [
        GraphicsMode::Cells(PixelMode::HalfBlock),
        GraphicsMode::Cells(PixelMode::Sextant),
        GraphicsMode::Cells(PixelMode::Braille),
        GraphicsMode::Protocol(GraphicsProtocol::Kitty),
        GraphicsMode::Protocol(GraphicsProtocol::Sixel),
    ];
    println!("Glide, {FRAMES} frames at 200×60 cells, the autopilot flying a course:");
    let renderers: &[bool] = if gpu { &[true, false] } else { &[false] };
    for &use_gpu in renderers {
        for mode in modes {
            let shared = Shared::new(SaveFile::new(), use_gpu, false);
            let name = shared.renderer.borrow().describe();
            let mut game = Limited {
                game: SceneStack::new(Play::bench(3), shared),
                frames: FRAMES,
            };
            let bytes = Rc::new(Cell::new(0));
            let backend = Sink {
                size: Size::new(200, 60),
                bytes: Rc::clone(&bytes),
            };
            let Ok(mut terminal) = Terminal::new(backend, Capabilities::default()) else {
                continue;
            };
            terminal.set_graphics_mode(mode);
            let config = RunConfig {
                fps: FpsLimit::Fixed(60),
                ..RunConfig::default()
            };
            let start = Instant::now();
            if let Err(error) = run_with(&mut game, config, terminal, ScriptedDriver::new()) {
                println!("  {mode}: {error}");
                continue;
            }
            let elapsed = start.elapsed();
            let (render, frames) = game.game.data().render_time.get();
            let frames = f64::from(frames.max(1));
            println!(
                "  {:<12} {:<28} {:>6.2} ms per frame (world {:>5.2} ms) {:>8.1} KB per frame",
                mode.to_string(),
                name,
                elapsed.as_secs_f64() * 1000.0 / FRAMES as f64,
                render / frames,
                bytes.get() as f64 / FRAMES as f64 / 1000.0,
            );
        }
    }
}
