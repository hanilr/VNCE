//! A small 4×4 matrix type, 2D / 3D vector helpers and a perspective camera, for
//! transforms and cameras on the CPU and the GPU.
//!
//! Conventions match `wgpu`: right-handed coordinates (x right, y up, the camera looks
//! along −z), column vectors (`projection * view * model`), clip-space depth in `0..=1`.
//!
//! Vectors are plain arrays ([`Vec2`], [`Vec3`]), so they can be written as literals and
//! stored in vertex data. The vector functions ([`add`], [`sub`], [`scale`], [`dot`],
//! [`length`], [`normalize`], [`lerp`], ...) work on arrays of any length; [`cross`] and
//! [`rotate`] are for 3D only.
//!
//! ```
//! use vnce_core::math::{Camera, Mat4, add, normalize};
//!
//! let model = Mat4::translation([0.0, 0.0, -5.0]) * Mat4::rotation_y(0.5);
//! let at = model.transform_point([0.0, 0.0, 0.0]);
//! assert_eq!(at, [0.0, 0.0, -5.0]);
//!
//! let camera = Camera::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0], 16.0 / 9.0);
//! // The point straight ahead lands in the middle of a 160 × 90 picture.
//! let (x, y, depth) = camera.project(at, 160.0, 90.0).unwrap();
//! assert!((x - 80.0).abs() < 1e-3 && (y - 45.0).abs() < 1e-3 && depth == 5.0);
//! assert_eq!(normalize(add([3.0, 0.0], [0.0, 4.0])), [0.6, 0.8]);
//! ```

use std::ops::Mul;

/// A 2D vector as `[x, y]`.
pub type Vec2 = [f32; 2];

/// A 3D vector as `[x, y, z]`.
pub type Vec3 = [f32; 3];

/// `a + b`.
#[must_use]
pub fn add<const N: usize>(a: [f32; N], b: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| a[i] + b[i])
}

/// `a − b`.
#[must_use]
pub fn sub<const N: usize>(a: [f32; N], b: [f32; N]) -> [f32; N] {
    std::array::from_fn(|i| a[i] - b[i])
}

/// `v * s`.
#[must_use]
pub fn scale<const N: usize>(v: [f32; N], s: f32) -> [f32; N] {
    v.map(|x| x * s)
}

/// `a + b * s`.
#[must_use]
pub fn add_scaled<const N: usize>(a: [f32; N], b: [f32; N], s: f32) -> [f32; N] {
    std::array::from_fn(|i| a[i] + b[i] * s)
}

/// The dot product of `a` and `b`.
#[must_use]
pub fn dot<const N: usize>(a: [f32; N], b: [f32; N]) -> f32 {
    let mut sum = 0.0;
    for (a, b) in a.iter().zip(&b) {
        sum += a * b;
    }
    sum
}

/// The length of `v`.
#[must_use]
pub fn length<const N: usize>(v: [f32; N]) -> f32 {
    dot(v, v).sqrt()
}

/// The distance between `a` and `b`.
#[must_use]
pub fn distance<const N: usize>(a: [f32; N], b: [f32; N]) -> f32 {
    length(sub(a, b))
}

/// `v` scaled to length 1 (a zero vector stays zero).
#[must_use]
pub fn normalize<const N: usize>(v: [f32; N]) -> [f32; N] {
    let length = length(v);
    if length == 0.0 {
        v
    } else {
        scale(v, 1.0 / length)
    }
}

/// Linear interpolation from `a` (`t = 0`) to `b` (`t = 1`).
#[must_use]
pub fn lerp<const N: usize>(a: [f32; N], b: [f32; N], t: f32) -> [f32; N] {
    add_scaled(a, sub(b, a), t)
}

/// The cross product `a × b` (right-handed).
#[must_use]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `v` rotated by `angle` radians around the unit vector `axis` (counter-clockwise when the
/// axis points at the viewer).
#[must_use]
pub fn rotate(v: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    add(
        add(scale(v, cos), scale(cross(axis, v), sin)),
        scale(axis, dot(axis, v) * (1.0 - cos)),
    )
}

