//! # `vnce_audio`
//!
//! Sound for games in the terminal:
//!
//! - [`Sound`]: decoded audio, from WAV files (and OGG Vorbis with the `ogg` feature).
//! - [`Mixer`]: mixes many sounds with volume, pan, speed, looping, fades, groups and a
//!   soft limiter. It works without a device, for tests or to render offline.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod mixer;
pub mod sound;
mod wav;

pub use mixer::{Group, MAX_VOICES, Mixer, PlayOptions, VoiceId};
pub use sound::{DecodeError, Sound};
