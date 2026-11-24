//! A single terminal cell.

use std::fmt;

use unicode_width::UnicodeWidthStr;

use crate::color::Color;
use crate::style::{Modifiers, Style};

/// Maximum number of bytes stored inline (without a heap allocation).
const INLINE_CAPACITY: usize = 22;

/// A short string that is stored inline when it fits, which avoids one heap allocation per
/// cell for almost all symbols.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Symbol {
    Inline {
        len: u8,
        bytes: [u8; INLINE_CAPACITY],
    },
    Heap(Box<str>),
}

impl Symbol {
    /// A single space.
    const SPACE: Self = {
        let mut bytes = [0; INLINE_CAPACITY];
        bytes[0] = b' ';
        Self::Inline { len: 1, bytes }
    };

    /// The empty string, used for continuation cells of wide characters.
    const EMPTY: Self = Self::Inline {
        len: 0,
        bytes: [0; INLINE_CAPACITY],
    };

    fn new(s: &str) -> Self {
        if s.len() <= INLINE_CAPACITY {
            let mut bytes = [0; INLINE_CAPACITY];
            bytes[..s.len()].copy_from_slice(s.as_bytes());
            // `s.len() <= INLINE_CAPACITY`, so the length always fits in a `u8`.
            let len = u8::try_from(s.len()).unwrap_or(u8::MAX);
            Self::Inline { len, bytes }
        } else {
            Self::Heap(s.into())
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Inline { len, bytes } => {
                // The bytes always come from a valid `&str` in `Symbol::new`.
                std::str::from_utf8(&bytes[..usize::from(*len)]).unwrap_or_default()
            }
            Self::Heap(s) => s,
        }
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

/// One terminal cell: a symbol (one grapheme cluster) with colors and modifiers.
///
/// A character that is two columns wide (for example CJK characters or most emoji) is
/// stored in its first cell; the following cell is a *continuation cell* with an empty
/// symbol. [`Buffer`](crate::Buffer) keeps this invariant when writing text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cell {
    symbol: Symbol,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Text modifiers.
    pub modifiers: Modifiers,
}

impl Cell {
    /// A blank cell: a space with default colors and no modifiers.
    pub const EMPTY: Self = Self {
        symbol: Symbol::SPACE,
        fg: Color::Reset,
        bg: Color::Reset,
        modifiers: Modifiers::NONE,
    };

    /// Creates a cell with the given symbol and default style.
    #[must_use]
    pub fn new(symbol: &str) -> Self {
        let mut cell = Self::EMPTY;
        cell.set_symbol(symbol);
        cell
    }

    /// The symbol of the cell. Empty for continuation cells.
    #[must_use]
    pub fn symbol(&self) -> &str {
        self.symbol.as_str()
    }

    /// Sets the symbol. It should be a single grapheme cluster.
    ///
    /// Prefer [`Buffer::set_string`](crate::Buffer::set_string) for writing text: it
    /// handles wide characters and filters out control characters.
    pub fn set_symbol(&mut self, symbol: &str) -> &mut Self {
        self.symbol = Symbol::new(symbol);
        self
    }

    /// Sets the symbol to a single character.
    pub fn set_char(&mut self, ch: char) -> &mut Self {
        let mut buf = [0; 4];
        self.set_symbol(ch.encode_utf8(&mut buf))
    }

    /// Applies a [`Style`] patch to the cell.
    pub fn set_style(&mut self, style: Style) -> &mut Self {
        if let Some(fg) = style.fg {
            self.fg = fg;
        }
        if let Some(bg) = style.bg {
            self.bg = bg;
        }
        self.modifiers.remove(style.sub_modifiers);
        self.modifiers.insert(style.add_modifiers);
        self
    }

    /// The full style of the cell.
    #[must_use]
    pub fn style(&self) -> Style {
        Style {
            fg: Some(self.fg),
            bg: Some(self.bg),
            add_modifiers: self.modifiers,
            sub_modifiers: Modifiers::ALL.difference(self.modifiers),
        }
    }

    /// Resets the cell to [`Cell::EMPTY`].
    pub fn reset(&mut self) {
        *self = Self::EMPTY;
    }

    /// Number of columns the symbol occupies (0 for continuation cells).
    #[must_use]
    pub fn width(&self) -> usize {
        self.symbol().width()
    }

    /// Returns `true` if this is the continuation cell of a wide character.
    #[must_use]
    pub fn is_continuation(&self) -> bool {
        self.symbol().is_empty()
    }

    /// Turns the cell into a continuation cell (empty symbol), keeping its style.
    pub(crate) fn set_continuation(&mut self) {
        self.symbol = Symbol::EMPTY;
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_and_heap_symbols() {
        let mut cell = Cell::new("a");
        assert_eq!(cell.symbol(), "a");

        // A long grapheme cluster (family emoji) still round-trips.
        let family = "👨\u{200d}👩\u{200d}👧\u{200d}👦";
        cell.set_symbol(family);
        assert_eq!(cell.symbol(), family);
        assert!(family.len() > INLINE_CAPACITY);

        cell.set_char('é');
        assert_eq!(cell.symbol(), "é");
    }

    #[test]
    fn widths() {
        assert_eq!(Cell::new("a").width(), 1);
        assert_eq!(Cell::new("界").width(), 2);
        assert_eq!(Cell::new("").width(), 0);
        assert!(Cell::new("").is_continuation());
        assert!(!Cell::EMPTY.is_continuation());
    }

    #[test]
    fn style_patch_on_cell() {
        let mut cell = Cell::EMPTY;
        cell.set_style(Style::new().fg(Color::Red).bold().italic());
        assert_eq!(cell.fg, Color::Red);
        assert_eq!(cell.bg, Color::Reset);
        assert_eq!(cell.modifiers, Modifiers::BOLD | Modifiers::ITALIC);

        cell.set_style(
            Style::new()
                .bg(Color::Blue)
                .remove_modifier(Modifiers::BOLD),
        );
        assert_eq!(cell.fg, Color::Red);
        assert_eq!(cell.bg, Color::Blue);
        assert_eq!(cell.modifiers, Modifiers::ITALIC);

        cell.set_style(Style::reset());
        assert_eq!(cell, Cell::EMPTY);
    }
}
