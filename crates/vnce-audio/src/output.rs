//! Playing sounds on the default audio device: [`Audio`] and [`Voice`].

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::mixer::{Group, MAX_VOICES, Mixer, PlayOptions, VoiceId};
use crate::sound::Sound;

/// Preferred device buffer in frames (about 11 ms at 44.1 kHz): low latency where the
/// device allows it.
const BUFFER_FRAMES: u32 = 512;

/// Requests from the game to the audio thread.
enum Command {
    Play(VoiceId, Sound, PlayOptions),
    Stop(VoiceId, Duration),
    StopAll(Duration),
    Volume(VoiceId, f32, Duration),
    Pan(VoiceId, f32),
    Speed(VoiceId, f32),
    Master(f32),
    Group(Group, f32),
    Muted(bool),
}

/// The ids of the sounds playing, published by the audio thread (0 = free slot).
type Playing = Arc<[AtomicU64; MAX_VOICES]>;

/// Why the audio output could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AudioError {
    /// There is no audio output device (for example on a server or over SSH).
    NoDevice,
    /// The device uses a sample format this crate does not support.
    Unsupported(String),
    /// The audio system reported an error.
    Backend(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => f.write_str("no audio output device"),
            Self::Unsupported(message) => write!(f, "unsupported audio device: {message}"),
            Self::Backend(message) => write!(f, "audio error: {message}"),
        }
    }
}

impl std::error::Error for AudioError {}

fn backend(error: impl fmt::Display) -> AudioError {
    AudioError::Backend(error.to_string())
}

/// Sound output on the default device. Sounds are mixed on the audio thread; every call
/// here only sends a request and returns at once.
///
/// Games should keep running without sound when no device exists:
///
/// ```no_run
/// use vnce_audio::{Audio, Sound};
///
/// let audio = Audio::new().ok(); // `None` without an audio device
/// let beep = Sound::mono(44_100, vec![0.2; 4_410]);
/// if let Some(audio) = &audio {
///     audio.play(&beep);
/// }
/// ```
pub struct Audio {
    /// Keeps the device open; dropping `Audio` stops the sound.
    _stream: cpal::Stream,
    commands: mpsc::Sender<Command>,
    playing: Playing,
    next_id: AtomicU64,
    sample_rate: u32,
    channels: u16,
    device: String,
}

impl fmt::Debug for Audio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Audio")
            .field("device", &self.device)
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

impl Audio {
    /// Opens the default output device.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::NoDevice`] without an output device, and other errors when the
    /// device cannot be opened. Never panics.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
        let supported = device.default_output_config().map_err(backend)?;
        let format = supported.sample_format();
        let mut config = supported.config();
        if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
            if (min..=max).contains(&BUFFER_FRAMES) {
                config.buffer_size = cpal::BufferSize::Fixed(BUFFER_FRAMES);
            }
        }
        let name = device
            .description()
            .map_or_else(|_| "default device".to_owned(), |d| d.name().to_owned());
        let (commands, receiver) = mpsc::channel();
        let playing: Playing = Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));
        let shared = Arc::clone(&playing);
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, receiver, shared),
            SampleFormat::F64 => build::<f64>(&device, &config, receiver, shared),
            SampleFormat::I8 => build::<i8>(&device, &config, receiver, shared),
            SampleFormat::I16 => build::<i16>(&device, &config, receiver, shared),
            SampleFormat::I24 => build::<cpal::I24>(&device, &config, receiver, shared),
            SampleFormat::I32 => build::<i32>(&device, &config, receiver, shared),
            SampleFormat::I64 => build::<i64>(&device, &config, receiver, shared),
            SampleFormat::U8 => build::<u8>(&device, &config, receiver, shared),
            SampleFormat::U16 => build::<u16>(&device, &config, receiver, shared),
            SampleFormat::U24 => build::<cpal::U24>(&device, &config, receiver, shared),
            SampleFormat::U32 => build::<u32>(&device, &config, receiver, shared),
            SampleFormat::U64 => build::<u64>(&device, &config, receiver, shared),
            other => return Err(AudioError::Unsupported(format!("{other:?} samples"))),
        }?;
        stream.play().map_err(backend)?;
        Ok(Self {
            _stream: stream,
            commands,
            playing,
            next_id: AtomicU64::new(1),
            sample_rate: config.sample_rate,
            channels: config.channels,
            device: name,
        })
    }

    /// The name of the output device.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device
    }

    /// Output frames per second.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Output channels.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    fn send(&self, command: Command) {
        // The audio thread only goes away with the stream, which `self` owns.
        let _ = self.commands.send(command);
    }

    /// Plays a sound once at full volume.
    pub fn play(&self, sound: &Sound) -> Voice {
        self.play_with(sound, PlayOptions::default())
    }

    /// Plays a sound with options (volume, pan, speed, looping, group, fade in).
    pub fn play_with(&self, sound: &Sound, options: PlayOptions) -> Voice {
        let id = VoiceId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.send(Command::Play(id, sound.clone(), options));
        Voice {
            id,
            commands: self.commands.clone(),
            playing: Arc::clone(&self.playing),
        }
    }

    /// Stops every sound, fading out over `fade`.
    pub fn stop_all(&self, fade: Duration) {
        self.send(Command::StopAll(fade));
    }

    /// Sets the volume of everything (`1.0` is unchanged).
    pub fn set_master_volume(&self, volume: f32) {
        self.send(Command::Master(volume));
    }

    /// Sets the volume of a group (music or effects).
    pub fn set_group_volume(&self, group: Group, volume: f32) {
        self.send(Command::Group(group, volume));
    }

    /// Silences everything without stopping it.
    pub fn set_muted(&self, muted: bool) {
        self.send(Command::Muted(muted));
    }
}

