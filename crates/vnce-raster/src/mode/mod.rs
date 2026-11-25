//! Pixel modes: converting a [`Canvas`] into terminal cells.

mod quantize;
mod tables;

use std::fmt;
use std::str::FromStr;

use vnce_core::{Buffer, Color, ColorDepth, Rect, Style};

use crate::canvas::Canvas;
use crate::color::{BlendMode, Rgba};

/// How pixels are mapped to terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelMode {
    /// 1×2 pixels per cell with `▀`; every pixel keeps its own color. Works everywhere.
    HalfBlock,
    /// 2×2 pixels per cell with quadrant blocks, two colors per cell.
    Quadrant,
    /// 2×3 pixels per cell with sextant blocks (Unicode 13), two colors per cell.
    Sextant,
    /// 2×4 pixels per cell with octant blocks (Unicode 16), two colors per cell.
    Octant,
    /// 2×4 pixels per cell with Braille dots: dot color and background color.
    Braille,
    /// 1×2 pixels per cell drawn as a character ramp (` .:-=+*#%@`); also works without
    /// color support.
    Ascii,
}

/// Characters of the ASCII ramp, from dark to bright.
const ASCII_RAMP: &[u8] = b" .:-=+*#%@";

/// 4×4 Bayer matrix for ordered dithering (values 0..16).
const BAYER4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

impl PixelMode {
    /// Every mode.
    pub const ALL: [Self; 6] = [
        Self::HalfBlock,
        Self::Quadrant,
        Self::Sextant,
        Self::Octant,
        Self::Braille,
        Self::Ascii,
    ];

    /// Pixels per cell: `(columns, rows)`.
    #[must_use]
    pub const fn cell_size(self) -> (u32, u32) {
        match self {
            Self::HalfBlock | Self::Ascii => (1, 2),
            Self::Quadrant => (2, 2),
            Self::Sextant => (2, 3),
            Self::Octant | Self::Braille => (2, 4),
        }
    }

    /// Width / height of one pixel on screen, assuming terminal cells twice as tall as
    /// wide. Divide horizontal sizes by this value to draw round circles.
    #[must_use]
    pub fn pixel_aspect(self) -> f32 {
        let (columns, rows) = self.cell_size();
        // Cell width 1, cell height 2.
        #[allow(clippy::cast_precision_loss, reason = "cell sizes are tiny")]
        let aspect = (1.0 / columns as f32) / (2.0 / rows as f32);
        aspect
    }

