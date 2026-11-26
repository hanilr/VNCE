//! Rows and columns: [`Table`], for high scores, inventories, save slots and key bindings.

use std::borrow::Cow;

use vnce_core::{Buffer, Position, Rect, Style};

use crate::block::Block;
use crate::layout::{Constraint, Layout};
use crate::list::ListState;
use crate::paragraph::{grapheme_width, truncate};
use crate::text::{Line, put_str, visible_graphemes};
use crate::{StatefulWidget, Widget as _};

/// The selected row and the scroll position of a [`Table`]: the same as a list's.
pub type TableState = ListState;

/// A table: an optional header row and rows of cells in columns of the given widths,
/// one of which can be selected. It scrolls so that the selected row stays visible; long
/// cells end with `…`.
///
/// ```
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{Constraint, StatefulWidget, Table, TableState};
///
/// let table = Table::new(
///     [["Ada", "120"], ["Bo", "95"]],
///     [Constraint::Length(4), Constraint::Length(3)],
/// )
/// .header(["Name", "Pts"]);
/// let mut state = TableState::default();
/// let mut buffer = Buffer::empty(Size::new(8, 3));
/// table.render(Rect::new(0, 0, 8, 3), &mut buffer, &mut state);
/// let row = |y| (0..8).map(|x| buffer.cell(x, y).unwrap().symbol()).collect::<String>();
/// assert_eq!(row(0), "Name Pts");
/// assert_eq!(row(2), "Bo   95 ");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table<'a> {
    rows: Vec<Vec<Line<'a>>>,
    header: Option<Vec<Line<'a>>>,
    widths: Vec<Constraint>,
    column_spacing: u16,
    style: Style,
    header_style: Style,
    highlight_style: Style,
    highlight_symbol: Cow<'a, str>,
    block: Option<Block<'a>>,
}

impl<'a> Table<'a> {
    /// A table of `rows` (each an iterator of cells) in columns of `widths`.
    pub fn new<R, C>(rows: R, widths: impl IntoIterator<Item = Constraint>) -> Self
    where
        R: IntoIterator,
        R::Item: IntoIterator<Item = C>,
        C: Into<Line<'a>>,
    {
        Self {
            rows: rows
                .into_iter()
                .map(|row| row.into_iter().map(Into::into).collect())
                .collect(),
            header: None,
            widths: widths.into_iter().collect(),
            column_spacing: 1,
            style: Style::new(),
            header_style: Style::new().bold(),
            highlight_style: Style::new().reverse(),
            highlight_symbol: Cow::Borrowed(""),
            block: None,
        }
    }

    /// A header row above the rows (bold by default); it does not scroll.
    #[must_use]
    pub fn header<C: Into<Line<'a>>>(mut self, cells: impl IntoIterator<Item = C>) -> Self {
        self.header = Some(cells.into_iter().map(Into::into).collect());
        self
    }

    /// Empty cells between columns (1 by default).
    #[must_use]
    pub fn column_spacing(mut self, spacing: u16) -> Self {
        self.column_spacing = spacing;
        self
    }

    /// Number of rows (without the header).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Returns `true` if the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Sets the style patch of the whole area.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the style patch of the header row.
    #[must_use]
    pub fn header_style(mut self, style: Style) -> Self {
        self.header_style = style;
        self
    }

    /// Sets the style patch of the selected row.
    #[must_use]
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Sets a symbol drawn before the selected row, for example `"> "`. Every row is
    /// indented by its width so the columns stay aligned.
    #[must_use]
    pub fn highlight_symbol(mut self, symbol: impl Into<Cow<'a, str>>) -> Self {
        self.highlight_symbol = symbol.into();
        self
    }

    /// Draws the table inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// The area of the rows inside `area` (without the block, the header and the symbol
    /// column), and the symbol's width.
    fn rows_area(&self, area: Rect) -> (Rect, u16) {
        let mut area = area;
        if let Some(block) = &self.block {
            area = block.inner(area);
        }
        if self.header.is_some() && area.height > 0 {
            area = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
        }
        let symbol = visible_graphemes(&self.highlight_symbol)
            .map(grapheme_width)
            .fold(0, u16::saturating_add)
            .min(area.width);
        (area, symbol)
    }

    /// The row under `position` when the table is drawn in `area` with `state` (as last
    /// drawn), for mouse clicks.
    #[must_use]
    pub fn row_at(&self, area: Rect, state: &TableState, position: Position) -> Option<usize> {
        let (rows, _) = self.rows_area(area);
        if !rows.contains(position) {
            return None;
        }
        let index = state.offset() + usize::from(position.y - rows.top());
        (index < self.rows.len()).then_some(index)
    }

    fn draw_cells(buffer: &mut Buffer, columns: &[Rect], y: u16, cells: &[Line<'_>]) {
        for (column, cell) in columns.iter().zip(cells) {
            let mut graphemes: Vec<(&str, Style)> = cell.graphemes(Style::new()).collect();
            truncate(&mut graphemes, column.width);
            let mut x = column.left()
                + cell.alignment.unwrap_or_default().offset(
                    graphemes.iter().map(|(g, _)| grapheme_width(g)).sum(),
                    column.width,
                );
            for (grapheme, style) in graphemes {
                let w = grapheme_width(grapheme);
                if x.saturating_add(w) > column.right() {
                    break;
                }
                buffer.set_symbol(x, y, grapheme, style);
                x += w;
            }
        }
    }
}

