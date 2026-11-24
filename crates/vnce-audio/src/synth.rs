//! Sounds made in code: [`Synth`] (retro sound effects), [`Preset`]s and [`Melody`].

use std::fmt;

use crate::sound::Sound;

/// The shape of the sound wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Wave {
    /// Hollow, "8-bit" (the width is set by [`Synth::duty`]).
    #[default]
    Square,
    /// Bright and buzzy.
    Saw,
    /// Soft, flute-like.
    Triangle,
    /// Pure tone.
    Sine,
    /// Hiss and crunch (explosions, hits); the frequency sets how coarse it is.
    Noise,
}

/// A sound effect generator in the spirit of classic retro sound tools: one wave with an
/// envelope, pitch slides, a pitch jump, vibrato and a low-pass filter. The same settings
/// always give the same sound.
///
/// ```
/// use vnce_audio::{Preset, Synth, Wave};
///
/// let coin = Synth::preset(Preset::Coin, 0).render(44_100);
/// let custom = Synth {
///     wave: Wave::Triangle,
///     frequency: 220.0,
///     slide: 1.0, // one octave up per second
///     ..Synth::default()
/// }
/// .render(44_100);
/// assert!(coin.duration().as_secs_f32() < 0.5 && custom.frames() > 0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Synth {
    /// The wave shape.
    pub wave: Wave,
    /// Starting pitch in Hz.
    pub frequency: f32,
    /// Pitch change in octaves per second (negative goes down).
    pub slide: f32,
    /// After this many seconds, multiply the pitch by the ratio (`0.0` = never), for the
    /// two-step "coin" sound.
    pub jump_time: f32,
    /// Pitch ratio of the jump, for example `4.0 / 3.0` for a fourth up.
    pub jump_ratio: f32,
    /// Vibrato depth in semitones.
    pub vibrato_depth: f32,
    /// Vibrato speed in Hz.
    pub vibrato_speed: f32,
    /// Width of the square wave, `0.5` is a plain square, small values sound thinner.
    pub duty: f32,
    /// Change of the width per second.
    pub duty_slide: f32,
    /// Seconds from silence to full volume.
    pub attack: f32,
    /// Seconds from full volume down to the sustain level.
    pub decay: f32,
    /// Level held after the decay, `0.0` to `1.0`.
    pub sustain: f32,
    /// Seconds the sustain level is held.
    pub hold: f32,
    /// Seconds from the sustain level to silence.
    pub release: f32,
    /// Low-pass filter cutoff in Hz (`0.0` = off): smaller values sound duller.
    pub low_pass: f32,
    /// Overall volume, `0.0` to `1.0`.
    pub volume: f32,
    /// Seed of the noise wave.
    pub seed: u64,
}

impl Default for Synth {
    fn default() -> Self {
        Self {
            wave: Wave::Square,
            frequency: 440.0,
            slide: 0.0,
            jump_time: 0.0,
            jump_ratio: 1.0,
            vibrato_depth: 0.0,
            vibrato_speed: 0.0,
            duty: 0.5,
            duty_slide: 0.0,
            attack: 0.005,
            decay: 0.05,
            sustain: 0.6,
            hold: 0.1,
            release: 0.1,
            low_pass: 0.0,
            volume: 0.5,
            seed: 1,
        }
    }
}

/// Ready-made sound effects for [`Synth::preset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// A rising "boing".
    Jump,
    /// A bright two-step chime.
    Coin,
    /// A short crunchy thud.
    Hit,
    /// A long rumbling blast.
    Explosion,
    /// A rising, wobbling tone.
    PowerUp,
    /// A falling zap.
    Laser,
    /// A short menu beep.
    Blip,
}

impl Preset {
    /// Every preset.
    pub const ALL: [Self; 7] = [
        Self::Jump,
        Self::Coin,
        Self::Hit,
        Self::Explosion,
        Self::PowerUp,
        Self::Laser,
        Self::Blip,
    ];

