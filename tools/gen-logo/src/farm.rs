//! The badge and the farm behind the horse, in the dark style: a dark square with round
//! corners, and the farm as dim grey outlines (hills, a barn, a fence and the moon; the
//! banner adds a silo)
//! so the white horse stays in front. Every scene layer is clipped to the badge.

use crate::geometry::{Path, Point, SIZE, cubic_point};
use crate::horse::{BACKGROUND, Layer};
use crate::paint::{Color, Paint};

/// The badge's distance from the edge of the image.
const INSET: f32 = 8.0;
/// The radius of the badge's corners.
const CORNER: f32 = 96.0;

/// The farm's lines: a dim grey, well behind the horse's white.
pub const FARM: Color = Color::hex(0x4a4a4a);

/// The badge: a square with round corners.
pub fn badge() -> Path {
    let side = SIZE - 2.0 * INSET;
    Path::rounded_rect(INSET, INSET, side, side, CORNER)
}

/// The points along a cubic curve, for stroking it.
pub fn curve(p0: Point, c1: Point, c2: Point, p3: Point) -> Vec<Point> {
    (0..=32)
        .map(|i| cubic_point(p0, c1, c2, p3, i as f32 / 32.0))
        .collect()
}

/// A barn's outline: walls under a gable roof, and a door with a cross brace. `left` and
/// `right` are the walls, `ground` the bottom, `eaves` where the roof starts and `peak`
/// its top.
pub fn barn(left: f32, right: f32, ground: f32, eaves: f32, peak: f32, width: f32) -> Path {
    let mid = (left + right) / 2.0;
    let mut path = Path::polyline(
        &[
            (left, ground),
            (left, eaves),
            (mid, peak),
            (right, eaves),
            (right, ground),
            (left, ground),
        ],
        width,
    );
    let half = (right - left) * 0.18;
    let top = ground - (ground - eaves) * 0.7;
    let door = [
        (mid - half, ground),
        (mid - half, top),
        (mid + half, top),
        (mid + half, ground),
    ];
    path.cmds.extend(Path::polyline(&door, width).cmds);
    path.cmds
        .extend(Path::line((mid - half, top), (mid + half, ground), width).cmds);
    path.cmds
        .extend(Path::line((mid + half, top), (mid - half, ground), width).cmds);
    path
}

/// A silo's outline: a tall box under a round top.
pub fn silo(left: f32, right: f32, ground: f32, top: f32, width: f32) -> Path {
    let r = (right - left) / 2.0;
    let mid = left + r;
    let mut points = vec![(left, ground), (left, top)];
    points.extend((0..=16).map(|i| {
        let a = std::f32::consts::PI * (1.0 + i as f32 / 16.0);
        (mid + r * a.cos(), top + r * a.sin())
    }));
    points.push((right, ground));
    Path::polyline(&points, width)
}

/// A fence: two rails from `from` to `to`, and posts at `posts`.
pub fn fence(from: f32, to: f32, rails: [f32; 2], posts: &[f32], width: f32) -> Path {
    let mut path = Path::new();
    for y in rails {
        path.cmds.extend(Path::line((from, y), (to, y), width).cmds);
    }
    let (top, bottom) = (rails[0] - 14.0, rails[1] + 16.0);
    for &x in posts {
        path.cmds
            .extend(Path::line((x, top), (x, bottom), width).cmds);
    }
    path
}

/// The logo's scene layers, back to front.
pub fn layers() -> Vec<Layer> {
    let line = 3.0;
    let mut farm = Path::new();
    // Hills behind everything.
    farm.cmds.extend(
        Path::polyline(
            &curve((0.0, 352.0), (150.0, 318.0), (360.0, 382.0), (512.0, 340.0)),
            line,
        )
        .cmds,
    );
    // The barn on the right, the fence in front. (The silo stays in the banner: on the
    // logo's left it fell into the biggest pixels and turned into grey slabs.)
    farm.cmds
        .extend(barn(410.0, 492.0, 396.0, 342.0, 312.0, line).cmds);
    farm.cmds.extend(
        fence(
            0.0,
            512.0,
            [412.0, 440.0],
            &[40.0, 92.0, 424.0, 476.0],
            line,
        )
        .cmds,
    );
    // The moon, top right.
    farm.cmds.extend(Path::ring((440.0, 92.0), 24.0, line).cmds);

    vec![
        Layer::in_badge(
            "badge",
            Path::rect(0.0, 0.0, SIZE, SIZE),
            Paint::Solid(BACKGROUND),
        ),
        Layer::in_badge("farm", farm, Paint::Solid(FARM)),
    ]
}
