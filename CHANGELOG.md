# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Cargo workspace with the `vnce` crate (imported as `vnce`).
- Project documentation, license and tooling configuration.
- `vnce-core` (`vnce_core`): geometry (`Position`, `Size`, `Rect`), `Color` with hex/name
  parsing and downsampling to 256 / 16 colors, `Style` and `Modifiers`, `Cell`, and a
  Unicode-aware `Buffer` with wide-character handling and diffing.
- `vnce-term` (`vnce_term`): own ANSI escape sequence writer with minimal SGR updates,
  double-buffered `Terminal` that writes only changed cells in a single write with
  synchronized output, `CrosstermBackend` (raw mode, alternate screen, restore on drop and
  on panic) and an in-memory `TestBackend` that emulates the screen.
- `vnce_term::input`: engine-owned `Event`, `KeyEvent`, `KeyCode`, `KeyModifiers` and
  `MouseEvent` types with `poll(timeout)` and `read()`.
- `Capabilities::detect()`: color depth from `VNCE_COLOR_DEPTH`, `NO_COLOR`, `COLORTERM`,
  `TERM`, `TERM_PROGRAM` and Windows Terminal; synchronized output disabled where
  unsupported.
- `hello` example: styled and Unicode text, truecolor gradient, animation, resize and
  input events.
- `TerminalOptions::keyboard_enhancement` and `Capabilities::key_release_events`: the kitty
  keyboard protocol is enabled where supported so key releases are reported (always
  available on Windows); it is popped again on restore and on panic.
- `vnce-runtime` (`vnce_runtime`): `Game` trait and `run` with fixed-timestep updates,
  configurable frame limit (`FpsLimit::Fixed(1..=1000)` or `Unlimited`, changeable at run
  time), spiral-of-death protection, interpolation `alpha`, measured FPS / UPS, pause,
  `InputState` (held keys with release events or a timeout fallback, case-insensitive
  letters, mouse) and a deterministic `ScriptedDriver` for tests.
- `stars` example: a small game with held-key movement, interpolation, a fading trail and
  live FPS limit changes.

### Fixed

- `hello` example: frames are paced by the clock; input events (for example mouse
  movement) no longer trigger extra frames and speed up the animation.
- `vnce-raster` (`vnce_raster`): `Rgba` pixels with `Replace`, `Alpha`, `Add` and
  `Multiply` blending; `Canvas` with clipping, Bresenham and anti-aliased (Wu) lines,
  rectangles, circles, triangles with top-left fill rule and per-vertex gradients, sprite
  blitting (regions, flips, color key, opacity) and text-art sprites.
- Pixel modes (`vnce_raster::PixelMode`): `HalfBlock`, `Quadrant`, `Sextant`, `Octant`,
  `Braille` and `Ascii`, with best two-color reduction per cell, ordered dithering, pixel
  aspect ratios, transparency-aware rendering and `PixelMode::detect` (compatible
  everywhere; `VNCE_PIXEL_MODE` override). Block tables are generated from the Unicode
  character database by `tools/gen-block-tables`.
- PNG loading: `Canvas::from_png` and `Canvas::load_png` (feature `png`, enabled by
  default) for every PNG color type and bit depth.
- `raster` example: plasma (or a PNG given on the command line), gradient triangle,
  anti-aliased cube and ring, bouncing sprite; `M` cycles pixel modes, `D` toggles
  dithering, `--bench` prints conversion timings.
- `PixelMode::available` lists the pixel modes expected to display correctly on the current
  terminal; `PixelMode::detect` always returns one of them.
- `raster` example: `M` only cycles through available modes. Octant mode showed question
  marks because almost no font contains the Unicode 16 octant characters; it is now only
  reachable with `Shift+M` / `--all-modes`, with a warning on the status line.
- Terminal queries (`vnce_term::query`, `Terminal::info`): XTVERSION (name and version),
  Kitty graphics and keyboard protocol support, Sixel support, color registers and maximum
  size, cell and window size in pixels; DA1 marks the end of the answers. Raw input is read
  with `rustix` (Unix) and `windows-sys` (Windows). `TerminalOptions::query_terminal`
  (enabled by default); keyboard enhancement now uses the query answer.
- Graphics protocol encoders (`vnce_raster::protocol`): Kitty (chunked base64 RGBA, ids,
  placement in cells, deletion), Sixel (median-cut palette up to 256 colors, optional
  Floyd–Steinberg dithering, run-length encoding, transparency) and iTerm2 (inline PNG).
  `Canvas::scaled` (bilinear / area averaging with premultiplied alpha) and
  `Canvas::to_png`.
- Images in frames: `Frame::draw_image(area, &canvas)` shows real pixels with Kitty, Sixel
  or iTerm2, or cells with a pixel mode. `GraphicsMode::detect` picks Kitty → Sixel →
  iTerm2 → cells from the terminal's answers (`VNCE_GRAPHICS` override; tmux / screen use
  cells). Unchanged images are not sent again, text under Sixel / iTerm2 images is not
  rewritten, removed images are deleted or their cells redrawn, and Sixel / iTerm2 images
  never touch the last row. `Context::terminal_info`, `Context::graphics_mode` and
  `Context::set_graphics_mode` in the runtime.

### Fixed

- `TestBackend` no longer loops forever on escape sequences other than CSI.
- `image` example: shows a PNG (or a generated test image) with the best method; `G` cycles
  through Kitty / Sixel / iTerm2 / cell modes; the status line shows the terminal, its
  protocol support, the cell size and the encoding time and size.

### Changed

- Minimum supported Rust version raised from 1.85 to 1.87 (required by `wgpu` 30).
