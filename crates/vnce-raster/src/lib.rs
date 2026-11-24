//! # `vnce_raster`
//!
//! Pixel graphics for the Variation CLI Engine:
//!
//! - [`Rgba`] pixels and [`BlendMode`]s.
//! - [`Canvas`]: a pixel buffer with clipping and drawing primitives (lines, anti-aliased
//!   lines, rectangles, circles, triangles with gradients, sprite blitting).
//! - [`PixelMode`]s: converting a canvas into terminal cells (half blocks, quadrants,
//!   sextants, octants, Braille or ASCII) with [`Canvas::render`].
//! - [`protocol`]: encoders for terminal graphics protocols (Kitty, Sixel, iTerm2).
//! - PNG loading with `Canvas::from_png` / `Canvas::load_png` (feature `png`, enabled by
//!   default).
//!
//! All drawing functions take signed coordinates; anything outside the canvas or the clip
//! rectangle is ignored, so shapes can safely be partly or completely off-screen.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod canvas;
pub mod color;
mod draw;
#[cfg(feature = "png")]
pub mod image;
pub mod mode;
pub mod protocol;

pub use canvas::{BlitOptions, Canvas, PixelRect};
pub use color::{BlendMode, Rgba};
#[cfg(feature = "png")]
pub use image::ImageError;
pub use mode::{ParsePixelModeError, PixelMode, Quality, RenderOptions};
