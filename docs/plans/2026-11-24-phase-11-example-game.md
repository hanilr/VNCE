# Plan: Phase 11 — Example game and tutorial

**Date:** 2026-11-24
**Status:** Approved; the game is done, the rest is planned for the next session

## Goal

A complete game that uses the whole engine (and shows what is missing), and a tutorial
that beginners can follow step by step.

## 1. Lantern (done)

An original top-down cave game (`examples/lantern.rs`): explore generated caves with a
lantern, collect every gem, reach the exit, avoid the bats. Deeper caves are bigger,
darker and have more bats; three lives; best score for the session.

Uses: cave generation (cellular automata, breadth-first distances), `TileMap` with solid
tiles, `move_and_collide` for the player and the bats, `SpatialGrid` for bat hits,
`Camera` (smoothing, dead zone, shake), sprites and `Animator`s, a lantern light done with
per-pixel darkening, `SceneStack` (title, settings, play, pause, game over), `List` menus
and volume settings, sound and music with `--features audio`.

## 2. Next session

- Engine: a seeded `Rng` in `vnce-game`, a small bitmap font in `vnce-raster`, a save
  file helper (best score and settings kept between runs).
- Tutorial in `docs/tutorial/` (compiled as doc-tests): first window, game loop and input,
  pixel graphics and display methods, sprites, tile maps / camera / collision, scenes and
  UI, sound, performance tips.
- Docs and changelog.

## Notes found while writing the game (not fixed yet, as asked)

- `Context::input()` borrows the context, so a scene that reads input and then calls a
  `&mut Context` method (for example `set_graphics_mode`) must read the input again
  afterwards. An API that returns a copy of the input changes (or a method that takes the
  graphics mode change after input handling) would be easier.
- There is no random generator in the engine; Lantern carries its own xorshift.
- Text inside the pixel world (score, titles) is not possible yet without a bitmap font;
  Lantern uses the status line and widgets instead.
- The best score is kept only while the game runs (no save file yet).
- Key rebinding is not in the settings screen yet (volumes, frame rate and display method
  are); planned with the save file helper so bindings can be stored.
- Cosmetic: the floor tile pattern repeats visibly as a grid.

## Resume here (next session)

State at the end of the first session: everything is committed on `main` (Lantern in
`crates/vnce/examples/lantern.rs`, settings with volumes, frame rate and display method).
The user tried the game: it works; half blocks run at about 110 FPS in Windows Terminal.

Order agreed with the user:

1. Fix the gaps listed above (engine `Rng`, bitmap font, save file helper with best score,
   settings and key bindings; key rebinding in the Lantern settings; the floor pattern),
   then move Lantern to them. Plan each engine addition before implementing it, as
   AGENTS.md requires.
2. Write the tutorial (`docs/tutorial/`, compiled as doc-tests).
3. Docs and changelog; close the phase with implementation notes.

Earlier findings that still stand: Sixel scrolling in Windows Terminal is limited by the
terminal itself (accepted by the user, see the Phase 10 notes); a native window backend
was discussed and declined.
