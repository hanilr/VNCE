//! The pictures of cell symbols: letters from the bundled font, and block elements,
//! sextants, braille and box drawing drawn by code, so the engine's pixel modes and
//! borders line up exactly with the cell grid.
//!
//! Every glyph is a coverage bitmap (0 = background, 255 = foreground) of the whole cell
//! (two cells for wide symbols).

use fontdue::{Font as FontFile, FontSettings};

/// The bundled font: `JetBrains Mono` Regular (SIL Open Font License 1.1, see
/// `fonts/OFL.txt`).
const FONT: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");

/// How a glyph is styled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GlyphStyle {
    /// Drawn thicker (the font has one weight; bold widens the strokes by a pixel).
    pub bold: bool,
    /// Slanted.
    pub italic: bool,
}

/// A coverage bitmap of `width` × `height` pixels, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}

impl Glyph {
    fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: vec![0; (width * height) as usize],
        }
    }

    fn set(&mut self, x: u32, y: u32, value: u8) {
        if x < self.width && y < self.height {
            let index = (y * self.width + x) as usize;
            self.coverage[index] = self.coverage[index].max(value);
        }
    }

    /// Fills `x0..x1` × `y0..y1` (clipped).
    fn fill(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, value: u8) {
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                self.set(x, y, value);
            }
        }
    }

    /// Whether every pixel is background.
    pub fn is_empty(&self) -> bool {
        self.coverage.iter().all(|&c| c == 0)
    }
}

/// The font at one pixel size, with the size of a cell.
#[derive(Debug)]
pub struct Font {
    file: FontFile,
    px: f32,
    cell: (u32, u32),
    /// Distance from the top of a cell to the baseline.
    baseline: f32,
}

impl Font {
    /// The bundled font at `px` pixels (the em size).
    ///
    /// # Panics
    ///
    /// Never: the bundled font is valid (checked by the tests).
    #[must_use]
    pub fn new(px: f32) -> Self {
        let file =
            FontFile::from_bytes(FONT, FontSettings::default()).expect("the bundled font is valid");
        let px = px.max(4.0);
        let advance = file.metrics('M', px).advance_width;
        let (ascent, height) = file
            .horizontal_line_metrics(px)
            .map_or((px * 0.8, px * 1.2), |m| {
                (m.ascent, m.ascent - m.descent + m.line_gap)
            });
        let cell = (
            advance.round().max(1.0) as u32,
            height.round().max(2.0) as u32,
        );
        // Center the font's line in the cell.
        let baseline = ascent + (cell.1 as f32 - height) / 2.0;
        Self {
            file,
            px,
            cell,
            baseline,
        }
    }

    /// Size of one cell in pixels.
    #[must_use]
    pub fn cell_size(&self) -> (u32, u32) {
        self.cell
    }

    /// The picture of `symbol` over `columns` cells (1, or 2 for wide symbols).
    #[must_use]
    pub fn glyph(&self, symbol: &str, style: GlyphStyle, columns: u32) -> Glyph {
        let (cell_width, height) = self.cell;
        let width = cell_width * columns.max(1);
        let Some(c) = symbol.chars().next() else {
            return Glyph::blank(width, height);
        };
        if let Some(drawn) = drawn(c, width, height) {
            return drawn;
        }
        let mut glyph = self.font_glyph(c, width, height);
        if style.italic {
            glyph = slant(&glyph, self.baseline);
        }
        if style.bold {
            glyph = embolden(&glyph);
        }
        glyph
    }

