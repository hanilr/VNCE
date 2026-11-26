//! Paints: solid colors and gradients, evaluated for the PNGs and written for the SVG with
//! the same math (user-space coordinates, padded ends, linear interpolation of stops).

use std::fmt::Write as _;

use crate::geometry::{Point, num, transform};

/// A color with straight (not premultiplied) alpha, every channel in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

impl Color {
    /// An opaque color from `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        Self::hex_alpha(rgb, 1.0)
    }

    /// A color from `0xRRGGBB` and an alpha.
    pub const fn hex_alpha(rgb: u32, a: f32) -> Self {
        Self {
            r: ((rgb >> 16) & 0xff) as f32 / 255.0,
            g: ((rgb >> 8) & 0xff) as f32 / 255.0,
            b: (rgb & 0xff) as f32 / 255.0,
            a,
        }
    }

    /// The color as `#rrggbb` (alpha is written separately).
    pub fn svg_rgb(self) -> String {
        let to = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", to(self.r), to(self.g), to(self.b))
    }

    fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

/// A gradient stop: an offset in `0.0..=1.0` and a color.
pub type Stop = (f32, Color);

/// How a shape is filled.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// One color.
    Solid(Color),
    /// A gradient along the line from `from` to `to`.
    Linear {
        /// Where offset 0 is.
        from: Point,
        /// Where offset 1 is.
        to: Point,
        /// The stops, by increasing offset.
        stops: Vec<Stop>,
    },
    /// A gradient from `center` (offset 0) out to `radius` (offset 1).
    Radial {
        /// The center.
        center: Point,
        /// The radius.
        radius: f32,
        /// The stops, by increasing offset.
        stops: Vec<Stop>,
    },
}

impl Paint {
    /// The color at `p`.
    pub fn at(&self, p: Point) -> Color {
        match self {
            Self::Solid(c) => *c,
            Self::Linear { from, to, stops } => {
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                let len2 = dx * dx + dy * dy;
                let t = ((p.0 - from.0) * dx + (p.1 - from.1) * dy) / len2;
                sample(stops, t)
            }
            Self::Radial {
                center,
                radius,
                stops,
            } => {
                let (dx, dy) = (p.0 - center.0, p.1 - center.1);
                sample(stops, (dx * dx + dy * dy).sqrt() / radius)
            }
        }
    }

    /// This paint scaled by `scale` around the origin, then moved by `offset` (to follow a
    /// shape moved with [`crate::geometry::Path::transformed`]).
    pub fn transformed(&self, scale: f32, offset: Point) -> Self {
        match self {
            Self::Solid(c) => Self::Solid(*c),
            Self::Linear { from, to, stops } => Self::Linear {
                from: transform(*from, scale, offset),
                to: transform(*to, scale, offset),
                stops: stops.clone(),
            },
            Self::Radial {
                center,
                radius,
                stops,
            } => Self::Radial {
                center: transform(*center, scale, offset),
                radius: radius * scale,
                stops: stops.clone(),
            },
        }
    }

    /// The SVG `fill` attributes for this paint; a gradient also adds its definition to
    /// `defs` under the id `id`.
    pub fn svg_fill(&self, id: &str, defs: &mut String) -> String {
        match self {
            Self::Solid(c) => {
                if c.a < 1.0 {
                    format!("fill=\"{}\" fill-opacity=\"{}\"", c.svg_rgb(), num(c.a))
                } else {
                    format!("fill=\"{}\"", c.svg_rgb())
                }
            }
            Self::Linear { from, to, stops } => {
                write!(
                    defs,
                    "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">",
                    num(from.0),
                    num(from.1),
                    num(to.0),
                    num(to.1)
                )
                .expect("writing to a String cannot fail");
                svg_stops(stops, defs);
                defs.push_str("</linearGradient>\n");
                format!("fill=\"url(#{id})\"")
            }
            Self::Radial {
                center,
                radius,
                stops,
            } => {
                write!(
                    defs,
                    "<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">",
                    num(center.0),
                    num(center.1),
                    num(*radius)
                )
                .expect("writing to a String cannot fail");
                svg_stops(stops, defs);
                defs.push_str("</radialGradient>\n");
                format!("fill=\"url(#{id})\"")
            }
        }
    }
}

/// The color at offset `t` of a gradient, padded beyond its ends.
fn sample(stops: &[Stop], t: f32) -> Color {
    let first = stops[0];
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.0 {
            let span = b.0 - a.0;
            let local = if span > 0.0 { (t - a.0) / span } else { 1.0 };
            return a.1.lerp(b.1, local);
        }
    }
    stops[stops.len() - 1].1
}

fn svg_stops(stops: &[Stop], defs: &mut String) {
    for (offset, color) in stops {
        write!(
            defs,
            "<stop offset=\"{}\" stop-color=\"{}\"",
            num(*offset),
            color.svg_rgb()
        )
        .expect("writing to a String cannot fail");
        if color.a < 1.0 {
            write!(defs, " stop-opacity=\"{}\"", num(color.a))
                .expect("writing to a String cannot fail");
        }
        defs.push_str("/>");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Color = Color::hex(0x000000);
    const WHITE: Color = Color::hex(0xffffff);

    #[test]
    fn a_linear_gradient_is_padded_and_interpolated() {
        let paint = Paint::Linear {
            from: (0.0, 0.0),
            to: (100.0, 0.0),
            stops: vec![(0.0, BLACK), (1.0, WHITE)],
        };
        assert_eq!(paint.at((-50.0, 0.0)), BLACK);
        assert_eq!(paint.at((150.0, 30.0)), WHITE);
        assert!((paint.at((25.0, 99.0)).r - 0.25).abs() < 1e-5);
    }

    #[test]
    fn a_radial_gradient_follows_the_distance() {
        let paint = Paint::Radial {
            center: (10.0, 10.0),
            radius: 10.0,
            stops: vec![(0.0, WHITE), (0.5, BLACK)],
        };
        assert_eq!(paint.at((10.0, 10.0)), WHITE);
        assert!((paint.at((12.5, 10.0)).g - 0.5).abs() < 1e-5);
        assert_eq!(paint.at((10.0, 30.0)), BLACK);
    }

    #[test]
    fn a_moved_gradient_gives_the_same_color_at_the_moved_point() {
        let paint = Paint::Radial {
            center: (10.0, 10.0),
            radius: 10.0,
            stops: vec![(0.0, WHITE), (1.0, BLACK)],
        };
        let moved = paint.transformed(2.0, (100.0, 0.0));
        assert_eq!(paint.at((14.0, 13.0)), moved.at((128.0, 26.0)));
    }

    #[test]
    fn hex_colors_round_trip_to_svg() {
        assert_eq!(Color::hex(0x8a4b26).svg_rgb(), "#8a4b26");
    }
}