    /// The name used by [`FromStr`] and `VNCE_PIXEL_MODE`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HalfBlock => "halfblock",
            Self::Quadrant => "quadrant",
            Self::Sextant => "sextant",
            Self::Octant => "octant",
            Self::Braille => "braille",
            Self::Ascii => "ascii",
        }
    }

    /// Chooses the best mode that is compatible with the current terminal:
    ///
    /// 1. `VNCE_PIXEL_MODE` (a mode name) overrides everything.
    /// 2. Without color support: [`PixelMode::Ascii`].
    /// 3. Kitty, `WezTerm`, foot and Ghostty draw block characters themselves, independent of
    ///    the font: [`PixelMode::Sextant`].
    /// 4. Everywhere else: [`PixelMode::HalfBlock`], which every common font supports.
    ///
    /// [`PixelMode::Octant`] and [`PixelMode::Braille`] are never chosen automatically.
    ///
    /// The result is always one of [`PixelMode::available`].
    #[must_use]
    pub fn detect(color_depth: ColorDepth) -> Self {
        Self::from_env(|name| std::env::var(name).ok(), color_depth, cfg!(windows))
    }

    /// Like [`PixelMode::detect`], with a custom environment lookup (for tests).
    ///
    /// `windows` tells whether the program runs on Windows.
    pub fn from_env(
        env: impl Fn(&str) -> Option<String>,
        color_depth: ColorDepth,
        windows: bool,
    ) -> Self {
        let available = Self::available_from_env(env, color_depth, windows);
        if let [explicit] = available[..] {
            return explicit;
        }
        [Self::Sextant, Self::HalfBlock, Self::Ascii]
            .into_iter()
            .find(|mode| available.contains(mode))
            .unwrap_or(Self::Ascii)
    }

    /// The modes expected to display correctly on the current terminal, best first.
    ///
    /// - [`PixelMode::HalfBlock`] and [`PixelMode::Quadrant`] (needs colors) and
    ///   [`PixelMode::Ascii`]: everywhere; these characters are in practically every font.
    /// - [`PixelMode::Sextant`]: only on terminals that draw block characters themselves
    ///   (Kitty, `WezTerm`, foot, Ghostty), because almost no font contains them.
    /// - [`PixelMode::Braille`]: everywhere except the classic Windows console and the Linux
    ///   console, which have no font fallback for the Braille block.
    /// - [`PixelMode::Octant`] (Unicode 16, 2024): never; almost no font or terminal can draw
    ///   it yet.
    ///
    /// When `VNCE_PIXEL_MODE` names a mode, only that mode is returned: the user knows their
    /// terminal can display it.
    #[must_use]
    pub fn available(color_depth: ColorDepth) -> Vec<Self> {
        Self::available_from_env(|name| std::env::var(name).ok(), color_depth, cfg!(windows))
    }

    /// Like [`PixelMode::available`], with a custom environment lookup (for tests).
    pub fn available_from_env(
        env: impl Fn(&str) -> Option<String>,
        color_depth: ColorDepth,
        windows: bool,
    ) -> Vec<Self> {
        if let Some(mode) = env("VNCE_PIXEL_MODE").and_then(|v| v.parse().ok()) {
            return vec![mode];
        }
        let term = env("TERM").unwrap_or_default().to_ascii_lowercase();
        let program = env("TERM_PROGRAM").unwrap_or_default().to_ascii_lowercase();
        let draws_blocks = ["kitty", "wezterm", "foot", "ghostty"]
            .iter()
            .any(|name| term.contains(name) || program.contains(name));
        let classic_windows_console =
            windows && env("WT_SESSION").is_none() && program.is_empty() && term.is_empty();
        let braille = !(classic_windows_console || term == "linux" || term == "dumb");
        let colors = color_depth != ColorDepth::NoColor;

        let mut modes = Vec::new();
        if colors && draws_blocks {
            modes.push(Self::Sextant);
        }
        if colors {
            modes.extend([Self::HalfBlock, Self::Quadrant]);
        }
        if braille {
            modes.push(Self::Braille);
        }
        modes.push(Self::Ascii);
        modes
    }
}

/// The grid of a block symbol used by the pixel modes: its columns, rows and filled
/// blocks (bit `row * columns + column`), for the quadrant (2×2), sextant (2×3) and
/// octant (2×4) characters. A renderer that draws these symbols itself (instead of a
/// font) gets exactly the pixels the mode meant. Returns the finest grid for symbols
/// shared by several modes; `None` for other symbols.
#[must_use]
pub fn block_pattern(symbol: char) -> Option<(u8, u8, u8)> {
    if symbol == ' ' {
        return None;
    }
    let find = |table: &[char]| table.iter().position(|&c| c == symbol);
    if let Some(bits) = find(&tables::OCTANT) {
        return Some((2, 4, u8::try_from(bits).ok()?));
    }
    if let Some(bits) = find(&tables::SEXTANT) {
        return Some((2, 3, u8::try_from(bits).ok()?));
    }
    find(&tables::QUADRANT).and_then(|bits| Some((2, 2, u8::try_from(bits).ok()?)))
}

impl fmt::Display for PixelMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for PixelMode {
    type Err = ParsePixelModeError;

    /// Parses a mode name (case-insensitive, `-` and `_` ignored), e.g. `half-block`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let name: String = s
            .trim()
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' '))
            .map(|c| c.to_ascii_lowercase())
            .collect();
        Self::ALL
            .into_iter()
            .find(|mode| mode.name() == name)
            .ok_or_else(|| ParsePixelModeError(s.to_owned()))
    }
}

/// Error returned when a string is not a pixel mode name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePixelModeError(String);

impl fmt::Display for ParsePixelModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown pixel mode `{}`", self.0)
    }
}

