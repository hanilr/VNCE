# Plan: Phase 7 — CI and release readiness

**Date:** 2026-11-24
**Status:** Approved

## Goal

Test every change on Windows, Linux and macOS automatically, make the crates ready for
crates.io, and show that the code reading external data does not crash on bad input.
Nothing is published in this phase, and no remote repository is set up yet: the CI
workflow is prepared for later.

## Findings

- `cargo package` works for every crate (17–38 KB), but the packages contain neither
  `LICENSE` nor `NOTICE`: the attribution required by `NOTICE` would not reach users.
- The manifests have no `repository`, `homepage` or `documentation`.
- All seven crate names are free on crates.io.

## 1. CI (`.github/workflows/ci.yml`)

- Matrix Windows / Linux / macOS: fmt, clippy (`--all-targets --all-features -D warnings`),
  tests, docs (`-D warnings`), and a build with `--no-default-features`.
- MSRV job: Rust 1.87.
- GPU tests run on Linux with Mesa's software Vulkan driver (lavapipe), installed on the CI
  machine only.
- `cargo-deny`: license compatibility, security advisories, duplicate dependencies.
- Build cache (`Swatinem/rust-cache`).
- A check that the per-crate copies of `LICENSE` / `NOTICE` match the root files.

## 2. Release readiness

- `LICENSE` and `NOTICE` copied into every crate directory (copies, not symlinks, which
  are unreliable on Windows).
- `repository` / `homepage` (`https://github.com/hanilr/VNCE`) and
  `documentation` in every manifest; docs.rs builds with all features.
- `docs/RELEASING.md`: publishing order (core → raster → term → runtime → gpu → ui →
  cli-engine), versioning and a checklist.
- CHANGELOG sections fixed.

## 3. Robustness tests (no new dependencies)

A small deterministic random generator in the tests drives thousands of random inputs
through the terminal answer parser, the Sixel / Kitty / iTerm2 encoders, `Buffer` writing
and diffing, `Layout` and `Paragraph` wrapping. Invariants: no panics, nothing outside its
area, consistent lengths.

## 4. Docs

Supported platforms in the README; ARCHITECTURE updated.

## Commits

1. `Add Phase 7 plan: CI and release readiness`
2. `Add package metadata and license files to every crate`
3. `Add randomized robustness tests`
4. `Add CI workflow with cargo-deny`
5. `Fix changelog sections and add release guide`

## Risks / notes

- The workflow cannot run until the repository is pushed; the first run may need small
  fixes.
- GitHub's macOS runners may have no Metal adapter; GPU tests then skip.
- Randomized tests are not a replacement for coverage-guided fuzzing.
