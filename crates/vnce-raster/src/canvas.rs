//! The pixel canvas.

use crate::color::{BlendMode, Rgba, to_u8};

/// A rectangle in pixel coordinates. `x` and `y` may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PixelRect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl PixelRect {
    /// Creates a rectangle.
    #[must_use]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Half-open pixel bounds `x0..x1`, `y0..y1`, in `i64` so that no computation overflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub(crate) x0: i64,
    pub(crate) y0: i64,
    pub(crate) x1: i64,
    pub(crate) y1: i64,
}

impl Bounds {
    pub(crate) fn is_empty(self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    pub(crate) fn contains(self, x: i64, y: i64) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }

    fn from_rect(rect: PixelRect) -> Self {
        Self {
            x0: i64::from(rect.x),
            y0: i64::from(rect.y),
            x1: i64::from(rect.x) + i64::from(rect.width),
            y1: i64::from(rect.y) + i64::from(rect.height),
        }
    }
}

/// Options for [`Canvas::blit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlitOptions {
    /// Mirror the source horizontally.
    pub flip_x: bool,
    /// Mirror the source vertically.
    pub flip_y: bool,
    /// Source pixels with exactly this RGB color are skipped (classic sprite transparency).
    pub color_key: Option<Rgba>,
    /// Extra opacity multiplied into every source pixel (255 = unchanged).
    pub opacity: u8,
}

impl Default for BlitOptions {
    fn default() -> Self {
        Self {
            flip_x: false,
            flip_y: false,
            color_key: None,
            opacity: 255,
        }
    }
}

/// A grid of [`Rgba`] pixels with a clip rectangle and a blend mode.
///
/// Every drawing function goes through the clip rectangle and the blend mode, except
/// [`Canvas::clear`], which always overwrites the whole canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
    clip: Option<PixelRect>,
    blend: BlendMode,
}

