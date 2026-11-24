//! # `vnce_gpu`
//!
//! GPU rendering for the Variation CLI Engine with [`wgpu`] (Vulkan, DX12, Metal or
//! OpenGL):
//!
//! - [`Gpu`]: connects to a GPU, with a software fallback for machines without one.
//! - [`RenderTarget`]: an offscreen image to render to with your own `wgpu` render
//!   passes, read back into a [`vnce_raster::Canvas`] with [`RenderTarget::read_into`].
//!
//! The canvas is then shown like any other: with `Frame::draw_image` (real pixels through
//! Kitty, Sixel or iTerm2, or cells) or `Canvas::render` (cells). Use
//! `Frame::image_pixel_size` to render at exactly the resolution that will be shown.
//!
//! ```no_run
//! use vnce_gpu::{Gpu, GpuOptions, RenderTarget};
//! use vnce_raster::{Canvas, Rgba};
//!
//! let gpu = Gpu::new(GpuOptions::default())?;
//! let mut target = RenderTarget::new(&gpu, 320, 200);
//! target.clear(&gpu, Rgba::rgb(30, 60, 90));
//! // ... your own render passes on `target.view()` ...
//! let mut canvas = Canvas::new(0, 0);
//! target.read_into(&gpu, &mut canvas)?;
//! # Ok::<(), vnce_gpu::GpuError>(())
//! ```
//!
//! The `wgpu` version used by this crate is re-exported as [`wgpu`].
//!
//! This crate is part of the **Variation** ecosystem.

pub mod color;
mod error;
mod gpu;
mod target;

pub use error::GpuError;
pub use gpu::{Gpu, GpuInfo, GpuOptions, Power};
pub use target::{Readback, RenderTarget};
pub use wgpu;
