# Plan: Phase 12 — Optimization

**Date:** 2026-11-25
**Status:** Approved

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
   `diff` stays.
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
   and only after asking (it needs a new `windows-sys` feature).
10. **Docs and closing** — ARCHITECTURE, CHANGELOG, numbers in this plan; full checks.

## Dependencies

None (except, possibly, a `windows-sys` feature in step 9, asked separately).

## Risks / open questions

- Public API: only additions (`diff_into`, `ScaleQuality`, ...).
- Cursor movement sequences are universal, but should be tried by hand in Windows
  Terminal and conhost.
- No comment is deleted; outdated comments are updated in place.
