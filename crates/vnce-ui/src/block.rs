//! Boxes with borders and titles: [`Block`].

use std::ops::BitOr;

use vnce_core::{Buffer, Rect, Style};

use crate::Widget;
use crate::text::{Alignment, Line};

/// Which sides of a [`Block`] have a border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Borders(u8);

impl Borders {
    /// No border.
    pub const NONE: Self = Self(0);
    /// The top border.
    pub const TOP: Self = Self(1);
    /// The right border.
    pub const RIGHT: Self = Self(1 << 1);
    /// The bottom border.
    pub const BOTTOM: Self = Self(1 << 2);
    /// The left border.
    pub const LEFT: Self = Self(1 << 3);
    /// Every side.
    pub const ALL: Self = Self(0b1111);

    /// Returns `true` if every side of `other` is in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for Borders {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// The characters used for borders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BorderType {
    /// `┌─┐` thin lines.
    #[default]
    Plain,
    /// `╭─╮` thin lines with rounded corners.
    Rounded,
    /// `╔═╗` double lines.
    Double,
    /// `┏━┓` thick lines.
    Thick,
    /// `+-+` ASCII only, for terminals and fonts without box-drawing characters.
    Ascii,
}

/// The symbols of a border set.
struct BorderSymbols {
    horizontal: &'static str,
    vertical: &'static str,
    top_left: &'static str,
    top_right: &'static str,
    bottom_left: &'static str,
    bottom_right: &'static str,
}

impl BorderType {
    const fn symbols(self) -> BorderSymbols {
        let s = match self {
            Self::Plain => ["─", "│", "┌", "┐", "└", "┘"],
            Self::Rounded => ["─", "│", "╭", "╮", "╰", "╯"],
            Self::Double => ["═", "║", "╔", "╗", "╚", "╝"],
            Self::Thick => ["━", "┃", "┏", "┓", "┗", "┛"],
            Self::Ascii => ["-", "|", "+", "+", "+", "+"],
        };
        BorderSymbols {
            horizontal: s[0],
            vertical: s[1],
            top_left: s[2],
            top_right: s[3],
            bottom_left: s[4],
            bottom_right: s[5],
        }
    }
}

/// Empty space inside the borders of a [`Block`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Padding {
    /// Columns on the left.
    pub left: u16,
    /// Columns on the right.
    pub right: u16,
    /// Rows on the top.
    pub top: u16,
    /// Rows on the bottom.
    pub bottom: u16,
}

impl Padding {
    /// The same padding on every side.
    #[must_use]
    pub const fn uniform(value: u16) -> Self {
        Self::new(value, value, value, value)
    }

    /// Padding on the left and right.
    #[must_use]
    pub const fn horizontal(value: u16) -> Self {
        Self::new(value, value, 0, 0)
    }

    /// Padding on the top and bottom.
    #[must_use]
    pub const fn vertical(value: u16) -> Self {
        Self::new(0, 0, value, value)
    }

    /// Padding for each side.
    #[must_use]
    pub const fn new(left: u16, right: u16, top: u16, bottom: u16) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }
}

/// Where a title is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TitlePosition {
    Top,
    Bottom,
}

/// A box with optional borders, titles, padding and a background style.
///
/// Draw the block, then draw the content in [`Block::inner`]:
///
/// ```
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{Block, BorderType, Widget};
///
/// let mut buffer = Buffer::empty(Size::new(10, 3));
/// let area = Rect::new(0, 0, 10, 3);
/// let block = Block::bordered().border_type(BorderType::Rounded).title(" Menu ");
/// block.render(area, &mut buffer);
/// assert_eq!(block.inner(area), Rect::new(1, 1, 8, 1));
/// assert_eq!(buffer.cell(0, 0).unwrap().symbol(), "╭");
/// assert_eq!(buffer.cell(2, 0).unwrap().symbol(), "M");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block<'a> {
    borders: Borders,
    border_type: BorderType,
    border_style: Style,
    style: Style,
    titles: Vec<(TitlePosition, Line<'a>)>,
    padding: Padding,
}

impl<'a> Block<'a> {
    /// A block without borders.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A block with borders on every side.
    #[must_use]
    pub fn bordered() -> Self {
        Self::new().borders(Borders::ALL)
    }

