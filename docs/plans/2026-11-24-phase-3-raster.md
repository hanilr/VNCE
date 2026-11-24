# Plan: Phase 3 — `vnce-raster` (pixel canvas, CPU rasterizer, pixel modes)

**Date:** 2026-11-24
**Status:** Approved

## Goal

Draw with pixels instead of characters — lines, shapes, triangles, sprites, images — and
convert the result into terminal cells at the highest resolution the terminal can show,
while staying compatible with every device.

## 1. Canvas

- `Rgba` pixels (8-bit RGB + alpha). `Canvas` with `u32` width / height.
- Drawing takes `i32` coordinates; anything outside the canvas (even `i32::MIN`) is
  clipped without panics. `set_clip(rect)` limits drawing to a rectangle.
- Blend modes: `Replace`, `Alpha` (source-over), `Add`, `Multiply`.

## 2. Rasterizer

| Function                   | Details                                                          |
| -------------------------- | ---------------------------------------------------------------- |
| `pixel`                    | One pixel, using the blend mode                                  |
| `line`                     | Bresenham; exact, symmetric end points                           |
| `line_aa`                  | Xiaolin Wu anti-aliased line                                     |
| `rect` / `fill_rect`       | Outline and filled rectangle                                     |
| `circle` / `fill_circle`   | Midpoint, symmetric                                              |
| `fill_triangle`            | Flat color or per-vertex gradient; top-left fill rule            |
| `blit`                     | Copy a canvas: alpha, color key, horizontal / vertical flip      |

## 3. Pixel modes

| Mode        | Pixels per cell | Pixel aspect | Color                         |
| ----------- | --------------- | ------------ | ----------------------------- |
| `HalfBlock` | 1×2             | square       | every pixel keeps its color   |
| `Quadrant`  | 2×2             | 1:2          | two colors per cell           |
| `Sextant`   | 2×3             | 3:4          | two colors per cell           |
| `Octant`    | 2×4             | square       | two colors per cell           |
| `Braille`   | 2×4             | square       | dots + background             |
| `Ascii`     | 1×2             | —            | luminance ramp ` .:-=+*#%@`   |

- Two-color reduction: exhaustive search for 2×2 and 2×3; for 2×4 a fast two-step
  clustering from the farthest pixel pair by default, exhaustive with `Quality::Exact`.
- Sextant / octant tables are generated from the official `UnicodeData.txt` character
  names by a small generator in `tools/`; tests check completeness, uniqueness, width 1
  and round-trips.
- `PixelMode::pixel_aspect()` for drawing round circles.
- Fully transparent cells are left untouched (pixels can be drawn over text).
- Braille: threshold against the cell's mean luminance, optional ordered (Bayer)
  dithering.
- `Canvas::for_area(area, mode)` and `canvas.render(buffer, area, mode, options)`.

## 4. Automatic mode (compatible with every device)

`PixelMode::Auto`:

- No color support → `Ascii`.
- Kitty, WezTerm, foot, Ghostty (they draw block glyphs themselves, independent of the
  font) → `Sextant`.
- Everything else → `HalfBlock` (`▀` exists in practically every font, including the
  Windows and Linux consoles).
- `Octant` and `Braille` are only used when chosen explicitly.
- `VNCE_PIXEL_MODE` overrides the choice.

## 5. Images

PNG loading behind the `png` feature (enabled by default), using the pure-Rust `png`
crate: `Canvas::from_png(bytes)` and `Canvas::load_png(path)`, converting every PNG color
type and bit depth to RGBA.

## 6. Demo: `examples/raster.rs`

Plasma background, rotating gradient triangle, anti-aliased wireframe cube, bouncing
sprite. `M` cycles pixel modes live (name, pixel resolution, FPS and conversion time
shown), `D` toggles dithering.

## Commits

1. `Add vnce-raster: canvas, blending, clipping and primitives`
2. `Add pixel modes: half-block, quadrant, sextant, octant, braille and ASCII`
3. `Add PNG loading behind the png feature`
4. `Add raster demo and Phase 3 docs`

## Risks / notes

- Fonts without sextant / octant glyphs show boxes; mitigated by the conservative `Auto`.
- `▀` and box-drawing characters are East Asian "ambiguous width"; some CJK-locale
  terminals render them double width.
- Exhaustive octant search is slower; the default heuristic must keep a 200×60-cell frame
  within a few milliseconds (measured in the demo).

## Implementation notes

- Automatic selection is `PixelMode::detect(color_depth)` (plus `PixelMode::from_env`
  for tests) instead of a `PixelMode::Auto` variant, so every `PixelMode` value has a
  fixed cell size.
- Coordinate convention for every primitive: pixel `(x, y)` is the point `(x, y)`.
- In two-color modes pixel 0 of a cell always belongs to the background group, so the
  same pixels always produce the same cell (no flicker between frames).
- Quadrant and sextant always use the exhaustive search (it is cheap); `Quality` only
  affects octant and Braille.
- Dithering applies to Braille and ASCII.
- Conversion times measured with `cargo run --release --example raster -- --bench`
  (200×60 cells): half-block 0.35 ms, quadrant 1.2 ms, sextant 2.1 ms, octant 1.5 ms
  (exact 6.2 ms), Braille 1.5 ms (exact 6.1 ms), ASCII 0.4 ms.
- A full-screen animation that changes every cell every frame produces a lot of output
  (about 5–6 MB/s at 60 FPS in a 120×29 terminal with truecolor). Shorter escape
  sequences could reduce this later.
