//! Debugging and popup helpers: [`FpsOverlay`] and [`Clear`].

use std::time::Duration;

use vnce_core::{Buffer, Cell, Rect, Style};

use crate::Widget;
use crate::block::{Block, BorderType};
use crate::paragraph::Paragraph;
use crate::text::{Line, Text};

/// Resets every cell of an area (symbol and style), for example under a popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Clear;

impl Widget for Clear {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        buffer.fill(area, &Cell::EMPTY);
    }
}

/// A corner of an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Corner {
    /// Top left.
    TopLeft,
    /// Top right.
    #[default]
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right.
    BottomRight,
}

/// Bars for sparklines, from low to high.
const BARS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// A small box in a corner showing frames and updates per second, the frame time and a
/// sparkline of recent frame times. It does not measure anything: pass the numbers in,
/// for example `ctx.fps()` and `ctx.ups()` of the runtime.
///
/// ```
/// use std::time::Duration;
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{FpsOverlay, Widget};
///
/// let mut buffer = Buffer::empty(Size::new(30, 6));
/// FpsOverlay::new(59.9)
///     .ups(60.0)
///     .frame_time(Duration::from_micros(16_700))
///     .render(Rect::new(0, 0, 30, 6), &mut buffer);
/// let row: String = (0..30).map(|x| buffer.cell(x, 1).unwrap().symbol()).collect();
/// assert!(row.ends_with("│FPS    59.9│"));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct FpsOverlay {
    fps: f64,
    ups: Option<f64>,
    frame_time: Option<Duration>,
    history: Vec<f32>,
    sparkline_width: u16,
    corner: Corner,
    style: Style,
    ascii: bool,
}

impl FpsOverlay {
    /// An overlay showing `fps`.
    #[must_use]
    pub fn new(fps: f64) -> Self {
        Self {
            fps,
            ups: None,
            frame_time: None,
            history: Vec::new(),
            sparkline_width: 12,
            corner: Corner::TopRight,
            style: Style::new(),
            ascii: false,
        }
    }

    /// Also shows updates per second.
    #[must_use]
    pub fn ups(mut self, ups: f64) -> Self {
        self.ups = Some(ups);
        self
    }

    /// Also shows the time of the last frame.
    #[must_use]
    pub fn frame_time(mut self, frame_time: Duration) -> Self {
        self.frame_time = Some(frame_time);
        self
    }

    /// Shows a sparkline of recent values (oldest first), for example frame times in
    /// milliseconds. Only the last [`FpsOverlay::sparkline_width`] values are drawn.
    #[must_use]
    pub fn history(mut self, values: impl IntoIterator<Item = f32>) -> Self {
        self.history = values.into_iter().collect();
        self
    }

    /// Sets how many columns the sparkline uses (12 by default).
    #[must_use]
    pub fn sparkline_width(mut self, width: u16) -> Self {
        self.sparkline_width = width;
        self
    }

    /// Sets the corner of the area where the overlay is drawn.
    #[must_use]
    pub fn corner(mut self, corner: Corner) -> Self {
        self.corner = corner;
        self
    }

    /// Sets the style patch of the box.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Draws with ASCII borders and a digit sparkline (`0`–`7`), for fonts without
    /// box-drawing or block characters.
    #[must_use]
    pub fn ascii(mut self, ascii: bool) -> Self {
        self.ascii = ascii;
        self
    }

    fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!("FPS {:>7.1}", self.fps)];
        if let Some(ups) = self.ups {
            lines.push(format!("UPS {ups:>7.1}"));
        }
        if let Some(time) = self.frame_time {
            lines.push(format!("{:>8.1} ms", time.as_secs_f64() * 1000.0));
        }
        let count = usize::from(self.sparkline_width);
        let recent = &self.history[self.history.len().saturating_sub(count)..];
        if !recent.is_empty() {
            lines.push(sparkline(recent, self.ascii));
        }
        lines
    }
}

/// One bar per value, scaled to the largest value.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the level is clamped to 0..=7"
)]
fn sparkline(values: &[f32], ascii: bool) -> String {
    let max = values.iter().copied().fold(0.0_f32, f32::max);
    values
        .iter()
        .map(|&value| {
            let level = if max > 0.0 && value.is_finite() {
                ((value / max) * 7.0).round().clamp(0.0, 7.0) as usize
            } else {
                0
            };
            if ascii {
                char::from(b'0' + level as u8).to_string()
            } else {
                BARS[level].to_owned()
            }
        })
        .collect()
}

