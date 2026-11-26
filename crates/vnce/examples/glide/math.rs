//! Vectors, matrices and the camera.
//!
//! The CPU renderer must work without the `gpu` feature, where `vnce_gpu::Mat4` does not
//! exist, so the game uses the engine's GPU-free types from `vnce_core::math` with the same
//! conventions: right-handed (x east, y up, −z north), column vectors, clip-space depth in
//! `0..=1`. `vnce_gpu::Mat4` is the same type, so the GPU renderer uses it as it is. This
//! module adds what only Glide needs: its lens and distances on the ground.

pub use vnce::vnce_core::math::{
    Camera, Mat4, Vec3, add, add_scaled, cross, direction, dot, length, lerp, normalize, rotate,
    scale, sub,
};

/// Distance on the ground (x and z only).
pub fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// Vertical field of view of every camera.
pub const FOV_Y: f32 = 1.05;
pub const NEAR: f32 = 0.5;
pub const FAR: f32 = 2400.0;

/// A camera at `eye` looking along `forward`, rolled so that `up` is up, with Glide's lens
/// ([`FOV_Y`], [`NEAR`], [`FAR`]).
pub fn camera(eye: Vec3, forward: Vec3, up: Vec3, aspect: f32) -> Camera {
    Camera::new(eye, forward, up, aspect).with_lens(FOV_Y, NEAR, FAR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cameras_use_glides_lens() {
        let c = camera([0.0, 5.0, 0.0], direction(0.3, -0.1), [0.0, 1.0, 0.0], 1.6);
        assert_eq!((c.fov_y, c.near, c.far), (FOV_Y, NEAR, FAR));
        // Closer than the near plane: not in the picture.
        assert!(
            c.project(add_scaled(c.eye, c.forward, 0.4), 100.0, 100.0)
                .is_none()
        );
        assert!((flat_distance([0.0, 9.0, 0.0], [3.0, -9.0, 4.0]) - 5.0).abs() < 1e-6);
    }
}
