//! A tour of the window: text styles, borders, block elements, sextants, braille and an
//! image, with the last key and mouse events. `Esc` quits; F11 / Alt+Enter: fullscreen.
//!
//! ```sh
//! cargo run --release -p vnce-window --example hello
//! cargo run --release -p vnce-window --example hello -- --frames 120   # quits by itself
//! ```

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "small pixel and cell coordinates"
)]

use vnce_core::{Color, Modifiers, Rect, Style};
use vnce_raster::{Canvas, Rgba};
use vnce_runtime::{Context, Game, RunConfig};
use vnce_term::{Event, Frame, KeyCode, KeyEventKind};
use vnce_window::WindowOptions;

struct Hello {
    last: String,
    frames: Option<u64>,
    picture: Canvas,
}

impl Game for Hello {
    fn event(&mut self, ctx: &mut Context, event: &Event) {
        if let Event::Key(key) = event {
            if key.code == KeyCode::Esc && key.kind == KeyEventKind::Press {
                ctx.quit();
            }
        }
        self.last = format!("{event:?}");
    }

    fn update(&mut self, _ctx: &mut Context) {}

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        if self.frames.is_some_and(|n| ctx.frame_count() >= n) {
            ctx.quit();
        }
        let t = ctx.elapsed().as_secs_f32();
        let size = ctx.size();
        let mode = frame.graphics_mode();
        let buffer = frame.buffer_mut();
        let title = Style::new().fg(Color::rgb(255, 200, 120)).bold();
        buffer.set_string(2, 1, "vnce in a window", title);
        let styles = [
            ("bold", Modifiers::BOLD),
            ("italic", Modifiers::ITALIC),
            ("underline", Modifiers::UNDERLINE),
            ("dim", Modifiers::DIM),
            ("reverse", Modifiers::REVERSE),
            ("strike", Modifiers::STRIKETHROUGH),
        ];
        let mut x = 2;
        for (name, modifier) in styles {
            buffer.set_string(x, 3, name, Style::new().add_modifier(modifier));
            x += name.len() as u16 + 2;
        }
        buffer.set_string(
            2,
            5,
            "┌──────┐ ╭──────╮ ╔══════╗ ┏━━━━━━┓",
            Style::new().fg(Color::Cyan),
        );
        buffer.set_string(
            2,
            6,
            "│ thin │ │ soft │ ║ dbl  ║ ┃ bold ┃",
            Style::new().fg(Color::Cyan),
        );
        buffer.set_string(
            2,
            7,
            "└──────┘ ╰──────╯ ╚══════╝ ┗━━━━━━┛",
            Style::new().fg(Color::Cyan),
        );
        buffer.set_string(
            2,
            9,
            "▀▄█▌▐░▒▓ ▁▂▃▄▅▆▇█ ▖▗▘▙▚▛▜▝▞▟",
            Style::new().fg(Color::Green),
        );
        let sextants: String = (0x1FB00..0x1FB3C).filter_map(char::from_u32).collect();
        buffer.set_string(2, 10, &sextants, Style::new().fg(Color::Yellow));
        let braille: String = (0x2800..0x2840).filter_map(char::from_u32).collect();
        buffer.set_string(2, 11, &braille, Style::new().fg(Color::Magenta));
        buffer.set_string(
            2,
            12,
            "Türkçe ğüşıöç · ünïcödé · → ← ↑ ↓ · 日本",
            Style::new(),
        );
        buffer.set_string(
            2,
            14,
            &format!("last event: {}", self.last),
            Style::new().dim(),
        );
        buffer.set_string(
            2,
            15,
            &format!(
                "{}×{} cells · {} · {:.0} fps · Esc: quit · F11: fullscreen",
                size.width,
                size.height,
                mode,
                ctx.fps()
            ),
            Style::new().dim(),
        );
        // A moving picture in real pixels.
        let area = Rect::new(
            2,
            17,
            40.min(size.width.saturating_sub(4)),
            10.min(size.height.saturating_sub(18)),
        );
        let (width, height) = frame.image_pixel_size(area);
        if width > 0 && height > 0 {
            if (self.picture.width(), self.picture.height()) != (width, height) {
                self.picture = Canvas::new(width, height);
            }
            for (i, pixel) in self.picture.pixels_mut().iter_mut().enumerate() {
                let (px, py) = (
                    (i as u32 % width) as f32 / width as f32,
                    (i as u32 / width) as f32 / height as f32,
                );
                let wave = ((px * 12.0 + t * 2.0).sin() * 0.5 + 0.5) * 255.0;
                *pixel = Rgba::rgb((px * 255.0) as u8, wave as u8, (py * 255.0) as u8);
            }
            frame.draw_image(area, &self.picture);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let frames = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok());
    let mut game = Hello {
        last: "none yet".to_owned(),
        frames,
        picture: Canvas::new(0, 0),
    };
    vnce_window::run(
        &mut game,
        RunConfig::default(),
        &WindowOptions::titled("vnce window"),
    )?;
    Ok(())
}
