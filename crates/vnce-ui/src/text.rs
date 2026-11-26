//! Styled text: [`Span`], [`Line`] and [`Text`].

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use vnce_core::Style;

/// Horizontal alignment of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Alignment {
    /// Aligned to the left edge.
    #[default]
    Left,
    /// Centered (an odd extra column goes to the right).
    Center,
    /// Aligned to the right edge.
    Right,
}

impl Alignment {
    /// Offset of a row of `width` columns in an area of `available` columns.
    #[must_use]
    pub(crate) fn offset(self, width: u16, available: u16) -> u16 {
        let free = available.saturating_sub(width);
        match self {
            Self::Left => 0,
            Self::Center => free / 2,
            Self::Right => free,
        }
    }
}

/// A piece of text with one style.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Span<'a> {
    /// The text. Control characters are never drawn.
    pub content: Cow<'a, str>,
    /// The style patch applied to the text.
    pub style: Style,
}

impl<'a> Span<'a> {
    /// Unstyled text.
    pub fn raw(content: impl Into<Cow<'a, str>>) -> Self {
        Self {
            content: content.into(),
            style: Style::new(),
        }
    }

    /// Styled text.
    pub fn styled(content: impl Into<Cow<'a, str>>, style: Style) -> Self {
        Self {
            content: content.into(),
            style,
        }
    }

    /// Width in terminal columns.
    #[must_use]
    pub fn width(&self) -> usize {
        visible_graphemes(&self.content)
            .map(UnicodeWidthStr::width)
            .sum()
    }
}

impl<'a> From<&'a str> for Span<'a> {
    fn from(content: &'a str) -> Self {
        Self::raw(content)
    }
}

impl From<String> for Span<'_> {
    fn from(content: String) -> Self {
        Self::raw(content)
    }
}

/// One line of text made of spans.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Line<'a> {
    /// The spans, left to right.
    pub spans: Vec<Span<'a>>,
    /// A style patch for the whole line (span styles are applied on top).
    pub style: Style,
    /// Overrides the alignment of the widget that draws the line.
    pub alignment: Option<Alignment>,
}

impl<'a> Line<'a> {
    /// A line of unstyled text.
    pub fn raw(content: impl Into<Cow<'a, str>>) -> Self {
        Self::from(Span::raw(content))
    }

    /// A line of text in one style.
    pub fn styled(content: impl Into<Cow<'a, str>>, style: Style) -> Self {
        Self::from(Span::styled(content, style))
    }

    /// Sets the style patch of the whole line.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the alignment of this line.
    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = Some(alignment);
        self
    }

    /// Centers this line.
    #[must_use]
    pub fn centered(self) -> Self {
        self.alignment(Alignment::Center)
    }

    /// Aligns this line to the right.
    #[must_use]
    pub fn right_aligned(self) -> Self {
        self.alignment(Alignment::Right)
    }

    /// Width in terminal columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum()
    }

    /// The visible graphemes of the line with their final styles, on top of `base`.
    pub(crate) fn graphemes(&self, base: Style) -> impl Iterator<Item = (&str, Style)> {
        let line_style = base.patch(self.style);
        self.spans.iter().flat_map(move |span| {
            let style = line_style.patch(span.style);
            visible_graphemes(&span.content).map(move |grapheme| (grapheme, style))
        })
    }
}

impl<'a> From<Span<'a>> for Line<'a> {
    fn from(span: Span<'a>) -> Self {
        Self {
            spans: vec![span],
            style: Style::new(),
            alignment: None,
        }
    }
}

impl<'a> From<Vec<Span<'a>>> for Line<'a> {
    fn from(spans: Vec<Span<'a>>) -> Self {
        Self {
            spans,
            style: Style::new(),
            alignment: None,
        }
    }
}

impl<'a> From<&'a str> for Line<'a> {
    fn from(content: &'a str) -> Self {
        Self::raw(content)
    }
}

impl From<String> for Line<'_> {
    fn from(content: String) -> Self {
        Self::raw(content)
    }
}

/// Several lines of text.
///
/// Converting a string splits it on `\n` (a trailing `\r` is removed from each line).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Text<'a> {
    /// The lines, top to bottom.
    pub lines: Vec<Line<'a>>,
    /// A style patch for all lines (line and span styles are applied on top).
    pub style: Style,
}

