//! # `vnce_term`
//!
//! Cross-platform terminal layer of the Variation CLI Engine:
//!
//! - [`ansi`]: our own escape sequence writer.
//! - [`backend`]: the [`Backend`] trait, the real-terminal [`CrosstermBackend`] and the
//!   in-memory [`TestBackend`].
//! - [`terminal`]: the double-buffered [`Terminal`] that only redraws changed cells.
//! - [`capabilities`]: what the terminal supports ([`Capabilities`]).
//! - [`input`]: keyboard, mouse, resize, focus and paste [`Event`]s.
//! - [`query`]: asking the terminal what it supports ([`TerminalInfo`]).
//! - [`graphics`]: how images are shown ([`GraphicsMode`]): Kitty, Sixel, iTerm2 or cells.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod ansi;
pub mod backend;
pub mod capabilities;
pub mod graphics;
pub mod input;
pub mod query;
mod sys;
pub mod terminal;

pub use backend::{Backend, CrosstermBackend, TestBackend};
pub use capabilities::Capabilities;
pub use graphics::{GraphicsMode, GraphicsProtocol};
pub use input::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
pub use query::TerminalInfo;
pub use terminal::{Frame, Terminal, TerminalOptions};