    /// Sets the sides with a border.
    #[must_use]
    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
        self
    }

    /// Sets the border characters.
    #[must_use]
    pub fn border_type(mut self, border_type: BorderType) -> Self {
        self.border_type = border_type;
        self
    }

    /// Sets the style patch of the borders (on top of [`Block::style`]).
    #[must_use]
    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Sets the style patch of the whole area (for example a background color).
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Adds a title on the top edge. Its alignment (left by default) comes from the line,
    /// for example `Line::from("Title").centered()`.
    #[must_use]
    pub fn title(mut self, title: impl Into<Line<'a>>) -> Self {
        self.titles.push((TitlePosition::Top, title.into()));
        self
    }

    /// Adds a title on the bottom edge.
    #[must_use]
    pub fn title_bottom(mut self, title: impl Into<Line<'a>>) -> Self {
        self.titles.push((TitlePosition::Bottom, title.into()));
        self
    }

    /// Sets the padding inside the borders.
    #[must_use]
    pub fn padding(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }

    /// The area left for content inside the borders, titles and padding.
    ///
    /// A title on a side without a border takes a row of its own.
    #[must_use]
    pub fn inner(&self, area: Rect) -> Rect {
        let has_title = |position| self.titles.iter().any(|(p, _)| *p == position);
        let edge = |side, position| u16::from(self.borders.contains(side) || has_title(position));
        let left = u16::from(self.borders.contains(Borders::LEFT)) + self.padding.left;
        let right = u16::from(self.borders.contains(Borders::RIGHT)) + self.padding.right;
        let top = edge(Borders::TOP, TitlePosition::Top) + self.padding.top;
        let bottom = edge(Borders::BOTTOM, TitlePosition::Bottom) + self.padding.bottom;
        let width = area.width.saturating_sub(left.saturating_add(right));
        let height = area.height.saturating_sub(top.saturating_add(bottom));
        if width == 0 || height == 0 {
            return Rect::new(area.x.saturating_add(left).min(area.right()), area.y, 0, 0);
        }
        Rect::new(area.x + left, area.y + top, width, height)
    }

    fn render_borders(&self, area: Rect, buffer: &mut Buffer) {
        let symbols = self.border_type.symbols();
        let style = self.border_style;
        let (left, top) = (area.left(), area.top());
        let (right, bottom) = (area.right() - 1, area.bottom() - 1);
        let has = |side| self.borders.contains(side);
        if has(Borders::TOP) {
            for x in left..=right {
                buffer.set_symbol(x, top, symbols.horizontal, style);
            }
        }
        if has(Borders::BOTTOM) {
            for x in left..=right {
                buffer.set_symbol(x, bottom, symbols.horizontal, style);
            }
        }
        if has(Borders::LEFT) {
            for y in top..=bottom {
                buffer.set_symbol(left, y, symbols.vertical, style);
            }
        }
        if has(Borders::RIGHT) {
            for y in top..=bottom {
                buffer.set_symbol(right, y, symbols.vertical, style);
            }
        }
        let corners = [
            (Borders::TOP | Borders::LEFT, left, top, symbols.top_left),
            (Borders::TOP | Borders::RIGHT, right, top, symbols.top_right),
            (
                Borders::BOTTOM | Borders::LEFT,
                left,
                bottom,
                symbols.bottom_left,
            ),
            (
                Borders::BOTTOM | Borders::RIGHT,
                right,
                bottom,
                symbols.bottom_right,
            ),
        ];
        for (sides, x, y, symbol) in corners {
            if has(sides) {
                buffer.set_symbol(x, y, symbol, style);
            }
        }
    }

    fn render_titles(&self, area: Rect, buffer: &mut Buffer, position: TitlePosition) {
        let row = match position {
            TitlePosition::Top => area.top(),
            TitlePosition::Bottom => area.bottom() - 1,
        };
        // Titles stay between the corners.
        let start = area.left() + u16::from(self.borders.contains(Borders::LEFT));
        let end = area.right() - u16::from(self.borders.contains(Borders::RIGHT));
        if start >= end {
            return;
        }
        let width = end - start;
        let titles = || {
            self.titles
                .iter()
                .filter(move |(p, _)| *p == position)
                .map(|(_, line)| line)
        };
        let by_alignment = |alignment: Alignment| {
            titles().filter(move |line| line.alignment.unwrap_or_default() == alignment)
        };
        let base = self.border_style;
        // Left titles from the left, one space apart.
        let mut x = start;
        for line in by_alignment(Alignment::Left) {
            x = draw_line(buffer, line, x, row, end, base).saturating_add(1);
        }
        // Right titles from the right.
        let mut right_end = end;
        for line in by_alignment(Alignment::Right)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let line_width = clamp_width(line.width());
            let begin = right_end.saturating_sub(line_width).max(start);
            draw_line(buffer, line, begin, row, right_end, base);
            right_end = begin.saturating_sub(1);
        }
        // Centered titles together in the middle.
        let centered: Vec<&Line<'_>> = by_alignment(Alignment::Center).collect();
        if !centered.is_empty() {
            let total =
                centered.iter().map(|line| line.width()).sum::<usize>() + centered.len() - 1;
            let mut x = start + Alignment::Center.offset(clamp_width(total), width);
            for line in centered {
                x = draw_line(buffer, line, x, row, end, base).saturating_add(1);
            }
        }
    }
}

fn clamp_width(width: usize) -> u16 {
    u16::try_from(width).unwrap_or(u16::MAX)
}

