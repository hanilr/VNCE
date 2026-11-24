//! Progress bars: [`Gauge`].

use vnce_core::{Buffer, Rect, Style};

use crate::Widget;
use crate::block::Block;
use crate::paragraph::grapheme_width;
use crate::text::{Alignment, Span, visible_graphemes};

/// Partial blocks for 1/8 to 7/8 of a cell.
const EIGHTHS: [&str; 7] = ["▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/// A horizontal progress bar with 1/8-cell precision and a centered label.
///
/// The filled part uses the foreground color of [`Gauge::gauge_style`], the rest its
/// background color. The label is drawn in reverse colors over the filled part, so it is
/// readable everywhere.
///
/// ```
/// use vnce_core::{Buffer, Color, Rect, Size, Style};
/// use vnce_ui::{Gauge, Widget};
///
/// let mut buffer = Buffer::empty(Size::new(10, 1));
/// Gauge::new()
///     .ratio(0.55)
///     .gauge_style(Style::new().fg(Color::Green).bg(Color::rgb(40, 40, 40)))
///     .render(Rect::new(0, 0, 10, 1), &mut buffer);
/// let row: String = (0..10).map(|x| buffer.cell(x, 0).unwrap().symbol()).collect();
/// assert_eq!(row, "███55%    ");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Gauge<'a> {
    ratio: f64,
    label: Option<Span<'a>>,
    show_label: bool,
    bar_style: Style,
    style: Style,
    ascii: bool,
    block: Option<Block<'a>>,
}

impl Default for Gauge<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Gauge<'a> {
    /// An empty gauge with a percentage label.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ratio: 0.0,
            label: None,
            show_label: true,
            bar_style: Style::new(),
            style: Style::new(),
            ascii: false,
            block: None,
        }
    }

    /// Sets how full the gauge is, from `0.0` to `1.0` (clamped; NaN is empty).
    #[must_use]
    pub fn ratio(mut self, ratio: f64) -> Self {
        self.ratio = if ratio.is_nan() {
            0.0
        } else {
            ratio.clamp(0.0, 1.0)
        };
        self
    }

    /// Sets how full the gauge is, in percent (`0..=100`).
    #[must_use]
    pub fn percent(self, percent: u16) -> Self {
        self.ratio(f64::from(percent) / 100.0)
    }

    /// Replaces the percentage label.
    #[must_use]
    pub fn label(mut self, label: impl Into<Span<'a>>) -> Self {
        self.label = Some(label.into());
        self.show_label = true;
        self
    }

    /// Hides the label.
    #[must_use]
    pub fn hide_label(mut self) -> Self {
        self.show_label = false;
        self
    }

    /// Sets the bar colors: foreground for the filled part, background for the rest.
    #[must_use]
    pub fn gauge_style(mut self, style: Style) -> Self {
        self.bar_style = style;
        self
    }

    /// Sets the style patch of the whole area (also around the bar inside a block).
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Draws with `#` and `-` only, for fonts without block characters (whole cells).
    #[must_use]
    pub fn ascii(mut self, ascii: bool) -> Self {
        self.ascii = ascii;
        self
    }

    /// Draws the gauge inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Filled width in eighths of a cell (whole cells in ASCII mode).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the ratio is clamped to 0..=1, so the result fits in the width"
    )]
    fn filled_eighths(&self, width: u16) -> u32 {
        let eighths = (self.ratio * f64::from(width) * 8.0).round() as u32;
        if self.ascii {
            (eighths + 4) / 8 * 8
        } else {
            eighths
        }
    }
}

impl Widget for Gauge<'_> {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let mut area = area.intersection(buffer.area());
        if let Some(block) = &self.block {
            block.render(area, buffer);
            area = block.inner(area);
        }
        if area.is_empty() {
            return;
        }
        buffer.set_style(area, self.style);
        let eighths = self.filled_eighths(area.width);
        let full = u16::try_from(eighths / 8).unwrap_or(u16::MAX);
        let partial = (eighths % 8) as usize;
        let (filled, empty) = if self.ascii { ("#", "-") } else { ("█", " ") };
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let column = x - area.left();
                let symbol = if column < full {
                    filled
                } else if column == full && partial > 0 {
                    EIGHTHS[partial - 1]
                } else {
                    empty
                };
                buffer.set_symbol(x, y, symbol, self.bar_style);
            }
        }
        if !self.show_label {
            return;
        }
        let percent;
        let label = if let Some(label) = &self.label {
            label
        } else {
            percent = Span::raw(format!("{}%", (self.ratio * 100.0).round()));
            &percent
        };
        let width = u16::try_from(label.width()).unwrap_or(u16::MAX);
        let y = area.top() + (area.height - 1) / 2;
        let mut x = area.left() + Alignment::Center.offset(width, area.width);
        let base = self.bar_style.patch(label.style);
        for grapheme in visible_graphemes(&label.content) {
            let w = grapheme_width(grapheme);
            if x.saturating_add(w) > area.right() {
                break;
            }
            // Over the filled part (at least half a cell), reverse the colors.
            let column = u32::from(x - area.left());
            let over_fill = column * 8 + 4 <= eighths;
            let style = if over_fill { base.reverse() } else { base };
            buffer.set_symbol(x, y, grapheme, style);
            x += w;
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Color, Modifiers, Size};

    use super::*;
    use crate::test_support::rows;

    fn render(gauge: &Gauge<'_>, width: u16, height: u16) -> Vec<String> {
        let mut buffer = Buffer::empty(Size::new(width, height));
        gauge.render(Rect::new(0, 0, width, height), &mut buffer);
        rows(&buffer)
    }

    #[test]
    fn fills_in_eighths() {
        let bar = |ratio| render(&Gauge::new().ratio(ratio).hide_label(), 4, 1)[0].clone();
        assert_eq!(bar(0.0), "    ");
        assert_eq!(bar(1.0 / 32.0), "▏   ");
        assert_eq!(bar(0.5), "██  ");
        assert_eq!(bar(0.6), "██▍ ");
        assert_eq!(bar(1.0), "████");
        assert_eq!(bar(7.0), "████");
        assert_eq!(bar(f64::NAN), "    ");
    }

    #[test]
    fn ascii_mode_uses_whole_cells() {
        let gauge = Gauge::new().ratio(0.6).hide_label().ascii(true);
        assert_eq!(render(&gauge, 5, 1), ["###--"]);
    }

    #[test]
    fn label_is_centered_and_reversed_over_the_fill() {
        let style = Style::new().fg(Color::Green).bg(Color::Black);
        let gauge = Gauge::new().percent(50).gauge_style(style);
        let mut buffer = Buffer::empty(Size::new(8, 3));
        gauge.render(Rect::new(0, 0, 8, 3), &mut buffer);
        assert_eq!(rows(&buffer), ["████    ", "██50%   ", "████    "]);
        // "5" is over the fill, "0" is not.
        assert!(
            buffer
                .cell(3, 1)
                .unwrap()
                .modifiers
                .contains(Modifiers::REVERSE)
        );
        assert!(
            !buffer
                .cell(4, 1)
                .unwrap()
                .modifiers
                .contains(Modifiers::REVERSE)
        );
        let custom = Gauge::new().ratio(0.0).label("HP");
        assert_eq!(render(&custom, 6, 1), ["  HP  "]);
    }
}
