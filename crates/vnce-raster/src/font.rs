//! Bitmap fonts: text drawn as pixels on a [`Canvas`].
//!
//! [`Font::builtin`] is a 5×7 pixel font with every printable ASCII character and the
//! Turkish letters `Ç ç Ğ ğ İ ı Ö ö Ş ş Ü ü`. Your own fonts can be made from text art
//! with [`Font::from_art`]. Characters without a glyph are drawn as a box.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::canvas::Canvas;
use crate::color::Rgba;

/// A fixed-width bitmap font.
///
/// ```
/// use vnce_raster::{Canvas, Font, Rgba};
///
/// let font = Font::builtin();
/// let mut canvas = Canvas::new(64, 16);
/// canvas.text(font, 1, 1, "Score 42", Rgba::WHITE);
/// assert_eq!(font.measure("Score 42"), (47, 7));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    width: u32,
    height: u32,
    letter_spacing: u32,
    line_spacing: u32,
    /// The pixels of every glyph, row by row (`width * height` values each).
    glyphs: HashMap<char, Vec<bool>>,
    /// Drawn for characters without a glyph.
    fallback: Vec<bool>,
}

impl Font {
    /// The built-in 5×7 font (1 pixel between letters and between lines).
    #[must_use]
    pub fn builtin() -> &'static Self {
        static FONT: OnceLock<Font> = OnceLock::new();
        FONT.get_or_init(|| Self::from_art(5, 7, BUILTIN))
    }

    /// Builds a font from text art. Each glyph is a string of rows separated by
    /// whitespace; `#` is a lit pixel and every other character is empty. Rows and glyphs
    /// that are too short are padded, longer ones are cut. Later entries replace earlier
    /// ones for the same character.
    ///
    /// ```
    /// use vnce_raster::Font;
    ///
    /// // A tiny 3×3 font with two glyphs.
    /// let font = Font::from_art(3, 3, &[('+', ".#. ### .#."), ('-', "... ### ...")]);
    /// assert!(font.has_glyph('+'));
    /// assert!(!font.has_glyph('x'));
    /// assert_eq!(font.measure("+-+"), (11, 3));
    /// ```
    #[must_use]
    pub fn from_art(width: u32, height: u32, glyphs: &[(char, &str)]) -> Self {
        let parse = |art: &str| {
            let mut rows = art.split_whitespace();
            let mut pixels = Vec::with_capacity(pixel_count(width, height));
            for _ in 0..height {
                let mut row = rows.next().unwrap_or("").chars();
                pixels.extend((0..width).map(|_| row.next() == Some('#')));
            }
            pixels
        };
        // A box filling the glyph cell.
        let fallback = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| x == 0 || y == 0 || x + 1 == width || y + 1 == height)
            .collect();
        Self {
            width,
            height,
            letter_spacing: 1,
            line_spacing: 1,
            glyphs: glyphs.iter().map(|&(ch, art)| (ch, parse(art))).collect(),
            fallback,
        }
    }

    /// The same font with other spacing: `letter` pixels between glyphs, `line` pixels
    /// between lines.
    #[must_use]
    pub fn with_spacing(mut self, letter: u32, line: u32) -> Self {
        self.letter_spacing = letter;
        self.line_spacing = line;
        self
    }

    /// The size of one glyph in pixels (`(width, height)`).
    #[must_use]
    pub const fn glyph_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Pixels between glyphs and between lines (`(letter, line)`).
    #[must_use]
    pub const fn spacing(&self) -> (u32, u32) {
        (self.letter_spacing, self.line_spacing)
    }

    /// Whether `ch` has its own glyph (otherwise it is drawn as a box).
    #[must_use]
    pub fn has_glyph(&self, ch: char) -> bool {
        self.glyphs.contains_key(&ch)
    }

    /// The size of `text` in pixels at scale 1 (`(width, height)`). Lines are separated by
    /// `\n`; `\r` is ignored. Multiply by the scale for [`Canvas::text_scaled`].
    #[must_use]
    pub fn measure(&self, text: &str) -> (u32, u32) {
        if text.is_empty() {
            return (0, 0);
        }
        let mut lines = 0_u32;
        let mut widest = 0_u32;
        for line in text.split('\n') {
            lines = lines.saturating_add(1);
            let chars =
                u32::try_from(line.chars().filter(|&ch| ch != '\r').count()).unwrap_or(u32::MAX);
            widest = widest.max(self.line_width(chars));
        }
        let height = lines
            .saturating_mul(self.height.saturating_add(self.line_spacing))
            .saturating_sub(self.line_spacing);
        (widest, height)
    }

    fn line_width(&self, chars: u32) -> u32 {
        chars
            .saturating_mul(self.width.saturating_add(self.letter_spacing))
            .saturating_sub(self.letter_spacing)
    }

    fn glyph(&self, ch: char) -> &[bool] {
        self.glyphs.get(&ch).unwrap_or(&self.fallback)
    }
}

