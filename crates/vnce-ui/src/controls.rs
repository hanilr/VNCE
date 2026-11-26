//! Small controls for settings screens and menus: [`Slider`] (a value in a range, as a
//! volume), [`Checkbox`] (on or off) and [`Button`] (a labelled box to press).
//!
//! They are plain widgets: the game keeps the values and changes them on keys or clicks
//! ([`Slider::value_at`] turns a click into a value). Slider and checkbox can also be
//! turned into a [`Line`] to put them into a [`List`](crate::List) row.

use std::borrow::Cow;

use vnce_core::{Buffer, Position, Rect, Style};

use crate::Widget;
use crate::block::Block;
use crate::text::{Alignment, Line, Span, put_str};

/// A value in a range shown as a bar with a thumb, for volumes and other amounts.
///
/// ```
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{Slider, Widget};
///
/// let volume = Slider::new(70.0, 0.0, 100.0);
/// let mut buffer = Buffer::empty(Size::new(10, 1));
/// volume.render(Rect::new(0, 0, 10, 1), &mut buffer);
/// let row: String = (0..10).map(|x| buffer.cell(x, 0).unwrap().symbol()).collect();
/// assert_eq!(row, "━━━━━━●───");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Slider<'a> {
    value: f64,
    min: f64,
    max: f64,
    filled: Cow<'a, str>,
    thumb: Cow<'a, str>,
    empty: Cow<'a, str>,
    style: Style,
    filled_style: Style,
    thumb_style: Style,
    show_value: bool,
    block: Option<Block<'a>>,
}

impl<'a> Slider<'a> {
    /// A slider showing `value` between `min` and `max`.
    #[must_use]
    pub fn new(value: f64, min: f64, max: f64) -> Self {
        Self {
            value,
            min,
            max,
            filled: Cow::Borrowed("━"),
            thumb: Cow::Borrowed("●"),
            empty: Cow::Borrowed("─"),
            style: Style::new(),
            filled_style: Style::new(),
            thumb_style: Style::new().bold(),
            show_value: false,
            block: None,
        }
    }

    /// The symbols of the filled part, the thumb and the empty part (one cell each), for
    /// example `("#", "#", "-")` for plain ASCII.
    #[must_use]
    pub fn symbols(
        mut self,
        filled: impl Into<Cow<'a, str>>,
        thumb: impl Into<Cow<'a, str>>,
        empty: impl Into<Cow<'a, str>>,
    ) -> Self {
        self.filled = filled.into();
        self.thumb = thumb.into();
        self.empty = empty.into();
        self
    }

    /// Sets the style patch of the whole slider (and the empty part).
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the style patch of the filled part.
    #[must_use]
    pub fn filled_style(mut self, style: Style) -> Self {
        self.filled_style = style;
        self
    }

    /// Sets the style patch of the thumb.
    #[must_use]
    pub fn thumb_style(mut self, style: Style) -> Self {
        self.thumb_style = style;
        self
    }

    /// Shows the value as a whole percentage of the range after the bar, in five cells
    /// (`  70%`, ` 100%`), so the bar keeps its length while the value changes.
    #[must_use]
    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }

    /// Draws the slider inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// The value as a fraction of the range, in `0..=1`.
    fn ratio(&self) -> f64 {
        let ratio = (self.value - self.min) / (self.max - self.min);
        if ratio.is_nan() {
            0.0
        } else {
            ratio.clamp(0.0, 1.0)
        }
    }

    /// The percentage text, for example `  70%`.
    fn label(&self) -> String {
        format!(" {:>3.0}%", self.ratio() * 100.0)
    }

    /// The column of the thumb in a bar `width` cells wide.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the ratio is clamped to 0..=1, so the result fits in the width"
    )]
    fn thumb_column(&self, width: u16) -> u16 {
        (self.ratio() * f64::from(width.saturating_sub(1))).round() as u16
    }

    /// The bar `width` cells wide as spans (without the value).
    fn bar_spans(&self, width: u16) -> Vec<Span<'a>> {
        if width == 0 {
            return Vec::new();
        }
        let thumb = self.thumb_column(width);
        vec![
            Span::styled(self.filled.repeat(usize::from(thumb)), self.filled_style),
            Span::styled(self.thumb.clone(), self.thumb_style),
            Span::styled(
                self.empty.repeat(usize::from(width - thumb - 1)),
                Style::new(),
            ),
        ]
    }

    /// The slider as a line `width` cells wide (the value included when shown), to put
    /// into a list row: `Line::from(vec![Span::raw("Music  "), ...])`.
    #[must_use]
    pub fn to_line(&self, width: u16) -> Line<'a> {
        let label = self.show_value.then(|| self.label());
        let label_width = label
            .as_ref()
            .map_or(0, |l| u16::try_from(l.len()).unwrap_or(u16::MAX));
        let mut spans = self.bar_spans(width.saturating_sub(label_width));
        if let Some(label) = label {
            spans.push(Span::raw(label));
        }
        Line::from(spans).style(self.style)
    }

    /// The value at column `x` of a slider drawn in `area`, for clicks and drags (the
    /// thumb's column maps back to the value it shows).
    #[must_use]
    pub fn value_at(&self, area: Rect, x: u16) -> f64 {
        let area = self.block.as_ref().map_or(area, |block| block.inner(area));
        let label = if self.show_value {
            u16::try_from(self.label().len()).unwrap_or(u16::MAX)
        } else {
            0
        };
        let width = area.width.saturating_sub(label);
        if width <= 1 {
            return self.min;
        }
        let column = x.saturating_sub(area.left()).min(width - 1);
        self.min + (self.max - self.min) * f64::from(column) / f64::from(width - 1)
    }
}

