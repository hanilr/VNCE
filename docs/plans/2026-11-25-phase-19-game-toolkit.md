# Plan: Phase 19 — Game toolkit: paths, particles, easing, math and more widgets

**Date:** 2026-11-25
**Status:** In progress (proposed after a review of what the library still lacks; the
user asked for the plan on 2026-11-25 and approved the detailed plan below on 2026-11-26,
asking for the whole phase to be done step by step)

## Goal

Relay, Isle and Glide each wrote the same tools inside their examples: A* path finding
(Isle), particles (sparks, rain, trails in all three), easing for camera and menu
motion, and a small vector / matrix module (Glide, because `vnce_gpu::Mat4` needs the
`gpu` feature). Settings and code entry screens were built by hand from `List`. This
phase moves these tools into the engine, so a new game gets them without copying.

## What it adds

1. **Math without the GPU** (`vnce_core::math` or a small `vnce-math` crate, decided in
   step 1): `Vec2`, `Vec3`, `Mat4` with the conventions of `vnce_gpu` (right-handed,
   column vectors, depth `0..=1`), camera helpers (`look_to`, `perspective`, `project`,
   `ray`). `vnce_gpu::Mat4` / `Vec3` become re-exports of it (same layout), so meshes and
   CPU code share one type. Glide's own `math.rs` moves onto it.
2. **Easing and tweens** (`vnce_game::ease`): the usual curves (linear, quad, cubic,
   sine, back, elastic, bounce; in / out / in-out) and a `Tween<T>` that runs a value
   from a start to an end over a duration, with `update(dt)` and `value()`.
3. **Particles** (`vnce_game::particles`): an emitter with rate, burst, lifetime,
   speed, spread, gravity, drag, colors over life and size; updated with `dt`, drawn into
   a `Canvas` (2D) or given out as points to project (3D, as Glide's sparks).
4. **Path finding** (`vnce_game::path`): A* on a grid (`TileMap` or a callback for
   costs and blocked cells), 4 or 8 neighbors, a cost limit; Isle's walking moves onto
   it.
5. **Widgets** (`vnce_ui`): `TextInput` (cursor, editing keys, a character filter, as
   Isle's and Glide's code entry), `Table` (columns with widths and a selected row),
   `Tabs`, `Slider` (a value in a range, as the volume rows), `Checkbox`, and mouse hit
   testing for `List`, `Tabs` and buttons (which item is under a cell).

## Decisions (approved 2026-11-26)

What the code showed when the details were planned:

- Easing already exists (`vnce_game::timer::ease`: linear, quad, cubic, `out_back`; used
  16 times by the examples). Step 3 adds the missing curves and `Tween`, not a new module.
- `vnce_gpu::math` has `Mat4` and `Vec3 = [f32; 3]`; Glide's `math.rs` adds vector
  helpers, `look_to`, `basis`, `apply` and a `Camera` with `project` and `ray`.
- `vnce_gpu` puts `Mat4` in its uniform structs, which need `bytemuck::Pod`; a `Mat4`
  from another crate cannot get `Pod` there (orphan rule).
- Isle's A* asks whether a step between two tiles is allowed (`can_step(from, to)`,
  because of height differences), not whether one tile is free.
- Relay (2D), Isle (isometric) and Glide (3D) each have their own sparks with the same
  rules: position, velocity, life, color, gravity, drag.
- `vnce-ui` depends only on `vnce-core`; the code entry screens handle keys by hand.

The decisions:

1. **Math lives in `vnce_core::math`**, not in a new crate (no extra crate to publish).
   `vnce_gpu::{Mat4, Vec3}` and `vnce_gpu::math` stay as re-exports, so no path breaks.
2. **`Pod`:** `vnce_gpu`'s private uniform structs hold `[[f32; 4]; 4]` (`Mat4::cols`)
   instead of `Mat4`, so `vnce-core` needs no `bytemuck`.
3. **Vectors stay arrays:** `Vec2 = [f32; 2]`, `Vec3 = [f32; 3]` with free functions
   (`add`, `sub`, `scale`, `dot`, `cross`, `length`, `normalize`, `lerp`, `rotate`, ...),
   which keeps the current API.
4. **`TextInput`:** the editing methods and the character filter live in `vnce-ui`; a
   `handle_key(&KeyEvent)` maps keys to them, so `vnce-ui` gains a dependency on the
   internal `vnce-term` crate (already part of every `vnce` build).

## Steps (one commit each)

1. Plan details: where the math lives, names, what stays in the examples.
2. Math module; `vnce_gpu` re-exports it; Glide uses it; tests.
3. Easing and tweens (the missing curves: sine, back, elastic, bounce; `Tween<T>` with
   `update(dt: Duration)`, `value()`, `is_finished()`; a `Lerp` trait for `f32`, vectors
   and colors); tests; used by one example (Isle's camera turn).
4. Particles; tests; Relay, Isle and Glide use them where it fits.
5. Path finding; tests; Isle uses it.
6. Widgets and mouse hit testing; tests; the `ui` example shows them; Glide's and Isle's
   code entry and settings use them where it makes them simpler.
7. Tutorial notes, changelog, architecture, closing.

## Dependencies

No new external crates. One new link between the project's own crates: `vnce-ui` uses
`vnce-term` (for `KeyEvent` in `TextInput::handle_key`).

## Risks / open questions

- Moving `Mat4` / `Vec3` must not break `vnce_gpu` users: re-exports keep the paths.
- The examples must keep their look and behavior (their smoke tests and snapshots are
  the check).
- Moving the sparks onto a shared emitter can change the order of random numbers and so
  the snapshots; if that happens, the user is asked before snapshots are renewed.
- `vnce_gpu` keeps its public API; the changes still go into the next release (0.2.0).
