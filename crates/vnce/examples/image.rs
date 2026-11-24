//! Shows an image with the best method the terminal supports: Kitty, Sixel or iTerm2 for
//! real pixels, otherwise cells.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example image                  # a generated test image
//! cargo run --release --example image -- photo.png     # your own PNG
//! ```
//!
//! `G`: next method · `Esc` / `Q`: quit.

// Image demo code converts freely between pixel sizes and floats; every value stays far
// inside the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "pixel sizes and colors are small and clamped"
)]

use std::time::{Duration, Instant};

use vnce::vnce_core::{Color, Rect, Size, Style};
use vnce::vnce_raster::protocol::{kitty, sixel};
use vnce::vnce_raster::{Canvas, Rgba};
use vnce::vnce_runtime::{self, Context, Game, RunConfig};
use vnce::vnce_term::{
    Event, Frame, GraphicsMode, GraphicsProtocol, KeyCode, KeyEventKind, TerminalInfo,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let image = match std::env::args().nth(1) {
        Some(path) => Canvas::load_png(path)?,
        None => test_image(),
    };
    vnce_runtime::run(&mut Viewer::new(image), RunConfig::default())?;
    Ok(())
}

/// A colorful image with fine details, to compare methods.
fn test_image() -> Canvas {
    let size = 256;
    let mut canvas = Canvas::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 / size as f32, y as f32 / size as f32);
            let color = Rgba::rgb(
                (255.0 * fx) as u8,
                (255.0 * (1.0 - fy)) as u8,
                (128.0 + 127.0 * ((fx + fy) * 6.0).sin()) as u8,
            );
            canvas.pixel(x as i32, y as i32, color);
        }
    }
    let center = (size / 2) as i32;
    for (radius, color) in [
        (110, Rgba::WHITE),
        (80, Rgba::BLACK),
        (50, Rgba::rgb(255, 220, 0)),
    ] {
        canvas.circle(center, center, radius, color);
        canvas.circle(center, center, radius - 1, color);
    }
    canvas.fill_circle(center, center, 20, Rgba::new(255, 40, 40, 200));
    // Fine lines: only real pixels show them clearly.
    for i in 0..16 {
        let offset = i * 4;
        canvas.line(8, 8 + offset, 72, 8 + offset, Rgba::WHITE);
    }
    canvas
}

struct Viewer {
    image: Canvas,
    modes: Vec<GraphicsMode>,
    /// How long encoding the image took with the current method, and its size.
    encoding: Option<(Duration, usize)>,
}

impl Viewer {
    fn new(image: Canvas) -> Self {
        Self {
            image,
            modes: Vec::new(),
            encoding: None,
        }
    }
}

/// Cell size in pixels: reported, computed from the window size, or a common fallback.
fn cell_pixels(info: &TerminalInfo, size: Size) -> (u16, u16) {
    if let Some(cell) = info.cell_pixels {
        return cell;
    }
    match info.window_pixels {
        Some((w, h)) if size.width > 0 && size.height > 0 => (
            (w / u32::from(size.width)).max(1) as u16,
            (h / u32::from(size.height)).max(1) as u16,
        ),
        _ => (10, 20),
    }
}

/// The largest area inside `bounds` with the image's aspect ratio, centered.
fn fit(bounds: Rect, image: &Canvas, cell: (u16, u16)) -> Rect {
    if image.width() == 0 || image.height() == 0 || bounds.is_empty() {
        return Rect::ZERO;
    }
    let image_aspect = image.width() as f32 / image.height() as f32;
    let cell_aspect = f32::from(cell.0) / f32::from(cell.1);
    // Width in cells for the full height, keeping the aspect ratio.
    let mut height = f32::from(bounds.height);
    let mut width = height * image_aspect / cell_aspect;
    if width > f32::from(bounds.width) {
        width = f32::from(bounds.width);
        height = width * cell_aspect / image_aspect;
    }
    let (width, height) = (
        width.round().max(1.0) as u16,
        height.round().max(1.0) as u16,
    );
    Rect::new(
        bounds.x + (bounds.width - width.min(bounds.width)) / 2,
        bounds.y + (bounds.height - height.min(bounds.height)) / 2,
        width.min(bounds.width),
        height.min(bounds.height),
    )
}

