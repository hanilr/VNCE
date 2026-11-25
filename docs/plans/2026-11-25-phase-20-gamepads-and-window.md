# Plan: Phase 20 — Gamepads and a more complete window

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (proposed after a review of
what the library still lacks; the user asked for the plan on 2026-11-25)

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

## Steps (one commit each)

1. Plan details: the gamepad library and the event types.
2. Gamepad events and input state; `ActionMap` bindings; tests with simulated events.
3. Gamepads in the window (polled with the window's events); the examples are playable
   with a controller.
4. Window control from the game; settings rows in the examples.
5. Partial picture uploads and scaling choice; bench before and after.
6. Docs (shipping guide: controllers), changelog, closing.

## Dependencies (to be approved)

- `gilrs` for gamepads (MIT OR Apache-2.0), only in `vnce-window`.

## Risks / open questions

- Gamepad support differs by platform (XInput and others on Windows, evdev on Linux,
  macOS); the CI only builds it.
