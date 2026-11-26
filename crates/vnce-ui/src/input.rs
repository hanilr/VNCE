//! A one-line text field: [`TextInput`] and its [`TextInputState`] (the text, the cursor and
//! the editing rules), for names, codes, seeds and chat lines.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use vnce_core::{Buffer, Rect, Style};
use vnce_term::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::block::Block;
use crate::paragraph::grapheme_width;
use crate::text::put_str;
use crate::{StatefulWidget, Widget as _};

/// The text of a [`TextInput`], its cursor and its editing rules, kept between frames.
///
/// Edit it with the methods ([`insert`](Self::insert), [`backspace`](Self::backspace),
/// [`left`](Self::left), ...) or hand it key and paste events with
/// [`handle_event`](Self::handle_event). Enter, Esc, Tab and the up and down arrows are
/// left to the game.
///
/// ```
/// use vnce_ui::TextInputState;
///
/// // Six letters or digits, typed in capitals.
/// let mut code = TextInputState::new()
///     .max_len(6)
///     .filter(|c| c.is_ascii_alphanumeric().then(|| c.to_ascii_uppercase()));
/// for c in "ab-c12345".chars() {
///     code.insert(c);
/// }
/// assert_eq!(code.text(), "ABC123");
/// code.backspace();
/// assert_eq!(code.text(), "ABC12");
/// ```
#[derive(Debug, Clone, Default)]
pub struct TextInputState {
    text: String,
    /// The cursor as a byte index into `text`, always at a grapheme boundary.
    cursor: usize,
    /// The first grapheme shown (the field scrolls to keep the cursor visible).
    offset: usize,
    max_len: Option<usize>,
    filter: Option<fn(char) -> Option<char>>,
}