impl Canvas {
    /// Creates a fully transparent canvas.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self::filled(width, height, Rgba::TRANSPARENT)
    }

    /// Creates a canvas filled with `color`.
    #[must_use]
    pub fn filled(width: u32, height: u32, color: Rgba) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; pixel_count(width, height)],
            clip: None,
            blend: BlendMode::default(),
        }
    }

    /// Creates a canvas from pixels stored row by row.
    ///
    /// Returns `None` if `pixels.len()` is not `width * height`.
    #[must_use]
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Rgba>) -> Option<Self> {
        (pixels.len() == pixel_count(width, height)).then_some(Self {
            width,
            height,
            pixels,
            clip: None,
            blend: BlendMode::default(),
        })
    }

    /// Creates a sprite from text art: each character is looked up in `palette`;
    /// characters that are not in the palette are transparent. Rows may have different
    /// lengths; the canvas is as wide as the longest row.
    ///
    /// ```
    /// use vnce_raster::{Canvas, Rgba};
    ///
    /// let red = Rgba::rgb(255, 0, 0);
    /// let sprite = Canvas::from_art(&[".r.", "rrr"], &[('r', red)]);
    /// assert_eq!(sprite.get(1, 0), Some(red));
    /// assert_eq!(sprite.get(0, 0), Some(Rgba::TRANSPARENT));
    /// ```
    #[must_use]
    pub fn from_art(rows: &[&str], palette: &[(char, Rgba)]) -> Self {
        let width = rows
            .iter()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0);
        let width = u32::try_from(width).unwrap_or(u32::MAX);
        let height = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let mut canvas = Self::new(width, height);
        for (y, row) in (0..height).zip(rows) {
            for (x, ch) in (0..width).zip(row.chars()) {
                if let Some(&(_, color)) = palette.iter().find(|(key, _)| *key == ch) {
                    canvas.put(i64::from(x), i64::from(y), color);
                }
            }
        }
        canvas
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// All pixels, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[Rgba] {
        &self.pixels
    }

    /// Mutable access to all pixels, row by row.
    pub fn pixels_mut(&mut self) -> &mut [Rgba] {
        &mut self.pixels
    }

    /// The pixel at `(x, y)`, or `None` if out of bounds.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Rgba> {
        self.index(i64::from(x), i64::from(y))
            .map(|index| self.pixels[index])
    }

    /// Resizes the canvas. All pixels become transparent; the clip rectangle is kept.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.pixels.clear();
        self.pixels
            .resize(pixel_count(width, height), Rgba::TRANSPARENT);
    }

    /// Sets every pixel to `color`, ignoring the clip rectangle and the blend mode.
    pub fn clear(&mut self, color: Rgba) {
        self.pixels.fill(color);
    }

    /// The current blend mode.
    #[must_use]
    pub fn blend_mode(&self) -> BlendMode {
        self.blend
    }

    /// Sets the blend mode used by all drawing functions.
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        self.blend = mode;
    }

    /// The current clip rectangle (`None` means the whole canvas).
    #[must_use]
    pub fn clip(&self) -> Option<PixelRect> {
        self.clip
    }

    /// Limits drawing to `clip` (`None` for the whole canvas).
    pub fn set_clip(&mut self, clip: Option<PixelRect>) {
        self.clip = clip;
    }

    /// Draws one pixel.
    pub fn pixel(&mut self, x: i32, y: i32, color: Rgba) {
        let bounds = self.bounds();
        let (x, y) = (i64::from(x), i64::from(y));
        if bounds.contains(x, y) {
            self.put(x, y, color);
        }
    }

    /// Copies `src` with its top-left corner at `(x, y)`.
    pub fn blit(&mut self, src: &Self, x: i32, y: i32, options: BlitOptions) {
        let region = PixelRect::new(0, 0, src.width, src.height);
        self.blit_region(src, region, x, y, options);
    }

    /// Copies the `region` of `src` (for example one frame of a sprite sheet) with its
    /// top-left corner at `(x, y)`. Flipping mirrors the region in place.
    pub fn blit_region(
        &mut self,
        src: &Self,
        region: PixelRect,
        x: i32,
        y: i32,
        options: BlitOptions,
    ) {
        let source = Bounds::from_rect(region).intersect(Bounds {
            x0: 0,
            y0: 0,
            x1: i64::from(src.width),
            y1: i64::from(src.height),
        });
        if source.is_empty() || options.opacity == 0 {
            return;
        }
        let width = source.x1 - source.x0;
        let height = source.y1 - source.y0;
        let target = Bounds {
            x0: i64::from(x),
            y0: i64::from(y),
            x1: i64::from(x) + width,
            y1: i64::from(y) + height,
        }
        .intersect(self.bounds());
        for ty in target.y0..target.y1 {
            let dy = ty - i64::from(y);
            let sy = if options.flip_y {
                source.y1 - 1 - dy
            } else {
                source.y0 + dy
            };
            for tx in target.x0..target.x1 {
                let dx = tx - i64::from(x);
                let sx = if options.flip_x {
                    source.x1 - 1 - dx
                } else {
                    source.x0 + dx
                };
                let Some(index) = src.index(sx, sy) else {
                    continue;
                };
                let mut pixel = src.pixels[index];
                if options
                    .color_key
                    .is_some_and(|key| (key.r, key.g, key.b) == (pixel.r, pixel.g, pixel.b))
                {
                    continue;
                }
                if options.opacity < 255 {
                    let alpha = u16::from(pixel.a) * u16::from(options.opacity) / 255;
                    pixel.a = u8::try_from(alpha).unwrap_or(u8::MAX);
                }
                self.put(tx, ty, pixel);
            }
        }
    }

    /// A copy scaled to `width × height`: bilinear when enlarging, area averaging when
    /// shrinking (so small details are averaged instead of skipped). Transparency is
    /// handled with premultiplied alpha, so transparent pixels do not darken edges.
    #[must_use]
    pub fn scaled(&self, width: u32, height: u32) -> Self {
        if (width, height) == (self.width, self.height) {
            let mut copy = self.clone();
            copy.clip = None;
            return copy;
        }
        let mut out = Self::new(width, height);
        if self.width == 0 || self.height == 0 || width == 0 || height == 0 {
            return out;
        }
        let x_axis = Axis::new(self.width, width);
        let y_axis = Axis::new(self.height, height);
        for y in 0..height {
            let rows = y_axis.taps(y);
            for x in 0..width {
                let columns = x_axis.taps(x);
                let mut sum = [0.0f32; 4];
                let mut total = 0.0;
                for &(sy, wy) in &rows {
                    for &(sx, wx) in &columns {
                        let weight = wx * wy;
                        let p = self.pixels[offset(self.width, sx, sy)];
                        let alpha = f32::from(p.a) / 255.0;
                        sum[0] += f32::from(p.r) * alpha * weight;
                        sum[1] += f32::from(p.g) * alpha * weight;
                        sum[2] += f32::from(p.b) * alpha * weight;
                        sum[3] += alpha * weight;
                        total += weight;
                    }
                }
                let alpha = if total > 0.0 { sum[3] / total } else { 0.0 };
                let pixel = if alpha <= 0.0 {
                    Rgba::TRANSPARENT
                } else {
                    let channel = |value: f32| to_u8(value / (alpha * total));
                    Rgba::new(
                        channel(sum[0]),
                        channel(sum[1]),
                        channel(sum[2]),
                        to_u8(alpha * 255.0),
                    )
                };
                out.pixels[offset(width, x, y)] = pixel;
            }
        }
        out
    }

    /// The area that can be drawn on: the canvas intersected with the clip rectangle.
    pub(crate) fn bounds(&self) -> Bounds {
        let canvas = Bounds {
            x0: 0,
            y0: 0,
            x1: i64::from(self.width),
            y1: i64::from(self.height),
        };
        match self.clip {
            Some(clip) => canvas.intersect(Bounds::from_rect(clip)),
            None => canvas,
        }
    }

    /// Index of `(x, y)` in `pixels`, or `None` if outside the canvas.
    pub(crate) fn index(&self, x: i64, y: i64) -> Option<usize> {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return None;
        }
        let index = y * i64::from(self.width) + x;
        usize::try_from(index).ok()
    }

    /// Blends `color` into the pixel at `(x, y)` if it is inside the canvas. The caller is
    /// responsible for clipping.
    pub(crate) fn put(&mut self, x: i64, y: i64, color: Rgba) {
        if let Some(index) = self.index(x, y) {
            let pixel = &mut self.pixels[index];
            *pixel = pixel.blend(color, self.blend);
        }
    }

    /// Blends `color` into the horizontal span `x0..=x1` on row `y`, clipped to `bounds`.
    pub(crate) fn span(&mut self, bounds: Bounds, x0: i64, x1: i64, y: i64, color: Rgba) {
        if y < bounds.y0 || y >= bounds.y1 {
            return;
        }
        let start = x0.max(bounds.x0);
        let end = (x1 + 1).min(bounds.x1);
        for x in start..end {
            self.put(x, y, color);
        }
    }
}

