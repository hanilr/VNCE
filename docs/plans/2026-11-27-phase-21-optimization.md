# Plan: Phase 21 — Optimization

**Date:** 2026-11-27
**Status:** Planned, waiting for the user's approval to start (the user asked on
2026-11-27 to make Phase 21 an optimization phase; the earlier Phase 21 plan, documentation
and publishing, moved to [Phase 28](2026-11-27-phase-28-docs-and-tutorial.md))

## Goal

Phase 12 made the terminal output and the CPU drawing faster and Phase 15 made images with
graphics protocols cheap. Everything added since then has never been measured the same
way: the GPU renderer and its readback, lit meshes, the native window, the audio mixer and
the game toolkit of Phase 19 (particles, tweens, path finding, the new widgets). This phase
measures the whole engine again and makes the slow parts faster without changing what is
drawn or heard. Every step is measured before and after, and the numbers are recorded
here, as in Phase 12.

## What it covers

1. **Measurement first:** the `perf` example (`crates/vnce/examples/perf.rs`) gains
   scenarios for the parts that have none yet:
   - GPU: render target readback, `ShaderQuad`, `MeshRenderer` with many meshes, showing a
     GPU picture in the window without a readback;
   - window: cells and images drawn per frame, partial uploads (Phase 20);
   - game toolkit: particles with many particles, A* on large maps, tweens, `ActionMap`
     and `InputState` per update;
   - widgets: `Table`, `List` and `Paragraph` with many rows;
   - audio: mixing many voices, decoding OGG / WAV.
   The showcase benches (`relay -- --bench`, and the same for Isle and Glide where they
   exist) are run too, on the CPU and the GPU. A baseline table goes into this plan.
2. **Optimizations chosen from the measurements:** only the parts that turn out slow are
   changed, the largest costs first. Candidates, to be confirmed by the numbers:
   allocations per frame (reused buffers), copies of whole canvases or vertex data,
   readback waits on the GPU, per-voice work in the mixer, the open list and the maps of
   A*, per-particle work.
3. **The earlier fast paths checked again:** the Phase 12 and Phase 15 scenarios are run
   to see that nothing became slower since then (no regressions).

## Steps (one commit each)

1. Plan details after the first measurements: the baseline table, the chosen targets and
   their order (presented to the user before any optimization starts).
2. New `perf` scenarios (GPU, window, toolkit, widgets, audio) and the baseline.
3. One commit per optimization, each with its numbers before and after; the output (cells,
   pixels, samples) stays the same, checked by the existing tests and, where needed, by a
   comparison with the old implementation.
4. Regression check of the Phase 12 and Phase 15 scenarios and the showcase benches.
5. Docs and closing: numbers in this plan, ARCHITECTURE, CHANGELOG; full checks
   (`cargo build`, `cargo test`, `cargo clippy`, `cargo fmt --check`, also with
   `--no-default-features`).

## Dependencies

None planned. If a measurement shows that a crate would help (for example a faster hash
map), it is proposed to the user first, with the numbers.

## Risks / open questions

- The list of optimizations is only known after step 1; the user approves it then.
- GPU timings vary from run to run (and between the RTX GPU, integrated GPUs and the
  software adapter in CI); they are compared as the best of several interleaved runs.
- Window scenarios need a real window and GPU, so they run locally, not in CI.
- Optimizations may not change the public API; if one would, it is asked first.
- MSRV stays 1.87.