    fn font_glyph(&self, c: char, width: u32, height: u32) -> Glyph {
        let mut glyph = Glyph::blank(width, height);
        if c.is_whitespace() {
            return glyph;
        }
        if self.file.lookup_glyph_index(c) == 0 {
            // Not in the font: an empty box, like terminals show.
            let inset = (width / 5).max(1);
            outline(
                &mut glyph,
                inset,
                height / 5,
                width - inset,
                height - height / 5,
            );
            return glyph;
        }
        let (metrics, bitmap) = self.file.rasterize(c, self.px);
        // Centered across the cells, standing on the baseline.
        let left = ((width as f32 - metrics.advance_width) / 2.0).round() as i32 + metrics.xmin;
        let top = (self.baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;
        for row in 0..metrics.height {
            for column in 0..metrics.width {
                let (x, y) = (left + column as i32, top + row as i32);
                if x >= 0 && y >= 0 {
                    glyph.set(x as u32, y as u32, bitmap[row * metrics.width + column]);
                }
            }
        }
        glyph
    }
}

/// The glyph slanted to the right above the baseline.
fn slant(glyph: &Glyph, baseline: f32) -> Glyph {
    let mut out = Glyph::blank(glyph.width, glyph.height);
    for y in 0..glyph.height {
        let shift = ((baseline - y as f32) * 0.2).round() as i32;
        for x in 0..glyph.width {
            let source = x as i32 - shift;
            if source >= 0 && (source as u32) < glyph.width {
                out.coverage[(y * glyph.width + x) as usize] =
                    glyph.coverage[(y * glyph.width + source as u32) as usize];
            }
        }
    }
    out
}

/// The glyph with every stroke a pixel wider.
fn embolden(glyph: &Glyph) -> Glyph {
    let mut out = glyph.clone();
    for y in 0..glyph.height {
        for x in 1..glyph.width {
            let left = glyph.coverage[(y * glyph.width + x - 1) as usize];
            out.set(x, y, left);
        }
    }
    out
}

/// A one-pixel rectangle outline.
fn outline(glyph: &mut Glyph, x0: u32, y0: u32, x1: u32, y1: u32) {
    glyph.fill(x0, y0, x1, y0 + 1, 255);
    glyph.fill(x0, y1.saturating_sub(1), x1, y1, 255);
    glyph.fill(x0, y0, x0 + 1, y1, 255);
    glyph.fill(x1.saturating_sub(1), y0, x1, y1, 255);
}

/// Symbols drawn by code instead of the font, or `None` for the font's.
#[must_use]
pub fn drawn(c: char, width: u32, height: u32) -> Option<Glyph> {
    if let Some((columns, rows, bits)) = vnce_raster::block_pattern(c) {
        return Some(grid(
            u32::from(columns),
            u32::from(rows),
            bits,
            width,
            height,
        ));
    }
    let code = u32::from(c);
    match code {
        0x2580..=0x259F => Some(block(code, width, height)),
        0x2800..=0x28FF => Some(braille(code - 0x2800, width, height)),
        _ => box_drawing(code, width, height),
    }
}

/// A grid of `columns` × `rows` blocks, filled where `bits` has a 1 (bit
/// `row * columns + column`): the quadrants, sextants and octants of the pixel modes.
fn grid(columns: u32, rows: u32, bits: u8, width: u32, height: u32) -> Glyph {
    let mut glyph = Glyph::blank(width, height);
    for row in 0..rows {
        for column in 0..columns {
            if bits & (1 << (row * columns + column)) != 0 {
                glyph.fill(
                    width * column / columns,
                    height * row / rows,
                    width * (column + 1) / columns,
                    height * (row + 1) / rows,
                    255,
                );
            }
        }
    }
    glyph
}

/// Where the fraction `eighths / 8` of `length` ends (shared by halves, eighths and
/// quadrants, so neighbors meet without a gap or an overlap).
fn split(length: u32, eighths: u32) -> u32 {
    length * eighths / 8
}

/// Block elements U+2580..U+259F.
fn block(code: u32, width: u32, height: u32) -> Glyph {
    let mut glyph = Glyph::blank(width, height);
    let (half_x, half_y) = (split(width, 4), split(height, 4));
    let full = 255;
    match code {
        0x2580 => glyph.fill(0, 0, width, half_y, full),
        // Lower one to eight eighths.
        0x2581..=0x2588 => glyph.fill(0, split(height, 8 - (code - 0x2580)), width, height, full),
        // Left seven to one eighths.
        0x2589..=0x258F => glyph.fill(0, 0, split(width, 0x2590 - code), height, full),
        0x2590 => glyph.fill(half_x, 0, width, height, full),
        // Light, medium and dark shade.
        0x2591..=0x2593 => glyph.fill(0, 0, width, height, ((code - 0x2590) * 64) as u8),
        0x2594 => glyph.fill(0, 0, width, split(height, 1), full),
        0x2595 => glyph.fill(split(width, 7), 0, width, height, full),
        _ => {
            // Quadrants: upper left, upper right, lower left, lower right.
            let quadrants: [bool; 4] = match code {
                0x2596 => [false, false, true, false],
                0x2597 => [false, false, false, true],
                0x2598 => [true, false, false, false],
                0x2599 => [true, false, true, true],
                0x259A => [true, false, false, true],
                0x259B => [true, true, true, false],
                0x259C => [true, true, false, true],
                0x259D => [false, true, false, false],
                0x259E => [false, true, true, false],
                _ => [false, true, true, true],
            };
            let boxes = [
                (0, 0, half_x, half_y),
                (half_x, 0, width, half_y),
                (0, half_y, half_x, height),
                (half_x, half_y, width, height),
            ];
            for (on, (x0, y0, x1, y1)) in quadrants.into_iter().zip(boxes) {
                if on {
                    glyph.fill(x0, y0, x1, y1, full);
                }
            }
        }
    }
    glyph
}

/// Braille pattern `dots` (U+2800 + dots): eight dots in two columns and four rows.
fn braille(dots: u32, width: u32, height: u32) -> Glyph {
    // Bit → (column, row): dots 1-3 down the left, 4-6 down the right, 7 and 8 below.
    const PLACES: [(u32, u32); 8] = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    let mut glyph = Glyph::blank(width, height);
    let (column_width, row_height) = (width as f32 / 2.0, height as f32 / 4.0);
    // A dot fills the middle of its part of the cell.
    let size = (column_width.min(row_height) * 0.62).max(1.0);
    for (bit, (column, row)) in PLACES.into_iter().enumerate() {
        if dots & (1 << bit) != 0 {
            let cx = (column as f32 + 0.5) * column_width;
            let cy = (row as f32 + 0.5) * row_height;
            let x0 = (cx - size / 2.0).round().max(0.0) as u32;
            let y0 = (cy - size / 2.0).round().max(0.0) as u32;
            let dot = size.round().max(1.0) as u32;
            glyph.fill(x0, y0, x0 + dot, y0 + dot, 255);
        }
    }
    glyph
}

/// How heavy an arm of a box drawing symbol is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    None,
    Light,
    Heavy,
    Double,
}

