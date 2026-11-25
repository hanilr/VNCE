# Plan: Phase 16 — Isometric demo "Isle"

**Date:** 2026-11-25
**Status:** Done (the user chose an isometric game over a raycaster and approved the
whole plan of Phases 15–17 on 2026-11-25; on starting the phase the user asked for a
showcase that covers everything and approved the wider scope below, with an optional GPU
effect)

## Goal

An original 2.5D showcase game in an isometric view that, like Relay, shows as much of
the engine as it can as part of play. A small island, generated anew every game, with
several height levels: collect the light crystals scattered over it and light the beacon
before night falls.

## What it shows

2.5D techniques:

- Isometric projection of a height map: levels, stairs (ramps), cliff sides with shading.
- The camera turns in 90° steps (`Q` / `E`), eased; depth order and heights stay right
  from every side. Zoom with the mouse wheel or `+` / `-`; a smoothly following camera.
- Depth sorting: trees, rocks, crystals, creatures and the player drawn in the right
  order, also behind and in front of higher tiles; soft drop shadows.
- X-ray: when the player is hidden behind a tree or a cliff, an outline shows through.
- The tile under the mouse is highlighted, with a preview of the path to it.
- Animated water around the island (an optional GPU shader with a CPU fallback), foam on
  the shore, cloud shadows drifting over the land.
- Weather: passing rain showers (falling streaks, splashes), fireflies at night, sparks
  added with `Add` blending.
- Light over time: dusk turning into night (an ambient color), the player's lantern, the
  crystals and the beacon as point lights in a light layer that multiplies the scene.
- A minimap drawn into its own small canvas.

Engine modules:

- `Rng` with a seed: every island has a code that can be typed in to play it again.
- `SpriteSheet` / `Animator`: the player walks in four directions; crabs and wisps.
- `SpatialGrid` and `move_and_collide`: crabs wander the beaches, wisps drift at night
  and steal lantern oil on contact.
- `Camera`, `Timer`, `ease`, `Font` (title), `Transition` between scenes, `ActionMap`
  (rebindable keys), `SaveFile` (settings, best time, last island code).
- `vnce_ui`: `Gauge` (night, lantern oil), `Paragraph` (hints), `List` (menus), `Block`,
  `FpsOverlay`-style performance panel.
- `vnce_audio`: waves and rain ambience, footsteps that depend on the ground, crystal
  chimes, day and night music in mixer groups.
- Every pixel mode and protocol, scale quality, resizing.
- `vnce_gpu` (feature `gpu`): the ocean as a `ShaderQuad`, read back pipelined; the CPU
  draws the same kind of ocean without a GPU.

Mouse: click a tile to walk there; the path is found with A* (written in the example,
not added to the engine). Arrow keys / WASD move directly.

## Structure

`crates/vnce/examples/isle/`: `main` (shared state, helpers, tests), `island`
(generation, walking rules, A*), `iso` (projection, rotation, terrain cache, picking),
`art` (sprites), `play` (the game), `scenes` (title, pause, settings, controls, island
code, end), `audio`, `visuals` (ocean on GPU / CPU), `bench`.
`cargo run --release --example isle` (`--features gpu,audio` for the GPU ocean and
sound).

## Steps (one commit each)

1. Plan.
2. Island generation and the isometric renderer (levels, sides, stairs, water, cache).
3. Player, keyboard movement, levels and stairs, depth sorting, turning the camera, zoom,
   x-ray.
4. Mouse: hover, click-to-walk with A*, path preview.
5. Crystals, beacon, creatures, day / night and light, weather, minimap, sound, GPU ocean.
6. Scenes, settings, island codes, save file.
7. `--bench`, smoke test, snapshots; docs and closing.

## Quality and speed

- Static terrain and props drawn once per camera direction into a cached picture; every
  frame copies the visible part, then draws actors and redraws only the few tiles in front
  of them (so they are hidden correctly).
- Target: under 3 ms per frame in half blocks at 200 × 60 cells; real-pixel modes benefit
  from Phase 15.

## Dependencies

None.

## Risks / open questions

- Depth sorting of moving actors across tile boundaries needs care (sort by the tile
  under the actor's base; redraw the tiles in front of it).
- `ShaderQuad` passes only time and a frame number; the ocean is drawn as a tiling
  picture in world space and placed by the CPU, so the camera position needs no uniform.
- Engine gaps found while building it are planned and approved separately, as with
  Lantern.

## Results

`cargo run --release --example isle --features gpu -- --bench` (200 × 60 cells, 600
frames at dusk in the rain, walking a route with two camera turns and a zoom; development
machine with an RTX 5070 Laptop GPU):

| Display method | Per frame | Of it in `render` | Output per frame |
| -------------- | --------- | ----------------- | ---------------- |
| Half blocks    | 0.97 ms   | 0.65 ms           | 99 KB            |
| Sextants       | 2.45 ms   | 2.11 ms           | 107 KB           |
| Braille        | 2.40 ms   | 2.06 ms           | 106 KB           |
| Kitty          | 10.2 ms   | 2.12 ms           | 316 KB           |
| Sixel          | 17.8 ms   | 2.28 ms           | 300 KB           |

The target (under 3 ms in half blocks) is met. A still frame from the terrain cache takes
0.2 ms (400 × 116 world pixels) to 0.7 ms (666 × 386); while the camera turns, every tile
is drawn at the current angle in 1 to 3.4 ms. With a graphics protocol most of the time is
sending: rain, the animated ocean and the light change most pixels every frame.

## Implementation notes

- Tile tops are drawn pixel by pixel through the inverse projection (texture, a soft
  border, stair steps and the shadows of nearby props), so they work at any camera angle
  and never leave seams; cliff sides are triangles that reach a pixel into the top.
- A tile drawn again over an actor (`Layer::Above`) produces exactly the pixels of the
  cache: only opaque parts, and only the sides and tops that rise above the actor, so a
  flat tile in front never cuts off feet.
- The x-ray keeps the player's pixels right after drawing them and compares them after
  everything else: the pixels that changed were covered.
- `ShaderQuad` passes only the time, so the ocean is a seamless 128 × 64 picture placed by
  the CPU under the camera; the GPU and CPU versions use the same shades.
- A `Transition::Switch` replaces only the top scene, so menus over another scene leave a
  `Request` in the shared data and pop; the scene below switches. (Relay's pause menu
  switches to the title from the top, which leaves its game scene below; noted for a
  separate fix.)
- Stairs are only placed where their sides are open (no higher neighbor beside them);
  otherwise the side between the stair and the higher tile would show a gap.

