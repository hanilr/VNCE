//! Drawing primitives.
//!
//! Coordinate convention: pixel `(x, y)` is the point `(x, y)`, so integer coordinates are
//! pixel centers for every primitive (lines, anti-aliased lines and triangles alike).

use crate::canvas::{Bounds, Canvas};
use crate::color::{Rgba, to_u8};

/// Margin (in pixels) around the drawable area within which lines are drawn without
/// clipping, so that ordinary lines keep their exact Bresenham pixels.
const LINE_MARGIN: i64 = 2;

impl Canvas {
    /// Draws a one-pixel line from `(x0, y0)` to `(x1, y1)`, both end points included
    /// (Bresenham). The result does not depend on the direction of the line.
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba) {
        let bounds = self.bounds();
        if bounds.is_empty() {
            return;
        }
        let mut a = (i64::from(x0), i64::from(y0));
        let mut b = (i64::from(x1), i64::from(y1));
        // A canonical direction makes `line(a, b)` and `line(b, a)` identical.
        if (b.1, b.0) < (a.1, a.0) {
            std::mem::swap(&mut a, &mut b);
        }
        let expanded = expand(bounds, LINE_MARGIN);
        if !expanded.contains(a.0, a.1) || !expanded.contains(b.0, b.1) {
            let point = |p: (i64, i64)| (to_f64(p.0), to_f64(p.1));
            let Some((ca, cb)) = clip_segment(point(a), point(b), expanded) else {
                return;
            };
            a = round_point(ca);
            b = round_point(cb);
        }

        let dx = (b.0 - a.0).abs();
        let dy = -(b.1 - a.1).abs();
        let sx = if a.0 < b.0 { 1 } else { -1 };
        let sy = if a.1 < b.1 { 1 } else { -1 };
        let mut error = dx + dy;
        let (mut x, mut y) = a;
        loop {
            if bounds.contains(x, y) {
                self.put(x, y, color);
            }
            if (x, y) == b {
                break;
            }
            let e2 = 2 * error;
            if e2 >= dy {
                error += dy;
                x += sx;
            }
            if e2 <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    /// Draws an anti-aliased line (Xiaolin Wu) between two points with sub-pixel
    /// precision. Coverage is applied through the alpha channel, so the result depends on
    /// the blend mode ([`BlendMode::Alpha`](crate::BlendMode::Alpha) is recommended).
    pub fn line_aa(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba) {
        let bounds = self.bounds();
        if bounds.is_empty() || ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return;
        }
        let expanded = expand(bounds, LINE_MARGIN);
        let Some((a, b)) = clip_segment(
            (f64::from(x0), f64::from(y0)),
            (f64::from(x1), f64::from(y1)),
            expanded,
        ) else {
            return;
        };
        let (mut ax, mut ay, mut bx, mut by) = (a.0, a.1, b.0, b.1);
        let steep = (by - ay).abs() > (bx - ax).abs();
        if steep {
            std::mem::swap(&mut ax, &mut ay);
            std::mem::swap(&mut bx, &mut by);
        }
        if ax > bx {
            std::mem::swap(&mut ax, &mut bx);
            std::mem::swap(&mut ay, &mut by);
        }
        let dx = bx - ax;
        let gradient = if dx == 0.0 { 1.0 } else { (by - ay) / dx };

        let mut plot = |x: f64, y: f64, coverage: f64| {
            let (px, py) = if steep { (y, x) } else { (x, y) };
            let (px, py) = (to_i64(px), to_i64(py));
            if coverage > 0.0 && bounds.contains(px, py) {
                let alpha = f64::from(color.a) * coverage.clamp(0.0, 1.0);
                #[allow(clippy::cast_possible_truncation, reason = "0..=255 fits in f32")]
                let alpha = to_u8(alpha as f32);
                if alpha > 0 {
                    self.put(px, py, color.with_alpha(alpha));
                }
            }
        };

        // First end point.
        let x_end = ax.round();
        let y_end = ay + gradient * (x_end - ax);
        let x_gap = 1.0 - (ax + 0.5).fract();
        let x_start = x_end;
        plot(x_start, y_end.floor(), (1.0 - y_end.fract()) * x_gap);
        plot(x_start, y_end.floor() + 1.0, y_end.fract() * x_gap);
        let mut inter_y = y_end + gradient;

        // Second end point.
        let x_end2 = bx.round();
        let y_end2 = by + gradient * (x_end2 - bx);
        let x_gap2 = (bx + 0.5).fract();
        if x_end2 > x_start {
            plot(x_end2, y_end2.floor(), (1.0 - y_end2.fract()) * x_gap2);
            plot(x_end2, y_end2.floor() + 1.0, y_end2.fract() * x_gap2);
        }

        // Main span.
        let mut x = x_start + 1.0;
        while x < x_end2 {
            plot(x, inter_y.floor(), 1.0 - inter_y.fract());
            plot(x, inter_y.floor() + 1.0, inter_y.fract());
            inter_y += gradient;
            x += 1.0;
        }
    }

    /// Draws the outline of a rectangle. Every pixel is drawn once, which matters for
    /// semi-transparent colors.
    pub fn rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: Rgba) {
        if width == 0 || height == 0 {
            return;
        }
        let bounds = self.bounds();
        let (x0, y0) = (i64::from(x), i64::from(y));
        let (x1, y1) = (x0 + i64::from(width) - 1, y0 + i64::from(height) - 1);
        self.span(bounds, x0, x1, y0, color);
        if y1 > y0 {
            self.span(bounds, x0, x1, y1, color);
        }
        for row in (y0 + 1).max(bounds.y0)..y1.min(bounds.y1) {
            self.span(bounds, x0, x0, row, color);
            if x1 > x0 {
                self.span(bounds, x1, x1, row, color);
            }
        }
    }

    /// Fills a rectangle.
    pub fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: Rgba) {
        if width == 0 || height == 0 {
            return;
        }
        let bounds = self.bounds();
        let (x0, y0) = (i64::from(x), i64::from(y));
        let (x1, y1) = (x0 + i64::from(width) - 1, y0 + i64::from(height) - 1);
        for row in y0.max(bounds.y0)..=y1.min(bounds.y1 - 1) {
            self.span(bounds, x0, x1, row, color);
        }
    }

    /// Draws the outline of a circle. The outline is exactly the border of
    /// [`Canvas::fill_circle`] with the same arguments, and every pixel is drawn once.
    pub fn circle(&mut self, cx: i32, cy: i32, radius: u32, color: Rgba) {
        let bounds = self.bounds();
        let (cx, cy) = (i64::from(cx), i64::from(cy));
        let r = i64::from(radius);
        for dy in (-r).max(bounds.y0 - cy)..=r.min(bounds.y1 - 1 - cy) {
            let half = circle_half_width(radius, dy);
            let inner = (half - 1)
                .min(circle_half_width(radius, dy - 1))
                .min(circle_half_width(radius, dy + 1));
            if inner < 0 {
                self.span(bounds, cx - half, cx + half, cy + dy, color);
            } else {
                self.span(bounds, cx - half, cx - inner - 1, cy + dy, color);
                self.span(bounds, cx + inner + 1, cx + half, cy + dy, color);
            }
        }
    }

    /// Fills a circle: every pixel whose center is within `radius + 0.5` of the center.
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: u32, color: Rgba) {
        let bounds = self.bounds();
        let (cx, cy) = (i64::from(cx), i64::from(cy));
        let r = i64::from(radius);
        for dy in (-r).max(bounds.y0 - cy)..=r.min(bounds.y1 - 1 - cy) {
            let half = circle_half_width(radius, dy);
            self.span(bounds, cx - half, cx + half, cy + dy, color);
        }
    }

    /// Fills a triangle with one color. Vertices have sub-pixel precision.
    ///
    /// Pixels exactly on an edge follow the top-left rule, so triangles that share an edge
    /// never draw the same pixel twice (no seams, no double blending).
    pub fn fill_triangle(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), color: Rgba) {
        self.rasterize_triangle(
            [a, b, c],
            &TriangleFill::<fn([f64; 3]) -> Rgba>::Solid(color),
        );
    }

    /// Fills a triangle whose color (including alpha) is interpolated between the colors
    /// of its vertices.
    pub fn fill_triangle_gradient(
        &mut self,
        (a, color_a): ((f32, f32), Rgba),
        (b, color_b): ((f32, f32), Rgba),
        (c, color_c): ((f32, f32), Rgba),
    ) {
        let colors = [color_a, color_b, color_c];
        self.rasterize_triangle(
            [a, b, c],
            &TriangleFill::Shaded(|weights: [f64; 3]| {
                let channel = |get: fn(Rgba) -> u8| {
                    #[allow(clippy::cast_possible_truncation, reason = "0..=255 fits in f32")]
                    let value = (f64::from(get(colors[0])) * weights[0]
                        + f64::from(get(colors[1])) * weights[1]
                        + f64::from(get(colors[2])) * weights[2])
                        as f32;
                    to_u8(value)
                };
                Rgba::new(
                    channel(|c| c.r),
                    channel(|c| c.g),
                    channel(|c| c.b),
                    channel(|c| c.a),
                )
            }),
        );
    }

    /// Rasterizes a triangle with edge functions. A [`TriangleFill::Shaded`] triangle calls
    /// its shader with the barycentric weights of every covered pixel.
    fn rasterize_triangle<F: Fn([f64; 3]) -> Rgba>(
        &mut self,
        vertices: [(f32, f32); 3],
        fill: &TriangleFill<F>,
    ) {
        let bounds = self.bounds();
        if bounds.is_empty() || !vertices.iter().all(|v| v.0.is_finite() && v.1.is_finite()) {
            return;
        }
        let mut v = vertices.map(|(x, y)| (f64::from(x), f64::from(y)));
        let mut order = [0, 1, 2];
        let mut area = edge(v[0], v[1], v[2]);
        if area < 0.0 {
            v.swap(1, 2);
            order.swap(1, 2);
            area = -area;
        }
        if area == 0.0 {
            return;
        }

        let min_x = v.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).ceil();
        let max_x = v
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .floor();
        let min_y = v.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).ceil();
        let max_y = v
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .floor();
        let x_start = to_i64(min_x).max(bounds.x0);
        let x_end = to_i64(max_x).min(bounds.x1 - 1);
        let y_start = to_i64(min_y).max(bounds.y0);
        let y_end = to_i64(max_y).min(bounds.y1 - 1);

        let edges = [(v[1], v[2]), (v[2], v[0]), (v[0], v[1])];
        let top_left = edges.map(|(from, to)| is_top_left(from, to));
        let weights_at = |x: i64, y: i64| {
            let p = (to_f64(x), to_f64(y));
            edges.map(|(from, to)| edge(from, to, p))
        };
        let covered = |x: i64, y: i64| {
            let w = weights_at(x, y);
            (0..3).all(|i| w[i] > 0.0 || (w[i] == 0.0 && top_left[i]))
        };
        for y in y_start..=y_end {
            let Some((left, right)) = covered_run(&edges, y, x_start, x_end, |x| covered(x, y))
            else {
                continue;
            };
            match fill {
                TriangleFill::Solid(color) => self.span(bounds, left, right, y, *color),
                TriangleFill::Shaded(shade) => {
                    for x in left..=right {
                        let w = weights_at(x, y);
                        // Weights in the original vertex order.
                        let mut weights = [0.0; 3];
                        for i in 0..3 {
                            weights[order[i]] = w[i] / area;
                        }
                        let color = shade(weights);
                        self.put(x, y, color);
                    }
                }
            }
        }
    }
}

