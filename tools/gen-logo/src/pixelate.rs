//! The pixel bands: from the left, 32, 16, 8 and 4 unit blocks (in the 512 design space),
//! then the smooth drawing. Each band's right edge moves by up to one block per row, so the
//! bands interlock instead of meeting on a straight line.

use std::collections::HashMap;

use crate::horse::BACKGROUND;
use crate::raster::Premul;

/// Block sizes of the pixel bands, in design units.
const BLOCKS: [f32; 4] = [32.0, 16.0, 8.0, 4.0];

/// Where each pixel band ends (before jitter), in design units (the logo's; a scene has
/// its own).
pub const EDGES: [f32; 4] = [144.0, 208.0, 256.0, 300.0];

/// The band index of the smooth drawing.
pub const SMOOTH: usize = 4;

/// How far from the background (RGB distance) a pixel must be to count as a line.
const INK: f32 = 0.15;

/// How wide (in pixels) a line crossing a block must be for the block to take its color.
const COVER: f32 = 2.5;

/// The bands' geometry for one output size.
#[derive(Clone, Copy, Debug)]
pub struct Bands {
    w: usize,
    h: usize,
    scale: f32,
    block: [usize; 4],
    edges: [f32; 4],
}

impl Bands {
    /// The bands of a `w` x `h` image of a design space `design_width` wide, ending at
    /// `edges` (design units). Blocks scale with the size and never go below one
    /// pixel; one-pixel blocks still differ from the smooth part (hard edges, palettes).
    pub fn new(edges: [f32; 4], design_width: f32, w: usize, h: usize) -> Self {
        let scale = w as f32 / design_width;
        let block = BLOCKS.map(|b| ((b * scale).round() as usize).max(1));
        Self {
            w,
            h,
            scale,
            block,
            edges,
        }
    }

    /// The bands of `scene` rendered `w` x `h`.
    pub fn of(scene: &crate::scene::Scene, w: usize, h: usize) -> Self {
        Self::new(scene.edges, scene.width, w, h)
    }

    /// The block size of band `band`, in pixels.
    pub fn block(&self, band: usize) -> usize {
        self.block[band]
    }

    /// The band of the pixel at (`x`, `y`): `0..=3` for the pixel bands, [`SMOOTH`] for the
    /// smooth drawing.
    pub fn band_of(&self, x: usize, y: usize) -> usize {
        let scale = self.scale;
        for band in 0..SMOOTH {
            let s = self.block[band];
            let (cell_x, cell_y) = (x / s, y / s);
            let center = (cell_x * s) as f32 + s as f32 / 2.0;
            let edge = self.edges[band] * scale + jitter(band, cell_y) as f32 * s as f32;
            if center < edge {
                return band;
            }
        }
        SMOOTH
    }
}

