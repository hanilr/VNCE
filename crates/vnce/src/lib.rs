//! # `vnce`
//!
//! The Variation CLI Engine: a library for building graphics, animations and
//! games in the terminal, rendered on the CPU or the GPU.
//!
//! This crate is part of the **Variation** ecosystem.
//!
//! The API is not stable yet.

/// Core types: geometry, colors, styles, cells and buffers.
pub use vnce_core;

/// Terminal output, double-buffered rendering and terminal backends.
pub use vnce_term;

/// The game loop: fixed-timestep updates, frame limiting, input state and the `Game` trait.
pub use vnce_runtime;

/// Pixel graphics: canvas, blending, drawing primitives and pixel-to-cell conversion.
pub use vnce_raster;