impl TextInputState {
    /// An empty field without limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A field holding `text`, with the cursor at its end.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        let mut state = Self::new();
        state.set_text(text);
        state
    }

    /// The most characters (grapheme clusters) the field takes.
    #[must_use]
    pub fn max_len(mut self, max: usize) -> Self {
        self.max_len = Some(max);
        self
    }

    /// A rule for typed and pasted characters: it returns the character to insert (maybe
    /// changed, for example to upper case) or `None` to refuse it.
    #[must_use]
    pub fn filter(mut self, filter: fn(char) -> Option<char>) -> Self {
        self.filter = Some(filter);
        self
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the text (without the filter, cut to the maximum length) and puts the
    /// cursor at its end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        let mut text = text.into();
        let cut = self
            .max_len
            .and_then(|max| text.grapheme_indices(true).nth(max))
            .map(|(cut, _)| cut);
        if let Some(cut) = cut {
            text.truncate(cut);
        }
        self.text = text;
        self.cursor = self.text.len();
        self.offset = 0;
    }

    /// Empties the field.
    pub fn clear(&mut self) {
        self.set_text("");
    }

    /// Returns `true` if the field is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Number of characters (grapheme clusters).
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.graphemes(true).count()
    }

    /// The cursor as a character (grapheme) index: `0` before the first one, [`len`]
    /// after the last one.
    ///
    /// [`len`]: Self::len
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.text[..self.cursor].graphemes(true).count()
    }

    /// Moves the cursor to character (grapheme) index `index` (clamped to the text).
    pub fn set_cursor(&mut self, index: usize) {
        self.cursor = self
            .text
            .grapheme_indices(true)
            .nth(index)
            .map_or(self.text.len(), |(i, _)| i);
    }

    /// Inserts `c` at the cursor, through the filter and within the maximum length.
    /// Returns `true` if it was inserted.
    pub fn insert(&mut self, c: char) -> bool {
        if c.is_control() {
            return false;
        }
        let Some(c) = (match self.filter {
            Some(filter) => filter(c),
            None => Some(c),
        }) else {
            return false;
        };
        let mut text = self.text.clone();
        text.insert(self.cursor, c);
        if self
            .max_len
            .is_some_and(|max| text.graphemes(true).count() > max)
        {
            return false;
        }
        self.text = text;
        self.cursor += c.len_utf8();
        true
    }

    /// Inserts every character of `text` that the rules let in (a paste). Returns `true`
    /// if anything was inserted.
    pub fn insert_str(&mut self, text: &str) -> bool {
        let mut any = false;
        for c in text.chars() {
            any |= self.insert(c);
        }
        any
    }

    /// The byte index of the grapheme boundary before the cursor.
    fn previous_boundary(&self) -> Option<usize> {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
    }

    /// The byte index of the grapheme boundary after the cursor.
    fn next_boundary(&self) -> Option<usize> {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map(|g| self.cursor + g.len())
    }

    /// Deletes the character before the cursor. Returns `true` if one was deleted.
    pub fn backspace(&mut self) -> bool {
        let Some(start) = self.previous_boundary() else {
            return false;
        };
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        true
    }

    /// Deletes the character after the cursor. Returns `true` if one was deleted.
    pub fn delete(&mut self) -> bool {
        let Some(end) = self.next_boundary() else {
            return false;
        };
        self.text.replace_range(self.cursor..end, "");
        true
    }

    /// Moves the cursor one character left. Returns `true` if it moved.
    pub fn left(&mut self) -> bool {
        self.previous_boundary().map(|i| self.cursor = i).is_some()
    }

    /// Moves the cursor one character right. Returns `true` if it moved.
    pub fn right(&mut self) -> bool {
        self.next_boundary().map(|i| self.cursor = i).is_some()
    }

    /// Moves the cursor to the start. Returns `true` if it moved.
    pub fn home(&mut self) -> bool {
        let moved = self.cursor != 0;
        self.cursor = 0;
        moved
    }

    /// Moves the cursor to the end. Returns `true` if it moved.
    pub fn end(&mut self) -> bool {
        let moved = self.cursor != self.text.len();
        self.cursor = self.text.len();
        moved
    }

    /// Edits the field by a key: characters, Backspace, Delete, Left, Right, Home and End
    /// (presses and repeats; releases and keys with Ctrl or Alt are ignored). Returns
    /// `true` if the key was used, so the game can handle the others (Enter, Esc, ...).
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        if !key.is_press()
            || key.modifiers.contains(KeyModifiers::CONTROL)
            || key.modifiers.contains(KeyModifiers::ALT)
        {
            return false;
        }
        match key.code {
            KeyCode::Char(c) => {
                self.insert(c);
                true
            }
            KeyCode::Backspace => {
                self.backspace();
                true
            }
            KeyCode::Delete => {
                self.delete();
                true
            }
            KeyCode::Left => {
                self.left();
                true
            }
            KeyCode::Right => {
                self.right();
                true
            }
            KeyCode::Home => {
                self.home();
                true
            }
            KeyCode::End => {
                self.end();
                true
            }
            _ => false,
        }
    }

    /// Edits the field by a key event (see [`handle_key`](Self::handle_key)) or a paste.
    /// Returns `true` if the event was used.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => {
                self.insert_str(text);
                true
            }
            _ => false,
        }
    }
}

/// A one-line text field. The text scrolls sideways to keep the cursor visible; the
/// cursor is drawn as a reversed cell while the field has the focus.
///
/// ```
/// use vnce_core::{Buffer, Rect, Size};
/// use vnce_ui::{StatefulWidget, TextInput, TextInputState};
///
/// let mut name = TextInputState::with_text("Ada");
/// let mut buffer = Buffer::empty(Size::new(8, 1));
/// TextInput::new().render(Rect::new(0, 0, 8, 1), &mut buffer, &mut name);
/// let row: String = (0..8).map(|x| buffer.cell(x, 0).unwrap().symbol()).collect();
/// assert_eq!(row, "Ada     ");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput<'a> {
    style: Style,
    cursor_style: Style,
    placeholder: Cow<'a, str>,
    placeholder_style: Style,
    mask: Option<char>,
    focused: bool,
    block: Option<Block<'a>>,
}

