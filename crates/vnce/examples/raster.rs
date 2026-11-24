//! Pixel graphics in the terminal: a plasma background, a rotating gradient triangle, an
//! anti-aliased wireframe cube, a pulsing ring and a bouncing sprite, rendered with every
//! pixel mode.
//!
//! Run with (release mode is recommended for pixel work):
//!
//! ```sh
//! cargo run --release --example raster
//! cargo run --release --example raster -- path/to/image.png   # image as background
//! cargo run --release --example raster -- --bench             # conversion timings
//! ```
//!
//! `M`: next pixel mode · `D`: dithering (Braille / ASCII) · `Esc` / `Q`: quit.
//!
//! `M` only cycles through the modes that display correctly on the current terminal
//! (`PixelMode::available`). `Shift+M`, or starting with `--all-modes`, also includes the
//! modes that need font support; the status line then warns about them.

// Graphics demo code converts freely between pixel coordinates and floats; every value
// stays far inside the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    reason = "pixel coordinates and colors are small and clamped; math uses short names"
)]

use std::time::{Duration, Instant};

use vnce::vnce_core::{Buffer, Color, Rect, Size, Style};
use vnce::vnce_raster::{BlitOptions, Canvas, PixelMode, Quality, RenderOptions, Rgba};
use vnce::vnce_runtime::{self, Context, Game, RunConfig, RunError};
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind, KeyModifiers};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--bench") {
        bench();
        return Ok(());
    }
    let all_modes = args.iter().any(|a| a == "--all-modes");
    let image = match args.iter().find(|a| !a.starts_with("--")) {
        Some(path) => Some(Canvas::load_png(path)?),
        None => None,
    };
    run(image, all_modes)?;
    Ok(())
}

fn run(image: Option<Canvas>, all_modes: bool) -> Result<(), RunError> {
    let mut demo = Demo::new(image);
    demo.all_modes = all_modes;
    vnce_runtime::run(&mut demo, RunConfig::default())
}

/// A small original sprite.
fn sprite() -> Canvas {
    let y = Rgba::rgb(255, 210, 60);
    let o = Rgba::rgb(240, 120, 30);
    let k = Rgba::rgb(30, 30, 40);
    let w = Rgba::WHITE;
    Canvas::from_art(
        &[
            "..yyyy..", ".yyyyyy.", "yywkywky", "yykkykky", "yyyyyyyy", "yoyyyyoy", ".yooooy.",
            "..yyyy..",
        ],
        &[('y', y), ('o', o), ('k', k), ('w', w)],
    )
}

struct Demo {
    mode: PixelMode,
    /// Modes that display correctly on this terminal.
    available: Vec<PixelMode>,
    /// Cycle through every mode with `M` (`--all-modes`).
    all_modes: bool,
    dither: bool,
    canvas: Canvas,
    image: Option<Canvas>,
    sprite: Canvas,
    /// Sprite position and velocity in pixels per second.
    sprite_pos: (f32, f32),
    sprite_vel: (f32, f32),
    convert_time: Duration,
}

impl Demo {
    fn new(image: Option<Canvas>) -> Self {
        Self {
            mode: PixelMode::HalfBlock,
            available: vec![PixelMode::HalfBlock],
            all_modes: false,
            dither: false,
            canvas: Canvas::new(0, 0),
            image,
            sprite: sprite(),
            sprite_pos: (4.0, 4.0),
            sprite_vel: (37.0, 23.0),
            convert_time: Duration::ZERO,
        }
    }

    /// Switches to the next mode: among the available ones, or among all of them.
    fn next_mode(&mut self, all: bool) {
        let modes: &[PixelMode] = if all || self.all_modes {
            &PixelMode::ALL
        } else {
            &self.available
        };
        let index = modes.iter().position(|&m| m == self.mode);
        self.mode = match index {
            Some(index) => modes[(index + 1) % modes.len()],
            None => modes[0],
        };
    }
}

/// The area used for pixels: everything below the status line.
fn pixel_area(size: Size) -> Rect {
    Rect::new(0, 1, size.width, size.height.saturating_sub(1))
}