impl std::error::Error for ParsePixelModeError {}

/// Speed / quality trade-off of the two-color modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Quality {
    /// Fast clustering for 2×4 cells (octant, braille); exact for smaller cells.
    #[default]
    Fast,
    /// Exhaustive search for every mode.
    Exact,
}

/// Options for [`Canvas::render`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderOptions {
    /// Speed / quality trade-off of the two-color modes.
    pub quality: Quality,
    /// Ordered (Bayer) dithering for [`PixelMode::Braille`] and [`PixelMode::Ascii`].
    pub dither: bool,
    /// Color behind semi-transparent pixels when the cell has no RGB background of its
    /// own. Default: black.
    pub background: Rgba,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            quality: Quality::Fast,
            dither: false,
            background: Rgba::BLACK,
        }
    }
}

impl Canvas {
    /// Creates a transparent canvas covering `area` cells in `mode`.
    #[must_use]
    pub fn for_area(area: Rect, mode: PixelMode) -> Self {
        let (columns, rows) = mode.cell_size();
        Self::new(
            u32::from(area.width) * columns,
            u32::from(area.height) * rows,
        )
    }

    /// Converts the canvas into cells of `buffer`, with pixel `(0, 0)` at the top-left cell
    /// of `area`.
    ///
    /// Cells whose pixels are all transparent are left untouched, so pixels can be drawn
    /// over text. Semi-transparent pixels are composited over the cell's RGB background (or
    /// [`RenderOptions::background`]).
    pub fn render(
        &self,
        buffer: &mut Buffer,
        area: Rect,
        mode: PixelMode,
        options: &RenderOptions,
    ) {
        let area = area.intersection(buffer.area());
        let (columns, rows) = mode.cell_size();
        let mut pixels = [Rgba::TRANSPARENT; 8];
        let count = usize::try_from(columns * rows).unwrap_or(8);
        for row in 0..area.height {
            for column in 0..area.width {
                let px = u32::from(column) * columns;
                let py = u32::from(row) * rows;
                if px >= self.width() || py >= self.height() {
                    continue;
                }
                for (k, pixel) in pixels.iter_mut().take(count).enumerate() {
                    let k = u32::try_from(k).unwrap_or(0);
                    *pixel = self
                        .get(px + k % columns, py + k / columns)
                        .unwrap_or(Rgba::TRANSPARENT);
                }
                let cell_pixels = &mut pixels[..count];
                if cell_pixels.iter().all(|p| p.is_transparent()) {
                    continue;
                }
                let (x, y) = (area.x + column, area.y + row);
                let base = match buffer.cell(x, y).map(|cell| cell.bg) {
                    Some(Color::Rgb(red, green, blue)) => Rgba::rgb(red, green, blue),
                    _ => options.background,
                };
                for pixel in cell_pixels.iter_mut() {
                    *pixel = base.blend(*pixel, BlendMode::Alpha);
                }
                let (symbol, fg, bg) = cell_for(mode, cell_pixels, (px, py), *options);
                let mut utf8 = [0; 4];
                buffer.set_symbol(
                    x,
                    y,
                    symbol.encode_utf8(&mut utf8),
                    Style::reset().fg(fg.to_color()).bg(bg.to_color()),
                );
            }
        }
    }
}

/// The character and colors of one cell. `pixels` are opaque, row by row; `origin` is the
/// canvas position of the first pixel (for dithering).
fn cell_for(
    mode: PixelMode,
    pixels: &[Rgba],
    origin: (u32, u32),
    options: RenderOptions,
) -> (char, Rgba, Rgba) {
    let exact = options.quality == Quality::Exact;
    match mode {
        PixelMode::HalfBlock => {
            let (top, bottom) = (pixels[0], pixels[1]);
            if top == bottom {
                (' ', top, top)
            } else {
                ('▀', top, bottom)
            }
        }
        PixelMode::Quadrant => blocks(pixels, &tables::QUADRANT, true),
        PixelMode::Sextant => blocks(pixels, &tables::SEXTANT, true),
        PixelMode::Octant => blocks(pixels, &tables::OCTANT, exact),
        PixelMode::Braille => braille(pixels, origin, options.dither, exact),
        PixelMode::Ascii => ascii(pixels, origin, options.dither),
    }
}

