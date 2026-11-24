//! Mixing many sounds into one signal: [`Mixer`].

use std::time::Duration;

use crate::sound::Sound;

/// At most this many sounds play at once; starting another stops the oldest.
pub const MAX_VOICES: usize = 32;

/// A group of sounds with its own volume (for separate music and effects sliders).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Group {
    /// Sound effects (the default).
    #[default]
    Effects,
    /// Music.
    Music,
}

/// How a sound is played.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayOptions {
    /// Volume, `1.0` is the sound as recorded.
    pub volume: f32,
    /// Stereo position from `-1.0` (left) through `0.0` (center) to `1.0` (right).
    pub pan: f32,
    /// Playback speed; it also changes the pitch (`2.0` is an octave higher).
    pub speed: f32,
    /// Starts again at the end instead of stopping.
    pub looping: bool,
    /// The volume group.
    pub group: Group,
    /// Time to fade in from silence (zero to start at full volume).
    pub fade_in: Duration,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            speed: 1.0,
            looping: false,
            group: Group::Effects,
            fade_in: Duration::ZERO,
        }
    }
}

impl PlayOptions {
    /// Options for looping background music.
    #[must_use]
    pub fn music() -> Self {
        Self {
            looping: true,
            group: Group::Music,
            ..Self::default()
        }
    }
}

/// Identifies a playing sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(pub(crate) u64);

/// A playing sound.
#[derive(Debug, Clone)]
struct Voice {
    id: VoiceId,
    sound: Sound,
    /// Position in source frames (fractional when the speed is not 1).
    position: f64,
    /// Source frames per output frame.
    step: f64,
    speed: f32,
    pan: f32,
    looping: bool,
    group: Group,
    /// Current level, moving towards `target` by `ramp` per output frame.
    level: f32,
    target: f32,
    ramp: f32,
    /// Removed when the level reaches zero (stopping with a fade).
    stopping: bool,
}

/// Mixes playing sounds into an output signal, without any audio device: the audio
/// output drives one on the audio thread, and it can also render offline (for tests or to
/// save a mix as WAV).
///
/// ```
/// use vnce_audio::{Mixer, PlayOptions, Sound};
///
/// let mut mixer = Mixer::new(48_000);
/// let beep = Sound::mono(48_000, vec![0.5; 480]);
/// mixer.play(&beep, PlayOptions::default());
/// let mut out = vec![0.0; 2 * 960]; // stereo, 20 ms
/// mixer.render(&mut out, 2);
/// assert!((out[0] - 0.5).abs() < 1e-6);
/// assert_eq!(out[2 * 600], 0.0); // the sound ended after 10 ms
/// assert_eq!(mixer.playing(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct Mixer {
    sample_rate: u32,
    voices: Vec<Voice>,
    next_id: u64,
    master: f32,
    muted: bool,
    groups: [f32; 2],
}

