# Plan: Phase 13 — Ready to publish on GitHub

**Date:** 2026-11-25
**Status:** Done (the repository stays private until the user makes it public)

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

## Implementation notes

- The history was rewritten in a scratch clone with `git filter-branch` (tree, message
  and author filters, one shared rule list) and checked before it replaced `main`: no old
  name or private address left in any commit or message, the same 91 commits, and the
  new tip passing fmt, clippy and every test. A bundle of the old history was kept
  outside the repository until the push succeeded.
- `\b` in the rename rules was first written as a backspace character by an editing
  script, so some rules silently matched nothing; the rule file was rewritten whole and
  every rule tested on sample text before the real run.
- The first CI run failed on three things, each fixed in its own commit: the MSRV build
  (Rust 1.87 inferred `str` for an argument in the core robustness test), the new
  `precise_wait` timing test on macOS (CI machines oversleep; the test now checks what is
  asked for, not how long sleeping takes) and the tutorial doc-tests without default
  features (compiled only with `game` and `ui` now). The second run passed on every job.
- The GitHub CLI was installed with `winget` to follow CI runs (`gh run list`,
  `gh run view --log-failed`).

## Resume here (next session)

Phase 13 is done and CI is green. The repository is still private. Possible next steps,
to be agreed with the user: make the repository public (checklist in
`docs/RELEASING.md`), publish 0.1.0 on crates.io, or a new feature phase.

## Dependencies

None.

## Risks / open questions

- The code has only been built on Windows so far; Linux and macOS may show warnings or
  test failures (CI treats warnings as errors).
- The MSRV job (Rust 1.87) may find newer language or library features.
- `cargo-deny` may report licenses or advisories that need a decision.
