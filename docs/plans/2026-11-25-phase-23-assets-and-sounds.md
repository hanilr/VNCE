# Plan: Phase 23 — Assets and sounds

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (the user asked on
2026-11-25 for support for adding assets and sounds to the library)

## Goal

Today a game loads every file itself, by path: `Image::load_png`, `Sound::load`,
`TileMap` from text. The examples carry their art in code or next to the binary. A
game should instead name its assets once, load them the same way in the terminal and
in a window, ship them inside the binary or in one pack file, and see changes while
developing without restarting.

## What it adds

1. **An asset store** (`vnce_game::assets` or a small `vnce-assets` crate, decided in
   step 1): assets by name (`assets.image("hero")`, `assets.sound("jump")`,
   `assets.text("level-1")`), loaded once and shared through handles; a typed error
   that says which file failed and why.
2. **Sources:** a folder on disk (development), files embedded in the binary at build
   time (one executable to ship), and a single pack file next to the binary; a game
   switches between them without changing its code.
3. **Hot reload** (debug builds): changed files on disk are reloaded, and the game is
   told which assets changed, so art and sounds can be tuned while the game runs.
4. **Sounds and music by name:** `vnce_audio` plays sounds from the store (WAV, OGG),
   music streams from it, and sound settings (volume groups) stay as they are.
5. **More image formats** where they help games (for example QOI or JPEG for large
   backgrounds), decided in step 1.
6. **The examples** move their files onto the store; one example shows hot reload.

## Steps (one commit each)

1. Plan details: the crate, the names, the pack format, the formats to add.
2. The store with the folder source; images, sounds, text; tests.
3. Embedded and pack sources; a tool (or `build.rs` helper) that writes a pack.
4. Hot reload in debug builds.
5. `vnce_audio` and the store; the examples move onto it.
6. Tutorial chapter or section, shipping guide (assets in a release), changelog,
   closing.

## Dependencies (to be approved)

- Possibly `notify` (CC0 / MIT / Apache-2.0) to watch files for hot reload, only in
  debug or behind a feature.
- Possibly an image decoder for the formats chosen in step 1.

## Risks / open questions

- Embedding large assets makes builds slower; the pack file is the answer for big
  games.
- Hot reload must not break a running game when a file is half written; a reload that
  fails keeps the old asset and reports the error.
