# Plan: Phase 19 — Game toolkit: paths, particles, easing, math and more widgets

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (proposed after a review of
what the library still lacks; the user asked for the plan on 2026-11-25)

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

## Steps (one commit each)

1. Plan details: where the math lives, names, what stays in the examples.
2. Math module; `vnce_gpu` re-exports it; Glide uses it; tests.
3. Easing and tweens; tests; used by one example (Isle's camera turn).
4. Particles; tests; Relay, Isle and Glide use them where it fits.
5. Path finding; tests; Isle uses it.
6. Widgets and mouse hit testing; tests; the `ui` example shows them; Glide's and Isle's
   code entry and settings use them where it makes them simpler.
7. Tutorial notes, changelog, architecture, closing.

## Dependencies

None.

## Risks / open questions

- Moving `Mat4` / `Vec3` must not break `vnce_gpu` users: re-exports keep the paths.
- The examples must keep their look and behavior (their smoke tests and snapshots are
  the check).