impl StatefulWidget for Table<'_> {
    type State = TableState;

    fn render(&self, area: Rect, buffer: &mut Buffer, state: &mut TableState) {
        let area = area.intersection(buffer.area());
        if let Some(block) = &self.block {
            block.render(area, buffer);
        }
        let (rows, symbol_width) = self.rows_area(area);
        let inner = self.block.as_ref().map_or(area, |b| b.inner(area));
        // Clamp the selection like a list does.
        let len = self.rows.len();
        state.select(state.selected().filter(|_| len > 0).map(|i| i.min(len - 1)));
        if inner.is_empty() {
            return;
        }
        buffer.set_style(inner, self.style);
        let text = Rect::new(
            inner.left() + symbol_width,
            inner.top(),
            inner.width - symbol_width,
            1,
        );
        let columns = Layout::horizontal(self.widths.iter().copied())
            .spacing(self.column_spacing)
            .split(text);

        if let Some(header) = &self.header {
            let y = inner.top();
            Self::draw_cells(buffer, &columns, y, header);
            buffer.set_style(
                Rect::new(inner.left(), y, inner.width, 1),
                self.header_style,
            );
        }
        if rows.is_empty() {
            return;
        }

        // Keep the selected row visible (the list's rule).
        let height = usize::from(rows.height);
        let mut offset = state.offset();
        if let Some(selected) = state.selected() {
            if selected < offset {
                offset = selected;
            } else if selected >= offset + height {
                offset = selected + 1 - height;
            }
        }
        offset = offset.min(len.saturating_sub(height));
        state.set_offset(offset);

        for (row, (index, cells)) in self
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .enumerate()
        {
            let y = rows.top() + u16::try_from(row).unwrap_or(u16::MAX);
            let columns: Vec<Rect> = columns
                .iter()
                .map(|c| Rect::new(c.x, y, c.width, 1))
                .collect();
            Self::draw_cells(buffer, &columns, y, cells);
            if state.selected() == Some(index) {
                put_str(
                    buffer,
                    rows.left(),
                    y,
                    rows.left() + symbol_width,
                    &self.highlight_symbol,
                    Style::new(),
                );
                buffer.set_style(
                    Rect::new(rows.left(), y, rows.width, 1),
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

    fn scores() -> Table<'static> {
        Table::new(
            [["Ada", "120"], ["Bo", "95"], ["Cy", "80"], ["Di", "7"]],
            [Constraint::Length(3), Constraint::Fill(1)],
        )
        .header(["Who", "Pts"])
        .highlight_symbol(">")
    }

    fn render(table: &Table<'_>, state: &mut TableState, width: u16, height: u16) -> Buffer {
        let mut buffer = Buffer::empty(Size::new(width, height));
        table.render(Rect::new(0, 0, width, height), &mut buffer, state);
        buffer
    }

    #[test]
    fn columns_header_and_selection() {
        let mut state = TableState::with_selected(Some(1));
        let b = render(&scores(), &mut state, 8, 3);
        assert_eq!(rows(&b), [" Who Pts", " Ada 120", ">Bo  95 "]);
        assert!(b.cell(1, 0).unwrap().modifiers.contains(Modifiers::BOLD));
        assert!(b.cell(7, 2).unwrap().modifiers.contains(Modifiers::REVERSE));
        assert!(!b.cell(7, 1).unwrap().modifiers.contains(Modifiers::REVERSE));
    }

    #[test]
    fn scrolls_and_hit_tests() {
        let table = scores();
        let mut state = TableState::with_selected(Some(3));
        let b = render(&table, &mut state, 8, 3);
        assert_eq!(rows(&b), [" Who Pts", " Cy  80 ", ">Di  7  "]);
        assert_eq!(state.offset(), 2);
        let area = Rect::new(0, 0, 8, 3);
        assert_eq!(table.row_at(area, &state, Position::new(4, 1)), Some(2));
        assert_eq!(table.row_at(area, &state, Position::new(4, 2)), Some(3));
        assert_eq!(
            table.row_at(area, &state, Position::new(4, 0)),
            None,
            "header"
        );
        assert_eq!(
            table.row_at(area, &state, Position::new(9, 1)),
            None,
            "outside"
        );
        // Out-of-range selections are clamped.
        state.select(Some(99));
        render(&table, &mut state, 8, 3);
        assert_eq!(state.selected(), Some(3));
    }

    #[test]
    fn long_cells_alignment_and_blocks() {
        let table = Table::new(
            [vec![Line::from("abcdef"), Line::from("9").right_aligned()]],
            [Constraint::Length(4), Constraint::Length(3)],
        )
        .block(Block::bordered());
        let b = render(&table, &mut TableState::default(), 10, 3);
        assert_eq!(rows(&b), ["┌────────┐", "│abc…   9│", "└────────┘"]);
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
        let none = Table::new(Vec::<Vec<&str>>::new(), [Constraint::Fill(1)]);
        let mut state = TableState::with_selected(Some(0));
        render(&none, &mut state, 4, 2);
        assert_eq!(state.selected(), None);
    }
}
