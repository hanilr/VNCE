//! Vectors, matrices and the camera.
//!
//! The CPU renderer must work without the `gpu` feature, where `vnce_gpu::Mat4` does not
//! exist, so the game has its own small types with the same conventions: right-handed
//! (x east, y up, −z north), column vectors, clip-space depth in `0..=1`. [`Mat4`] has the
//! same memory layout as `vnce_gpu::Mat4`, so the GPU renderer converts it for free.

use std::ops::Mul;

/// A 3D vector as `[x, y, z]`.
pub type Vec3 = [f32; 3];

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(v: Vec3, s: f32) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}

/// `a + b * s`.
pub fn add_scaled(a: Vec3, b: Vec3, s: f32) -> Vec3 {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn length(v: Vec3) -> f32 {
    dot(v, v).sqrt()
}

/// `v` with length 1 (a zero vector stays zero).
pub fn normalize(v: Vec3) -> Vec3 {
    let l = length(v);
    if l == 0.0 { v } else { scale(v, 1.0 / l) }
}

pub fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    add_scaled(a, sub(b, a), t)
}

/// Distance on the ground (x and z only).
pub fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// `v` rotated by `angle` radians around the unit vector `axis` (counter-clockwise when the
/// axis points at the viewer).
pub fn rotate(v: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    add(
        add(scale(v, cos), scale(cross(axis, v), sin)),
        scale(axis, dot(axis, v) * (1.0 - cos)),
    )
}

/// The direction of `yaw` (0 = north, −z; π/2 = east, +x) and `pitch` (up is positive).
pub fn direction(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    [cp * sy, sp, -cp * cy]
}

/// A 4×4 matrix stored column by column (the layout of `vnce_gpu::Mat4`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn translation(offset: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[3] = [offset[0], offset[1], offset[2], 1.0];
        m
    }

    pub fn scale(s: f32) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[0][0] = s;
        m.cols[1][1] = s;
        m.cols[2][2] = s;
        m
    }

    /// A rotation whose columns are the new x, y and z axes (`right`, `up`, `back`).
    pub fn basis(right: Vec3, up: Vec3, back: Vec3) -> Self {
        Self {
            cols: [
                [right[0], right[1], right[2], 0.0],
                [up[0], up[1], up[2], 0.0],
                [back[0], back[1], back[2], 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// A rotation around the y axis (yaw towards the east is negative here: use
    /// [`Mat4::heading`] for compass headings).
    pub fn rotation_y(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        let mut m = Self::IDENTITY;
        m.cols[0] = [cos, 0.0, -sin, 0.0];
        m.cols[2] = [sin, 0.0, cos, 0.0];
        m
    }

    /// Turns a model that faces north (−z) to face `yaw` (clockwise seen from above).
    pub fn heading(yaw: f32) -> Self {
        Self::rotation_y(-yaw)
    }

    /// Perspective projection (see `vnce_gpu::Mat4::perspective`).
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y / 2.0).tan();
        let range = far / (near - far);
        Self {
            cols: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, range, -1.0],
                [0.0, 0.0, range * near, 0.0],
            ],
        }
    }

    /// A camera at `eye` looking along `forward` with `up` up.
    pub fn look_to(eye: Vec3, forward: Vec3, up: Vec3) -> Self {
        let forward = normalize(forward);
        let side = normalize(cross(forward, up));
        let up = cross(side, forward);
        Self {
            cols: [
                [side[0], up[0], -forward[0], 0.0],
                [side[1], up[1], -forward[1], 0.0],
                [side[2], up[2], -forward[2], 0.0],
                [-dot(side, eye), -dot(up, eye), dot(forward, eye), 1.0],
            ],
        }
    }

    /// `self * [p, 1]` without the perspective division.
    pub fn apply(&self, p: Vec3) -> [f32; 4] {
        let mut out = self.cols[3];
        for (col, value) in self.cols.iter().take(3).zip(p) {
            for (o, c) in out.iter_mut().zip(col) {
                *o += c * value;
            }
        }
        out
    }

    /// `self * [v, 0]`: a direction, unaffected by translation.
    pub fn apply_direction(&self, v: Vec3) -> Vec3 {
        let mut out = [0.0; 3];
        for (col, value) in self.cols.iter().take(3).zip(v) {
            for (o, c) in out.iter_mut().zip(col) {
                *o += c * value;
            }
        }
        out
    }

    /// A point transformed (no perspective division).
    pub fn point(&self, p: Vec3) -> Vec3 {
        let [x, y, z, _] = self.apply(p);
        [x, y, z]
    }
}

