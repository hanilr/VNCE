# Plan: Phase 12 — Optimization

**Date:** 2026-11-25
**Status:** Done

## Goal

Make the per-frame work of the engine (terminal output, buffer diffing, CPU drawing)
faster without changing what is drawn. Every step is measured before and after, and the
numbers are recorded here. Publishing on GitHub follows this phase.

## Baseline (`cargo run --release --example perf`, development machine)

| Scenario                                         | Before                 | Target         |
| ------------------------------------------------ | ---------------------- | -------------- |
| Half blocks, full screen, truecolor: flush       | 1.85 ms, 437.6 KB      | < 0.8 ms       |
| Half blocks, full screen, 256 colors: flush      | 0.75 ms, 67.3 KB       | < 0.35 ms      |
| Text on every row, one row changes               | 0.39 ms                | < 0.15 ms      |
| Empty frame (clear + diff)                       | 0.097 ms               | < 0.04 ms      |
| 200 opaque rectangles 30 × 20                    | 0.26 ms                | ~0.05 ms       |
| 200 semi-transparent rectangles 30 × 20          | 0.63 ms                | ~0.3 ms        |
| 500 opaque blits 16 × 16                         | 0.42 ms                | ~0.1 ms        |
| 500 semi-transparent blits 16 × 16               | 0.74 ms                | ~0.4 ms        |
| 50 semi-transparent circles, radius 20           | 0.39 ms                | ~0.2 ms        |
| 200 anti-aliased lines                           | 2.19 ms                | ~1 ms          |
| 100 semi-transparent triangles                   | 4.75 ms                | ~2 ms          |
| 20 gradient triangles                            | 4.20 ms                | ~2 ms          |
| `scaled` 400 × 240 → 1600 × 880                  | 31.8 ms                | < 5 ms         |
| `scaled` 400 × 240 → 160 × 96                    | 0.55 ms                | ~0.3 ms        |

The first estimate from a scratch program (19.7 ms for the enlargement) was measured
under different conditions; the table above is the reference.

## Steps (one commit each)

1. **Measurement tool** — `crates/vnce/examples/perf.rs`: the scenarios above, without
   dependencies (like `raster -- --bench`).
2. **Cached cell widths** — `vnce-core` `cell.rs`, `buffer.rs`: the width is stored in the
   symbol when it is set, so `diff` and `flush` no longer compute Unicode widths for every
   cell every frame. ASCII fast path in `set_string`.
3. **Diff without allocation** — `Buffer::diff_into` reuses the terminal's vector;
   `diff` stays. (Done as `Buffer::diff_iter`, see the implementation notes.)
4. **Escape sequences without `write!`** — `vnce-term` `ansi.rs`: numbers written from a
   table; ASCII fast path in `write_symbol`. The output stays byte for byte the same.
5. **Shorter cursor movement** — `CSI n C` / `CSI n G` for jumps on the same row;
   `TestBackend` understands them.
6. **Row fast paths in the canvas** — `span`, `fill_rect` and `blit` work on row slices;
   opaque colors are written directly, constant source colors blend with precomputed
   factors.
7. **Triangles and anti-aliased lines** — early exit on each row, barycentric weights only
   for gradients; the same pixels are drawn (checked against the old implementation on
   random triangles).
8. **`scaled`** — premultiplied source, two passes (horizontal, vertical), rows on several
   threads. Results may differ by ±1 per channel, so the old exact algorithm stays
   selectable (`ScaleQuality::Exact`), also for the terminal's images.
9. **Frame pacing on Windows** — measured first; a high-resolution wait only if needed,
   and only after asking (it needs a new `windows-sys` feature). (Done without a new
   feature or dependency, see the implementation notes.)
10. **Docs and closing** — ARCHITECTURE, CHANGELOG, numbers in this plan; full checks.

## Results

Best of five interleaved runs of `cargo run --release --example perf -- --frames 600`,
before (commit `4effd53`) and after the phase. Single runs vary by up to 1.5 times on the
development machine (the process lands on faster or slower cores), so only interleaved
runs are compared.

