# Variation CLI Engine

`vnce` is a Rust library for building graphics, animations and games that run in the
terminal, rendered on the **CPU** or the **GPU**.

It is part of the **Variation** ecosystem.

> **Status:** early development (Phase 1: core types and terminal layer). The API is not
> stable yet.

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

Run the bundled example (press `q` to quit):

```sh
cargo run --example hello
```

Set `VNCE_COLOR_DEPTH` to `truecolor`, `256`, `16` or `none` to override color detection.

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
| `docs/ARCHITECTURE.md`          | High-level design.                        |
| `docs/plans/`                   | Approved implementation plans.            |
| `CHANGELOG.md`                  | Release history.                          |

## License and attribution

Copyright 2026 Hakkı Anıl Ragıboğlu (hanilr).

Licensed under the [Apache License, Version 2.0](LICENSE).

If you use, modify or redistribute this project, you must keep the [`NOTICE`](NOTICE) file
and credit the original author, **Hakkı Anıl Ragıboğlu (hanilr)**, as required by the
license.
