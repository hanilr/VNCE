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
- `image` example: shows a PNG (or a generated test image) with the best method; `G` cycles
  through Kitty / Sixel / iTerm2 / cell modes; the status line shows the terminal, its
  protocol support, the cell size and the encoding time and size.
- `vnce-gpu` (`vnce_gpu`, feature `gpu` of `vnce`): GPU rendering with `wgpu` 30.
  `Gpu` connects to a hardware adapter with a software fallback (WARP, llvmpipe);
  `VNCE_GPU_BACKEND` and `VNCE_GPU_POWER` overrides. `RenderTarget` renders offscreen in sRGB
  and reads back into a `Canvas`, immediately or pipelined. `ShaderQuad` turns one WGSL
  `shade` function into a full-screen shader (errors with line numbers); `MeshRenderer`
  draws colored meshes with depth testing and MSAA; `Mat4` for transforms and cameras.
- `Frame::image_pixel_size`, `Frame::image_pixel_aspect` and `Frame::cell_pixels`: the
  exact pixel size and pixel shape an image is shown with, for rendering at the right
  resolution.
- `gpu_shader` (raymarched scene) and `gpu_cube` (3D cubes with anti-aliasing) examples
  with `--bench` timings.
- `vnce-ui` (`vnce_ui`, feature `ui` of `vnce`, on by default): `Widget` /
  `StatefulWidget`, `Layout` with `Length` / `Percentage` / `Ratio` / `Min` / `Max` / `Fill`
  constraints, `Span` / `Line` / `Text` and `Paragraph` (alignment, word wrapping, `…`,
  scrolling), `Block` (five border sets, titles, padding), `Gauge` (1/8-cell precision),
  `List` / `ListState` (menus), `FpsOverlay` (with a sparkline) and `Clear`.
  `Rect::centered` in `vnce_core`.
- `ui` example: a dashboard with a menu, gauges, a pixel chart, scrolling text, a popup
  and an FPS overlay.
- Every crate package contains `LICENSE` and `NOTICE`, and links to the repository and its
  docs.rs page; docs.rs documents all features.
- Randomized robustness tests for buffers, frames on the emulated screen, the terminal
  answer parser, drawing, pixel modes, the graphics protocol encoders, layouts and widgets.
- CI (GitHub Actions) on Windows, Linux and macOS, a Rust 1.87 check, `cargo-deny`
  (`deny.toml`) and a check of the per-crate license files. Release steps in
  `docs/RELEASING.md`.
- `vnce-game` (`vnce_game`, feature `game` of `vnce`, on by default): `ActionMap` key
  bindings; `Aabb`, `move_and_collide` (through solid tiles, no tunnelling) and
  `SpatialGrid`; `SpriteSheet`, `Animation` (loop, once, ping-pong) and `Animator`;
  `TileMap` (layers, loaded from text) with `Tileset`; `Camera` (follow, smoothing, dead
  zone, bounds, shake, interpolation); `Scene` / `SceneStack` (push, pop, switch, overlays,
  shared data); `Timer` and easing curves.
- `platformer` example: an original platformer with a text level, an animated player,
  coins, a goal, a following camera and title, play, pause and finish scenes.
- `vnce-audio` (`vnce_audio`, feature `audio` of `vnce`, off by default): `Mixer` (32 voices,
  volume, pan, speed, looping, fades, groups, master volume, mute, soft limiter), `Audio`
  playback with `cpal` on any sample format, `Sound` with an own WAV decoder and encoder,
  OGG Vorbis with `lewton` (feature `ogg`), `Synth` sound effects with seeded `Preset`s,
  `note` and `Melody` for tunes.
- `platformer` plays generated sound effects and music with `--features audio`; `sfx`
  example: pick, tweak, hear, see and save sound effects.
- Faster graphics protocols: Kitty images as compressed PNG (`kitty::Options`,
  `transmit_and_place_with`), Sixel encoding about 17 times faster with a palette kept
  across frames (`sixel::Encoder`), and only the changed parts of images sent again
  (Sixel and iTerm2 patches, Kitty tiles).
- Graphics inside tmux and zellij: Sixel when the multiplexer draws it, Kitty in tmux
  through passthrough with Unicode placeholders (`vnce_term::tmux`,
  `kitty::transmit_virtual`, `kitty::placeholder`, `Terminal::set_tmux_passthrough`).
- `lantern` example: an original top-down cave game (generated caves, a lantern light,
  gems, chasing bats, deeper levels) with title, settings (volumes, frame rate, display
  method), pause and game over scenes.
- Sixel `pixel_aspect` (`"2;1`): half-height images with tall rows, used in Windows Terminal
  (`Terminal::set_sixel_tall_pixels`, `VNCE_SIXEL_ASPECT`); elsewhere changed images wait
  a few frames when the terminal falls behind.
- `vnce_game::Rng`: a seeded random generator (xorshift64*) with `below`, `range`, `unit`,
  `chance`, `pick`, `shuffle` and saving / restoring the state; the `lantern` and `stars`
  examples use it instead of their own copies (`stars` now needs the `game` feature).
- `vnce_raster::Font`: bitmap text on a canvas (`Canvas::text`, `Canvas::text_scaled`) with
  a built-in 5×7 font (printable ASCII and the Turkish letters), custom fonts from text
  art and `Font::measure`; `lantern` draws its title and a heads-up display with it.

### Changed

- Minimum supported Rust version raised from 1.85 to 1.87 (required by `wgpu` 30).

### Fixed

- `hello` example: frames are paced by the clock; input events (for example mouse
  movement) no longer trigger extra frames and speed up the animation.
- `raster` example: `M` only cycles through available modes. Octant mode showed question
  marks because almost no font contains the Unicode 16 octant characters; it is now only
  reachable with `Shift+M` / `--all-modes`, with a warning on the status line.
- `TestBackend` no longer loops forever on escape sequences other than CSI.
- Terminal queries: an unterminated APC, DCS or OSC answer (or a broken CSI) no longer
  hides the answers after it; an `ESC` inside a sequence cancels it, as in xterm.
- The raster robustness tests build without the `png` feature.
