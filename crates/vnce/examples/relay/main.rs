//! Relay: the showcase game of vnce. A derelict signal station lies dark; bring the power
//! cores of its five wings back to the relay in the atrium. Every wing shows one part of
//! the engine as part of play:
//!
//! - Atrium: tile maps, a following camera, lighting with `Add` / `Multiply` blending,
//!   doors eased with timers.
//! - Observatory: a nebula from a GPU fragment shader (`ShaderQuad`), or the CPU.
//! - Reactor: a 3D core with depth and MSAA (`MeshRenderer`), or a CPU rasterized one.
//! - Prism Hall: one picture in every pixel mode, real pixels, fast and exact scaling.
//! - Sound Lab: synthesized notes, music and panning (`vnce_audio`), and a waveform.
//! - Archive: layouts, wrapped Unicode text, lists, gauges, borders, the FPS overlay.
//!
//! Drones patrol the wings (collision through a `SpatialGrid`); the best time, the
//! settings and the controls are kept in a save file.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example relay
//! cargo run --release --example relay --features gpu,audio   # GPU scenes and sound
//! cargo run --release --example relay -- --cpu               # CPU pictures only
//! cargo run --release --example relay --features gpu -- --bench   # frame timings
//! ```
//!
//! `←↑→↓` / `WASD`: move · `E` / click: use · `P` / `Esc`: pause · `G`: display method ·
//! `F`: performance panel · `Q`: quit.

// Game code converts freely between pixels, tiles and floats; every value stays far inside
// the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel and tile coordinates are small"
)]

mod art;
mod audio;
mod bench;
mod play;
mod scenes;
mod showrooms;
mod visuals;
mod world;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{ActionMap, SaveFile, SceneStack};
use vnce::vnce_raster::{Canvas, Font, Rgba};
use vnce::vnce_runtime::{self, Context, FpsLimit, RunConfig};
use vnce::vnce_term::input::MouseButton;
use vnce::vnce_term::{Frame, GraphicsMode, KeyCode};
use vnce::vnce_ui::{Alignment, Block, BorderType, Clear, Line, Padding, Paragraph, Widget};

use crate::audio::Sounds;
use crate::visuals::Visuals;

/// What the player can do; keys and mouse buttons are bound to these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Up,
    Down,
    Left,
    Right,
    Use,
    Pause,
    Confirm,
    Back,
    Quit,
    Method,
    Perf,
}

/// Every action with its name in the save file.
pub(crate) const ACTIONS: [(Action, &str); 11] = [
    (Action::Up, "up"),
    (Action::Down, "down"),
    (Action::Left, "left"),
    (Action::Right, "right"),
    (Action::Use, "use"),
    (Action::Pause, "pause"),
    (Action::Confirm, "confirm"),
    (Action::Back, "back"),
    (Action::Quit, "quit"),
    (Action::Method, "method"),
    (Action::Perf, "perf"),
];

/// The controls before the player changes them.
pub(crate) fn default_controls() -> ActionMap<Action> {
    ActionMap::new()
        .bind(Action::Up, KeyCode::Up)
        .bind(Action::Up, 'w')
        .bind(Action::Down, KeyCode::Down)
        .bind(Action::Down, 's')
        .bind(Action::Left, KeyCode::Left)
        .bind(Action::Left, 'a')
        .bind(Action::Right, KeyCode::Right)
        .bind(Action::Right, 'd')
        .bind(Action::Use, 'e')
        .bind(Action::Use, MouseButton::Left)
        .bind(Action::Pause, 'p')
        .bind(Action::Pause, KeyCode::Esc)
        .bind(Action::Confirm, KeyCode::Enter)
        .bind(Action::Confirm, ' ')
        .bind(Action::Back, KeyCode::Esc)
        .bind(Action::Quit, 'q')
        .bind(Action::Method, 'g')
        .bind(Action::Perf, 'f')
}

/// Frame rate limits offered in the settings, in order.
pub(crate) const FRAME_RATES: [FpsLimit; 5] = [
    FpsLimit::Fixed(30),
    FpsLimit::Fixed(60),
    FpsLimit::Fixed(90),
    FpsLimit::Fixed(120),
    FpsLimit::Unlimited,
];

