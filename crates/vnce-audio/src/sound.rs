//! Decoded audio: [`Sound`].

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Audio samples in memory, ready to play: mono or stereo, any sample rate (the mixer
/// converts it). Cloning is cheap: the samples are shared.
#[derive(Clone, PartialEq)]
pub struct Sound {
    samples: Arc<[f32]>,
    channels: u16,
    sample_rate: u32,
}

impl fmt::Debug for Sound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sound")
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .field("frames", &self.frames())
            .finish_non_exhaustive()
    }
}

impl Sound {
    /// A sound from interleaved samples in `-1.0..=1.0`. More than two channels are
    /// reduced to the first two; non-finite samples become silence.
    #[must_use]
    pub fn from_samples(channels: u16, sample_rate: u32, samples: Vec<f32>) -> Self {
        let channels = channels.max(1);
        let clean = |s: f32| {
            if s.is_finite() {
                s.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        let samples: Vec<f32> = if channels <= 2 {
            let mut samples = samples;
            samples.truncate(samples.len() - samples.len() % usize::from(channels));
            for sample in &mut samples {
                *sample = clean(*sample);
            }
            samples
        } else {
            samples
                .chunks_exact(usize::from(channels))
                .flat_map(|frame| [clean(frame[0]), clean(frame[1])])
                .collect()
        };
        Self {
            samples: samples.into(),
            channels: channels.min(2),
            sample_rate: sample_rate.max(1),
        }
    }

    /// A mono sound.
    #[must_use]
    pub fn mono(sample_rate: u32, samples: Vec<f32>) -> Self {
        Self::from_samples(1, sample_rate, samples)
    }

    /// Decodes a WAV file, or an OGG Vorbis file with the `ogg` feature, recognized by its
    /// content.
    ///
    /// # Errors
    ///
    /// Returns an error for other formats and for broken files.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.starts_with(b"RIFF") {
            return crate::wav::decode(bytes);
        }
        if bytes.starts_with(b"OggS") {
            #[cfg(feature = "ogg")]
            return crate::ogg::decode(bytes);
            #[cfg(not(feature = "ogg"))]
            return Err(DecodeError::Unsupported(
                "OGG Vorbis needs the `ogg` feature".to_owned(),
            ));
        }
        Err(DecodeError::Unsupported(
            "not a WAV or OGG Vorbis file".to_owned(),
        ))
    }

    /// Reads and decodes a sound file (see [`Sound::decode`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or decoded.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DecodeError> {
        let bytes = std::fs::read(path).map_err(|e| DecodeError::Io(e.to_string()))?;
        Self::decode(&bytes)
    }

    /// Encodes the sound as a 16-bit PCM WAV file.
    #[must_use]
    pub fn to_wav(&self) -> Vec<u8> {
        crate::wav::encode(self)
    }

    /// Interleaved samples.
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// 1 (mono) or 2 (stereo).
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Samples per second of one channel.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of frames (one sample per channel).
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Playing time at normal speed.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "frame counts of sounds fit in f64"
    )]
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.frames() as f64 / f64::from(self.sample_rate))
    }

    /// The frame at `index` as `(left, right)` (mono sounds give the same value twice);
    /// silence past the end.
    #[must_use]
    pub(crate) fn frame(&self, index: usize) -> (f32, f32) {
        if self.channels == 1 {
            let s = self.samples.get(index).copied().unwrap_or(0.0);
            (s, s)
        } else {
            let Some(at) = index.checked_mul(2) else {
                return (0.0, 0.0);
            };
            match (self.samples.get(at), self.samples.get(at + 1)) {
                (Some(&l), Some(&r)) => (l, r),
                _ => (0.0, 0.0),
            }
        }
    }
}

/// Why a sound could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The file could not be read.
    Io(String),
    /// The format (or a variant of it) is not supported.
    Unsupported(String),
    /// The file is broken.
    Invalid(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "cannot read the sound file: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported sound: {message}"),
            Self::Invalid(message) => write!(f, "broken sound file: {message}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_cleaned() {
        let sound = Sound::from_samples(2, 100, vec![0.5, f32::NAN, 2.0, -3.0, 0.1]);
        assert_eq!(sound.samples(), [0.5, 0.0, 1.0, -1.0]);
        assert_eq!((sound.frames(), sound.channels()), (2, 2));
        assert_eq!(sound.frame(1), (1.0, -1.0));
        assert_eq!(sound.frame(9), (0.0, 0.0));
        let surround = Sound::from_samples(4, 100, vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]);
        assert_eq!(surround.samples(), [0.1, 0.2, 0.5, 0.6]);
        let mono = Sound::mono(4, vec![0.25; 6]);
        assert_eq!(mono.frame(2), (0.25, 0.25));
        assert_eq!(mono.duration(), Duration::from_millis(1500));
        assert_eq!(Sound::from_samples(0, 0, vec![]).sample_rate(), 1);
    }

    #[test]
    fn unknown_formats_are_rejected() {
        assert!(matches!(
            Sound::decode(b"ID3 mp3 data"),
            Err(DecodeError::Unsupported(_))
        ));
        assert!(matches!(
            Sound::load("does/not/exist.wav"),
            Err(DecodeError::Io(_))
        ));
    }
}
