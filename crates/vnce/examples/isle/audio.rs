//! Sound with the `audio` feature: effects, footsteps that depend on the ground, waves and
//! rain made from shaped noise, and a day and a night tune, played through the mixer's
//! effect and music groups. Without the feature, or without a sound device, the game is
//! silent.

/// Sound effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    Crystal,
    Beacon,
    Blip,
    Pinch,
    Drain,
    Thunder,
    Wrong,
}

/// Which tune plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    /// No tune (only the sound version starts with it).
    #[cfg_attr(
        not(feature = "audio"),
        allow(dead_code, reason = "the silent version plays nothing")
    )]
    Off,
    Day,
    Night,
}

#[cfg(feature = "audio")]
pub use real::Sounds;

#[cfg(not(feature = "audio"))]
pub use silent::Sounds;

/// Noise shaped into a looping sound: `brown` mixes in low rumble, `swell` is the period
/// of a slow rise and fall in seconds (0: steady).
#[cfg(feature = "audio")]
fn shaped_noise(rate: u32, seconds: f32, brown: f32, swell: f32, seed: u32) -> Vec<f32> {
    let count = (rate as f32 * seconds) as usize;
    let mut state = seed.max(1);
    let mut low = 0.0_f32;
    let mut smooth = 0.0_f32;
    (0..count)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let white = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            low = (low + white * 0.02).clamp(-1.0, 1.0) * 0.995;
            smooth += (white - smooth) * 0.25;
            let sample = smooth * (1.0 - brown) + low * brown * 3.0;
            let t = i as f32 / rate as f32;
            let level = if swell > 0.0 {
                0.55 + 0.45 * (t / swell * std::f32::consts::TAU).sin()
            } else {
                1.0
            };
            // Fade the ends so the loop has no click.
            let edge = (t.min(seconds - t) / 0.05).min(1.0);
            sample * level * edge * 0.5
        })
        .collect()
}

#[cfg(feature = "audio")]
mod real {
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    use vnce::vnce_audio::{Audio, Group, Melody, PlayOptions, Preset, Sound, Synth, Voice, Wave};

    use super::{Mood, Sfx, shaped_noise};
    use crate::island::Ground;

    pub struct Sounds {
        audio: Option<Audio>,
        effects: Vec<(Sfx, Sound)>,
        /// Footsteps: sand, grass, rock, stairs.
        steps: [Sound; 4],
        waves: Sound,
        rain: Sound,
        day: Vec<Sound>,
        night: Vec<Sound>,
        mood: Cell<Mood>,
        music: RefCell<Vec<Voice>>,
        ambience: RefCell<Option<(Voice, Voice)>>,
        /// Small variations in pitch for repeated sounds.
        variation: Cell<u32>,
    }

