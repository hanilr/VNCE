//! Glide: the 3D showcase of vnce. Fly a small craft low over endless low-poly land,
//! through rings, from sunrise towards sunset.
//!
//! The land is generated from a course code and built in chunks ahead of the craft. With
//! the `gpu` feature the meshes are drawn by `vnce_gpu::MeshRenderer` with a sun, ambient
//! light and fog, over a sky shader with clouds; without a GPU the CPU draws the same
//! world at lower detail. Two modes: a ring course against the clock with checkpoints,
//! and free flight where every ring adds time.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example glide --features gpu,audio
//! cargo run --release --example glide                 # CPU only
//! cargo run --release --example glide --features gpu -- --bench   # timings
//! ```
//!
//! `↑↓` / `WS`: climb / dive · `←→` / `AD`: bank · `E` / `Q`: throttle · `C`: camera ·
//! drag: turn the orbit camera · `O`: photo mode · `P` / `Esc`: pause · `G`: display
//! method · `F`: performance.

// Game code converts freely between pixels, counters and floats; every value stays far
// inside the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel coordinates and counters are small"
)]
// Geometry reads best with its usual names (x, y, z, u, v).
#![allow(clippy::many_single_char_names, reason = "geometry names")]
// The game was built in steps; parts used only by the tests or only by the GPU renderer
// are marked where they are.

mod audio;
mod bench;
mod course;
mod flight;
mod hud;
mod math;
mod models;
mod play;
mod render;
mod scenes;
mod sky;
mod terrain;

use std::cell::{Cell, RefCell};

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{ActionMap, SaveFile, SceneStack};
use vnce::vnce_raster::Canvas;
use vnce::vnce_runtime::{self, Context, FpsLimit, RunConfig};
use vnce::vnce_term::{Frame, GraphicsMode, KeyCode};
use vnce::vnce_ui::{Alignment, Block, BorderType, Clear, Line, Padding, Paragraph, Widget};

use crate::audio::Sounds;
use crate::render::Renderer;

/// What the player can do; keys are bound to these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Up,
    Down,
    Left,
    Right,
    Faster,
    Slower,
    Camera,
    Photo,
    Pause,
    Confirm,
    Back,
    Quit,
    Method,
    Perf,
}

/// Every action with its name in the save file.
pub(crate) const ACTIONS: [(Action, &str); 14] = [
    (Action::Up, "climb"),
    (Action::Down, "dive"),
    (Action::Left, "left"),
    (Action::Right, "right"),
    (Action::Faster, "faster"),
    (Action::Slower, "slower"),
    (Action::Camera, "camera"),
    (Action::Photo, "photo"),
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
        .bind(Action::Faster, 'e')
        .bind(Action::Slower, 'q')
        .bind(Action::Camera, 'c')
        .bind(Action::Photo, 'o')
        .bind(Action::Pause, 'p')
        .bind(Action::Pause, KeyCode::Esc)
        .bind(Action::Confirm, KeyCode::Enter)
        .bind(Action::Confirm, ' ')
        .bind(Action::Back, KeyCode::Esc)
        .bind(Action::Quit, 'x')
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

/// Render resolutions offered in the settings, in percent of the shown size.
pub(crate) const RESOLUTIONS: [u32; 4] = [100, 75, 50, 33];

/// The two ways to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Fly through a course of rings against the clock.
    Course,
    /// Fly as long as the rings keep the clock running.
    Free,
}

/// What a menu shown over another scene asks the scene below it to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Request {
    /// Fly this mode on the course with this seed.
    Play(Mode, u64),
    /// Go back to the title.
    Title,
}

/// Data shared by every scene.
pub(crate) struct Shared {
    pub(crate) controls: ActionMap<Action>,
    /// Display methods of this terminal.
    pub(crate) modes: Vec<GraphicsMode>,
    /// The seed of the last course flown, and how.
    pub(crate) last: u64,
    pub(crate) mode: Mode,
    /// Best free flight: rings.
    pub(crate) best_free: u32,
    /// Set by a menu for the scene below it.
    pub(crate) request: Option<Request>,
    /// Let the engine scale the picture (smooth) instead of whole pixels.
    pub(crate) smooth: bool,
    /// Render resolution in percent.
    pub(crate) resolution: u32,
    /// Show the performance panel.
    pub(crate) perf: bool,
    /// Volumes in percent.
    pub(crate) music_volume: u32,
    pub(crate) effects_volume: u32,
    pub(crate) sounds: Sounds,
    /// Drawing needs `&mut`, scenes draw with `&Shared`.
    pub(crate) renderer: RefCell<Renderer>,
    pub(crate) save: SaveFile,
    /// Milliseconds spent drawing the world, summed, and the frames counted (for the
    /// bench).
    pub(crate) render_time: Cell<(f64, u32)>,
}

