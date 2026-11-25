# Plan: Phase 11 — Example game and tutorial

**Date:** 2026-11-24
**Status:** Done

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
  **Done:** the request methods of `Context` take `&self` (the values live in `Cell`s).
- There is no random generator in the engine; Lantern carries its own xorshift.
  **Done:** `vnce_game::Rng` (Lantern and `stars` use it).
- Text inside the pixel world (score, titles) is not possible yet without a bitmap font;
  Lantern uses the status line and widgets instead.
  **Done:** `vnce_raster::Font` (5×7, ASCII and Turkish letters); Lantern draws a pixel
  title and a heads-up display.
- The best score is kept only while the game runs (no save file yet).
  **Done:** `vnce_game::SaveFile`; Lantern saves the best score, settings and controls.
- Key rebinding is not in the settings screen yet (volumes, frame rate and display method
  are); planned with the save file helper so bindings can be stored.
  **Done:** Settings → Controls (Esc cancels, so it cannot be bound there).
- Cosmetic: the floor tile pattern repeats visibly as a grid.
  **Done:** four floor variants placed by position.

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

## Progress (second session, 2026-11-25)

Step 1 of the agreed order is done, one commit per piece: `Rng`, `Font`, key and binding
names (`KeyCode` / `Binding` implement `Display` and `FromStr`), `SaveFile` with
`data_dir`, Lantern saving, the controls screen and the floor variants. The `Context::input()`
borrow is fixed too. Next: step 2, the tutorial.

Step 2 is done: `docs/tutorial/` has an index and eight chapters, compiled as doc-tests
through `crates/vnce/src/lib.rs` (chapter 7 only with the `audio` feature). Next: step 3,
docs, changelog and closing the phase.

## Implementation notes

- Lantern came first, written only against the public API; everything it had to work
  around became the list of gaps above, fixed in the second session one commit each.
- `Rng` keeps Lantern's xorshift64* and seed mixing; `below` switched from a modulo to a
  multiply-high (even spread, no division), so the same seed now gives different caves
  than before, which does not matter since Lantern seeds from the time. `vnce-audio`
  keeps its own small generator: it does not depend on `vnce-game`.
- `Font`: the 5×7 glyphs were drawn for the engine, not copied from an existing font, and
  checked by printing the table. Turkish capitals with marks above are one or two rows
  shorter to fit the cell. Unknown characters draw as a box.
- `SaveFile` needs no dependency: no serde, and the data folder is found from
  `APPDATA`, `HOME` and `XDG_DATA_HOME` (only absolute paths are trusted). Lantern saves
  when the settings or controls close, when a game ends and when the player quits.
- Controls screen: the key is read from `Scene::event`; one update is skipped after a key
  is bound so the same press does not also act. Esc cancels, so it cannot be bound there
  (it can be in the save file).
- Floor: four variants chosen per cell from an `Rng` seeded with the position, so the
  pattern is stable across frames.
- `Context` requests take `&self` through `Cell`s instead of returning a copy of the input
  (cloning the `InputState` every update would allocate). `Context` is no longer `Sync`;
  it was never shared between threads.
- Let-chains (`if let … && let …`) were avoided: they need Rust 1.88 and the MSRV is 1.87.
- The tutorial is compiled as doc-tests through `#[cfg(doctest)]` modules in
  `crates/vnce/src/lib.rs` that include the Markdown files; chapter 7 only with `audio`.
  A wrong camera offset in a first draft was caught this way and replaced by the measured
  value.
- The roadmap in `ARCHITECTURE.md` had lost its Phase 9 entry in an earlier docs commit;
  it was restored.
- Not seen on a screen in the second session (no interactive terminal): the pixel title,
  the heads-up display, the controls screen and the new floor. They build and their logic
  is covered by the engine tests; a look in a real terminal is still useful.
