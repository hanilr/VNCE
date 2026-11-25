# Plan: Phase 21 — Documentation, publishing and a wiki front page

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (proposed after a review of
what the library still lacks; the user asked for the plan on 2026-11-25)

## Goal

The newest and most valuable parts (GPU rendering, lit 3D meshes, the native window and
shipping a game) are not in the tutorial yet, the crates were never published, and a flaky
GPU test on Linux was never looked into. This phase makes the library ready for other
people: documented, published (when the user decides), and with a front page.

## What it adds

1. **Tutorial chapters** (compiled and tested like the others): 09 GPU (render targets,
   `ShaderQuad`, readback modes), 10 3D (meshes, light, fog, cameras), 11 a game in a
   window (the `window` feature, `vnce::run`, shipping).
2. **The flaky test:** the `vnce-gpu` `target` test once crashed with SIGSEGV on the
   Ubuntu CI (lavapipe software adapter); find the cause or isolate the test.
3. **Ready for crates.io:** check every crate's metadata, `include` lists, docs.rs
   settings (features to document), README per crate; a dry run of publishing in the
   right order (`cargo publish --dry-run`). Publishing itself only when the user asks.
4. **A front page:** the repository is private and its wiki is off (GitHub's free plan
   offers wikis only for public repositories). The source of truth stays in the
   repository (tested tutorial, `docs/`); a wiki or GitHub Pages page is only a front
   page: what vnce is, pictures of the showcases (window screenshots), how to start,
   links to the tutorial, the examples and the shipping guide. Made when the repository
   becomes public, or as a `docs/` page until then.

## Steps (one commit each)

1. Plan details: chapter contents, the front page's place (wiki, Pages or `docs/`).
2. Tutorial chapter 09 (GPU).
3. Tutorial chapter 10 (3D).
4. Tutorial chapter 11 (window and shipping).
5. The flaky GPU test.
6. Publishing readiness and dry runs.
7. Front page (screenshots with `VNCE_WINDOW_SCREENSHOT`), changelog, closing.

## Dependencies

None.

## Risks / open questions

- Making the repository public and publishing to crates.io are the user's decisions
  (outward-facing); this phase only prepares them.
- Tutorial code that opens a window cannot run in CI; such blocks are `no_run`.