/// Number of pixels in a `width * height` canvas.
fn pixel_count(width: u32, height: u32) -> usize {
    usize::try_from(u64::from(width) * u64::from(height)).unwrap_or(usize::MAX)
}

/// Index of pixel `(x, y)` in a canvas of the given width.
fn offset(width: u32, x: u32, y: u32) -> usize {
    usize::try_from(u64::from(y) * u64::from(width) + u64::from(x)).unwrap_or(usize::MAX)
}

/// Maps destination pixels to weighted source pixels along one axis.
struct Axis {
    source: u32,
    target: u32,
}

impl Axis {
    fn new(source: u32, target: u32) -> Self {
        Self { source, target }
    }

    /// Source pixels and weights that make up destination pixel `index`.
    fn taps(&self, index: u32) -> Vec<(u32, f32)> {
        let ratio = f64::from(self.source) / f64::from(self.target);
        let last = self.source - 1;
        if self.target >= self.source {
            // Enlarging: linear interpolation between the two nearest source pixels.
            let center = ((f64::from(index) + 0.5) * ratio - 0.5).clamp(0.0, f64::from(last));
            let first = to_u32(center.floor()).min(last);
            let second = (first + 1).min(last);
            #[allow(clippy::cast_possible_truncation, reason = "a fraction in 0..1")]
            let t = (center - f64::from(first)) as f32;
            vec![(first, 1.0 - t), (second, t)]
        } else {
            // Shrinking: every source pixel covered by the destination pixel, weighted by
            // how much of it is covered.
            let start = f64::from(index) * ratio;
            let end = f64::from(index + 1) * ratio;
            (to_u32(start.floor())..to_u32(end.ceil()).min(self.source))
                .filter_map(|source| {
                    let covered = end.min(f64::from(source) + 1.0) - start.max(f64::from(source));
                    #[allow(clippy::cast_possible_truncation, reason = "a fraction in 0..1")]
                    let weight = covered as f32;
                    (weight > 0.0).then_some((source, weight))
                })
                .collect()
        }
    }
}

