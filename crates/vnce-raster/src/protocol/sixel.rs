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
    Encoder::new().encode(canvas, options, out);
}

/// Encodes a sequence of images (for example the frames of a game), reusing the palette
/// while the colors stay about the same: encoding is faster, and colors do not flicker from
/// frame to frame. A new palette is made when more than 2 % of the pixels fall outside the
/// colors the current one was made for.
#[derive(Default)]
pub struct Encoder {
    cache: Option<Cached>,
}

/// A palette kept for the next images.
struct Cached {
    palette: Palette,
    /// The histogram bins of the image the palette was made for.
    bins: Vec<bool>,
    max_colors: usize,
    /// Whether the last image reused this palette.
    reused: bool,
}

impl std::fmt::Debug for Encoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encoder")
            .field(
                "palette",
                &self.cache.as_ref().map(|c| c.palette.colors.len()),
            )
            .finish()
    }
}

impl Encoder {
    /// An encoder without a palette yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the last image reused the palette of the one before.
    #[must_use]
    pub fn reused_palette(&self) -> bool {
        self.cache.as_ref().is_some_and(|c| c.reused)
    }

    /// Appends `canvas` as one Sixel image, like [`encode`].
    pub fn encode(&mut self, canvas: &Canvas, options: SixelOptions, out: &mut Vec<u8>) {
        let (width, height) = (canvas.width() as usize, canvas.height() as usize);
        if width == 0 || height == 0 {
            return;
        }
        let max_colors = usize::from(options.max_colors.clamp(1, 256));
        let histogram = Histogram::of(canvas.pixels());
        let reuse = self.cache.as_ref().is_some_and(|cache| {
            cache.max_colors == max_colors && histogram.covered_by(&cache.bins)
        });
        if reuse {
            if let Some(cache) = &mut self.cache {
                cache.reused = true;
            }
        } else {
            self.cache = Some(Cached {
                palette: build_palette(canvas.pixels(), &histogram, max_colors),
                bins: histogram.counts.iter().map(|&c| c > 0).collect(),
                max_colors,
                reused: false,
            });
        }
        let Some(cache) = &mut self.cache else {
            return;
        };
        write(canvas, options, &mut cache.palette, out);
    }
}

