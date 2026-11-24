# Plan: Phase 2 — `vnce-runtime` and the first interactive demo

**Date:** 2026-11-24
**Status:** Approved

## Goal

Let users write a terminal game by implementing `update` and `render` only, running at the
same speed on every machine and responding smoothly to held keys.

## Problem found in Phase 1

In the `hello` example, every input event triggered a new frame: `input::poll` returns as
soon as an event arrives and the loop redraws immediately. Moving the mouse produced dozens
of frames per second and the animation (tied to the frame count) sped up. The runtime
decouples timing from events; the `hello` example is fixed as well.

## Held keys

| Environment                                       | Key release events                         |
| ------------------------------------------------- | ------------------------------------------ |
| Windows (console host and Windows Terminal)       | Yes, from the Windows console API          |
| Kitty, WezTerm, foot, Ghostty, Alacritty, iTerm2  | Yes, with the kitty keyboard protocol      |
| Other Unix terminals                              | No, only presses and repeats               |

- The kitty keyboard protocol is enabled automatically where supported and disabled again
  on exit and on panic.
- Elsewhere, a key counts as released when no press/repeat arrived for a timeout
  (default 500 ms).
- `Capabilities::key_release_events` tells which mode is active.

## `vnce-term` changes

- `TerminalOptions::keyboard_enhancement`.
- `Capabilities::key_release_events`.
- `Backend::key_release_events()`.
- The keyboard protocol is popped when the terminal is restored.

## `vnce-runtime` (`vnce_runtime`)

- **Fixed timestep:** `update` runs at a fixed rate (default 60 Hz); `render` runs at
  the frame limit (default 60 FPS). Both are configurable, also at run time.
- **`FpsLimit`:** `Fixed(n)` (any value from 1 to 1000) or `Unlimited`.
- **Spiral-of-death protection:** frame time is clamped (default 250 ms).
- **Interpolation:** `render` gets `alpha` in `0.0..1.0`.
- **Waiting = input:** the time until the next frame is spent waiting for input, so input
  is handled immediately; all pending events are processed once per frame.
- **`Game` trait:** `init`, `event`, `update`, `render`.
- **`Context`:** `quit`, `dt`, `elapsed`, `alpha`, counters, measured FPS / UPS,
  `set_fps`, `set_update_rate`, `set_paused`, `input`, `capabilities`, `size`.
- **`InputState`:** `is_down`, `was_pressed`, `was_released`, mouse position and buttons,
  scroll. Character keys are case-insensitive (Shift does not create stuck keys). All keys
  are released when focus is lost.
- **`RunConfig`:** update rate, FPS limit, max frame time, quit on `Ctrl+C`, pause on focus
  loss, key hold timeout, terminal options. Invalid values are rejected.
- **`Driver` trait** (clock + event source) with the real `TerminalDriver` and a
  deterministic `ScriptedDriver` for tests.

## Demo: `examples/stars.rs`

Move `@` with the arrow keys / WASD (diagonals too), collect stars, fading trail, HUD with
score, FPS / UPS and key mode. `+` / `-` change the FPS limit live, `P` pauses, `Esc` quits.

## Commits

1. `Add keyboard enhancement and key release capability to vnce-term`
2. `Add vnce-runtime: fixed-timestep loop, input state and Game trait`
3. `Add stars demo, fix hello frame pacing and Phase 2 docs`

## Risks / notes

- Timeout mode: the OS key-repeat delay (about 250–600 ms) forces a long timeout, so
  movement stops slightly late after a release. Terminals with release events do not have
  this problem.
- Sleep precision on Windows is about 1 ms; interpolation hides the jitter.
- Single-threaded, no async.

## Implementation notes

- With the kitty keyboard protocol, character keys arrive as the base key plus modifiers
  (`Shift+=` is `Char('=')` with `SHIFT`), because alternate keys are not requested. This
  keeps presses and releases of the same physical key consistent.
- `Context::set_fps_limit`, `Context::set_update_rate` and `Context::set_key_hold_timeout`
  change settings at run time; invalid values are rejected.
- `Driver` is also implemented for `&mut D`, so a `ScriptedDriver` can be inspected after
  `run_with` returns.
- The `stars` demo avoids ambiguous-width glyphs for game objects (`@`, `*`, colored
  background cells for the trail) so it renders the same in every terminal and locale.