impl Shared {
    /// Shared data from `save`, with the GPU when `gpu` (and the feature) allow it, and
    /// sound when `sound` is set and there is a device.
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
        let msaa = save.get_or("msaa", 4_u32);
        let mut renderer = Renderer::new(gpu, if msaa > 1 { 4 } else { 1 });
        renderer.set_gpu(gpu && save.get_or("gpu", true));
        let resolution = save.get_or("resolution", 100_u32);
        let shared = Self {
            controls,
            modes: Vec::new(),
            last: save
                .get_str("course")
                .and_then(course::parse_code)
                .unwrap_or(1),
            mode: Mode::Course,
            best_free: save.get_or("best_free", 0),
            request: None,
            smooth: save.get_or("smooth", false),
            resolution: if RESOLUTIONS.contains(&resolution) {
                resolution
            } else {
                100
            },
            perf: save.get_or("perf", false),
            music_volume: save.get_or("music_volume", 50_u32).min(100),
            effects_volume: save.get_or("effects_volume", 80_u32).min(100),
            sounds: Sounds::new(sound),
            renderer: RefCell::new(renderer),
            save,
            render_time: Cell::new((0.0, 0)),
        };
        shared
            .sounds
            .volumes(shared.music_volume, shared.effects_volume);
        shared
    }

    /// The best time of course `seed` in tenths of a second (0: none yet).
    pub(crate) fn best_time(&self, seed: u64) -> u32 {
        self.save.get_or(&format!("best.{}", course::code(seed)), 0)
    }

    pub(crate) fn set_best_time(&mut self, seed: u64, tenths: u32) {
        self.save
            .set(&format!("best.{}", course::code(seed)), tenths);
    }

    /// Writes the bests, the last course, the settings and the controls to the save file.
    pub(crate) fn store(&mut self, ctx: &Context) {
        let (gpu, msaa) = {
            let renderer = self.renderer.borrow();
            (renderer.using_gpu(), renderer.msaa())
        };
        let save = &mut self.save;
        save.set("best_free", self.best_free);
        save.set("course", course::code(self.last));
        save.set("music_volume", self.music_volume);
        save.set("effects_volume", self.effects_volume);
        save.set("smooth", self.smooth);
        save.set("resolution", self.resolution);
        save.set("perf", self.perf);
        save.set("gpu", gpu);
        save.set("msaa", msaa);
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
    let shared = Shared::new(SaveFile::open("glide"), !flag("--cpu"), true);
    let title = scenes::Title::new(&shared);
    let mut game = SceneStack::new(title, shared);
    let mut config = RunConfig::default();
    config.terminal.mouse_capture = true;
    vnce_runtime::run(&mut game, config)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Drawing helpers shared by the scenes
// ---------------------------------------------------------------------------------------

pub(crate) const ACCENT: Color = Color::Rgb(255, 200, 120);
pub(crate) const COOL: Color = Color::Rgb(140, 210, 255);
pub(crate) const PANEL: Color = Color::Rgb(16, 22, 34);

/// The area for the world (between the status line and the two bottom lines), the size
/// it is rendered at (the render resolution applied) and its aspect ratio.
pub(crate) fn view(shared: &Shared, ctx: &Context, frame: &Frame<'_>) -> (Rect, u32, u32, f32) {
    let size = ctx.size();
    let area = Rect::new(0, 1, size.width, size.height.saturating_sub(3));
    let (width, height) = frame.image_pixel_size(area);
    if width == 0 || height == 0 {
        return (area, 0, 0, 1.0);
    }
    let aspect = width as f32 / height as f32 * frame.image_pixel_aspect();
    let scale = shared.resolution as f32 / 100.0;
    let render = |v: u32| ((v as f32 * scale).round() as u32).max(1);
    (area, render(width), render(height), aspect)
}

/// Shows `canvas` enlarged to the exact pixel size of `area` (nearest neighbour), reusing
/// `big` between frames. Rows that repeat a source row are copied, not recomputed.
pub(crate) fn present(frame: &mut Frame<'_>, area: Rect, canvas: &Canvas, big: &mut Canvas) {
    let (width, height) = frame.image_pixel_size(area);
    if canvas.width() == 0 || canvas.height() == 0 {
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

/// Shows the world picture in `area`: enlarged in whole pixels by [`present`], or scaled
/// by the engine (with its scale quality) when the player chose smooth upscaling.
pub(crate) fn show(
    shared: &Shared,
    frame: &mut Frame<'_>,
    area: Rect,
    canvas: &Canvas,
    big: &mut Canvas,
) {
    if shared.smooth {
        frame.draw_image(area, canvas);
    } else {
        present(frame, area, canvas, big);
    }
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

/// A time in tenths of a second as `m:ss.t`.
pub(crate) fn format_time(tenths: u32) -> String {
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vnce::vnce_game::{SaveFile, SceneStack};
    use vnce::vnce_runtime::{RunConfig, ScriptedDriver, run_with};
    use vnce::vnce_term::input::{MouseButton, MouseEvent, MouseEventKind};
    use vnce::vnce_term::{
        Capabilities, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, Terminal, TestBackend,
    };

    use crate::math::{Camera, direction};
    use crate::play::Play;
    use crate::render::{Renderer, View};
    use crate::sky::daylight;
    use crate::terrain::Terrain;
    use crate::{Mode, Shared, bench, course, scenes};

    /// Presses `code` at `at` milliseconds and releases it `hold` milliseconds later.
    fn press(driver: &mut ScriptedDriver, at: u64, hold: u64, code: KeyCode) {
        for (offset, kind) in [(0, KeyEventKind::Press), (hold, KeyEventKind::Release)] {
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

    fn terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        let capabilities = Capabilities {
            key_release_events: true,
            ..Capabilities::default()
        };
        Terminal::new(TestBackend::new(width, height), capabilities)
            .expect("the test backend has a size")
    }

    /// Flies a course: climbs, banks, changes speed, switches cameras, drags the orbit
    /// camera, takes a photo and flies on, on the CPU.
    #[test]
    fn smoke_test_flying_a_course() {
        let mut game = bench::Limited {
            game: SceneStack::new(
                Play::new(Mode::Course, 5),
                Shared::new(SaveFile::new(), false, false),
            ),
            frames: 600,
        };
        let mut driver = ScriptedDriver::new();
        press(&mut driver, 3300, 900, KeyCode::Up);
        press(&mut driver, 4400, 1200, KeyCode::Left);
        press(&mut driver, 5000, 800, KeyCode::Char('e'));
        press(&mut driver, 5900, 60, KeyCode::Char('c'));
        press(&mut driver, 6400, 60, KeyCode::Char('c'));
        press(&mut driver, 6900, 600, KeyCode::Right);
        press(&mut driver, 7600, 60, KeyCode::Char('o'));
        for (at, column, kind) in [
            (7800, 40, MouseEventKind::Down(MouseButton::Left)),
            (7900, 50, MouseEventKind::Drag(MouseButton::Left)),
            (8000, 60, MouseEventKind::Drag(MouseButton::Left)),
            (8100, 60, MouseEventKind::Up(MouseButton::Left)),
        ] {
            driver.push_event(
                Duration::from_millis(at),
                Event::Mouse(MouseEvent {
                    kind,
                    column,
                    row: 12,
                    modifiers: KeyModifiers::NONE,
                }),
            );
        }
        press(&mut driver, 8400, 60, KeyCode::Char('o'));
        press(&mut driver, 8600, 60, KeyCode::Char('c'));
        press(&mut driver, 8800, 60, KeyCode::Char('f'));
        run_with(&mut game, RunConfig::default(), terminal(90, 30), driver).expect("the game runs");
    }

    /// From the title: types a course code, flies, pauses, changes a setting, and goes
    /// back to the title through the pause menu; the scene stack never grows.
    #[test]
    fn smoke_test_from_title_through_the_menus() {
        let shared = Shared::new(SaveFile::new(), false, false);
        let title = scenes::Title::new(&shared);
        let mut game = bench::Limited {
            game: SceneStack::new(title, shared),
            frames: 900,
        };
        let mut driver = ScriptedDriver::new();
        // Title: the fourth entry is "Course code…".
        for i in 0..3 {
            press(&mut driver, 300 + i * 200, 60, KeyCode::Down);
        }
        press(&mut driver, 1000, 60, KeyCode::Enter);
        for (i, c) in "k4p2qx".chars().enumerate() {
            press(&mut driver, 1300 + i as u64 * 120, 40, KeyCode::Char(c));
        }
        press(&mut driver, 2200, 60, KeyCode::Enter);
        // Flying: pause, open the settings, lower the resolution, back, continue.
        press(&mut driver, 5000, 600, KeyCode::Up);
        press(&mut driver, 5800, 60, KeyCode::Char('p'));
        press(&mut driver, 6100, 60, KeyCode::Down);
        press(&mut driver, 6300, 60, KeyCode::Enter);
        // Settings: the eighth row is the resolution.
        for i in 0..7 {
            press(&mut driver, 6600 + i * 150, 60, KeyCode::Down);
        }
        press(&mut driver, 7800, 60, KeyCode::Right);
        press(&mut driver, 8100, 60, KeyCode::Esc);
        press(&mut driver, 8400, 60, KeyCode::Char('p'));
        press(&mut driver, 8800, 600, KeyCode::Right);
        // Pause again and go back to the title (the fourth entry).
        press(&mut driver, 9800, 60, KeyCode::Char('p'));
        for i in 0..3 {
            press(&mut driver, 10_100 + i * 200, 60, KeyCode::Down);
        }
        press(&mut driver, 10_900, 60, KeyCode::Enter);
        run_with(&mut game, RunConfig::default(), terminal(90, 30), driver).expect("the game runs");
        let shared = game.game.data();
        assert_eq!(
            course::code(shared.last),
            "K4P2QX",
            "the typed course was flown"
        );
        assert_eq!(shared.resolution, 75, "the setting was changed");
        assert_eq!(game.game.len(), 1, "back on the title alone");
    }

    /// Writes pictures of the terrain to the folder in `GLIDE_SNAPSHOTS` (skipped without
    /// it), on the CPU and, with `GLIDE_GPU` set, on the GPU. For looking at the art.
    #[test]
    fn terrain_snapshots() {
        let Ok(folder) = std::env::var("GLIDE_SNAPSHOTS") else {
            return;
        };
        let gpu = std::env::var("GLIDE_GPU").is_ok();
        let terrain = Terrain::new(1);
        for (name, phase, yaw, pitch) in [
            ("morning", 0.08, 1.2, -0.15),
            ("noon", 0.5, 3.0, -0.3),
            ("evening", 0.97, 4.4, -0.1),
        ] {
            let mut renderer = Renderer::new(gpu, 4);
            let eye = [300.0, terrain.surface(300.0, 200.0).max(0.0) + 110.0, 200.0];
            let camera = Camera::new(eye, direction(yaw, pitch), [0.0, 1.0, 0.0], 1.6);
            renderer.warm_up(&terrain, &camera);
            let view = View {
                camera,
                light: daylight(phase),
                objects: Vec::new(),
                time: 3.0,
            };
            // The GPU's pictures arrive a frame late.
            for _ in 0..3 {
                renderer.render(&terrain, &view, 640, 400);
            }
            let png = renderer.canvas().to_png().expect("a canvas encodes as PNG");
            let kind = if renderer.using_gpu() { "gpu" } else { "cpu" };
            std::fs::write(format!("{folder}/terrain-{name}-{kind}.png"), png).expect("writable");
        }
    }

    /// Writes frames of a flight (chase, cockpit and orbit cameras) to `GLIDE_SNAPSHOTS`.
    #[test]
    fn flight_snapshots() {
        let Ok(folder) = std::env::var("GLIDE_SNAPSHOTS") else {
            return;
        };
        let gpu = std::env::var("GLIDE_GPU").is_ok();
        let shared = Shared::new(SaveFile::new(), gpu, false);
        let kind = if shared.renderer.borrow().using_gpu() {
            "gpu"
        } else {
            "cpu"
        };
        let mut play = Play::bench(3);
        for (index, seconds) in [6.0, 14.0, 22.0].into_iter().enumerate() {
            play.fly_for(seconds, &shared);
            play.set_camera(index);
            let png = play.snapshot(&shared, 640, 400);
            std::fs::write(format!("{folder}/flight-{index}-{kind}.png"), png).expect("writable");
        }
    }
}
