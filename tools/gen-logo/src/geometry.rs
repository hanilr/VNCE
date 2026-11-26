//! Vector paths: building, stroking lines, flattening into polygons and writing SVG path
//! data.

use std::fmt::Write as _;

/// A point in a design space: the logo's 512 x 512, or the banner's 1280 x 640.
pub type Point = (f32, f32);

/// The width (and height) of the logo's design space.
pub const SIZE: f32 = 512.0;

/// One path command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    /// Starts a new sub-path.
    Move(Point),
    /// A straight line.
    Line(Point),
    /// A cubic Bézier curve: two control points, then the end point.
    Cubic(Point, Point, Point),
    /// Closes the current sub-path.
    Close,
}

/// A closed shape made of one or more sub-paths, filled with the nonzero rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    /// The commands, in order.
    pub cmds: Vec<Cmd>,
}

/// Segments used for each cubic curve when flattening.
const CURVE_STEPS: usize = 32;

/// How far the control points of a quarter ellipse sit from its ends (as a fraction of the
/// radius) for the usual four-curve approximation.
const KAPPA: f32 = 0.552_284_8;

impl Path {
    /// Starts an empty path.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a sub-path at `p`.
    pub fn move_to(mut self, p: Point) -> Self {
        self.cmds.push(Cmd::Move(p));
        self
    }

    /// Adds a straight line to `p`.
    // The horse uses only curves; straight lines draw the badge, the barn and the fence.
    pub fn line_to(mut self, p: Point) -> Self {
        self.cmds.push(Cmd::Line(p));
        self
    }

    /// Adds a cubic curve to `p` with control points `c1` and `c2`.
    pub fn cubic_to(mut self, c1: Point, c2: Point, p: Point) -> Self {
        self.cmds.push(Cmd::Cubic(c1, c2, p));
        self
    }

    /// Closes the current sub-path.
    pub fn close(mut self) -> Self {
        self.cmds.push(Cmd::Close);
        self
    }

    /// An ellipse centered on `c` with radii `rx` and `ry`, as four cubic curves.
    pub fn ellipse(c: Point, rx: f32, ry: f32) -> Self {
        let (cx, cy) = c;
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        Self::new()
            .move_to((cx + rx, cy))
            .cubic_to((cx + rx, cy + ky), (cx + kx, cy + ry), (cx, cy + ry))
            .cubic_to((cx - kx, cy + ry), (cx - rx, cy + ky), (cx - rx, cy))
            .cubic_to((cx - rx, cy - ky), (cx - kx, cy - ry), (cx, cy - ry))
            .cubic_to((cx + kx, cy - ry), (cx + rx, cy - ky), (cx + rx, cy))
            .close()
    }