/// A sound started with [`Audio::play`]: change or stop it while it plays. Dropping it
/// does not stop the sound.
#[derive(Debug, Clone)]
pub struct Voice {
    id: VoiceId,
    commands: mpsc::Sender<Command>,
    playing: Playing,
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Command")
    }
}

impl Voice {
    fn send(&self, command: Command) {
        // Fails only after `Audio` was dropped, when nothing plays anyway.
        let _ = self.commands.send(command);
    }

    /// The id of the sound.
    #[must_use]
    pub fn id(&self) -> VoiceId {
        self.id
    }

    /// Returns `true` while the sound plays. Updated by the audio thread after each
    /// device buffer, so it may lag a few milliseconds behind.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
            .iter()
            .any(|slot| slot.load(Ordering::Relaxed) == self.id.0)
    }

    /// Stops the sound, fading out over `fade` (zero to stop at once).
    pub fn stop(&self, fade: Duration) {
        self.send(Command::Stop(self.id, fade));
    }

    /// Changes the volume, over `fade`.
    pub fn set_volume(&self, volume: f32, fade: Duration) {
        self.send(Command::Volume(self.id, volume, fade));
    }

    /// Changes the stereo position (`-1.0` left to `1.0` right).
    pub fn set_pan(&self, pan: f32) {
        self.send(Command::Pan(self.id, pan));
    }

    /// Changes the speed and pitch.
    pub fn set_speed(&self, speed: f32) {
        self.send(Command::Speed(self.id, speed));
    }
}

fn apply(mixer: &mut Mixer, command: Command) {
    match command {
        Command::Play(id, sound, options) => mixer.play_as(id, &sound, options),
        Command::Stop(id, fade) => mixer.stop(id, fade),
        Command::StopAll(fade) => mixer.stop_all(fade),
        Command::Volume(id, volume, fade) => mixer.set_volume(id, volume, fade),
        Command::Pan(id, pan) => mixer.set_pan(id, pan),
        Command::Speed(id, speed) => mixer.set_speed(id, speed),
        Command::Master(volume) => mixer.set_master_volume(volume),
        Command::Group(group, volume) => mixer.set_group_volume(group, volume),
        Command::Muted(muted) => mixer.set_muted(muted),
    }
}

/// Opens a stream with samples of type `T`, mixing on the audio thread.
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    receiver: mpsc::Receiver<Command>,
    playing: Playing,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels;
    let mut mixer = Mixer::new(config.sample_rate);
    // Grows only if the device asks for a bigger buffer than ever before.
    let mut scratch = vec![0.0_f32; 8192];
    let data = move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
        while let Ok(command) = receiver.try_recv() {
            apply(&mut mixer, command);
        }
        if scratch.len() < out.len() {
            scratch.resize(out.len(), 0.0);
        }
        let buffer = &mut scratch[..out.len()];
        mixer.render(buffer, channels);
        for (sample, &value) in out.iter_mut().zip(buffer.iter()) {
            *sample = T::from_sample(value);
        }
        let mut ids = mixer.voice_ids();
        for slot in playing.iter() {
            slot.store(ids.next().map_or(0, |id| id.0), Ordering::Relaxed);
        }
    };
    // Errors during playback (for example an unplugged device) only mean silence.
    let error = |_error: cpal::Error| {};
    device
        .build_output_stream::<T, _, _>(*config, data, error, None)
        .map_err(backend)
}
