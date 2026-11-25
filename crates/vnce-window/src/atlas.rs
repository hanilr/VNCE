//! The glyph atlas: every cell picture used so far, packed in rows into one texture.

use std::collections::HashMap;

use crate::glyphs::{Font, Glyph, GlyphStyle};

/// Side of the atlas texture in pixels.
pub const ATLAS_SIZE: u32 = 2048;

/// Where a glyph is in the atlas; an empty slot (zero width) is a blank glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Slot {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    symbol: String,
    style: GlyphStyle,
    columns: u8,
}

/// The packed glyphs and the pictures still to be copied to the texture.
#[derive(Debug, Default)]
pub struct Atlas {
    slots: HashMap<Key, Slot>,
    /// Where the next glyph goes: the current row and its height.
    x: u32,
    y: u32,
    row: u32,
    uploads: Vec<(Slot, Glyph)>,
    /// Set when the atlas was emptied to make room (slots handed out before are gone).
    pub cleared: bool,
}

impl Atlas {
    /// The slot of `symbol`, rasterizing it the first time.
    pub fn slot(&mut self, font: &Font, symbol: &str, style: GlyphStyle, columns: u8) -> Slot {
        let key = Key {
            symbol: symbol.to_owned(),
            style,
            columns,
        };
        if let Some(&slot) = self.slots.get(&key) {
            return slot;
        }
        let glyph = font.glyph(symbol, style, u32::from(columns));
        let slot = if glyph.is_empty() {
            Slot::default()
        } else {
            let slot = self.place(glyph.width, glyph.height);
            self.uploads.push((slot, glyph));
            slot
        };
        self.slots.insert(key, slot);
        slot
    }

    /// Room for a `width` × `height` picture: the next place in the row, else a new row,
    /// else a fresh atlas.
    fn place(&mut self, width: u32, height: u32) -> Slot {
        if self.x + width > ATLAS_SIZE {
            self.x = 0;
            self.y += self.row;
            self.row = 0;
        }
        if self.y + height > ATLAS_SIZE {
            self.clear();
            self.cleared = true;
        }
        let slot = Slot {
            x: self.x as u16,
            y: self.y as u16,
            width: width as u16,
            height: height as u16,
        };
        self.x += width;
        self.row = self.row.max(height);
        slot
    }

    /// Forgets every glyph (after the cell size changed, or when full).
    pub fn clear(&mut self) {
        self.slots.clear();
        self.uploads.clear();
        self.x = 0;
        self.y = 0;
        self.row = 0;
    }

    /// The pictures placed since the last call, to copy into the texture.
    pub fn take_uploads(&mut self) -> Vec<(Slot, Glyph)> {
        std::mem::take(&mut self.uploads)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_packed_once_without_overlap() {
        let font = Font::new(16.0);
        let mut atlas = Atlas::default();
        let style = GlyphStyle::default();
        let a = atlas.slot(&font, "a", style, 1);
        let b = atlas.slot(&font, "b", style, 1);
        assert_eq!(atlas.slot(&font, "a", style, 1), a);
        assert_ne!(a, b);
        assert!(a.x + a.width <= b.x || b.x + b.width <= a.x || a.y != b.y);
        // Blanks take no room; bold is a glyph of its own.
        assert_eq!(atlas.slot(&font, " ", style, 1).width, 0);
        let bold = atlas.slot(
            &font,
            "a",
            GlyphStyle {
                bold: true,
                italic: false,
            },
            1,
        );
        assert_ne!(bold, a);
        assert_eq!(atlas.take_uploads().len(), 3);
        assert_eq!(atlas.take_uploads().len(), 0);
        assert_eq!(atlas.len(), 4);
    }

    #[test]
    fn a_full_atlas_starts_over() {
        let font = Font::new(64.0);
        let mut atlas = Atlas::default();
        let style = GlyphStyle::default();
        for code in 0x4E00_u32..0x4E00 + 3000 {
            let symbol = char::from_u32(code).unwrap().to_string();
            atlas.slot(&font, &symbol, style, 2);
            if atlas.cleared {
                break;
            }
        }
        assert!(atlas.cleared, "the atlas filled up");
    }
}
