//! # `vnce_audio`
//!
//! Sound for games in the terminal:
//!
//! - [`Sound`]: decoded audio, from WAV files (and OGG Vorbis with the `ogg` feature).
//! - [`Mixer`]: mixes many sounds with volume, pan, speed, looping, fades, groups and a
//!   soft limiter. It works without a device, for tests or to render offline.
//! - [`Synth`], [`Preset`] and [`Melody`]: sound effects and tunes generated in code, no
//!   files needed.
//! - [`Audio`] (feature `output`, on by default): plays sounds on the default device;
//!   [`Voice`] changes or stops a playing sound.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod mixer;
#[cfg(feature = "output")]
pub mod output;
pub mod sound;
pub mod synth;
mod wav;

pub use mixer::{Group, MAX_VOICES, Mixer, PlayOptions, VoiceId};
#[cfg(feature = "output")]
pub use output::{Audio, AudioError, Voice};
pub use sound::{DecodeError, Sound};
pub use synth::{Melody, ParseNoteError, Preset, Synth, Wave, note};
