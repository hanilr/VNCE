# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Cargo workspace with the `vnce` crate (imported as `vnce`).
- Project documentation, license and tooling configuration.
- `vnce-core` (`vnce_core`): geometry (`Position`, `Size`, `Rect`), `Color` with hex/name
  parsing and downsampling to 256 / 16 colors, `Style` and `Modifiers`, `Cell`, and a
  Unicode-aware `Buffer` with wide-character handling and diffing.
- `vnce-term` (`vnce_term`): own ANSI escape sequence writer with minimal SGR updates,
  double-buffered `Terminal` that writes only changed cells in a single write with
  synchronized output, `CrosstermBackend` (raw mode, alternate screen, restore on drop and
  on panic) and an in-memory `TestBackend` that emulates the screen.
- `vnce_term::input`: engine-owned `Event`, `KeyEvent`, `KeyCode`, `KeyModifiers` and
  `MouseEvent` types with `poll(timeout)` and `read()`.
- `Capabilities::detect()`: color depth from `VNCE_COLOR_DEPTH`, `NO_COLOR`, `COLORTERM`,
  `TERM`, `TERM_PROGRAM` and Windows Terminal; synchronized output disabled where
  unsupported.
- `hello` example: styled and Unicode text, truecolor gradient, animation, resize and
  input events.
- `TerminalOptions::keyboard_enhancement` and `Capabilities::key_release_events`: the kitty
  keyboard protocol is enabled where supported so key releases are reported (always
  available on Windows); it is popped again on restore and on panic.
- `vnce-runtime` (`vnce_runtime`): `Game` trait and `run` with fixed-timestep updates,
  configurable frame limit (`FpsLimit::Fixed(1..=1000)` or `Unlimited`, changeable at run
  time), spiral-of-death protection, interpolation `alpha`, measured FPS / UPS, pause,
  `InputState` (held keys with release events or a timeout fallback, case-insensitive
  letters, mouse) and a deterministic `ScriptedDriver` for tests.
- `stars` example: a small game with held-key movement, interpolation, a fading trail and
  live FPS limit changes.

### Fixed

- `hello` example: frames are paced by the clock; input events (for example mouse
  movement) no longer trigger extra frames and speed up the animation.
