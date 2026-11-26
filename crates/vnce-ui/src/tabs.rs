//! A row of tab titles: [`Tabs`], for screens with pages (settings, help, inventory).

use std::borrow::Cow;

use vnce_core::{Buffer, Position, Rect, Style};

use crate::Widget;
use crate::block::Block;
use crate::paragraph::grapheme_width;
use crate::text::{Line, put_str};

/// A row of titles, one of them selected. The game keeps the selected index (change it
/// with the arrow keys, or with a click through [`Tabs::tab_at`]).
///
/// ```
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{Tabs, Widget};
///
/// let tabs = Tabs::new(["Sound", "Video", "Keys"]).select(1);
/// let mut buffer = Buffer::empty(Size::new(22, 1));
/// tabs.render(Rect::new(0, 0, 22, 1), &mut buffer);
/// let row: String = (0..22).map(|x| buffer.cell(x, 0).unwrap().symbol()).collect();
/// assert_eq!(row, " Sound │ Video │ Keys ");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tabs<'a> {
    titles: Vec<Line<'a>>,
    selected: usize,
    style: Style,
    highlight_style: Style,
    divider: Cow<'a, str>,
    padding: u16,
    block: Option<Block<'a>>,
}

impl<'a> Tabs<'a> {
    /// Tabs with these titles; the first is selected and reversed.
    pub fn new<I>(titles: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Line<'a>>,
    {
        Self {
            titles: titles.into_iter().map(Into::into).collect(),
            selected: 0,
            style: Style::new(),
            highlight_style: Style::new().reverse(),
            divider: Cow::Borrowed("│"),
            padding: 1,
            block: None,
        }
    }

    /// Selects the tab at `index`.
    #[must_use]
    pub fn select(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    /// Number of tabs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.titles.len()
    }

    /// Returns `true` if there are no tabs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.titles.is_empty()
    }

    /// Sets the style patch of the whole row.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the style patch of the selected title (with its padding).
    #[must_use]
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Sets the text between titles (`│` by default).
    #[must_use]
    pub fn divider(mut self, divider: impl Into<Cow<'a, str>>) -> Self {
        self.divider = divider.into();
        self
    }

    /// Sets the empty cells on each side of a title (1 by default).
    #[must_use]
    pub fn padding(mut self, padding: u16) -> Self {
        self.padding = padding;
        self
    }

    /// Draws the tabs inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// The cells each title takes in `area` (padding included), left to right; titles
    /// that do not fit are left out.
    fn spans(&self, area: Rect) -> Vec<Rect> {
        let area = self.block.as_ref().map_or(area, |block| block.inner(area));
        if area.is_empty() {
            return Vec::new();
        }
        let divider = crate::text::visible_graphemes(&self.divider)
            .map(grapheme_width)
            .fold(0, u16::saturating_add);
        let mut spans = Vec::new();
        let mut x = area.left();
        for (index, title) in self.titles.iter().enumerate() {
            if index > 0 {
                x = x.saturating_add(divider);
            }
            let width = u16::try_from(title.width())
                .unwrap_or(u16::MAX)
                .saturating_add(self.padding * 2);
            if x.saturating_add(width) > area.right() {
                break;
            }
            spans.push(Rect::new(x, area.top(), width, 1));
            x += width;
        }
        spans
    }

    /// The tab under `position` when the tabs are drawn in `area`, for mouse clicks.
    #[must_use]
    pub fn tab_at(&self, area: Rect, position: Position) -> Option<usize> {
        self.spans(area)
            .iter()
            .position(|span| span.contains(position))
    }
}

impl Widget for Tabs<'_> {
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
        for (index, span) in self.spans(area).into_iter().enumerate() {
            if index > 0 {
                let divider_x = span.left().saturating_sub(
                    crate::text::visible_graphemes(&self.divider)
                        .map(grapheme_width)
                        .fold(0, u16::saturating_add),
                );
                put_str(
                    buffer,
                    divider_x,
                    span.top(),
                    span.left(),
                    &self.divider,
                    Style::new(),
                );
            }
            let mut x = span.left() + self.padding;
            for (grapheme, style) in self.titles[index].graphemes(Style::new()) {
                let w = grapheme_width(grapheme);
                if x.saturating_add(w) > span.right() {
                    break;
                }
                buffer.set_symbol(x, span.top(), grapheme, style);
                x += w;
            }
            if index == self.selected {
                buffer.set_style(span, self.highlight_style);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Modifiers, Size};

    use super::*;
    use crate::test_support::rows;

    #[test]
    fn titles_dividers_and_the_selection() {
        let tabs = Tabs::new(["A", "Bb", "C"]).select(1).divider("|");
        let mut buffer = Buffer::empty(Size::new(12, 1));
        tabs.render(Rect::new(0, 0, 12, 1), &mut buffer);
        assert_eq!(rows(&buffer), [" A | Bb | C "]);
        let reversed: Vec<u16> = (0..12)
            .filter(|&x| {
                buffer
                    .cell(x, 0)
                    .unwrap()
                    .modifiers
                    .contains(Modifiers::REVERSE)
            })
            .collect();
        assert_eq!(reversed, [4, 5, 6, 7]);
        assert_eq!(tabs.len(), 3);
    }

    #[test]
    fn hit_testing_and_clipping() {
        let tabs = Tabs::new(["One", "Two", "Three"]).padding(0);
        let area = Rect::new(0, 0, 9, 1);
        // "One│Two│" fits, "Three" does not.
        assert_eq!(tabs.tab_at(area, Position::new(1, 0)), Some(0));
        assert_eq!(tabs.tab_at(area, Position::new(3, 0)), None, "the divider");
        assert_eq!(tabs.tab_at(area, Position::new(5, 0)), Some(1));
        assert_eq!(tabs.tab_at(area, Position::new(8, 0)), None);
        let mut buffer = Buffer::empty(Size::new(9, 1));
        tabs.render(area, &mut buffer);
        assert_eq!(rows(&buffer), ["One│Two  "]);
        let boxed = Tabs::new(["X"]).block(Block::bordered());
        let area = Rect::new(0, 0, 5, 3);
        assert_eq!(boxed.tab_at(area, Position::new(2, 1)), Some(0));
        assert!(Tabs::new(Vec::<&str>::new()).is_empty());
    }
}
