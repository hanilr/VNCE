# Plan: Phase 15 — Real-pixel speed

**Date:** 2026-11-25
**Status:** Done (approved with Phases 15–17 on 2026-11-25; commits stay local until the
user asks to push)

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

## Baseline (step 1)

`cargo run --release --example perf -- --frames 100`, full-screen 2000 × 1200 pixel image,
three runs (values agreed within 5 %), ms per frame:

| Scenario | render | flush | bytes per frame |
| --- | ---: | ---: | ---: |
| Kitty, still | 8.7 | 0.1 | 0 |
| Kitty, moving sprite | 9.8 | 2.9 | 164 KB |
| Sixel, still | 8.7 | 0.1 | 0 |
| Sixel, moving sprite | 9.8 | 23.0 | 3.8 KB |

`render` is almost entirely the hash. The Sixel `flush` of 23 ms for a 3.8 KB patch is
also far too much; step 4 looks at where it goes.

(Step 4 found it: Sixel images never cover the last row, so the 1200-pixel scenario image
was scaled to 1180 pixels every frame. The scenarios now use `Frame::image_pixel_size`,
as Relay does; the corrected baseline is in the results below.)

## Results

`perf --frames 100`, the old engine (d0b4a04) and the new one built with the same
scenarios and run interleaved three times; the values agreed within 10 %, ms per frame:

| Scenario | render before | render after | flush before | flush after |
| --- | ---: | ---: | ---: | ---: |
| Kitty, still | 7.7 | 1.2 | 0.04 | 0.04 |
| Kitty, moving sprite | 8.4 | 0.6 | 2.4 | 2.0 |
| Sixel, still | 7.5 | 1.2 | 0.06 | 0.06 |
| Sixel, moving sprite | 8.2 | 0.6 | 2.9 | 1.5 |

Relay `--bench` (GPU pictures, 600 frames, 200 × 60 cells), old and new run interleaved
twice:

| Display method | Per frame before | after | `render` before | after |
| --- | ---: | ---: | ---: | ---: |
| Kitty | 15.1 ms | 6.8 ms | 8.9 ms | 1.0 ms |
| Sixel | 23.3 ms | 13.8 ms | 9.0 ms | 1.2 ms |

The target (render about 3 ms) is met. Comparing an unchanged image is bound by memory
bandwidth (about 1.2 ms for 9.4 MB); comparing in vectorized blocks did not help. The
remaining Sixel time in Relay is encoding: the GPU scene changes most of the image every
frame (161 KB per frame).

## Implementation notes

- A request without a canvas means "identical to the image on the screen"; the `hash`
  fields are gone.
- `ImageState::spare` keeps at most `SPARE_CANVASES` (2) canvases: replaced canvases,
  canvases of deferred requests and of images that left the screen.
- `PlacedImage::shown` is `None` when the canvas already fits its cells; the
  `PlacedImage::shown()` method returns the canvas then.
