//! A grid of cells.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::cell::Cell;
use crate::geometry::{Position, Rect, Size};
use crate::style::Style;

/// A rectangular grid of [`Cell`]s, stored row by row.
///
/// The buffer keeps wide characters consistent: when text is written over part of a wide
/// character, the remaining half is replaced by a space so that no half-character is ever
/// sent to the terminal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Buffer {
    size: Size,
    cells: Vec<Cell>,
}

/// A cell that changed between two buffers, returned by [`Buffer::diff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellUpdate<'a> {
    /// Column of the cell.
    pub x: u16,
    /// Row of the cell.
    pub y: u16,
    /// The new content of the cell.
    pub cell: &'a Cell,
}

impl Buffer {
    /// Creates a buffer filled with blank cells.
    #[must_use]
    pub fn empty(size: Size) -> Self {
        Self::filled(size, &Cell::EMPTY)
    }

    /// Creates a buffer filled with copies of `cell`.
    #[must_use]
    pub fn filled(size: Size, cell: &Cell) -> Self {
        Self {
            size,
            cells: vec![cell.clone(); size.area()],
        }
    }

    /// The size of the buffer.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// The whole buffer as a rectangle at the origin.
    #[must_use]
    pub fn area(&self) -> Rect {
        Rect::from(self.size)
    }

    /// All cells, row by row.
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Index of the cell at `(x, y)` in [`Buffer::cells`], or `None` if out of bounds.
    #[must_use]
    pub fn index_of(&self, x: u16, y: u16) -> Option<usize> {
        (x < self.size.width && y < self.size.height)
            .then(|| usize::from(y) * usize::from(self.size.width) + usize::from(x))
    }

    /// Position of the cell at `index` in [`Buffer::cells`], or `None` if out of bounds.
    #[must_use]
    pub fn position_of(&self, index: usize) -> Option<Position> {
        if index >= self.cells.len() {
            return None;
        }
        let width = usize::from(self.size.width);
        let x = u16::try_from(index % width).ok()?;
        let y = u16::try_from(index / width).ok()?;
        Some(Position::new(x, y))
    }

    /// The cell at `(x, y)`, or `None` if out of bounds.
    #[must_use]
    pub fn cell(&self, x: u16, y: u16) -> Option<&Cell> {
        self.index_of(x, y).map(|index| &self.cells[index])
    }

