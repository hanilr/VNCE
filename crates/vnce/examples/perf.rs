//! Performance measurements of the engine's per-frame work: terminal output, buffer
//! diffing and CPU drawing. Nothing is shown on the screen; frames are written to a
//! backend that only counts bytes.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example perf
//! cargo run --release --example perf -- --frames 1000   # more frames, steadier numbers
//! ```
//!
//! The numbers are averages per frame (or per call). They are used to compare changes on
//! the same machine; see `docs/plans/2026-11-25-phase-12-optimization.md`.

// Measurement code converts freely between pixel coordinates and floats; every value stays
// far inside the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "pixel coordinates and colors are small and clamped"
)]

use std::hint::black_box;
use std::io;
use std::time::{Duration, Instant};

use vnce::vnce_core::{Color, ColorDepth, Rect, Size, Style};
use vnce::vnce_raster::{
    BlendMode, BlitOptions, Canvas, PixelMode, RenderOptions, Rgba, ScaleQuality,
};
use vnce::vnce_term::{
    Backend, Capabilities, GraphicsMode, GraphicsProtocol, Terminal, TerminalOptions,
};

/// Terminal size of the terminal scenarios, in cells.
const COLUMNS: u16 = 200;
/// Terminal size of the terminal scenarios, in cells.
const ROWS: u16 = 60;

/// A backend that throws the output away and counts its bytes.
struct Sink {
    size: Size,
    bytes: usize,
}

impl Backend for Sink {
    fn size(&self) -> io::Result<Size> {
        Ok(self.size)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.bytes += bytes.len();
        black_box(bytes);
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

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let frames = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(300u32)
        .max(1);

    println!("Terminal ({COLUMNS}×{ROWS} cells, {frames} frames):");
    terminal_scenarios(frames)?;
    println!();
    println!("Images with graphics protocols ({COLUMNS}×{ROWS} cells, {frames} frames):");
    image_scenarios(frames)?;
    println!();
    println!("Canvas (400×240 pixels, {frames} frames):");
    canvas_scenarios(frames);
    println!();
    println!("Scaling:");
    scaling_scenarios(frames.div_ceil(10));
    Ok(())
}

/// A terminal that writes into a [`Sink`], with its first frame already drawn.
fn sink_terminal(depth: ColorDepth) -> io::Result<Terminal<Sink>> {
    let backend = Sink {
        size: Size::new(COLUMNS, ROWS),
        bytes: 0,
    };
    let capabilities = Capabilities {
        color_depth: depth,
        ..Capabilities::default()
    };
    let mut terminal = Terminal::new(backend, capabilities)?;
    terminal.draw(|_| {})?;
    terminal.backend_mut().bytes = 0;
    Ok(terminal)
}

/// Prints one result line: time per frame and, when given, bytes per frame.
fn report(name: &str, frames: u32, time: Duration, bytes: Option<usize>) {
    let ms = time.as_secs_f64() * 1000.0 / f64::from(frames);
    match bytes {
        Some(bytes) => {
            let kb = bytes as f64 / f64::from(frames) / 1000.0;
            println!("  {name:<46} {ms:>8.3} ms {kb:>9.1} KB");
        }
        None => println!("  {name:<46} {ms:>8.3} ms"),
    }
}

/// Fills `canvas` with a moving gradient: every pixel changes every frame.
fn gradient(canvas: &mut Canvas, frame: u32) {
    let width = canvas.width();
    for (i, pixel) in canvas.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (i as u32 % width, i as u32 / width);
        let v = ((x + y + frame) % 256) as u8;
        *pixel = Rgba::rgb(v, v.wrapping_mul(3), 255 - v);
    }
}