impl Mixer {
    /// A mixer producing `sample_rate` frames per second.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            voices: Vec::with_capacity(MAX_VOICES),
            next_id: 1,
            master: 1.0,
            muted: false,
            groups: [1.0, 1.0],
        }
    }

    /// Output frames per second.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of sounds playing.
    #[must_use]
    pub fn playing(&self) -> usize {
        self.voices.len()
    }

    /// Returns `true` while the sound `id` is playing.
    #[must_use]
    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.voices.iter().any(|v| v.id == id)
    }

    /// The ids of the sounds playing.
    #[cfg(feature = "output")]
    pub(crate) fn voice_ids(&self) -> impl Iterator<Item = VoiceId> + '_ {
        self.voices.iter().map(|v| v.id)
    }

    /// Reserves the id of the next sound (used by the audio output to answer `play` at
    /// once, before the audio thread starts the sound).
    pub(crate) fn reserve_id(&mut self) -> VoiceId {
        let id = VoiceId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Starts playing `sound` and returns its id.
    pub fn play(&mut self, sound: &Sound, options: PlayOptions) -> VoiceId {
        let id = self.reserve_id();
        self.play_as(id, sound, options);
        id
    }

    /// Starts playing `sound` with a given id.
    pub(crate) fn play_as(&mut self, id: VoiceId, sound: &Sound, options: PlayOptions) {
        if sound.frames() == 0 {
            return;
        }
        if self.voices.len() >= MAX_VOICES {
            // The oldest voice has the smallest id.
            if let Some(oldest) = (0..self.voices.len()).min_by_key(|&i| self.voices[i].id) {
                self.voices.swap_remove(oldest);
            }
        }
        let volume = finite_or(options.volume, 1.0).max(0.0);
        let speed = finite_or(options.speed, 1.0).clamp(0.01, 100.0);
        let fade_frames = self.frames_of(options.fade_in);
        let (level, ramp) = if fade_frames == 0.0 {
            (volume, 0.0)
        } else {
            (0.0, volume / fade_frames)
        };
        self.voices.push(Voice {
            id,
            sound: sound.clone(),
            position: 0.0,
            step: f64::from(speed) * f64::from(sound.sample_rate()) / f64::from(self.sample_rate),
            speed,
            pan: finite_or(options.pan, 0.0).clamp(-1.0, 1.0),
            looping: options.looping,
            group: options.group,
            level,
            target: volume,
            ramp,
            stopping: false,
        });
    }

    /// Stops a sound, fading out over `fade` (zero to stop at once).
    pub fn stop(&mut self, id: VoiceId, fade: Duration) {
        let frames = self.frames_of(fade);
        if frames == 0.0 {
            self.voices.retain(|v| v.id != id);
            return;
        }
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            voice.target = 0.0;
            voice.ramp = voice.level / frames;
            voice.stopping = true;
        }
    }

    /// Stops every sound, fading out over `fade`.
    pub fn stop_all(&mut self, fade: Duration) {
        let ids: Vec<VoiceId> = self.voices.iter().map(|v| v.id).collect();
        for id in ids {
            self.stop(id, fade);
        }
    }

    /// Changes the volume of a playing sound, over `fade`.
    pub fn set_volume(&mut self, id: VoiceId, volume: f32, fade: Duration) {
        let frames = self.frames_of(fade);
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id && !v.stopping) {
            voice.target = finite_or(volume, 1.0).max(0.0);
            if frames == 0.0 {
                voice.level = voice.target;
                voice.ramp = 0.0;
            } else {
                voice.ramp = (voice.target - voice.level).abs() / frames;
            }
        }
    }

    /// Changes the stereo position of a playing sound.
    pub fn set_pan(&mut self, id: VoiceId, pan: f32) {
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            voice.pan = finite_or(pan, 0.0).clamp(-1.0, 1.0);
        }
    }

    /// Changes the speed (and pitch) of a playing sound.
    pub fn set_speed(&mut self, id: VoiceId, speed: f32) {
        let rate = self.sample_rate;
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            voice.speed = finite_or(speed, 1.0).clamp(0.01, 100.0);
            voice.step =
                f64::from(voice.speed) * f64::from(voice.sound.sample_rate()) / f64::from(rate);
        }
    }

    /// Sets the volume of everything (`1.0` is unchanged).
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master = finite_or(volume, 1.0).max(0.0);
    }

    /// The volume of everything.
    #[must_use]
    pub fn master_volume(&self) -> f32 {
        self.master
    }

    /// Sets the volume of a group.
    pub fn set_group_volume(&mut self, group: Group, volume: f32) {
        self.groups[group as usize] = finite_or(volume, 1.0).max(0.0);
    }

    /// The volume of a group.
    #[must_use]
    pub fn group_volume(&self, group: Group) -> f32 {
        self.groups[group as usize]
    }

    /// Silences everything without stopping it.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Returns `true` while muted.
    #[must_use]
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "frame counts of fades are small"
    )]
    fn frames_of(&self, duration: Duration) -> f32 {
        (duration.as_secs_f64() * f64::from(self.sample_rate)).round() as f32
    }

    /// Mixes the next frames into `out`: interleaved samples for `channels` channels
    /// (mono gets left and right averaged, extra channels stay silent). `out` is
    /// overwritten. Never allocates.
    pub fn render(&mut self, out: &mut [f32], channels: u16) {
        let channels = usize::from(channels.max(1));
        let gain = if self.muted { 0.0 } else { self.master };
        let groups = self.groups;
        for frame in out.chunks_mut(channels) {
            let (mut left, mut right) = (0.0_f32, 0.0_f32);
            for voice in &mut self.voices {
                let (l, r) = voice.next(groups[voice.group as usize]);
                left += l;
                right += r;
            }
            let (left, right) = (limit(left * gain), limit(right * gain));
            match frame {
                [mono] => *mono = f32::midpoint(left, right),
                [l, r, rest @ ..] => {
                    *l = left;
                    *r = right;
                    rest.fill(0.0);
                }
                [] => {}
            }
        }
        self.voices.retain(|v| !v.finished());
    }
}

