//! Randomized tests: broken sound files never panic, and the mixer always produces finite
//! samples in range, whatever it is asked to do.

// Test code converts small random numbers freely.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "random values are reduced to small ranges first"
)]

use std::time::Duration;

use vnce_audio::{Group, Mixer, PlayOptions, Sound};

/// A small deterministic random generator (xorshift64*), so failures can be reproduced.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n` (0 when `n` is 0).
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }

    /// Any float, including NaN and infinities, now and then.
    fn wild(&mut self) -> f32 {
        match self.below(8) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => -1e9,
            _ => self.below(4000) as f32 / 1000.0 - 2.0,
        }
    }
}

#[test]
fn broken_wav_files_never_panic() {
    let mut rng = Rng(0x0A0D_10F1);
    let good = Sound::from_samples(
        2,
        44_100,
        (0..200).map(|i| (i as f32 / 100.0).sin()).collect(),
    )
    .to_wav();
    for _ in 0..5_000 {
        let mut bytes = good.clone();
        match rng.below(4) {
            // Cut the file anywhere.
            0 => bytes.truncate(rng.below(bytes.len() as u64) as usize),
            // Change a few bytes, often in the headers.
            1 => {
                for _ in 0..=rng.below(6) {
                    let range = if rng.below(2) == 0 {
                        48
                    } else {
                        bytes.len() as u64
                    };
                    let at = rng.below(range) as usize;
                    bytes[at] = rng.next() as u8;
                }
            }
            // Huge chunk sizes.
            2 => {
                let at = [4_usize, 16, 40][rng.below(3) as usize];
                bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            }
            // Noise after the RIFF header.
            _ => {
                bytes.truncate(12);
                bytes.extend((0..rng.below(200)).map(|_| rng.next() as u8));
            }
        }
        if let Ok(sound) = Sound::decode(&bytes) {
            assert!(
                sound
                    .samples()
                    .iter()
                    .all(|s| s.is_finite() && s.abs() <= 1.0)
            );
            assert!(sound.channels() == 1 || sound.channels() == 2);
        }
    }
}

#[test]
fn the_mixer_always_outputs_finite_samples_in_range() {
    let mut rng = Rng(77);
    let mut mixer = Mixer::new(8_000);
    let sounds: Vec<Sound> = (0..4)
        .map(|n| {
            let samples = (0..rng.below(300)).map(|_| rng.wild()).collect();
            Sound::from_samples(1 + n % 2, 100 + rng.below(96_000) as u32, samples)
        })
        .collect();
    let mut ids = Vec::new();
    let mut out = vec![0.0; 1024];
    for _ in 0..2_000 {
        match rng.below(8) {
            0 | 1 => {
                let options = PlayOptions {
                    volume: rng.wild(),
                    pan: rng.wild(),
                    speed: rng.wild(),
                    looping: rng.below(3) == 0,
                    group: if rng.below(2) == 0 {
                        Group::Music
                    } else {
                        Group::Effects
                    },
                    fade_in: Duration::from_millis(rng.below(50)),
                };
                let sound = &sounds[rng.below(4) as usize];
                ids.push(mixer.play(sound, options));
            }
            2 if !ids.is_empty() => {
                let id = ids[rng.below(ids.len() as u64) as usize];
                mixer.stop(id, Duration::from_millis(rng.below(30)));
            }
            3 if !ids.is_empty() => {
                let id = ids[rng.below(ids.len() as u64) as usize];
                mixer.set_volume(id, rng.wild(), Duration::from_millis(rng.below(30)));
                mixer.set_speed(id, rng.wild());
                mixer.set_pan(id, rng.wild());
            }
            4 => mixer.set_master_volume(rng.wild()),
            5 => mixer.set_group_volume(Group::Music, rng.wild()),
            _ => {}
        }
        let channels = 1 + rng.below(6) as u16;
        let frames = rng.below(200) as usize;
        let len = frames * usize::from(channels);
        mixer.render(&mut out[..len.min(1024)], channels);
        assert!(
            out[..len.min(1024)]
                .iter()
                .all(|s| s.is_finite() && s.abs() <= 1.0),
            "{:?}",
            &out[..len.min(16)]
        );
        assert!(mixer.playing() <= vnce_audio::MAX_VOICES);
    }
}
