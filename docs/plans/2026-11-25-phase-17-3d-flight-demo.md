# Plan: Phase 17 — Lit meshes and the 3D flight showcase "Glide"

**Date:** 2026-11-25
**Status:** Done (the user chose a flight game, approved the engine
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
- Steps 3–8 done (the user asked to go on until the game is finished): terrain, models,
  sky and both renderers in one commit (the CPU renderer of step 5 came early so the
  viewer of step 3 ran without a GPU too); craft, camera, rings, collisions, the wider
  scope of step 6 and the scenes of step 7 in one commit (`Glide: the game`), as they were
  built and tested together; then the bench, smoke tests, snapshots and these docs.

## Results

`cargo run --release --example glide --features gpu -- --bench` (200 × 60 cells, 480
frames of the autopilot flying a course; development machine with an RTX 5070 Laptop GPU;
Kitty and Sixel pictures are 2000 × 1140 pixels at the default cell size):

| Display method | GPU per frame | GPU world | CPU per frame | CPU world | Output (GPU) |
| -------------- | ------------- | --------- | ------------- | --------- | ------------ |
| Half blocks    | 2.8 ms        | 1.7 ms    | 3.1 ms        | 2.5 ms    | 258 KB       |
| Sextants       | 4.2 ms        | 1.6 ms    | 4.6 ms        | 2.5 ms    | 297 KB       |
| Braille        | 4.2 ms        | 2.0 ms    | 4.8 ms        | 3.1 ms    | 291 KB       |
| Kitty          | 26.7 ms       | 10.4 ms   | 29.4 ms       | 18.4 ms   | 1334 KB      |
| Sixel          | 28.0 ms       | 10.5 ms   | 33.5 ms       | 18.3 ms   | 603 KB       |

"World" is the 3D picture with the sky; the rest is the instruments, the text and
encoding the picture for the terminal. The GPU draws the chunks within about 1.4 km at
full detail; the CPU those within about 850 m at half detail, trees and rocks only up to
520 m. Laying the meshes over the sky first
took 47 ms on a full-size picture; looking the sky up on a coarse grid and blending in
whole numbers brought it to 9 ms. The render resolution setting (75, 50, 33 %) lowers
both further.

## Implementation notes

- The CPU renderer must work without the `gpu` feature, where `vnce_gpu::Mat4` does not
  exist, so Glide has its own small `Mat4` with the same layout (converted for free on the
  GPU path). A math module that does not need the GPU would be a small engine addition
  for later.
- `ShaderQuad` passes only the time and a frame number. The sky is therefore a panorama
  (azimuth across, elevation down) that the GPU draws with the day's progress packed into
  the frame number; every pixel of the picture looks it up along its view direction, so
  it turns and tilts with the camera without an engine change.
- The meshes are rendered on a transparent background with fog whose color has alpha 0:
  far terrain fades out and the target holds premultiplied colors there, which the
  composition adds to the sky (`front + sky × (1 − alpha)`).
- The GPU's picture arrives a frame late (pipelined readback); the renderer returns the
  camera it was taken with, and the trail, sparks and instruments are placed with that
  camera, so they never drift from the picture.
- Chunks are built a few per frame, nearest and ahead first; the ground under the craft
  is computed from the same grid and triangle split as the mesh, so collisions match what
  is drawn. Each chunk keeps its trees and rocks apart, so the CPU leaves them out beyond
  520 m.
- The audio's engine hum is a one-second loop in which every part repeats a whole number
  of times, so it loops without a click; `Voice::set_speed` bends its pitch with the speed.

## Resume here (next session)

State on 2026-11-25: Phases 15, 16 and 17 are done and committed locally (not pushed: the
user asked not to push yet). This phase started at step 2 (the engine change), following the
workflow in `AGENTS.md`: the plan is approved, so carry out each step with checks
(`--all-features` and `--no-default-features`), focused commits, and present the result.
After this phase comes Phase 18, the native window backend
(`2026-11-25-phase-18-window-backend.md`).
