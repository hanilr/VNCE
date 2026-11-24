# Architecture

> Status: draft. Nothing below is implemented yet; this document records the intended
> direction and will be updated as design decisions are made and approved.

## Goal

`vnce` lets developers build graphics, animations and games that run inside a
terminal. Frames can be produced on the **CPU** or on the **GPU**, then converted into
terminal output (characters, colors, block/braille glyphs, or terminal graphics
protocols where supported).

## Intended high-level components

| Component        | Responsibility                                                      |
| ---------------- | ------------------------------------------------------------------- |
| Core             | Shared types: colors, pixels, frame buffers, geometry, time.        |
| CPU renderer     | Software rasterization into a frame buffer.                         |
| GPU renderer     | Hardware-accelerated rendering into a frame buffer.                 |
| Terminal output  | Converting frame buffers into efficient terminal output.            |
| Input            | Keyboard / mouse / resize events from the terminal.                 |
| Runtime          | Game loop, timing, frame pacing.                                    |

Each component may become its own crate under `crates/` as it grows. The
`vnce` crate (`vnce`) is the public entry point.

## Open questions

- Which terminal backend to use (e.g. `crossterm`)?
- Which GPU API to use (e.g. `wgpu`)?
- How GPU-rendered frames are read back and mapped to terminal cells.
- Which terminal graphics protocols to support (Sixel, Kitty, iTerm2).

These will be decided in separate, approved plans stored in [`docs/plans/`](plans/).
