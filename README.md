# Variation CLI Engine

`vnce` is a Rust library for building graphics, animations and games that run in the
terminal, rendered on the **CPU** or the **GPU**.

It is part of the **Variation** ecosystem.

> **Status:** early development (Phase 3: core types, terminal layer, game loop and pixel
> graphics). The API is not stable yet.

## Usage

```toml
[dependencies]
vnce = "0.1"
```

```rust,no_run
use vnce::vnce_core::{Color, Style};
use vnce::vnce_term::Terminal;

fn main() -> std::io::Result<()> {
    let mut terminal = Terminal::enter()?;
    terminal.draw(|frame| {
        frame
            .buffer_mut()
            .set_string(0, 0, "Hello!", Style::new().fg(Color::Green).bold());
    })?;
    Ok(())
}
```

For games, implement `vnce_runtime::Game` and call `vnce_runtime::run`: updates run at a fixed
rate, frames at a configurable limit, and `ctx.input()` tells which keys are held down.

For pixel graphics, draw on a `vnce_raster::Canvas` (lines, shapes, triangles, sprites, PNG
images) and convert it into cells with `canvas.render(buffer, area, mode, &options)`.
`PixelMode::detect` picks a mode that works on the current terminal.

Run the bundled examples (press `q` or `Esc` to quit):

```sh
cargo run --example hello             # styles, colors, Unicode, input events
cargo run --example stars             # a small game: arrows/WASD, +/- to change the FPS
cargo run --release --example raster  # pixel graphics; M: pixel mode, D: dithering
cargo run --release --example raster -- image.png   # show a PNG image
```

Environment overrides:

- `VNCE_COLOR_DEPTH`: `truecolor`, `256`, `16` or `none`.
- `VNCE_PIXEL_MODE`: `halfblock`, `quadrant`, `sextant`, `octant`, `braille` or `ascii`.

## Building

Requires Rust 1.85 or newer.

```sh
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --all --check
```

## Project layout

| Path                            | Contents                                  |
| ------------------------------- | ----------------------------------------- |
| `crates/vnce/`  | The main library crate (`vnce`).     |
| `crates/vnce-core/`        | Core types (`vnce_core`).                   |
| `crates/vnce-term/`        | Terminal layer (`vnce_term`).               |
| `crates/vnce-runtime/`     | Game loop and input state (`vnce_runtime`). |
| `crates/vnce-raster/`      | Pixel canvas and pixel modes (`vnce_raster`). |
| `tools/gen-block-tables/`       | Generates block-character tables.         |
| `docs/ARCHITECTURE.md`          | High-level design.                        |
| `docs/plans/`                   | Approved implementation plans.            |
| `CHANGELOG.md`                  | Release history.                          |

## License and attribution

Copyright 2026 Hakkı Anıl Ragıboğlu (hanilr).

Licensed under the [Apache License, Version 2.0](LICENSE).

If you use, modify or redistribute this project, you must keep the [`NOTICE`](NOTICE) file
and credit the original author, **Hakkı Anıl Ragıboğlu (hanilr)**, as required by the
license.