/// Box drawing symbols with their arms (up, right, down, left), and whether the corner
/// is rounded.
fn arms(code: u32) -> Option<([Arm; 4], bool)> {
    use Arm::{Double as D, Heavy as H, Light as L, None as N};
    let arms = match code {
        0x2500 => [N, L, N, L],
        0x2501 => [N, H, N, H],
        0x2502 => [L, N, L, N],
        0x2503 => [H, N, H, N],
        0x250C => [N, L, L, N],
        0x250F => [N, H, H, N],
        0x2510 => [N, N, L, L],
        0x2513 => [N, N, H, H],
        0x2514 => [L, L, N, N],
        0x2517 => [H, H, N, N],
        0x2518 => [L, N, N, L],
        0x251B => [H, N, N, H],
        0x251C => [L, L, L, N],
        0x2523 => [H, H, H, N],
        0x2524 => [L, N, L, L],
        0x252B => [H, N, H, H],
        0x252C => [N, L, L, L],
        0x2533 => [N, H, H, H],
        0x2534 => [L, L, N, L],
        0x253B => [H, H, N, H],
        0x253C => [L, L, L, L],
        0x254B => [H, H, H, H],
        0x2550 => [N, D, N, D],
        0x2551 => [D, N, D, N],
        0x2554 => [N, D, D, N],
        0x2557 => [N, N, D, D],
        0x255A => [D, D, N, N],
        0x255D => [D, N, N, D],
        0x256D => return Some(([N, L, L, N], true)),
        0x256E => return Some(([N, N, L, L], true)),
        0x256F => return Some(([L, N, N, L], true)),
        0x2570 => return Some(([L, L, N, N], true)),
        _ => return None,
    };
    Some((arms, false))
}

/// Box drawing U+2500..U+257F (the lines, corners, tees and crosses of borders; the
/// rest comes from the font).
fn box_drawing(code: u32, width: u32, height: u32) -> Option<Glyph> {
    let (arms, rounded) = arms(code)?;
    let mut glyph = Glyph::blank(width, height);
    let light = (width / 8).max(1);
    let (cx, cy) = (split(width, 4), split(height, 4));
    let [up, right, down, left] = arms;
    if rounded {
        rounded_corner(&mut glyph, [up, right, down, left], light);
        return Some(glyph);
    }
    let double = arms.contains(&Arm::Double);
    if double {
        double_lines(&mut glyph, arms, light);
        return Some(glyph);
    }
    let thickness = |arm: Arm| if arm == Arm::Heavy { light * 2 } else { light };
    // The center square is as thick as the heaviest arm.
    let center = arms
        .iter()
        .filter(|&&a| a != Arm::None)
        .map(|&a| thickness(a))
        .max()
        .unwrap_or(light);
    let (x0, y0) = (cx - center / 2, cy - center / 2);
    for (arm, horizontal, from_start) in [
        (up, false, true),
        (right, true, false),
        (down, false, false),
        (left, true, true),
    ] {
        if arm == Arm::None {
            continue;
        }
        let t = thickness(arm);
        if horizontal {
            let y = cy - t / 2;
            let (a, b) = if from_start {
                (0, x0 + center)
            } else {
                (x0, width)
            };
            glyph.fill(a, y, b, y + t, 255);
        } else {
            let x = cx - t / 2;
            let (a, b) = if from_start {
                (0, y0 + center)
            } else {
                (y0, height)
            };
            glyph.fill(x, a, x + t, b, 255);
        }
    }
    Some(glyph)
}

