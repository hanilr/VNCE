//! The real audio output. Skipped (not failed) on machines without an audio device. Only
//! silence is played, so running the tests makes no noise.

#![cfg(feature = "output")]

use std::thread::sleep;
use std::time::{Duration, Instant};

use vnce_audio::{Audio, PlayOptions, Sound};

fn audio() -> Option<Audio> {
    match Audio::new() {
        Ok(audio) => {
            eprintln!("using {audio:?}");
            Some(audio)
        }
        Err(error) => {
            eprintln!("skipped: {error}");
            None
        }
    }
}

/// Waits up to a second for `condition`.
fn eventually(condition: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        if condition() {
            return true;
        }
        sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn sounds_start_end_and_stop() {
    let Some(audio) = audio() else { return };
    assert!(audio.sample_rate() > 0 && audio.channels() > 0);
    assert_ne!(audio.device_name(), "");
    let silence = Sound::mono(
        audio.sample_rate(),
        vec![0.0; audio.sample_rate() as usize / 10],
    );

    // A short sound starts and ends by itself.
    let short = audio.play(&silence);
    assert!(eventually(|| short.is_playing()), "never started");
    assert!(eventually(|| !short.is_playing()), "never ended");

    // A looping sound plays until it is stopped.
    let music = audio.play_with(&silence, PlayOptions::music());
    assert!(eventually(|| music.is_playing()));
    sleep(Duration::from_millis(250));
    assert!(music.is_playing(), "a looping sound ended");
    music.set_volume(0.5, Duration::ZERO);
    music.set_pan(-0.5);
    music.set_speed(1.5);
    audio.set_master_volume(0.8);
    audio.set_muted(true);
    music.stop(Duration::from_millis(20));
    assert!(eventually(|| !music.is_playing()), "stop did not stop it");

    let other = audio.play_with(&silence, PlayOptions::music());
    assert_ne!(other.id(), music.id());
    audio.stop_all(Duration::ZERO);
    assert!(eventually(|| !other.is_playing()));
}