/// The covered pixels `left..=right` of row `y` of a triangle, or `None` if none is
/// covered. `covered` is the exact pixel test.
///
/// On a row, every edge function is monotonic in `x` even in floating point (the
/// subtraction and the product round monotonically), so the pixels inside each edge form
/// a half-line and the covered pixels form one run. The ends of the run are estimated
/// from where each edge crosses the row, then settled with the exact test, so only a few
/// pixels per row are tested and the result is exactly the pixels a full scan would find.
fn covered_run(
    edges: &[Edge; 3],
    y: i64,
    x_start: i64,
    x_end: i64,
    covered: impl Fn(i64) -> bool,
) -> Option<(i64, i64)> {
    if x_start > x_end {
        return None;
    }
    // Where each edge function crosses zero on this row: covered pixels lie between the
    // largest lower bound and the smallest upper bound.
    let (mut lower, mut upper) = (f64::NEG_INFINITY, f64::INFINITY);
    for &(from, to) in edges {
        let slope = to.1 - from.1;
        let constant = (to.0 - from.0) * (to_f64(y) - from.1);
        if slope > 0.0 {
            upper = upper.min(from.0 + constant / slope);
        } else if slope < 0.0 {
            lower = lower.max(from.0 + constant / slope);
        }
    }
    let guess = |value: f64, otherwise: i64| {
        if value.is_finite() {
            to_i64(value)
        } else {
            otherwise
        }
    };

    // Left end: walk from the estimate to the first covered pixel. If the estimate was
    // right of the run, nothing is found and the row is scanned from its start.
    let mut left = guess(lower.ceil(), x_start).clamp(x_start, x_end + 1);
    while left > x_start && covered(left - 1) {
        left -= 1;
    }
    while left <= x_end && !covered(left) {
        left += 1;
    }
    if left > x_end {
        left = x_start;
        while left <= x_end && !covered(left) {
            left += 1;
        }
        if left > x_end {
            return None;
        }
    }

    // Right end: `left` is covered, so walking left always stops.
    let mut right = guess(upper.floor(), x_end).clamp(left, x_end);
    while right < x_end && covered(right + 1) {
        right += 1;
    }
    while !covered(right) {
        right -= 1;
    }
    Some((left, right))
}