/// Two-color block modes. Uniform cells become a space with a background color, so they
/// never depend on the font.
fn blocks(pixels: &[Rgba], table: &[char], exact: bool) -> (char, Rgba, Rgba) {
    let split = quantize::split(pixels, exact);
    let full = (1u32 << pixels.len()) - 1;
    match split.mask {
        0 => (' ', split.bg, split.bg),
        mask if mask == full => (' ', split.fg, split.fg),
        mask => (
            table[usize::try_from(mask).unwrap_or(0)],
            split.fg,
            split.bg,
        ),
    }
}

/// Braille bit of the pixel at `(column, row)` in a 2×4 cell.
fn braille_bit(column: u32, row: u32) -> u32 {
    match (column, row) {
        (0, 3) => 6,
        (1, 3) => 7,
        (0, r) => r,
        (_, r) => 3 + r,
    }
}

fn braille(pixels: &[Rgba], origin: (u32, u32), dither: bool, exact: bool) -> (char, Rgba, Rgba) {
    // Which pixels are dots (bit k = pixel k, row by row).
    let (dots, fg, bg) = if dither {
        let mut dots = 0u32;
        for (k, p) in (0u32..).zip(pixels) {
            if p.luminance() * 16.0 > bayer(origin.0 + k % 2, origin.1 + k / 2) + 0.5 {
                dots |= 1 << k;
            }
        }
        let on = average(pixels, |k| dots & (1 << k) != 0);
        let off = average(pixels, |k| dots & (1 << k) == 0);
        (dots, on.unwrap_or(Rgba::BLACK), off.unwrap_or(Rgba::BLACK))
    } else {
        let split = quantize::split(pixels, exact);
        (split.mask, split.fg, split.bg)
    };
    let mut code = 0u32;
    for k in 0..8u32 {
        if dots & (1 << k) != 0 {
            code |= 1 << braille_bit(k % 2, k / 2);
        }
    }
    if code == 0 {
        return (' ', bg, bg);
    }
    let symbol = char::from_u32(0x2800 + code).unwrap_or(' ');
    (symbol, fg, bg)
}

fn ascii(pixels: &[Rgba], origin: (u32, u32), dither: bool) -> (char, Rgba, Rgba) {
    let color = average(pixels, |_| true).unwrap_or(Rgba::BLACK);
    let last = ASCII_RAMP.len() - 1;
    #[allow(clippy::cast_precision_loss, reason = "the ramp is tiny")]
    let steps = last as f32;
    let mut level = color.luminance() * steps;
    if dither {
        level += (bayer(origin.0, origin.1 / 2) + 0.5) / 16.0 - 0.5;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the level is clamped to the ramp"
    )]
    let index = (level.round().clamp(0.0, steps) as usize).min(last);
    (char::from(ASCII_RAMP[index]), color, Rgba::BLACK)
}

/// Threshold of the 4×4 Bayer matrix at a canvas position (0..16).
fn bayer(x: u32, y: u32) -> f32 {
    f32::from(BAYER4[(y % 4) as usize][(x % 4) as usize])
}

