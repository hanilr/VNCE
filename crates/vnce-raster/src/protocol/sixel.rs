//! Sixel graphics.
//!
//! The image is reduced to a palette of at most 256 colors (median cut, optional
//! Floyd–Steinberg dithering) and written in bands of six rows, with run-length encoding.
//! Pixels with alpha below 128 are left transparent.

use std::collections::HashMap;
use std::io::Write as _;

use crate::canvas::Canvas;
use crate::color::Rgba;

/// Options for [`encode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SixelOptions {
    /// Palette size, 1 to 256 (terminals report their limit; 256 is the common maximum).
    pub max_colors: u16,
    /// Floyd–Steinberg dithering: smoother gradients, more output.
    pub dither: bool,
}

impl Default for SixelOptions {
    fn default() -> Self {
        Self {
            max_colors: 256,
            dither: false,
        }
    }
}

type Rgb = [u8; 3];

/// Appends `canvas` as one Sixel image (a complete DCS sequence) to `out`. The image is
/// drawn at the cursor position. Nothing is written for an empty canvas.
pub fn encode(canvas: &Canvas, options: SixelOptions, out: &mut Vec<u8>) {
    let (width, height) = (canvas.width() as usize, canvas.height() as usize);
    if width == 0 || height == 0 {
        return;
    }
    let max_colors = usize::from(options.max_colors.clamp(1, 256));
    let palette = build_palette(canvas.pixels(), max_colors);
    let indices = map_pixels(canvas.pixels(), width, &palette, options.dither);

    // DCS P1;P2;P3 q: P2 = 1 keeps pixels without a color transparent.
    out.extend_from_slice(b"\x1bP0;1;0q");
    // Raster attributes: 1:1 pixel aspect ratio and the image size.
    let _ = write!(out, "\"1;1;{width};{height}");
    for (index, color) in palette.iter().enumerate() {
        let percent = |v: u8| (u32::from(v) * 100 + 127) / 255;
        let _ = write!(
            out,
            "#{index};2;{};{};{}",
            percent(color[0]),
            percent(color[1]),
            percent(color[2])
        );
    }

    let mut row = vec![0u8; width];
    for band in (0..height).step_by(6) {
        let band_rows = (height - band).min(6);
        let mut colors: Vec<u8> = indices[band * width..(band + band_rows) * width]
            .iter()
            .flatten()
            .copied()
            .collect();
        colors.sort_unstable();
        colors.dedup();
        for (n, &color) in colors.iter().enumerate() {
            if n > 0 {
                // Graphics carriage return: back to the start of the band.
                out.push(b'$');
            }
            let _ = write!(out, "#{color}");
            for (x, bits) in row.iter_mut().enumerate() {
                *bits = 0;
                for dy in 0..band_rows {
                    if indices[(band + dy) * width + x] == Some(color) {
                        *bits |= 1 << dy;
                    }
                }
            }
            let used = row
                .iter()
                .rposition(|&bits| bits != 0)
                .map_or(0, |last| last + 1);
            write_runs(&row[..used], out);
        }
        if band + 6 < height {
            // Graphics new line: next band.
            out.push(b'-');
        }
    }
    out.extend_from_slice(b"\x1b\\");
}

/// Writes sixel values with run-length encoding (`!count char`) for runs of 4 or more.
fn write_runs(values: &[u8], out: &mut Vec<u8>) {
    let mut index = 0;
    while index < values.len() {
        let value = values[index];
        let run = values[index..].iter().take_while(|&&v| v == value).count();
        let symbol = 63 + value;
        if run >= 4 {
            let _ = write!(out, "!{run}");
            out.push(symbol);
        } else {
            out.extend(std::iter::repeat_n(symbol, run));
        }
        index += run;
    }
}

fn is_opaque(pixel: Rgba) -> bool {
    pixel.a >= 128
}

/// The palette: the exact colors when there are few enough, otherwise median cut.
fn build_palette(pixels: &[Rgba], max_colors: usize) -> Vec<Rgb> {
    let mut counts: HashMap<Rgb, u32> = HashMap::new();
    for &p in pixels.iter().filter(|&&p| is_opaque(p)) {
        *counts.entry([p.r, p.g, p.b]).or_default() += 1;
    }
    let mut colors: Vec<(Rgb, u32)> = counts.into_iter().collect();
    // A stable order makes the output deterministic.
    colors.sort_unstable();
    if colors.len() <= max_colors {
        return colors.into_iter().map(|(color, _)| color).collect();
    }
    median_cut(colors, max_colors)
}

