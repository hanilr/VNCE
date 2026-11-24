//! # `vnce_runtime`
//!
//! The game loop of the Variation CLI Engine.
//!
//! Implement [`Game`] and pass it to [`run`]:
//!
//! - [`Game::update`] runs at a **fixed rate** (default 60 times per second), so the game
//!   runs at the same speed on every machine.
//! - [`Game::render`] runs at the **frame limit** ([`FpsLimit`], default 60 FPS), with an
//!   interpolation factor ([`Context::alpha`]) for smooth movement.
//! - Input events are handled as soon as they arrive, but never trigger extra frames.
//! - [`InputState`] tracks which keys are held down, also on terminals that do not report
//!   key releases.
//!
//! ```no_run
//! use vnce_core::Style;
//! use vnce_runtime::{Context, Game, RunConfig};
//! use vnce_term::Frame;
//!
//! struct Counter(u64);
//!
//! impl Game for Counter {
//!     fn update(&mut self, _ctx: &mut Context) {
//!         self.0 += 1;
//!     }
//!
//!     fn render(&mut self, _ctx: &Context, frame: &mut Frame<'_>) {
//!         let text = format!("updates: {}", self.0);
//!         frame.buffer_mut().set_string(0, 0, &text, Style::new());
//!     }
//! }
//!
//! vnce_runtime::run(&mut Counter(0), RunConfig::default())?;
//! # Ok::<(), vnce_runtime::RunError>(())
//! ```
//!
//! This crate is part of the **Variation** ecosystem.

pub mod config;
pub mod context;
pub mod driver;
pub mod input;
mod meter;
mod run;

pub use config::{ConfigError, FpsLimit, MAX_RATE, RunConfig};
pub use context::Context;
pub use driver::{Driver, ScriptedDriver, TerminalDriver};
pub use input::InputState;
pub use run::{RunError, run, run_with};

use vnce_term::{Event, Frame};

/// A game (or any interactive application) driven by [`run`].
///
/// Only [`Game::update`] and [`Game::render`] are required.
pub trait Game {
    /// Called once before the first update.
    fn init(&mut self, _ctx: &mut Context) {}

    /// Called for every input event, as soon as it arrives (also while paused).
    ///
    /// Use it for one-off actions such as menus, pausing or typing. For movement and other
    /// continuous actions, prefer [`Context::input`] in [`Game::update`].
    fn event(&mut self, _ctx: &mut Context, _event: &Event) {}

    /// Advances the game by one fixed time step ([`Context::dt`]).
    fn update(&mut self, ctx: &mut Context);

    /// Draws the current state into `frame`.
    ///
    /// [`Context::alpha`] tells how far the time is between the last update and the next
    /// one, for interpolating positions.
    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>);
}
