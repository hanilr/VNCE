# Plan: Phase 26 — The VNCE logo as the default icon

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (the user asked on
2026-11-25 that every build made with vnce gets the logo as its default icon)

## Goal

Every game built with vnce shows the VNCE logo (`assets/logo/`, see
`2026-11-25-logo.md`) as its icon unless the game sets its own.

## What it adds

1. **Window and taskbar icon:** `WindowOptions::icon` (`vnce-window`) falls back to the
   VNCE logo when it is `None`; a game sets its own icon as today. The PNGs are embedded
   in the crate (a copy inside `crates/vnce-window`, because a published crate cannot
   read files outside its own folder; CI checks that the copy matches `assets/logo`,
   like the NOTICE copies).
2. **The file icon on Windows:** a library cannot change the icon of the game's `.exe`;
   that needs a resource compiled by the game's own `build.rs`. vnce offers a small
   helper (for example `vnce::build::windows_icon()`, or a documented `build.rs` snippet)
   that embeds the VNCE logo `.ico`, or the game's own, in one line. The tool from the
   logo plan also writes `vnce-logo.ico`.
3. **macOS and Linux:** the window icon from item 1; app bundles (`.icns`) and desktop
   files (`.desktop` + PNG) are documented in `docs/SHIPPING-GAMES.md`, with the logo
   files ready to use.
4. **Terminal:** a terminal program has no icon of its own; this is noted in the docs.

## Steps (one commit each)

1. Refine this plan (which helper for item 2, and whether it needs a build dependency).
2. The default window icon and the CI copy check.
3. The `.ico` output and the Windows helper.
4. Docs: `docs/SHIPPING-GAMES.md`, the tutorial window chapter, the README.

## Dependencies

Possibly a build-time crate for Windows resources (for example `winresource` or
`embed-resource`), only for games that opt in; to be approved by the user.

## Risks / open questions

- Should games that do not want the VNCE icon opt out with a flag, or only by setting
  their own icon?
- Embedding PNGs adds a few kilobytes to every windowed build.