impl Widget for FpsOverlay {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let lines = self.lines();
        let content_width = lines
            .iter()
            .map(|line| Line::raw(line.as_str()).width())
            .max()
            .unwrap_or(0);
        let width = u16::try_from(content_width + 2)
            .unwrap_or(u16::MAX)
            .min(area.width);
        let height = u16::try_from(lines.len() + 2)
            .unwrap_or(u16::MAX)
            .min(area.height);
        let x = match self.corner {
            Corner::TopLeft | Corner::BottomLeft => area.left(),
            Corner::TopRight | Corner::BottomRight => area.right() - width,
        };
        let y = match self.corner {
            Corner::TopLeft | Corner::TopRight => area.top(),
            Corner::BottomLeft | Corner::BottomRight => area.bottom() - height,
        };
        let rect = Rect::new(x, y, width, height);
        Clear.render(rect, buffer);
        let border = if self.ascii {
            BorderType::Ascii
        } else {
            BorderType::Rounded
        };
        let text = Text::from(lines.into_iter().map(Line::from).collect::<Vec<_>>());
        Paragraph::new(text)
            .ellipsis(false)
            .block(Block::bordered().border_type(border).style(self.style))
            .render(rect, buffer);
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Color, Size};

    use super::*;
    use crate::test_support::rows;

    #[test]
    fn clear_resets_cells() {
        let mut buffer = Buffer::empty(Size::new(3, 1));
        buffer.set_string(0, 0, "abc", Style::new().bg(Color::Red));
        Clear.render(Rect::new(1, 0, 1, 1), &mut buffer);
        assert_eq!(rows(&buffer), ["a c"]);
        assert_eq!(buffer.cell(1, 0).unwrap().bg, Color::Reset);
    }

    #[test]
    fn overlay_in_every_corner() {
        let overlay = FpsOverlay::new(144.0)
            .ups(60.0)
            .history([1.0, 2.0, 4.0, 8.0]);
        let mut buffer = Buffer::empty(Size::new(16, 7));
        overlay
            .clone()
            .corner(Corner::BottomLeft)
            .render(Rect::new(0, 0, 16, 7), &mut buffer);
        assert_eq!(
            rows(&buffer),
            [
                "                ",
                "                ",
                "╭───────────╮   ",
                "│FPS   144.0│   ",
                "│UPS    60.0│   ",
                "│▂▃▅█       │   ",
                "╰───────────╯   ",
            ]
        );
        for corner in [Corner::TopLeft, Corner::TopRight, Corner::BottomRight] {
            let mut buffer = Buffer::empty(Size::new(16, 7));
            overlay
                .clone()
                .corner(corner)
                .render(Rect::new(0, 0, 16, 7), &mut buffer);
            let (x, y) = match corner {
                Corner::TopLeft => (0, 0),
                Corner::TopRight => (3, 0),
                _ => (3, 2),
            };
            assert_eq!(buffer.cell(x, y).unwrap().symbol(), "╭", "{corner:?}");
        }
    }

    #[test]
    fn sparkline_scales_to_the_largest_value() {
        assert_eq!(sparkline(&[0.0, 3.5, 7.0], false), "▁▅█");
        assert_eq!(sparkline(&[0.0, 0.0], false), "▁▁");
        assert_eq!(sparkline(&[1.0, f32::NAN, 2.0], true), "407");
        let overlay = FpsOverlay::new(1.0)
            .sparkline_width(2)
            .history([9.0, 1.0, 2.0]);
        assert_eq!(overlay.lines().last().unwrap(), "▅█");
    }

    #[test]
    fn small_areas_clip_the_overlay() {
        let mut buffer = Buffer::empty(Size::new(5, 2));
        FpsOverlay::new(60.0)
            .frame_time(Duration::from_millis(16))
            .ascii(true)
            .render(Rect::new(0, 0, 5, 2), &mut buffer);
        assert_eq!(rows(&buffer), ["+---+", "+---+"]);
    }
}