impl Mul for Mat4 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        let mut out = [[0.0; 4]; 4];
        for (out_col, rhs_col) in out.iter_mut().zip(&rhs.cols) {
            for (row, value) in out_col.iter_mut().enumerate() {
                *value = (0..4).map(|k| self.cols[k][row] * rhs_col[k]).sum();
            }
        }
        Self { cols: out }
    }
}

/// Vertical field of view of every camera.
pub const FOV_Y: f32 = 1.05;
pub const NEAR: f32 = 0.5;
pub const FAR: f32 = 2400.0;

/// A camera: position, orientation and the image's aspect ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub eye: Vec3,
    /// Unit vector the camera looks along.
    pub forward: Vec3,
    /// Unit vector up in the picture (perpendicular to `forward`).
    pub up: Vec3,
    /// Width / height of the picture, in real (square) units.
    pub aspect: f32,
}

impl Camera {
    /// A camera at `eye` looking along `forward`, rolled so that `up` is up.
    pub fn new(eye: Vec3, forward: Vec3, up: Vec3, aspect: f32) -> Self {
        let forward = normalize(forward);
        let side = normalize(cross(forward, up));
        Self {
            eye,
            forward,
            up: cross(side, forward),
            aspect,
        }
    }

    pub fn right(&self) -> Vec3 {
        cross(self.forward, self.up)
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_to(self.eye, self.forward, self.up)
    }

    pub fn projection(&self) -> Mat4 {
        Mat4::perspective(FOV_Y, self.aspect, NEAR, FAR)
    }

    pub fn view_projection(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Where `point` appears in a `width` × `height` picture (pixels from the top left)
    /// and its distance along the view, or `None` behind the camera.
    pub fn project(&self, point: Vec3, width: f32, height: f32) -> Option<(f32, f32, f32)> {
        let offset = sub(point, self.eye);
        let depth = dot(offset, self.forward);
        if depth < NEAR {
            return None;
        }
        let tan = (FOV_Y / 2.0).tan();
        let x = dot(offset, self.right()) / (depth * tan * self.aspect);
        let y = dot(offset, self.up) / (depth * tan);
        Some((
            f32::midpoint(x, 1.0) * width,
            f32::midpoint(1.0, -y) * height,
            depth,
        ))
    }

    /// The direction through the picture point `(sx, sy)` in −1..1 (y up), not normalized.
    pub fn ray(&self, sx: f32, sy: f32) -> Vec3 {
        let tan = (FOV_Y / 2.0).tan();
        add(
            add(self.forward, scale(self.right(), sx * tan * self.aspect)),
            scale(self.up, sy * tan),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn directions_follow_the_compass() {
        assert!(near(direction(0.0, 0.0), [0.0, 0.0, -1.0]));
        assert!(near(
            direction(std::f32::consts::FRAC_PI_2, 0.0),
            [1.0, 0.0, 0.0]
        ));
        // A model facing north turned to the east faces east.
        let east = Mat4::heading(std::f32::consts::FRAC_PI_2).apply_direction([0.0, 0.0, -1.0]);
        assert!(near(east, [1.0, 0.0, 0.0]), "{east:?}");
    }

    #[test]
    fn camera_projection_matches_the_matrices() {
        let camera = Camera::new([10.0, 5.0, 3.0], [0.3, -0.2, -1.0], [0.0, 1.0, 0.0], 1.6);
        let point = [14.0, 2.0, -40.0];
        let (x, y, _) = camera.project(point, 200.0, 100.0).unwrap();
        let clip = camera.view_projection().apply(point);
        let (nx, ny) = (clip[0] / clip[3], clip[1] / clip[3]);
        assert!((x - (nx + 1.0) * 100.0).abs() < 1e-2);
        assert!((y - (1.0 - ny) * 50.0).abs() < 1e-2);
        // Behind the camera: nothing.
        assert!(camera.project([10.0, 5.0, 50.0], 200.0, 100.0).is_none());
        // The ray through the projected point points at it.
        let ray = normalize(camera.ray(x / 100.0 - 1.0, 1.0 - y / 50.0));
        assert!(near(ray, normalize(sub(point, camera.eye))));
    }

    #[test]
    fn rotation_around_an_axis() {
        let v = rotate(
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f32::consts::FRAC_PI_2,
        );
        assert!(near(v, [0.0, 1.0, 0.0]));
    }
}