/// A triangle edge: from one corner to the next.
type Edge = ((f64, f64), (f64, f64));

/// How a triangle is filled.
enum TriangleFill<F> {
    /// One color: covered pixels are drawn in horizontal runs.
    Solid(Rgba),
    /// A color for every pixel from its barycentric weights.
    Shaded(F),
}

/// Edge function: positive when `p` is on the inner side of the edge `a -> b` for
/// triangles with positive area.
fn edge(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

/// Top-left rule for the edge `from -> to` of a triangle with positive area (y points
/// down): a top edge is horizontal going right, a left edge goes up.
fn is_top_left(from: (f64, f64), to: (f64, f64)) -> bool {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    (dy == 0.0 && dx > 0.0) || dy < 0.0
}

/// Half width of the circle row `dy`: pixels `-half..=half` are inside, or -1 when the row
/// is outside the circle. A pixel is inside when `dx² + dy² <= r² + r` (radius + 0.5).
fn circle_half_width(radius: u32, dy: i64) -> i64 {
    let r = i128::from(radius);
    let dy = i128::from(dy);
    let limit = r * r + r - dy * dy;
    if limit < 0 {
        return -1;
    }
    i64::try_from(isqrt(limit.unsigned_abs())).unwrap_or(i64::MAX)
}

/// Integer square root (floor).
fn isqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the estimate is corrected exactly below"
    )]
    let mut root = (value as f64).sqrt() as u128;
    while root * root > value {
        root -= 1;
    }
    while (root + 1) * (root + 1) <= value {
        root += 1;
    }
    root
}