    impl Sounds {
        /// Opens the default sound device, or stays silent with `device == false`.
        #[allow(clippy::too_many_lines, reason = "every sound is designed here")]
        pub fn new(device: bool) -> Self {
            let audio = if device { Audio::new().ok() } else { None };
            let rate = audio.as_ref().map_or(44_100, Audio::sample_rate);
            let effect = |preset, seed| Synth::preset(preset, seed).render(rate);
            let chime = Synth {
                wave: Wave::Triangle,
                frequency: 1318.5,
                jump_time: 0.07,
                jump_ratio: 1.5,
                attack: 0.003,
                decay: 0.5,
                sustain: 0.0,
                release: 0.3,
                vibrato_depth: 0.003,
                vibrato_speed: 6.0,
                volume: 0.55,
                ..Synth::default()
            };
            let thunder = Synth {
                frequency: 70.0,
                slide: -0.2,
                low_pass: 400.0,
                decay: 1.8,
                release: 1.0,
                volume: 0.7,
                ..Synth::preset(Preset::Explosion, 4)
            };
            let effects = vec![
                (Sfx::Crystal, chime.render(rate)),
                (Sfx::Beacon, effect(Preset::PowerUp, 3)),
                (Sfx::Blip, effect(Preset::Blip, 0)),
                (Sfx::Pinch, effect(Preset::Hit, 6)),
                (Sfx::Drain, effect(Preset::Laser, 12)),
                (Sfx::Thunder, thunder.render(rate)),
                (Sfx::Wrong, effect(Preset::Hit, 1)),
            ];
            // Footsteps: short noise bursts, darker or brighter by ground.
            let step = |frequency: f32, low_pass: f32, decay: f32, volume: f32| {
                Synth {
                    wave: Wave::Noise,
                    frequency,
                    attack: 0.002,
                    decay,
                    sustain: 0.0,
                    release: 0.02,
                    low_pass,
                    volume,
                    ..Synth::default()
                }
                .render(rate)
            };
            let steps = [
                step(900.0, 2500.0, 0.06, 0.18),
                step(500.0, 1400.0, 0.05, 0.16),
                step(2400.0, 5000.0, 0.03, 0.2),
                step(300.0, 900.0, 0.07, 0.24),
            ];
            let waves = Sound::mono(rate, shaped_noise(rate, 12.0, 0.7, 6.0, 7));
            let rain = Sound::mono(rate, shaped_noise(rate, 4.0, 0.1, 0.0, 99));
            let silent = || Sound::mono(rate, Vec::new());
            let tune = |synth: Synth, tempo: f32, notes: &str| {
                Melody::new(tempo, synth)
                    .render(notes, rate)
                    .unwrap_or_else(|_| silent())
            };
            let pluck = Synth {
                wave: Wave::Triangle,
                attack: 0.005,
                decay: 0.25,
                sustain: 0.2,
                release: 0.25,
                volume: 0.28,
                ..Synth::default()
            };
            let bass = Synth {
                wave: Wave::Sine,
                attack: 0.02,
                decay: 0.4,
                sustain: 0.5,
                release: 0.3,
                volume: 0.32,
                ..Synth::default()
            };
            let pad = Synth {
                wave: Wave::Saw,
                attack: 0.6,
                decay: 0.5,
                sustain: 0.6,
                release: 0.8,
                low_pass: 700.0,
                vibrato_depth: 0.004,
                vibrato_speed: 3.0,
                volume: 0.12,
                ..Synth::default()
            };
            // Original tunes: a light island melody, and a slow, darker one for the night.
            let day = vec![
                tune(bass, 96.0, "C3:4 A2:4 F2:4 G2:4"),
                tune(
                    pluck,
                    96.0,
                    "E4:1 G4:1 C5:2 B4:1 G4:1 A4:2 F4:1 A4:1 C5:1 A4:1 G4:2 D4:1 E4:1",
                ),
            ];
            let night = vec![
                tune(bass, 64.0, "A2:4 F2:4 D2:4 E2:4"),
                tune(pad, 64.0, "A3:4 C4:4 F3:4 E3:4"),
            ];
            Self {
                audio,
                effects,
                steps,
                waves,
                rain,
                day,
                night,
                mood: Cell::new(Mood::Off),
                music: RefCell::new(Vec::new()),
                ambience: RefCell::new(None),
                variation: Cell::new(0),
            }
        }

        pub fn available(&self) -> bool {
            self.audio.is_some()
        }

        pub fn device(&self) -> String {
            self.audio
                .as_ref()
                .map_or_else(|| "no device".to_owned(), |a| a.device_name().to_owned())
        }

        /// A little different each time, for sounds that repeat.
        fn vary(&self, amount: f32) -> f32 {
            let step = self.variation.get().wrapping_add(1);
            self.variation.set(step);
            1.0 - amount + (step % 5) as f32 * amount / 2.0
        }

