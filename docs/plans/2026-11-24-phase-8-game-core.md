# Plan: Phase 8 — Game core (`vnce-game`)

**Date:** 2026-11-24
**Status:** Approved

## Goal

Add the parts every 2D game writes again and again: animated sprites, a scrolling camera,
tile maps, scenes, collision and key bindings. Everything draws on the existing `Canvas`,
so cell modes, Kitty / Sixel and the GPU path keep working.

## 1. Key bindings: `ActionMap<A>`

The game defines its own action type and binds keys or mouse buttons to each action;
queries go through `InputState`: `is_down`, `was_pressed`, `was_released`, and
`axis(negative, positive)` (−1, 0 or 1). Bindings can change at run time.

## 2. Collision

- `Aabb`: intersection, overlap, point containment, minimum translation to separate.
- `move_and_collide`: moves a box through a tile map, x then y, stops at solid tiles and
  reports the sides that were hit; moves in steps so fast objects never pass through.
- `SpatialGrid`: fast "who is near" queries for many objects.

## 3. Sprites and animation

- `SpriteSheet`: splits an image into equal frames (spacing and margin).
- `Animation`: frames and durations; loop, once or ping-pong.
- `Animator`: advances with elapsed time, reports the current frame and whether it ended.
- Drawing with horizontal / vertical flips (existing `blit_region`).

## 4. Tile maps and camera

- `TileMap`: layered grid, loaded from text (`"#..#"` and a character → tile mapping),
  tile properties (solid), `tile_at(world position)`, draws only the visible part.
- `Camera`: follows a target (optional smoothing and dead zone), clamps to world bounds,
  shakes, converts between world and screen coordinates, and keeps its previous position
  for smooth scrolling with `ctx.alpha()`.

## 5. Scenes and timers

- `Scene` trait: like `Game`, but returns a transition (none, push, pop, switch, quit).
- `SceneStack` implements `Game`: only the top scene updates; scenes below a transparent
  one are drawn too (a pause menu over the game).
- `Timer` (cooldowns, repeating events) and easing functions.

## 6. Demo, tests, docs

- `examples/platformer.rs`: an original small platformer — a level from text, an animated
  player, camera follow, collision, coins, title → game → pause (scene stack), arrows and
  WASD (key bindings).
- Tests for collision edge cases (corners, exact contact, fast movement), animation timing,
  camera bounds and scene transitions; a randomized test that `move_and_collide` never
  ends inside a wall.
- `vnce` gets a `game` feature, on by default. No new dependencies.

## Commits

1. `Add Phase 8 plan: game core`
2. `Explain the vnce name and realign the documentation tables`
3. `Add vnce-game: input actions and collision`
4. `Add sprite sheets, animation, tile maps and camera`
5. `Add scene stack and timers`
6. `Add platformer example and Phase 8 docs`

## Risks / notes

- `move_and_collide` steps through fast movement; very fast objects cost a little more.
- No ECS: the engine does not impose a game architecture.
