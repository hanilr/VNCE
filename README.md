# Variation CLI Engine

`vnce` is a Rust library for building graphics, animations and games that run in the
terminal, rendered on the **CPU** or the **GPU**.

It is part of the **Variation** ecosystem.

> **Status:** early development. There is no usable public API yet.

## Usage

```toml
[dependencies]
vnce = "0.1"
```

```rust
use vnce as _;
```

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
| `docs/ARCHITECTURE.md`          | High-level design.                        |
| `docs/plans/`                   | Approved implementation plans.            |
| `CHANGELOG.md`                  | Release history.                          |

## License and attribution

Copyright 2026 Hakkı Anıl Ragıboğlu (hanilr).

Licensed under the [Apache License, Version 2.0](LICENSE).

If you use, modify or redistribute this project, you must keep the [`NOTICE`](NOTICE) file
and credit the original author, **Hakkı Anıl Ragıboğlu (hanilr)**, as required by the
license.
