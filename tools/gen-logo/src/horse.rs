//! The horse's face, seen from the front: the horse of the Variation ecosystem, as drawn
//! by hand for the variation-ui banner (github.com/hanilr/variation-ui). White shards
//! around the edge of the head, a red left eye, a blue right eye, a green muzzle and a
//! network of white lines down the nose, on a dark background.
//!
//! The shards were traced once from that banner (1280 x 640): each white or colored patch
//! became a polygon, with its pixel steps smoothed out. The nose lines were measured by hand
//! and are drawn as strokes. Coordinates are the banner's, so the banner uses them as they
//! are and the logo scales them into its 512 x 512 space.

use crate::geometry::{Path, Point};
use crate::paint::{Color, Paint};

/// One shape and how it is filled.
#[derive(Clone, Debug)]
pub struct Layer {
    /// A name, used for SVG ids.
    pub name: String,
    /// The shape.
    pub path: Path,
    /// The fill.
    pub paint: Paint,
    /// Whether the shape is clipped to the badge (the background scene is): the scene's clip
    /// shape, when it has one.
    pub in_badge: bool,
}

impl Layer {
    /// A layer drawn as it is.
    pub fn new(name: impl Into<String>, path: Path, paint: Paint) -> Self {
        Self {
            name: name.into(),
            path,
            paint,
            in_badge: false,
        }
    }

    /// This layer scaled by `scale` around the origin, then moved by `offset`.
    pub fn transformed(&self, scale: f32, offset: Point) -> Self {
        Self {
            name: self.name.clone(),
            path: self.path.transformed(scale, offset),
            paint: self.paint.transformed(scale, offset),
            in_badge: self.in_badge,
        }
    }

    /// A layer clipped to the badge.
    pub fn in_badge(name: impl Into<String>, path: Path, paint: Paint) -> Self {
        Self {
            name: name.into(),
            path,
            paint,
            in_badge: true,
        }
    }
}

/// The background of the dark style.
pub const BACKGROUND: Color = Color::hex(0x161616);
/// The white of the shards, the lines and the letters.
pub const WHITE: Color = Color::hex(0xe9e9e9);
/// The left eye, and the first of the three accent colors.
pub const RED: Color = Color::hex(0xc83737);
/// The muzzle, and the second accent color.
pub const GREEN: Color = Color::hex(0x37c837);
/// The right eye, and the third accent color.
pub const BLUE: Color = Color::hex(0x3737c8);

/// The color of a shard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// The white edge of the head.
    White,
    /// The left eye.
    Red,
    /// The muzzle.
    Green,
    /// The right eye.
    Blue,
}

impl Ink {
    /// The layer name and color of this ink.
    fn layer(self) -> (&'static str, Color) {
        match self {
            Self::White => ("shards", WHITE),
            Self::Red => ("eye-left", RED),
            Self::Green => ("muzzle", GREEN),
            Self::Blue => ("eye-right", BLUE),
        }
    }
}

/// The width of the nose lines.
const LINE_WIDTH: f32 = 3.5;

/// The nose lines, as (from, to) pairs: two crossing triangles at the top, an hourglass,
/// the long center line, and the fan of lines around the nostrils at the bottom.
const LINES: [(Point, Point); 19] = [
    // The top: two long lines from the corners, and a triangle standing on its base.
    ((186.0, 154.0), (230.0, 237.0)),
    ((273.0, 153.0), (230.0, 237.0)),
    ((230.0, 155.0), (188.0, 238.0)),
    ((230.0, 155.0), (272.0, 237.0)),
    ((188.0, 238.0), (272.0, 237.0)),
    // The hourglass below it.
    ((188.0, 238.0), (272.0, 321.0)),
    ((272.0, 237.0), (188.0, 321.0)),
    ((188.0, 321.0), (272.0, 321.0)),
    // The center line, down to the bottom.
    ((230.0, 237.0), (230.0, 495.0)),
    // The two long sides down to the nostrils, and the straight sides below them.
    ((230.0, 321.0), (187.0, 408.0)),
    ((230.0, 321.0), (273.0, 408.0)),
    ((187.0, 408.0), (187.0, 495.0)),
    ((273.0, 408.0), (273.0, 495.0)),
    // The fan from the point between the nostrils.
    ((230.0, 408.0), (187.0, 452.0)),
    ((230.0, 408.0), (273.0, 452.0)),
    ((187.0, 452.0), (230.0, 495.0)),
    ((273.0, 452.0), (230.0, 495.0)),
    ((230.0, 408.0), (188.0, 495.0)),
    ((230.0, 408.0), (272.0, 495.0)),
];

