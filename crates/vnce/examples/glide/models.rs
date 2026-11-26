//! Flat-shaded low-poly models built from code: the craft, rings, trees, rocks and the
//! shadow under the craft. Both renderers draw the same triangles: the GPU uploads them
//! once as meshes, the CPU sorts and fills them every frame.

use std::f32::consts::TAU;

use vnce::vnce_raster::Rgba;

use crate::math::{Mat4, Vec3, add, cross, dot, normalize, scale, sub};

/// One triangle with its face normal (zero: never lit) and color. Counter-clockwise when
/// seen from the side the normal points to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tri {
    pub p: [Vec3; 3],
    pub n: Vec3,
    pub color: Rgba,
}

impl Tri {
    /// A lit triangle; its normal follows the winding.
    pub fn new(a: Vec3, b: Vec3, c: Vec3, color: Rgba) -> Self {
        Self {
            p: [a, b, c],
            n: normalize(cross(sub(b, a), sub(c, a))),
            color,
        }
    }

    /// A lit triangle facing away from `inside` (winding fixed if needed).
    pub fn outward(a: Vec3, b: Vec3, c: Vec3, inside: Vec3, color: Rgba) -> Self {
        let t = Self::new(a, b, c, color);
        let center = scale(add(add(a, b), c), 1.0 / 3.0);
        if dot(t.n, sub(center, inside)) < 0.0 {
            Self::new(a, c, b, color)
        } else {
            t
        }
    }

    /// An unlit triangle, drawn in exactly its color.
    pub fn unlit(a: Vec3, b: Vec3, c: Vec3, color: Rgba) -> Self {
        Self {
            p: [a, b, c],
            n: [0.0; 3],
            color,
        }
    }

    /// Whether the triangle has a normal (and so is lit).
    pub fn is_lit(&self) -> bool {
        self.n.iter().any(|c| c.abs() > 0.0)
    }

    /// The triangle moved by `model` (rotation, uniform scale and translation).
    pub fn transformed(&self, model: &Mat4) -> Self {
        let n = if self.is_lit() {
            normalize(model.transform_direction(self.n))
        } else {
            self.n
        };
        Self {
            p: self.p.map(|p| model.point(p)),
            n,
            color: self.color,
        }
    }
}

/// Adds both triangles of the quad `a b c d` facing away from `inside`.
fn quad(out: &mut Vec<Tri>, [a, b, c, d]: [Vec3; 4], inside: Vec3, color: Rgba) {
    out.push(Tri::outward(a, b, c, inside, color));
    out.push(Tri::outward(a, c, d, inside, color));
}

/// A thin plate around the flat quad `corners` (in order around it), `half` thick on
/// each side of it along `normal`: top, bottom and the four edges.
fn plate(out: &mut Vec<Tri>, corners: [Vec3; 4], normal: Vec3, half: f32, color: Rgba) {
    let top = corners.map(|p| add(p, scale(normal, half)));
    let bottom = corners.map(|p| sub(p, scale(normal, half)));
    let inside = scale(corners.iter().fold([0.0; 3], |s, &p| add(s, p)), 0.25);
    quad(out, top, inside, color);
    quad(out, bottom, inside, color);
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad(out, [top[i], top[j], bottom[j], bottom[i]], inside, color);
    }
}

/// A cone or pyramid with `sides` faces: base circle at `base` height with `radius`, tip
/// at `tip` height, turned by `turn` radians.
fn cone(
    out: &mut Vec<Tri>,
    sides: usize,
    radius: f32,
    base: f32,
    tip: f32,
    turn: f32,
    colors: [Rgba; 2],
) {
    let apex = [0.0, tip, 0.0];
    let inside = [0.0, base + (tip - base) * 0.3, 0.0];
    let corner = |i: usize| {
        let a = turn + i as f32 * TAU / sides as f32;
        [radius * a.cos(), base, radius * a.sin()]
    };
    for i in 0..sides {
        let (a, b) = (corner(i), corner(i + 1));
        out.push(Tri::outward(a, b, apex, inside, colors[i % 2]));
        out.push(Tri::outward(a, b, [0.0, base, 0.0], inside, colors[1]));
    }
}

/// A square post from `bottom` to `top`, `half` wide on each side.
fn post(out: &mut Vec<Tri>, half: f32, bottom: f32, top: f32, color: Rgba) {
    let c = |x: f32, y: f32, z: f32| [x * half, y, z * half];
    let inside = [0.0, f32::midpoint(bottom, top), 0.0];
    for (x0, z0, x1, z1) in [
        (-1.0, -1.0, 1.0, -1.0),
        (1.0, -1.0, 1.0, 1.0),
        (1.0, 1.0, -1.0, 1.0),
        (-1.0, 1.0, -1.0, -1.0),
    ] {
        quad(
            out,
            [
                c(x0, bottom, z0),
                c(x1, bottom, z1),
                c(x1, top, z1),
                c(x0, top, z0),
            ],
            inside,
            color,
        );
    }
}

