# Plan: Phase 13 — Ready to publish on GitHub

**Date:** 2026-11-25
**Status:** Approved

## Goal

Put the project on GitHub (`https://github.com/hanilr/VNCE`, private for now), get CI green
on Windows, Linux and macOS for the first time, and leave the repository ready to be made
public. Nothing is published on crates.io in this phase.

## Decisions (with the user)

- Before the first push the history was rewritten (nothing had been pushed yet):
  - every old name became `vnce`: `variation-cli-engine` → `vnce`, `variation-*` →
    `vnce-*` (directories included), `vn_*` → `vnce_*`, `VN_*` → `VNCE_*`, `Vn*` →
    `Vnce*`; the repository URL became `https://github.com/hanilr/VNCE`. "Variation"
    (the ecosystem) and "Variation CLI Engine" (what `vnce` stands for) stay;
  - texts that only described the old names (two changelog entries, the rename section
    of the Phase 8 plan) were removed, and the two rename commits got messages that
    describe what they still change;
  - the author's e-mail address was replaced by the GitHub `noreply` address in every
    commit and in `Cargo.toml`, so it is never published. The repository's local git
    configuration uses the `noreply` address for new commits.
- The GitHub CLI (`gh`) is installed to follow CI runs.
- The CI matrix stays as it is (macOS minutes cost more on private repositories; accepted).
- No extra community files for now (contributing guide, issue templates, Dependabot).

## Steps

1. This plan.
2. Add the `origin` remote and push `main`.
3. First CI run: fix every failing job, one commit per cause.
4. `docs/RELEASING.md`: GitHub steps (green CI, push the tag, GitHub release).
5. README: CI badge; a checklist for making the repository public.
6. Close the phase: changelog, implementation notes.

## Dependencies

None.

## Risks / open questions

- The code has only been built on Windows so far; Linux and macOS may show warnings or
  test failures (CI treats warnings as errors).
- The MSRV job (Rust 1.87) may find newer language or library features.
- `cargo-deny` may report licenses or advisories that need a decision.