/// Draws a line from `x` up to `end` (exclusive) and returns the column after it.
pub(crate) fn draw_line(
    buffer: &mut Buffer,
    line: &Line<'_>,
    mut x: u16,
    y: u16,
    end: u16,
    base: Style,
) -> u16 {
    for (grapheme, style) in line.graphemes(base) {
        let width = clamp_width(unicode_width::UnicodeWidthStr::width(grapheme));
        if x.saturating_add(width) > end {
            break;
        }
        buffer.set_symbol(x, y, grapheme, style);
        x += width;
    }
    x
}

impl Widget for Block<'_> {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        let area = area.intersection(buffer.area());
        if area.is_empty() {
            return;
        }
        buffer.set_style(area, self.style);
        self.render_borders(area, buffer);
        self.render_titles(area, buffer, TitlePosition::Top);
        if area.height > 1 || !self.titles.iter().any(|(p, _)| *p == TitlePosition::Top) {
            self.render_titles(area, buffer, TitlePosition::Bottom);
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Color, Size};

    use super::*;
    use crate::test_support::rows;

    fn render(block: &Block<'_>, width: u16, height: u16) -> Vec<String> {
        let mut buffer = Buffer::empty(Size::new(width, height));
        block.render(Rect::new(0, 0, width, height), &mut buffer);
        rows(&buffer)
    }

    #[test]
    fn border_sets() {
        assert_eq!(render(&Block::bordered(), 4, 3), ["┌──┐", "│  │", "└──┘"]);
        let rounded = Block::bordered().border_type(BorderType::Rounded);
        assert_eq!(render(&rounded, 3, 2), ["╭─╮", "╰─╯"]);
        let double = Block::bordered().border_type(BorderType::Double);
        assert_eq!(render(&double, 3, 3), ["╔═╗", "║ ║", "╚═╝"]);
        let thick = Block::bordered().border_type(BorderType::Thick);
        assert_eq!(render(&thick, 3, 2), ["┏━┓", "┗━┛"]);
        let ascii = Block::bordered().border_type(BorderType::Ascii);
        assert_eq!(render(&ascii, 3, 3), ["+-+", "| |", "+-+"]);
    }

    #[test]
    fn some_sides_only() {
        let block = Block::new().borders(Borders::TOP | Borders::LEFT);
        assert_eq!(render(&block, 4, 3), ["┌───", "│   ", "│   "]);
        assert_eq!(block.inner(Rect::new(0, 0, 4, 3)), Rect::new(1, 1, 3, 2));
        let bottom = Block::new().borders(Borders::BOTTOM);
        assert_eq!(render(&bottom, 3, 2), ["   ", "───"]);
    }

    #[test]
    fn titles_are_aligned_and_clipped() {
        let block = Block::bordered()
            .title("ab")
            .title(Line::from("cd").right_aligned())
            .title(Line::from("mid").centered())
            .title_bottom(Line::from("end").centered());
        assert_eq!(
            render(&block, 13, 3),
            ["┌ab──mid──cd┐", "│           │", "└────end────┘"]
        );
        // Titles never cover the corners.
        let long = Block::bordered().title("a very long title");
        assert_eq!(render(&long, 6, 2), ["┌a ve┐", "└────┘"]);
    }

    #[test]
    fn inner_area_with_titles_and_padding() {
        let area = Rect::new(2, 1, 12, 8);
        assert_eq!(Block::bordered().inner(area), Rect::new(3, 2, 10, 6));
        let padded = Block::bordered().padding(Padding::new(1, 2, 1, 0));
        assert_eq!(padded.inner(area), Rect::new(4, 3, 7, 5));
        // A title without a top border takes a row.
        let titled = Block::new().title("t");
        assert_eq!(titled.inner(area), Rect::new(2, 2, 12, 7));
        assert!(Block::bordered().inner(Rect::new(0, 0, 2, 2)).is_empty());
        assert_eq!(Padding::uniform(2), Padding::new(2, 2, 2, 2));
        assert_eq!(Padding::horizontal(1), Padding::new(1, 1, 0, 0));
        assert_eq!(Padding::vertical(1), Padding::new(0, 0, 1, 1));
    }

    #[test]
    fn styles_are_patches() {
        let mut buffer = Buffer::empty(Size::new(3, 3));
        let block = Block::bordered()
            .style(Style::new().bg(Color::Blue))
            .border_style(Style::new().fg(Color::Yellow));
        block.render(Rect::new(0, 0, 3, 3), &mut buffer);
        let corner = buffer.cell(0, 0).unwrap();
        assert_eq!((corner.fg, corner.bg), (Color::Yellow, Color::Blue));
        let inside = buffer.cell(1, 1).unwrap();
        assert_eq!((inside.fg, inside.bg), (Color::Reset, Color::Blue));
    }
}