impl Default for Font {
    /// A copy of [`Font::builtin`].
    fn default() -> Self {
        Self::builtin().clone()
    }
}

impl Canvas {
    /// Draws `text` with its top-left corner at `(x, y)`. Lines are separated by `\n`.
    /// Text follows the clip rectangle and the blend mode like every other shape.
    pub fn text(&mut self, font: &Font, x: i32, y: i32, text: &str, color: Rgba) {
        self.text_scaled(font, x, y, text, color, 1);
    }

    /// Draws `text` with every font pixel as a `scale` × `scale` square, for titles.
    /// A scale of `0` draws nothing.
    pub fn text_scaled(
        &mut self,
        font: &Font,
        x: i32,
        y: i32,
        text: &str,
        color: Rgba,
        scale: u32,
    ) {
        if scale == 0 {
            return;
        }
        let scale = i64::from(scale);
        let advance_x = i64::from(font.width) + i64::from(font.letter_spacing);
        let advance_y = i64::from(font.height) + i64::from(font.line_spacing);
        let mut top = i64::from(y);
        for line in text.split('\n') {
            let mut left = i64::from(x);
            for ch in line.chars().filter(|&ch| ch != '\r') {
                if ch != ' ' {
                    self.glyph(font, font.glyph(ch), left, top, scale, color);
                }
                left += advance_x * scale;
            }
            top += advance_y * scale;
        }
    }

    fn glyph(
        &mut self,
        font: &Font,
        pixels: &[bool],
        left: i64,
        top: i64,
        scale: i64,
        color: Rgba,
    ) {
        let lit = (0..font.height)
            .flat_map(|gy| (0..font.width).map(move |gx| (gx, gy)))
            .zip(pixels)
            .filter_map(|(at, &on)| on.then_some(at));
        for (gx, gy) in lit {
            let gx = left + i64::from(gx) * scale;
            let gy = top + i64::from(gy) * scale;
            // Pixels far outside the `i32` range cannot be visible.
            let (Ok(px), Ok(py)) = (i32::try_from(gx), i32::try_from(gy)) else {
                continue;
            };
            if scale == 1 {
                self.pixel(px, py, color);
            } else {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the scale comes from a u32"
                )]
                let size = scale as u32;
                self.fill_rect(px, py, size, size, color);
            }
        }
    }
}

fn pixel_count(width: u32, height: u32) -> usize {
    (width as usize).saturating_mul(height as usize)
}

