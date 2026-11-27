# Plan: Phase 28 — Documentation, the tutorial and a front page

**Date:** 2026-11-27
**Status:** Planned, waiting for the user's approval to start (the user asked on
2026-11-27 to make the documentation and tutorial work the last phase; it takes over
Phase 21, whose plan stays in
[`2026-11-25-phase-21-docs-and-publishing.md`](2026-11-25-phase-21-docs-and-publishing.md))

## Goal

The tutorial teaches a 2D game in the terminal well (chapters 01–08), but the newest
parts of the engine are missing from it: GPU rendering, lit 3D meshes, the native window,
gamepads and isometric drawing are only mentioned or shown in examples. Phases 22–27 add
more (every language, assets, mobile, high-resolution audio, the default icon, the
mouse). Documenting each of them while the API still moves would mean rewriting chapters
again, so this phase comes last: once the features are in, the tutorial and the
documentation are brought up to date in one pass.

## What the documentation has today

- `docs/tutorial/` 01–08: first window, game loop and input, pixel graphics, sprites and
  animation, tile maps / camera / collision / path finding, scenes / UI / saving, sound,
  performance. Every code block is tested (`cargo test --doc -p vnce`).
- `docs/SHIPPING-GAMES.md` (the window and releases), `docs/ARCHITECTURE.md`,
  `CHANGELOG.md`, `README.md`, rustdoc on every crate.
- Gaps: GPU is one paragraph in chapter 08; `ShaderQuad`, `RenderTarget`, `MeshRenderer`,
  `Vec3` / `Mat4`, light, fog and 3D cameras are not taught; the window is a link to the
  shipping guide; gamepads are one paragraph (no connect / disconnect events, sticks for
  movement or rumble); isometric drawing is only named.

## What it adds

1. **New tutorial chapters** (compiled and tested like the others; code that opens a
   window or needs a GPU is `no_run`):
   - 09 GPU: render targets, `ShaderQuad`, your own `wgpu` passes, readback and showing
     GPU pictures in the window without a readback.
   - 10 3D: `Vec3` / `Mat4`, meshes, cameras, sun and ambient light, fog (the ideas of
     Glide).
   - 11 A game in a window: the `window` feature, `vnce::run`, `WindowOptions`,
     fullscreen, vsync, sharp or smooth scaling, the icon (Phase 26), shipping.
   - 12 Gamepads and the mouse: pad events, sticks and dead zones, bindings, rumble;
     the mouse in pixels, picture coordinates, drags and mouse look (Phase 27).
   - Further chapters for what Phases 22–25 add (text in every language, assets and
     sounds from files, mobile, high-resolution audio), decided when those phases are
     done.
2. **Existing chapters brought up to date:** isometric drawing (the ideas of Isle) in
   chapter 04 or 05, and every chapter checked against the API of that time.
3. **The other documents:** `README.md`, `docs/SHIPPING-GAMES.md`,
   `docs/ARCHITECTURE.md`, the crate-level rustdoc and the examples' comments checked
   against the code; a README per crate for crates.io if still wanted.
4. **The flaky test** carried over from Phase 21: the `vnce-gpu` `target` test once
   crashed with SIGSEGV on the Ubuntu CI (lavapipe software adapter); find the cause or
   isolate the test.
5. **A front page:** what vnce is, pictures of the showcases (window screenshots with
   `VNCE_WINDOW_SCREENSHOT`), how to start, links to the tutorial, the examples and the
   shipping guide. The source of truth stays in the repository; the front page is a
   wiki, GitHub Pages or a `docs/` page (the repository is public now, its wiki is off).

## Steps (one commit each)

1. Plan details: the final list of chapters (after Phases 22–27), the front page's place.
2. Tutorial chapter 09 (GPU).
3. Tutorial chapter 10 (3D).
4. Tutorial chapter 11 (window and shipping).
5. Tutorial chapter 12 (gamepads and the mouse).
6. Chapters for Phases 22–25, one commit each.
7. Existing chapters updated (isometric drawing, API changes).
8. README, shipping guide, architecture, rustdoc and example comments checked.
9. The flaky GPU test.
10. Front page, changelog, closing.

## Dependencies

None.

## Risks / open questions

- The chapters depend on the phases before this one; a phase that is skipped or
  changed changes the list of chapters (step 1 settles it).
- Turning on the wiki or GitHub Pages is the user's decision (outward-facing); until
  then the front page can be a `docs/` page.
- Tutorial code that opens a window or needs a GPU cannot run in CI; such blocks are
  `no_run`, so their correctness depends on the examples they are taken from.
