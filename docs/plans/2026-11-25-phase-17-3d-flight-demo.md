# Plan: Phase 17 — Lit meshes and the 3D flight showcase "Glide"

**Date:** 2026-11-25
**Status:** Approved, in progress — step 2 done (the user chose a flight game, approved the engine
extension and wants a CPU fallback; the whole plan of Phases 15–17 was approved on
2026-11-25; after Phase 16 the user agreed to the wider scope below, so Glide, like Relay
and Isle, shows everything it can, and asked to start it in a later session)

## Goal

An original 3D showcase game: fly a craft low over generated low-poly terrain, through
rings, past rocks, with a chase camera, a sun and a hazy horizon. To make it look right,
`MeshRenderer` first learns lighting and fog. Without a GPU the same scene is drawn on the
CPU. Like Relay (everything) and Isle (2.5D), Glide shows as much of the engine as it can
in 3D, as part of play.

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

### Wider scope (agreed after Phase 16)

- Sky: a GPU `ShaderQuad` sky (gradient, sun disk, clouds) behind the meshes; the meshes
  are rendered with a transparent clear and laid over it on the CPU (no engine change).
  The CPU draws a simpler sky.
- Time of day over a flight, from sunrise to sunset: the sun's direction and color, the
  ambient light and the fog change with it (showing the new light and fog uniforms).
- Low-poly trees and rocks placed on the terrain (many `MeshDraw`s), a water surface, and
  a fake blob shadow under the craft.
- Cameras: chase, cockpit, and an orbit / photo camera dragged with the mouse.
- Exhaust trail and sparks drawn on the CPU and projected with the same matrices; speed
  lines.
- Courses from a code (as Isle's island codes); two modes: a ring course against the clock
  with checkpoints, and free flight.
- HUD: speed and altitude gauges, a compass, a radar of the rings (`vnce_ui` widgets and
  pixel drawing).
- Sound: an engine hum whose pitch follows the speed (`Voice::set_speed`), wind by speed,
  ring chimes, a crash.
- Settings: display method, MSAA, render resolution (lower for speed), sharp or engine
  upscaling, GPU or CPU, volumes, frame rate, performance panel; rebindable keys; save
  file. Menus over another scene use a request instead of a switch (see Isle).

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
6. Sky, time of day, trees, rocks and water, cameras, trail and effects, HUD, sound.
7. Scenes, courses and modes, settings, save file.
8. `--bench`, smoke test, snapshots; docs and closing.

## Dependencies

None.

## Risks / open questions

- The `Vertex` change is a breaking change of a public type (acceptable before 1.0).
- CPU rendering of a full 3D scene is slower; it keeps a lower triangle budget.
- `ShaderQuad` passes only the time and a frame number; the sky's sun position follows the
  time of day, so it may need the sun direction. If it does, a small `ShaderQuad`
  parameter uniform is an engine change to plan and approve separately; otherwise the sky
  is drawn for a fixed set of sun heights.
- Engine gaps found while building the game are planned and approved separately.

## Progress

- Step 2 done: `Vertex::normal`, `Vertex::lit`, `flat_shaded`, `MeshDraw::model` /
  `MeshDraw::new`, `MeshScene` with `Light` and `Fog` (`MeshRenderer::set_scene`), public
  vector helpers in `math`; GPU tests for light and fog; `gpu_cube` (`L` key) and Relay's
  core are lit. Fog fades its alpha too, so Glide can fade terrain out over the sky (the
  target then holds premultiplied colors in partly transparent pixels; un-premultiply or
  composite accordingly in step 6).

## Resume here (next session)

State on 2026-11-25: Phases 15 and 16 are done and committed locally (not pushed: the user
asked not to push yet). Start this phase at step 2 (the engine change), following the
workflow in `AGENTS.md`: the plan is approved, so carry out each step with checks
(`--all-features` and `--no-default-features`), focused commits, and present the result.
After this phase comes Phase 18, the native window backend
(`2026-11-25-phase-18-window-backend.md`).