fn terminal_scenarios(frames: u32) -> io::Result<()> {
    let area = Rect::new(0, 0, COLUMNS, ROWS);

    // A full-screen half-block animation: every cell changes every frame.
    for (name, depth) in [
        ("half blocks, full screen, truecolor", ColorDepth::TrueColor),
        (
            "half blocks, full screen, 256 colors",
            ColorDepth::Indexed256,
        ),
    ] {
        let mut terminal = sink_terminal(depth)?;
        let mut canvas = Canvas::for_area(area, PixelMode::HalfBlock);
        let (mut convert, mut total) = (Duration::ZERO, Duration::ZERO);
        for frame in 0..frames {
            gradient(&mut canvas, frame);
            let start = Instant::now();
            terminal.draw(|f| {
                let start = Instant::now();
                canvas.render(
                    f.buffer_mut(),
                    area,
                    PixelMode::HalfBlock,
                    &RenderOptions::default(),
                );
                convert += start.elapsed();
            })?;
            total += start.elapsed();
        }
        let bytes = terminal.backend().bytes;
        report(&format!("{name}: convert"), frames, convert, None);
        report(
            &format!("{name}: flush"),
            frames,
            total.saturating_sub(convert),
            Some(bytes),
        );
    }

    // Text on every row; only the first row changes.
    let mut terminal = sink_terminal(ColorDepth::TrueColor)?;
    let line = "The quick brown fox jumps over the lazy dog. ".repeat(5);
    let start = Instant::now();
    for frame in 0..frames {
        terminal.draw(|f| {
            let buffer = f.buffer_mut();
            for y in 0..ROWS {
                buffer.set_string(0, y, &line, Style::new().fg(Color::Green));
            }
            let counter = format!("frame {frame}");
            buffer.set_string(0, 0, &counter, Style::new().fg(Color::Yellow).bold());
        })?;
    }
    let bytes = terminal.backend().bytes;
    report(
        "text on every row, one row changes",
        frames,
        start.elapsed(),
        Some(bytes),
    );

    // Scattered cells: 600 "stars" move one column per frame over an empty screen.
    let mut terminal = sink_terminal(ColorDepth::TrueColor)?;
    let start = Instant::now();
    for frame in 0..frames {
        terminal.draw(|f| {
            let buffer = f.buffer_mut();
            for i in 0..600u32 {
                let x = (i * 7919 + frame) % u32::from(COLUMNS);
                let y = (i * 104_729) % u32::from(ROWS);
                let style = Style::new().fg(Color::Rgb(255, 255, (i % 256) as u8));
                buffer.set_string(x as u16, y as u16, "*", style);
            }
        })?;
    }
    let bytes = terminal.backend().bytes;
    report(
        "600 scattered cells move",
        frames,
        start.elapsed(),
        Some(bytes),
    );

    // Nothing drawn: the fixed cost of a frame (clearing and diffing).
    let mut terminal = sink_terminal(ColorDepth::TrueColor)?;
    let start = Instant::now();
    for _ in 0..frames {
        terminal.draw(|_| {})?;
    }
    let bytes = terminal.backend().bytes;
    report("empty frame", frames, start.elapsed(), Some(bytes));
    Ok(())
}

/// Full-screen images shown with Kitty and Sixel, at the size of
/// `Frame::image_pixel_size` (as Relay draws them, so nothing is scaled): a still picture,
/// and the same picture with a moving sprite.
/// `render` is the time spent inside the draw closure (`Frame::draw_image`); `flush` is the
/// rest of the frame (planning and encoding).
fn image_scenarios(frames: u32) -> io::Result<()> {
    let area = Rect::new(0, 0, COLUMNS, ROWS);
    for protocol in [GraphicsProtocol::Kitty, GraphicsProtocol::Sixel] {
        for moving in [false, true] {
            let mut terminal = sink_terminal(ColorDepth::TrueColor)?;
            terminal.set_graphics_mode(GraphicsMode::Protocol(protocol));
            let mut size = (0, 0);
            terminal.draw(|f| size = f.image_pixel_size(area))?;
            let mut background = Canvas::new(size.0, size.1);
            gradient(&mut background, 0);
            let mut canvas = background.clone();
            // The first frame sends the whole image; it is not measured.
            terminal.draw(|f| f.draw_image(area, &canvas))?;
            terminal.backend_mut().bytes = 0;
            let (mut render, mut total) = (Duration::ZERO, Duration::ZERO);
            for frame in 0..frames {
                if moving {
                    canvas.pixels_mut().copy_from_slice(background.pixels());
                    let x = (frame * 7 % (canvas.width() - 40)) as i32;
                    canvas.fill_rect(x, 300, 40, 40, Rgba::rgb(255, 255, 255));
                }
                let start = Instant::now();
                terminal.draw(|f| {
                    let start = Instant::now();
                    f.draw_image(area, &canvas);
                    render += start.elapsed();
                })?;
                total += start.elapsed();
            }
            let bytes = terminal.backend().bytes;
            let name = format!(
                "{protocol:?}, {}",
                if moving { "moving sprite" } else { "still" }
            );
            report(&format!("{name}: render"), frames, render, None);
            report(
                &format!("{name}: flush"),
                frames,
                total.saturating_sub(render),
                Some(bytes),
            );
        }
    }
    Ok(())
}

