# Plan: Phase 25 — Research: higher bit depths and sample rates for sound

**Date:** 2026-11-25
**Status:** Planned, waiting for the user's approval to start (the user asked on
2026-11-25 to research whether higher bit depths can be supported)

## Goal

Find out whether `vnce_audio` should support sound beyond what it has now, and what it
would cost. This phase is research: it ends with a report and a recommendation, and
code is written only if the user approves it afterwards.

## What vnce has today

- WAV decoding: 8, 16, 24 and 32-bit integer PCM and 32-bit float.
- OGG Vorbis decoding (feature `ogg`).
- The mixer works in 32-bit float (`f32`): about 24 bits of precision, roughly 144 dB
  of dynamic range, more than any output device delivers.
- Output: whatever the device offers through `cpal` (8 to 64-bit integer, 32 and 64-bit
  float, including 24-bit).

## Questions to answer

1. **64-bit float WAV:** decoding it is small work; is it worth it for games?
2. **A 64-bit float mixer:** does mixing many sounds, with effects, ever lose audible
   quality in `f32`? Measure noise and error against an `f64` reference.
3. **Sample rates:** 48 kHz is the usual device rate; what happens with 88.2 / 96 /
   192 kHz files and devices, and how good is the resampling (quality, cost)?
4. **Lossless formats:** FLAC (up to 24-bit and high sample rates) for music without
   OGG's loss; size and decoding cost against OGG.
5. **Dither** when the device takes fewer bits (16-bit) than the mixer has.
6. **What other engines do** (for example Godot, Unity, FMOD, Bevy) and what players can
   hear.

## Steps

1. Measurements and a short report in `docs/` (tables of error, noise, size and CPU
   cost), with a recommendation.
2. The user decides; the approved parts get their own plan steps.

## Dependencies

None for the research; possibly a FLAC decoder (for example `claxon`, Apache-2.0) if
FLAC is approved later.

## Risks / open questions

- "Higher bits" may not be audible in games; the report says so plainly if that is the
  result.