/// Splits the color space into `max_colors` boxes of similar weight and returns each box's
/// weighted mean color.
fn median_cut(colors: Vec<(Rgb, u32)>, max_colors: usize) -> Vec<Rgb> {
    let mut boxes = vec![colors];
    while boxes.len() < max_colors {
        // Split the box with the largest channel range, weighted by its pixel count.
        let candidate = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(index, b)| {
                let (channel, range) = widest_channel(b);
                let weight: u64 = b.iter().map(|&(_, count)| u64::from(count)).sum();
                (index, channel, u64::from(range) * weight)
            })
            .max_by_key(|&(_, _, score)| score);
        let Some((index, channel, _)) = candidate else {
            break;
        };
        let mut entries = boxes.swap_remove(index);
        entries.sort_unstable_by_key(|&(color, _)| color[channel]);
        let total: u64 = entries.iter().map(|&(_, count)| u64::from(count)).sum();
        let mut running = 0;
        let mut split = entries.len() / 2;
        for (i, &(_, count)) in entries.iter().enumerate() {
            running += u64::from(count);
            if running * 2 >= total {
                split = (i + 1).clamp(1, entries.len() - 1);
                break;
            }
        }
        let upper = entries.split_off(split);
        boxes.push(entries);
        boxes.push(upper);
    }
    boxes
        .iter()
        .map(|entries| {
            let mut sum = [0u64; 3];
            let mut total = 0u64;
            for &(color, count) in entries {
                for (s, &c) in sum.iter_mut().zip(&color) {
                    *s += u64::from(c) * u64::from(count);
                }
                total += u64::from(count);
            }
            sum.map(|s| u8::try_from((s + total / 2) / total.max(1)).unwrap_or(u8::MAX))
        })
        .collect()
}

/// The channel with the largest range in a box, and that range.
fn widest_channel(entries: &[(Rgb, u32)]) -> (usize, u8) {
    (0..3)
        .map(|channel| {
            let (min, max) = entries.iter().fold((u8::MAX, u8::MIN), |(lo, hi), (c, _)| {
                (lo.min(c[channel]), hi.max(c[channel]))
            });
            (channel, max - min)
        })
        .max_by_key(|&(_, range)| range)
        .unwrap_or((0, 0))
}

/// Palette index of every pixel (`None` for transparent pixels).
fn map_pixels(pixels: &[Rgba], width: usize, palette: &[Rgb], dither: bool) -> Vec<Option<u8>> {
    let mut cache: HashMap<Rgb, u8> = HashMap::new();
    let mut nearest = |color: Rgb| -> u8 {
        *cache.entry(color).or_insert_with(|| {
            let distance = |p: &Rgb| -> u32 {
                (0..3)
                    .map(|i| u32::from(p[i].abs_diff(color[i])).pow(2))
                    .sum()
            };
            let best = palette
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| distance(p))
                .map_or(0, |(index, _)| index);
            u8::try_from(best).unwrap_or(u8::MAX)
        })
    };
    if !dither {
        return pixels
            .iter()
            .map(|&p| is_opaque(p).then(|| nearest([p.r, p.g, p.b])))
            .collect();
    }

    // Floyd–Steinberg: the error of each pixel is spread to its unprocessed neighbors.
    let mut errors = vec![[0.0f32; 3]; pixels.len()];
    let mut out = vec![None; pixels.len()];
    for (index, &pixel) in pixels.iter().enumerate() {
        if !is_opaque(pixel) {
            continue;
        }
        let original = [pixel.r, pixel.g, pixel.b];
        let wanted: [f32; 3] =
            std::array::from_fn(|i| (f32::from(original[i]) + errors[index][i]).clamp(0.0, 255.0));
        let adjusted = wanted.map(crate::color::to_u8);
        let chosen = nearest(adjusted);
        out[index] = Some(chosen);
        let got = palette[usize::from(chosen)];
        let error: [f32; 3] = std::array::from_fn(|i| wanted[i] - f32::from(got[i]));
        let (x, y) = (index % width, index / width);
        let height = pixels.len() / width;
        let mut spread = |dx: isize, dy: usize, weight: f32| {
            let Some(nx) = x.checked_add_signed(dx) else {
                return;
            };
            let ny = y + dy;
            if nx >= width || ny >= height {
                return;
            }
            let target = ny * width + nx;
            if is_opaque(pixels[target]) {
                for i in 0..3 {
                    errors[target][i] += error[i] * weight;
                }
            }
        };
        spread(1, 0, 7.0 / 16.0);
        spread(-1, 1, 3.0 / 16.0);
        spread(0, 1, 5.0 / 16.0);
        spread(1, 1, 1.0 / 16.0);
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test helpers work with small, known values"
)]
mod tests {
    use super::*;

