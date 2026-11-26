# Plan: Phase 20 — Gamepads and a more complete window

**Date:** 2026-11-25
**Status:** Done (proposed after a review of what the library still lacks; the
user asked for the plan on 2026-11-25 and approved the detailed plan below, `gilrs`, the
CI change and `#[non_exhaustive]` on 2026-11-26, asking for the whole phase; finished the same day)

## Goal

Games released in a window (Phase 18) are played with controllers as often as with a
keyboard, and let the player choose fullscreen and resolution from their settings. This
phase adds gamepads and fills the gaps found while building the window. (Text in every
language, with font fallback in the window, moved to Phase 22 on 2026-11-25.)

## What it adds

1. **Gamepads** (window only; terminals cannot read them): buttons and sticks as engine
   events and input state (`InputState::gamepad`, pressed / released / axis values), a
   standard layout (south / east / west / north buttons, shoulders, triggers, sticks,
   d-pad), connect and disconnect events, and bindings in `ActionMap` (a button or a
   stick direction bound to an action, saved like keys). Rumble if the library offers it.
2. **Window control from the game:** a handle in `Context` (or through `vnce_window`)
   to switch fullscreen, hide or show the mouse cursor, change the title, and read the
   window's size and scale; the settings of Glide and Isle get a fullscreen row.
3. **Faster overlays:** pictures drawn with `draw_image` upload only the rows that
   changed, so a full-size transparent instruments layer (Glide) costs much less.
4. **Window details:** a choice of sharp (nearest) or smooth scaling for pictures, and
   vsync as a setting.

## Decisions (approved 2026-11-26)

What the code showed when the details were planned:

- The game only sees `Context`; the window is behind the `Backend` trait. `Context`
  already passes one request to the terminal (`set_graphics_mode`, applied before the
  next frame).
- `Event` and `Binding` are exhaustive enums, so new variants are a breaking change
  (0.3.0).
- The window uploads a whole picture texture whenever the picture changed; Glide's
  full-size transparent instruments layer changes every frame.
- Pictures are always scaled with `Linear` filtering; vsync is only an open option.
- `gilrs` 0.11 (MIT OR Apache-2.0) needs the `libudev` development files on Linux.

The decisions:

1. **Gamepad events in `vnce_term`:** `Event::Gamepad(GamepadEvent)` with a pad id and a
   kind (connected, disconnected, button pressed / released, axis value) in a standard
   layout (`GamepadButton`: south, east, west, north, shoulders, triggers, stick presses,
   d-pad, start, select, mode; `GamepadAxis`: both sticks, both triggers). Terminals
   never send them; `ScriptedDriver` can, so tests need no controller.
2. **`InputState`:** buttons held / pressed / released (any pad, or one pad), axis values
   with a dead zone, connected pads.
3. **`ActionMap`:** `Binding::Button(GamepadButton)` and `Binding::Stick(GamepadAxis,
   direction)` (a stick pushed more than half way counts as held), saved by name
   (`pad:South`, `pad:LeftX+`).
4. **Window control through `Context`:** `set_fullscreen`, `set_cursor_visible`,
   `set_title`, `set_vsync`, `set_smooth_scaling`, `rumble`, collected as requests and
   handed to a new `Backend` method with a no-op default after each frame, so terminals
   ignore them; `ctx.is_windowed()` tells the settings screens whether to show window
   rows.
5. **`gilrs` behind a `gamepad` feature of `vnce-window`, on by default;** the `window`
   feature of `vnce` enables it. Linux builds need `libudev-dev` (shipping guide, CI).
6. **Partial uploads:** the window keeps each picture's last pixels and uploads only the
   band of rows that changed.
7. **`#[non_exhaustive]`** on `Event`, `Binding` and the new gamepad enums, so later
   variants are not breaking.

## Steps (one commit each)

1. Plan details: the gamepad library and the event types.
2. Gamepad events and input state; `ActionMap` bindings; tests with simulated events.
3. Gamepads in the window (polled with the window's events); the examples are playable
   with a controller.
4. Window control from the game; settings rows in the examples.
5. Partial picture uploads and scaling choice; bench before and after.
6. Docs (shipping guide: controllers), changelog, closing.

## Dependencies (approved 2026-11-26)

- `gilrs` 0.11 for gamepads (MIT OR Apache-2.0), only in `vnce-window`, behind its
  `gamepad` feature (on by default). CI installs `libudev-dev` on Linux.

## Risks / open questions

- Gamepad support differs by platform (XInput and others on Windows, evdev on Linux,
  macOS); the CI only builds it. A real controller can only be tried on the user's
  machine; the user is asked to try one after step 3.
- New enum variants and `#[non_exhaustive]` are breaking: the next release is 0.3.0.