/// Measures how long the current method takes to encode the image for `area`.
fn measure(
    mode: GraphicsMode,
    image: &Canvas,
    area: Rect,
    cell: (u16, u16),
    colors: u16,
) -> Option<(Duration, usize)> {
    let GraphicsMode::Protocol(protocol) = mode else {
        return None;
    };
    let start = Instant::now();
    let mut out = Vec::new();
    match protocol {
        GraphicsProtocol::Kitty => {
            let placement = kitty::Placement {
                id: 1,
                columns: area.width,
                rows: area.height,
                z_index: 0,
            };
            kitty::transmit_and_place_with(image, placement, kitty::Options::default(), &mut out);
        }
        GraphicsProtocol::Sixel => {
            let scaled = image.scaled(
                u32::from(area.width) * u32::from(cell.0),
                u32::from(area.height) * u32::from(cell.1),
            );
            let options = sixel::SixelOptions {
                max_colors: colors,
                dither: false,
                ..sixel::SixelOptions::default()
            };
            sixel::encode(&scaled, options, &mut out);
        }
        GraphicsProtocol::Iterm2 => {
            vnce::vnce_raster::protocol::iterm2::encode(image, area.width, area.height, &mut out)
                .ok()?;
        }
    }
    Some((start.elapsed(), out.len()))
}

impl Game for Viewer {
    fn init(&mut self, ctx: &mut Context) {
        self.modes = GraphicsMode::available(ctx.terminal_info(), &ctx.capabilities());
        if !self.modes.contains(&ctx.graphics_mode()) {
            self.modes.insert(0, ctx.graphics_mode());
        }
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
            KeyCode::Char('g' | 'G') => {
                let current = ctx.graphics_mode();
                let index = self.modes.iter().position(|&m| m == current).unwrap_or(0);
                ctx.set_graphics_mode(self.modes[(index + 1) % self.modes.len()]);
                self.encoding = None;
            }
            _ => {}
        }
    }

    fn update(&mut self, _ctx: &mut Context) {}

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let size = ctx.size();
        let info = ctx.terminal_info();
        let cell = cell_pixels(info, size);
        let bounds = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
        let area = fit(bounds, &self.image, cell);
        let mode = frame.graphics_mode();
        if self.encoding.is_none() {
            self.encoding = measure(
                mode,
                &self.image,
                area,
                cell,
                info.sixel_colors.unwrap_or(256),
            );
        }
        frame.draw_image(area, &self.image);

        let buffer = frame.buffer_mut();
        let terminal = match (&info.name, &info.version) {
            (Some(name), Some(version)) => format!("{name} {version}"),
            (Some(name), None) => name.clone(),
            _ if info.responded => "unknown terminal".to_owned(),
            _ => "terminal did not answer".to_owned(),
        };
        let yes_no = |b: bool| if b { "yes" } else { "no" };
        let encoding = match self.encoding {
            Some((time, bytes)) => format!(
                " · encode {:.1} ms, {} KB",
                time.as_secs_f64() * 1000.0,
                bytes / 1024
            ),
            None => String::new(),
        };
        let status = format!(
            " {terminal} · kitty {} · sixel {} · cell {}×{} px · mode {mode}{encoding}",
            yes_no(info.kitty_graphics),
            yes_no(info.sixel),
            cell.0,
            cell.1,
        );
        let style = Style::new()
            .fg(Color::rgb(20, 20, 30))
            .bg(Color::rgb(160, 220, 140));
        buffer.set_style(Rect::new(0, 0, size.width, 1), style);
        buffer.set_string(0, 0, &status, style.bold());
        let modes: Vec<&str> = self.modes.iter().map(|m| m.name()).collect();
        let help = format!(" G: next method ({}) · Esc: quit", modes.join(", "));
        buffer.set_string(0, size.height.saturating_sub(1), &help, Style::new().dim());
    }
}