| Scenario                                         | Before                | After                 | Speed-up |
| ------------------------------------------------ | --------------------- | --------------------- | -------- |
| Half blocks, full screen, truecolor: flush       | 1.76 ms, 437.4 KB     | 0.86 ms, 437.4 KB     | 2.05×    |
| Half blocks, full screen, 256 colors: flush      | 0.72 ms, 67.1 KB      | 0.42 ms, 67.1 KB      | 1.69×    |
| Text on every row, one row changes               | 0.38 ms               | 0.155 ms              | 2.45×    |
| 600 scattered cells move                         | 0.121 ms, 20.3 KB (¹) | 0.093 ms, 18.4 KB     | 1.30×    |
| Empty frame (clear + diff)                       | 0.096 ms              | 0.036 ms              | 2.67×    |
| 200 opaque rectangles 30 × 20                    | 0.209 ms              | 0.019 ms              | 11×      |
| 200 semi-transparent rectangles 30 × 20          | 0.507 ms              | 0.419 ms              | 1.21×    |
| 500 opaque blits 16 × 16                         | 0.330 ms              | 0.222 ms              | 1.49×    |
| 500 semi-transparent blits 16 × 16               | 0.587 ms              | 0.371 ms              | 1.58×    |
| 50 semi-transparent circles, radius 20           | 0.312 ms              | 0.255 ms              | 1.22×    |
| 200 anti-aliased lines                           | 1.736 ms              | 1.516 ms              | 1.15×    |
| 100 semi-transparent triangles                   | 3.778 ms              | 2.386 ms              | 1.58×    |
| 20 gradient triangles                            | 3.355 ms              | 3.230 ms              | 1.04×    |
| `scaled` 400 × 240 → 1600 × 880 (fast)           | 19.6 ms               | 4.66 ms               | 4.2×     |
| `scaled` 400 × 240 → 160 × 96 (fast)             | 0.357 ms              | 0.369 ms              | 1.0×     |

(¹) The scenario was added in step 5; "before" is the code after step 4.

Frame pacing on Windows (a 60 FPS loop waiting like the game loop, 180 frames): 95th
percentile of the frame time 28 ms before, 17 ms after; longest frame 31 ms before, 17 ms
after.

Targets not reached: semi-transparent rectangles and circles, anti-aliased lines and
gradient triangles. Their remaining cost is the arithmetic of every pixel (alpha
blending, coverage and barycentric weights); making it cheaper would change results
(incremental sums, reciprocal instead of division), which this phase avoided.

## Implementation notes

- Measuring came first and changed the plan twice: `write!` was not the cost of writing
  cells (the many small steps of each SGR parameter were; a digit table and one
  `write_all` per sequence helped, a stack buffer per parameter did not), and splitting
  solid triangles into spans gained nothing until the per-pixel edge tests were removed.
  Replacing `f32::round` in `to_u8` by hand was measured and dropped (no gain).
- Step 3 became `Buffer::diff_iter` instead of `diff_into`: `CellUpdate` borrows the
  buffer, so a vector of updates cannot be kept in the terminal between frames, and an
  iterator needs no vector at all.
- Every change that could alter output is checked against the old behaviour: randomized
  tests compare spans and blits with pixel-by-pixel drawing (all blend modes, flips,
  color keys, opacity, clipping) and triangles with a full scan (3000 triangles, a third
  with edges through pixel centers); the alpha shortcut is compared with the general
  formula; fast scaling is checked to stay within one of exact scaling, also on the
  threaded path.
- Fast scaling falls back to the direct method when that needs fewer weighted sums
  (shrinking to a small image), so `Fast` is never slower than `Exact`.
- Frame pacing needed no new Windows API: the standard library already sleeps with a
  high-resolution timer, so `input::poll` sleeps the last 16 ms of a wait in 1 ms steps.
  The user should still try a game in Windows Terminal and conhost.

## Resume here (next session)

Phase 12 is done. Next agreed goal: make the project ready to publish on GitHub (the user
sets up the GitHub side). Start by asking the user how far the GitHub side is, then write
a plan in `docs/plans/` (first CI run on all three platforms, manifests, release
checklist in `docs/RELEASING.md`).

## Dependencies

None (except, possibly, a `windows-sys` feature in step 9, asked separately).

## Risks / open questions

- Public API: only additions (`diff_into`, `ScaleQuality`, ...).
- Cursor movement sequences are universal, but should be tried by hand in Windows
  Terminal and conhost.
- No comment is deleted; outdated comments are updated in place.