/// Render times and frame rates of the last frames, for the performance panel.
#[derive(Default)]
pub(crate) struct Stats {
    /// Milliseconds spent in `render` (drawing and converting, before the terminal write).
    pub(crate) render_ms: VecDeque<f32>,
    pub(crate) fps: VecDeque<f32>,
}

impl Stats {
    const LENGTH: usize = 48;

    pub(crate) fn record(&mut self, render: Duration, fps: f64) {
        for (list, value) in [
            (&mut self.render_ms, render.as_secs_f32() * 1000.0),
            (&mut self.fps, fps as f32),
        ] {
            if list.len() == Self::LENGTH {
                list.pop_front();
            }
            list.push_back(value);
        }
    }

    /// Average render time over the kept frames, in milliseconds.
    pub(crate) fn average_render(&self) -> f32 {
        if self.render_ms.is_empty() {
            0.0
        } else {
            self.render_ms.iter().sum::<f32>() / self.render_ms.len() as f32
        }
    }
}

/// Data shared by every scene.
pub(crate) struct Shared {
    pub(crate) controls: ActionMap<Action>,
    pub(crate) modes: Vec<GraphicsMode>,
    /// Best time in tenths of a second (0: none yet).
    pub(crate) best: u32,
    /// Volumes in percent.
    pub(crate) music_volume: u32,
    pub(crate) effects_volume: u32,
    /// Show the performance panel.
    pub(crate) perf: bool,
    pub(crate) sounds: Sounds,
    /// Drawing needs `&mut`, scenes draw with `&Shared`.
    pub(crate) visuals: RefCell<Visuals>,
    pub(crate) stats: RefCell<Stats>,
    pub(crate) save: SaveFile,
}

impl Shared {
    pub(crate) fn new(save: SaveFile, gpu: bool, sound: bool) -> Self {
        let mut controls = default_controls();
        for (action, name) in ACTIONS {
            if let Some(bindings) = save.bindings(&format!("keys.{name}")) {
                controls.unbind(action);
                for binding in bindings {
                    controls.add(action, binding);
                }
            }
        }
        let mut visuals = Visuals::new(gpu);
        visuals.set_gpu(gpu && save.get_or("gpu", true));
        Self {
            controls,
            modes: Vec::new(),
            best: save.get_or("best", 0),
            music_volume: save.get_or("music_volume", 60_u32).min(100),
            effects_volume: save.get_or("effects_volume", 80_u32).min(100),
            perf: save.get_or("perf", false),
            sounds: Sounds::new(sound),
            visuals: RefCell::new(visuals),
            stats: RefCell::new(Stats::default()),
            save,
        }
    }

    /// Writes the best time, the settings and the controls to the save file.
    pub(crate) fn store(&mut self, ctx: &Context) {
        let gpu = self.visuals.borrow().using_gpu();
        let save = &mut self.save;
        save.set("best", self.best);
        save.set("music_volume", self.music_volume);
        save.set("effects_volume", self.effects_volume);
        save.set("perf", self.perf);
        save.set("gpu", gpu);
        save.set("frame_rate", ctx.fps_limit());
        save.set("display", ctx.graphics_mode());
        for (action, name) in ACTIONS {
            save.set_bindings(&format!("keys.{name}"), self.controls.bindings(action));
        }
        // A game must keep running when the disk is full or read-only; the values stay
        // in memory and the next save tries again.
        let _ = save.save();
    }

    /// Finds the display methods of this terminal (once) and restores the saved ones.
    pub(crate) fn detect_modes(&mut self, ctx: &Context) {
        if !self.modes.is_empty() {
            return;
        }
        self.modes = GraphicsMode::available(ctx.terminal_info(), &ctx.capabilities());
        let rate = self.save.get_str("frame_rate");
        if let Some(&limit) = FRAME_RATES
            .iter()
            .find(|l| Some(l.to_string().as_str()) == rate)
        {
            // Every offered rate is valid.
            let _ = ctx.set_fps_limit(limit);
        }
        if let Some(mode) = self
            .save
            .get::<GraphicsMode>("display")
            .filter(|mode| self.modes.contains(mode))
        {
            ctx.set_graphics_mode(mode);
        }
        self.sounds.volumes(self.music_volume, self.effects_volume);
    }

