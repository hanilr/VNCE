//! Sound with the `audio` feature: effects and music made with `Synth` and `Melody`,
//! played through mixer groups. Without the feature, or without a sound device, the game
//! is silent.

/// Sound effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    Core,
    Hit,
    Door,
    Relay,
    Blip,
    Wrong,
}

/// Number of pads in the sound lab.
pub const PADS: usize = 8;

#[cfg(feature = "audio")]
pub use real::Sounds;

#[cfg(not(feature = "audio"))]
pub use silent::Sounds;

#[cfg(feature = "audio")]
mod real {
    use std::cell::RefCell;
    use std::time::Duration;

    use vnce::vnce_audio::{Audio, Group, Melody, PlayOptions, Preset, Sound, Synth, Voice, Wave};

    use super::{PADS, Sfx};

    pub struct Sounds {
        audio: Option<Audio>,
        effects: Vec<(Sfx, Sound)>,
        pads: Vec<Sound>,
        music: Vec<Sound>,
        playing: RefCell<Vec<Voice>>,
        /// Small variations in pitch for repeated effects.
        variation: std::cell::Cell<u32>,
    }

    impl Sounds {
        /// Opens the default sound device, or stays silent with `device == false`.
        pub fn new(device: bool) -> Self {
            let audio = if device { Audio::new().ok() } else { None };
            let rate = audio.as_ref().map_or(44_100, Audio::sample_rate);
            let effect = |preset, seed| Synth::preset(preset, seed).render(rate);
            let effects = vec![
                (Sfx::Core, effect(Preset::Coin, 11)),
                (Sfx::Hit, effect(Preset::Hit, 2)),
                (Sfx::Door, effect(Preset::Blip, 5)),
                (Sfx::Relay, effect(Preset::PowerUp, 3)),
                (Sfx::Blip, effect(Preset::Blip, 0)),
                (Sfx::Wrong, effect(Preset::Laser, 9)),
            ];
            // A soft bell for the pads: C major pentatonic over two octaves.
            let bell = Synth {
                wave: Wave::Triangle,
                attack: 0.005,
                decay: 0.35,
                sustain: 0.0,
                release: 0.2,
                vibrato_depth: 0.004,
                vibrato_speed: 5.0,
                volume: 0.6,
                ..Synth::default()
            };
            let pads = ["C4", "D4", "E4", "G4", "A4", "C5", "D5", "E5"]
                .iter()
                .map(|name| {
                    let frequency = vnce::vnce_audio::note(name).unwrap_or(440.0);
                    Synth { frequency, ..bell }.render(rate)
                })
                .collect();
            // An original slow loop: a low pulse and a lead, 16 beats each.
            let bass = Synth {
                wave: Wave::Triangle,
                attack: 0.02,
                decay: 0.3,
                sustain: 0.5,
                release: 0.2,
                volume: 0.35,
                low_pass: 900.0,
                ..Synth::default()
            };
            let lead = Synth {
                wave: Wave::Square,
                duty: 0.25,
                attack: 0.03,
                decay: 0.2,
                sustain: 0.4,
                release: 0.3,
                volume: 0.16,
                low_pass: 2200.0,
                vibrato_depth: 0.006,
                vibrato_speed: 4.5,
                ..Synth::default()
            };
            let silent = || Sound::mono(rate, Vec::new());
            let music = vec![
                Melody::new(88.0, bass)
                    .render("A2:4 F2:4 C3:4 G2:4", rate)
                    .unwrap_or_else(|_| silent()),
                Melody::new(88.0, lead)
                    .render("A3:2 E4:1 C4:1 B3:2 G3:2 A3:2 C4:1 E4:1 D4:2 B3:2", rate)
                    .unwrap_or_else(|_| silent()),
            ];
            Self {
                audio,
                effects,
                pads,
                music,
                playing: RefCell::new(Vec::new()),
                variation: std::cell::Cell::new(0),
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

        pub fn play(&self, sfx: Sfx) {
            let Some(audio) = &self.audio else {
                return;
            };
            let Some((_, sound)) = self.effects.iter().find(|(kind, _)| *kind == sfx) else {
                return;
            };
            // Repeated hits and doors vary a little in pitch.
            let step = self.variation.get().wrapping_add(1);
            self.variation.set(step);
            let speed = match sfx {
                Sfx::Hit | Sfx::Door => 0.92 + (step % 5) as f32 * 0.04,
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

        /// Plays pad `index`, panned from left to right across the room.
        pub fn pad(&self, index: usize) {
            if let (Some(audio), Some(sound)) = (&self.audio, self.pads.get(index)) {
                let pan = index as f32 / (PADS - 1) as f32 * 1.6 - 0.8;
                audio.play_with(
                    sound,
                    PlayOptions {
                        pan,
                        ..PlayOptions::default()
                    },
                );
            }
        }

        /// The samples of pad `index`, for the waveform screen.
        pub fn pad_wave(&self, index: usize) -> Option<&[f32]> {
            self.pads.get(index).map(Sound::samples)
        }

        pub fn music(&self, on: bool) {
            let Some(audio) = &self.audio else {
                return;
            };
            let mut playing = self.playing.borrow_mut();
            if on && playing.is_empty() {
                for sound in &self.music {
                    playing.push(audio.play_with(
                        sound,
                        PlayOptions {
                            fade_in: Duration::from_millis(1200),
                            ..PlayOptions::music()
                        },
                    ));
                }
            } else if !on {
                for voice in playing.drain(..) {
                    voice.stop(Duration::from_millis(600));
                }
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
    use super::Sfx;

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

        pub fn pad(&self, _index: usize) {}

        pub fn pad_wave(&self, _index: usize) -> Option<&[f32]> {
            None
        }

        pub fn music(&self, _on: bool) {}

        pub fn volumes(&self, _music: u32, _effects: u32) {}
    }
}