    /// Mutable access to the cell at `(x, y)`, or `None` if out of bounds.
    ///
    /// Writing symbols directly does not maintain the wide-character invariant; prefer
    /// [`Buffer::set_string`] or [`Buffer::set_symbol`] for text.
    pub fn cell_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        self.index_of(x, y).map(|index| &mut self.cells[index])
    }

    /// Writes `text` starting at `(x, y)` and returns the column after the last written
    /// character. Text that does not fit in the row is clipped.
    ///
    /// Control characters (such as `\n`, `\t` or `ESC`) are skipped, so text can never
    /// inject escape sequences into the terminal.
    pub fn set_string(&mut self, x: u16, y: u16, text: &str, style: Style) -> u16 {
        let max_width = self.size.width.saturating_sub(x);
        self.set_stringn(x, y, text, max_width, style)
    }

    /// Like [`Buffer::set_string`], but writes at most `max_width` columns.
    pub fn set_stringn(&mut self, x: u16, y: u16, text: &str, max_width: u16, style: Style) -> u16 {
        if y >= self.size.height || x >= self.size.width {
            return x;
        }
        let end = x.saturating_add(max_width).min(self.size.width);
        let mut column = x;
        if text.is_ascii() {
            // Fast path: every ASCII character is its own grapheme, one column wide; ASCII
            // control characters are exactly the ones `char::is_control` skips below.
            for (i, byte) in text.bytes().enumerate() {
                if byte.is_ascii_control() {
                    continue;
                }
                if column >= end {
                    break;
                }
                self.put_grapheme(column, y, &text[i..=i], 1, style);
                column += 1;
            }
            return column;
        }
        for grapheme in text.graphemes(true) {
            if grapheme.chars().any(char::is_control) {
                continue;
            }
            let width = grapheme.width();
            if width == 0 {
                continue;
            }
            let Ok(width) = u16::try_from(width) else {
                break;
            };
            if column.saturating_add(width) > end {
                break;
            }
            self.put_grapheme(column, y, grapheme, width, style);
            column += width;
        }
        column
    }

    /// Writes a single grapheme cluster at `(x, y)`. Returns `false` if it was not written
    /// (out of bounds, does not fit in the row, zero width or a control character).
    pub fn set_symbol(&mut self, x: u16, y: u16, symbol: &str, style: Style) -> bool {
        if symbol.chars().any(char::is_control) || x >= self.size.width {
            return false;
        }
        let Ok(width) = u16::try_from(symbol.width()) else {
            return false;
        };
        if width == 0 || y >= self.size.height || x.saturating_add(width) > self.size.width {
            return false;
        }
        self.put_grapheme(x, y, symbol, width, style);
        true
    }

    /// Applies a style patch to every cell inside `area`.
    pub fn set_style(&mut self, area: Rect, style: Style) {
        let area = area.intersection(self.area());
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = self.cell_mut(x, y) {
                    cell.set_style(style);
                }
            }
        }
    }

    /// Fills every cell inside `area` with a copy of `cell`.
    ///
    /// `cell` should hold a single-width symbol. Wide characters that are cut by the edges
    /// of `area` are replaced by spaces.
    pub fn fill(&mut self, area: Rect, cell: &Cell) {
        let area = area.intersection(self.area());
        if area.is_empty() {
            return;
        }
        for y in area.top()..area.bottom() {
            self.break_wide_at(area.left(), y);
            self.break_wide_at(area.right() - 1, y);
            for x in area.left()..area.right() {
                if let Some(target) = self.cell_mut(x, y) {
                    target.clone_from(cell);
                }
            }
        }
    }

    /// Resets every cell to [`Cell::EMPTY`].
    pub fn clear(&mut self) {
        self.cells.fill(Cell::EMPTY);
    }

    /// Resizes the buffer. All cells are reset to [`Cell::EMPTY`].
    pub fn resize(&mut self, size: Size) {
        self.size = size;
        self.cells.clear();
        self.cells.resize(size.area(), Cell::EMPTY);
    }

    /// Cells of `self` that differ from `previous`, in row-major order.
    ///
    /// Continuation cells of wide characters are never returned (the terminal draws the
    /// whole character from its first cell). When a wide character is replaced, the cells
    /// it used to cover are returned as well. If the sizes differ, every cell except
    /// continuation cells is returned.
    #[must_use]
    pub fn diff<'a>(&'a self, previous: &Self) -> Vec<CellUpdate<'a>> {
        let mut updates = Vec::new();
        if self.size != previous.size {
            for (index, cell) in self.cells.iter().enumerate() {
                if !cell.is_continuation() {
                    self.push_update(&mut updates, index, cell);
                }
            }
            return updates;
        }

        // Number of following cells that must be redrawn because a wide character changed.
        let mut invalidated: usize = 0;
        // Number of following cells covered by the current wide character.
        let mut to_skip: usize = 0;
        for (index, (current, before)) in self.cells.iter().zip(&previous.cells).enumerate() {
            if to_skip == 0 && (current != before || invalidated > 0) && !current.is_continuation()
            {
                self.push_update(&mut updates, index, current);
            }
            let current_width = current.width();
            to_skip = current_width.saturating_sub(1);
            let affected_width = current_width.max(before.width());
            invalidated = affected_width.max(invalidated).saturating_sub(1);
        }
        updates
    }

    fn push_update<'a>(&self, updates: &mut Vec<CellUpdate<'a>>, index: usize, cell: &'a Cell) {
        if let Some(Position { x, y }) = self.position_of(index) {
            updates.push(CellUpdate { x, y, cell });
        }
    }

    /// Writes a grapheme of the given width at `(x, y)`. The caller guarantees that it
    /// fits in the row.
    fn put_grapheme(&mut self, x: u16, y: u16, grapheme: &str, width: u16, style: Style) {
        // Break every wide character that the new grapheme overlaps.
        for offset in 0..width {
            self.break_wide_at(x + offset, y);
        }
        if let Some(cell) = self.cell_mut(x, y) {
            cell.set_symbol(grapheme).set_style(style);
        }
        for offset in 1..width {
            if let Some(cell) = self.cell_mut(x + offset, y) {
                cell.set_continuation();
                cell.set_style(style);
            }
        }
    }

    /// If the cell at `(x, y)` belongs to a wide character, replaces the whole character
    /// (its first cell and all continuation cells) with spaces, keeping their styles.
    fn break_wide_at(&mut self, x: u16, y: u16) {
        let Some(cell) = self.cell(x, y) else {
            return;
        };
        if !cell.is_continuation() && cell.width() <= 1 {
            return;
        }
        // Find the first cell of the wide character.
        let mut start = x;
        while start > 0 && self.cell(start, y).is_some_and(Cell::is_continuation) {
            start -= 1;
        }
        let width = self
            .cell(start, y)
            .map_or(1, |cell| u16::try_from(cell.width()).unwrap_or(1).max(1));
        for column in start..start.saturating_add(width).min(self.size.width) {
            if let Some(cell) = self.cell_mut(column, y) {
                cell.set_symbol(" ");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn row(buffer: &Buffer, y: u16) -> String {
        (0..buffer.size().width)
            .map(|x| buffer.cell(x, y).map_or("?", Cell::symbol))
            .map(|s| if s.is_empty() { "_" } else { s })
            .collect()
    }

    #[test]
    fn set_string_writes_and_clips() {
        let mut buffer = Buffer::empty(Size::new(5, 1));
        let end = buffer.set_string(1, 0, "hello", Style::new().fg(Color::Red));
        assert_eq!(end, 5);
        assert_eq!(row(&buffer, 0), " hell");
        assert_eq!(buffer.cell(1, 0).map(|c| c.fg), Some(Color::Red));
        assert_eq!(buffer.cell(0, 0).map(|c| c.fg), Some(Color::Reset));
    }

    #[test]
    fn set_string_out_of_bounds_is_ignored() {
        let mut buffer = Buffer::empty(Size::new(3, 1));
        assert_eq!(buffer.set_string(5, 0, "x", Style::new()), 5);
        assert_eq!(buffer.set_string(0, 3, "x", Style::new()), 0);
        assert_eq!(buffer, Buffer::empty(Size::new(3, 1)));
    }

    #[test]
    fn set_string_skips_control_characters() {
        let mut buffer = Buffer::empty(Size::new(6, 1));
        buffer.set_string(0, 0, "a\x1b[2Jb\n\tc", Style::new());
        assert_eq!(row(&buffer, 0), "a[2Jbc");
    }

    #[test]
    fn ascii_fast_path_matches_the_grapheme_path() {
        // A leading "é" sends the same text through the grapheme path.
        for text in [
            "a\x1b[2Jb\r\n\tc\x7fd",
            "hello world",
            "",
            "\r\n",
            "x\u{0}y",
        ] {
            for max in [20, 3, 1, 0] {
                let style = Style::new().fg(Color::Red);
                let mut fast = Buffer::empty(Size::new(12, 1));
                fast.set_string(0, 0, "é", style);
                let fast_end = fast.set_stringn(1, 0, text, max, style);
                let mut slow = Buffer::empty(Size::new(12, 1));
                let slow_end = slow.set_stringn(0, 0, &format!("é{text}"), max + 1, style);
                assert_eq!((fast_end, &fast), (slow_end, &slow), "{text:?} {max}");
            }
        }
    }

    #[test]
    fn wide_characters_use_continuation_cells() {
        let mut buffer = Buffer::empty(Size::new(5, 1));
        let end = buffer.set_string(0, 0, "a界b", Style::new());
        assert_eq!(end, 4);
        assert_eq!(row(&buffer, 0), "a界_b ");
    }

    #[test]
    fn wide_character_that_does_not_fit_is_clipped() {
        let mut buffer = Buffer::empty(Size::new(3, 1));
        let end = buffer.set_string(0, 0, "ab界", Style::new());
        assert_eq!(end, 2);
        assert_eq!(row(&buffer, 0), "ab ");
    }

    #[test]
    fn overwriting_half_of_a_wide_character_breaks_it() {
        let mut buffer = Buffer::empty(Size::new(4, 1));
        buffer.set_string(0, 0, "界界", Style::new());
        assert_eq!(row(&buffer, 0), "界_界_");

        // Overwrite the second half of the first wide character.
        buffer.set_string(1, 0, "x", Style::new());
        assert_eq!(row(&buffer, 0), " x界_");

        // Overwrite the first half of the second wide character.
        buffer.set_string(2, 0, "y", Style::new());
        assert_eq!(row(&buffer, 0), " xy ");
    }

    #[test]
    fn combining_characters_stay_in_one_cell() {
        let mut buffer = Buffer::empty(Size::new(3, 1));
        let end = buffer.set_string(0, 0, "e\u{301}x", Style::new());
        assert_eq!(end, 2);
        assert_eq!(buffer.cell(0, 0).map(Cell::symbol), Some("e\u{301}"));
    }

    #[test]
    fn set_symbol_checks_bounds() {
        let mut buffer = Buffer::empty(Size::new(2, 1));
        assert!(buffer.set_symbol(0, 0, "界", Style::new()));
        assert!(!buffer.set_symbol(1, 0, "界", Style::new()));
        assert!(!buffer.set_symbol(0, 0, "\x1b", Style::new()));
        assert!(!buffer.set_symbol(0, 1, "a", Style::new()));
    }

    #[test]
    fn fill_and_set_style() {
        let mut buffer = Buffer::empty(Size::new(4, 3));
        buffer.fill(Rect::new(1, 1, 10, 10), &Cell::new("#"));
        assert_eq!(row(&buffer, 0), "    ");
        assert_eq!(row(&buffer, 1), " ###");
        assert_eq!(row(&buffer, 2), " ###");

        buffer.set_style(Rect::new(0, 0, 2, 1), Style::new().bg(Color::Blue));
        assert_eq!(buffer.cell(1, 0).map(|c| c.bg), Some(Color::Blue));
        assert_eq!(buffer.cell(2, 0).map(|c| c.bg), Some(Color::Reset));
    }

    #[test]
    fn fill_breaks_wide_characters_at_edges() {
        let mut buffer = Buffer::empty(Size::new(4, 1));
        buffer.set_string(0, 0, "界界", Style::new());
        buffer.fill(Rect::new(1, 0, 2, 1), &Cell::new("."));
        assert_eq!(row(&buffer, 0), " .. ");
    }

    #[test]
    fn resize_and_clear() {
        let mut buffer = Buffer::empty(Size::new(2, 2));
        buffer.set_string(0, 0, "ab", Style::new());
        buffer.resize(Size::new(3, 1));
        assert_eq!(buffer.size(), Size::new(3, 1));
        assert_eq!(buffer.cells().len(), 3);
        assert_eq!(row(&buffer, 0), "   ");

        buffer.set_string(0, 0, "x", Style::new());
        buffer.clear();
        assert_eq!(buffer, Buffer::empty(Size::new(3, 1)));
    }

    #[test]
    fn index_and_position_round_trip() {
        let buffer = Buffer::empty(Size::new(4, 3));
        assert_eq!(buffer.index_of(3, 2), Some(11));
        assert_eq!(buffer.position_of(11), Some(Position::new(3, 2)));
        assert_eq!(buffer.index_of(4, 0), None);
        assert_eq!(buffer.position_of(12), None);
    }

    fn positions(updates: &[CellUpdate<'_>]) -> Vec<(u16, u16, String)> {
        updates
            .iter()
            .map(|u| (u.x, u.y, u.cell.symbol().to_owned()))
            .collect()
    }

    #[test]
    fn diff_returns_only_changes() {
        let previous = Buffer::empty(Size::new(4, 2));
        let mut current = previous.clone();
        current.set_string(1, 1, "ab", Style::new());
        assert_eq!(
            positions(&current.diff(&previous)),
            vec![(1, 1, "a".into()), (2, 1, "b".into())]
        );
        assert_eq!(current.diff(&current.clone()), Vec::new());
    }

    #[test]
    fn diff_detects_style_changes() {
        let previous = Buffer::empty(Size::new(2, 1));
        let mut current = previous.clone();
        current.set_style(Rect::new(1, 0, 1, 1), Style::new().bold());
        assert_eq!(
            positions(&current.diff(&previous)),
            vec![(1, 0, " ".into())]
        );
    }

    #[test]
    fn diff_skips_continuation_cells() {
        let previous = Buffer::empty(Size::new(4, 1));
        let mut current = previous.clone();
        current.set_string(0, 0, "界", Style::new());
        assert_eq!(
            positions(&current.diff(&previous)),
            vec![(0, 0, "界".into())]
        );
    }

    #[test]
    fn diff_redraws_cells_covered_by_a_removed_wide_character() {
        let mut previous = Buffer::empty(Size::new(4, 1));
        previous.set_string(0, 0, "界", Style::new());
        let mut current = previous.clone();
        current.set_string(0, 0, "a", Style::new());
        assert_eq!(
            positions(&current.diff(&previous)),
            vec![(0, 0, "a".into()), (1, 0, " ".into())]
        );
    }

    #[test]
    fn diff_with_different_sizes_returns_everything() {
        let previous = Buffer::empty(Size::new(1, 1));
        let mut current = Buffer::empty(Size::new(3, 1));
        current.set_string(0, 0, "界", Style::new());
        assert_eq!(
            positions(&current.diff(&previous)),
            vec![(0, 0, "界".into()), (2, 0, " ".into())]
        );
    }
}