/// The craft, about 6.5 m long with 9 m wings, facing north (−z) with +y up.
pub fn craft() -> Vec<Tri> {
    let body = Rgba::rgb(236, 238, 244);
    let belly = Rgba::rgb(170, 178, 192);
    let canopy = Rgba::rgb(40, 62, 96);
    let wing = Rgba::rgb(214, 220, 230);
    let accent = Rgba::rgb(242, 128, 46);
    let mut out = Vec::new();
    let nose = [0.0, 0.05, -3.3];
    let tail = [0.0, 0.35, 3.1];
    // Two cross sections: top, right, bottom, left.
    let mid = [
        [0.0, 0.62, -0.8],
        [0.55, 0.05, -0.8],
        [0.0, -0.48, -0.8],
        [-0.55, 0.05, -0.8],
    ];
    let rear = [
        [0.0, 0.38, 1.5],
        [0.3, 0.1, 1.5],
        [0.0, -0.18, 1.5],
        [-0.3, 0.1, 1.5],
    ];
    let inside = [0.0, 0.05, -0.2];
    for i in 0..4 {
        let j = (i + 1) % 4;
        // The upper front faces are the canopy.
        let front = if i == 0 || j == 0 { canopy } else { belly };
        out.push(Tri::outward(nose, mid[i], mid[j], inside, front));
        let side = if i < 1 || j == 0 { body } else { belly };
        quad(&mut out, [mid[i], mid[j], rear[j], rear[i]], inside, side);
        out.push(Tri::outward(rear[i], rear[j], tail, [0.0, 0.1, 1.8], body));
    }
    // Wings, slightly swept back and up, with orange tips.
    for side in [-1.0_f32, 1.0] {
        let x = |v: f32| v * side;
        plate(
            &mut out,
            [
                [x(0.4), 0.0, -1.0],
                [x(3.4), 0.25, -0.3],
                [x(3.4), 0.25, 0.5],
                [x(0.4), 0.0, 0.5],
            ],
            [0.0, 1.0, 0.0],
            0.07,
            wing,
        );
        plate(
            &mut out,
            [
                [x(3.4), 0.25, -0.3],
                [x(4.6), 0.4, 0.0],
                [x(4.6), 0.4, 0.6],
                [x(3.4), 0.25, 0.5],
            ],
            [0.0, 1.0, 0.0],
            0.06,
            accent,
        );
        // Tail plane.
        plate(
            &mut out,
            [
                [x(0.15), 0.3, 2.3],
                [x(1.5), 0.38, 2.75],
                [x(1.5), 0.38, 3.1],
                [x(0.15), 0.3, 3.05],
            ],
            [0.0, 1.0, 0.0],
            0.04,
            wing,
        );
    }
    // Fin.
    plate(
        &mut out,
        [
            [0.0, 0.3, 1.7],
            [0.0, 1.55, 2.75],
            [0.0, 1.55, 3.15],
            [0.0, 0.3, 3.1],
        ],
        [1.0, 0.0, 0.0],
        0.05,
        accent,
    );
    out
}

/// What a ring is for on a course.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RingKind {
    Normal,
    Checkpoint,
    Finish,
}

impl RingKind {
    pub const ALL: [Self; 3] = [Self::Normal, Self::Checkpoint, Self::Finish];

    pub fn color(self) -> Rgba {
        match self {
            Self::Normal => Rgba::rgb(255, 196, 64),
            Self::Checkpoint => Rgba::rgb(80, 220, 255),
            Self::Finish => Rgba::rgb(255, 110, 190),
        }
    }
}

/// A ring of radius 1 (tube radius `tube`) in the xy plane, facing ±z. `glow` draws it
/// unlit and brighter (the next ring to fly through).
pub fn ring(kind: RingKind, glow: bool) -> Vec<Tri> {
    const AROUND: usize = 20;
    const TUBE: usize = 5;
    let tube = 0.085;
    let base = kind.color();
    let bright = base.lerp(Rgba::rgb(255, 255, 255), 0.35);
    let mut out = Vec::new();
    let point = |i: usize, k: usize| {
        let a = i as f32 * TAU / AROUND as f32;
        let b = k as f32 * TAU / TUBE as f32;
        let r = 1.0 + tube * b.cos();
        [r * a.cos(), r * a.sin(), tube * b.sin()]
    };
    for i in 0..AROUND {
        let center = {
            let a = (i as f32 + 0.5) * TAU / AROUND as f32;
            [a.cos(), a.sin(), 0.0]
        };
        let color = if i % 2 == 0 { base } else { bright };
        for k in 0..TUBE {
            let corners = [
                point(i, k),
                point(i + 1, k),
                point(i + 1, k + 1),
                point(i, k + 1),
            ];
            if glow {
                out.push(Tri::unlit(corners[0], corners[1], corners[2], color));
                out.push(Tri::unlit(corners[0], corners[2], corners[3], color));
            } else {
                quad(&mut out, corners, center, color);
            }
        }
    }
    out
}

