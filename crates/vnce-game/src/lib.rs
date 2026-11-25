//! # `vnce_game`
//!
//! Building blocks for 2D games in the terminal:
//!
//! - [`ActionMap`]: binds keys and mouse buttons to your own actions.
//! - [`Aabb`], [`move_and_collide`] and [`SpatialGrid`]: collision between boxes, against
//!   solid tiles and among many objects.
//! - [`SpriteSheet`], [`Animation`] and [`Animator`]: animated sprites.
//! - [`TileMap`] and [`Tileset`]: grid worlds loaded from text, with solid tiles.
//! - [`Camera`]: a scrolling view that follows a target, smoothly and inside the world.
//! - [`Scene`] and [`SceneStack`]: title, game, pause and game-over screens.
//! - [`Timer`] and [`ease`]: cooldowns, repeating events and easing curves.
//! - [`Rng`]: seeded random numbers for level generation and gameplay.
//! - [`SaveFile`]: best scores, settings and key bindings kept between runs.
//!
//! Everything draws on a [`vnce_raster::Canvas`], so games work with every display method
//! of the engine (cells, Kitty, Sixel, iTerm2) and with the GPU renderer. Nothing here
//! imposes a game architecture: use the pieces you need.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod actions;
pub mod camera;
pub mod collision;
pub mod random;
pub mod save;
pub mod scene;
pub mod sprite;
pub mod tilemap;
pub mod timer;

pub use actions::{ActionMap, Binding, ParseBindingError};
pub use camera::Camera;
pub use collision::{Aabb, Movement, Sides, SpatialGrid, move_and_collide};
pub use random::Rng;
pub use save::{SaveFile, data_dir};
pub use scene::{Scene, SceneStack, Transition};
pub use sprite::{Animation, Animator, Playback, SpriteSheet};
pub use tilemap::{TileId, TileMap, Tileset};
pub use timer::{Timer, ease};
