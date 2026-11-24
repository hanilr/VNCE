//! # `vnce_game`
//!
//! Building blocks for 2D games in the terminal:
//!
//! - [`ActionMap`]: binds keys and mouse buttons to your own actions.
//! - [`Aabb`], [`move_and_collide`] and [`SpatialGrid`]: collision between boxes, against
//!   solid tiles and among many objects.
//!
//! Everything draws on a [`vnce_raster::Canvas`], so games work with every display method
//! of the engine (cells, Kitty, Sixel, iTerm2) and with the GPU renderer. Nothing here
//! imposes a game architecture: use the pieces you need.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod actions;
pub mod collision;

pub use actions::{ActionMap, Binding};
pub use collision::{Aabb, Movement, Sides, SpatialGrid, move_and_collide};
