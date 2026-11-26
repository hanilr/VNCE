//! # `vnce_core`
//!
//! Core types of the Variation CLI Engine:
//!
//! - [`geometry`]: [`Position`], [`Size`] and [`Rect`] in terminal cell coordinates.
//! - [`color`]: [`Color`] (ANSI, 256-color and truecolor) and [`ColorDepth`].
//! - [`style`]: [`Style`] and text [`Modifiers`].
//! - [`cell`]: a single terminal [`Cell`].
//! - [`buffer`]: a grid of cells ([`Buffer`]) with Unicode-aware writing and diffing.
//! - [`math`]: vectors, a 4×4 matrix and a perspective camera for 2D and 3D games.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod buffer;
pub mod cell;
pub mod color;
pub mod geometry;
pub mod math;
pub mod style;

pub use buffer::{Buffer, CellUpdate, Diff};
pub use cell::Cell;
pub use color::{Color, ColorDepth, ParseColorError};
pub use geometry::{Position, Rect, Size};
pub use style::{Modifiers, Style};