        pub fn play(&self, sfx: Sfx) {
            let Some(audio) = &self.audio else {
                return;
            };
            let Some((_, sound)) = self.effects.iter().find(|(kind, _)| *kind == sfx) else {
                return;
            };
            let speed = match sfx {
                Sfx::Pinch | Sfx::Drain => self.vary(0.08),
                _ => 1.0,
            };
            audio.play_with(
                sound,
                PlayOptions {
                    speed,
                    ..PlayOptions::default()
                },
            );
        }

        /// A footstep on `ground` (stairs when `stair`), at stereo position `pan`.
        pub fn step(&self, ground: Ground, stair: bool, pan: f32) {
            let Some(audio) = &self.audio else {
                return;
            };
            let index = if stair {
                3
            } else {
                match ground {
                    Ground::Sand | Ground::Water => 0,
                    Ground::Grass | Ground::Meadow => 1,
                    Ground::Rock => 2,
                }
            };
            audio.play_with(
                &self.steps[index],
                PlayOptions {
                    speed: self.vary(0.1),
                    pan,
                    ..PlayOptions::default()
                },
            );
        }

        /// Starts or stops the waves and the (silent until it rains) rain loop.
        pub fn ambience(&self, on: bool) {
            let Some(audio) = &self.audio else {
                return;
            };
            let mut ambience = self.ambience.borrow_mut();
            if on && ambience.is_none() {
                let looped = |sound: &Sound, volume: f32| {
                    audio.play_with(
                        sound,
                        PlayOptions {
                            looping: true,
                            volume,
                            fade_in: Duration::from_millis(1500),
                            ..PlayOptions::default()
                        },
                    )
                };
                *ambience = Some((looped(&self.waves, 0.5), looped(&self.rain, 0.0)));
            } else if !on {
                if let Some((waves, rain)) = ambience.take() {
                    waves.stop(Duration::from_millis(800));
                    rain.stop(Duration::from_millis(800));
                }
            }
        }

        /// How hard it rains, 0 to 1.
        pub fn rain(&self, intensity: f32) {
            if let Some((_, rain)) = self.ambience.borrow().as_ref() {
                rain.set_volume(intensity * 0.6, Duration::from_millis(200));
            }
        }

        pub fn music(&self, mood: Mood) {
            let Some(audio) = &self.audio else {
                return;
            };
            if self.mood.get() == mood {
                return;
            }
            self.mood.set(mood);
            let mut playing = self.music.borrow_mut();
            for voice in playing.drain(..) {
                voice.stop(Duration::from_millis(1500));
            }
            let tracks = match mood {
                Mood::Off => return,
                Mood::Day => &self.day,
                Mood::Night => &self.night,
            };
            for sound in tracks {
                playing.push(audio.play_with(
                    sound,
                    PlayOptions {
                        fade_in: Duration::from_millis(2000),
                        ..PlayOptions::music()
                    },
                ));
            }
        }

        pub fn volumes(&self, music: u32, effects: u32) {
            if let Some(audio) = &self.audio {
                audio.set_group_volume(Group::Music, music as f32 / 100.0);
                audio.set_group_volume(Group::Effects, effects as f32 / 100.0);
            }
        }
    }
}

#[cfg(not(feature = "audio"))]
mod silent {
    use super::{Mood, Sfx};
    use crate::island::Ground;

    /// The silent stand-in: the same methods, no sound.
    pub struct Sounds;

    #[allow(
        clippy::unused_self,
        clippy::needless_pass_by_value,
        reason = "same methods as the audio version"
    )]
    impl Sounds {
        pub fn new(_device: bool) -> Self {
            Self
        }

        pub fn available(&self) -> bool {
            false
        }

        pub fn device(&self) -> String {
            "build with --features audio".to_owned()
        }

        pub fn play(&self, _sfx: Sfx) {}

        pub fn step(&self, _ground: Ground, _stair: bool, _pan: f32) {}

        pub fn ambience(&self, _on: bool) {}

        pub fn rain(&self, _intensity: f32) {}

        pub fn music(&self, _mood: Mood) {}

        pub fn volumes(&self, _music: u32, _effects: u32) {}
    }
}