/// Grows bounds by `margin` pixels on every side.
fn expand(bounds: Bounds, margin: i64) -> Bounds {
    Bounds {
        x0: bounds.x0 - margin,
        y0: bounds.y0 - margin,
        x1: bounds.x1 + margin,
        y1: bounds.y1 + margin,
    }
}

/// Clips the segment `a -> b` to `bounds` (Liang–Barsky). Returns `None` if nothing is
/// left.
fn clip_segment(a: (f64, f64), b: (f64, f64), bounds: Bounds) -> Option<((f64, f64), (f64, f64))> {
    let (x_min, y_min) = (to_f64(bounds.x0), to_f64(bounds.y0));
    let (x_max, y_max) = (to_f64(bounds.x1 - 1), to_f64(bounds.y1 - 1));
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-dx, a.0 - x_min),
        (dx, x_max - a.0),
        (-dy, a.1 - y_min),
        (dy, y_max - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    Some((
        (a.0 + t0 * dx, a.1 + t0 * dy),
        (a.0 + t1 * dx, a.1 + t1 * dy),
    ))
}

fn round_point(p: (f64, f64)) -> (i64, i64) {
    (to_i64(p.0.round()), to_i64(p.1.round()))
}

/// Converts a pixel coordinate to `f64`. Coordinates come from `i32` values or canvas sizes
/// plus small margins, so they are far below 2^52 and convert exactly.
fn to_f64(value: i64) -> f64 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "pixel coordinates are far below 2^52"
    )]
    let result = value as f64;
    result
}

