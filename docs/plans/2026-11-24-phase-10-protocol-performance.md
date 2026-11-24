# Plan: Phase 10 — Graphics protocols in tmux, and their speed

**Date:** 2026-11-24
**Status:** Approved

## Goal

Make real pixels usable for games, not only for still images, and inside tmux.

## Measurements before (this machine)

| Image     | Kitty encode | Kitty size | Sixel encode | Sixel size |
| --------- | ------------ | ---------- | ------------ | ---------- |
| 640×360   | 1.6 ms       | 1.2 MB     | 93 ms        | 41 KB      |
| 1280×720  | 6.6 ms       | 4.8 MB     | 271 ms       | 106 KB     |
| 1920×1080 | 13.7 ms      | 10.8 MB    | 458 ms       | 206 KB     |

Kitty is fast to encode but sends far too much; Sixel is small but slow to encode
(palette and mapping).

## 1. Kitty: less data

- RGB (`f=24`) when no pixel is transparent: 25 % less.
- zlib compression (`o=z`) with `miniz_oxide` (already in the dependency tree through
  `png`), fast level.
- **Tiles:** when the image is shown at its exact cell resolution, it is split into a grid
  of tiles aligned to cells, each a Kitty image of its own; only the tiles that changed are
  sent again. Works on every terminal with the Kitty protocol (no animation commands).
- Images are replaced under the same id (no delete-then-send flicker).

## 2. Sixel: faster encoding

- Palette from a 5-5-5 histogram instead of every pixel; mapping through a lookup table.
- The palette is reused across frames until the colors change noticeably.
- Bands of 6 rows are encoded in parallel (`std::thread::scope`).
- Only the changed region (expanded to cells) is sent; whole frames when everything
  changed. Same for iTerm2.
- Target: under 40 ms for a full 720p frame.

## 3. tmux and other multiplexers

- No blanket ban: if the multiplexer reports Sixel in its DA1 answer (tmux 3.4 built with
  Sixel, zellij), Sixel is used.
- Kitty inside tmux: commands wrapped in tmux passthrough, images placed with Unicode
  placeholders (`U=1`), so tmux moves them like text; detected by sending the Kitty query
  wrapped too. Needs `set -g allow-passthrough on`.
- GNU screen keeps cells.

## 4. Demos and tests

- `image` and benchmarks show the new timings; the platformer and GPU demos should become
  playable with real pixels (measured).
- Tests: compressed Kitty data decodes to the exact pixels; the fast Sixel palette stays
  within quality bounds of the old one; partial updates give the same screen as full
  ones; tmux wrapping and placeholders; randomized tests cover the new paths.
- tmux is tried in WSL Ubuntu (approved: start it, install tmux if missing).

## Commits

1. `Add Phase 10 plan: tmux and protocol performance`
2. `Send Kitty images as RGB with zlib compression`
3. `Speed up Sixel encoding: histogram palette, lookup table, parallel bands`
4. `Send only the changed parts of images (Kitty tiles, Sixel and iTerm2 regions)`
5. `Support graphics protocols inside tmux and zellij`
6. `Update benchmarks, demos and Phase 10 docs`

## Risks / notes

- Real tmux with Kitty cannot be seen here (Windows Terminal has no Kitty protocol); the
  passthrough path is unit-tested and needs a try on Linux or macOS.
- Tiles are only used when the image matches its cell resolution exactly, so tile edges
  never show seams.

## Implementation notes

- Kitty compression was measured before choosing: on a 720p gradient, raw RGBA is 4.8 MB,
  zlib level 1 1.45 MB in 10.5 ms, PNG with `png::Compression::Fast` (fdeflate) 581 KB in
  4.5 ms; on a game-like frame PNG is 18 KB in about 1 ms. PNG (`f=100`) became the
  default, zlib (`o=z`) the fallback without the `png` feature.
- Sixel: most of the old time went into writing bands (every color of a band scanned the
  whole width six times). Bands are now built in one pass; the palette comes from a 5-5-5
  histogram (exact colors when there are few enough, with a slower exact path only when
  close colors share a bin) and pixels map through a 32 768-entry table. 720p: 271 ms →
  15 ms, same output size. Parallel output is byte-identical to single-threaded output
  (tested).
- Partial updates compare images at cell level. Sixel compares the images as drawn
  (scaled to the cells) and cuts patches from them, so patches never show seams. Changed
  Kitty images keep their ids: freed, then sent again inside the synchronized update.
- tmux: the query adds the Kitty query and a DA1 wrapped for passthrough and reads until
  two DA1 answers arrived; only the first (tmux's) decides Sixel. Kitty commands are
  wrapped right before writing (`tmux::wrap_kitty`), so every Kitty path is covered,
  including deletes after switching modes. Placeholders are written into the cell buffer
  on every frame and only reach the terminal when they change.
- Checked in WSL Ubuntu with tmux 3.6 (already installed, nothing was installed): tmux
  answers DA1 with `?1;2;4` (Sixel), `allow-passthrough` is off by default, and
  placeholders with their diacritics are kept, one column each. Kitty itself cannot be
  seen here (Windows Terminal has no Kitty protocol).
- The Kitty row/column diacritics come from Kitty's `rowcolumn-diacritics.txt` (297 code
  points); a test checks they are distinct and zero-width.
- Measured with a sprite moving over a 1600 × 880 picture on a `TestBackend`: Sixel
  8.7 ms and Kitty 7.1 ms per frame, about 5 KB each.
- Raw ESC bytes that crept into a few test literals during editing were turned back into
  `\x1b` escapes.
- Follow-up after a report of a slower platformer with Sixel (measured on a `TestBackend`
  of 160 × 45 cells, Sixel forced, scripted run): 113 ms of CPU per frame instead of
  80 ms before Phase 10, with spikes of 300 ms. Causes: the image was scaled to its cells
  twice per frame (64 ms each at 1600 × 860), `Canvas::scaled` computed its taps per
  pixel, the demo produced a canvas slightly off the area size so scaling was needed at
  all, and exact palettes searched the whole palette for every pixel of a 5-5-5 bin shared
  by close colors (a sky gradient: 271 ms). After four fixes: 7.8 ms per frame on average,
  21 ms at worst (a full frame with a new palette), 3 KB per frame.
