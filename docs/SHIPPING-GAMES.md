# Shipping a game

A vnce game runs in a terminal for development and sharing, and in its own window for a
release: a program that starts with a double click, like any other game (for example on
Steam or itch.io). The game code is the same; only the build differs.

## 1. Run through `vnce::run`

Start the game with `vnce::run(&mut game, config)` instead of `vnce_runtime::run`. Built
without the `window` feature it runs in the terminal; built with it, it opens a window:

```toml
[dependencies]
vnce = { version = "0.1", features = ["window"] }   # add "gpu", "audio" as needed
```

```sh
cargo run --release                 # the window
cargo run --release -- --terminal   # the same build in the terminal (or VNCE_TERMINAL=1)
```

For more control (title, size in cells or pixels, font size, fullscreen at start, icon,
vsync) call `vnce::vnce_window::run(&mut game, config, &options)` with
`vnce_window::WindowOptions`.

In the window, F11 and Alt+Enter switch fullscreen, keys are reported with presses and
releases, and the close button sends `Event::Closed` to the game before the loop ends (a
good moment to save). Images are drawn under the cells, so menus stay visible over the
game's picture. With the `gpu` feature, `RenderTarget::show` hands GPU pictures to the
window without reading them back.

## 2. Hide the console on Windows

A Windows program opens a console window unless it is built for the Windows subsystem.
Add this to the top of the game's `main.rs`, so only window builds lose the console:

```rust,ignore
#![cfg_attr(feature = "window", windows_subsystem = "windows")]
```

(with a `window` feature in the game's own `Cargo.toml` that enables `vnce/window`).
Printed output is then not visible; write logs to a file instead.

## 3. Build for each platform

```sh
cargo build --release --features window
```

- **Windows:** ship the `.exe` (and any assets). An icon for the file itself needs a
  resource compiler step (for example the `embed-resource` crate in a `build.rs`); the
  window and taskbar icon come from `WindowOptions::icon`.
- **macOS:** wrap the binary in an app bundle (`Game.app/Contents/MacOS/game` with an
  `Info.plist`), for example with `cargo bundle`. Build on a Mac (or in CI) for Apple
  silicon and Intel.
- **Linux:** ship the binary in a tarball or an AppImage. The window works on X11 and
  Wayland; the system needs `libxkbcommon` and the usual graphics drivers (Vulkan or
  OpenGL).

The GPU drivers are found at run time (Vulkan, DX12, Metal or OpenGL); without a GPU,
games with a CPU renderer still run.

## 4. Save files and settings

`SaveFile::open("game")` stores settings and progress in the user's data folder
(`%APPDATA%\game`, `~/Library/Application Support/game`, `~/.local/share/game`). Store
the display method under a key of its own for the window (see the examples'
`display_key`), so a terminal's choice does not carry over to the window.

## 5. Licenses

For the engine itself, a credit that players can see is enough: say that the game was
made with VNCE (for example "Made with VNCE" in the credits, on the title screen, on the
store page or in the documentation). The additional permission in `NOTICE` allows this;
the engine's `LICENSE` and `NOTICE` may still be shipped, but they do not have to be.
This covers only vnce: the libraries it depends on keep their own licenses, and their
notices belong in the game's third-party licenses (for example with `cargo about`).
A game built with the `window` feature also contains the JetBrains Mono font: include
`crates/vnce-window/fonts/OFL.txt` (SIL Open Font License 1.1). The font may be shipped
and sold with the game; it may not be sold on its own.

## 6. Check before releasing

- Run the game in a window at several sizes and in fullscreen, on each platform.
- `VNCE_WINDOW_SCREENSHOT=shot.png` saves a frame (frame 30, or
  `VNCE_WINDOW_SCREENSHOT_FRAME`) for store pages.
- The examples show the patterns: `cargo run --release --example glide --features
  gpu,window,audio`.
