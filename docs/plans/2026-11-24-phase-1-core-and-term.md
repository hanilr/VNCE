# Plan: Phase 1 — `vnce-core` and `vnce-term`

**Date:** 2026-11-24
**Status:** Approved

## Goal

Draw colored, Unicode-aware content in the terminal without flicker, on Windows, Linux and
macOS, and receive keyboard / mouse / resize events.

## `vnce-core` (`vnce_core`)

- **Geometry:** `Position`, `Size`, `Rect` (intersection, clipping, containment).
- **`Color`:** `Reset`, the 16 ANSI colors, `Indexed(u8)`, `Rgb(u8, u8, u8)`.
  `FromStr` for `#rgb` / `#rrggbb`. Nearest-color conversion to 256 and 16 colors.
- **`Modifiers`:** bold, dim, italic, underline, blink, reverse, hidden, strikethrough.
  Own small bit-flag type (no extra dependency).
- **`Style`:** `fg`, `bg`, `modifiers`, with a builder API (`Style::new().fg(..).bold()`).
- **`Cell`:** one grapheme cluster + a `Style`. Wide characters (CJK, emoji) occupy two
  cells; the second cell is a continuation cell.
- **`Buffer`:** `get`, `set`, `set_string` (with clipping), `fill`, `clear`, `resize`,
  and `diff(&previous)` returning only changed cells.

## `vnce-term` (`vnce_term`)

- **`ansi`:** own escape sequence writer: cursor movement, SGR (only changed attributes),
  clearing, alternate screen, synchronized output, mouse capture, bracketed paste.
- **`Backend` trait** with `CrosstermBackend` (real terminal) and `TestBackend`
  (in-memory, for tests).
- **`Terminal`:** `enter()` enables raw mode, the alternate screen and hides the cursor.
  Restored on `Drop` and from a panic hook. `draw(|frame| ...)` diffs and writes once.
  On resize, buffers are reallocated and a full redraw is forced.
- **`capabilities`:** color depth (`COLORTERM`, `TERM`, Windows), synchronized output
  support, terminal size.
- **`input`:** own types `Event::{Key, Mouse, Resize, FocusGained, FocusLost, Paste}`,
  `KeyCode`, `KeyModifiers`; `poll(timeout)` and `read()`.

## Facade and example

- `vnce` re-exports `vnce_core` and `vnce_term`.
- `examples/hello.rs`: colored + Unicode text, reacts to resize, shows the last key,
  exits on `q` / `Esc`.

## Dependencies

| Crate                  | Reason                                            |
| ---------------------- | ------------------------------------------------- |
| `crossterm`            | Raw mode, input, terminal size, Windows VT mode.  |
| `unicode-width`        | Display width of characters.                      |
| `unicode-segmentation` | Splitting text into grapheme clusters.            |

## Tests

Color parsing and conversion; buffer writes, clipping and wide-character handling; diff
correctness; escape output via `TestBackend`.

## Commits

1. `Add vnce-core: geometry, color, style, cell and buffer`
2. `Add vnce-term: ANSI output, double-buffered terminal and backends`
3. `Add input events, capability detection and hello example`

## Risks / notes

- The classic Windows console does not support synchronized output; it stays disabled
  there, drawing still works.
- Package names use the `vnce-` prefix (`vn-core` is already taken on crates.io).

## Implementation notes

- Mouse capture, bracketed paste and focus events are enabled through `crossterm`
  rather than `vnce_term::ansi`, because on Windows they are console-mode settings and not
  escape sequences. All drawing output is still produced by `vnce_term::ansi`.
- `VNCE_COLOR_DEPTH` (`truecolor`, `256`, `16`, `none`) was added as a user override for
  color depth detection.
- Media keys and modifier-only key events are not reported yet.