/// Converts a finite, already rounded value to `i64` (saturating).
fn to_i64(value: f64) -> i64 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "saturating float to int conversion"
    )]
    let result = value as i64;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::BlendMode;

    /// Fills a triangle by testing every pixel of the canvas, the way `rasterize_triangle`
    /// worked before it searched for the ends of each row.
    fn triangle_reference(canvas: &mut Canvas, vertices: [(f32, f32); 3], colors: [Rgba; 3]) {
        let mut v = vertices.map(|(x, y)| (f64::from(x), f64::from(y)));
        let mut order = [0, 1, 2];
        let mut area = edge(v[0], v[1], v[2]);
        if area < 0.0 {
            v.swap(1, 2);
            order.swap(1, 2);
            area = -area;
        }
        if area == 0.0 {
            return;
        }
        let edges = [(v[1], v[2]), (v[2], v[0]), (v[0], v[1])];
        let top_left = edges.map(|(from, to)| is_top_left(from, to));
        for y in 0..i64::from(canvas.height()) {
            for x in 0..i64::from(canvas.width()) {
                let p = (to_f64(x), to_f64(y));
                let w = edges.map(|(from, to)| edge(from, to, p));
                if (0..3).all(|i| w[i] > 0.0 || (w[i] == 0.0 && top_left[i])) {
                    let mut weights = [0.0; 3];
                    for i in 0..3 {
                        weights[order[i]] = w[i] / area;
                    }
                    let channel = |get: fn(Rgba) -> u8| {
                        #[allow(clippy::cast_possible_truncation, reason = "0..=255")]
                        let value = (f64::from(get(colors[0])) * weights[0]
                            + f64::from(get(colors[1])) * weights[1]
                            + f64::from(get(colors[2])) * weights[2])
                            as f32;
                        to_u8(value)
                    };
                    let color = Rgba::new(
                        channel(|c| c.r),
                        channel(|c| c.g),
                        channel(|c| c.b),
                        channel(|c| c.a),
                    );
                    canvas.pixel(
                        i32::try_from(x).unwrap_or(0),
                        i32::try_from(y).unwrap_or(0),
                        color,
                    );
                }
            }
        }
    }

    #[test]
    fn triangle_rows_match_a_full_scan() {
        // A small xorshift generator. Every third triangle has its corners on a quarter-pixel
        // grid, so its edges pass exactly through pixel centers, where the top-left rule
        // decides.
        let mut state = 0x7a1a_9e1e_u64;
        let mut next = move |n: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % n
        };
        let colors = [
            Rgba::new(255, 0, 0, 120),
            Rgba::new(0, 255, 0, 200),
            Rgba::new(0, 0, 255, 60),
        ];
        for round in 0..3000 {
            let grid = round % 3 == 0;
            let mut point = || {
                #[allow(clippy::cast_precision_loss, reason = "small numbers")]
                let (x, y) = if grid {
                    (next(200) as f32 / 4.0 - 6.0, next(200) as f32 / 4.0 - 6.0)
                } else {
                    (
                        next(50_000) as f32 / 1000.0 - 6.0,
                        next(50_000) as f32 / 1000.0 - 6.0,
                    )
                };
                (x, y)
            };
            let vertices = [point(), point(), point()];
            let mut canvas = Canvas::new(32, 28);
            canvas.set_blend_mode(BlendMode::Alpha);
            let mut expected = canvas.clone();
            if round % 2 == 0 {
                canvas.fill_triangle(vertices[0], vertices[1], vertices[2], colors[0]);
                triangle_reference(&mut expected, vertices, [colors[0]; 3]);
            } else {
                canvas.fill_triangle_gradient(
                    (vertices[0], colors[0]),
                    (vertices[1], colors[1]),
                    (vertices[2], colors[2]),
                );
                triangle_reference(&mut expected, vertices, colors);
            }
            assert_eq!(canvas.pixels(), expected.pixels(), "{vertices:?}");
        }
    }

    const W: Rgba = Rgba::WHITE;

    /// The canvas as text: `#` for drawn pixels, `.` for transparent ones.
    fn art(canvas: &Canvas) -> Vec<String> {
        (0..canvas.height())
            .map(|y| {
                (0..canvas.width())
                    .map(|x| {
                        if canvas.get(x, y).is_some_and(|p| !p.is_transparent()) {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn count(canvas: &Canvas) -> usize {
        canvas
            .pixels()
            .iter()
            .filter(|p| !p.is_transparent())
            .count()
    }

    #[test]
    fn line_shapes() {
        let mut canvas = Canvas::new(5, 3);
        canvas.line(0, 0, 4, 2, W);
        assert_eq!(art(&canvas), ["#....", ".##..", "...##"]);

        let mut canvas = Canvas::new(3, 3);
        canvas.line(1, 0, 1, 2, W);
        assert_eq!(art(&canvas), [".#.", ".#.", ".#."]);

        let mut canvas = Canvas::new(2, 2);
        canvas.line(1, 1, 1, 1, W);
        assert_eq!(art(&canvas), ["..", ".#"]);
    }

    #[test]
    fn line_direction_does_not_matter() {
        for (x0, y0, x1, y1) in [(0, 0, 7, 3), (1, 5, 6, 0), (0, 2, 7, 2), (3, 0, 4, 7)] {
            let mut forward = Canvas::new(8, 8);
            forward.line(x0, y0, x1, y1, W);
            let mut backward = Canvas::new(8, 8);
            backward.line(x1, y1, x0, y0, W);
            assert_eq!(art(&forward), art(&backward));
        }
    }

    #[test]
    fn huge_lines_are_clipped_quickly() {
        let mut canvas = Canvas::new(10, 10);
        canvas.line(i32::MIN, i32::MIN, i32::MAX, i32::MAX, W);
        // The diagonal crosses the whole canvas.
        assert_eq!(count(&canvas), 10);
        canvas.line(i32::MIN, 5, i32::MAX, 5, W);
        assert!((0..10).all(|x| canvas.get(x, 5) == Some(W)));
        let mut canvas = Canvas::new(10, 10);
        canvas.line(-100, -5, 100, -5, W);
        assert_eq!(count(&canvas), 0);
    }

    #[test]
    fn rect_outline_and_fill() {
        let mut canvas = Canvas::new(5, 4);
        canvas.rect(0, 0, 5, 4, W);
        assert_eq!(art(&canvas), ["#####", "#...#", "#...#", "#####"]);

        let mut canvas = Canvas::new(4, 4);
        canvas.fill_rect(-2, 1, 4, 10, W);
        assert_eq!(art(&canvas), ["....", "##..", "##..", "##.."]);
    }

    #[test]
    fn semi_transparent_outlines_draw_each_pixel_once() {
        let color = Rgba::new(255, 255, 255, 100);
        let mut canvas = Canvas::new(9, 9);
        canvas.rect(0, 0, 9, 9, color);
        canvas.circle(4, 4, 3, color);
        assert!(canvas.pixels().iter().all(|p| p.a == 0 || p.a == 100));

        // A 1x1 and a 1-wide rectangle are single spans.
        let mut canvas = Canvas::new(3, 3);
        canvas.rect(1, 0, 1, 3, color);
        assert!(canvas.pixels().iter().all(|p| p.a == 0 || p.a == 100));
    }

    #[test]
    fn circles() {
        let mut canvas = Canvas::new(3, 3);
        canvas.circle(1, 1, 1, W);
        assert_eq!(art(&canvas), ["###", "#.#", "###"]);

        let mut canvas = Canvas::new(7, 7);
        canvas.fill_circle(3, 3, 3, W);
        assert_eq!(
            art(&canvas),
            [
                "..###..", ".#####.", "#######", "#######", "#######", ".#####.", "..###.."
            ]
        );

        let mut outline = Canvas::new(7, 7);
        outline.circle(3, 3, 3, W);
        assert_eq!(
            art(&outline),
            [
                "..###..", ".#...#.", "#.....#", "#.....#", "#.....#", ".#...#.", "..###.."
            ]
        );

        let mut dot = Canvas::new(3, 3);
        dot.circle(1, 1, 0, W);
        assert_eq!(art(&dot), ["...", ".#.", "..."]);
    }

    #[test]
    fn circles_are_symmetric_and_clipped() {
        let mut canvas = Canvas::new(21, 21);
        canvas.circle(10, 10, 9, W);
        for y in 0..21 {
            for x in 0..21 {
                let p = canvas.get(x, y);
                assert_eq!(p, canvas.get(20 - x, y));
                assert_eq!(p, canvas.get(y, x));
            }
        }
        let mut canvas = Canvas::new(4, 4);
        canvas.fill_circle(0, 0, u32::MAX, W);
        assert_eq!(count(&canvas), 16);
        canvas.circle(i32::MIN, i32::MIN, 5, W);
    }

    #[test]
    fn triangles_sharing_an_edge_do_not_overlap() {
        // Two triangles covering a 6x6 square (pixel centers 0..=5 inside [-0.5, 5.5]).
        let color = Rgba::new(255, 255, 255, 100);
        let mut canvas = Canvas::new(8, 8);
        let (a, b, c, d) = ((-0.5, -0.5), (5.5, -0.5), (5.5, 5.5), (-0.5, 5.5));
        canvas.fill_triangle(a, b, c, color);
        canvas.fill_triangle(a, c, d, color);
        assert_eq!(count(&canvas), 36);
        assert!(canvas.pixels().iter().all(|p| p.a == 0 || p.a == 100));

        // Same with integer vertices on pixel centers: shared edges through pixel centers.
        let mut canvas = Canvas::new(8, 8);
        canvas.fill_triangle((0.0, 0.0), (6.0, 0.0), (6.0, 6.0), color);
        canvas.fill_triangle((0.0, 0.0), (6.0, 6.0), (0.0, 6.0), color);
        assert!(canvas.pixels().iter().all(|p| p.a == 0 || p.a == 100));
        assert_eq!(count(&canvas), 36);
    }

    #[test]
    fn triangle_winding_does_not_matter() {
        let mut cw = Canvas::new(8, 8);
        cw.fill_triangle((0.0, 0.0), (7.0, 0.0), (0.0, 7.0), W);
        let mut ccw = Canvas::new(8, 8);
        ccw.fill_triangle((0.0, 0.0), (0.0, 7.0), (7.0, 0.0), W);
        assert_eq!(art(&cw), art(&ccw));
        assert!(count(&cw) > 20);

        let mut degenerate = Canvas::new(4, 4);
        degenerate.fill_triangle((0.0, 0.0), (1.0, 1.0), (2.0, 2.0), W);
        degenerate.fill_triangle((f32::NAN, 0.0), (1.0, 1.0), (2.0, 0.0), W);
        degenerate.fill_triangle((-1e30, -1e30), (1e30, -1e30), (0.0, 1e30), W);
        assert_eq!(count(&degenerate), 16);
    }

    #[test]
    fn gradient_triangle_interpolates() {
        let mut canvas = Canvas::new(11, 11);
        canvas.set_blend_mode(BlendMode::Replace);
        let red = Rgba::rgb(255, 0, 0);
        let blue = Rgba::rgb(0, 0, 255);
        canvas.fill_triangle_gradient(((0.0, 0.0), red), ((10.0, 0.0), blue), ((0.0, 10.0), red));
        assert_eq!(canvas.get(0, 0), Some(red));
        let middle = canvas.get(5, 0).unwrap();
        assert!(
            middle.r.abs_diff(128) <= 1 && middle.b.abs_diff(128) <= 1,
            "{middle:?}"
        );
    }

    #[test]
    fn anti_aliased_lines() {
        let mut canvas = Canvas::new(10, 5);
        canvas.line_aa(0.0, 2.0, 9.0, 2.0, W);
        // A horizontal line on pixel centers is fully opaque on its row.
        assert!((1..9).all(|x| canvas.get(x, 2) == Some(W)));
        assert!((1..9).all(|x| canvas.get(x, 1).is_some_and(Rgba::is_transparent)));

        let mut canvas = Canvas::new(10, 5);
        canvas.line_aa(0.0, 1.5, 9.0, 1.5, W);
        // Halfway between two rows: both rows get about half coverage.
        let upper = canvas.get(4, 1).unwrap();
        let lower = canvas.get(4, 2).unwrap();
        assert!(upper.a.abs_diff(128) <= 2 && lower.a.abs_diff(128) <= 2);

        let mut canvas = Canvas::new(4, 4);
        canvas.line_aa(f32::MIN, f32::MIN, f32::MAX, f32::MAX, W);
        canvas.line_aa(f32::NAN, 0.0, 1.0, 1.0, W);
        canvas.line_aa(f32::INFINITY, 0.0, 1.0, 1.0, W);
    }
}