impl Game for Demo {
    fn init(&mut self, ctx: &mut Context) {
        let color_depth = ctx.capabilities().color_depth;
        self.mode = PixelMode::detect(color_depth);
        self.available = PixelMode::available(color_depth);
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        let Event::Key(key) = event else {
            return;
        };
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => ctx.quit(),
            // Shift+M arrives as `M`, or as `m` with SHIFT when keyboard enhancement is on.
            KeyCode::Char('M') => self.next_mode(true),
            KeyCode::Char('m') => self.next_mode(key.modifiers.contains(KeyModifiers::SHIFT)),
            KeyCode::Char('d' | 'D') => self.dither = !self.dither,
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut Context) {
        // Bounce the sprite inside the canvas.
        let dt = ctx.dt_secs();
        let max_x = (self.canvas.width() as f32 - self.sprite.width() as f32).max(0.0);
        let max_y = (self.canvas.height() as f32 - self.sprite.height() as f32).max(0.0);
        self.sprite_pos.0 += self.sprite_vel.0 * dt;
        self.sprite_pos.1 += self.sprite_vel.1 * dt;
        if self.sprite_pos.0 < 0.0 || self.sprite_pos.0 > max_x {
            self.sprite_vel.0 = -self.sprite_vel.0;
            self.sprite_pos.0 = self.sprite_pos.0.clamp(0.0, max_x);
        }
        if self.sprite_pos.1 < 0.0 || self.sprite_pos.1 > max_y {
            self.sprite_vel.1 = -self.sprite_vel.1;
            self.sprite_pos.1 = self.sprite_pos.1.clamp(0.0, max_y);
        }
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let area = pixel_area(ctx.size());
        let (columns, rows) = self.mode.cell_size();
        let (width, height) = (
            u32::from(area.width) * columns,
            u32::from(area.height) * rows,
        );
        if (self.canvas.width(), self.canvas.height()) != (width, height) {
            self.canvas.resize(width, height);
        }
        // Smooth time between updates.
        let time = ctx.elapsed().as_secs_f32() + ctx.alpha() * ctx.dt_secs();
        draw_scene(
            &mut self.canvas,
            self.mode,
            time,
            self.image.as_ref(),
            &self.sprite,
            self.sprite_pos,
        );

        let buffer = frame.buffer_mut();
        let options = RenderOptions {
            dither: self.dither,
            ..RenderOptions::default()
        };
        let start = Instant::now();
        self.canvas.render(buffer, area, self.mode, &options);
        self.convert_time = start.elapsed();

        let warning = if self.available.contains(&self.mode) {
            ""
        } else {
            " (needs font support)"
        };
        let status = format!(
            " {}{warning} ({columns}×{rows}) · {width}×{height} px · convert {:.2} ms · FPS {:.0} · dither {} · M: mode · Shift+M: all modes · D: dither · Esc: quit",
            self.mode,
            self.convert_time.as_secs_f64() * 1000.0,
            ctx.fps(),
            if self.dither { "on" } else { "off" },
        );
        let style = Style::new()
            .fg(Color::rgb(20, 20, 30))
            .bg(Color::rgb(120, 200, 255));
        buffer.set_style(Rect::new(0, 0, ctx.size().width, 1), style);
        buffer.set_string(0, 0, &status, style.bold());
    }
}

/// Draws the whole scene for time `t` (seconds).
fn draw_scene(
    canvas: &mut Canvas,
    mode: PixelMode,
    t: f32,
    image: Option<&Canvas>,
    sprite: &Canvas,
    sprite_pos: (f32, f32),
) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    if w < 4.0 || h < 4.0 {
        return;
    }
    // Horizontal pixel distances are multiplied by this to look correct on screen.
    let aspect = mode.pixel_aspect();

    match image {
        Some(image) => draw_image_fit(canvas, image),
        None => plasma(canvas, t, aspect),
    }

    let (cx, cy) = (w / 2.0, h / 2.0);
    let scale = h.min(w * aspect) * 0.35;
    let to_screen = |x: f32, y: f32| (cx + x * scale / aspect, cy + y * scale);

    // Rotating gradient triangle.
    let corner = |i: f32| {
        let angle = t * 0.7 + i * std::f32::consts::TAU / 3.0;
        to_screen(angle.cos() * 0.9, angle.sin() * 0.9)
    };
    canvas.fill_triangle_gradient(
        (corner(0.0), Rgba::new(255, 60, 60, 220)),
        (corner(1.0), Rgba::new(60, 255, 90, 220)),
        (corner(2.0), Rgba::new(70, 120, 255, 220)),
    );

    // Anti-aliased wireframe cube.
    let vertices: Vec<(f32, f32)> = (0..8)
        .map(|i| {
            let p = [
                if i & 1 == 0 { -0.5 } else { 0.5 },
                if i & 2 == 0 { -0.5 } else { 0.5 },
                if i & 4 == 0 { -0.5 } else { 0.5 },
            ];
            let (sa, ca) = (t * 0.9).sin_cos();
            let (sb, cb) = (t * 0.6).sin_cos();
            let x = p[0] * ca - p[2] * sa;
            let z = p[0] * sa + p[2] * ca;
            let y = p[1] * cb - z * sb;
            let z = p[1] * sb + z * cb;
            let perspective = 1.6 / (z + 2.4);
            to_screen(x * perspective, y * perspective)
        })
        .collect();
    for i in 0..8usize {
        for bit in [1, 2, 4] {
            let j = i ^ bit;
            if i < j {
                let (a, b) = (vertices[i], vertices[j]);
                canvas.line_aa(a.0, a.1, b.0, b.1, Rgba::WHITE);
            }
        }
    }

    // Pulsing anti-aliased ring.
    let radius = 0.95 + 0.08 * (t * 2.0).sin();
    let segments = 64;
    let point = |i: i32| {
        let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
        to_screen(angle.cos() * radius, angle.sin() * radius)
    };
    for i in 0..segments {
        let (a, b) = (point(i), point(i + 1));
        canvas.line_aa(a.0, a.1, b.0, b.1, Rgba::rgb(255, 230, 120));
    }

    // Bouncing sprite.
    canvas.blit(
        sprite,
        sprite_pos.0.round() as i32,
        sprite_pos.1.round() as i32,
        BlitOptions::default(),
    );
}

