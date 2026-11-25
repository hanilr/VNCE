//! A sound effect workshop built with `vnce_audio`: pick a preset, change its settings,
//! listen, see the waveform, and save it as a WAV file.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example sfx --features audio
//! ```
//!
//! `↑` `↓`: choose · `Tab`: presets / settings · `←` `→`: change a setting ·
//! `Space` / `Enter`: play · `R`: random variation · `S`: save WAV · `Esc` / `Q`: quit.

// Demo code converts freely between sizes and floats; every value stays far inside the
// target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "sizes, pixels and settings are small"
)]

use vnce::vnce_audio::{Audio, Preset, Sound, Synth, Wave};
use vnce::vnce_core::{Color, Style};
use vnce::vnce_raster::{Canvas, PixelMode, RenderOptions, Rgba};
use vnce::vnce_runtime::{Context, Game, RunConfig};
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind};
use vnce::vnce_ui::{
    Block, BorderType, Constraint, Layout, Line, List, ListState, Paragraph, Span, StatefulWidget,
    Widget,
};

/// The settings that can be changed, in display order.
const SETTINGS: [&str; 13] = [
    "wave",
    "frequency",
    "slide",
    "vibrato depth",
    "vibrato speed",
    "duty",
    "attack",
    "decay",
    "sustain",
    "hold",
    "release",
    "low-pass",
    "volume",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let audio = Audio::new().ok();
    let rate = audio.as_ref().map_or(44_100, Audio::sample_rate);
    let mut workshop = Workshop {
        audio,
        rate,
        presets: ListState::with_selected(Some(0)),
        settings: ListState::with_selected(Some(1)),
        editing: false,
        seed: 0,
        synth: Synth::preset(Preset::ALL[0], 0),
        sound: Sound::mono(rate, Vec::new()),
        message: String::new(),
        mode: PixelMode::HalfBlock,
    };
    workshop.rebuild();
    vnce::run(&mut workshop, RunConfig::default())?;
    Ok(())
}

struct Workshop {
    /// `None` without an audio device: sounds are shown but not played.
    audio: Option<Audio>,
    rate: u32,
    presets: ListState,
    settings: ListState,
    /// `true` while the settings list has the focus.
    editing: bool,
    seed: u64,
    synth: Synth,
    sound: Sound,
    message: String,
    mode: PixelMode,
}

impl Workshop {
    fn preset(&self) -> Preset {
        Preset::ALL[self.presets.selected().unwrap_or(0) % Preset::ALL.len()]
    }

    /// Renders the sound again after a change.
    fn rebuild(&mut self) {
        self.sound = self.synth.render(self.rate);
    }

    fn play(&mut self) {
        match &self.audio {
            Some(audio) => {
                audio.play(&self.sound);
            }
            None => "no audio device: nothing is played".clone_into(&mut self.message),
        }
    }

    fn load_preset(&mut self) {
        self.synth = Synth::preset(self.preset(), self.seed);
        self.rebuild();
        self.play();
    }

    /// Changes the selected setting one step in `direction` (-1 or 1).
    fn adjust(&mut self, direction: f32) {
        let s = &mut self.synth;
        let step = |value: &mut f32, amount: f32, min: f32, max: f32| {
            *value = (*value + amount * direction).clamp(min, max);
        };
        match self.settings.selected().unwrap_or(0) {
            0 => {
                let waves = [
                    Wave::Square,
                    Wave::Saw,
                    Wave::Triangle,
                    Wave::Sine,
                    Wave::Noise,
                ];
                let index = waves.iter().position(|&w| w == s.wave).unwrap_or(0) as i32;
                let next = (index + direction as i32).rem_euclid(waves.len() as i32);
                s.wave = waves[next as usize];
            }
            1 => s.frequency = (s.frequency * 1.06_f32.powf(direction)).clamp(20.0, 8000.0),
            2 => step(&mut s.slide, 0.25, -8.0, 8.0),
            3 => step(&mut s.vibrato_depth, 0.1, 0.0, 12.0),
            4 => step(&mut s.vibrato_speed, 1.0, 0.0, 40.0),
            5 => step(&mut s.duty, 0.05, 0.05, 0.95),
            6 => step(&mut s.attack, 0.01, 0.0, 2.0),
            7 => step(&mut s.decay, 0.01, 0.0, 2.0),
            8 => step(&mut s.sustain, 0.05, 0.0, 1.0),
            9 => step(&mut s.hold, 0.02, 0.0, 3.0),
            10 => step(&mut s.release, 0.02, 0.0, 3.0),
            11 => step(&mut s.low_pass, 500.0, 0.0, 20_000.0),
            _ => step(&mut s.volume, 0.05, 0.0, 1.0),
        }
        self.rebuild();
        self.play();
    }

    fn setting_values(&self) -> [String; 13] {
        let s = &self.synth;
        [
            format!("{:?}", s.wave),
            format!("{:.0} Hz", s.frequency),
            format!("{:+.2} oct/s", s.slide),
            format!("{:.1} semitones", s.vibrato_depth),
            format!("{:.0} Hz", s.vibrato_speed),
            format!("{:.2}", s.duty),
            format!("{:.2} s", s.attack),
            format!("{:.2} s", s.decay),
            format!("{:.2}", s.sustain),
            format!("{:.2} s", s.hold),
            format!("{:.2} s", s.release),
            if s.low_pass > 0.0 {
                format!("{:.0} Hz", s.low_pass)
            } else {
                "off".to_owned()
            },
            format!("{:.2}", s.volume),
        ]
    }

