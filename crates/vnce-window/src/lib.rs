//! # `vnce_window`
//!
//! A native window for the Variation CLI Engine: the same games that run in a terminal
//! run in their own window, ready to be shipped like any other game.
//!
//! - Cells are drawn on the GPU from a glyph atlas of the bundled `JetBrains Mono` font;
//!   block elements, sextants, octants, braille and box drawing are drawn by code, so
//!   the engine's pixel modes line up exactly with the cell grid.
//! - Images (`Frame::draw_image`) are textures in the window's real pixels
//!   ([`GraphicsMode::Native`](vnce_term::GraphicsMode::Native)), uploaded only when
//!   they change.
//! - Keys are reported with presses and releases, the mouse in cells, and the close
//!   button as [`Event::Closed`](vnce_term::Event::Closed). F11 and Alt+Enter switch
//!   fullscreen.
//!
//! ```no_run
//! use vnce_runtime::{Context, Game, RunConfig};
//! use vnce_term::Frame;
//! use vnce_window::WindowOptions;
//!
//! struct Hello;
//!
//! impl Game for Hello {
//!     fn update(&mut self, _ctx: &mut Context) {}
//!
//!     fn render(&mut self, _ctx: &Context, frame: &mut Frame<'_>) {
//!         frame
//!             .buffer_mut()
//!             .set_string(1, 1, "Hello from a window!", vnce_core::Style::new());
//!     }
//! }
//!
//! vnce_window::run(&mut Hello, RunConfig::default(), &WindowOptions::titled("Hello"))?;
//! # Ok::<(), vnce_runtime::RunError>(())
//! ```
//!
//! The bundled font is `JetBrains Mono` (SIL Open Font License 1.1, see `fonts/OFL.txt`).
//!
//! This crate is part of the **Variation** ecosystem.

// Pixel sizes, cell coordinates and colors are converted freely; every value stays far
// inside the target ranges (cells are at most 65 535, pictures at most a few thousand
// pixels).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel sizes and cell coordinates are small"
)]

mod atlas;
mod gamepad;
mod glyphs;
mod keys;
mod renderer;
mod window;

use vnce_core::ColorDepth;
use vnce_runtime::{Game, RunConfig, RunError};
use vnce_term::{Capabilities, Terminal};

pub use window::{WindowBackend, WindowDriver, WindowError, WindowOptions, WindowSize, open};

/// Runs `game` in a new window until it quits or the window is closed.
///
/// Must be called on the main thread (required by some platforms).
///
/// # Errors
///
/// Returns an error if the configuration is invalid or the window cannot be opened.
pub fn run<G: Game>(
    game: &mut G,
    config: RunConfig,
    options: &WindowOptions,
) -> Result<(), RunError> {
    let (backend, driver) = open(options).map_err(std::io::Error::from)?;
    let capabilities = Capabilities {
        color_depth: ColorDepth::TrueColor,
        key_release_events: true,
        ..Capabilities::default()
    };
    let mut terminal = Terminal::new(backend, capabilities)?;
    terminal.enter_mode(config.terminal)?;
    vnce_runtime::run_with(game, config, terminal, driver)
}