/// The horse's layers, scaled by `scale` and moved by `offset` from the banner's
/// coordinates, back to front.
///
/// The first layer fills the outline of the head with the background color, so the farm
/// behind does not show through the face.
pub fn layers(scale: f32, offset: Point) -> Vec<Layer> {
    let mut layers = vec![Layer::new("head", outline(), Paint::Solid(BACKGROUND))];
    for ink in [Ink::White, Ink::Red, Ink::Green, Ink::Blue] {
        let mut path = Path::new();
        for (_, polygons) in SHARDS.iter().filter(|(i, _)| *i == ink) {
            for polygon in *polygons {
                path.cmds.extend(Path::polygon(polygon).cmds);
            }
        }
        let (name, color) = ink.layer();
        layers.push(Layer::new(name, path, Paint::Solid(color)));
    }
    let mut lines = Path::new();
    for (a, b) in LINES {
        lines.cmds.extend(Path::line(a, b, LINE_WIDTH).cmds);
    }
    layers.push(Layer::new("nose", lines, Paint::Solid(WHITE)));
    layers
        .iter()
        .map(|l| l.transformed(scale, offset))
        .collect()
}

/// The outline of the head: the convex hull of every shard.
fn outline() -> Path {
    let mut points: Vec<Point> = SHARDS
        .iter()
        .flat_map(|(_, polygons)| polygons.iter().flat_map(|p| p.iter().copied()))
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let cross =
        |o: Point, a: Point, b: Point| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<Point> = Vec::new();
    for pass in [false, true] {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &Point>> = if pass {
            Box::new(points.iter().rev())
        } else {
            Box::new(points.iter())
        };
        for &p in ordered {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    Path::polygon(&hull)
}

/// The shards, by ink: each is one or more closed polygons (an outer edge, and holes
/// walked the other way round).
#[rustfmt::skip]
pub const SHARDS: &[(Ink, &[&[Point]])] = &[
    (
        Ink::White,
        &[
            &[(367.0, 56.0), (370.0, 56.0), (371.0, 94.0), (351.0, 133.0), (344.0, 129.0), (344.0, 122.0), (353.0, 82.0), (349.0, 82.0), (347.0, 86.0), (311.0, 107.0), (305.0, 103.0), (331.0, 75.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(86.0, 58.0), (127.0, 79.0), (153.0, 104.0), (144.0, 109.0), (102.0, 86.0), (101.0, 90.0), (104.0, 92.0), (114.0, 127.0), (114.0, 129.0), (105.0, 134.0), (85.0, 96.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(315.0, 85.0), (318.0, 86.0), (303.0, 102.0), (290.0, 94.0), (284.0, 94.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(143.0, 89.0), (171.0, 95.0), (166.0, 95.0), (160.0, 100.0), (155.0, 101.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(276.0, 97.0), (289.0, 97.0), (312.0, 112.0), (291.0, 113.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(166.0, 98.0), (183.0, 98.0), (168.0, 114.0), (141.0, 115.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(327.0, 100.0), (325.0, 116.0), (313.0, 109.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(127.0, 103.0), (140.0, 109.0), (141.0, 112.0), (130.0, 117.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(342.0, 111.0), (344.0, 113.0), (341.0, 126.0), (329.0, 119.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(113.0, 114.0), (126.0, 121.0), (116.0, 126.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(315.0, 115.0), (320.0, 116.0), (324.0, 121.0), (328.0, 121.0), (330.0, 129.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(135.0, 118.0), (142.0, 119.0), (128.0, 133.0), (127.0, 123.0), (131.0, 123.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(360.0, 121.0), (368.0, 153.0), (354.0, 136.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(333.0, 125.0), (349.0, 134.0), (363.0, 153.0), (334.0, 153.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(97.0, 126.0), (103.0, 137.0), (87.0, 157.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(122.0, 126.0), (124.0, 126.0), (124.0, 154.0), (94.0, 155.0), (102.0, 141.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(357.0, 156.0), (366.0, 156.0), (373.0, 165.0), (373.0, 169.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(91.0, 157.0), (101.0, 157.0), (101.0, 159.0), (84.0, 175.0), (84.0, 166.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(376.0, 171.0), (393.0, 193.0), (377.0, 193.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(80.0, 172.0), (81.0, 196.0), (63.0, 197.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(375.0, 195.0), (391.0, 197.0), (369.0, 207.0)],
        ],
    ),
    (
        Ink::Blue,
        &[
            &[(394.0, 197.0), (396.0, 197.0), (396.0, 252.0), (356.0, 235.0), (356.0, 216.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(80.0, 198.0), (86.0, 209.0), (68.0, 201.0), (68.0, 199.0)],
        ],
    ),
    (
        Ink::Red,
        &[
            &[(60.0, 201.0), (66.0, 201.0), (102.0, 218.0), (102.0, 238.0), (61.0, 257.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(371.0, 244.0), (394.0, 255.0), (378.0, 256.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(84.0, 248.0), (86.0, 250.0), (81.0, 259.0), (65.0, 258.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(379.0, 258.0), (395.0, 258.0), (381.0, 307.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(61.0, 261.0), (80.0, 261.0), (79.0, 311.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(88.0, 344.0), (101.0, 371.0), (101.0, 405.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(370.0, 351.0), (370.0, 360.0), (360.0, 400.0), (359.0, 385.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(104.0, 382.0), (129.0, 451.0), (103.0, 450.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(356.0, 394.0), (359.0, 449.0), (338.0, 450.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(337.0, 452.0), (348.0, 452.0), (348.0, 454.0), (332.0, 468.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(116.0, 453.0), (130.0, 454.0), (135.0, 469.0), (117.0, 456.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(278.0, 519.0), (310.0, 519.0), (298.0, 556.0), (296.0, 556.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(165.0, 520.0), (185.0, 520.0), (167.0, 557.0), (155.0, 523.0), (155.0, 521.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(281.0, 534.0), (290.0, 548.0), (295.0, 561.0), (258.0, 561.0), (261.0, 555.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(181.0, 535.0), (207.0, 560.0), (169.0, 560.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(175.0, 563.0), (187.0, 563.0), (194.0, 571.0)],
        ],
    ),
    (
        Ink::Green,
        &[
            &[(191.0, 563.0), (271.0, 563.0), (271.0, 565.0), (254.0, 584.0), (209.0, 584.0)],
        ],
    ),
    (
        Ink::White,
        &[
            &[(275.0, 563.0), (288.0, 564.0), (268.0, 572.0)],
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ink_has_a_shard() {
        for ink in [Ink::White, Ink::Red, Ink::Green, Ink::Blue] {
            assert!(SHARDS.iter().any(|(i, _)| *i == ink), "{ink:?}");
        }
    }

    #[test]
    fn the_horse_stays_where_the_banner_has_it() {
        let points: Vec<Point> = layers(1.0, (0.0, 0.0))
            .iter()
            .flat_map(|l| l.path.flatten())
            .flatten()
            .collect();
        let min_x = points.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let min_y = points.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        assert!((55.0..65.0).contains(&min_x) && (390.0..400.0).contains(&max_x));
        assert!((50.0..60.0).contains(&min_y) && (580.0..590.0).contains(&max_y));
    }

    #[test]
    fn the_outline_covers_the_eyes() {
        let hull = outline().flatten();
        let inside = |p: Point| {
            // Inside a convex polygon: on the same side of every edge.
            let poly = &hull[0];
            let sides: Vec<bool> = (0..poly.len())
                .map(|i| {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) >= 0.0
                })
                .collect();
            sides.iter().all(|&s| s) || sides.iter().all(|&s| !s)
        };
        assert!(inside((80.0, 228.0)) && inside((376.0, 224.0)) && inside((230.0, 573.0)));
    }
}