/// The 2D vector `v` rotated by `angle` radians (counter-clockwise when y points up).
#[must_use]
pub fn rotate2(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

/// The direction of `yaw` (0 = ahead, −z; π/2 = right, +x) and `pitch` (up is positive),
/// as a unit vector. With −z as north, `yaw` is a compass heading.
#[must_use]
pub fn direction(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    [cp * sy, sp, -cp * cy]
}

/// A 4×4 matrix stored column by column, as WGSL `mat4x4<f32>` expects.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// The four columns.
    pub cols: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    /// The identity matrix.
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// A translation by `offset`.
    #[must_use]
    pub const fn translation(offset: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[3] = [offset[0], offset[1], offset[2], 1.0];
        m
    }

    /// A scale by `factors` along each axis.
    #[must_use]
    pub const fn scale(factors: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[0][0] = factors[0];
        m.cols[1][1] = factors[1];
        m.cols[2][2] = factors[2];
        m
    }

    /// A rotation whose columns are the new x, y and z axes (`right`, `up`, `back`).
    #[must_use]
    pub const fn basis(right: Vec3, up: Vec3, back: Vec3) -> Self {
        Self {
            cols: [
                [right[0], right[1], right[2], 0.0],
                [up[0], up[1], up[2], 0.0],
                [back[0], back[1], back[2], 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// A rotation around the x axis by `angle` radians (counter-clockwise when looking
    /// from +x towards the origin).
    #[must_use]
    pub fn rotation_x(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        let mut m = Self::IDENTITY;
        m.cols[1] = [0.0, cos, sin, 0.0];
        m.cols[2] = [0.0, -sin, cos, 0.0];
        m
    }

    /// A rotation around the y axis by `angle` radians (yaw towards the right is negative
    /// here: use [`Mat4::heading`] for compass headings).
    #[must_use]
    pub fn rotation_y(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        let mut m = Self::IDENTITY;
        m.cols[0] = [cos, 0.0, -sin, 0.0];
        m.cols[2] = [sin, 0.0, cos, 0.0];
        m
    }

    /// A rotation around the z axis by `angle` radians.
    #[must_use]
    pub fn rotation_z(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        let mut m = Self::IDENTITY;
        m.cols[0] = [cos, sin, 0.0, 0.0];
        m.cols[1] = [-sin, cos, 0.0, 0.0];
        m
    }

    /// Turns a model that faces −z (north) to face `yaw` (clockwise seen from above, as
    /// [`direction`]).
    #[must_use]
    pub fn heading(yaw: f32) -> Self {
        Self::rotation_y(-yaw)
    }

    /// A perspective projection: vertical field of view `fov_y` in radians, `aspect` =
    /// width / height, and the near and far clip distances (both positive).
    #[must_use]
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

    /// A camera at `eye` looking at `target`, with `up` pointing up.
    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        Self::look_to(eye, sub(target, eye), up)
    }

    /// A camera at `eye` looking along `forward`, with `up` pointing up.
    #[must_use]
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

    /// `self * [p, 1]`: the point in homogeneous coordinates, without the perspective
    /// division.
    #[must_use]
    pub fn apply(&self, p: Vec3) -> [f32; 4] {
        let mut out = self.cols[3];
        for (col, value) in self.cols.iter().take(3).zip(p) {
            for (o, c) in out.iter_mut().zip(col) {
                *o += c * value;
            }
        }
        out
    }

    /// Transforms a point (with perspective division).
    #[must_use]
    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        let out = self.apply(point);
        // Points at infinity (w = 0) are returned without division.
        let w = if out[3] == 0.0 { 1.0 } else { out[3] };
        [out[0] / w, out[1] / w, out[2] / w]
    }

    /// `self * [v, 0]`: a direction, unaffected by translation.
    #[must_use]
    pub fn transform_direction(&self, v: Vec3) -> Vec3 {
        let mut out = [0.0; 3];
        for (col, value) in self.cols.iter().take(3).zip(v) {
            for (o, c) in out.iter_mut().zip(col) {
                *o += c * value;
            }
        }
        out
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

/// A perspective camera: position, orientation, lens and the picture's aspect ratio.
///
/// It gives the matrices for a renderer ([`Camera::view`], [`Camera::projection`]) and
/// answers the questions a game asks without them: where a point appears in the picture
/// ([`Camera::project`]) and which way a picture point looks ([`Camera::ray`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Position.
    pub eye: Vec3,
    /// Unit vector the camera looks along.
    pub forward: Vec3,
    /// Unit vector up in the picture (perpendicular to `forward`).
    pub up: Vec3,
    /// Width / height of the picture, in real (square) units.
    pub aspect: f32,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Near clip distance (positive): closer points are not shown.
    pub near: f32,
    /// Far clip distance.
    pub far: f32,
}

impl Camera {
    /// Default vertical field of view: 60°.
    pub const FOV_Y: f32 = std::f32::consts::FRAC_PI_3;
    /// Default near clip distance.
    pub const NEAR: f32 = 0.1;
    /// Default far clip distance.
    pub const FAR: f32 = 1000.0;

    /// A camera at `eye` looking along `forward`, rolled so that `up` is up, with the
    /// default lens ([`Camera::FOV_Y`], [`Camera::NEAR`], [`Camera::FAR`]).
    #[must_use]
    pub fn new(eye: Vec3, forward: Vec3, up: Vec3, aspect: f32) -> Self {
        let forward = normalize(forward);
        let side = normalize(cross(forward, up));
        Self {
            eye,
            forward,
            up: cross(side, forward),
            aspect,
            fov_y: Self::FOV_Y,
            near: Self::NEAR,
            far: Self::FAR,
        }
    }

    /// The same camera with another lens: vertical field of view in radians and the near
    /// and far clip distances.
    #[must_use]
    pub fn with_lens(mut self, fov_y: f32, near: f32, far: f32) -> Self {
        self.fov_y = fov_y;
        self.near = near;
        self.far = far;
        self
    }

    /// Unit vector to the right in the picture.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        cross(self.forward, self.up)
    }

    /// The view matrix (world to camera).
    #[must_use]
    pub fn view(&self) -> Mat4 {
        Mat4::look_to(self.eye, self.forward, self.up)
    }

    /// The projection matrix (camera to clip space).
    #[must_use]
    pub fn projection(&self) -> Mat4 {
        Mat4::perspective(self.fov_y, self.aspect, self.near, self.far)
    }

    /// `projection * view`.
    #[must_use]
    pub fn view_projection(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Where `point` appears in a `width` × `height` picture (pixels from the top left)
    /// and its distance along the view, or `None` behind the camera (closer than `near`).
    #[must_use]
    pub fn project(&self, point: Vec3, width: f32, height: f32) -> Option<(f32, f32, f32)> {
        let offset = sub(point, self.eye);
        let depth = dot(offset, self.forward);
        if depth < self.near {
            return None;
        }
        let tan = (self.fov_y / 2.0).tan();
        let x = dot(offset, self.right()) / (depth * tan * self.aspect);
        let y = dot(offset, self.up) / (depth * tan);
        Some((
            f32::midpoint(x, 1.0) * width,
            f32::midpoint(1.0, -y) * height,
            depth,
        ))
    }

    /// The direction through the picture point `(sx, sy)` in −1..1 (y up), not normalized.
    #[must_use]
    pub fn ray(&self, sx: f32, sy: f32) -> Vec3 {
        let tan = (self.fov_y / 2.0).tan();
        add(
            add(self.forward, scale(self.right(), sx * tan * self.aspect)),
            scale(self.up, sy * tan),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    fn assert_near(actual: Vec3, expected: Vec3) {
        for (a, e) in actual.iter().zip(&expected) {
            assert!((a - e).abs() < 1e-4, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn vectors_of_any_length() {
        assert_eq!(add([1.0, 2.0], [3.0, 4.0]), [4.0, 6.0]);
        assert_eq!(sub([1.0, 2.0, 3.0], [1.0, 1.0, 1.0]), [0.0, 1.0, 2.0]);
        assert_eq!(scale([1.0, -2.0], 2.0), [2.0, -4.0]);
        assert_eq!(add_scaled([1.0, 1.0], [2.0, 0.0], 0.5), [2.0, 1.0]);
        assert_eq!(dot([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]), 32.0);
        assert_eq!(length([3.0, 4.0]), 5.0);
        assert_eq!(distance([1.0, 1.0], [4.0, 5.0]), 5.0);
        assert_eq!(normalize([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
        assert_eq!(lerp([0.0, 10.0], [10.0, 20.0], 0.25), [2.5, 12.5]);
        let [x, y] = rotate2([1.0, 0.0], FRAC_PI_2);
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn identity_translation_and_scale() {
        assert_near(
            Mat4::IDENTITY.transform_point([1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0],
        );
        assert_near(
            Mat4::translation([1.0, -1.0, 2.0]).transform_point([1.0, 1.0, 1.0]),
            [2.0, 0.0, 3.0],
        );
        assert_near(
            Mat4::scale([2.0, 3.0, 4.0]).transform_point([1.0, 1.0, 1.0]),
            [2.0, 3.0, 4.0],
        );
        assert_eq!(Mat4::default(), Mat4::IDENTITY);
        // Directions ignore the translation.
        assert_near(
            Mat4::translation([5.0, 5.0, 5.0]).transform_direction([0.0, 1.0, 0.0]),
            [0.0, 1.0, 0.0],
        );
    }

    #[test]
    fn rotations_are_counter_clockwise() {
        assert_near(
            Mat4::rotation_z(FRAC_PI_2).transform_point([1.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0],
        );
        assert_near(
            Mat4::rotation_x(FRAC_PI_2).transform_point([0.0, 1.0, 0.0]),
            [0.0, 0.0, 1.0],
        );
        assert_near(
            Mat4::rotation_y(FRAC_PI_2).transform_point([0.0, 0.0, 1.0]),
            [1.0, 0.0, 0.0],
        );
        assert_near(
            rotate([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], FRAC_PI_2),
            [0.0, 1.0, 0.0],
        );
    }

    #[test]
    fn directions_follow_the_compass() {
        assert_near(direction(0.0, 0.0), [0.0, 0.0, -1.0]);
        assert_near(direction(FRAC_PI_2, 0.0), [1.0, 0.0, 0.0]);
        assert_near(direction(0.0, FRAC_PI_2), [0.0, 1.0, 0.0]);
        // A model facing north turned to the east faces east.
        let east = Mat4::heading(FRAC_PI_2).transform_direction([0.0, 0.0, -1.0]);
        assert_near(east, [1.0, 0.0, 0.0]);
        // A basis maps the axes onto its columns.
        let m = Mat4::basis([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        assert_near(m.transform_direction([1.0, 0.0, 0.0]), [0.0, 0.0, -1.0]);
    }

    #[test]
    fn multiplication_applies_the_right_matrix_first() {
        let m = Mat4::translation([10.0, 0.0, 0.0]) * Mat4::scale([2.0, 2.0, 2.0]);
        assert_near(m.transform_point([1.0, 1.0, 1.0]), [12.0, 2.0, 2.0]);
        assert_eq!(Mat4::IDENTITY * m, m);
        assert_eq!(m * Mat4::IDENTITY, m);
    }

    #[test]
    fn perspective_maps_near_and_far_to_zero_and_one() {
        let p = Mat4::perspective(FRAC_PI_2, 2.0, 1.0, 10.0);
        assert_near(p.transform_point([0.0, 0.0, -1.0]), [0.0, 0.0, 0.0]);
        assert_near(p.transform_point([0.0, 0.0, -10.0]), [0.0, 0.0, 1.0]);
        // 90° field of view: the top edge at the near plane is y = 1.
        assert_near(p.transform_point([2.0, 1.0, -1.0]), [1.0, 1.0, 0.0]);
    }

    #[test]
    fn look_at_moves_the_target_in_front_of_the_camera() {
        let view = Mat4::look_at([0.0, 0.0, 5.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_near(view.transform_point([0.0, 0.0, 0.0]), [0.0, 0.0, -5.0]);
        assert_near(view.transform_point([1.0, 2.0, 0.0]), [1.0, 2.0, -5.0]);
        let side = Mat4::look_at([5.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_near(side.transform_point([0.0, 0.0, 0.0]), [0.0, 0.0, -5.0]);
        // Looking along −x, +z is on the left.
        assert_near(side.transform_point([0.0, 0.0, 1.0]), [-1.0, 0.0, -5.0]);
    }

    #[test]
    fn camera_projection_matches_the_matrices() {
        let camera = Camera::new([10.0, 5.0, 3.0], [0.3, -0.2, -1.0], [0.0, 1.0, 0.0], 1.6)
            .with_lens(1.05, 0.5, 2400.0);
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
        assert_near(ray, normalize(sub(point, camera.eye)));
    }
}
