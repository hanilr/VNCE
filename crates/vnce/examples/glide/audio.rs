//! Sound with the `audio` feature: an engine hum whose pitch follows the speed
//! (`Voice::set_speed`), wind that grows with it, ring chimes, a crash, and a tune for the
//! menus and one for flying. Without the feature, or without a sound device, the game is
//! silent.

/// Sound effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    Ring,
    Checkpoint,
    Finish,
    Crash,
    Blip,
    Wrong,
    Camera,
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
    Menu,
    Flight,
}

#[cfg(feature = "audio")]
pub use real::Sounds;

#[cfg(not(feature = "audio"))]
pub use silent::Sounds;

#[cfg(feature = "audio")]
mod real {
    use std::cell::{Cell, RefCell};
    use std::f32::consts::TAU;
    use std::time::Duration;

    use vnce::vnce_audio::{Audio, Group, Melody, PlayOptions, Preset, Sound, Synth, Voice, Wave};

    use super::{Mood, Sfx};

    /// One second of engine hum: a buzzing saw and a low sine with a slow throb. Every
    /// part repeats a whole number of times per second, so the loop is seamless.
    fn hum(rate: u32) -> Vec<f32> {
        let mut low = 0.0_f32;
        (0..rate)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let saw = |f: f32| 2.0 * (t * f).fract() - 1.0;
                let raw = saw(110.0) * 0.5 + saw(165.0) * 0.18 + (t * 55.0 * TAU).sin() * 0.6;
                // A gentle low-pass takes the edge off the saw.
                low += (raw - low) * 0.18;
                let throb = 0.85 + 0.15 * (t * 12.0 * TAU).sin();
                low * throb * 0.4
            })
            .collect()
    }

    /// Two seconds of wind: smoothed noise that swells a little.
    fn wind(rate: u32) -> Vec<f32> {
        let count = rate as usize * 2;
        let mut state = 0x2545_F491_u32;
        let (mut smooth, mut slow) = (0.0_f32, 0.0_f32);
        (0..count)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let white = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                smooth += (white - smooth) * 0.08;
                slow += (white - slow) * 0.01;
                let t = i as f32 / rate as f32;
                let swell = 0.7 + 0.3 * (t * TAU * 0.5).sin();
                // Fade the ends so the loop has no click.
                let edge = (t.min(2.0 - t) / 0.05).min(1.0);
                (smooth * 0.8 + slow * 3.0) * swell * edge * 0.6
            })
            .collect()
    }

    pub struct Sounds {
        audio: Option<Audio>,
        effects: Vec<(Sfx, Sound)>,
        hum: Sound,
        wind: Sound,
        menu: Vec<Sound>,
        flight: Vec<Sound>,
        mood: Cell<Mood>,
        music: RefCell<Vec<Voice>>,
        engine: RefCell<Option<(Voice, Voice)>>,
    }

    impl Sounds {
        /// Opens the default sound device, or stays silent with `device == false`.
        #[allow(clippy::too_many_lines, reason = "every sound is designed here")]
        pub fn new(device: bool) -> Self {
            let audio = if device { Audio::new().ok() } else { None };
            let rate = audio.as_ref().map_or(44_100, Audio::sample_rate);
            let effect = |preset, seed| Synth::preset(preset, seed).render(rate);
            let chime = |frequency: f32, volume: f32| {
                Synth {
                    wave: Wave::Triangle,
                    frequency,
                    jump_time: 0.06,
                    jump_ratio: 1.5,
                    attack: 0.003,
                    decay: 0.45,
                    sustain: 0.0,
                    release: 0.3,
                    vibrato_depth: 0.003,
                    vibrato_speed: 6.0,
                    volume,
                    ..Synth::default()
                }
                .render(rate)
            };
            let crash = Synth {
                frequency: 60.0,
                slide: -0.25,
                low_pass: 700.0,
                decay: 1.2,
                release: 0.8,
                volume: 0.8,
                ..Synth::preset(Preset::Explosion, 9)
            };
            let effects = vec![
                (Sfx::Ring, chime(987.8, 0.5)),
                (Sfx::Checkpoint, chime(1318.5, 0.6)),
                (Sfx::Finish, effect(Preset::PowerUp, 5)),
                (Sfx::Crash, crash.render(rate)),
                (Sfx::Blip, effect(Preset::Blip, 0)),
                (Sfx::Wrong, effect(Preset::Hit, 1)),
                (Sfx::Camera, effect(Preset::Blip, 7)),
            ];
            let silent = || Sound::mono(rate, Vec::new());
            let tune = |synth: Synth, tempo: f32, notes: &str| {
                Melody::new(tempo, synth)
                    .render(notes, rate)
                    .unwrap_or_else(|_| silent())
            };
            let pad = Synth {
                wave: Wave::Saw,
                attack: 0.8,
                decay: 0.6,
                sustain: 0.6,
                release: 1.0,
                low_pass: 900.0,
                vibrato_depth: 0.004,
                vibrato_speed: 2.5,
                volume: 0.12,
                ..Synth::default()
            };
            let bell = Synth {
                wave: Wave::Triangle,
                attack: 0.005,
                decay: 0.6,
                sustain: 0.1,
                release: 0.5,
                volume: 0.22,
                ..Synth::default()
            };
            let bass = Synth {
                wave: Wave::Sine,
                attack: 0.03,
                decay: 0.4,
                sustain: 0.5,
                release: 0.4,
                volume: 0.3,
                ..Synth::default()
            };
            // Original tunes: a wide, open one for the menus and a light, moving one for
            // flying.
            let menu = vec![
                tune(pad, 72.0, "D3:4 B2:4 G2:4 A2:4"),
                tune(bell, 72.0, "F#4:2 A4:2 D5:3 C#5:1 B4:2 G4:2 A4:4"),
            ];
            let flight = vec![
                tune(bass, 104.0, "E2:2 E2:2 C2:2 C2:2 G2:2 G2:2 D2:2 D2:2"),
                tune(
                    bell,
                    104.0,
                    "B4:1 E5:1 G5:2 F#5:1 E5:1 D5:2 B4:1 D5:1 E5:4 G4:2",
                ),
            ];
            Self {
                hum: Sound::mono(rate, hum(rate)),
                wind: Sound::mono(rate, wind(rate)),
                audio,
                effects,
                menu,
                flight,
                mood: Cell::new(Mood::Off),
                music: RefCell::new(Vec::new()),
                engine: RefCell::new(None),
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
            if let Some((_, sound)) = self.effects.iter().find(|(kind, _)| *kind == sfx) {
                audio.play(sound);
            }
        }

        /// Starts or stops the engine and the wind.
        pub fn engine(&self, on: bool) {
            let Some(audio) = &self.audio else {
                return;
            };
            let mut engine = self.engine.borrow_mut();
            if on && engine.is_none() {
                let looped = |sound: &Sound, volume: f32| {
                    audio.play_with(
                        sound,
                        PlayOptions {
                            looping: true,
                            volume,
                            fade_in: Duration::from_millis(600),
                            ..PlayOptions::default()
                        },
                    )
                };
                *engine = Some((looped(&self.hum, 0.45), looped(&self.wind, 0.0)));
            } else if !on {
                if let Some((hum, wind)) = engine.take() {
                    hum.stop(Duration::from_millis(400));
                    wind.stop(Duration::from_millis(400));
                }
            }
        }

        /// The engine's pitch and the wind's strength follow `speed` (0 slow, 1 fast).
        pub fn flight(&self, speed: f32) {
            if let Some((hum, wind)) = self.engine.borrow().as_ref() {
                let speed = speed.clamp(0.0, 1.2);
                hum.set_speed(0.7 + speed * 0.75);
                wind.set_volume(speed * speed * 0.55, Duration::from_millis(100));
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
                voice.stop(Duration::from_millis(1200));
            }
            let tracks = match mood {
                Mood::Off => return,
                Mood::Menu => &self.menu,
                Mood::Flight => &self.flight,
            };
            for sound in tracks {
                playing.push(audio.play_with(
                    sound,
                    PlayOptions {
                        fade_in: Duration::from_millis(1500),
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

        pub fn engine(&self, _on: bool) {}

        pub fn flight(&self, _speed: f32) {}

        pub fn music(&self, _mood: Mood) {}

        pub fn volumes(&self, _music: u32, _effects: u32) {}
    }
}
