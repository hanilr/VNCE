# Plan: Phase 16 — Isometric demo "Isle"

**Date:** 2026-11-25
**Status:** Proposed (the user chose an isometric game over a raycaster; the whole plan
of Phases 15–17 still needs approval)

## Goal

An original 2.5D demo game drawn entirely on the CPU, in an isometric view. A small
island, generated anew every game, with several height levels: collect the light
crystals scattered over it and light the beacon before night falls.

## What it shows

- Isometric projection of a height map: levels, stairs and ramps, cliff sides.
- Depth sorting: trees, rocks, crystals and the player drawn in the right order, also
  behind and in front of higher tiles; soft drop shadows.
- Animated water around the island, waves on the shore.
- Light over time: dusk turning into night (a tint over the scene), the player's lantern
  and glowing crystals lighting their surroundings (the light layer of Relay).
- Mouse: click a tile to walk there; the path is found with A* (written in the example,
  not added to the engine). Arrow keys / WASD move directly.
- Title, pause, settings (volumes, frame rate, display method), controls and a save file
  with the best time, like Relay; sound with the `audio` feature.

## Structure

`crates/vnce/examples/isle/` in a few modules (island generation, projection and depth
sorting, actors and path finding, scenes). `cargo run --release --example isle`.

## Steps (one commit each)

1. Plan.
2. Island generation and the isometric renderer (levels, sides, water).
3. Player, keyboard movement, collision with levels, depth sorting.
4. Mouse click-to-walk with A*.
5. Crystals, beacon, day / night and light, sound.
6. Scenes, settings, save file.
7. `--bench`, smoke test, snapshots; docs and closing.

## Quality and speed

- Static terrain drawn once into a cached picture per camera position band; only actors,
  water and light are drawn every frame.
- Target: under 2 ms per frame in half blocks at 200 × 60 cells; real-pixel modes benefit
  from Phase 15.

## Dependencies

None.

## Risks / open questions

- Depth sorting of tall objects across tile boundaries needs care (sort by the tile
  under the object's base, then by height).
- Engine gaps found while building it are planned and approved separately, as with
  Lantern.