impl Voice {
    /// The next output frame of this voice, and advances it.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "positions are non-negative frame indices of a sound"
    )]
    fn next(&mut self, group: f32) -> (f32, f32) {
        let frames = self.sound.frames();
        if self.finished() || frames == 0 {
            return (0.0, 0.0);
        }
        let index = self.position as usize;
        let fraction = (self.position - index as f64) as f32;
        let (l0, r0) = self.sound.frame(index);
        let next = if index + 1 < frames {
            index + 1
        } else if self.looping {
            0
        } else {
            usize::MAX
        };
        let (l1, r1) = self.sound.frame(next);
        let left = l0 + (l1 - l0) * fraction;
        let right = r0 + (r1 - r0) * fraction;

        // Move the level towards its target.
        if self.level < self.target {
            self.level = (self.level + self.ramp).min(self.target);
        } else if self.level > self.target {
            self.level = (self.level - self.ramp).max(self.target);
        }
        let level = self.level * group;
        // Balance: the far side gets quieter, the center is unchanged.
        let left_gain = (1.0 - self.pan).min(1.0);
        let right_gain = (1.0 + self.pan).min(1.0);

        self.position += self.step;
        if self.position >= frames as f64 {
            if self.looping {
                self.position %= frames as f64;
            } else {
                self.position = f64::INFINITY;
            }
        }
        (left * level * left_gain, right * level * right_gain)
    }

    fn finished(&self) -> bool {
        self.position.is_infinite() || (self.stopping && self.level <= 0.0)
    }
}

