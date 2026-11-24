//! # `vnce_ui`
//!
//! A small set of widgets for games and graphics applications in the terminal:
//!
//! - [`Layout`]: splits an area into parts with [`Constraint`]s.
//!
//! Widgets implement [`Widget`] (or [`StatefulWidget`] when they keep state between
//! frames) and draw into a [`vnce_core::Buffer`], usually `frame.buffer_mut()`, so they mix
//! freely with pixel graphics and images in the same frame. Styles are patches: a widget
//! drawn on top of another keeps the colors it does not set.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod layout;
mod widget;

pub use layout::{Constraint, Direction, Layout};
pub use widget::{StatefulWidget, Widget};