    /// A minimal Sixel decoder: returns the image as RGB colors (`None` = transparent).
    fn decode(data: &[u8]) -> (usize, usize, Vec<Option<Rgb>>) {
        let body = data
            .strip_prefix(b"\x1bP0;1;0q")
            .and_then(|b| b.strip_suffix(b"\x1b\\"))
            .expect("a complete sixel sequence");
        let text = std::str::from_utf8(body).unwrap();
        let mut chars = text.chars().peekable();
        let number = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
            let mut n = 0usize;
            while let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
                n = n * 10 + d as usize;
                chars.next();
            }
            n
        };
        let (mut width, mut height) = (0, 0);
        let mut palette: HashMap<usize, Rgb> = HashMap::new();
        let mut pixels = Vec::new();
        let (mut x, mut band, mut color) = (0usize, 0usize, 0usize);
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    let mut params = [0; 4];
                    for (i, p) in params.iter_mut().enumerate() {
                        *p = number(&mut chars);
                        if i < 3 {
                            assert_eq!(chars.next(), Some(';'));
                        }
                    }
                    (width, height) = (params[2], params[3]);
                    pixels = vec![None; width * height];
                }
                '#' => {
                    color = number(&mut chars);
                    if chars.peek() == Some(&';') {
                        chars.next();
                        let mut values = [0; 4];
                        for (i, v) in values.iter_mut().enumerate() {
                            *v = number(&mut chars);
                            if i < 3 {
                                chars.next();
                            }
                        }
                        let byte = |percent: usize| ((percent * 255 + 50) / 100) as u8;
                        palette.insert(color, [byte(values[1]), byte(values[2]), byte(values[3])]);
                    }
                }
                '$' => x = 0,
                '-' => {
                    x = 0;
                    band += 6;
                }
                '!' => {
                    let count = number(&mut chars);
                    let symbol = chars.next().unwrap();
                    for _ in 0..count {
                        paint(&mut pixels, width, height, x, band, symbol, palette[&color]);
                        x += 1;
                    }
                }
                symbol @ '?'..='~' => {
                    paint(&mut pixels, width, height, x, band, symbol, palette[&color]);
                    x += 1;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        (width, height, pixels)
    }

    fn paint(
        pixels: &mut [Option<Rgb>],
        w: usize,
        h: usize,
        x: usize,
        band: usize,
        symbol: char,
        rgb: Rgb,
    ) {
        let bits = symbol as u8 - 63;
        for dy in 0..6 {
            if bits & (1 << dy) != 0 && x < w && band + dy < h {
                pixels[(band + dy) * w + x] = Some(rgb);
            }
        }
    }

    fn encoded(canvas: &Canvas, options: SixelOptions) -> Vec<u8> {
        let mut out = Vec::new();
        encode(canvas, options, &mut out);
        out
    }

    #[test]
    fn exact_small_image() {
        let r = Rgba::rgb(255, 0, 0);
        let b = Rgba::rgb(0, 0, 255);
        let canvas = Canvas::from_art(&["rb", "r."], &[('r', r), ('b', b)]);
        let out = encoded(&canvas, SixelOptions::default());
        // Palette sorted by color: blue (#0) then red (#1).
        assert_eq!(
            String::from_utf8(out.clone()).unwrap(),
            "\x1bP0;1;0q\"1;1;2;2#0;2;0;0;100#1;2;100;0;0#0?@$#1B\x1b\\"
        );
        let (w, h, pixels) = decode(&out);
        assert_eq!((w, h), (2, 2));
        assert_eq!(
            pixels,
            [
                Some([255, 0, 0]),
                Some([0, 0, 255]),
                Some([255, 0, 0]),
                None
            ]
        );
    }

    #[test]
    fn round_trip_with_bands_and_runs() {
        let mut canvas = Canvas::new(23, 14);
        canvas.fill_rect(0, 0, 23, 14, Rgba::rgb(10, 200, 30));
        canvas.fill_rect(3, 2, 10, 9, Rgba::rgb(250, 250, 0));
        canvas.set_blend_mode(crate::BlendMode::Replace);
        canvas.pixel(22, 13, Rgba::TRANSPARENT);
        let out = encoded(&canvas, SixelOptions::default());
        assert!(
            out.windows(3).any(|w| w == b"!23" || w == b"!10"),
            "runs are compressed"
        );
        let (w, h, pixels) = decode(&out);
        assert_eq!((w, h), (23, 14));
        for (index, pixel) in canvas.pixels().iter().enumerate() {
            match pixels[index] {
                // Sixel colors are percentages, so channels may be off by one or two.
                Some(got) => {
                    assert!(pixel.a >= 128, "pixel {index} must be transparent");
                    let want = [pixel.r, pixel.g, pixel.b];
                    assert!(
                        got.iter().zip(&want).all(|(g, w)| g.abs_diff(*w) <= 2),
                        "pixel {index}: {got:?} vs {want:?}"
                    );
                }
                None => assert!(pixel.a < 128, "pixel {index} must be drawn"),
            }
        }
    }

    #[test]
    fn palette_is_limited() {
        let mut canvas = Canvas::new(64, 16);
        for y in 0..16 {
            for x in 0..64 {
                canvas.pixel(x, y, Rgba::rgb((x * 4) as u8, (y * 16) as u8, 128));
            }
        }
        // Limits for the mean squared error: per pixel, and of 4×4 block averages (what the
        // eye sees; dithering trades per-pixel accuracy for accurate averages). Reducing
        // this 2D gradient to 16 colors leaves steps of about 63 per channel, i.e. a
        // theoretical per-pixel error of about 2 × 63² / 12 ≈ 660.
        for (max_colors, dither, pixel_limit, block_limit) in [
            (16, false, 800, 800),
            (16, true, 2000, 150),
            (256, false, 40, 40),
        ] {
            let out = encoded(&canvas, SixelOptions { max_colors, dither });
            // Color definitions look like `#index;2;r;g;b`.
            let definitions = out
                .iter()
                .enumerate()
                .filter(|&(i, &b)| {
                    let digits = out[i + 1..]
                        .iter()
                        .take_while(|c| c.is_ascii_digit())
                        .count();
                    b == b'#' && digits > 0 && out[i + 1 + digits..].starts_with(b";2;")
                })
                .count();
            assert!(
                definitions <= usize::from(max_colors),
                "{definitions} colors"
            );
            let (_, _, pixels) = decode(&out);
            assert!(pixels.iter().all(Option::is_some));
            let got: Vec<[f32; 3]> = pixels.iter().map(|p| p.unwrap().map(f32::from)).collect();
            let want: Vec<[f32; 3]> = canvas
                .pixels()
                .iter()
                .map(|p| [p.r, p.g, p.b].map(f32::from))
                .collect();
            let squared =
                |a: [f32; 3], b: [f32; 3]| -> f32 { (0..3).map(|i| (a[i] - b[i]).powi(2)).sum() };
            // The reduced image stays close to the original.
            let pixel_error = got
                .iter()
                .zip(&want)
                .map(|(g, w)| squared(*g, *w))
                .sum::<f32>()
                / 1024.0;
            let block_mean = |image: &[[f32; 3]], bx: usize, by: usize| {
                let mut sum = [0.0f32; 3];
                for y in by * 4..by * 4 + 4 {
                    for x in bx * 4..bx * 4 + 4 {
                        for i in 0..3 {
                            sum[i] += image[y * 64 + x][i] / 16.0;
                        }
                    }
                }
                sum
            };
            let mut block_error = 0.0;
            for by in 0..4 {
                for bx in 0..16 {
                    block_error += squared(block_mean(&got, bx, by), block_mean(&want, bx, by));
                }
            }
            block_error /= 64.0;
            let label = format!("{max_colors} colors, dither {dither}");
            assert!(
                pixel_error < pixel_limit as f32,
                "{label}: pixel error {pixel_error}"
            );
            assert!(
                block_error < block_limit as f32,
                "{label}: block error {block_error}"
            );
        }
    }

    #[test]
    fn empty_and_transparent_images() {
        assert_eq!(encoded(&Canvas::new(0, 5), SixelOptions::default()), b"");
        let out = encoded(&Canvas::new(3, 7), SixelOptions::default());
        let (_, _, pixels) = decode(&out);
        assert!(pixels.iter().all(Option::is_none));
    }

    #[test]
    fn run_length_encoding() {
        let mut out = Vec::new();
        write_runs(&[1, 1, 1, 1, 1, 2, 2, 2, 0], &mut out);
        assert_eq!(out, b"!5@AAA?");
    }
}