/// Converts a non-negative, already rounded value to `u32` (saturating).
fn to_u32(value: f64) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "saturating float to int conversion of a non-negative value"
    )]
    let result = value as u32;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rgba = Rgba::rgb(255, 0, 0);
    const G: Rgba = Rgba::rgb(0, 255, 0);

    #[test]
    fn pixels_are_clipped() {
        let mut canvas = Canvas::new(4, 4);
        canvas.pixel(-1, 0, R);
        canvas.pixel(4, 0, R);
        canvas.pixel(i32::MIN, i32::MAX, R);
        assert!(canvas.pixels().iter().all(|p| p.is_transparent()));

        canvas.set_clip(Some(PixelRect::new(1, 1, 2, 2)));
        canvas.pixel(0, 0, R);
        canvas.pixel(1, 1, R);
        canvas.pixel(3, 3, R);
        assert_eq!(canvas.get(0, 0), Some(Rgba::TRANSPARENT));
        assert_eq!(canvas.get(1, 1), Some(R));
        assert_eq!(canvas.get(3, 3), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn clear_ignores_clip_and_blend() {
        let mut canvas = Canvas::new(2, 2);
        canvas.set_clip(Some(PixelRect::new(0, 0, 1, 1)));
        canvas.clear(Rgba::new(1, 2, 3, 4));
        assert!(canvas.pixels().iter().all(|&p| p == Rgba::new(1, 2, 3, 4)));
    }

    #[test]
    fn from_pixels_checks_length() {
        assert!(Canvas::from_pixels(2, 2, vec![R; 4]).is_some());
        assert!(Canvas::from_pixels(2, 2, vec![R; 3]).is_none());
    }

    #[test]
    fn from_art_builds_sprites() {
        let sprite = Canvas::from_art(&["r.", "gr", "g"], &[('r', R), ('g', G)]);
        assert_eq!((sprite.width(), sprite.height()), (2, 3));
        assert_eq!(sprite.get(0, 0), Some(R));
        assert_eq!(sprite.get(1, 0), Some(Rgba::TRANSPARENT));
        assert_eq!(sprite.get(0, 1), Some(G));
        assert_eq!(sprite.get(1, 2), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn blit_with_flip_key_and_clipping() {
        let sprite = Canvas::from_art(&["rg"], &[('r', R), ('g', G)]);
        let mut canvas = Canvas::new(3, 1);
        canvas.blit(&sprite, 0, 0, BlitOptions::default());
        assert_eq!(canvas.pixels(), &[R, G, Rgba::TRANSPARENT]);

        let mut canvas = Canvas::new(3, 1);
        let flipped = BlitOptions {
            flip_x: true,
            ..BlitOptions::default()
        };
        canvas.blit(&sprite, 1, 0, flipped);
        assert_eq!(canvas.pixels(), &[Rgba::TRANSPARENT, G, R]);

        let mut canvas = Canvas::new(3, 1);
        let keyed = BlitOptions {
            color_key: Some(G),
            ..BlitOptions::default()
        };
        canvas.blit(&sprite, -1, 0, keyed);
        assert!(canvas.pixels().iter().all(|p| p.is_transparent()));

        canvas.blit(&sprite, i32::MAX, i32::MIN, BlitOptions::default());
        assert!(canvas.pixels().iter().all(|p| p.is_transparent()));
    }

    #[test]
    fn blit_region_and_opacity() {
        let sheet = Canvas::from_art(&["rrgg"], &[('r', R), ('g', G)]);
        let mut canvas = Canvas::filled(2, 1, Rgba::BLACK);
        let half = BlitOptions {
            opacity: 128,
            ..BlitOptions::default()
        };
        canvas.blit_region(&sheet, PixelRect::new(2, 0, 2, 1), 0, 0, half);
        let pixel = canvas.get(0, 0).unwrap();
        assert_eq!(pixel.r, 0);
        assert!(pixel.g.abs_diff(128) <= 1, "{pixel:?}");
    }

    #[test]
    fn resize_clears() {
        let mut canvas = Canvas::filled(2, 2, R);
        canvas.resize(3, 1);
        assert_eq!(canvas.pixels(), &[Rgba::TRANSPARENT; 3]);
    }

    #[test]
    fn scaling() {
        let canvas = Canvas::from_art(&["rg"], &[('r', R), ('g', G)]);
        assert_eq!(canvas.scaled(2, 1).pixels(), canvas.pixels());

        // Shrinking averages: half red, half green.
        let half = canvas.scaled(1, 1).get(0, 0).unwrap();
        assert!(half.r.abs_diff(128) <= 1 && half.g.abs_diff(128) <= 1 && half.a == 255);

        // Enlarging keeps the end colors and blends in between.
        let wide = canvas.scaled(4, 2);
        assert_eq!(wide.get(0, 0), Some(R));
        assert_eq!(wide.get(3, 1), Some(G));
        let middle = wide.get(1, 0).unwrap();
        assert!(middle.r > middle.g, "{middle:?}");

        assert_eq!(canvas.scaled(0, 5).pixels().len(), 0);
        assert_eq!(
            Canvas::new(0, 0).scaled(3, 3).pixels(),
            &[Rgba::TRANSPARENT; 9]
        );
    }

    #[test]
    fn scaling_does_not_darken_transparent_edges() {
        let canvas = Canvas::from_art(&["r."], &[('r', R)]);
        let shrunk = canvas.scaled(1, 1).get(0, 0).unwrap();
        // The color stays pure red; only the alpha is halved.
        assert_eq!((shrunk.r, shrunk.g, shrunk.b), (255, 0, 0));
        assert!(shrunk.a.abs_diff(128) <= 1);
    }
}
