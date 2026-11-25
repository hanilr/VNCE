# Plan: Phase 14 — Showcase game "Relay"

**Date:** 2026-11-25
**Status:** Done

## Goal

One original game that shows everything the engine can do, as part of play rather than
as a list of demos, and runs as well as the engine allows. A derelict signal station:
the player restores the relay by collecting the power cores of its five wings; every
wing shows one part of the engine.

## Rooms

| Room            | In the game                                        | Shows                                                    |
| --------------- | -------------------------------------------------- | -------------------------------------------------------- |
| Atrium (hub)    | Dark station lit by the player's lamp, doors       | Tile map, `Camera`, light with `Multiply`, `Timer`/easing |
| Observatory     | A window onto a nebula                             | GPU `ShaderQuad`; CPU plasma without a GPU               |
| Reactor         | A spinning 3D core                                 | GPU `MeshRenderer` (depth, MSAA); CPU wireframe fallback |
| Prism Hall      | A wall screen                                      | The same picture in every pixel mode; protocol vs cells  |
| Sound Lab       | Synth pads on the floor, a waveform screen         | `Synth`, `Preset`, `Melody`, mixer groups; silent without a device |
| Archive         | A terminal that opens an interface                 | `Layout`, `Paragraph` (Turkish, CJK, emoji, wrapping), `List`, `Gauge`, `Block` |

Gameplay: patrolling drones (`SpatialGrid`, `move_and_collide`), animated sprites, sparks
(`Add` blending), score and time. Scenes: title (big pixel font over a live GPU or CPU
scene), play, pause popup, settings, controls, end screen. Settings: volumes, frame rate,
display method (every pixel mode and protocol), scale quality, key bindings; kept with
the best time in a `SaveFile`. Mouse support. `F` toggles a performance panel (FPS, frame
time, the drawing path in use).

## Quality and speed

- The world is drawn at the exact pixel size it is shown at (no extra scaling).
- Static tiles are cached per room; GPU readback is pipelined.
- `--bench` runs the game without a screen and prints frame times; target: 60 FPS with
  plenty of room at 200 × 60 cells.
- A smoke test with `ScriptedDriver` plays from the title into the game.

## Structure

`crates/vnce/examples/relay/` split into modules (`main`, `world`, `actors`, `visuals`,
`audio`, `ui`). `cargo run --release --example relay`; `--features gpu,audio` turns on
the GPU scenes and sound, otherwise CPU fallbacks and silence are used and shown on
screen.

## Commits

1. Plan. 2. Skeleton and world. 3. Player and drones. 4. GPU / CPU visuals. 5. Prism
Hall. 6. Sound. 7. Interface, menus, save file. 8. Bench, test and polish. 9. Docs.

## Results

`cargo run --release --example relay --features gpu,audio -- --bench` (200 × 60 cells,
600 frames walking through the station, development machine with an RTX 5070 Laptop GPU):

| Display method | Per frame | Of it in `render` | Output per frame |
| -------------- | --------- | ----------------- | ---------------- |
| Half blocks    | 1.05 ms   | 0.61 ms           | 158 KB           |
| Sextants       | 2.62 ms   | 2.60 ms           | 155 KB           |
| Braille        | 3.11 ms   | 2.38 ms           | 135 KB           |
| Kitty          | 17.6 ms   | 9.6 ms            | 145 KB           |
| Sixel          | 28.6 ms   | 10.2 ms           | 161 KB           |

With `--cpu` the cell modes are about as fast (the GPU pictures are small). With a
graphics protocol the shown image is about 2000 × 1160 pixels; most of `render` is then
spent inside `Frame::draw_image`, which hashes and copies the whole image every frame
(about 9 MB). That is an engine cost, noted for a later, separately approved change.
(Done in Phase 15: render is now about 1 ms with Kitty and Sixel; see
`2026-11-25-phase-15-real-pixel-speed.md`.)

## Implementation notes

- The game is one example in nine modules (`main`, `world`, `art`, `visuals`, `audio`,
  `play`, `scenes`, `showrooms`, `bench`), about 4400 lines with tests. It was written as
  a whole and committed as one addition, not in the seven steps listed above.
- No engine change was needed. The scale quality is a terminal setting that `Context`
  does not expose, so instead of a settings row the prism gallery shows fast and exact
  scaling side by side with live timings.
- Light: a light layer is cleared to the ambient color, light sprites (cached by radius
  and rounded color) are added with `BlendMode::Add`, and the layer multiplies the scene
  (`BlendMode::Multiply`); windows, screens, sparks and drone eyes are drawn after it, so
  they glow. The map is drawn once into a canvas; every frame copies its visible part.
- Galleries that mix cells and images (prism, archive) are full-screen scenes, because
  Sixel and iTerm2 images hide the cells under them.
- `draw_world` takes a small `FrameInfo` instead of the `Context`, so a test can draw every
  room into PNG files (`RELAY_SNAPSHOTS=<folder>`, with `RELAY_GPU=1` for the GPU); the
  pictures were used to tune the lighting and the room banner.
- The reachability test found that the sound lab's waveform screen closed the corridor
  into the room; the screen moved to the south wall.

## Resume here (next session)

Phase 14 is done. Next, to be agreed with the user: the engine cost of `draw_image` with
graphics protocols (hashing and copying the whole image every frame), crates.io 0.1.0, or
a new feature phase.

## Dependencies

None.

## Risks / open questions

- Engine gaps (for example sprite rotation or particles): planned and approved
  separately before the engine changes, as with Lantern.
- Size: about 2500–3500 lines.