/// Keeps samples in `-1.0..=1.0` without hard clipping: unchanged up to 0.8, then a smooth
/// curve towards 1.0.
fn limit(sample: f32) -> f32 {
    const KNEE: f32 = 0.8;
    let magnitude = sample.abs();
    if magnitude <= KNEE {
        sample
    } else {
        let over = (magnitude - KNEE) / (1.0 - KNEE);
        sample.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(mixer: &mut Mixer, frames: usize) -> Vec<(f32, f32)> {
        let mut out = vec![1.0; frames * 2];
        mixer.render(&mut out, 2);
        out.chunks(2).map(|f| (f[0], f[1])).collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn volume_pan_and_groups() {
        let mut mixer = Mixer::new(100);
        let sound = Sound::mono(100, vec![0.4; 100]);
        let id = mixer.play(
            &sound,
            PlayOptions {
                volume: 0.5,
                pan: -1.0,
                ..PlayOptions::default()
            },
        );
        let frame = render(&mut mixer, 1)[0];
        assert!(close(frame.0, 0.2) && close(frame.1, 0.0), "{frame:?}");
        mixer.set_pan(id, 0.5);
        mixer.set_volume(id, 1.0, Duration::ZERO);
        let frame = render(&mut mixer, 1)[0];
        assert!(close(frame.0, 0.2) && close(frame.1, 0.4), "{frame:?}");
        mixer.set_group_volume(Group::Effects, 0.5);
        mixer.set_master_volume(0.5);
        assert!(close(render(&mut mixer, 1)[0].1, 0.1));
        mixer.set_muted(true);
        assert_eq!(render(&mut mixer, 1)[0], (0.0, 0.0));
        assert!(mixer.is_muted() && close(mixer.group_volume(Group::Effects), 0.5));
        assert!(close(mixer.group_volume(Group::Music), 1.0));
        assert!(close(mixer.master_volume(), 0.5));
    }

    #[test]
    fn sounds_end_and_loop() {
        let mut mixer = Mixer::new(10);
        let sound = Sound::mono(10, vec![0.1, 0.2, 0.3]);
        let once = mixer.play(&sound, PlayOptions::default());
        let looped = mixer.play(&sound, PlayOptions::music());
        let frames = render(&mut mixer, 7);
        let left: Vec<f32> = frames.iter().map(|f| f.0).collect();
        let expected = [0.2, 0.4, 0.6, 0.1, 0.2, 0.3, 0.1];
        for (a, b) in left.iter().zip(expected) {
            assert!(close(*a, b), "{left:?}");
        }
        assert!(!mixer.is_playing(once) && mixer.is_playing(looped));
        mixer.stop(looped, Duration::ZERO);
        assert_eq!(mixer.playing(), 0);
    }

    #[test]
    fn speed_resamples_with_interpolation() {
        // A sound at half the output rate plays every frame twice (interpolated).
        let mut mixer = Mixer::new(20);
        mixer.play(
            &Sound::mono(10, vec![0.0, 1.0, 0.0]),
            PlayOptions::default(),
        );
        let left: Vec<f32> = render(&mut mixer, 7).iter().map(|f| f.0).collect();
        let expected = [0.0, 0.5, 1.0, 0.5, 0.0, 0.0, 0.0];
        for (a, b) in left.iter().zip(expected) {
            assert!(close(*a, limit(b)), "{left:?}");
        }
        // Twice as fast: every other frame.
        let mut fast = Mixer::new(10);
        let id = fast.play(
            &Sound::mono(10, vec![0.1, 0.2, 0.3, 0.4]),
            PlayOptions::default(),
        );
        fast.set_speed(id, 2.0);
        let left: Vec<f32> = render(&mut fast, 3).iter().map(|f| f.0).collect();
        assert!(close(left[0], 0.1) && close(left[1], 0.3) && close(left[2], 0.0));
    }

    #[test]
    fn fades_in_and_out() {
        let mut mixer = Mixer::new(10);
        let sound = Sound::mono(10, vec![0.5; 100]);
        let id = mixer.play(
            &sound,
            PlayOptions {
                fade_in: Duration::from_millis(400),
                ..PlayOptions::default()
            },
        );
        let left: Vec<f32> = render(&mut mixer, 5).iter().map(|f| f.0).collect();
        assert!(
            close(left[0], 0.125) && close(left[3], 0.5) && close(left[4], 0.5),
            "{left:?}"
        );
        mixer.stop(id, Duration::from_millis(200));
        let left: Vec<f32> = render(&mut mixer, 3).iter().map(|f| f.0).collect();
        assert!(close(left[0], 0.25) && close(left[1], 0.0), "{left:?}");
        assert_eq!(mixer.playing(), 0);
    }

    #[test]
    fn voice_limit_drops_the_oldest() {
        let mut mixer = Mixer::new(10);
        let sound = Sound::mono(10, vec![0.01; 10]);
        let first = mixer.play(&sound, PlayOptions::default());
        let mut last = first;
        for _ in 0..MAX_VOICES {
            last = mixer.play(&sound, PlayOptions::default());
        }
        assert_eq!(mixer.playing(), MAX_VOICES);
        assert!(!mixer.is_playing(first) && mixer.is_playing(last));
        mixer.stop_all(Duration::ZERO);
        assert_eq!(mixer.playing(), 0);
        // Empty sounds are ignored.
        mixer.play(&Sound::mono(10, vec![]), PlayOptions::default());
        assert_eq!(mixer.playing(), 0);
    }

    #[test]
    fn output_layouts_and_limiting() {
        let mut mixer = Mixer::new(10);
        let stereo = Sound::from_samples(2, 10, vec![1.0, -1.0, 1.0, -1.0]);
        for _ in 0..3 {
            mixer.play(&stereo, PlayOptions::default());
        }
        let mut mono = [9.0; 1];
        mixer.render(&mut mono, 1);
        assert!(close(mono[0], 0.0));
        let mut surround = [9.0; 4];
        mixer.render(&mut surround, 4);
        // Three full-scale sounds are limited, not clipped.
        assert!(surround[0] <= 1.0 && surround[0] > 0.95);
        assert!(surround[1] >= -1.0 && surround[1] < -0.95);
        assert_eq!(&surround[2..], [0.0, 0.0]);
        assert!(close(limit(0.5), 0.5) && limit(100.0) <= 1.0);
    }
}
