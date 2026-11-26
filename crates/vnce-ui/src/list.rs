//! Selectable rows for menus: [`List`] and [`ListState`].

use std::borrow::Cow;

use vnce_core::{Buffer, Position, Rect, Style};

use crate::block::Block;
use crate::paragraph::{grapheme_width, truncate};
use crate::text::{Line, visible_graphemes};
use crate::{StatefulWidget, Widget as _};

/// The selected row and the scroll position of a [`List`], kept between frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ListState {
    selected: Option<usize>,
    offset: usize,
}

impl ListState {
    /// A state with `selected` as the selected row.
    #[must_use]
    pub fn with_selected(selected: Option<usize>) -> Self {
        Self {
            selected,
            offset: 0,
        }
    }

    /// The selected row. Clamped to the last row when the list is drawn.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Selects a row (`None` for no selection).
    pub fn select(&mut self, selected: Option<usize>) {
        self.selected = selected;
    }

    /// The first visible row.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Scrolls so that `offset` is the first visible row (drawing still keeps the
    /// selected row visible).
    pub fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }

    /// Selects the next row of a list with `len` rows; wraps around to the first row when
    /// `wrap` is set. Selects the first row when nothing is selected.
    pub fn select_next(&mut self, len: usize, wrap: bool) {
        if len == 0 {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            None => 0,
            Some(index) if index + 1 >= len => {
                if wrap {
                    0
                } else {
                    len - 1
                }
            }
            Some(index) => index + 1,
        });
    }

    /// Selects the previous row of a list with `len` rows; wraps around to the last row
    /// when `wrap` is set. Selects the last row when nothing is selected.
    pub fn select_previous(&mut self, len: usize, wrap: bool) {
        if len == 0 {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            None => len - 1,
            Some(0) => {
                if wrap {
                    len - 1
                } else {
                    0
                }
            }
            Some(index) => (index - 1).min(len - 1),
        });
    }
}

/// A list of rows, one of which can be selected, for menus and settings screens.
///
/// The list scrolls so that the selected row stays visible. Long rows end with `…`.
///
/// ```
/// use vnce_core::{Buffer, Rect, Size, Style};
/// use vnce_ui::{List, ListState, StatefulWidget};
///
/// let list = List::new(["New game", "Options", "Quit"]).highlight_symbol("> ");
/// let mut state = ListState::default();
/// state.select_next(list.len(), true);
/// state.select_next(list.len(), true);
///
/// let mut buffer = Buffer::empty(Size::new(10, 3));
/// list.render(Rect::new(0, 0, 10, 3), &mut buffer, &mut state);
/// let row: String = (0..10).map(|x| buffer.cell(x, 1).unwrap().symbol()).collect();
/// assert_eq!(row, "> Options ");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct List<'a> {
    items: Vec<Line<'a>>,
    style: Style,
    highlight_style: Style,
    highlight_symbol: Cow<'a, str>,
    block: Option<Block<'a>>,
}

impl<'a> List<'a> {
    /// A list of rows. The selected row is reversed by default.
    pub fn new<I>(items: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Line<'a>>,
    {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            style: Style::new(),
            highlight_style: Style::new().reverse(),
            highlight_symbol: Cow::Borrowed(""),
            block: None,
        }
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the list has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Sets the style patch of the whole area.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the style patch of the selected row.
    #[must_use]
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Sets a symbol drawn before the selected row, for example `"> "`. Every row is
    /// indented by its width so the rows stay aligned.
    #[must_use]
    pub fn highlight_symbol(mut self, symbol: impl Into<Cow<'a, str>>) -> Self {
        self.highlight_symbol = symbol.into();
        self
    }

    /// Draws the list inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// The row under `position` when the list is drawn in `area` with `state` (as last
    /// drawn), for mouse clicks and hovering.
    ///
    /// ```
    /// use vnce_core::{Buffer, Position, Rect, Size};
    /// use vnce_ui::{List, ListState, StatefulWidget};
    ///
    /// let list = List::new(["New game", "Options", "Quit"]);
    /// let area = Rect::new(2, 1, 10, 3);
    /// let mut state = ListState::default();
    /// list.render(area, &mut Buffer::empty(Size::new(20, 5)), &mut state);
    /// assert_eq!(list.item_at(area, &state, Position::new(5, 2)), Some(1));
    /// assert_eq!(list.item_at(area, &state, Position::new(0, 2)), None);
    /// ```
    #[must_use]
    pub fn item_at(&self, area: Rect, state: &ListState, position: Position) -> Option<usize> {
        let area = self.block.as_ref().map_or(area, |block| block.inner(area));
        if !area.contains(position) {
            return None;
        }
        let index = state.offset + usize::from(position.y - area.top());
        (index < self.items.len()).then_some(index)
    }
}

impl StatefulWidget for List<'_> {
    type State = ListState;