    /// The next display method of this terminal.
    pub(crate) fn cycle_method(&self, ctx: &Context) {
        if self.modes.is_empty() {
            return;
        }
        let current = ctx.graphics_mode();
        let index = self.modes.iter().position(|&m| m == current).unwrap_or(0);
        ctx.set_graphics_mode(self.modes[(index + 1) % self.modes.len()]);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    if flag("--bench") {
        bench::run(!flag("--cpu"));
        return Ok(());
    }
    let shared = Shared::new(SaveFile::open("relay"), !flag("--cpu"), true);
    let mut game = SceneStack::new(scenes::Title::new(), shared);
    let mut config = RunConfig::default();
    config.terminal.mouse_capture = true;
    vnce_runtime::run(&mut game, config)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Drawing helpers shared by the scenes
// ---------------------------------------------------------------------------------------

pub(crate) const ACCENT: Color = Color::Rgb(255, 210, 110);
pub(crate) const COOL: Color = Color::Rgb(140, 200, 255);
pub(crate) const PANEL: Color = Color::Rgb(18, 20, 30);

/// The area for the world (between the status and the help line) and the size of the
/// world view in world pixels: about 15 tiles high, with square world pixels.
pub(crate) fn view(ctx: &Context, frame: &Frame<'_>) -> (Rect, u32, u32) {
    let size = ctx.size();
    let area = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
    let (width, height) = frame.image_pixel_size(area);
    if width == 0 || height == 0 {
        return (area, 0, 0);
    }
    let scale = (height / (world::TILE * 15)).max(1);
    let view_width = (width as f32 * frame.image_pixel_aspect() / scale as f32).round() as u32;
    (area, view_width.max(1), (height / scale).max(1))
}

/// Shows `canvas` enlarged to the exact pixel size of `area` (nearest neighbour), reusing
/// `big` between frames. Rows that repeat a source row are copied, not recomputed.
pub(crate) fn present(frame: &mut Frame<'_>, area: Rect, canvas: &Canvas, big: &mut Canvas) {
    let (width, height) = frame.image_pixel_size(area);
    if canvas.width() == 0 || canvas.height() == 0 {
        // Nothing to enlarge (for example before the first GPU picture arrives).
        return;
    }
    if (width, height) == (canvas.width(), canvas.height()) || width == 0 || height == 0 {
        frame.draw_image(area, canvas);
        return;
    }
    if (big.width(), big.height()) != (width, height) {
        *big = Canvas::new(width, height);
    }
    let (source_width, source_height) = (canvas.width() as usize, canvas.height() as usize);
    let columns: Vec<usize> = (0..width as usize)
        .map(|x| x * source_width / width as usize)
        .collect();
    let source = canvas.pixels();
    let pixels = big.pixels_mut();
    let row_length = width as usize;
    let mut previous: Option<usize> = None;
    for y in 0..height as usize {
        let row = y * source_height / height as usize;
        let start = y * row_length;
        if previous == Some(row) {
            pixels.copy_within(start - row_length..start, start);
        } else {
            let line = &source[row * source_width..(row + 1) * source_width];
            for (target, &x) in pixels[start..start + row_length].iter_mut().zip(&columns) {
                *target = line[x];
            }
        }
        previous = Some(row);
    }
    frame.draw_image(area, big);
}

pub(crate) fn status_line(frame: &mut Frame<'_>, text: &str) {
    let width = frame.area().width;
    let style = Style::new().fg(Color::rgb(16, 18, 26)).bg(ACCENT);
    let buffer = frame.buffer_mut();
    buffer.set_style(Rect::new(0, 0, width, 1), style);
    buffer.set_string(0, 0, text, style.bold());
}

pub(crate) fn help_line(frame: &mut Frame<'_>, text: &str) {
    let y = frame.area().height.saturating_sub(1);
    frame
        .buffer_mut()
        .set_string(0, y, text, Style::new().dim());
}

/// A centered popup with a rounded border.
pub(crate) fn popup(frame: &mut Frame<'_>, title: &str, lines: Vec<Line<'_>>, accent: Color) {
    let width = lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 8;
    let area = frame.area().centered(width.max(40), lines.len() as u16 + 4);
    let buffer = frame.buffer_mut();
    Clear.render(area, buffer);
    Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(Style::new().bg(PANEL))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(accent))
                .title(Line::from(format!(" {title} ")).centered())
                .padding(Padding::vertical(1)),
        )
        .render(area, buffer);
}