    /// The name of the preset.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Jump => "jump",
            Self::Coin => "coin",
            Self::Hit => "hit",
            Self::Explosion => "explosion",
            Self::PowerUp => "power-up",
            Self::Laser => "laser",
            Self::Blip => "blip",
        }
    }
}

/// A small deterministic random generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A float in `-1.0..1.0`.
    #[allow(
        clippy::cast_precision_loss,
        reason = "24 random bits are exact in f32"
    )]
    fn signed(&mut self) -> f32 {
        ((self.next() >> 40) as f32 / (1_u64 << 24) as f32) * 2.0 - 1.0
    }
}

impl Synth {
    /// A preset effect. Seed `0` gives the preset exactly; other seeds vary it slightly, so
    /// repeated sounds (for example many coins) do not all sound the same.
    #[must_use]
    pub fn preset(preset: Preset, seed: u64) -> Self {
        let base = Self::default();
        let mut synth = match preset {
            Preset::Jump => Self {
                frequency: 300.0,
                slide: 1.8,
                decay: 0.04,
                sustain: 0.5,
                hold: 0.08,
                release: 0.12,
                ..base
            },
            Preset::Coin => Self {
                frequency: 988.0,
                duty: 0.25,
                jump_time: 0.06,
                jump_ratio: 4.0 / 3.0,
                decay: 0.02,
                sustain: 0.7,
                hold: 0.12,
                release: 0.18,
                ..base
            },
            Preset::Hit => Self {
                wave: Wave::Noise,
                frequency: 1800.0,
                slide: -2.5,
                attack: 0.0,
                decay: 0.08,
                sustain: 0.2,
                hold: 0.02,
                release: 0.08,
                low_pass: 3500.0,
                volume: 0.6,
                ..base
            },
            Preset::Explosion => Self {
                wave: Wave::Noise,
                frequency: 600.0,
                slide: -1.2,
                attack: 0.0,
                decay: 0.3,
                sustain: 0.35,
                hold: 0.25,
                release: 0.6,
                low_pass: 1600.0,
                volume: 0.7,
                ..base
            },
            Preset::PowerUp => Self {
                frequency: 330.0,
                slide: 1.2,
                vibrato_depth: 0.7,
                vibrato_speed: 14.0,
                duty: 0.4,
                duty_slide: -0.4,
                hold: 0.3,
                release: 0.2,
                ..base
            },
            Preset::Laser => Self {
                wave: Wave::Saw,
                frequency: 1500.0,
                slide: -3.5,
                attack: 0.0,
                decay: 0.05,
                sustain: 0.5,
                hold: 0.08,
                release: 0.12,
                low_pass: 6000.0,
                volume: 0.4,
                ..base
            },
            Preset::Blip => Self {
                frequency: 1200.0,
                attack: 0.002,
                decay: 0.01,
                sustain: 0.8,
                hold: 0.03,
                release: 0.04,
                volume: 0.35,
                ..base
            },
        };
        synth.seed = seed.wrapping_add(1);
        if seed != 0 {
            let mut rng = Rng::new(seed);
            synth.frequency *= 1.0 + 0.08 * rng.signed();
            synth.slide *= 1.0 + 0.15 * rng.signed();
            synth.hold *= 1.0 + 0.2 * rng.signed();
            synth.release *= 1.0 + 0.2 * rng.signed();
        }
        synth
    }

    /// Total length in seconds (attack, decay, hold and release).
    #[must_use]
    pub fn length(&self) -> f32 {
        [self.attack, self.decay, self.hold, self.release]
            .iter()
            .map(|t| if t.is_finite() { t.max(0.0) } else { 0.0 })
            .sum::<f32>()
            .min(60.0)
    }

