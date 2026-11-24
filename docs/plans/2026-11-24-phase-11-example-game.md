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
