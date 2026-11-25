# Plan: Phase 17 — Lit meshes and the 3D flight demo "Glide"

**Date:** 2026-11-25
**Status:** Proposed (the user chose a flight game, approved the engine extension and
wants a CPU fallback; the whole plan of Phases 15–17 still needs approval)

## Goal

An original 3D demo game: fly a craft low over generated low-poly terrain, through
rings, past rocks, with a chase camera, a sun and a hazy horizon. To make it look right,
`MeshRenderer` first learns lighting and fog. Without a GPU the same scene is drawn on the
CPU.

## 1. Engine: lit meshes and fog (`vnce-gpu`, approved by the user)

- `Vertex` gains a normal (`position`, `normal`, `color`). This breaks code that builds
  `Vertex` as a struct literal; noted in the changelog. Helpers: `Vertex::new` keeps
  working for unlit vertices, and a helper computes flat (per face) normals for a list of
  triangles.
- A scene uniform: sun direction and color, ambient light, fog color and distances.
  `MeshDraw` carries the model matrix besides the full transform, so normals and fog
  distance are computed in world space.
- Unlit drawing stays possible (no light given: colors as before), so existing users see
  the same pictures.
- `gpu_cube` and Relay's reactor core move to the new vertices; tests on software
  adapters check light (a face toward the sun is brighter) and fog (far pixels approach
  the fog color).

## 2. The game

- Terrain: a height map from fractal noise, turned into a low-poly mesh with flat-shaded
  faces colored by height (water, sand, grass, rock, snow), streamed in chunks ahead of
  the craft.
- Rings to fly through (score, speed boost), rocks and the ground to avoid; a chase camera
  that leans into turns; speed lines and a fog horizon.
- Title, pause, settings, controls, save file with the best score; sound with the `audio`
  feature.

## 3. CPU fallback

The same meshes drawn with `vnce_raster`: triangles transformed and clipped on the CPU,
sorted back to front, lit and fogged per face. Lower detail (fewer chunks), shown on the
screen as "CPU".

## Structure

`crates/vnce/examples/glide/`; `cargo run --release --example glide --features gpu`
(without the feature, or without a GPU, it runs on the CPU).

## Steps (one commit each)

1. Plan.
2. `Vertex` normals, scene light and fog in `MeshRenderer`; tests; examples updated.
3. Terrain generation and chunk meshes.
4. Craft, camera, rings and collisions.
5. CPU renderer.
6. Scenes, settings, save file, sound.
7. `--bench`, smoke test, snapshots; docs and closing.

## Dependencies

None.

## Risks / open questions

- The `Vertex` change is a breaking change of a public type (acceptable before 1.0).
- CPU rendering of a full 3D scene is slower; it keeps a lower triangle budget.
- Engine gaps found while building the game are planned and approved separately.