    /// The envelope level `t` seconds after the start.
    fn envelope(&self, t: f32) -> f32 {
        let clean = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let (attack, decay, hold, release) = (
            clean(self.attack),
            clean(self.decay),
            clean(self.hold),
            clean(self.release),
        );
        let sustain = clean(self.sustain).min(1.0);
        if t < attack {
            t / attack
        } else if t < attack + decay {
            1.0 - (1.0 - sustain) * (t - attack) / decay
        } else if t < attack + decay + hold {
            sustain
        } else if t < attack + decay + hold + release {
            sustain * (1.0 - (t - attack - decay - hold) / release)
        } else {
            0.0
        }
    }

    /// Generates the sound (mono) at `sample_rate`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "lengths are at most 60 s of samples"
    )]
    pub fn render(&self, sample_rate: u32) -> Sound {
        let rate = sample_rate.max(1) as f32;
        let frames = (self.length() * rate).ceil() as usize;
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let base = finite(self.frequency, 440.0).clamp(1.0, 20_000.0);
        let volume = finite(self.volume, 0.5).clamp(0.0, 1.0);
        let mut rng = Rng::new(self.seed);
        let mut noise = rng.signed();
        let mut phase = 0.0_f32;
        let mut filtered = 0.0_f32;
        let cutoff = finite(self.low_pass, 0.0);
        // One-pole low-pass coefficient.
        let smoothing = if cutoff > 0.0 {
            1.0 - (-2.0 * std::f32::consts::PI * cutoff / rate).exp()
        } else {
            1.0
        };
        let mut samples = Vec::with_capacity(frames);
        for n in 0..frames {
            let t = n as f32 / rate;
            let mut frequency = base * 2_f32.powf(finite(self.slide, 0.0) * t);
            if self.jump_time > 0.0 && t >= self.jump_time {
                frequency *= finite(self.jump_ratio, 1.0);
            }
            if self.vibrato_depth != 0.0 {
                let wobble =
                    (2.0 * std::f32::consts::PI * finite(self.vibrato_speed, 0.0) * t).sin();
                frequency *= 2_f32.powf(finite(self.vibrato_depth, 0.0) * wobble / 12.0);
            }
            let frequency = frequency.clamp(1.0, rate / 2.0);
            let previous = phase;
            phase = (phase + frequency / rate).fract();
            let duty =
                (finite(self.duty, 0.5) + finite(self.duty_slide, 0.0) * t).clamp(0.05, 0.95);
            let value = match self.wave {
                Wave::Square => {
                    if phase < duty {
                        1.0
                    } else {
                        -1.0
                    }
                }
                Wave::Saw => phase * 2.0 - 1.0,
                Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
                Wave::Sine => (phase * 2.0 * std::f32::consts::PI).sin(),
                Wave::Noise => {
                    // A new random value 16 times per period.
                    let segment = |p: f32| (p * 16.0) as u8;
                    if segment(phase) != segment(previous) {
                        noise = rng.signed();
                    }
                    noise
                }
            };
            filtered += (value - filtered) * smoothing;
            samples.push(filtered * self.envelope(t) * volume);
        }
        Sound::mono(sample_rate, samples)
    }
}

/// The frequency of a note name such as `A4` (440 Hz), `C#5` or `Eb3`, or `None` if the
/// name is not a note.
///
/// ```
/// assert_eq!(vnce_audio::note("A4"), Some(440.0));
/// assert!((vnce_audio::note("C5").unwrap() - 523.25).abs() < 0.01);
/// ```
#[must_use]
pub fn note(name: &str) -> Option<f32> {
    let mut chars = name.chars();
    let semitone = match chars.next()?.to_ascii_uppercase() {
        'C' => -9,
        'D' => -7,
        'E' => -5,
        'F' => -4,
        'G' => -2,
        'A' => 0,
        'B' => 2,
        _ => return None,
    };
    let rest = chars.as_str();
    let (accidental, octave) = match rest.chars().next()? {
        '#' => (1, &rest[1..]),
        'b' => (-1, &rest[1..]),
        _ => (0, rest),
    };
    let octave: i32 = octave.parse().ok().filter(|o| (0..=9).contains(o))?;
    let steps = semitone + accidental + (octave - 4) * 12;
    #[allow(clippy::cast_precision_loss, reason = "a few octaves of semitones")]
    let steps = steps as f32;
    Some(440.0 * 2_f32.powf(steps / 12.0))
}

