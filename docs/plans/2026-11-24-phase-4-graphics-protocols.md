# Plan: Phase 4 — Terminal graphics protocols (Kitty, Sixel, iTerm2)

**Date:** 2026-11-24
**Status:** Approved

## Goal

Show real pixels on terminals that support a graphics protocol, and fall back to the
cell-based pixel modes everywhere else, automatically.

## 1. Terminal queries (`vnce_term::query`)

Sent once in `Terminal::enter`, before any input is read, in a single write:

| Query                     | Answer used for                                   |
| ------------------------- | ------------------------------------------------- |
| XTVERSION `CSI > 0 q`     | terminal name and version                         |
| Kitty graphics `APC G a=q`| Kitty graphics protocol support                   |
| Kitty keyboard `CSI ? u`  | keyboard enhancement support                      |
| `CSI 16 t` / `CSI 14 t`   | cell size / window size in pixels                 |
| XTSMGRAPHICS              | Sixel color registers and maximum image size      |
| DA1 `CSI c` (last)        | Sixel support (`4`); every terminal answers it    |

- The DA1 answer marks the end of the answers; without it the query times out (300 ms).
- Raw input is read with `rustix` (Unix: poll + read on the terminal) and `windows-sys`
  (Windows: console input with virtual terminal input enabled temporarily).
- `Backend::query` so that `TestBackend` can script answers; a parser for CSI, DCS, APC
  and OSC answers.
- Keyboard enhancement uses the query answer instead of `crossterm`'s own check.

## 2. Encoders (`vnce_raster::protocol`)

- **Kitty:** RGBA (`f=32`), base64, 4096-byte chunks, image ids, placement in cells
  (`c`, `r`), no cursor movement (`C=1`), quiet (`q=2`), delete by id.
- **Sixel:** palette of up to 256 colors (median cut) with optional Floyd–Steinberg
  dithering, run-length encoding, transparent background, bands of 6 rows.
- **iTerm2:** `OSC 1337 File=inline=1;width=…;height=…` with a base64 PNG (requires the
  `png` feature).
- Own base64 encoder.

## 3. Terminal integration

- `frame.draw_image(area, &canvas)`.
- Cells under Sixel / iTerm2 images are not rewritten (text would erase the image).
- Images are only sent again when their content (hash) or area changes; removed or moved
  images get their cells redrawn (and Kitty images deleted).
- Sixel images are resampled to the exact pixel size of their area (cell size from the
  query, with a fallback) and never touch the last row (no scrolling).
- `GraphicsMode`: `Kitty | Sixel | Iterm2 | Cells(PixelMode)`; detection order Kitty →
  Sixel → iTerm2 → cells; `VNCE_GRAPHICS` override; tmux / screen fall back to cells.
- `TestBackend` records image sequences instead of printing them.

## 4. Demo: `examples/image.rs`

Shows a PNG with the best method; `G` cycles methods; status line with terminal name and
version, supported protocols, cell size and encoding time.

## Dependencies

| Crate         | Platform | Reason                         |
| ------------- | -------- | ------------------------------ |
| `rustix`      | Unix     | raw terminal input for queries |
| `windows-sys` | Windows  | raw console input for queries  |

Both are already in the dependency tree through `crossterm`.

## Commits

1. `Add terminal query support (DA1, XTVERSION, kitty, cell size)`
2. `Add kitty, sixel and iTerm2 image encoders`
3. `Integrate images into Terminal with automatic fallback`
4. `Add image demo and Phase 4 docs`

## Risks / notes

- Sixel and iTerm2 resend the whole image on every change: best for still or slowly
  changing images; games should use cells or Kitty.
- No Kitty compression in the first version.
- tmux / screen need passthrough wrapping; not supported yet, cells are used instead.

## Implementation notes

- The queries run inside `Backend::enter` (the backend owns raw input), and the backend
  enables keyboard enhancement from the answer; `Terminal` reads the result with
  `Backend::terminal_info`. `TerminalOptions::query_terminal` turns the queries off.
- Verified on a real Windows console: the DA1 answer arrives in about 1 ms and is parsed
  correctly (no Sixel there, so cells are used).
- Images are encoded in `vnce_raster::protocol` (no terminal dependency); `vnce_term` now
  depends on `vnce_raster`. Its `png` feature (default) enables iTerm2.
- Kitty placements use `z=0` (above text) and `C=1` (the cursor does not move).
- Sixel images are scaled to the exact pixel size of their area; the cell size comes from
  `CSI 16 t`, else the window size divided by the cell count, else 10×20.
- `Canvas::scaled` was added (bilinear enlarging, area-averaging shrinking,
  premultiplied alpha).
- `TestBackend` records graphics sequences instead of printing them; this also fixed an
  endless loop on escape sequences other than CSI.
- The runtime exposes `Context::terminal_info`, `Context::graphics_mode` and
  `Context::set_graphics_mode`.