    /// A rectangle.
    pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::new()
            .move_to((x, y))
            .line_to((x + w, y))
            .line_to((x + w, y + h))
            .line_to((x, y + h))
            .close()
    }

    /// A rectangle with round corners of radius `r`.
    pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Self {
        let k = r * (1.0 - KAPPA);
        let (x2, y2) = (x + w, y + h);
        Self::new()
            .move_to((x + r, y))
            .line_to((x2 - r, y))
            .cubic_to((x2 - k, y), (x2, y + k), (x2, y + r))
            .line_to((x2, y2 - r))
            .cubic_to((x2, y2 - k), (x2 - k, y2), (x2 - r, y2))
            .line_to((x + r, y2))
            .cubic_to((x + k, y2), (x, y2 - k), (x, y2 - r))
            .line_to((x, y + r))
            .cubic_to((x, y + k), (x + k, y), (x + r, y))
            .close()
    }

    /// A closed polygon through `points`.
    pub fn polygon(points: &[Point]) -> Self {
        let mut path = Self::new().move_to(points[0]);
        for &p in &points[1..] {
            path = path.line_to(p);
        }
        path.close()
    }

    /// A straight line from `a` to `b`, `width` wide, with round ends.
    ///
    /// Every part winds the same way as [`Path::ellipse`] and [`Path::rect`], so lines that
    /// cross or touch add up under the nonzero rule instead of cutting holes.
    pub fn line(a: Point, b: Point, width: f32) -> Self {
        let r = width / 2.0;
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        let (nx, ny) = (-dy / len * r, dx / len * r);
        let mut path = Self::new()
            .move_to((a.0 - nx, a.1 - ny))
            .line_to((b.0 - nx, b.1 - ny))
            .line_to((b.0 + nx, b.1 + ny))
            .line_to((a.0 + nx, a.1 + ny))
            .close();
        path.cmds.extend(Self::ellipse(a, r, r).cmds);
        path.cmds.extend(Self::ellipse(b, r, r).cmds);
        path
    }

    /// Lines through `points`, one after the other, `width` wide.
    pub fn polyline(points: &[Point], width: f32) -> Self {
        let mut path = Self::new();
        for pair in points.windows(2) {
            path.cmds.extend(Self::line(pair[0], pair[1], width).cmds);
        }
        path
    }

    /// A circle's outline: a ring centered on `c`, `r` to the middle of the line.
    pub fn ring(c: Point, r: f32, width: f32) -> Self {
        let mut path = Self::ellipse(c, r + width / 2.0, r + width / 2.0);
        // A negative radius walks the inner circle the other way round: a hole.
        let inner = r - width / 2.0;
        path.cmds.extend(Self::ellipse(c, inner, -inner).cmds);
        path
    }

    /// This path scaled by `scale` around the origin, then moved by `offset`.
    pub fn transformed(&self, scale: f32, offset: Point) -> Self {
        let t = |p: Point| transform(p, scale, offset);
        let cmds = self
            .cmds
            .iter()
            .map(|cmd| match *cmd {
                Cmd::Move(p) => Cmd::Move(t(p)),
                Cmd::Line(p) => Cmd::Line(t(p)),
                Cmd::Cubic(a, b, p) => Cmd::Cubic(t(a), t(b), t(p)),
                Cmd::Close => Cmd::Close,
            })
            .collect();
        Self { cmds }
    }

    /// The path as closed polygons (one per sub-path).
    pub fn flatten(&self) -> Vec<Vec<Point>> {
        let mut polygons = Vec::new();
        let mut current: Vec<Point> = Vec::new();
        let mut pen = (0.0, 0.0);
        for cmd in &self.cmds {
            match *cmd {
                Cmd::Move(p) => {
                    if current.len() > 2 {
                        polygons.push(std::mem::take(&mut current));
                    }
                    current.clear();
                    current.push(p);
                    pen = p;
                }
                Cmd::Line(p) => {
                    current.push(p);
                    pen = p;
                }
                Cmd::Cubic(c1, c2, p) => {
                    for step in 1..=CURVE_STEPS {
                        let t = step as f32 / CURVE_STEPS as f32;
                        current.push(cubic_point(pen, c1, c2, p, t));
                    }
                    pen = p;
                }
                Cmd::Close => {
                    if current.len() > 2 {
                        polygons.push(std::mem::take(&mut current));
                    }
                    current.clear();
                }
            }
        }
        if current.len() > 2 {
            polygons.push(current);
        }
        polygons
    }

    /// The path as SVG path data (`d` attribute).
    pub fn svg_data(&self) -> String {
        let mut d = String::new();
        for cmd in &self.cmds {
            match *cmd {
                Cmd::Move(p) => write!(d, "M{} {}", num(p.0), num(p.1)),
                Cmd::Line(p) => write!(d, "L{} {}", num(p.0), num(p.1)),
                Cmd::Cubic(a, b, p) => write!(
                    d,
                    "C{} {} {} {} {} {}",
                    num(a.0),
                    num(a.1),
                    num(b.0),
                    num(b.1),
                    num(p.0),
                    num(p.1)
                ),
                Cmd::Close => write!(d, "Z"),
            }
            .expect("writing to a String cannot fail");
        }
        d
    }
}

/// `p` scaled by `scale` around the origin, then moved by `offset`.
pub fn transform(p: Point, scale: f32, offset: Point) -> Point {
    (p.0 * scale + offset.0, p.1 * scale + offset.1)
}

/// The point at `t` on a cubic Bézier curve.
pub fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

/// A number for SVG output: at most two decimals, no trailing zeros.
pub fn num(v: f32) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cubic_flattens_to_its_end_points() {
        let path = Path::new()
            .move_to((0.0, 0.0))
            .cubic_to((10.0, 0.0), (20.0, 10.0), (20.0, 20.0))
            .close();
        let polygons = path.flatten();
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].len(), CURVE_STEPS + 1);
        assert_eq!(polygons[0][0], (0.0, 0.0));
        let last = polygons[0][CURVE_STEPS];
        assert!((last.0 - 20.0).abs() < 1e-4 && (last.1 - 20.0).abs() < 1e-4);
    }

    #[test]
    fn crossing_lines_wind_the_same_way() {
        // Shoelace sums: every polygon of a line has the sign of an ellipse.
        let sign = |poly: &Vec<Point>| {
            (0..poly.len())
                .map(|i| {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    a.0 * b.1 - b.0 * a.1
                })
                .sum::<f32>()
                > 0.0
        };
        let circle = sign(&Path::ellipse((0.0, 0.0), 5.0, 5.0).flatten()[0]);
        for (a, b) in [((0.0, 0.0), (10.0, 0.0)), ((10.0, 10.0), (0.0, -3.0))] {
            for poly in Path::line(a, b, 2.0).flatten() {
                assert_eq!(sign(&poly), circle);
            }
        }
        // A ring's inner circle goes the other way.
        let ring = Path::ring((0.0, 0.0), 10.0, 2.0).flatten();
        assert_eq!(sign(&ring[0]), circle);
        assert_ne!(sign(&ring[1]), circle);
    }

    #[test]
    fn a_transform_scales_then_moves() {
        let path = Path::rect(10.0, 20.0, 30.0, 40.0).transformed(2.0, (5.0, -5.0));
        assert_eq!(
            path.flatten()[0],
            vec![(25.0, 35.0), (85.0, 35.0), (85.0, 115.0), (25.0, 115.0)]
        );
    }

    #[test]
    fn svg_numbers_are_short() {
        assert_eq!(num(12.0), "12");
        assert_eq!(num(12.5), "12.5");
        assert_eq!(num(-0.001), "0");
        assert_eq!(
            Path::new()
                .move_to((1.0, 2.0))
                .line_to((3.5, 4.0))
                .close()
                .svg_data(),
            "M1 2L3.5 4Z"
        );
    }
}