/// A short tune played by one [`Synth`] voice, for music loops and jingles.
///
/// Notes are separated by spaces: a note name (`C4`, `F#3`, `Bb5`) or `-` for a rest,
/// optionally followed by `:beats` (default 1), for example `"C4 E4 G4:2 - C5"`.
///
/// ```
/// use vnce_audio::{Melody, Synth, Wave};
///
/// let tune = Melody::new(120.0, Synth { wave: Wave::Triangle, ..Synth::default() });
/// let sound = tune.render("C4 E4 G4:2", 44_100).unwrap();
/// assert!((sound.duration().as_secs_f32() - 2.0).abs() < 0.01); // 4 beats at 120 BPM
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Melody {
    /// Beats per minute.
    pub tempo: f32,
    /// The voice: its wave, envelope shape and effects (frequency and hold are set per
    /// note).
    pub voice: Synth,
}

/// A note that [`Melody::render`] could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseNoteError(pub String);

impl fmt::Display for ParseNoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a note: {:?}", self.0)
    }
}

impl std::error::Error for ParseNoteError {}

impl Melody {
    /// A melody played at `tempo` beats per minute by `voice`.
    #[must_use]
    pub fn new(tempo: f32, voice: Synth) -> Self {
        Self { tempo, voice }
    }

    /// Renders `notes` (see [`Melody`]) at `sample_rate`. Every note takes exactly its
    /// beats, so the sound loops seamlessly.
    ///
    /// # Errors
    ///
    /// Returns the first token that is neither a note, a rest nor a valid length.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "tunes are short"
    )]
    pub fn render(&self, notes: &str, sample_rate: u32) -> Result<Sound, ParseNoteError> {
        let tempo = if self.tempo.is_finite() && self.tempo > 0.0 {
            self.tempo
        } else {
            120.0
        };
        let beat = 60.0 / tempo;
        let rate = sample_rate.max(1);
        let mut samples: Vec<f32> = Vec::new();
        for token in notes.split_whitespace() {
            let error = || ParseNoteError(token.to_owned());
            let (name, beats) = match token.split_once(':') {
                Some((name, beats)) => (name, beats.parse::<f32>().map_err(|_| error())?),
                None => (token, 1.0),
            };
            if !(beats.is_finite() && beats > 0.0 && beats <= 64.0) {
                return Err(error());
            }
            let length = (beats * beat * rate as f32).round() as usize;
            let start = samples.len();
            samples.resize(start + length, 0.0);
            if name == "-" {
                continue;
            }
            let frequency = note(name).ok_or_else(error)?;
            let seconds = length as f32 / rate as f32;
            let mut voice = self.voice;
            voice.frequency = frequency;
            // Fit the envelope into the note: shorten the release first, then the hold.
            let fixed = voice.attack.max(0.0) + voice.decay.max(0.0);
            voice.release = voice.release.max(0.0).min((seconds - fixed).max(0.0));
            voice.hold = (seconds - fixed - voice.release).max(0.0);
            let rendered = voice.render(rate);
            for (out, value) in samples[start..].iter_mut().zip(rendered.samples()) {
                *out = *value;
            }
        }
        Ok(Sound::mono(rate, samples))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 8_000;

    #[test]
    fn every_preset_is_short_finite_and_in_range() {
        for preset in Preset::ALL {
            for seed in [0, 1, 99] {
                let sound = Synth::preset(preset, seed).render(RATE);
                let seconds = sound.duration().as_secs_f32();
                assert!(
                    seconds > 0.05 && seconds < 1.5,
                    "{}: {seconds}",
                    preset.name()
                );
                let peak = sound.samples().iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                assert!(peak > 0.05 && peak <= 1.0, "{}: peak {peak}", preset.name());
                assert!(sound.samples().iter().all(|s| s.is_finite()));
                // It fades to silence.
                assert!(sound.samples().last().unwrap().abs() < 0.05);
            }
        }
    }

    #[test]
    fn seeds_are_deterministic_and_vary() {
        let a = Synth::preset(Preset::Coin, 7).render(RATE);
        let b = Synth::preset(Preset::Coin, 7).render(RATE);
        let c = Synth::preset(Preset::Coin, 8).render(RATE);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(Synth::preset(Preset::Coin, 0).frequency, 988.0);
    }

    #[test]
    fn envelope_shape() {
        let synth = Synth {
            attack: 0.1,
            decay: 0.1,
            sustain: 0.5,
            hold: 0.1,
            release: 0.1,
            ..Synth::default()
        };
        assert!((synth.length() - 0.4).abs() < 1e-6);
        let points = [
            (0.05, 0.5),
            (0.1, 1.0),
            (0.15, 0.75),
            (0.25, 0.5),
            (0.35, 0.25),
            (0.5, 0.0),
        ];
        for (t, level) in points {
            assert!(
                (synth.envelope(t) - level).abs() < 1e-4,
                "{t}: {}",
                synth.envelope(t)
            );
        }
    }

    #[test]
    fn waves_have_the_right_pitch() {
        // Count rising zero crossings of one second of a 100 Hz wave.
        for wave in [Wave::Square, Wave::Saw, Wave::Triangle, Wave::Sine] {
            let sound = Synth {
                wave,
                frequency: 100.0,
                attack: 0.0,
                decay: 0.0,
                sustain: 1.0,
                hold: 1.0,
                release: 0.0,
                ..Synth::default()
            }
            .render(RATE);
            let crossings = sound
                .samples()
                .windows(2)
                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                .count();
            assert!((99..=101).contains(&crossings), "{wave:?}: {crossings}");
        }
    }

    #[test]
    fn wild_settings_stay_safe() {
        let wild = Synth {
            frequency: f32::NAN,
            slide: 1e9,
            jump_time: 0.01,
            jump_ratio: f32::INFINITY,
            vibrato_depth: 50.0,
            vibrato_speed: f32::NAN,
            duty: -3.0,
            attack: f32::INFINITY,
            hold: 1e6,
            low_pass: -5.0,
            volume: 7.0,
            ..Synth::default()
        };
        let sound = wild.render(RATE);
        assert!(sound.duration().as_secs_f32() <= 60.01);
        assert!(
            sound
                .samples()
                .iter()
                .all(|s| s.is_finite() && s.abs() <= 1.0)
        );
    }

    #[test]
    fn notes_and_melodies() {
        assert_eq!(note("A4"), Some(440.0));
        assert!((note("a3").unwrap() - 220.0).abs() < 1e-3);
        assert!((note("C#4").unwrap() - note("Db4").unwrap()).abs() < 1e-3);
        for bad in ["H4", "A", "A10", "C##4", ""] {
            assert_eq!(note(bad), None, "{bad}");
        }
        let melody = Melody::new(240.0, Synth::default());
        let sound = melody.render("C4 - E4:2 G4:0.5", RATE).unwrap();
        // 4.5 beats at 240 BPM = 1.125 s.
        assert_eq!(sound.frames(), 9_000);
        // The rest is silent.
        assert!(sound.samples()[2_100..3_900].iter().all(|s| *s == 0.0));
        assert_eq!(
            melody.render("C4 X9", RATE),
            Err(ParseNoteError("X9".to_owned()))
        );
        assert!(melody.render("C4:0", RATE).is_err());
        assert!(melody.render("C4:abc", RATE).is_err());
        assert_eq!(melody.render("", RATE).unwrap().frames(), 0);
    }
}
