# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `vnce-window` (`vnce_window`): games in a native window. `winit` for the window and
  its events (pumped from the engine's own loop), `wgpu` to draw: every cell in one
  instanced draw from a glyph atlas of the bundled JetBrains Mono (SIL Open Font
  License 1.1, with fake bold and italic); block elements, quadrants, sextants, octants,
  braille and box drawing drawn by code, so pixel modes and borders line up exactly.
  Images are textures uploaded only when they change. Keys with presses and releases,
  the mouse in cells, the close button, resizing and scale factors; F11 / Alt+Enter for
  fullscreen; title, size, icon, font size and vsync options; `VNCE_WINDOW_SCREENSHOT`
  saves a frame as PNG. `vnce_window::run` runs a game in a window; the `hello` example
  tours it.
- `vnce_raster::block_pattern` gives the grid of a quadrant, sextant or octant symbol,
  for renderers that draw them themselves.
- Frames for native windows (`vnce_term`): `Backend::present` receives a finished frame
  (`FrameView`: the cell buffer and the images in drawing order, each marked when it
  changed) and the terminal then writes no escape sequences. `GraphicsMode::Native`
  ("window") shows images in the window's real pixels; `Frame::draw_texture` shows an
  `ExternalImage` (for example a GPU texture) without copying it, when
  `Frame::accepts_textures` says so. `TerminalInfo::native_window` marks such a backend.
  `Event::Closed` (a window's close button) reaches the game, then the runtime quits.
- `vnce_gpu` lighting and fog: `MeshRenderer::set_scene` takes a `MeshScene` with a sun
  and ambient light (`Light`) and distance fog (`Fog`, whose alpha can fade far meshes
  out over a sky behind the image). Vertices with a normal are lit (normals are turned
  into world space with the inverse transpose of the model matrix); vertices without one
  keep their colors, so the default scene draws as before. `Vertex::lit` builds a vertex
  with a normal, `flat_shaded` gives every triangle its own face normal, and `math`
  exposes `sub`, `dot`, `cross` and `normalize`. `gpu_cube` (`L` toggles light and fog)
  and Relay's reactor core use them.
- `relay` example: the showcase game. Five wings of a dark station show the GPU shader
  and mesh renderer (with CPU fallbacks), every pixel mode next to real pixels and fast /
  exact scaling, synthesized music and a pad puzzle, and the widgets with Unicode text;
  light sprites are added into a layer that multiplies the scene. `--bench` measures it
  without a screen; a smoke test plays it.
- `isle` example: the 2.5D showcase. An isometric island generated from a six-letter code,
  with height levels, stairs, cliffs, forests and beaches; collect seven light crystals
  and light the beacon before night falls. The camera turns in eased quarter turns and
  zooms; the terrain is cached per direction and only tiles in front of moving things are
  drawn again, so trees and cliffs hide what is behind them, with an x-ray outline of the
  hidden player. Click a tile to walk there (A*), with a path preview. Crabs and wisps,
  rain with lightning, cloud shadows, fireflies, foam, the lantern and other lights in a
  multiplied light layer, a minimap, gauges, menus, rebindable keys and a save file; an
  ocean from a GPU shader (`gpu`) or the CPU; sound with `audio`. `--bench` and a smoke
  test that goes through the menus.
- `glide` example: the 3D showcase. Fly a small craft over endless low-poly land
  generated from a six-letter code (seas, beaches, valleys, snowy ridges, trees and
  rocks), built in chunks ahead of the craft. A ring course against the clock with
  checkpoints, or free flight where every ring adds time; bank to turn, dive to gain
  speed, rings give a boost. With `gpu` the meshes are lit and fogged by `MeshRenderer`
  over a `ShaderQuad` sky with clouds, read back pipelined; without a GPU the CPU clips,
  lights, fogs and sorts the triangles at lower detail. The day runs from sunrise towards
  sunset. Chase, cockpit and orbit cameras (mouse drag), photo mode, exhaust trail,
  sparks, speed lines, compass, radar, gauges, a blob shadow; an engine hum that follows
  the speed with `audio`; menus, settings (renderer, MSAA, render resolution,
  upscaling), rebindable keys and a save file. `--bench` and smoke tests.
- `perf` example: full-screen Kitty and Sixel image scenarios (a still picture and a
  moving sprite), timing `draw_image` and the flush separately.
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
  (`deny.toml`) and a check of the per-crate license files.
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
- Text names for keys and bindings: `KeyCode` and `vnce_game::Binding` implement `Display`
  and `FromStr` (`w`, `Space`, `Esc`, `F5`, `mouse:left`, ...), so controls can be saved.
- `vnce_game::SaveFile`: plain-text `key = value` save files in the user's data folder
  (`data_dir`: `%APPDATA%`, `~/Library/Application Support`, `$XDG_DATA_HOME` or
  `~/.local/share`) with typed values, key bindings and safe writes through a temporary
  file; missing or broken files start empty.
- `lantern` keeps the best score, volumes, frame rate, display method and controls in its
  save file between runs.
- `lantern` controls screen (Settings → Controls): change the key of every action, reset
  one or all to the defaults; keys used twice are marked.
- Tutorial in `docs/tutorial/` (eight chapters, from the first window to sound and
  performance); its code blocks are compiled and run as doc-tests of `vnce`.
- `perf` example: timings of the per-frame work (terminal output, diffing, canvas
  drawing, scaling) without screen output.
- `Buffer::diff_iter` (and the `Diff` iterator): the changed cells without collecting
  them.
- `ansi::move_right` (`CSI n C`); `TestBackend` understands `CSI n C` and `CSI n G`.
- The repository is on GitHub (`https://github.com/hanilr/VNCE`) with CI on Windows,
  Linux and macOS.
- `ScaleQuality` (`Fast`, `Exact`), `Canvas::scaled_with`, `Terminal::scale_quality` /
  `set_scale_quality` and the `VNCE_SCALE_QUALITY` environment variable.

### Changed

- **Breaking:** `Event` has a `Closed` variant, `GraphicsMode` a `Native` variant and
  `TerminalInfo` a `native_window` field; exhaustive matches and struct literals must
  add them.
- **Breaking:** `vnce_gpu::Vertex` has a `normal` field (between `position` and `color`)
  and `MeshDraw` a `model` matrix (model to world space, for light and fog). Code that
  builds them as struct literals must add the field; `Vertex::new` (unlit) still works and
  `MeshDraw::new(mesh, view_projection, model)` fills both transforms.
- Images with graphics protocols are cheaper every frame (Phase 15): `Frame::draw_image`
  compares the image with the one on the screen instead of hashing it, copies changed
  images into reused memory, and Sixel images that already fit their cells are not
  copied. Relay's `render` with Kitty or Sixel dropped from about 9 ms to 1 ms per frame
  (whole frame: Kitty 15.1 → 6.8 ms, Sixel 23.3 → 13.8 ms).
- `Canvas` implements `Clone::clone_from` by copying into its existing pixel memory.
- Faster per-frame work (Phase 12, measured with the `perf` example): a full-screen
  truecolor frame is written twice as fast, an empty frame or a mostly unchanged text
  screen 2.5 times as fast; cell widths are cached, diffing does not allocate and escape
  sequences are written without `write!`. Downsampling to 256 colors uses a table.
- The terminal moves the cursor with `CSI n C` for jumps on the same row (9 % fewer bytes
  for scattered changes).
- Canvas: spans and blits are drawn row by row (opaque rectangles 11 times faster, blits
  1.5 times), triangles search the ends of each row (1.6 times faster). The same pixels
  are drawn as before.
- `Canvas::scaled` uses two passes on several threads (4 times faster when enlarging to
  1600 × 880); channels can differ by one from before, which `ScaleQuality::Exact` keeps.
- Windows: `input::poll` (and so the game loop) ends its wait within about a millisecond
  instead of on the system timer's 15.6 ms steps, so frames keep an even pace.
- `Context::quit`, `set_paused`, `set_fps_limit`, `set_update_rate` and
  `set_graphics_mode` take `&self`, so they can be called while the input from
  `Context::input` is still borrowed. `Context` is no longer `Sync`.
- `lantern`: the cave floor uses four randomly placed tile variants (specks, a pebble, a
  crack) instead of one repeating tile.
- Minimum supported Rust version raised from 1.85 to 1.87 (required by `wgpu` 30).

### Fixed

- `relay`: going back to the title from the pause menu no longer leaves the station on
  the scene stack (a `Transition::Switch` from the menu replaced only the menu, so every
  game played added a scene). The menu now asks the station to switch.
- `cargo test --no-default-features` no longer fails on the tutorial: its doc-tests are
  compiled only with the `game` and `ui` features.
- The core robustness test builds with Rust 1.87 again, and the `input::poll` timing test
  no longer depends on how precisely the machine sleeps (it failed on macOS CI).
- `hello` example: frames are paced by the clock; input events (for example mouse
  movement) no longer trigger extra frames and speed up the animation.
- `raster` example: `M` only cycles through available modes. Octant mode showed question
  marks because almost no font contains the Unicode 16 octant characters; it is now only
  reachable with `Shift+M` / `--all-modes`, with a warning on the status line.
- `TestBackend` no longer loops forever on escape sequences other than CSI.
- Terminal queries: an unterminated APC, DCS or OSC answer (or a broken CSI) no longer
  hides the answers after it; an `ESC` inside a sequence cancels it, as in xterm.
- The raster robustness tests build without the `png` feature.