    fn render(&self, area: Rect, buffer: &mut Buffer, state: &mut ListState) {
        let mut area = area.intersection(buffer.area());
        if let Some(block) = &self.block {
            block.render(area, buffer);
            area = block.inner(area);
        }
        state.selected = state
            .selected
            .filter(|_| !self.items.is_empty())
            .map(|index| index.min(self.items.len() - 1));
        if area.is_empty() {
            return;
        }
        buffer.set_style(area, self.style);

        // Keep the selected row visible.
        let height = usize::from(area.height);
        if let Some(selected) = state.selected {
            if selected < state.offset {
                state.offset = selected;
            } else if selected >= state.offset + height {
                state.offset = selected + 1 - height;
            }
        }
        state.offset = state.offset.min(self.items.len().saturating_sub(height));

        let symbol_width: u16 = visible_graphemes(&self.highlight_symbol)
            .map(grapheme_width)
            .fold(0, u16::saturating_add);
        let text_x = area.left().saturating_add(symbol_width).min(area.right());
        let text_width = area.right() - text_x;
        for (row, (index, item)) in self
            .items
            .iter()
            .enumerate()
            .skip(state.offset)
            .take(height)
            .enumerate()
        {
            let y = area.top() + u16::try_from(row).unwrap_or(u16::MAX);
            let selected = state.selected == Some(index);
            if selected {
                let mut x = area.left();
                for grapheme in visible_graphemes(&self.highlight_symbol) {
                    let w = grapheme_width(grapheme);
                    if x.saturating_add(w) > area.right() {
                        break;
                    }
                    buffer.set_symbol(x, y, grapheme, Style::new());
                    x += w;
                }
            }
            let mut graphemes: Vec<(&str, Style)> = item.graphemes(Style::new()).collect();
            truncate(&mut graphemes, text_width);
            let mut x = text_x;
            for (grapheme, style) in graphemes {
                let w = grapheme_width(grapheme);
                if x.saturating_add(w) > area.right() {
                    break;
                }
                buffer.set_symbol(x, y, grapheme, style);
                x += w;
            }
            if selected {
                buffer.set_style(
                    Rect::new(area.left(), y, area.width, 1),
                    self.highlight_style,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Modifiers, Size};

    use super::*;
    use crate::test_support::rows;

    fn render(list: &List<'_>, state: &mut ListState, width: u16, height: u16) -> Vec<String> {
        let mut buffer = Buffer::empty(Size::new(width, height));
        StatefulWidget::render(list, Rect::new(0, 0, width, height), &mut buffer, state);
        rows(&buffer)
    }

    #[test]
    fn selection_helpers() {
        let mut state = ListState::default();
        state.select_next(3, false);
        assert_eq!(state.selected(), Some(0));
        state.select_previous(3, true);
        assert_eq!(state.selected(), Some(2));
        state.select_next(3, false);
        assert_eq!(state.selected(), Some(2));
        state.select_next(3, true);
        assert_eq!(state.selected(), Some(0));
        state.select_previous(3, false);
        assert_eq!(state.selected(), Some(0));
        state.select(None);
        state.select_previous(3, false);
        assert_eq!(state.selected(), Some(2));
        state.select_next(0, true);
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn scrolls_to_keep_the_selection_visible() {
        let list = List::new(["a", "b", "c", "d", "e"]).highlight_symbol(">");
        let mut state = ListState::with_selected(Some(3));
        assert_eq!(render(&list, &mut state, 3, 2), [" c ", ">d "]);
        assert_eq!(state.offset(), 2);
        state.select(Some(0));
        assert_eq!(render(&list, &mut state, 3, 2), [">a ", " b "]);
        // Out-of-range selections are clamped to the last row.
        state.select(Some(99));
        assert_eq!(render(&list, &mut state, 3, 2), [" d ", ">e "]);
        assert_eq!(state.selected(), Some(4));
    }

    #[test]
    fn highlight_style_and_truncation() {
        let list = List::new(["short", "a long item"]);
        let mut state = ListState::with_selected(Some(0));
        let mut buffer = Buffer::empty(Size::new(6, 2));
        StatefulWidget::render(&list, Rect::new(0, 0, 6, 2), &mut buffer, &mut state);
        assert_eq!(rows(&buffer), ["short ", "a lon…"]);
        // The whole selected row is highlighted, the others are not.
        assert!(
            buffer
                .cell(5, 0)
                .unwrap()
                .modifiers
                .contains(Modifiers::REVERSE)
        );
        assert!(
            !buffer
                .cell(0, 1)
                .unwrap()
                .modifiers
                .contains(Modifiers::REVERSE)
        );
    }

    #[test]
    fn hit_testing_follows_scrolling_and_blocks() {
        let list = List::new(["a", "b", "c", "d"]).block(Block::bordered());
        let area = Rect::new(0, 0, 5, 4);
        let mut state = ListState::with_selected(Some(3));
        let mut buffer = Buffer::empty(Size::new(5, 4));
        StatefulWidget::render(&list, area, &mut buffer, &mut state);
        assert_eq!(state.offset(), 2);
        assert_eq!(list.item_at(area, &state, Position::new(1, 1)), Some(2));
        assert_eq!(list.item_at(area, &state, Position::new(1, 2)), Some(3));
        assert_eq!(
            list.item_at(area, &state, Position::new(0, 1)),
            None,
            "border"
        );
        state.set_offset(0);
        assert_eq!(list.item_at(area, &state, Position::new(1, 1)), Some(0));
    }

    #[test]
    fn empty_lists_and_blocks() {
        let empty = List::new(Vec::<&str>::new());
        let mut state = ListState::with_selected(Some(2));
        assert_eq!(render(&empty, &mut state, 3, 1), ["   "]);
        assert_eq!(state.selected(), None);
        assert!(empty.is_empty());
        let boxed = List::new(["x"]).block(Block::bordered());
        let mut buffer = Buffer::empty(Size::new(3, 3));
        StatefulWidget::render(
            &boxed,
            Rect::new(0, 0, 3, 3),
            &mut buffer,
            &mut ListState::default(),
        );
        assert_eq!(rows(&buffer), ["┌─┐", "│x│", "└─┘"]);
    }
}
