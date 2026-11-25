# Plan: Phase 15 — Real-pixel speed

**Date:** 2026-11-25
**Status:** Proposed (the user answered the design questions; the whole plan of Phases
15–17 still needs approval)

## Goal

Make images shown with a graphics protocol (Kitty, Sixel, iTerm2) cheap to draw every
frame. In the Phase 14 bench, Relay took 17.6 ms per frame with Kitty and 28.6 ms with
Sixel, about 10 ms of it in `render`, against 1 ms with half blocks. The shown image is
about 2000 × 1160 pixels (9.3 MB), and most of that time is spent inside
`Frame::draw_image`, not in the game.

## Findings (`crates/vnce-term/src/terminal.rs`)

1. `draw_image` calls `image_hash` on every frame: an FNV hash fed one byte at a time over
   the whole canvas (9.3 million steps). This is the largest cost.
2. When the image changed, `draw_image` clones the canvas: a new 9.3 MB allocation and
   copy per frame.
3. For Sixel, `sixel_shown` calls `Canvas::scaled`, which clones the canvas when it
   already has the exact pixel size of its area: a second 9.3 MB copy.

## Steps (one commit each)

1. **Measurement** — `crates/vnce/examples/perf.rs`: full-screen image scenarios for Kitty
   and Sixel (a large picture with a moving sprite), timing `draw` and counting bytes.
   Baseline recorded here.
2. **No hash** — `draw_image` compares the canvas with the image already placed in the
   same area (stops at the first difference; a full comparison only when identical).
   `ImageRequest.hash` and `PlacedImage.hash` go; `ImageRequest.canvas == None` already
   means "identical to the image on the screen" in `ImageState::plan`.
3. **Reused buffers** — canvases of replaced images go to a small pool in `ImageState`;
   `draw_image` copies into a pooled canvas with `clone_from` instead of allocating.
4. **No Sixel copy** — `sixel_shown` keeps no copy when the canvas already has the exact
   size; its users take the canvas itself.
5. **Docs and closing** — results (interleaved runs, best of several), ARCHITECTURE,
   CHANGELOG; Relay's bench before and after.

## Tests

The existing image tests (resending, patches, tiles, tall Sixel pixels, tmux) keep
passing; new tests: an identical image is not sent again, a changed image reuses a
pooled buffer, and Sixel output is the same with and without the copy.

## Target

Relay with Kitty / Sixel: `render` from about 10 ms to about 3 ms per frame.

## Dependencies

None.

## Risks / open questions

- Comparing whole images costs about 1 ms when nothing changed; still far below hashing.
- The pool must not keep large buffers forever (it holds at most a few canvases).