/// Trees and rocks on the terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropKind {
    Pine,
    Broadleaf,
    Rock,
}

impl PropKind {
    /// Radius and height of the prop at scale 1, for collisions.
    pub fn size(self) -> (f32, f32) {
        match self {
            Self::Pine => (2.2, 12.5),
            Self::Broadleaf => (3.0, 9.0),
            Self::Rock => (3.2, 4.0),
        }
    }
}

/// A prop model at scale 1, standing on y = 0; `variant` changes the shape a little.
pub fn prop(kind: PropKind, variant: u32) -> Vec<Tri> {
    let trunk = Rgba::rgb(104, 76, 52);
    let mut out = Vec::new();
    let turn = variant as f32 * 0.7;
    match kind {
        PropKind::Pine => {
            post(&mut out, 0.25, -1.0, 2.4, trunk);
            let greens = [Rgba::rgb(40, 92, 56), Rgba::rgb(30, 74, 46)];
            cone(&mut out, 6, 2.4, 1.8, 8.4, turn, greens);
            cone(&mut out, 6, 1.6, 5.6, 12.5, turn + 0.5, greens);
        }
        PropKind::Broadleaf => {
            post(&mut out, 0.3, -1.0, 3.6, trunk);
            let greens = [Rgba::rgb(84, 140, 58), Rgba::rgb(64, 118, 48)];
            // A faceted crown: two pyramids base to base.
            cone(&mut out, 7, 3.0, 5.2, 9.0, turn, greens);
            cone(&mut out, 7, 3.0, 5.2, 2.6, turn, greens);
        }
        PropKind::Rock => {
            // A squashed octahedron with jittered corners.
            let jitter = |i: u32| {
                0.75 + ((variant.wrapping_mul(2_654_435_761) >> (i * 3)) & 7) as f32 / 16.0
            };
            let corners = [
                [2.6 * jitter(0), 0.4, 0.0],
                [-2.4 * jitter(1), 0.3, 0.2],
                [0.2, 0.3, 2.5 * jitter(2)],
                [-0.1, 0.5, -2.7 * jitter(3)],
            ];
            let top = [0.3, 3.4 * jitter(4), -0.2];
            let bottom = [0.0, -1.2, 0.0];
            let inside = [0.0, 1.0, 0.0];
            let order = [0, 2, 1, 3];
            let colors = [Rgba::rgb(132, 126, 118), Rgba::rgb(112, 106, 100)];
            for i in 0..4 {
                let (a, b) = (corners[order[i]], corners[order[(i + 1) % 4]]);
                out.push(Tri::outward(a, b, top, inside, colors[i % 2]));
                out.push(Tri::outward(a, b, bottom, inside, colors[1]));
            }
        }
    }
    out
}

/// A soft dark disc of radius 1 on the xz plane, unlit and partly transparent.
pub fn shadow() -> Vec<Tri> {
    const SIDES: usize = 14;
    let color = Rgba::new(0, 0, 0, 96);
    let point = |i: usize| {
        let a = i as f32 * TAU / SIDES as f32;
        [a.cos(), 0.0, a.sin()]
    };
    (0..SIDES)
        .map(|i| Tri::unlit([0.0; 3], point(i + 1), point(i), color))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nearly every face of a tree or rock points away from its middle, and every face of
    /// a ring away from the middle of its tube.
    #[test]
    fn faces_point_outwards() {
        for tris in [prop(PropKind::Pine, 3), prop(PropKind::Rock, 5)] {
            let points: Vec<Vec3> = tris.iter().flat_map(|t| t.p).collect();
            let center = scale(
                points.iter().fold([0.0; 3], |s, &p| add(s, p)),
                1.0 / points.len() as f32,
            );
            let outward = tris
                .iter()
                .filter(|t| dot(t.n, sub(t.p[0], center)) >= -1e-3)
                .count();
            // Trees are made of several parts, so a few faces may face their own part's
            // middle instead; nearly all face out.
            assert!(
                outward * 10 >= tris.len() * 8,
                "{outward} of {}",
                tris.len()
            );
        }
        for tri in ring(RingKind::Finish, false) {
            let p = tri.p[0];
            let around = normalize([p[0], p[1], 0.0]);
            assert!(dot(tri.n, sub(p, around)) > -1e-3);
        }
    }

    #[test]
    fn transformed_triangles_keep_unit_normals() {
        let model = Mat4::translation([5.0, 1.0, 0.0]) * Mat4::heading(1.0) * Mat4::scale([2.0; 3]);
        for tri in craft() {
            let moved = tri.transformed(&model);
            let length = dot(moved.n, moved.n).sqrt();
            assert!((length - 1.0).abs() < 1e-3);
        }
        let shadow = shadow()[0].transformed(&model);
        assert!(!shadow.is_lit(), "unlit stays unlit");
    }
}
