//! Text styles: colors and modifiers.

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not, Sub, SubAssign};

use crate::color::Color;

/// A set of text modifiers (bold, italic, ...).
///
/// Combine modifiers with `|`:
///
/// ```
/// use vnce_core::Modifiers;
///
/// let modifiers = Modifiers::BOLD | Modifiers::ITALIC;
/// assert!(modifiers.contains(Modifiers::BOLD));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers(u16);

impl Modifiers {
    /// No modifiers.
    pub const NONE: Self = Self(0);
    /// Bold (increased intensity).
    pub const BOLD: Self = Self(1);
    /// Dim (decreased intensity).
    pub const DIM: Self = Self(1 << 1);
    /// Italic.
    pub const ITALIC: Self = Self(1 << 2);
    /// Underline.
    pub const UNDERLINE: Self = Self(1 << 3);
    /// Blinking text.
    pub const BLINK: Self = Self(1 << 4);
    /// Swapped foreground and background colors.
    pub const REVERSE: Self = Self(1 << 5);
    /// Hidden (invisible) text.
    pub const HIDDEN: Self = Self(1 << 6);
    /// Strikethrough.
    pub const STRIKETHROUGH: Self = Self(1 << 7);
    /// All modifiers.
    pub const ALL: Self = Self((1 << 8) - 1);

    /// Every single modifier with its name, in bit order.
    const NAMED: [(Self, &'static str); 8] = [
        (Self::BOLD, "BOLD"),
        (Self::DIM, "DIM"),
        (Self::ITALIC, "ITALIC"),
        (Self::UNDERLINE, "UNDERLINE"),
        (Self::BLINK, "BLINK"),
        (Self::REVERSE, "REVERSE"),
        (Self::HIDDEN, "HIDDEN"),
        (Self::STRIKETHROUGH, "STRIKETHROUGH"),
    ];

    /// The raw bit representation.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Creates a set from raw bits, ignoring unknown bits.
    #[must_use]
    pub const fn from_bits_truncate(bits: u16) -> Self {
        Self(bits & Self::ALL.0)
    }

    /// Returns `true` if no modifier is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if all modifiers in `other` are set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Returns `true` if at least one modifier in `other` is set.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Modifiers set in either `self` or `other`.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Modifiers set in both `self` and `other`.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Modifiers set in `self` but not in `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Adds the modifiers in `other`.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Removes the modifiers in `other`.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Iterates over the single modifiers that are set, in bit order.
    pub fn iter(self) -> impl Iterator<Item = Self> {
        Self::NAMED
            .into_iter()
            .map(|(modifier, _)| modifier)
            .filter(move |&modifier| self.contains(modifier))
    }
}

impl fmt::Debug for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("Modifiers(NONE)");
        }
        f.write_str("Modifiers(")?;
        let mut first = true;
        for (modifier, name) in Self::NAMED {
            if self.contains(modifier) {
                if !first {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        f.write_str(")")
    }
}

impl BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        self.insert(rhs);
    }
}

impl BitAnd for Modifiers {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        self.intersection(rhs)
    }
}

impl BitAndAssign for Modifiers {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl Sub for Modifiers {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.difference(rhs)
    }
}

impl SubAssign for Modifiers {
    fn sub_assign(&mut self, rhs: Self) {
        self.remove(rhs);
    }
}

impl Not for Modifiers {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0 & Self::ALL.0)
    }
}

/// A style that can be applied on top of a cell.
///
/// A style is a *patch*: fields that are `None` (or modifiers that are not mentioned)
/// leave the cell unchanged. `add_modifiers` are switched on, `sub_modifiers` are
/// switched off.
///
/// ```
/// use vnce_core::{Color, Style};
///
/// let style = Style::new().fg(Color::Red).bg(Color::Black).bold();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// Foreground color, or `None` to keep the current one.
    pub fg: Option<Color>,
    /// Background color, or `None` to keep the current one.
    pub bg: Option<Color>,
    /// Modifiers to switch on.
    pub add_modifiers: Modifiers,
    /// Modifiers to switch off.
    pub sub_modifiers: Modifiers,
}

