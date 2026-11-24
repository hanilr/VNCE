//! Blocks of text: [`Paragraph`].

use unicode_width::UnicodeWidthStr;
use vnce_core::{Buffer, Rect, Style};

use crate::Widget;
use crate::block::Block;
use crate::text::{Alignment, Text};

/// One row on the screen: graphemes with their styles, and the alignment of its line.
type Row<'t> = (Vec<(&'t str, Style)>, Alignment);

/// Text drawn in an area: aligned, optionally wrapped, truncated with `…`, scrollable and
/// optionally inside a [`Block`].
///
/// ```
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{Paragraph, Widget};
///
/// let mut buffer = Buffer::empty(Size::new(8, 2));
/// Paragraph::new("A short story about wrapping")
///     .wrap(true)
///     .render(Rect::new(0, 0, 8, 2), &mut buffer);
/// let row: String = (0..8).map(|x| buffer.cell(x, 1).unwrap().symbol()).collect();
/// assert_eq!(row, "story   ");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paragraph<'a> {
    text: Text<'a>,
    style: Style,
    alignment: Alignment,
    wrap: bool,
    ellipsis: bool,
    scroll: u16,
    block: Option<Block<'a>>,
}

impl<'a> Paragraph<'a> {
    /// A paragraph showing `text`, left-aligned, without wrapping.
    pub fn new(text: impl Into<Text<'a>>) -> Self {
        Self {
            text: text.into(),
            style: Style::new(),
            alignment: Alignment::Left,
            wrap: false,
            ellipsis: true,
            scroll: 0,
            block: None,
        }
    }

    /// Sets the style patch of the whole area.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the alignment of lines that do not set their own.
    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Wraps long lines at word boundaries (words longer than a row are broken).
    #[must_use]
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Ends lines that are cut off with `…` (on by default; without wrapping only).
    #[must_use]
    pub fn ellipsis(mut self, ellipsis: bool) -> Self {
        self.ellipsis = ellipsis;
        self
    }

    /// Skips the first `rows` rows (after wrapping).
    #[must_use]
    pub fn scroll(mut self, rows: u16) -> Self {
        self.scroll = rows;
        self
    }

    /// Draws the paragraph inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Number of rows the text takes in `width` columns (after wrapping), for scrolling.
    #[must_use]
    pub fn line_count(&self, width: u16) -> usize {
        self.rows(width).len()
    }

    /// The rows of the text for `width` columns.
    fn rows(&self, width: u16) -> Vec<Row<'_>> {
        let mut rows = Vec::new();
        for line in &self.text.lines {
            let alignment = line.alignment.unwrap_or(self.alignment);
            let graphemes = line.graphemes(self.text.style);
            if self.wrap {
                for row in wrap(graphemes, width) {
                    rows.push((row, alignment));
                }
            } else {
                let mut row: Vec<(&str, Style)> = graphemes.collect();
                if self.ellipsis {
                    truncate(&mut row, width);
                }
                rows.push((row, alignment));
            }
        }
        rows
    }
}

fn grapheme_width(grapheme: &str) -> u16 {
    u16::try_from(grapheme.width()).unwrap_or(u16::MAX)
}

