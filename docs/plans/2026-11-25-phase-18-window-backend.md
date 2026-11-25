# Plan: Phase 18 — Native window backend `vnce-window`

**Date:** 2026-11-25
**Status:** Approved, in progress (the user chose this over writing an own terminal; on
2026-11-25 the user approved the detailed plan below, chose a free font that fits the
project's license, and asked for the most performant way, so showing GPU pictures without
a readback is part of this phase)

## Goal

Games made with vnce can be sold and started like any other game (for example on Steam):
a double-clicked executable in its own window, fullscreen if wanted, without a terminal.
The same game code runs in a terminal (for development and sharing) and in a window (for
release). The window draws the engine's cells and images directly on the GPU, so it is
also the fastest and sharpest way to show a vnce game.

## Idea

The engine already writes every frame through the `Backend` trait (`CrosstermBackend`,
`TestBackend`, the benches' sinks). A new crate `vnce-window` adds a backend that opens a
native window instead of writing escape sequences:

- Cells: a glyph atlas built from a monospace font (bundled, with a fallback for wide
  characters and emoji), drawn as instanced quads with foreground and background colors;
  the terminal look stays exactly as it is.
- Images (`Frame::draw_image`): uploaded as textures and drawn in their cell areas, with
  no Kitty / Sixel encoding and no color limits; GPU canvases (`vnce-gpu`) can be shown
  without a readback.
- Input: keyboard with presses and releases always reported, mouse in cells and pixels,
  window focus and resize mapped to the existing `Event` types; gamepads as a later step.
- Window: title, icon, size in cells or pixels, fullscreen toggle, scaling (whole cells,
  sharp pixels), optional effects (CRT / scanlines) as a final shader pass.
- A game chooses the window with no code change beyond calling `vnce::run`: built with
  the `window` feature it opens in a window (see the decisions below).

## Decisions (approved 2026-11-25)

- **A cargo feature:** `window` is a feature of `vnce`. Without it a game runs in the
  terminal and none of the window's dependencies are built. Built with it, the same game
  opens in a native window by default; `--terminal` (or `VNCE_TERMINAL=1`) still runs it
  in the terminal for development. A game calls `vnce::run` instead of
  `vnce_runtime::run`; nothing else changes.
- **Frames without escape sequences:** `Backend` gains a hook that receives the finished
  frame (the cell buffer and the image requests) directly; a backend that takes it skips
  the ANSI output. The window's `Driver` pumps the `winit` event loop
  (`pump_app_events`) from vnce's own game loop, so the runtime loop stays as it is
  (Windows, macOS, Linux).
- **New engine values (breaking, noted in the changelog):** `GraphicsMode::Native`
  ("window": images in the window's real pixels, any size, no encoding) and
  `Event::Closed` (the window's close button; the runtime quits).
- **Cells:** a glyph atlas rasterized with `fontdue` from the bundled JetBrains Mono
  (SIL Open Font License 1.1: free, may be bundled and shipped with games, the license
  file stays next to it). Block elements, sextants, braille and box drawing are drawn by
  code, not taken from the font, so the pixel modes line up exactly with the cell grid.
  Bold is drawn by offsetting; underline as a line. Cells are instanced quads: one pass
  for backgrounds, one for glyphs.
- **Images:** textures in their cell areas, uploaded only when they changed, scaled with
  the engine's scale quality on the GPU.
- **No readback (most performant):** the window shares the game's GPU device. `vnce_gpu`
  keeps the instance and offers the first `Gpu` it created to the window; `Frame` gets a
  way to show an external texture, and `RenderTarget::show(gpu, frame, area)` hands its
  texture to the window directly, or reads it back and calls `draw_image` in a terminal.
  When the devices cannot be shared (an adapter that cannot present to the window), the
  window falls back to the readback.

## Steps (one commit each)

1. Plan (this section).
2. Engine hooks: the `Backend` frame hook, `GraphicsMode::Native`, `Event::Closed`, the
   external texture request in `Frame`; tests with `TestBackend`.
3. `vnce-window`: the window, the wgpu surface, the event pump and key mapping (presses
   and releases always reported), the cell renderer with the glyph atlas and the drawn
   block / sextant / braille / box glyphs.
4. Images as textures; resize, DPI, focus, fullscreen (F11 / Alt+Enter), window title,
   icon and size options, sharp whole-pixel scaling.
5. Sharing the GPU: `vnce_gpu` instance and shared device, `RenderTarget::show`, the window
   drawing external textures; `gpu_cube`, `gpu_shader`, Relay and Glide use it where their
   pictures are finished on the GPU.
6. `vnce::run` and the `window` feature, `--terminal`; Relay, Isle, Glide and the other
   examples run in a window; a release guide (hiding the console on Windows, a macOS app
   bundle, Linux).
7. Bench (window against Kitty), tests (headless where possible: atlas, drawn glyphs, key
   mapping, rendering into a texture on a software adapter), docs and closing.

## Dependencies (approved)

- `winit` for windows and events (Apache-2.0).
- `fontdue` to rasterize the font (MIT OR Apache-2.0 OR Zlib).
- `wgpu` is already used by `vnce-gpu`.
- JetBrains Mono Regular, bundled with its OFL-1.1 license file.

## Risks / open questions

- The event loop of `winit` owns the main thread on some platforms (macOS); the runtime's
  loop may need to run inside it.
- Fonts: wide characters, emoji and box drawing must line up with the cell grid.
- Not in this phase: an own terminal emulator (a separate project if ever wanted).

## Order

After Phase 17 (Glide). Phases 15–17 plans are in this folder.