/// The glyphs of the built-in 5×7 font: seven rows of five pixels each. Lowercase letters
/// with descenders (`g j p q y`) sit one or two rows higher, as in most 5×7 fonts.
const BUILTIN: &[(char, &str)] = &[
    (' ', "..... ..... ..... ..... ..... ..... ....."),
    ('!', "..#.. ..#.. ..#.. ..#.. ..#.. ..... ..#.."),
    ('"', ".#.#. .#.#. ..... ..... ..... ..... ....."),
    ('#', ".#.#. .#.#. ##### .#.#. ##### .#.#. .#.#."),
    ('$', "..#.. .#### #.#.. .###. ..#.# ####. ..#.."),
    ('%', "##... ##..# ...#. ..#.. .#... #..## ...##"),
    ('&', ".##.. #..#. #.#.. .#... #.#.# #..#. .##.#"),
    ('\'', "..#.. ..#.. ..... ..... ..... ..... ....."),
    ('(', "...#. ..#.. .#... .#... .#... ..#.. ...#."),
    (')', ".#... ..#.. ...#. ...#. ...#. ..#.. .#..."),
    ('*', "..... ..#.. #.#.# .###. #.#.# ..#.. ....."),
    ('+', "..... ..#.. ..#.. ##### ..#.. ..#.. ....."),
    (',', "..... ..... ..... ..... .##.. ..#.. .#..."),
    ('-', "..... ..... ..... ##### ..... ..... ....."),
    ('.', "..... ..... ..... ..... ..... .##.. .##.."),
    ('/', "..... ....# ...#. ..#.. .#... #.... ....."),
    ('0', ".###. #...# #..## #.#.# ##..# #...# .###."),
    ('1', "..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###."),
    ('2', ".###. #...# ....# ...#. ..#.. .#... #####"),
    ('3', "##### ...#. ..#.. ...#. ....# #...# .###."),
    ('4', "...#. ..##. .#.#. #..#. ##### ...#. ...#."),
    ('5', "##### #.... ####. ....# ....# #...# .###."),
    ('6', "..##. .#... #.... ####. #...# #...# .###."),
    ('7', "##### ....# ...#. ..#.. .#... .#... .#..."),
    ('8', ".###. #...# #...# .###. #...# #...# .###."),
    ('9', ".###. #...# #...# .#### ....# ...#. .##.."),
    (':', "..... .##.. .##.. ..... .##.. .##.. ....."),
    (';', "..... .##.. .##.. ..... .##.. ..#.. .#..."),
    ('<', "...#. ..#.. .#... #.... .#... ..#.. ...#."),
    ('=', "..... ..... ##### ..... ##### ..... ....."),
    ('>', ".#... ..#.. ...#. ....# ...#. ..#.. .#..."),
    ('?', ".###. #...# ....# ...#. ..#.. ..... ..#.."),
    ('@', ".###. #...# ....# .##.# #.#.# #.#.# .###."),
    ('A', ".###. #...# #...# ##### #...# #...# #...#"),
    ('B', "####. #...# #...# ####. #...# #...# ####."),
    ('C', ".###. #...# #.... #.... #.... #...# .###."),
    ('D', "####. #...# #...# #...# #...# #...# ####."),
    ('E', "##### #.... #.... ####. #.... #.... #####"),
    ('F', "##### #.... #.... ####. #.... #.... #...."),
    ('G', ".###. #...# #.... #.### #...# #...# .####"),
    ('H', "#...# #...# #...# ##### #...# #...# #...#"),
    ('I', ".###. ..#.. ..#.. ..#.. ..#.. ..#.. .###."),
    ('J', "..### ...#. ...#. ...#. ...#. #..#. .##.."),
    ('K', "#...# #..#. #.#.. ##... #.#.. #..#. #...#"),
    ('L', "#.... #.... #.... #.... #.... #.... #####"),
    ('M', "#...# ##.## #.#.# #.#.# #...# #...# #...#"),
    ('N', "#...# #...# ##..# #.#.# #..## #...# #...#"),
    ('O', ".###. #...# #...# #...# #...# #...# .###."),
    ('P', "####. #...# #...# ####. #.... #.... #...."),
    ('Q', ".###. #...# #...# #...# #.#.# #..#. .##.#"),
    ('R', "####. #...# #...# ####. #.#.. #..#. #...#"),
    ('S', ".#### #.... #.... .###. ....# ....# ####."),
    ('T', "##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#.."),
    ('U', "#...# #...# #...# #...# #...# #...# .###."),
    ('V', "#...# #...# #...# #...# #...# .#.#. ..#.."),
    ('W', "#...# #...# #...# #.#.# #.#.# #.#.# .#.#."),
    ('X', "#...# #...# .#.#. ..#.. .#.#. #...# #...#"),
    ('Y', "#...# #...# .#.#. ..#.. ..#.. ..#.. ..#.."),
    ('Z', "##### ....# ...#. ..#.. .#... #.... #####"),
    ('[', ".###. .#... .#... .#... .#... .#... .###."),
    ('\\', "..... #.... .#... ..#.. ...#. ....# ....."),
    (']', ".###. ...#. ...#. ...#. ...#. ...#. .###."),
    ('^', "..#.. .#.#. #...# ..... ..... ..... ....."),
    ('_', "..... ..... ..... ..... ..... ..... #####"),
    ('`', ".#... ..#.. ..... ..... ..... ..... ....."),
    ('a', "..... ..... .###. ....# .#### #...# .####"),
    ('b', "#.... #.... #.##. ##..# #...# #...# ####."),
    ('c', "..... ..... .###. #.... #.... #...# .###."),
    ('d', "....# ....# .##.# #..## #...# #...# .####"),
    ('e', "..... ..... .###. #...# ##### #.... .###."),
    ('f', "..##. .#..# .#... ###.. .#... .#... .#..."),
    ('g', "..... .#### #...# #...# .#### ....# .###."),
    ('h', "#.... #.... #.##. ##..# #...# #...# #...#"),
    ('i', "..#.. ..... .##.. ..#.. ..#.. ..#.. .###."),
    ('j', "...#. ..... ..##. ...#. ...#. #..#. .##.."),
    ('k', "#.... #.... #..#. #.#.. ##... #.#.. #..#."),
    ('l', ".##.. ..#.. ..#.. ..#.. ..#.. ..#.. .###."),
    ('m', "..... ..... ##.#. #.#.# #.#.# #...# #...#"),
    ('n', "..... ..... #.##. ##..# #...# #...# #...#"),
    ('o', "..... ..... .###. #...# #...# #...# .###."),
    ('p', "..... ..... ####. #...# ####. #.... #...."),
    ('q', "..... ..... .##.# #..## .#### ....# ....#"),
    ('r', "..... ..... #.##. ##..# #.... #.... #...."),
    ('s', "..... ..... .###. #.... .###. ....# ####."),
    ('t', ".#... .#... ###.. .#... .#... .#..# ..##."),
    ('u', "..... ..... #...# #...# #...# #..## .##.#"),
    ('v', "..... ..... #...# #...# #...# .#.#. ..#.."),
    ('w', "..... ..... #...# #...# #.#.# #.#.# .#.#."),
    ('x', "..... ..... #...# .#.#. ..#.. .#.#. #...#"),
    ('y', "..... ..... #...# #...# .#### ....# .###."),
    ('z', "..... ..... ##### ...#. ..#.. .#... #####"),
    ('{', "...#. ..#.. ..#.. .#... ..#.. ..#.. ...#."),
    ('|', "..#.. ..#.. ..#.. ..#.. ..#.. ..#.. ..#.."),
    ('}', ".#... ..#.. ..#.. ...#. ..#.. ..#.. .#..."),
    ('~', "..... ..... .#... #.#.# ...#. ..... ....."),
    // Turkish letters. Capitals with a mark above are one or two rows shorter.
    ('Ç', ".###. #...# #.... #.... #...# .###. ..#.."),
    ('ç', "..... .###. #.... #.... #...# .###. ..#.."),
    ('Ğ', ".###. ..... .###. #.... #.### #...# .###."),
    ('ğ', ".###. ..... .#### #...# .#### ....# .###."),
    ('İ', "..#.. ..... .###. ..#.. ..#.. ..#.. .###."),
    ('ı', "..... ..... .##.. ..#.. ..#.. ..#.. .###."),
    ('Ö', ".#.#. ..... .###. #...# #...# #...# .###."),
    ('ö', "..... .#.#. ..... .###. #...# #...# .###."),
    ('Ş', ".#### #.... .###. ....# ....# ####. ..#.."),
    ('ş', "..... .###. #.... .###. ....# ####. ..#.."),
    ('Ü', ".#.#. ..... #...# #...# #...# #...# .###."),
    ('ü', "..... .#.#. ..... #...# #...# #..## .##.#"),
];

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba::rgb(255, 0, 0);

    /// The lit pixels of a canvas as text art (`#` = lit).
    fn art(canvas: &Canvas) -> Vec<String> {
        (0..canvas.height())
            .map(|y| {
                (0..canvas.width())
                    .map(|x| {
                        if canvas.get(x, y).is_some_and(|p| p.a > 0) {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn builtin_table_is_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for &(ch, glyph) in BUILTIN {
            assert!(seen.insert(ch), "{ch:?} is defined twice");
            let rows: Vec<&str> = glyph.split_whitespace().collect();
            assert_eq!(rows.len(), 7, "{ch:?} needs 7 rows");
            for row in rows {
                assert_eq!(row.len(), 5, "{ch:?} rows need 5 pixels");
                assert!(row.chars().all(|c| c == '#' || c == '.'), "{ch:?}: {row}");
            }
        }
    }

    #[test]
    fn builtin_covers_ascii_and_turkish() {
        let font = Font::builtin();
        for ch in (32_u8..=126).map(char::from).chain("ÇçĞğİıÖöŞşÜü".chars()) {
            assert!(font.has_glyph(ch), "missing {ch:?}");
        }
        assert!(!font.has_glyph('€'));
    }

    #[test]
    fn draws_a_glyph() {
        let mut canvas = Canvas::new(5, 7);
        canvas.text(Font::builtin(), 0, 0, "T", RED);
        assert_eq!(art(&canvas)[0], "#####");
        assert_eq!(art(&canvas)[6], "..#..");
        assert_eq!(canvas.get(2, 3), Some(RED));
    }

    #[test]
    fn unknown_characters_are_boxes() {
        let mut canvas = Canvas::new(5, 7);
        canvas.text(Font::builtin(), 0, 0, "€", RED);
        let art = art(&canvas);
        assert_eq!(art[0], "#####");
        assert_eq!(art[3], "#...#");
        assert_eq!(art[6], "#####");
    }

    #[test]
    fn measures_lines() {
        let font = Font::builtin();
        assert_eq!(font.measure(""), (0, 0));
        assert_eq!(font.measure("A"), (5, 7));
        assert_eq!(font.measure("AB"), (11, 7));
        assert_eq!(font.measure("AB\nC"), (11, 15));
        assert_eq!(font.measure("A\r\nB"), (5, 15));
        assert_eq!(font.measure("ş"), (5, 7));
        let wide = font.clone().with_spacing(2, 3);
        assert_eq!(wide.measure("AB\nC"), (12, 17));
        assert_eq!(wide.spacing(), (2, 3));
    }

    #[test]
    fn second_line_starts_below_the_first() {
        let mut canvas = Canvas::new(5, 15);
        canvas.text(Font::builtin(), 0, 0, "_\n_", RED);
        let art = art(&canvas);
        assert_eq!(art[6], "#####");
        assert_eq!(art[7], ".....");
        assert_eq!(art[14], "#####");
    }

    #[test]
    fn scaled_text_doubles_every_pixel() {
        let mut canvas = Canvas::new(10, 14);
        canvas.text_scaled(Font::builtin(), 0, 0, "T", RED, 2);
        let art = art(&canvas);
        assert_eq!(art[0], "##########");
        assert_eq!(art[1], "##########");
        assert_eq!(art[13], "....##....");
        let mut empty = Canvas::new(10, 14);
        empty.text_scaled(Font::builtin(), 0, 0, "T", RED, 0);
        assert!(empty.pixels().iter().all(|p| p.a == 0));
    }

    #[test]
    fn text_is_clipped() {
        let mut canvas = Canvas::new(8, 8);
        canvas.text(Font::builtin(), -3, -3, "W", RED);
        canvas.text(Font::builtin(), 6, 6, "W", RED);
        canvas.text(Font::builtin(), i32::MAX - 2, i32::MIN, "WWW", RED);
        canvas.text_scaled(Font::builtin(), i32::MAX, 0, "W", RED, u32::MAX);
        assert_eq!(canvas.get(1, 0), Some(RED)); // bottom of the clipped W
        assert_eq!(canvas.get(6, 6), Some(RED));
    }

    #[test]
    fn spaces_draw_nothing() {
        let mut canvas = Canvas::new(20, 7);
        canvas.text(Font::builtin(), 0, 0, "   ", RED);
        assert!(canvas.pixels().iter().all(|p| p.a == 0));
    }

    #[test]
    fn custom_font() {
        let font = Font::from_art(3, 2, &[('x', "#.# .#."), ('x', "### ###"), ('y', "#")]);
        let mut canvas = Canvas::new(7, 2);
        canvas.text(&font, 0, 0, "xy", RED);
        assert_eq!(art(&canvas), ["###.#..", "###...."]);
        assert_eq!(font.glyph_size(), (3, 2));
    }
}
