//! # `vnce_term`
//!
//! Cross-platform terminal layer of the Variation CLI Engine:
//!
//! - [`ansi`]: our own escape sequence writer.
//! - [`backend`]: the [`Backend`] trait, the real-terminal [`CrosstermBackend`] and the
//!   in-memory [`TestBackend`].
//! - [`terminal`]: the double-buffered [`Terminal`] that only redraws changed cells.
//! - [`capabilities`]: what the terminal supports ([`Capabilities`]).
//!
//! This crate is part of the **Variation** ecosystem.

pub mod ansi;
pub mod backend;
pub mod capabilities;
pub mod terminal;

pub use backend::{Backend, CrosstermBackend, TestBackend};
pub use capabilities::Capabilities;
pub use terminal::{Frame, Terminal, TerminalOptions};
