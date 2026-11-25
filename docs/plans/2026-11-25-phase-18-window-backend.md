# Plan: Phase 18 — Native window backend `vnce-window`

**Date:** 2026-11-25
**Status:** Agreed in direction, after Phase 17 (the user chose this over writing an own
terminal; the details below are reviewed and approved before the phase starts)

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
- `vnce_runtime::run` gains a way to choose the window (for example a `RunConfig` option
  or a `--window` flag that examples pass through), so a game switches with no code
  change.

## Steps (to be detailed before starting)

1. Plan review: dependencies, font handling, API for choosing the window.
2. Window and event loop, the `Backend` implementation for cells (glyph atlas).
3. Images as textures; `vnce-gpu` targets shown directly.
4. Input, resize, focus, fullscreen.
5. Examples: Relay, Isle and Glide run in a window; a release build guide (Windows,
   macOS, Linux) for packaging a game.
6. Bench, tests (headless where possible), docs and closing.

## Dependencies (to be approved)

- `winit` for windows and events (cross-platform).
- `wgpu` is already used by `vnce-gpu`.
- A font rasterizer (for example `fontdue` or `ab_glyph`) and a bundled open-license
  monospace font.

## Risks / open questions

- The event loop of `winit` owns the main thread on some platforms (macOS); the runtime's
  loop may need to run inside it.
- Fonts: wide characters, emoji and box drawing must line up with the cell grid.
- Not in this phase: an own terminal emulator (a separate project if ever wanted).

## Order

After Phase 17 (Glide). Phases 15–17 plans are in this folder.
