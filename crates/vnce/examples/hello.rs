//! A first look at the engine: colored and Unicode text, a truecolor gradient, a small
//! animation, terminal resizing and input events.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example hello
//! ```
//!
//! Press `q`, `Esc` or `Ctrl+C` to quit.

use std::io;
use std::time::{Duration, Instant};

use vnce::vnce_core::{Cell, Color, Rect, Style};
use vnce::vnce_term::input::{self, Event, KeyCode, KeyModifiers};
use vnce::vnce_term::{Frame, Terminal, TerminalOptions};

/// Time to wait for input before drawing the next frame (about 20 frames per second).
const FRAME_TIME: Duration = Duration::from_millis(50);

fn main() -> io::Result<()> {
    let mut terminal = Terminal::enter_with(TerminalOptions {
        mouse_capture: true,
        focus_events: true,
        ..TerminalOptions::default()
    })?;
    let color_depth = terminal.capabilities().color_depth;
    let mut last_event = String::from("(none yet)");
    let mut next_frame = Instant::now();

    loop {
        terminal.draw(|frame| render(frame, &format!("{color_depth:?}"), &last_event))?;

        // Frames follow the clock, not the input: every event that arrives before the next
        // frame is handled, but none of them triggers an extra frame. (Previously each
        // event caused a redraw, so moving the mouse sped up the animation.)
        next_frame = (next_frame + FRAME_TIME).max(Instant::now());
        while let Some(event) = input::poll(next_frame.saturating_duration_since(Instant::now()))? {
            if is_quit(&event) {
                return Ok(());
            }
            last_event = format!("{event:?}");
        }
    }
}

/// `q`, `Esc` or `Ctrl+C`, on key press only (Windows also reports key releases).
fn is_quit(event: &Event) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    key.is_press()
        && (matches!(key.code, KeyCode::Char('q' | 'Q') | KeyCode::Esc)
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

fn render(frame: &mut Frame<'_>, color_depth: &str, last_event: &str) {
    let area = frame.area();
    let count = frame.count();
    let buffer = frame.buffer_mut();

    // Title bar.
    let title_style = Style::new()
        .fg(Color::rgb(20, 20, 30))
        .bg(Color::rgb(120, 200, 255))
        .bold();
    buffer.set_style(Rect::new(0, 0, area.width, 1), title_style);
    buffer.set_string(1, 0, "Variation CLI Engine — hello", title_style);

    // Information lines.
    let label = Style::new().fg(Color::rgb(150, 150, 170));
    let value = Style::new().fg(Color::rgb(255, 220, 120));
    let lines = [
        ("Terminal size", format!("{} × {}", area.width, area.height)),
        ("Color depth", color_depth.to_owned()),
        ("Frame", count.to_string()),
        ("Last event", last_event.to_owned()),
    ];
    for (row, (name, text)) in (2..).zip(lines) {
        let x = buffer.set_string(2, row, &format!("{name:<14}"), label);
        buffer.set_string(x, row, &text, value);
    }

    // Text styles and Unicode.
    let mut x = 2;
    for (text, style) in [
        ("bold", Style::new().bold()),
        ("italic", Style::new().italic()),
        ("underline", Style::new().underline()),
        ("strike", Style::new().strikethrough()),
        ("reverse", Style::new().reverse()),
    ] {
        x = buffer.set_string(x, 7, text, style.fg(Color::BrightGreen));
        x = buffer.set_string(x, 7, "  ", Style::new());
    }
    buffer.set_string(
        2,
        8,
        "Unicode: héllo wörld · 日本語 · ½ → ∞ · 🦀",
        Style::new().fg(Color::BrightMagenta),
    );

    // Truecolor gradient (downsampled automatically on 256 / 16 color terminals).
    let gradient_row = 10;
    for column in 0..area.width {
        let hue = f32::from(column) / f32::from(area.width.max(1));
        if let Some(cell) = buffer.cell_mut(column, gradient_row) {
            cell.set_char('▀');
            cell.fg = hue_to_color(hue);
            cell.bg = hue_to_color((hue + 0.5) % 1.0);
        }
    }

    // A bouncing ball: only the cells that change are sent to the terminal.
    let track = Rect::new(2, 12, area.width.saturating_sub(4), 1);
    if !track.is_empty() {
        buffer.fill(track, &Cell::new("·"));
        let period = u64::from(track.width.max(2) - 1) * 2;
        let step = count % period;
        let offset = if step < period / 2 {
            step
        } else {
            period - step
        };
        let ball_x = track.x + u16::try_from(offset).unwrap_or(0);
        buffer.set_string(ball_x, track.y, "●", Style::new().fg(Color::BrightYellow));
    }

    // Footer.
    let footer = "q / Esc / Ctrl+C: quit · move the mouse, type, resize the window";
    let footer_row = area.height.saturating_sub(1);
    buffer.set_string(1, footer_row, footer, Style::new().dim());
}

/// Converts a hue in `0.0..1.0` to a saturated RGB color.
fn hue_to_color(hue: f32) -> Color {
    let sector = hue.rem_euclid(1.0) * 6.0;
    let ramp = 1.0 - (sector % 2.0 - 1.0).abs();
    let (red, green, blue) = match sector {
        s if s < 1.0 => (1.0, ramp, 0.0),
        s if s < 2.0 => (ramp, 1.0, 0.0),
        s if s < 3.0 => (0.0, 1.0, ramp),
        s if s < 4.0 => (0.0, ramp, 1.0),
        s if s < 5.0 => (ramp, 0.0, 1.0),
        _ => (1.0, 0.0, ramp),
    };
    Color::rgb(channel(red), channel(green), channel(blue))
}

/// Converts a channel in `0.0..=1.0` to `0..=255`.
fn channel(value: f32) -> u8 {
    // The value is clamped to 0..=255 first, so the conversion cannot truncate.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped to the u8 range"
    )]
    let byte = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    byte
}