impl Widget for Slider<'_> {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let area = area.intersection(buffer.area());
        if let Some(block) = &self.block {
            block.render(area, buffer);
        }
        let inner = self.block.as_ref().map_or(area, |block| block.inner(area));
        if inner.is_empty() {
            return;
        }
        buffer.set_style(inner, self.style);
        let y = inner.top() + (inner.height - 1) / 2;
        let mut x = inner.left();
        for span in &self.to_line(inner.width).spans {
            x = put_str(buffer, x, y, inner.right(), &span.content, span.style);
        }
    }
}

/// An on / off switch with a label: `[x] Music` or `[ ] Music`.
///
/// ```
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{Checkbox, Widget};
///
/// let mut buffer = Buffer::empty(Size::new(9, 1));
/// Checkbox::new("Music", true).render(Rect::new(0, 0, 9, 1), &mut buffer);
/// let row: String = (0..9).map(|x| buffer.cell(x, 0).unwrap().symbol()).collect();
/// assert_eq!(row, "[x] Music");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkbox<'a> {
    label: Line<'a>,
    checked: bool,
    on: Cow<'a, str>,
    off: Cow<'a, str>,
    style: Style,
    box_style: Style,
}

impl<'a> Checkbox<'a> {
    /// A checkbox with `label`, checked or not.
    pub fn new(label: impl Into<Line<'a>>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            on: Cow::Borrowed("[x]"),
            off: Cow::Borrowed("[ ]"),
            style: Style::new(),
            box_style: Style::new(),
        }
    }

    /// The symbols when checked and not, for example `("☑", "☐")`.
    #[must_use]
    pub fn symbols(mut self, on: impl Into<Cow<'a, str>>, off: impl Into<Cow<'a, str>>) -> Self {
        self.on = on.into();
        self.off = off.into();
        self
    }

    /// Sets the style patch of the whole checkbox.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the style patch of the box symbol.
    #[must_use]
    pub fn box_style(mut self, style: Style) -> Self {
        self.box_style = style;
        self
    }

    /// The checkbox as a line, to put into a list row.
    #[must_use]
    pub fn to_line(&self) -> Line<'a> {
        let symbol = if self.checked { &self.on } else { &self.off };
        let mut spans = vec![Span::styled(symbol.clone(), self.box_style), Span::raw(" ")];
        spans.extend(self.label.spans.iter().cloned().map(|mut span| {
            span.style = self.label.style.patch(span.style);
            span
        }));
        Line::from(spans).style(self.style)
    }
}

impl Widget for Checkbox<'_> {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let area = area.intersection(buffer.area());
        if area.is_empty() {
            return;
        }
        buffer.set_style(area, self.style);
        let mut x = area.left();
        for span in &self.to_line().spans {
            x = put_str(
                buffer,
                x,
                area.top(),
                area.right(),
                &span.content,
                span.style,
            );
        }
    }
}

/// A labelled button: the label centered in its area, with a style for the focused
/// (selected) button. Clicks hit it where [`Button::contains`] says.
///
/// ```
/// use vnce_core::{Buffer, Position, Rect, Size};
/// use vnce_ui::{Block, Button, Widget};
///
/// let ok = Button::new("OK").block(Block::bordered());
/// let area = Rect::new(0, 0, 6, 3);
/// let mut buffer = Buffer::empty(Size::new(6, 3));
/// ok.render(area, &mut buffer);
/// assert_eq!(buffer.cell(2, 1).unwrap().symbol(), "O");
/// assert!(Button::contains(area, Position::new(5, 2)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button<'a> {
    label: Line<'a>,
    focused: bool,
    style: Style,
    focused_style: Style,
    block: Option<Block<'a>>,
}