impl Default for TextInput<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> TextInput<'a> {
    /// A focused field with a reversed cursor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            style: Style::new(),
            cursor_style: Style::new().reverse(),
            placeholder: Cow::Borrowed(""),
            placeholder_style: Style::new().dim(),
            mask: None,
            focused: true,
            block: None,
        }
    }

    /// Sets the style patch of the whole field.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets the style patch of the cursor cell.
    #[must_use]
    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }

    /// A hint shown while the field is empty, for example `"Your name"`.
    #[must_use]
    pub fn placeholder(mut self, text: impl Into<Cow<'a, str>>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Sets the style patch of the placeholder (dim by default).
    #[must_use]
    pub fn placeholder_style(mut self, style: Style) -> Self {
        self.placeholder_style = style;
        self
    }

    /// Shows every character as `mask` (for passwords).
    #[must_use]
    pub fn mask(mut self, mask: char) -> Self {
        self.mask = Some(mask);
        self
    }

    /// Whether the field has the focus: only then is the cursor drawn.
    #[must_use]
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Draws the field inside a block.
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl StatefulWidget for TextInput<'_> {
    type State = TextInputState;

    fn render(&self, area: Rect, buffer: &mut Buffer, state: &mut TextInputState) {
        let mut area = area.intersection(buffer.area());
        if let Some(block) = &self.block {
            block.render(area, buffer);
            area = block.inner(area);
        }
        if area.is_empty() {
            return;
        }
        buffer.set_style(area, self.style);
        let y = area.top();
        let mask = self.mask.map(String::from);
        let graphemes: Vec<&str> = match &mask {
            Some(mask) => state.text.graphemes(true).map(|_| mask.as_str()).collect(),
            None => state.text.graphemes(true).collect(),
        };
        let cursor = state.cursor();

        if graphemes.is_empty() && !self.placeholder.is_empty() {
            put_str(
                buffer,
                area.left(),
                y,
                area.right(),
                &self.placeholder,
                self.placeholder_style,
            );
        }

        // Scroll so that the cursor (and the cell after the text) stays visible.
        let width = |g: &&str| grapheme_width(g).max(1);
        state.offset = state.offset.min(cursor);
        loop {
            let used: u16 = graphemes[state.offset..cursor]
                .iter()
                .map(width)
                .fold(0, u16::saturating_add);
            if used < area.width || state.offset >= cursor {
                break;
            }
            state.offset += 1;
        }

        let mut x = area.left();
        for (index, grapheme) in graphemes.iter().enumerate().skip(state.offset) {
            let w = width(grapheme);
            if x.saturating_add(w) > area.right() {
                break;
            }
            let style = if self.focused && index == cursor {
                self.cursor_style
            } else {
                Style::new()
            };
            if grapheme_width(grapheme) == 0 {
                buffer.set_symbol(x, y, " ", style);
            } else {
                buffer.set_symbol(x, y, grapheme, style);
            }
            x += w;
        }
        if self.focused && cursor == graphemes.len() && x < area.right() {
            // The cursor after the text: a reversed empty cell (or the placeholder's first).
            buffer.set_style(Rect::new(x, y, 1, 1), self.cursor_style);
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Modifiers, Size};
    use vnce_term::KeyEventKind;

    use super::*;
    use crate::test_support::rows;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }
    }

    fn render(input: &TextInput<'_>, state: &mut TextInputState, width: u16) -> Buffer {
        let mut buffer = Buffer::empty(Size::new(width, 1));
        input.render(Rect::new(0, 0, width, 1), &mut buffer, state);
        buffer
    }

    fn reversed(buffer: &Buffer) -> Vec<u16> {
        (0..buffer.size().width)
            .filter(|&x| {
                buffer
                    .cell(x, 0)
                    .unwrap()
                    .modifiers
                    .contains(Modifiers::REVERSE)
            })
            .collect()
    }

    #[test]
    fn editing_moves_the_cursor() {
        let mut s = TextInputState::with_text("héllo");
        assert_eq!((s.cursor(), s.len()), (5, 5));
        assert!(s.left() && s.left());
        assert!(s.backspace());
        assert_eq!(s.text(), "hélo");
        assert_eq!(s.cursor(), 2);
        assert!(s.delete());
        assert_eq!(s.text(), "héo");
        assert!(s.insert('!'));
        assert_eq!((s.text(), s.cursor()), ("hé!o", 3));
        assert!(s.home() && !s.home() && !s.backspace() && !s.left());
        assert!(s.end() && !s.end() && !s.delete() && !s.right());
        s.set_cursor(1);
        assert_eq!(s.cursor(), 1);
        s.set_cursor(99);
        assert_eq!(s.cursor(), 4);
        // A combined character is one step.
        let mut e = TextInputState::with_text("e\u{301}x");
        assert_eq!(e.len(), 2);
        e.left();
        assert!(e.backspace());
        assert_eq!(e.text(), "x");
        s.clear();
        assert!(s.is_empty() && !s.insert('\n'));
    }

    #[test]
    fn limits_filters_and_pastes() {
        let mut s = TextInputState::new()
            .max_len(3)
            .filter(|c| c.is_ascii_digit().then_some(c));
        assert!(s.insert_str("1a2b3c4"));
        assert_eq!(s.text(), "123");
        assert!(!s.insert('5'), "full");
        s.set_text("98765");
        assert_eq!(s.text(), "987", "cut to the limit");
        assert!(!s.insert_str("x"));
    }

    #[test]
    fn keys_and_events() {
        let mut s = TextInputState::new();
        for code in [
            KeyCode::Char('a'),
            KeyCode::Char('c'),
            KeyCode::Left,
            KeyCode::Char('b'),
            KeyCode::End,
            KeyCode::Char('d'),
            KeyCode::Home,
            KeyCode::Delete,
            KeyCode::Right,
            KeyCode::Backspace,
        ] {
            assert!(s.handle_key(&key(code)));
        }
        assert_eq!(s.text(), "cd");
        // Left to the game: Enter, Esc, releases, Ctrl + key.
        assert!(!s.handle_key(&key(KeyCode::Enter)));
        assert!(!s.handle_key(&key(KeyCode::Esc)));
        let mut release = key(KeyCode::Char('x'));
        release.kind = KeyEventKind::Release;
        assert!(!s.handle_key(&release));
        let mut ctrl = key(KeyCode::Char('x'));
        ctrl.modifiers = KeyModifiers::CONTROL;
        assert!(!s.handle_key(&ctrl));
        assert!(s.handle_event(&Event::Paste("ef".into())));
        assert_eq!(s.text(), "efcd");
        assert!(!s.handle_event(&Event::FocusLost));
    }

    #[test]
    fn draws_text_cursor_and_placeholder() {
        let mut s = TextInputState::with_text("abc");
        let b = render(&TextInput::new(), &mut s, 5);
        assert_eq!(rows(&b), ["abc  "]);
        assert_eq!(reversed(&b), [3], "after the text");
        s.left();
        assert_eq!(reversed(&render(&TextInput::new(), &mut s, 5)), [2]);
        assert_eq!(
            reversed(&render(&TextInput::new().focused(false), &mut s, 5)),
            Vec::<u16>::new()
        );
        let b = render(&TextInput::new().mask('*'), &mut s, 5);
        assert_eq!(rows(&b), ["***  "]);
        let mut empty = TextInputState::new();
        let b = render(&TextInput::new().placeholder("name"), &mut empty, 6);
        assert_eq!(rows(&b), ["name  "]);
        assert_eq!(reversed(&b), [0]);
    }

    #[test]
    fn scrolls_to_keep_the_cursor_visible() {
        let mut s = TextInputState::with_text("abcdefgh");
        assert_eq!(rows(&render(&TextInput::new(), &mut s, 4)), ["fgh "]);
        s.home();
        assert_eq!(rows(&render(&TextInput::new(), &mut s, 4)), ["abcd"]);
        // Boxed.
        let mut buffer = Buffer::empty(Size::new(6, 3));
        TextInput::new().block(Block::bordered()).render(
            Rect::new(0, 0, 6, 3),
            &mut buffer,
            &mut s,
        );
        assert_eq!(rows(&buffer), ["┌────┐", "│abcd│", "└────┘"]);
    }
}
