# Plan: Phase 9 — Audio (`vnce-audio`)

**Date:** 2026-11-24
**Status:** Approved

## Goal

Sound effects and music for games, including sounds generated in code (no files needed),
which suits retro terminal games. Behind an `audio` feature that is off by default.
Before that, the environment variables follow the `vnce` name.

## 0. Environment variables

`VNCE_COLOR_DEPTH`, `VNCE_PIXEL_MODE`, `VNCE_GRAPHICS`, `VNCE_GPU_BACKEND` and `VNCE_GPU_POWER` become
`VNCE_COLOR_DEPTH`, `VNCE_PIXEL_MODE`, `VNCE_GRAPHICS`, `VNCE_GPU_BACKEND` and
`VNCE_GPU_POWER`.

## 1. Output and mixer

- `cpal` (0.18, Apache-2.0): WASAPI on Windows, CoreAudio on macOS, ALSA on Linux. ALSA
  needs `libasound2-dev` (Fedora: `alsa-lib-devel`) at build time, hence the feature is
  off by default; CI installs it.
- `Audio::new()` opens the default device, any sample format (f32, i16, u16). Without a
  device (servers, SSH) it returns an error and the game stays silent; it never panics.
- Own mixer on the audio thread: commands arrive through a lock-free queue, no allocation
  while mixing, at most 32 voices (the oldest stops), soft limiting against clipping.
- Playback: `audio.play(&sound)` returns a handle; volume, pan, speed / pitch, looping,
  stop, fade in / out; master volume and mute; music and effects groups with their own
  volume.

## 2. Sounds

- `Sound`: decoded once and shared; resampled to the device rate when loaded.
- Own WAV decoder: 8 / 16 / 24 / 32-bit integer and 32-bit float, mono and stereo; never
  panics on broken files.
- OGG Vorbis through `lewton` (pure Rust, MIT / Apache-2.0) behind an `ogg` feature.

## 3. Synthesis

`Synth`: square, saw, triangle, sine and noise waves; ADSR envelope, pitch slide,
vibrato, square duty; presets (jump, coin, hit, explosion, power-up, laser, blip) with a
seed, so the same seed always gives the same sound; a small note sequencer for short music
loops.

## 4. Facade, demos, tests

- `vnce` gets `audio` (and `ogg`) features, off by default.
- The platformer plays jump, coin and goal sounds and a generated music loop with
  `--features audio`; without it, it runs silently as today.
- `examples/sfx.rs`: pick presets from a list, tweak parameters, listen (with `vnce_ui`).
- Tests need no audio device: mixer output (volume, pan, looping, voice limit, fades), WAV
  decoding and randomized broken files, synthesis (length, envelope, no NaN, peak).

## Commits

1. `Add Phase 9 plan: audio`
2. `Format the color depth check in Capabilities`
3. `Add vnce-audio: sounds, WAV decoding and the mixer`
4. `Add audio output with cpal`
5. `Add sound synthesis and effect presets`
6. `Add OGG Vorbis decoding`
7. `Add audio feature, sound to the platformer, sfx example and Phase 9 docs`

## Risks / notes

- Real output can only be tried on Windows here; Linux and macOS are build-checked.
- Small device buffers keep latency low where the device allows it.

## Implementation notes

- Sounds are not resampled when loaded (the device rate is not known then): the mixer
  steps through each sound at `speed × sound rate / output rate` with linear
  interpolation, which converts any rate on the fly.
- `Mixer` is public and needs no device; `Audio` wraps one on the audio thread. Ids are
  assigned on the game side so `play` returns at once; the audio thread publishes the
  playing ids in atomics after every buffer for `Voice::is_playing`.
- Soft limiter: unchanged up to 0.8, then a `tanh` knee towards 1.0.
- The tests found an overflow in `Sound::frame` for the end-of-sound index of stereo
  sounds; fixed before the first commit.
- Verified on Windows with a real device (silence only, so the tests make no noise); the
  OGG decoder was checked on a real file found on this machine, which is not committed
  (it belongs to another game). Linux builds of `cpal` need the ALSA development files,
  so the Linux cross-check here covers `vnce-audio` without the `output` feature; CI
  installs `libasound2-dev` and builds everything on Linux.
- The platformer keeps working without the feature (a silent `Sounds` stand-in); with it,
  the music fades in when playing starts, ducks under the pause menu and fades out at the
  goal.