impl Style {
    /// An empty style that changes nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            add_modifiers: Modifiers::NONE,
            sub_modifiers: Modifiers::NONE,
        }
    }

    /// A style that resets colors to the terminal default and removes all modifiers.
    #[must_use]
    pub const fn reset() -> Self {
        Self {
            fg: Some(Color::Reset),
            bg: Some(Color::Reset),
            add_modifiers: Modifiers::NONE,
            sub_modifiers: Modifiers::ALL,
        }
    }

    /// Sets the foreground color.
    #[must_use]
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Sets the background color.
    #[must_use]
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Switches the given modifiers on.
    #[must_use]
    pub const fn add_modifier(mut self, modifiers: Modifiers) -> Self {
        self.add_modifiers = self.add_modifiers.union(modifiers);
        self.sub_modifiers = self.sub_modifiers.difference(modifiers);
        self
    }

    /// Switches the given modifiers off.
    #[must_use]
    pub const fn remove_modifier(mut self, modifiers: Modifiers) -> Self {
        self.sub_modifiers = self.sub_modifiers.union(modifiers);
        self.add_modifiers = self.add_modifiers.difference(modifiers);
        self
    }

    /// Switches bold on.
    #[must_use]
    pub const fn bold(self) -> Self {
        self.add_modifier(Modifiers::BOLD)
    }

    /// Switches dim on.
    #[must_use]
    pub const fn dim(self) -> Self {
        self.add_modifier(Modifiers::DIM)
    }

    /// Switches italic on.
    #[must_use]
    pub const fn italic(self) -> Self {
        self.add_modifier(Modifiers::ITALIC)
    }

    /// Switches underline on.
    #[must_use]
    pub const fn underline(self) -> Self {
        self.add_modifier(Modifiers::UNDERLINE)
    }

    /// Switches blink on.
    #[must_use]
    pub const fn blink(self) -> Self {
        self.add_modifier(Modifiers::BLINK)
    }

    /// Switches reverse on.
    #[must_use]
    pub const fn reverse(self) -> Self {
        self.add_modifier(Modifiers::REVERSE)
    }

    /// Switches hidden on.
    #[must_use]
    pub const fn hidden(self) -> Self {
        self.add_modifier(Modifiers::HIDDEN)
    }

    /// Switches strikethrough on.
    #[must_use]
    pub const fn strikethrough(self) -> Self {
        self.add_modifier(Modifiers::STRIKETHROUGH)
    }

    /// Applies `other` on top of `self`: fields set in `other` win.
    #[must_use]
    pub const fn patch(mut self, other: Self) -> Self {
        if let Some(fg) = other.fg {
            self.fg = Some(fg);
        }
        if let Some(bg) = other.bg {
            self.bg = Some(bg);
        }
        self.add_modifiers = self
            .add_modifiers
            .difference(other.sub_modifiers)
            .union(other.add_modifiers);
        self.sub_modifiers = self
            .sub_modifiers
            .difference(other.add_modifiers)
            .union(other.sub_modifiers);
        self
    }
}

impl From<Color> for Style {
    /// A style with only the foreground color set.
    fn from(color: Color) -> Self {
        Self::new().fg(color)
    }
}

impl From<Modifiers> for Style {
    /// A style that switches the given modifiers on.
    fn from(modifiers: Modifiers) -> Self {
        Self::new().add_modifier(modifiers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_set_operations() {
        let mut m = Modifiers::BOLD | Modifiers::ITALIC;
        assert!(m.contains(Modifiers::BOLD));
        assert!(!m.contains(Modifiers::BOLD | Modifiers::DIM));
        assert!(m.intersects(Modifiers::BOLD | Modifiers::DIM));
        m.remove(Modifiers::BOLD);
        assert_eq!(m, Modifiers::ITALIC);
        m |= Modifiers::UNDERLINE;
        assert_eq!(m - Modifiers::ITALIC, Modifiers::UNDERLINE);
        assert_eq!(!Modifiers::NONE, Modifiers::ALL);
        assert_eq!(Modifiers::from_bits_truncate(u16::MAX), Modifiers::ALL);
    }

    #[test]
    fn modifiers_iter_and_debug() {
        let m = Modifiers::STRIKETHROUGH | Modifiers::BOLD;
        let list: Vec<_> = m.iter().collect();
        assert_eq!(list, vec![Modifiers::BOLD, Modifiers::STRIKETHROUGH]);
        assert_eq!(format!("{m:?}"), "Modifiers(BOLD | STRIKETHROUGH)");
        assert_eq!(format!("{:?}", Modifiers::NONE), "Modifiers(NONE)");
    }

    #[test]
    fn style_builder() {
        let style = Style::new().fg(Color::Red).bold().italic();
        assert_eq!(style.fg, Some(Color::Red));
        assert_eq!(style.bg, None);
        assert_eq!(style.add_modifiers, Modifiers::BOLD | Modifiers::ITALIC);

        let style = style.remove_modifier(Modifiers::BOLD);
        assert_eq!(style.add_modifiers, Modifiers::ITALIC);
        assert_eq!(style.sub_modifiers, Modifiers::BOLD);
    }

    #[test]
    fn style_patch() {
        let base = Style::new().fg(Color::Red).bg(Color::Blue).bold();
        let top = Style::new()
            .fg(Color::Green)
            .remove_modifier(Modifiers::BOLD)
            .italic();
        let patched = base.patch(top);
        assert_eq!(patched.fg, Some(Color::Green));
        assert_eq!(patched.bg, Some(Color::Blue));
        assert_eq!(patched.add_modifiers, Modifiers::ITALIC);
        assert_eq!(patched.sub_modifiers, Modifiers::BOLD);
    }
}
