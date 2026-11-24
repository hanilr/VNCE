# Plan: Project scaffold and initial commit

**Date:** 2026-11-24
**Status:** Approved

## Goal

Set up a Rust project that is ready to grow, with documentation and tooling in place from
the start, then commit it as `Initial commit`.

## Decisions

- **Cargo workspace** with crates under `crates/`. Starts with a single crate,
  `vnce`, imported as `vnce`.
- **Edition 2024**, minimum supported Rust version 1.85.
- **License:** Apache-2.0 with a `NOTICE` file, so redistributors must keep the author
  attribution.
- **Default branch:** `main`.
- Shared lints in `[workspace.lints]`: `missing_docs` (warn), `unsafe_op_in_unsafe_fn`
  (deny), `clippy::pedantic` (warn).
- No dependencies yet; terminal and GPU libraries will be chosen in separate plans.
- Examples will live in `crates/<crate>/examples/` so Cargo discovers them automatically.

## Files

- `Cargo.toml` (workspace), `crates/vnce/{Cargo.toml,src/lib.rs}`
- `README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE`
- `docs/ARCHITECTURE.md`, `docs/plans/`
- `rust-toolchain.toml`, `rustfmt.toml`, `.editorconfig`, `.gitattributes`, `.gitignore`

## Verification

`cargo build`, `cargo test`, `cargo clippy`, `cargo fmt --check` must all pass before the
commit.
