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
