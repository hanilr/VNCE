//! # `vnce_raster`
//!
//! Pixel graphics for the Variation CLI Engine:
//!
//! - [`Rgba`] pixels and [`BlendMode`]s.
//! - [`Canvas`]: a pixel buffer with clipping and drawing primitives (lines, anti-aliased
//!   lines, rectangles, circles, triangles with gradients, sprite blitting).
//! - [`PixelMode`]s: converting a canvas into terminal cells (half blocks, quadrants,
//!   sextants, octants, Braille or ASCII) with [`Canvas::render`].
//!
//! All drawing functions take signed coordinates; anything outside the canvas or the clip
//! rectangle is ignored, so shapes can safely be partly or completely off-screen.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod canvas;
pub mod color;
mod draw;
pub mod mode;

pub use canvas::{BlitOptions, Canvas, PixelRect};
pub use color::{BlendMode, Rgba};
pub use mode::{ParsePixelModeError, PixelMode, Quality, RenderOptions};