    fn save(&mut self) {
        let name = format!("sfx-{}-{}.wav", self.preset().name(), self.seed);
        self.message = match std::fs::write(&name, self.sound.to_wav()) {
            Ok(()) => format!("saved {name}"),
            Err(error) => format!("cannot save {name}: {error}"),
        };
    }
}

impl Game for Workshop {
    fn init(&mut self, ctx: &mut Context) {
        self.mode = PixelMode::detect(ctx.capabilities().color_depth);
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        let Event::Key(key) = event else {
            return;
        };
        if key.kind == KeyEventKind::Release {
            return;
        }
        let presets = Preset::ALL.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => ctx.quit(),
            KeyCode::Tab => self.editing = !self.editing,
            KeyCode::Up | KeyCode::Down => {
                let up = key.code == KeyCode::Up;
                if self.editing {
                    if up {
                        self.settings.select_previous(SETTINGS.len(), true);
                    } else {
                        self.settings.select_next(SETTINGS.len(), true);
                    }
                } else {
                    if up {
                        self.presets.select_previous(presets, true);
                    } else {
                        self.presets.select_next(presets, true);
                    }
                    self.seed = 0;
                    self.load_preset();
                }
            }
            KeyCode::Left if self.editing => self.adjust(-1.0),
            KeyCode::Right if self.editing => self.adjust(1.0),
            KeyCode::Enter | KeyCode::Char(' ') => self.play(),
            KeyCode::Char('r' | 'R') => {
                self.seed = self.seed.wrapping_add(1);
                self.load_preset();
            }
            KeyCode::Char('s' | 'S') => self.save(),
            _ => {}
        }
    }

    fn update(&mut self, _ctx: &mut Context) {}

    fn render(&mut self, _ctx: &Context, frame: &mut Frame<'_>) {
        let area = frame.area();
        let buffer = frame.buffer_mut();
        let [title, body, help] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let device = self
            .audio
            .as_ref()
            .map_or("no audio device".to_owned(), |a| {
                format!("{} · {} Hz", a.device_name(), a.sample_rate())
            });
        Paragraph::new(Line::from(vec![
            Span::styled(" vnce sfx ", Style::new().bold()),
            Span::raw(format!("· {device} · {}", self.message)),
        ]))
        .style(
            Style::new()
                .fg(Color::rgb(20, 20, 30))
                .bg(Color::rgb(130, 220, 200)),
        )
        .render(title, buffer);
        Paragraph::new(
            " ↑↓: choose · Tab: presets / settings · ←→: change · Space: play · R: variation · S: save · Q: quit",
        )
        .style(Style::new().dim())
        .render(help, buffer);

        let [left, right] = Layout::horizontal([Constraint::Length(36), Constraint::Fill(1)])
            .spacing(1)
            .areas(body);
        let [preset_area, settings_area] =
            Layout::vertical([Constraint::Length(9), Constraint::Fill(1)]).areas(left);
        let focus = |active: bool| {
            if active {
                Style::new().fg(Color::rgb(240, 200, 90))
            } else {
                Style::new().dim()
            }
        };
        let highlight = Style::new()
            .fg(Color::rgb(20, 20, 30))
            .bg(Color::rgb(240, 200, 90));
        List::new(Preset::ALL.map(Preset::name))
            .highlight_symbol("▶ ")
            .highlight_style(highlight)
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(focus(!self.editing))
                    .title(format!(" Presets (seed {}) ", self.seed)),
            )
            .render(preset_area, buffer, &mut self.presets);
        let values = self.setting_values();
        let rows: Vec<String> = SETTINGS
            .iter()
            .zip(&values)
            .map(|(name, value)| format!("{name:<14}{value}"))
            .collect();
        List::new(rows)
            .highlight_symbol("◀▶ ")
            .highlight_style(highlight)
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(focus(self.editing))
                    .title(" Settings "),
            )
            .render(settings_area, buffer, &mut self.settings);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(format!(
                " Waveform · {:.2} s ",
                self.sound.duration().as_secs_f32()
            ));
        let inner = block.inner(right);
        block.render(right, buffer);
        if !inner.is_empty() {
            let mut canvas = Canvas::for_area(inner, self.mode);
            draw_waveform(&mut canvas, &self.sound);
            canvas.render(buffer, inner, self.mode, &RenderOptions::default());
        }
    }
}

/// The minimum and maximum of the samples under each column, as vertical lines.
fn draw_waveform(canvas: &mut Canvas, sound: &Sound) {
    let (width, height) = (canvas.width(), canvas.height());
    canvas.clear(Rgba::rgb(14, 16, 26));
    let middle = height as i32 / 2;
    canvas.line(0, middle, width as i32, middle, Rgba::rgb(50, 55, 80));
    let samples = sound.samples();
    if samples.is_empty() || width == 0 {
        return;
    }
    let per_column = samples.len().div_ceil(width as usize).max(1);
    for (x, chunk) in samples.chunks(per_column).enumerate() {
        let (low, high) = chunk
            .iter()
            .fold((0.0_f32, 0.0_f32), |(lo, hi), &s| (lo.min(s), hi.max(s)));
        let to_y = |v: f32| (middle as f32 - v * (height as f32 / 2.0 - 1.0)).round() as i32;
        let hue = x as f32 / width as f32;
        let color = Rgba::rgb((120.0 + 120.0 * hue) as u8, (220.0 - 80.0 * hue) as u8, 200);
        canvas.line(x as i32, to_y(high), x as i32, to_y(low), color);
    }
}