/// Pixel text with a dark shadow, readable on light and dark ground.
pub(crate) fn shadow_text(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    text: &str,
    color: Rgba,
    scale: u32,
) {
    let font = Font::builtin();
    let step = scale as i32;
    let shadow = Rgba::new(8, 8, 14, color.a);
    canvas.text_scaled(font, x + step, y + step, text, shadow, scale);
    canvas.text_scaled(font, x, y, text, color, scale);
}

/// A time in tenths of a second as `m:ss.t`.
pub(crate) fn format_time(tenths: u32) -> String {
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vnce::vnce_game::{SaveFile, SceneStack};
    use vnce::vnce_runtime::{RunConfig, ScriptedDriver, run_with};
    use vnce::vnce_term::{
        Capabilities, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, Terminal, TestBackend,
    };

    use super::{Shared, bench, scenes};

    fn press(driver: &mut ScriptedDriver, at: u64, code: KeyCode) {
        for (offset, kind) in [(0, KeyEventKind::Press), (60, KeyEventKind::Release)] {
            driver.push_event(
                Duration::from_millis(at + offset),
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::NONE,
                    kind,
                }),
            );
        }
    }

    /// Plays from the title into the station and walks through it, opens the pause menu
    /// and the settings, on an emulated screen without GPU or sound.
    #[test]
    fn smoke_test_from_title_through_the_station() {
        let shared = Shared::new(SaveFile::new(), false, false);
        let mut game = bench::Limited {
            game: SceneStack::new(scenes::Title::new(), shared),
            frames: 1200,
        };
        let capabilities = Capabilities {
            key_release_events: true,
            ..Capabilities::default()
        };
        let terminal = Terminal::new(TestBackend::new(100, 34), capabilities)
            .expect("the test backend has a size");
        let mut driver = ScriptedDriver::new();
        press(&mut driver, 300, KeyCode::Enter);
        press(&mut driver, 600, KeyCode::Char('f'));
        bench::walk(&mut driver, 900);
        // Pause, open the settings, back, continue.
        press(&mut driver, 15_000, KeyCode::Char('p'));
        press(&mut driver, 15_300, KeyCode::Down);
        press(&mut driver, 15_600, KeyCode::Enter);
        press(&mut driver, 16_000, KeyCode::Esc);
        press(&mut driver, 16_300, KeyCode::Char('p'));
        run_with(&mut game, RunConfig::default(), terminal, driver).expect("the game runs");
    }

    /// The title and the station with GPU pictures (CPU where there is no GPU): the first
    /// frames come before the first pipelined readback, so the pictures are still empty.
    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_pictures_from_the_first_frame() {
        let shared = Shared::new(SaveFile::new(), true, false);
        let mut game = bench::Limited {
            game: SceneStack::new(scenes::Title::new(), shared),
            frames: 120,
        };
        let capabilities = Capabilities {
            key_release_events: true,
            ..Capabilities::default()
        };
        let terminal = Terminal::new(TestBackend::new(80, 24), capabilities)
            .expect("the test backend has a size");
        let mut driver = ScriptedDriver::new();
        press(&mut driver, 1000, KeyCode::Enter);
        run_with(&mut game, RunConfig::default(), terminal, driver).expect("the game runs");
    }
}