/// Double lines (═ ║ and the four corners): two light lines with a light gap, the inner
/// and outer lines of a corner meeting at their own corners.
fn double_lines(glyph: &mut Glyph, [up, right, down, left]: [Arm; 4], light: u32) {
    let (width, height) = (glyph.width, glyph.height);
    let (cx, cy) = (i64::from(split(width, 4)), i64::from(split(height, 4)));
    let gap = i64::from(light);
    let t = i64::from(light);
    let fill = |glyph: &mut Glyph, x0: i64, y0: i64, x1: i64, y1: i64| {
        let clamp = |v: i64, max: u32| v.clamp(0, i64::from(max)) as u32;
        glyph.fill(
            clamp(x0, width),
            clamp(y0, height),
            clamp(x1, width),
            clamp(y1, height),
            255,
        );
    };
    let horizontal = left != Arm::None || right != Arm::None;
    let vertical = up != Arm::None || down != Arm::None;
    if horizontal && !vertical {
        for y in [cy - gap - t / 2, cy + gap - t / 2] {
            fill(glyph, 0, y, i64::from(width), y + t);
        }
        return;
    }
    if vertical && !horizontal {
        for x in [cx - gap - t / 2, cx + gap - t / 2] {
            fill(glyph, x, 0, x + t, i64::from(height));
        }
        return;
    }
    // A corner: `hx` towards the horizontal arm, `vy` towards the vertical one.
    let hx: i64 = if right == Arm::None { -1 } else { 1 };
    let vy: i64 = if down == Arm::None { -1 } else { 1 };
    for side in [-1_i64, 1] {
        // Outer line (side −1) and inner line (side +1).
        let y = cy + side * vy * gap - t / 2;
        let x = cx + side * hx * gap - t / 2;
        let edge_x = if hx > 0 { i64::from(width) } else { 0 };
        let edge_y = if vy > 0 { i64::from(height) } else { 0 };
        let (ax, bx) = (x.min(edge_x), (x + t).max(edge_x));
        fill(glyph, ax, y, bx, y + t);
        let (ay, by) = (y.min(edge_y), (y + t).max(edge_y));
        fill(glyph, x, ay, x + t, by);
    }
}