/// Sprites used by the canvas scenarios.
struct Sprites {
    /// Semi-transparent, with an opaque circle.
    blended: Canvas,
    /// Fully opaque.
    opaque: Canvas,
}

/// One canvas scenario: draws on the canvas for frame `f`.
type Scenario = fn(&mut Canvas, &Sprites, i32);

/// The canvas scenarios, each timed separately.
const CANVAS_SCENARIOS: [(&str, Scenario); 9] = [
    ("clear", |c, _, _| c.clear(Rgba::BLACK)),
    ("200 opaque rectangles 30×20", |c, _, f| {
        for i in 0..200 {
            c.fill_rect(
                (i * 11 + f) % 400,
                (i * 7) % 240,
                30,
                20,
                Rgba::rgb(0, 0, 255),
            );
        }
    }),
    ("200 semi-transparent rectangles 30×20", |c, _, f| {
        let color = Rgba::new(0, 0, 255, 90);
        for i in 0..200 {
            c.fill_rect((i * 11 + f) % 400, (i * 7) % 240, 30, 20, color);
        }
    }),
    ("500 opaque blits 16×16", |c, s, f| {
        for i in 0..500 {
            let (x, y) = ((i * 37 + f) % 400, (i * 53) % 240);
            c.blit(&s.opaque, x, y, BlitOptions::default());
        }
    }),
    ("500 semi-transparent blits 16×16", |c, s, f| {
        for i in 0..500 {
            let (x, y) = ((i * 37 + f) % 400, (i * 53) % 240);
            c.blit(&s.blended, x, y, BlitOptions::default());
        }
    }),
    ("50 semi-transparent circles, radius 20", |c, _, f| {
        for i in 0..50 {
            let (x, y) = ((i * 17 + f) % 400, (i * 29) % 240);
            c.fill_circle(x, y, 20, Rgba::new(0, 255, 0, 128));
        }
    }),
    ("200 anti-aliased lines", |c, _, _| {
        for i in 0..200 {
            let y = (i * 3 % 240) as f32;
            c.line_aa(0.0, i as f32, 399.0, y, Rgba::new(255, 255, 0, 200));
        }
    }),
    ("100 semi-transparent triangles", |c, _, f| {
        for i in 0..100 {
            let x = ((i * 7 + f) % 400) as f32;
            let color = Rgba::new(255, 0, 255, 100);
            c.fill_triangle((x, 0.0), (x + 40.0, 100.0), (x - 30.0, 200.0), color);
        }
    }),
    ("20 gradient triangles", |c, _, f| {
        for i in 0..20 {
            let x = ((i * 19 + f) % 400) as f32;
            c.fill_triangle_gradient(
                ((x, 0.0), Rgba::new(255, 0, 0, 200)),
                ((x + 60.0, 120.0), Rgba::new(0, 255, 0, 200)),
                ((x - 50.0, 230.0), Rgba::new(0, 0, 255, 200)),
            );
        }
    }),
];

fn canvas_scenarios(frames: u32) {
    let mut canvas = Canvas::new(400, 240);
    canvas.set_blend_mode(BlendMode::Alpha);
    let mut blended = Canvas::filled(16, 16, Rgba::new(200, 100, 50, 180));
    blended.fill_circle(8, 8, 5, Rgba::WHITE);
    let sprites = Sprites {
        blended,
        opaque: Canvas::filled(16, 16, Rgba::rgb(40, 160, 220)),
    };

    let mut times = [Duration::ZERO; CANVAS_SCENARIOS.len()];
    for frame in 0..frames {
        for ((_, draw), time) in CANVAS_SCENARIOS.iter().zip(&mut times) {
            let start = Instant::now();
            draw(&mut canvas, &sprites, frame as i32);
            *time += start.elapsed();
        }
    }
    black_box(&canvas);
    for ((name, _), time) in CANVAS_SCENARIOS.iter().zip(times) {
        report(name, frames, time, None);
    }
}

fn scaling_scenarios(frames: u32) {
    let mut source = Canvas::new(400, 240);
    gradient(&mut source, 0);
    for (name, width, height) in [
        ("400×240 → 1600×880 (enlarge)", 1600, 880),
        ("400×240 → 160×96 (shrink)", 160, 96),
    ] {
        for quality in ScaleQuality::ALL {
            let start = Instant::now();
            for _ in 0..frames {
                black_box(source.scaled_with(width, height, quality));
            }
            report(&format!("{quality}: {name}"), frames, start.elapsed(), None);
        }
    }
}