/// -1, 0 or +1 blocks, the same for every output size.
fn jitter(band: usize, row: usize) -> i32 {
    let mut h = (band as u32).wrapping_mul(0x9e37_79b9) ^ (row as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 13;
    (h % 3) as i32 - 1
}

/// The logo: the pixel bands made from `smooth` (an anti-aliased render the size of
/// `bands`), and `smooth` itself in the smooth band.
pub fn compose(smooth: &[Premul], bands: &Bands) -> Vec<Premul> {
    let (w, h) = (bands.w, bands.h);
    let mut cells: HashMap<(usize, usize, usize), Premul> = HashMap::new();
    let mut out = vec![[0.0f32; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let band = bands.band_of(x, y);
            out[y * w + x] = if band == SMOOTH {
                smooth[y * w + x]
            } else {
                let s = bands.block(band);
                *cells
                    .entry((band, x / s, y / s))
                    .or_insert_with(|| block_color(smooth, (w, h), x / s * s, y / s * s, s))
            };
        }
    }
    out
}

/// The color of one block, with hard alpha.
///
/// In the dark style most of a block is background and the drawing is lines and shards.
/// A block takes the color that covers most of it after the background, when that color
/// covers at least as much as a line [`COVER`] pixels wide crossing the block (and never
/// more than a quarter of the block), like pixel art drawn by hand at that size: shards and lines
/// crossing a block light it, a hairline grazing its corner does not. The same line
/// lights every block it crosses, whatever the block size. A block with too little line
/// keeps the background.
fn block_color(
    smooth: &[Premul],
    (w, h): (usize, usize),
    x0: usize,
    y0: usize,
    s: usize,
) -> Premul {
    let mut alpha = 0.0;
    let mut count = 0.0;
    // Line pixels grouped by color (rounded), with their count and color sum.
    let mut inks: HashMap<[u8; 3], (f32, [f32; 3])> = HashMap::new();
    for y in y0..(y0 + s).min(h) {
        for x in x0..(x0 + s).min(w) {
            let p = smooth[y * w + x];
            alpha += p[3];
            count += 1.0;
            if p[3] <= 0.0 {
                continue;
            }
            let c = [p[0] / p[3], p[1] / p[3], p[2] / p[3]];
            let (dr, dg, db) = (
                c[0] - BACKGROUND.r,
                c[1] - BACKGROUND.g,
                c[2] - BACKGROUND.b,
            );
            if (dr * dr + dg * dg + db * db).sqrt() < INK {
                continue;
            }
            let key = c.map(|v| (v * 8.0).round() as u8);
            let entry = inks.entry(key).or_insert((0.0, [0.0; 3]));
            entry.0 += 1.0;
            for k in 0..3 {
                entry.1[k] += c[k];
            }
        }
    }
    if alpha / count < 0.5 {
        return [0.0; 4];
    }
    let main = inks
        .values()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .filter(|(n, _)| *n >= (COVER * s as f32).min(count / 4.0));
    let c = match main {
        Some((n, sum)) => sum.map(|v| v / n),
        None => [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b],
    };
    [c[0], c[1], c[2], 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::SIZE;

    /// The logo's bands at `n` x `n`.
    fn logo(n: usize) -> Bands {
        Bands::new(EDGES, SIZE, n, n)
    }

    #[test]
    fn bands_go_from_pixels_on_the_left_to_smooth_on_the_right() {
        let bands = logo(512);
        for y in 0..512 {
            let mut last = 0;
            for x in 0..512 {
                let band = bands.band_of(x, y);
                assert!(band >= last, "row {y}: band {band} after {last} at x {x}");
                last = band;
            }
            assert_eq!(bands.band_of(0, y), 0);
            assert_eq!(bands.band_of(511, y), SMOOTH);
        }
    }

    #[test]
    fn band_edges_interlock() {
        let bands = logo(512);
        let starts: Vec<usize> = (0..512)
            .step_by(16)
            .map(|y| (0..512).find(|&x| bands.band_of(x, y) != 0).unwrap())
            .collect();
        assert!(
            starts.iter().any(|&s| s != starts[0]),
            "the first edge is a straight line"
        );
    }

    #[test]
    fn small_sizes_keep_one_pixel_blocks() {
        let bands = logo(16);
        assert_eq!(bands.block, [1, 1, 1, 1]);
        assert_eq!(logo(256).block(0), 16);
    }

    #[test]
    fn wide_bands_follow_the_design_width() {
        // A 1280 unit wide scene rendered at half size: edges and blocks halve.
        let bands = Bands::new([100.0, 200.0, 300.0, 400.0], 1280.0, 640, 320);
        assert_eq!(bands.block(0), 16);
        for y in 0..320 {
            assert_eq!(bands.band_of(0, y), 0);
            assert_eq!(bands.band_of(250, y), SMOOTH);
            assert_eq!(bands.band_of(639, y), SMOOTH);
        }
    }

    #[test]
    fn blocks_have_hard_alpha() {
        let n = 32;
        let mut smooth = vec![[0.0f32; 4]; n * n];
        // A half-transparent left column and an opaque white square.
        for y in 0..n {
            smooth[y * n] = [0.2, 0.2, 0.2, 0.2];
            for x in 4..12 {
                smooth[y * n + x] = [1.0, 1.0, 1.0, 1.0];
            }
        }
        let out = compose(&smooth, &logo(n));
        assert_eq!(out[0][3], 0.0);
        assert_eq!(out[5 * n + 5], [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn a_wide_line_lights_its_block_and_a_hairline_does_not() {
        let n = 512;
        let bg = [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b, 1.0];
        let mut smooth = vec![bg; n * n];
        // In the first 32 pixel block column, a red line 6 pixels wide (19% of the block);
        // in the second, a white line 1 pixel wide (3%).
        for y in 0..n {
            for x in 10..16 {
                smooth[y * n + x] = [0.8, 0.2, 0.2, 1.0];
            }
            smooth[y * n + 40] = [1.0, 1.0, 1.0, 1.0];
        }
        let out = compose(&smooth, &logo(n));
        let red = out[100 * n];
        assert!((red[0] - 0.8).abs() < 1e-5 && (red[1] - 0.2).abs() < 1e-5);
        assert_eq!(out[100 * n + 31], red);
        assert_eq!(out[100 * n + 40], bg);
    }
}
