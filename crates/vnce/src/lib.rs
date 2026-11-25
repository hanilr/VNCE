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

/// GPU rendering with `wgpu`: render targets read back into canvases, full-screen shaders
/// and meshes. Requires the `gpu` feature.
#[cfg(feature = "gpu")]
pub use vnce_gpu;

/// Widgets for games and graphics applications: layout, text, blocks, gauges, lists and an
/// FPS overlay. Enabled by default (feature `ui`).
#[cfg(feature = "ui")]
pub use vnce_ui;

/// Game building blocks: key bindings, collision, sprites and animation, tile maps,
/// camera, scenes and timers. Enabled by default (feature `game`).
#[cfg(feature = "game")]
pub use vnce_game;

/// Sound: a mixer, WAV (and OGG with the `ogg` feature) decoding, sound effects and tunes
/// generated in code, and playback on the default device. Requires the `audio` feature.
#[cfg(feature = "audio")]
pub use vnce_audio;

/// A native window: the same games in their own window, cells and images drawn on the
/// GPU. Requires the `window` feature.
#[cfg(feature = "window")]
pub use vnce_window;

/// Runs `game` until it quits: in a native window when the program was built with the
/// `window` feature, otherwise (and with `--terminal` on the command line or
/// `VNCE_TERMINAL=1`) in the terminal. The window is titled after the program; use
/// [`vnce_window::run`] for more control.
///
/// # Errors
///
/// Returns an error if the configuration is invalid, or the terminal or the window
/// cannot be used.
pub fn run<G: vnce_runtime::Game>(
    game: &mut G,
    config: vnce_runtime::RunConfig,
) -> Result<(), vnce_runtime::RunError> {
    #[cfg(feature = "window")]
    if !wants_terminal() {
        let title = std::env::current_exe()
            .ok()
            .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "vnce".to_owned());
        return vnce_window::run(game, config, &vnce_window::WindowOptions::titled(title));
    }
    vnce_runtime::run(game, config)
}

/// Whether the terminal was asked for instead of the window.
#[cfg(feature = "window")]
fn wants_terminal() -> bool {
    std::env::args().any(|arg| arg == "--terminal")
        || std::env::var("VNCE_TERMINAL").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// The tutorial in `docs/tutorial`: every code block in it is compiled (and, unless marked
/// `no_run`, run) by `cargo test --doc -p vnce`. It uses the default features (`game`,
/// `ui`), so it is only compiled with them.
#[cfg(all(doctest, feature = "game", feature = "ui"))]
mod tutorial {
    #[doc = include_str!("../../../docs/tutorial/README.md")]
    mod readme {}
    #[doc = include_str!("../../../docs/tutorial/01-first-window.md")]
    mod first_window {}
    #[doc = include_str!("../../../docs/tutorial/02-game-loop-and-input.md")]
    mod game_loop_and_input {}
    #[doc = include_str!("../../../docs/tutorial/03-pixel-graphics.md")]
    mod pixel_graphics {}
    #[doc = include_str!("../../../docs/tutorial/04-sprites-and-animation.md")]
    mod sprites_and_animation {}
    #[doc = include_str!("../../../docs/tutorial/05-tilemaps-camera-collision.md")]
    mod tilemaps_camera_collision {}
    #[doc = include_str!("../../../docs/tutorial/06-scenes-and-ui.md")]
    mod scenes_and_ui {}
    #[cfg(feature = "audio")]
    #[doc = include_str!("../../../docs/tutorial/07-sound.md")]
    mod sound {}
    #[doc = include_str!("../../../docs/tutorial/08-performance.md")]
    mod performance {}
}
