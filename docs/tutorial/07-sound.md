# 7. Sound

Sound lives in `vnce::vnce_audio` and needs the `audio` feature:

```sh
cargo add vnce --features audio
```

On Linux the ALSA development files are needed to build (`libasound2-dev` on Debian and
Ubuntu, `alsa-lib-devel` on Fedora).

## Effects and music without files

`Synth` generates retro sound effects in code; `Preset` has ready ones (jump, coin, hit,
explosion, power-up, laser, blip). The seed varies a preset slightly, so a hundred coins
do not all sound the same. `Melody` plays a tune from note names: `C4`, `F#3`, `Bb5`,
`-` for a rest, `:2` for two beats.

```rust
use vnce::vnce_audio::{Melody, Preset, Synth, Wave};

let rate = 44_100;
let coin = Synth::preset(Preset::Coin, 0).render(rate);
let jump = Synth::preset(Preset::Jump, 7).render(rate);
assert!(coin.duration().as_secs_f32() < 1.0 && jump.frames() > 0);

// Your own effect: a falling square wave.
let drop = Synth { frequency: 600.0, slide: -2.0, hold: 0.15, ..Synth::default() }.render(rate);
assert!(drop.frames() > 0);

// A short loop for background music.
let voice = Synth { wave: Wave::Triangle, volume: 0.3, ..Synth::default() };
let music = Melody::new(140.0, voice).render("C4 E4 G4 E4 A3 C4 E4:2 -", rate).unwrap();
assert!(music.duration().as_secs_f32() > 3.0);
```

Sound files work too: `Sound::load("boom.wav")`, or `Sound::decode(bytes)` for files
embedded with `include_bytes!`. OGG Vorbis needs the `ogg` feature.

## Playing sounds

`Audio` opens the default output device and mixes everything in the background. Keep it
in your shared game data; if there is no sound device, `Audio::new` returns an error and
the game simply stays silent.

```rust,no_run
use std::time::Duration;
use vnce::vnce_audio::{Audio, Group, Melody, PlayOptions, Preset, Synth, Wave};

let audio = Audio::new().ok(); // `None` without a sound device
let rate = audio.as_ref().map_or(44_100, Audio::sample_rate);
let coin = Synth::preset(Preset::Coin, 0).render(rate);
let voice = Synth { wave: Wave::Triangle, volume: 0.3, ..Synth::default() };
let music = Melody::new(140.0, voice).render("C4 E4 G4 E4", rate).unwrap();

if let Some(audio) = &audio {
    audio.play(&coin); // fire and forget
    let song = audio.play_with(&music, PlayOptions::music()); // looping, music group
    audio.set_group_volume(Group::Music, 0.6); // settings: music volume
    audio.set_group_volume(Group::Effects, 0.8);
    song.stop(Duration::from_millis(500)); // fade out when the level ends
}
```

`PlayOptions` sets the volume, the stereo position (`pan`), the speed (also the pitch),
looping and a fade-in. The `Voice` returned by `play` changes or stops a sound while it
plays.

## Testing sound without a device

`Mixer` is the same mixer without a device: it renders into a buffer, for tests or to
write a WAV file.

```rust
use vnce::vnce_audio::{Mixer, PlayOptions, Preset, Synth};

let mut mixer = Mixer::new(44_100);
let blip = Synth::preset(Preset::Blip, 0).render(44_100);
mixer.play(&blip, PlayOptions::default());
let mut out = vec![0.0; 2 * 1024]; // 1024 stereo frames
mixer.render(&mut out, 2);
assert!(out.iter().any(|&sample| sample != 0.0));
```

Next: [Performance tips](08-performance.md).
