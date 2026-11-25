# Plan: Phase 22 — Text in every language

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (the user asked on
2026-11-25 for support of the characters of every language)

## Goal

A game made with vnce should show and take text in any language: Turkish, Greek,
Cyrillic, Arabic, Hebrew, Devanagari, Thai, Chinese, Japanese, Korean and emoji. The
terminal layer already measures text by grapheme clusters and display width
(`unicode-segmentation`, `unicode-width`), but the pixel font is ASCII only, the window
shows empty boxes for symbols missing from JetBrains Mono, there is no input method
(IME) for languages typed through one, and right-to-left text was never tried.

## What it adds

1. **A test set of scripts:** strings in each script above (with combining marks,
   wide CJK symbols, emoji with ZWJ sequences and skin tones, right-to-left text) run
   through `Buffer::set_string`, `Paragraph` wrapping and truncation, `List` and the
   terminal diff; snapshot tests show where cells go wrong.
2. **Font fallback in the window** (moved from Phase 20): symbols missing from the
   bundled font are taken from fonts installed on the system, so text in any language
   shows instead of empty boxes; wide symbols keep two cells; color emoji where the
   system font has them.
3. **More pixel fonts:** the built-in 5×7 font gains Latin Extended (Turkish and the
   other European languages), Greek and Cyrillic; `Font` loads bitmap fonts (BDF) and
   rasterizes TrueType / OpenType fonts at a pixel size, so games can bring a font for
   any script into a `Canvas`.
4. **Text input through an IME** (window): `winit`'s IME events become engine events
   (preedit text and the committed text), with the candidate window placed at the
   cursor; `TextInput` (Phase 19) uses them. In a terminal, the terminal's own IME
   already sends the finished text.
5. **Right-to-left text (research first):** what terminals do with Arabic and Hebrew
   (most show logical order, some reorder), whether vnce should reorder lines itself
   (the Unicode bidi algorithm) and shape Arabic letters in the window. Implemented
   only where the research shows it works the same everywhere; otherwise documented.

## Steps (one commit each)

1. Plan details: the libraries, the scripts in the test set, the RTL research.
2. The test set and fixes in `vnce_core`, `vnce_term` and `vnce_ui`.
3. Font fallback in the window.
4. Built-in font extended; BDF and TrueType loading for `Font`.
5. IME events and their use in `TextInput`.
6. Right-to-left research, and what it decides.
7. A tutorial note, an example screen with every script, changelog, closing.

## Dependencies (to be approved)

- `fontdb` to find fonts installed on the system (MIT), only in `vnce-window`.
- A font rasterizer for `Font` (`fontdue` or `ab_glyph`, both MIT / Apache-2.0); the one
  `vnce-window` already uses is preferred, behind a feature of `vnce-raster`.
- Possibly `unicode-bidi` (MIT / Apache-2.0) and a shaper (`rustybuzz`, MIT) after the
  research in step 6.

## Risks / open questions

- Terminals disagree on the width of some symbols (emoji with variation selectors,
  some CJK punctuation); vnce follows `unicode-width` and documents the exceptions.
- System fonts differ between machines; the bundled font stays the first choice so the
  look does not change where it has the symbol.
- Arabic shaping and right-to-left lines are large topics; this phase may end with a
  documented limit instead of full support.