/// A rounded corner (╭ ╮ ╯ ╰): a quarter circle joined to the edges by straight lines.
fn rounded_corner(glyph: &mut Glyph, [_, right, down, _]: [Arm; 4], light: u32) {
    let (width, height) = (glyph.width as f32, glyph.height as f32);
    let (cx, cy) = (
        split(glyph.width, 4) as f32 + light as f32 / 2.0,
        split(glyph.height, 4) as f32 + light as f32 / 2.0,
    );
    let hx = if right == Arm::None { -1.0 } else { 1.0 };
    let vy = if down == Arm::None { -1.0 } else { 1.0 };
    let radius = (width / 2.0).min(height / 2.0);
    // The circle's middle, diagonally away from the corner.
    let (ox, oy) = (cx + hx * radius, cy + vy * radius);
    let half = light as f32 / 2.0;
    for y in 0..glyph.height {
        for x in 0..glyph.width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let in_quarter = (px - ox) * hx <= 0.0 && (py - oy) * vy <= 0.0;
            let on_arc = in_quarter && ((px - ox).hypot(py - oy) - radius).abs() <= half + 0.25;
            // Straight parts from the arc to the edges.
            let horizontal =
                (py - cy).abs() <= half && (px - ox) * hx >= 0.0 && px >= 0.0 && px <= width;
            let vertical = (px - cx).abs() <= half && (py - oy) * vy >= 0.0;
            if on_arc || horizontal || vertical {
                glyph.set(x, y, 255);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covered(glyph: &Glyph, x: u32, y: u32) -> bool {
        glyph.coverage[(y * glyph.width + x) as usize] > 128
    }

    #[test]
    fn the_font_loads_with_a_tall_cell() {
        let font = Font::new(16.0);
        let (width, height) = font.cell_size();
        assert!((8..=12).contains(&width), "{width}");
        assert!(height > width * 3 / 2, "{height}");
        let a = font.glyph("A", GlyphStyle::default(), 1);
        assert!(!a.is_empty());
        assert_eq!((a.width, a.height), (width, height));
        assert!(font.glyph(" ", GlyphStyle::default(), 1).is_empty());
        // Wide symbols get two cells.
        assert_eq!(font.glyph("A", GlyphStyle::default(), 2).width, width * 2);
        // Bold has more ink.
        let ink = |g: &Glyph| g.coverage.iter().map(|&c| u32::from(c)).sum::<u32>();
        let bold = font.glyph(
            "A",
            GlyphStyle {
                bold: true,
                italic: false,
            },
            1,
        );
        assert!(ink(&bold) > ink(&a));
        // A missing symbol is a box, not nothing.
        assert!(
            !font
                .glyph("\u{10FFFD}", GlyphStyle::default(), 1)
                .is_empty()
        );
    }

    #[test]
    fn halves_and_quadrants_tile_the_cell_exactly() {
        for (width, height) in [(9, 19), (10, 21), (8, 16)] {
            let upper = drawn('▀', width, height).unwrap();
            let lower = drawn('▄', width, height).unwrap();
            let left = drawn('▌', width, height).unwrap();
            let right = drawn('▐', width, height).unwrap();
            for y in 0..height {
                for x in 0..width {
                    assert!(covered(&upper, x, y) != covered(&lower, x, y), "{x},{y}");
                    assert!(covered(&left, x, y) != covered(&right, x, y), "{x},{y}");
                }
            }
            let full = drawn('█', width, height).unwrap();
            assert!(full.coverage.iter().all(|&c| c == 255));
            // ▚ is upper left and lower right.
            let diagonal = drawn('▚', width, height).unwrap();
            assert!(covered(&diagonal, 0, 0) && covered(&diagonal, width - 1, height - 1));
            assert!(!covered(&diagonal, width - 1, 0));
        }
    }

    #[test]
    fn sextants_follow_the_unicode_order() {
        // U+1FB00 is the upper left sixth; U+1FB3B is all but the upper left (pattern 62).
        let first = drawn('\u{1FB00}', 10, 21).unwrap();
        assert!(covered(&first, 0, 0) && !covered(&first, 9, 0) && !covered(&first, 0, 20));
        let last = drawn('\u{1FB3B}', 10, 21).unwrap();
        assert!(!covered(&last, 0, 0) && covered(&last, 9, 0) && covered(&last, 0, 20));
        // Pattern 22 (after the skipped left half) is U+1FB14: top right, middle left and
        // bottom left.
        let skipped = drawn('\u{1FB14}', 10, 21).unwrap();
        assert!(covered(&skipped, 9, 2) && covered(&skipped, 0, 10) && covered(&skipped, 0, 18));
        assert!(!covered(&skipped, 0, 2) && !covered(&skipped, 9, 10));
    }

    #[test]
    fn braille_dots_sit_in_their_places() {
        // Dot 1 (top left) and dot 8 (bottom right).
        let glyph = drawn('\u{2881}', 10, 20).unwrap();
        assert!(covered(&glyph, 2, 2) && covered(&glyph, 7, 17));
        assert!(!covered(&glyph, 7, 2) && !covered(&glyph, 2, 17));
    }

    #[test]
    fn borders_reach_the_cell_edges() {
        let (width, height) = (10, 20);
        let corner = drawn('┌', width, height).unwrap();
        assert!(covered(&corner, width - 1, height / 2) && covered(&corner, width / 2, height - 1));
        assert!(!covered(&corner, 0, height / 2) && !covered(&corner, width / 2, 0));
        let line = drawn('─', width, height).unwrap();
        assert!((0..width).all(|x| covered(&line, x, height / 2)));
        for c in ['╭', '╮', '╯', '╰', '╔', '╝', '═', '║', '┼', '━'] {
            let glyph = drawn(c, width, height).unwrap();
            assert!(!glyph.is_empty(), "{c}");
        }
        // A rounded corner reaches the edges its arms point to.
        let rounded = drawn('╭', width, height).unwrap();
        assert!(
            covered(&rounded, width - 1, height / 2) && covered(&rounded, width / 2, height - 1)
        );
        assert!(drawn('a', width, height).is_none());
    }
}
