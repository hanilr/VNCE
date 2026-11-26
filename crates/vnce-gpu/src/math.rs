//! A small 4×4 matrix type and 3D vector helpers for transforms and cameras.
//!
//! Conventions match `wgpu`: right-handed coordinates (x right, y up, the camera looks
//! along −z), column vectors (`projection * view * model`), clip-space depth in `0..=1`.
//!
//! The types and functions live in [`vnce_core::math`], so code without the GPU (a CPU
//! renderer, game logic) uses the same [`Mat4`] and [`Vec3`]; this module re-exports
//! them, so `vnce_gpu::math` and `vnce_gpu::{Mat4, Vec3}` keep working.

pub use vnce_core::math::*;