/// Mean color of the selected pixels.
fn average(pixels: &[Rgba], selected: impl Fn(u32) -> bool) -> Option<Rgba> {
    let mut sum = [0u32; 3];
    let mut count = 0;
    for (k, p) in (0u32..).zip(pixels) {
        if selected(k) {
            sum[0] += u32::from(p.r);
            sum[1] += u32::from(p.g);
            sum[2] += u32::from(p.b);
            count += 1;
        }
    }
    (count > 0).then(|| {
        let channel = |s: u32| u8::try_from((s + count / 2) / count).unwrap_or(u8::MAX);
        Rgba::rgb(channel(sum[0]), channel(sum[1]), channel(sum[2]))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use unicode_width::UnicodeWidthChar;
    use vnce_core::Size;

    use super::*;

    const R: Rgba = Rgba::rgb(255, 0, 0);
    const B: Rgba = Rgba::rgb(0, 0, 255);

    fn render(canvas: &Canvas, mode: PixelMode, width: u16, height: u16) -> Buffer {
        let mut buffer = Buffer::empty(Size::new(width, height));
        let area = buffer.area();
        canvas.render(&mut buffer, area, mode, &RenderOptions::default());
        buffer
    }

    fn cell(buffer: &Buffer, x: u16, y: u16) -> (String, Color, Color) {
        let cell = buffer.cell(x, y).expect("in bounds");
        (cell.symbol().to_owned(), cell.fg, cell.bg)
    }

    #[test]
    fn tables_are_complete_unique_and_narrow() {
        for table in [
            &tables::QUADRANT[..],
            &tables::SEXTANT[..],
            &tables::OCTANT[..],
        ] {
            let unique: HashSet<char> = table.iter().copied().collect();
            assert_eq!(unique.len(), table.len());
            assert_eq!(table[0], ' ');
            for &ch in table {
                assert_eq!(ch.width(), Some(1), "{ch:?} must be one cell wide");
            }
        }
        // Spot checks against well-known characters.
        assert_eq!(block_pattern('▌'), Some((2, 4, 0b0101_0101)));
        assert_eq!(block_pattern('\u{1fb00}'), Some((2, 3, 1)));
        assert_eq!(block_pattern('a'), None);
        assert_eq!(tables::QUADRANT[0b0101], '▌');
        assert_eq!(tables::QUADRANT[0b1001], '▚');
        assert_eq!(tables::SEXTANT[0b01_0101], '▌');
        assert_eq!(tables::SEXTANT[0b11_1111], '█');
        assert_eq!(tables::OCTANT[0b0000_1111], '▀');
        assert_eq!(tables::OCTANT[0b1100_0000], '▂');
        assert_eq!(tables::OCTANT[0b0000_0101], '▘');
        assert_eq!(tables::OCTANT[0b1111_1111], '█');
        assert_eq!(tables::OCTANT[0b0000_0001], '\u{1CEA8}');
        assert_eq!(tables::OCTANT[0b0000_0100], '\u{1CD00}');
    }

    #[test]
    fn cell_sizes_and_aspect() {
        assert_eq!(PixelMode::HalfBlock.cell_size(), (1, 2));
        assert_eq!(PixelMode::Sextant.cell_size(), (2, 3));
        assert!((PixelMode::HalfBlock.pixel_aspect() - 1.0).abs() < 1e-6);
        assert!((PixelMode::Quadrant.pixel_aspect() - 0.5).abs() < 1e-6);
        assert!((PixelMode::Sextant.pixel_aspect() - 0.75).abs() < 1e-6);
        assert!((PixelMode::Octant.pixel_aspect() - 1.0).abs() < 1e-6);
        let canvas = Canvas::for_area(Rect::new(3, 3, 10, 5), PixelMode::Octant);
        assert_eq!((canvas.width(), canvas.height()), (20, 20));
    }

    #[test]
    fn half_block() {
        let canvas = Canvas::from_art(&["rr", "br"], &[('r', R), ('b', B)]);
        let buffer = render(&canvas, PixelMode::HalfBlock, 2, 1);
        assert_eq!(
            cell(&buffer, 0, 0),
            ("▀".into(), R.to_color(), B.to_color())
        );
        // Uniform cells use a space, independent of the font.
        assert_eq!(
            cell(&buffer, 1, 0),
            (" ".into(), R.to_color(), R.to_color())
        );
    }

    #[test]
    fn block_modes_pick_the_right_character() {
        // Left column red, right column blue: a left half block in every mode.
        for (mode, rows) in [
            (PixelMode::Quadrant, 2),
            (PixelMode::Sextant, 3),
            (PixelMode::Octant, 4),
        ] {
            let art = vec!["rb"; rows];
            let canvas = Canvas::from_art(&art, &[('r', R), ('b', B)]);
            let buffer = render(&canvas, mode, 1, 1);
            // Pixel 0 (red) is the background, so the blue right half is the glyph.
            assert_eq!(
                cell(&buffer, 0, 0),
                ("▐".into(), B.to_color(), R.to_color()),
                "{mode}"
            );
        }

        let canvas = Canvas::from_art(&["r.", "..", "..", ".."], &[('r', R)]);
        let mut buffer = Buffer::empty(Size::new(1, 1));
        let options = RenderOptions {
            background: B,
            ..RenderOptions::default()
        };
        canvas.render(
            &mut buffer,
            Rect::new(0, 0, 1, 1),
            PixelMode::Octant,
            &options,
        );
        // Transparent pixels show the background. Pixel 0 (red) is always the background
        // group, so the glyph covers the other seven cells and is drawn in blue: the same
        // picture as a red "cell 1" glyph on blue.
        let expected = tables::OCTANT[0b1111_1110].to_string();
        assert_eq!(cell(&buffer, 0, 0), (expected, B.to_color(), R.to_color()));
    }

    #[test]
    fn transparent_cells_are_left_untouched() {
        let mut buffer = Buffer::empty(Size::new(2, 1));
        buffer.set_string(0, 0, "ab", Style::new());
        let mut canvas = Canvas::new(2, 2);
        canvas.pixel(1, 0, R);
        canvas.render(
            &mut buffer,
            Rect::new(0, 0, 2, 1),
            PixelMode::HalfBlock,
            &RenderOptions::default(),
        );
        assert_eq!(cell(&buffer, 0, 0).0, "a");
        assert_eq!(cell(&buffer, 1, 0).0, "▀");
    }

    #[test]
    fn semi_transparent_pixels_use_the_cell_background() {
        let mut buffer = Buffer::empty(Size::new(1, 1));
        buffer.set_style(Rect::new(0, 0, 1, 1), Style::new().bg(Color::Rgb(0, 0, 0)));
        let canvas = Canvas::filled(1, 2, Rgba::new(255, 255, 255, 128));
        canvas.render(
            &mut buffer,
            Rect::new(0, 0, 1, 1),
            PixelMode::HalfBlock,
            &RenderOptions::default(),
        );
        assert_eq!(cell(&buffer, 0, 0).2, Color::Rgb(128, 128, 128));
    }

    #[test]
    fn braille_dots() {
        // Every pixel lit except the bottom-right one: all dots except dot 8.
        let canvas = Canvas::from_art(&["ww", "ww", "ww", "w."], &[('w', Rgba::WHITE)]);
        let buffer = render(&canvas, PixelMode::Braille, 1, 1);
        // Pixel 0 (white) is the background group; the black pixel is the only "dot".
        assert_eq!(cell(&buffer, 0, 0).0, "\u{2880}");
        assert_eq!(braille_bit(0, 0), 0);
        assert_eq!(braille_bit(1, 2), 5);
        assert_eq!(braille_bit(0, 3), 6);
    }

    #[test]
    fn ascii_ramp() {
        let canvas = Canvas::from_art(&["wk", "wk"], &[('w', Rgba::WHITE), ('k', Rgba::BLACK)]);
        let buffer = render(&canvas, PixelMode::Ascii, 2, 1);
        assert_eq!(cell(&buffer, 0, 0).0, "@");
        assert_eq!(cell(&buffer, 1, 0).0, " ");
    }

    #[test]
    fn rendering_is_clipped_to_the_buffer() {
        let canvas = Canvas::filled(100, 100, R);
        let buffer = render(&canvas, PixelMode::Octant, 3, 2);
        assert_eq!(
            cell(&buffer, 2, 1),
            (" ".into(), R.to_color(), R.to_color())
        );
        let small = Canvas::filled(1, 1, R);
        let buffer = render(&small, PixelMode::Quadrant, 3, 2);
        assert_eq!(cell(&buffer, 1, 0).0, " ");
        assert_eq!(cell(&buffer, 1, 0).2, Color::Reset);
    }

    #[test]
    fn mode_names_and_detection() {
        assert_eq!("half-block".parse(), Ok(PixelMode::HalfBlock));
        assert_eq!("OCTANT".parse(), Ok(PixelMode::Octant));
        assert!("pixels".parse::<PixelMode>().is_err());
        for mode in PixelMode::ALL {
            assert_eq!(mode.to_string().parse(), Ok(mode));
        }

        let detect = |vars: &[(&str, &str)], depth| {
            PixelMode::from_env(
                |name| {
                    vars.iter()
                        .find(|(k, _)| *k == name)
                        .map(|(_, v)| (*v).to_owned())
                },
                depth,
                false,
            )
        };
        let truecolor = ColorDepth::TrueColor;
        assert_eq!(detect(&[], truecolor), PixelMode::HalfBlock);
        assert_eq!(
            detect(&[("TERM", "xterm-kitty")], truecolor),
            PixelMode::Sextant
        );
        assert_eq!(
            detect(&[("TERM_PROGRAM", "WezTerm")], truecolor),
            PixelMode::Sextant
        );
        assert_eq!(
            detect(&[("TERM", "xterm-256color")], truecolor),
            PixelMode::HalfBlock
        );
        assert_eq!(detect(&[], ColorDepth::NoColor), PixelMode::Ascii);
        assert_eq!(
            detect(
                &[("VNCE_PIXEL_MODE", "octant"), ("TERM", "dumb")],
                ColorDepth::NoColor
            ),
            PixelMode::Octant
        );
        assert_eq!(
            detect(&[("VNCE_PIXEL_MODE", "bogus")], truecolor),
            PixelMode::HalfBlock
        );
    }

    fn available(vars: &[(&str, &str)], depth: ColorDepth, windows: bool) -> Vec<PixelMode> {
        PixelMode::available_from_env(
            |name| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            },
            depth,
            windows,
        )
    }

    #[test]
    fn available_modes() {
        use PixelMode::{Ascii, Braille, HalfBlock, Octant, Quadrant, Sextant};
        let truecolor = ColorDepth::TrueColor;

        // Octant is never offered automatically: almost no font has it.
        for vars in [
            &[][..],
            &[("TERM", "xterm-kitty")][..],
            &[("WT_SESSION", "x")][..],
        ] {
            assert!(!available(vars, truecolor, true).contains(&Octant));
            assert!(!available(vars, truecolor, false).contains(&Octant));
        }

        // Windows Terminal: font fallback for Braille, no self-drawn sextants assumed.
        assert_eq!(
            available(&[("WT_SESSION", "x")], truecolor, true),
            [HalfBlock, Quadrant, Braille, Ascii]
        );
        // Classic Windows console: no font fallback.
        assert_eq!(
            available(&[], truecolor, true),
            [HalfBlock, Quadrant, Ascii]
        );
        // Linux console.
        assert_eq!(
            available(&[("TERM", "linux")], ColorDepth::Ansi16, false),
            [HalfBlock, Quadrant, Ascii]
        );
        // Terminals that draw block characters themselves.
        assert_eq!(
            available(&[("TERM", "xterm-ghostty")], truecolor, false),
            [Sextant, HalfBlock, Quadrant, Braille, Ascii]
        );
        // No colors: only modes that work in monochrome.
        assert_eq!(
            available(&[("TERM", "xterm")], ColorDepth::NoColor, false),
            [Braille, Ascii]
        );
        // An explicit choice is trusted.
        assert_eq!(
            available(&[("VNCE_PIXEL_MODE", "octant")], truecolor, true),
            [Octant]
        );
    }

    #[test]
    fn detect_always_returns_an_available_mode() {
        let cases: [&[(&str, &str)]; 5] = [
            &[],
            &[("TERM", "linux")],
            &[("TERM", "xterm-kitty")],
            &[("WT_SESSION", "x")],
            &[("VNCE_PIXEL_MODE", "braille")],
        ];
        for vars in cases {
            for depth in [
                ColorDepth::TrueColor,
                ColorDepth::Ansi16,
                ColorDepth::NoColor,
            ] {
                for windows in [false, true] {
                    let mode = PixelMode::from_env(
                        |name| {
                            vars.iter()
                                .find(|(k, _)| *k == name)
                                .map(|(_, v)| (*v).to_owned())
                        },
                        depth,
                        windows,
                    );
                    assert!(available(vars, depth, windows).contains(&mode), "{vars:?}");
                }
            }
        }
    }
}