/// A colorful animated background.
fn plasma(canvas: &mut Canvas, t: f32, aspect: f32) {
    let (width, height) = (canvas.width(), canvas.height());
    let scale = 8.0 / height.max(1) as f32;
    for y in 0..height {
        for x in 0..width {
            let fx = x as f32 * aspect * scale;
            let fy = y as f32 * scale;
            let v = (fx + t).sin() + (fy * 1.3 - t * 0.8).sin() + ((fx + fy) * 0.7 + t * 0.5).sin();
            let r = 0.5 + 0.5 * (v * 1.1).sin();
            let g = 0.5 + 0.5 * (v * 1.1 + 2.1).sin();
            let b = 0.5 + 0.5 * (v * 1.1 + 4.2).sin();
            let color = Rgba::rgb((r * 90.0) as u8, (g * 70.0) as u8, (b * 140.0 + 40.0) as u8);
            if let Some(pixel) = canvas.pixels_mut().get_mut((y * width + x) as usize) {
                *pixel = color;
            }
        }
    }
}

/// Draws `image` scaled (nearest neighbor) to fill the canvas.
fn draw_image_fit(canvas: &mut Canvas, image: &Canvas) {
    let (width, height) = (canvas.width(), canvas.height());
    if image.width() == 0 || image.height() == 0 {
        return;
    }
    canvas.clear(Rgba::BLACK);
    for y in 0..height {
        for x in 0..width {
            let sx = (u64::from(x) * u64::from(image.width()) / u64::from(width)) as u32;
            let sy = (u64::from(y) * u64::from(image.height()) / u64::from(height)) as u32;
            if let Some(color) = image.get(sx, sy) {
                canvas.pixel(x as i32, y as i32, color);
            }
        }
    }
}

/// Prints how long each mode takes to convert a full 200×60-cell scene.
fn bench() {
    let size = Size::new(200, 60);
    let area = Rect::from(size);
    let sprite = sprite();
    println!(
        "Converting a {}×{}-cell scene ({} frames per mode):",
        size.width, size.height, 60
    );
    for mode in PixelMode::ALL {
        for quality in [Quality::Fast, Quality::Exact] {
            if quality == Quality::Exact && !matches!(mode, PixelMode::Octant | PixelMode::Braille)
            {
                continue;
            }
            let mut canvas = Canvas::for_area(area, mode);
            let mut buffer = Buffer::empty(size);
            let options = RenderOptions {
                quality,
                ..RenderOptions::default()
            };
            let mut total = Duration::ZERO;
            for frame in 0..60 {
                draw_scene(
                    &mut canvas,
                    mode,
                    frame as f32 / 60.0,
                    None,
                    &sprite,
                    (10.0, 10.0),
                );
                let start = Instant::now();
                canvas.render(&mut buffer, area, mode, &options);
                total += start.elapsed();
            }
            println!(
                "  {:<10} {:<6} {:>4}×{:<4} px  {:>7.2} ms / frame",
                mode.name(),
                format!("{quality:?}"),
                canvas.width(),
                canvas.height(),
                total.as_secs_f64() * 1000.0 / 60.0
            );
        }
    }
}