impl<'a> Button<'a> {
    /// A button with `label`.
    pub fn new(label: impl Into<Line<'a>>) -> Self {
        Self {
            label: label.into(),
            focused: false,
            style: Style::new(),
            focused_style: Style::new().reverse(),
            block: None,
        }
    }

    /// Whether the button is focused (selected by the keyboard, or under the mouse).
    #[must_use]
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Sets the style patch of the button.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the style patch of the focused button (reversed by default).
    #[must_use]
    pub fn focused_style(mut self, style: Style) -> Self {
        self.focused_style = style;
        self
    }

    /// Draws the button inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Whether `position` is on a button drawn in `area` (its block included).
    #[must_use]
    pub fn contains(area: Rect, position: Position) -> bool {
        area.contains(position)
    }
}

impl Widget for Button<'_> {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let area = area.intersection(buffer.area());
        let style = if self.focused {
            self.style.patch(self.focused_style)
        } else {
            self.style
        };
        buffer.set_style(area, style);
        if let Some(block) = &self.block {
            block.render(area, buffer);
        }
        let inner = self.block.as_ref().map_or(area, |block| block.inner(area));
        if inner.is_empty() {
            return;
        }
        let width = u16::try_from(self.label.width()).unwrap_or(u16::MAX);
        let y = inner.top() + (inner.height - 1) / 2;
        let mut x = inner.left() + Alignment::Center.offset(width, inner.width);
        for (grapheme, grapheme_style) in self.label.graphemes(Style::new()) {
            x = put_str(buffer, x, y, inner.right(), grapheme, grapheme_style);
        }
        if self.focused {
            buffer.set_style(area, self.focused_style);
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Modifiers, Size};

    use super::*;
    use crate::test_support::rows;

    fn draw(widget: &impl Widget, width: u16, height: u16) -> Buffer {
        let mut buffer = Buffer::empty(Size::new(width, height));
        widget.render(Rect::new(0, 0, width, height), &mut buffer);
        buffer
    }

    #[test]
    fn slider_bar_value_and_clicks() {
        let s = |v| rows(&draw(&Slider::new(v, 0.0, 10.0), 5, 1))[0].clone();
        assert_eq!(s(0.0), "●────");
        assert_eq!(s(5.0), "━━●──");
        assert_eq!(s(10.0), "━━━━●");
        assert_eq!(s(99.0), "━━━━●", "clamped");
        assert_eq!(s(f64::NAN), "●────");
        let shown = Slider::new(50.0, 0.0, 100.0).show_value(true);
        assert_eq!(rows(&draw(&shown, 9, 1)), ["━━●─  50%"]);
        let area = Rect::new(2, 0, 10, 1); // a bar of 5 cells
        assert!((shown.value_at(area, 2) - 0.0).abs() < 1e-9);
        assert!((shown.value_at(area, 4) - 50.0).abs() < 1e-9);
        assert!(
            (shown.value_at(area, 20) - 100.0).abs() < 1e-9,
            "past the end"
        );
        let ascii = Slider::new(1.0, 0.0, 2.0).symbols("#", "#", "-");
        assert_eq!(rows(&draw(&ascii, 5, 1)), ["###--"]);
        // As a list row.
        let line = Slider::new(30.0, 0.0, 100.0).to_line(4);
        assert_eq!(line.width(), 4);
    }

    #[test]
    fn checkbox_states_and_lines() {
        assert_eq!(
            rows(&draw(&Checkbox::new("Map", false), 8, 1)),
            ["[ ] Map "]
        );
        let fancy = Checkbox::new("Map", true).symbols("☑", "☐");
        assert_eq!(rows(&draw(&fancy, 6, 1)), ["☑ Map "]);
        assert_eq!(fancy.to_line().width(), 5);
    }

    #[test]
    fn buttons_center_and_focus() {
        let b = draw(&Button::new("Go").block(Block::bordered()), 8, 3);
        assert_eq!(rows(&b), ["┌──────┐", "│  Go  │", "└──────┘"]);
        let focused = draw(&Button::new("Go").focused(true), 6, 1);
        assert!(
            focused
                .cell(0, 0)
                .unwrap()
                .modifiers
                .contains(Modifiers::REVERSE)
        );
        assert!(!Button::contains(
            Rect::new(0, 0, 6, 1),
            Position::new(6, 0)
        ));
    }
}
