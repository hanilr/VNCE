//! A small 4×4 matrix type and 3D vector helpers for transforms and cameras.
//!
//! Conventions match `wgpu`: right-handed coordinates (x right, y up, the camera looks
//! along −z), column vectors (`projection * view * model`), clip-space depth in `0..=1`.

use std::ops::Mul;

/// A 3D vector as `[x, y, z]`.
pub type Vec3 = [f32; 3];

/// A 4×4 matrix stored column by column, as WGSL `mat4x4<f32>` expects.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
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

    /// A rotation around the y axis by `angle` radians.
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
        let forward = normalize(sub(target, eye));
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

    /// Transforms a point (with perspective division).
    #[must_use]
    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        let v = [point[0], point[1], point[2], 1.0];
        let mut out = [0.0; 4];
        for (col, &value) in self.cols.iter().zip(&v) {
            for (o, c) in out.iter_mut().zip(col) {
                *o += c * value;
            }
        }
        // Points at infinity (w = 0) are returned without division.
        let w = if out[3] == 0.0 { 1.0 } else { out[3] };
        [out[0] / w, out[1] / w, out[2] / w]
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

/// `a − b`.
#[must_use]
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The dot product of `a` and `b`.
#[must_use]
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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

/// `v` scaled to length 1 (a zero vector stays zero).
#[must_use]
pub fn normalize(v: Vec3) -> Vec3 {
    let length = dot(v, v).sqrt();
    if length == 0.0 {
        v
    } else {
        [v[0] / length, v[1] / length, v[2] / length]
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    fn assert_near(actual: Vec3, expected: Vec3) {
        for (a, e) in actual.iter().zip(&expected) {
            assert!((a - e).abs() < 1e-5, "{actual:?} != {expected:?}");
        }
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
}