impl<'a> Text<'a> {
    /// Unstyled text, split into lines.
    pub fn raw(content: impl Into<Cow<'a, str>>) -> Self {
        match content.into() {
            Cow::Borrowed(text) => Self::from_lines(
                text.split('\n')
                    .map(|line| Line::raw(line.strip_suffix('\r').unwrap_or(line))),
            ),
            Cow::Owned(text) => Self::from_lines(
                text.split('\n')
                    .map(|line| Line::raw(line.strip_suffix('\r').unwrap_or(line).to_owned())),
            ),
        }
    }

    /// Text in one style, split into lines.
    pub fn styled(content: impl Into<Cow<'a, str>>, style: Style) -> Self {
        Self {
            style,
            ..Self::raw(content)
        }
    }

    fn from_lines(lines: impl IntoIterator<Item = Line<'a>>) -> Self {
        Self {
            lines: lines.into_iter().collect(),
            style: Style::new(),
        }
    }

    /// Adds a line at the end.
    pub fn push_line(&mut self, line: impl Into<Line<'a>>) {
        self.lines.push(line.into());
    }

    /// Number of lines (without wrapping).
    #[must_use]
    pub fn height(&self) -> usize {
        self.lines.len()
    }

    /// Width of the widest line in terminal columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.lines.iter().map(Line::width).max().unwrap_or(0)
    }
}

impl<'a> From<&'a str> for Text<'a> {
    fn from(content: &'a str) -> Self {
        Self::raw(content)
    }
}

impl From<String> for Text<'_> {
    fn from(content: String) -> Self {
        Self::raw(content)
    }
}

impl<'a> From<Span<'a>> for Text<'a> {
    fn from(span: Span<'a>) -> Self {
        Self::from_lines([Line::from(span)])
    }
}

impl<'a> From<Line<'a>> for Text<'a> {
    fn from(line: Line<'a>) -> Self {
        Self::from_lines([line])
    }
}

impl<'a> From<Vec<Line<'a>>> for Text<'a> {
    fn from(lines: Vec<Line<'a>>) -> Self {
        Self::from_lines(lines)
    }
}

/// Grapheme clusters that take space on the screen (no control characters, no
/// zero-width clusters).
pub(crate) fn visible_graphemes(text: &str) -> impl Iterator<Item = &str> {
    text.graphemes(true)
        .filter(|g| !g.chars().any(char::is_control) && g.width() > 0)
}

/// Writes the visible graphemes of `text` from column `x` of row `y` in `style`, stopping
/// before column `right`; returns the column after the last one written.
pub(crate) fn put_str(
    buffer: &mut vnce_core::Buffer,
    x: u16,
    y: u16,
    right: u16,
    text: &str,
    style: Style,
) -> u16 {
    let mut x = x;
    for grapheme in visible_graphemes(text) {
        let w = crate::paragraph::grapheme_width(grapheme);
        if x.saturating_add(w) > right {
            break;
        }
        buffer.set_symbol(x, y, grapheme, style);
        x += w;
    }
    x
}

#[cfg(test)]
mod tests {
    use vnce_core::Color;

    use super::*;

    #[test]
    fn widths_count_columns() {
        assert_eq!(Span::raw("abc").width(), 3);
        assert_eq!(Span::raw("日本").width(), 4);
        // Combining accent: one column; control characters: none.
        assert_eq!(Span::raw("e\u{301}\t").width(), 1);
        let line = Line::from(vec![Span::raw("ab"), Span::raw("日")]);
        assert_eq!(line.width(), 4);
    }

    #[test]
    fn text_splits_lines() {
        let text = Text::from("one\r\ntwo\n\nfour");
        assert_eq!(text.height(), 4);
        assert_eq!(text.lines[1], Line::raw("two"));
        assert_eq!(text.lines[2].width(), 0);
        assert_eq!(text.width(), 4);
        let owned = Text::from(String::from("a\nbb"));
        assert_eq!(owned.width(), 2);
    }

    #[test]
    fn styles_are_layered() {
        let red = Style::new().fg(Color::Red);
        let bold = Style::new().bold();
        let line = Line::from(vec![Span::raw("a"), Span::styled("b", red)]).style(bold);
        let styles: Vec<Style> = line.graphemes(Style::new()).map(|(_, s)| s).collect();
        assert_eq!(styles[0], bold);
        assert_eq!(styles[1], bold.patch(red));
    }

    #[test]
    fn alignment_offsets() {
        assert_eq!(Alignment::Left.offset(3, 10), 0);
        assert_eq!(Alignment::Center.offset(3, 10), 3);
        assert_eq!(Alignment::Right.offset(3, 10), 7);
        assert_eq!(Alignment::Right.offset(30, 10), 0);
    }
}