/// Splits a line into rows of at most `width` columns at whitespace; words longer than a
/// row are broken. Spaces at a wrap point are dropped, leading spaces are kept.
fn wrap<'t>(
    graphemes: impl Iterator<Item = (&'t str, Style)>,
    width: u16,
) -> Vec<Vec<(&'t str, Style)>> {
    struct State<'t> {
        width: u16,
        rows: Vec<Vec<(&'t str, Style)>>,
        row: Vec<(&'t str, Style)>,
        row_width: u16,
        spaces: Vec<(&'t str, Style)>,
        spaces_width: u16,
        word: Vec<(&'t str, Style)>,
        word_width: u16,
    }

    impl State<'_> {
        fn new_row(&mut self) {
            self.rows.push(std::mem::take(&mut self.row));
            self.row_width = 0;
        }

        fn place_word(&mut self) {
            let width = self.width;
            let fits = move |used: u16, extra: u16| used.saturating_add(extra) <= width;
            if fits(
                self.row_width,
                self.spaces_width.saturating_add(self.word_width),
            ) {
                self.row.append(&mut self.spaces);
                self.row_width += self.spaces_width;
            } else if self.word_width <= self.width {
                // The word goes to the next row; the spaces before it are dropped.
                self.new_row();
            } else if fits(self.row_width, self.spaces_width)
                && self.row_width + self.spaces_width < width
            {
                // A long word starts after the spaces and is broken.
                self.row.append(&mut self.spaces);
                self.row_width += self.spaces_width;
            } else if !self.row.is_empty() {
                self.new_row();
            }
            self.spaces.clear();
            self.spaces_width = 0;
            for (grapheme, style) in std::mem::take(&mut self.word) {
                let width = grapheme_width(grapheme);
                if self.row_width.saturating_add(width) > self.width && !self.row.is_empty() {
                    self.new_row();
                }
                self.row.push((grapheme, style));
                self.row_width = self.row_width.saturating_add(width);
            }
            self.word_width = 0;
        }
    }

    let mut state = State {
        width: width.max(1),
        rows: Vec::new(),
        row: Vec::new(),
        row_width: 0,
        spaces: Vec::new(),
        spaces_width: 0,
        word: Vec::new(),
        word_width: 0,
    };
    for (grapheme, style) in graphemes {
        let width = grapheme_width(grapheme);
        if grapheme.chars().all(char::is_whitespace) {
            if !state.word.is_empty() {
                state.place_word();
            }
            state.spaces.push((grapheme, style));
            state.spaces_width = state.spaces_width.saturating_add(width);
        } else {
            state.word.push((grapheme, style));
            state.word_width = state.word_width.saturating_add(width);
        }
    }
    if !state.word.is_empty() {
        state.place_word();
    } else if state.row.is_empty() && !state.spaces.is_empty() {
        // A line of only spaces keeps them (up to the width).
        let spaces = std::mem::take(&mut state.spaces);
        state.word = spaces;
        state.word_width = state.spaces_width;
        state.spaces_width = 0;
        state.place_word();
    }
    state.rows.push(state.row);
    state.rows
}

/// Cuts a row to `width` columns, ending it with `…` when something was cut.
fn truncate(row: &mut Vec<(&str, Style)>, width: u16) {
    let total: u16 = row
        .iter()
        .fold(0, |sum, (g, _)| sum.saturating_add(grapheme_width(g)));
    if total <= width {
        return;
    }
    let mut used = 0_u16;
    let mut keep = 0;
    for (grapheme, _) in row.iter() {
        let w = grapheme_width(grapheme);
        // Leave one column for the ellipsis.
        if used + w > width.saturating_sub(1) {
            break;
        }
        used += w;
        keep += 1;
    }
    let style = row
        .get(keep)
        .or_else(|| row.last())
        .map_or(Style::new(), |(_, s)| *s);
    row.truncate(keep);
    if width > 0 {
        row.push(("…", style));
    }
}

impl Widget for Paragraph<'_> {
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
        let rows = self.rows(area.width);
        for (y, (row, alignment)) in
            (area.top()..area.bottom()).zip(rows.iter().skip(self.scroll.into()))
        {
            let width: u16 = row
                .iter()
                .fold(0, |sum, (g, _)| sum.saturating_add(grapheme_width(g)));
            let mut x = area.left() + alignment.offset(width, area.width);
            for &(grapheme, style) in row {
                let w = grapheme_width(grapheme);
                if x.saturating_add(w) > area.right() {
                    break;
                }
                buffer.set_symbol(x, y, grapheme, style);
                x += w;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Color, Size};

    use super::*;
    use crate::block::Block;
    use crate::test_support::rows;
    use crate::text::{Line, Span};

    fn render(paragraph: &Paragraph<'_>, width: u16, height: u16) -> Vec<String> {
        let mut buffer = Buffer::empty(Size::new(width, height));
        paragraph.render(Rect::new(0, 0, width, height), &mut buffer);
        rows(&buffer)
    }

    #[test]
    fn wraps_at_words_and_breaks_long_words() {
        let p = Paragraph::new("the quick brown fox").wrap(true);
        assert_eq!(
            render(&p, 10, 3),
            ["the quick ", "brown fox ", "          "]
        );
        let long = Paragraph::new("a abcdefghij").wrap(true);
        assert_eq!(render(&long, 4, 4), ["a ab", "cdef", "ghij", "    "]);
        assert_eq!(p.line_count(10), 2);
        assert_eq!(p.line_count(3), 6);
    }

    #[test]
    fn wrapping_keeps_wide_characters_whole() {
        let p = Paragraph::new("日本語のテキスト").wrap(true);
        assert_eq!(render(&p, 5, 4), ["日本 ", "語の ", "テキ ", "スト "]);
        // A wide character never ends up half outside a one-column area.
        let narrow = Paragraph::new("日").wrap(true);
        assert_eq!(render(&narrow, 1, 1), [" "]);
    }

    #[test]
    fn leading_spaces_and_empty_lines_are_kept() {
        let p = Paragraph::new("  indented\n\nnext").wrap(true);
        assert_eq!(
            render(&p, 12, 3),
            ["  indented  ", "            ", "next        "]
        );
    }

    #[test]
    fn truncation_with_ellipsis() {
        let p = Paragraph::new("abcdefgh");
        assert_eq!(render(&p, 5, 1), ["abcd…"]);
        assert_eq!(render(&p.clone().ellipsis(false), 5, 1), ["abcde"]);
        // The ellipsis replaces a wide character that does not fit.
        assert_eq!(render(&Paragraph::new("ab日本"), 4, 1), ["ab… "]);
        assert_eq!(render(&Paragraph::new("fits"), 4, 1), ["fits"]);
    }

    #[test]
    fn alignment_per_paragraph_and_line() {
        let text = vec![Line::from("ab"), Line::from("cd").right_aligned()];
        let p = Paragraph::new(text).alignment(Alignment::Center);
        assert_eq!(render(&p, 6, 2), ["  ab  ", "    cd"]);
    }

    #[test]
    fn scrolling_skips_rows() {
        let p = Paragraph::new("one\ntwo\nthree").scroll(1);
        assert_eq!(render(&p, 5, 2), ["two  ", "three"]);
        assert_eq!(render(&p.clone().scroll(9), 5, 1), ["     "]);
    }

    #[test]
    fn inside_a_block_with_styles() {
        let p = Paragraph::new(Line::from(vec![
            Span::raw("hi "),
            Span::styled("you", Style::new().fg(Color::Red)),
        ]))
        .block(Block::bordered())
        .style(Style::new().bg(Color::Blue));
        let mut buffer = Buffer::empty(Size::new(8, 3));
        p.render(Rect::new(0, 0, 8, 3), &mut buffer);
        assert_eq!(rows(&buffer), ["┌──────┐", "│hi you│", "└──────┘"]);
        let red = buffer.cell(4, 1).unwrap();
        assert_eq!((red.fg, red.bg), (Color::Red, Color::Blue));
        // The border is outside the paragraph style.
        assert_eq!(buffer.cell(0, 0).unwrap().bg, Color::Reset);
    }

    #[test]
    fn control_characters_are_never_drawn() {
        let p = Paragraph::new("a\x1b[31mb\tc");
        assert_eq!(render(&p, 8, 1), ["a[31mbc "]);
    }
}
