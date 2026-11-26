//! # `vnce_term`
//!
//! Cross-platform terminal layer of the Variation CLI Engine:
//!
//! - [`ansi`]: our own escape sequence writer.
//! - [`backend`]: the [`Backend`] trait, the real-terminal [`CrosstermBackend`] and the
//!   in-memory [`TestBackend`].
//! - [`terminal`]: the double-buffered [`Terminal`] that only redraws changed cells.
//! - [`capabilities`]: what the terminal supports ([`Capabilities`]).
//! - [`input`]: keyboard, mouse, resize, focus, paste and gamepad [`Event`]s.
//! - [`gamepad`]: gamepad buttons and axes in a standard layout.
//! - [`query`]: asking the terminal what it supports ([`TerminalInfo`]).
//! - [`graphics`]: how images are shown ([`GraphicsMode`]): Kitty, Sixel, iTerm2 or cells.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod ansi;
pub mod backend;
pub mod capabilities;
pub mod gamepad;
pub mod graphics;
pub mod input;
pub mod native;
pub mod query;
mod sys;
pub mod terminal;
pub mod tmux;

pub use backend::{Backend, CrosstermBackend, TestBackend};
pub use capabilities::Capabilities;
pub use gamepad::{GamepadAxis, GamepadButton, GamepadEvent, GamepadEventKind, GamepadId};
pub use graphics::{GraphicsMode, GraphicsProtocol};
pub use input::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ParseKeyCodeError};
pub use native::{ExternalImage, FrameView, ImageView, WindowRequest, WindowState};
pub use query::TerminalInfo;
pub use terminal::{Frame, Terminal, TerminalOptions};