/// Writes the image with `palette`.
fn write(canvas: &Canvas, options: SixelOptions, palette: &mut Palette, out: &mut Vec<u8>) {
    let (width, height) = (canvas.width() as usize, canvas.height() as usize);
    let indices = map_pixels(canvas.pixels(), width, palette, options.dither);
    let palette = &palette.colors;

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

    let bands: Vec<usize> = (0..height).step_by(6).collect();
    // Bands are independent: large images are encoded on several threads, in contiguous
    // groups of bands joined in order, so the output is the same as on one thread.
    let threads = if width * height < PARALLEL_PIXELS {
        1
    } else {
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(bands.len())
    };
    if threads <= 1 {
        let mut band_encoder = BandEncoder::new(width);
        for &band in &bands {
            band_encoder.encode(&indices, height, band, out);
        }
    } else {
        let per_thread = bands.len().div_ceil(threads);
        let parts: Vec<Vec<u8>> = std::thread::scope(|scope| {
            let workers: Vec<_> = bands
                .chunks(per_thread)
                .map(|group| {
                    let indices = &indices;
                    scope.spawn(move || {
                        let mut band_encoder = BandEncoder::new(width);
                        let mut part = Vec::new();
                        for &band in group {
                            band_encoder.encode(indices, height, band, &mut part);
                        }
                        part
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap_or_default())
                .collect()
        });
        for part in parts {
            out.extend_from_slice(&part);
        }
    }
    out.extend_from_slice(b"\x1b\\");
}

/// Images with at least this many pixels are encoded on several threads.
const PARALLEL_PIXELS: usize = 200_000;

/// Encodes bands of six rows, reusing its buffers from band to band.
struct BandEncoder {
    width: usize,
    /// Sixel bits of each color used in the band, one row of `width` per slot.
    rows: Vec<u8>,
    /// The slot of each palette index in this band (`u16::MAX` = not used).
    slot_of: [u16; 256],
    /// Palette index, first and last column of each slot.
    slots: Vec<(u8, usize, usize)>,
}

impl BandEncoder {
    fn new(width: usize) -> Self {
        Self {
            width,
            rows: Vec::new(),
            slot_of: [u16::MAX; 256],
            slots: Vec::new(),
        }
    }

    /// Appends the band starting at row `band`, and a graphics new line if another band
    /// follows.
    fn encode(&mut self, indices: &[Option<u8>], height: usize, band: usize, out: &mut Vec<u8>) {
        let width = self.width;
        let band_rows = (height - band).min(6);
        // One pass over the band collects the bits of every color it uses.
        for dy in 0..band_rows {
            let row = &indices[(band + dy) * width..(band + dy + 1) * width];
            for (x, index) in row.iter().enumerate() {
                let Some(color) = *index else {
                    continue;
                };
                let mut slot = self.slot_of[usize::from(color)];
                if slot == u16::MAX {
                    slot = u16::try_from(self.slots.len()).unwrap_or(u16::MAX);
                    self.slot_of[usize::from(color)] = slot;
                    self.slots.push((color, x, x));
                    let needed = self.slots.len() * width;
                    if self.rows.len() < needed {
                        self.rows.resize(needed, 0);
                    }
                }
                let entry = &mut self.slots[usize::from(slot)];
                entry.1 = entry.1.min(x);
                entry.2 = entry.2.max(x);
                self.rows[usize::from(slot) * width + x] |= 1 << dy;
            }
        }
        // Colors in ascending order, as before, so the output stays deterministic.
        let mut order: Vec<usize> = (0..self.slots.len()).collect();
        order.sort_unstable_by_key(|&slot| self.slots[slot].0);
        for (n, &slot) in order.iter().enumerate() {
            let (color, first, last) = self.slots[slot];
            if n > 0 {
                // Graphics carriage return: back to the start of the band.
                out.push(b'$');
            }
            let _ = write!(out, "#{color}");
            let row = &mut self.rows[slot * width..(slot + 1) * width];
            // Columns before the first one used are empty: one run of zero bits.
            write_runs_of_zero(first, out);
            write_runs(&row[first..=last], out);
            row[first..=last].fill(0);
            self.slot_of[usize::from(color)] = u16::MAX;
        }
        self.slots.clear();
        if band + 6 < height {
            // Graphics new line: next band.
            out.push(b'-');
        }
    }
}

/// Writes `count` empty sixels, the same way [`write_runs`] would.
fn write_runs_of_zero(count: usize, out: &mut Vec<u8>) {
    if count >= 4 {
        let _ = write!(out, "!{count}");
        out.push(63);
    } else {
        out.extend(std::iter::repeat_n(63, count));
    }
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

/// Number of histogram bins: 5 bits per channel.
const BINS: usize = 1 << 15;

/// The histogram bin of a color.
fn bin(color: Rgb) -> usize {
    (usize::from(color[0] >> 3) << 10)
        | (usize::from(color[1] >> 3) << 5)
        | usize::from(color[2] >> 3)
}

/// Lookup table entry: not computed yet.
const UNSET: u16 = u16::MAX;
/// Lookup table entry: several exact palette colors share this bin; search them.
/// (Entries from `SEARCH` up are `SEARCH + n`: the colors of the bin are in `shared[n]`, so
/// only those few are searched, not the whole palette.)
const SEARCH: u16 = 256;

/// The colors of an image and a table from histogram bins to palette indices.
struct Palette {
    colors: Vec<Rgb>,
    lookup: Vec<u16>,
    /// The exact colors (and their indices) of bins shared by several palette colors.
    shared: Vec<Vec<(Rgb, u8)>>,
}

impl Palette {
    fn new(colors: Vec<Rgb>) -> Self {
        Self {
            colors,
            lookup: vec![UNSET; BINS],
            shared: Vec::new(),
        }
    }

    /// Records that palette color `index` shares its bin with other palette colors.
    fn share(&mut self, index: u8) {
        let color = self.colors[usize::from(index)];
        let b = bin(color);
        let entry = self.lookup[b];
        if (SEARCH..UNSET).contains(&entry) {
            self.shared[usize::from(entry - SEARCH)].push((color, index));
        } else {
            self.lookup[b] = SEARCH + u16::try_from(self.shared.len()).unwrap_or(0);
            self.shared.push(vec![(color, index)]);
        }
    }

    /// The palette index closest to `color`.
    fn index(&mut self, color: Rgb) -> u8 {
        let bin = bin(color);
        let entry = self.lookup[bin];
        match entry {
            entry if (SEARCH..UNSET).contains(&entry) => {
                let colors = &self.shared[usize::from(entry - SEARCH)];
                // Exact colors of the palette are found among the few in their bin.
                colors
                    .iter()
                    .find(|(c, _)| *c == color)
                    .map_or_else(|| nearest(&self.colors, color), |&(_, index)| index)
            }
            UNSET => {
                // The middle of the bin stands for every color in it.
                let center = color.map(|c| (c & !7) | 4);
                let index = nearest(&self.colors, center);
                self.lookup[bin] = u16::from(index);
                index
            }
            index => u8::try_from(index).unwrap_or(u8::MAX),
        }
    }
}

/// The index of the palette color closest to `color`.
fn nearest(palette: &[Rgb], color: Rgb) -> u8 {
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
}

/// Colors of an image counted in bins of 5 bits per channel.
struct Histogram {
    counts: Vec<u32>,
    sums: Vec<[u64; 3]>,
    // The first exact color seen in each bin, and whether other colors share the bin.
    first: Vec<Rgb>,
    mixed: Vec<bool>,
}

impl Histogram {
    fn of(pixels: &[Rgba]) -> Self {
        let mut histogram = Self {
            counts: vec![0; BINS],
            sums: vec![[0; 3]; BINS],
            first: vec![[0; 3]; BINS],
            mixed: vec![false; BINS],
        };
        for &p in pixels.iter().filter(|&&p| is_opaque(p)) {
            let color = [p.r, p.g, p.b];
            let b = bin(color);
            if histogram.counts[b] == 0 {
                histogram.first[b] = color;
            } else if histogram.first[b] != color {
                histogram.mixed[b] = true;
            }
            histogram.counts[b] += 1;
            for (sum, &c) in histogram.sums[b].iter_mut().zip(&color) {
                *sum += u64::from(c);
            }
        }
        histogram
    }

    /// Returns `true` if at least 98 % of the pixels fall into `bins`.
    fn covered_by(&self, bins: &[bool]) -> bool {
        let (mut total, mut covered) = (0_u64, 0_u64);
        for (&count, &known) in self.counts.iter().zip(bins) {
            total += u64::from(count);
            if known {
                covered += u64::from(count);
            }
        }
        covered * 100 >= total * 98
    }
}

/// The palette: the exact colors when there are few enough, otherwise median cut.
///
/// Colors are counted in a histogram of 5 bits per channel (32 768 bins) instead of one
/// entry per exact color, which is much faster on large images.
fn build_palette(pixels: &[Rgba], histogram: &Histogram, max_colors: usize) -> Palette {
    let Histogram {
        counts,
        sums,
        first,
        mixed,
    } = histogram;
    let used: Vec<usize> = (0..BINS).filter(|&b| counts[b] > 0).collect();

    if used.len() <= max_colors {
        if used.iter().all(|&b| !mixed[b]) {
            // Every bin holds a single exact color: those are the palette.
            let mut colors: Vec<Rgb> = used.iter().map(|&b| first[b]).collect();
            // A stable order makes the output deterministic.
            colors.sort_unstable();
            let mut palette = Palette::new(colors);
            for (index, color) in palette.colors.iter().enumerate() {
                palette.lookup[bin(*color)] = u16::try_from(index).unwrap_or(0);
            }
            return palette;
        }
        // Few bins but close colors share some: count the exact colors.
        let mut exact: HashMap<Rgb, u32> = HashMap::new();
        for &p in pixels.iter().filter(|&&p| is_opaque(p)) {
            *exact.entry([p.r, p.g, p.b]).or_default() += 1;
        }
        if exact.len() <= max_colors {
            let mut colors: Vec<Rgb> = exact.into_keys().collect();
            colors.sort_unstable();
            let mut palette = Palette::new(colors);
            for index in 0..palette.colors.len() {
                let b = bin(palette.colors[index]);
                let index = u8::try_from(index).unwrap_or(u8::MAX);
                if mixed[b] {
                    palette.share(index);
                } else {
                    palette.lookup[b] = u16::from(index);
                }
            }
            return palette;
        }
    }

    // Median cut over the bins, each with its mean color and pixel count.
    let entries: Vec<(Rgb, u32)> = used
        .iter()
        .map(|&b| {
            let count = u64::from(counts[b]);
            let mean = sums[b].map(|s| u8::try_from((s + count / 2) / count).unwrap_or(u8::MAX));
            (mean, counts[b])
        })
        .collect();
    let mut palette = Palette::new(median_cut(entries, max_colors));
    for &b in &used {
        let count = u64::from(counts[b]);
        let mean = sums[b].map(|s| u8::try_from((s + count / 2) / count).unwrap_or(u8::MAX));
        palette.lookup[b] = u16::from(nearest(&palette.colors, mean));
    }
    palette
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
fn map_pixels(
    pixels: &[Rgba],
    width: usize,
    palette: &mut Palette,
    dither: bool,
) -> Vec<Option<u8>> {
    if !dither {
        return pixels
            .iter()
            .map(|&p| is_opaque(p).then(|| palette.index([p.r, p.g, p.b])))
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
        let chosen = palette.index(adjusted);
        out[index] = Some(chosen);
        let got = palette.colors[usize::from(chosen)];
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

    /// Asserts that `out` decodes to `canvas` (within the rounding of Sixel percentages).
    fn assert_decodes_to(out: &[u8], canvas: &Canvas) {
        let (_, _, pixels) = decode(out);
        for (index, pixel) in canvas.pixels().iter().enumerate() {
            let got = pixels[index].expect("opaque pixel");
            let want = [pixel.r, pixel.g, pixel.b];
            assert!(
                got.iter().zip(&want).all(|(g, w)| g.abs_diff(*w) <= 2),
                "pixel {index}: {got:?} vs {want:?}"
            );
        }
    }

    #[test]
    fn close_colors_sharing_a_bin_keep_their_exact_colors() {
        // A sky gradient: neighbouring rows differ by one step, so several exact colors
        // fall into each 5-bit bin.
        let mut canvas = Canvas::new(8, 64);
        canvas.set_blend_mode(crate::BlendMode::Replace);
        for y in 0..64 {
            let v = 100 + y as u8;
            canvas.fill_rect(0, y, 8, 1, Rgba::rgb(v / 2, v, 200));
        }
        let mut palette = build_palette(canvas.pixels(), &Histogram::of(canvas.pixels()), 256);
        assert_eq!(palette.colors.len(), 64, "every exact color is kept");
        assert!(!palette.shared.is_empty(), "some bins are shared");
        for pixel in canvas.pixels() {
            let color = [pixel.r, pixel.g, pixel.b];
            let index = palette.index(color);
            assert_eq!(palette.colors[usize::from(index)], color);
        }
        // A color that is not in the palette maps to the nearest one.
        let other = [61, 122, 201];
        let index = palette.index(other);
        assert_eq!(palette.colors[usize::from(index)], [61, 122, 200]);
    }

    #[test]
    fn the_encoder_reuses_the_palette_while_the_colors_stay() {
        let background = Rgba::rgb(10, 20, 30);
        let red = Rgba::rgb(250, 10, 10);
        let mut encoder = Encoder::new();
        let frame = |x: i32, color: Rgba| {
            let mut canvas = Canvas::filled(30, 12, background);
            canvas.fill_rect(x, 3, 6, 6, color);
            canvas
        };
        let mut out = Vec::new();
        encoder.encode(&frame(2, red), SixelOptions::default(), &mut out);
        assert!(!encoder.reused_palette());
        assert_decodes_to(&out, &frame(2, red));
        // The square moved: same colors, same palette.
        out.clear();
        encoder.encode(&frame(12, red), SixelOptions::default(), &mut out);
        assert!(encoder.reused_palette());
        assert_decodes_to(&out, &frame(12, red));
        // A new color covering more than 2 % of the image: a new palette.
        let blue = Rgba::rgb(10, 10, 250);
        out.clear();
        encoder.encode(&frame(12, blue), SixelOptions::default(), &mut out);
        assert!(!encoder.reused_palette());
        assert_decodes_to(&out, &frame(12, blue));
        // A different palette size: a new palette.
        let small = SixelOptions {
            max_colors: 16,
            ..SixelOptions::default()
        };
        out.clear();
        encoder.encode(&frame(12, blue), small, &mut out);
        assert!(!encoder.reused_palette());
        assert!(format!("{encoder:?}").contains("Encoder"));
    }

    #[test]
    fn parallel_and_single_threaded_output_are_identical() {
        // Large enough to be encoded on several threads.
        let (width, height) = (700_u32, 300_u32);
        let mut canvas = Canvas::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let color = Rgba::rgb((x % 256) as u8, (y % 256) as u8, ((x + y) % 7 * 30) as u8);
                canvas.pixels_mut()[(y * width + x) as usize] = color;
            }
        }
        let options = SixelOptions::default();
        let parallel = encoded(&canvas, options);
        let mut palette = build_palette(canvas.pixels(), &Histogram::of(canvas.pixels()), 256);
        let indices = map_pixels(canvas.pixels(), width as usize, &mut palette, false);
        let mut single = Vec::new();
        let mut bands = BandEncoder::new(width as usize);
        for band in (0..height as usize).step_by(6) {
            bands.encode(&indices, height as usize, band, &mut single);
        }
        assert!(parallel.ends_with(&[&single[..], b"\x1b\\"].concat()));
    }

    #[test]
    fn run_length_encoding() {
        let mut out = Vec::new();
        write_runs(&[1, 1, 1, 1, 1, 2, 2, 2, 0], &mut out);
        assert_eq!(out, b"!5@AAA?");
    }
}
