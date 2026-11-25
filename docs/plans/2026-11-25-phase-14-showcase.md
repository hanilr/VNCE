# Plan: Phase 14 — Showcase game "Relay"

**Date:** 2026-11-25
**Status:** Approved

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

## Dependencies

None.

## Risks / open questions

- Engine gaps (for example sprite rotation or particles): planned and approved
  separately before the engine changes, as with Lantern.
- Size: about 2500–3500 lines.
