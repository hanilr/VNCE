//! Isle: the isometric 2.5D showcase of vnce. Night is coming to a small island; collect
//! its light crystals and light the beacon on the highest hill before it is dark.
//!
//! Every island is generated from a code, with height levels, stairs, forests and beaches.
//! The view is isometric and can be turned and zoomed; the terrain is drawn once per
//! direction and only the tiles in front of moving things are drawn again, so trees and
//! cliffs hide what is behind them.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example isle
//! cargo run --release --example isle --features gpu,audio   # GPU ocean and sound
//! ```
//!
//! `←↑→↓` / `WASD`: move · click: walk there · `Q` / `E`: turn · wheel / `+` `-`: zoom ·
//! `Space`: use · `P` / `Esc`: pause · `G`: display method · `F`: performance.

// Game code converts freely between pixels, tiles and floats; every value stays far inside
// the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel and tile coordinates are small"
)]
// Geometry reads best with its usual names (x, y, u, v, h).
#![allow(clippy::many_single_char_names, reason = "geometry names")]
#![allow(dead_code, reason = "the game is built up step by step")]

mod art;
mod audio;
mod bench;
mod island;
mod iso;
mod life;
mod minimap;
mod play;
mod sky;
mod visuals;

use std::cell::RefCell;

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{ActionMap, SceneStack};
use vnce::vnce_raster::{Canvas, Font, Rgba};
use vnce::vnce_runtime::{self, Context, RunConfig};
use vnce::vnce_term::input::MouseButton;
use vnce::vnce_term::{Frame, KeyCode};

use crate::audio::Sounds;
use crate::iso::TILE_HEIGHT;
use crate::visuals::Visuals;

/// What the player can do; keys and mouse buttons are bound to these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Up,
    Down,
    Left,
    Right,
    Use,
    TurnLeft,
    TurnRight,
    ZoomIn,
    ZoomOut,
    Walk,
    Pause,
    Confirm,
    Back,
    Quit,
    Method,
    Perf,
    Map,
}

/// Every action with its name in the save file.
pub(crate) const ACTIONS: [(Action, &str); 17] = [
    (Action::Up, "up"),
    (Action::Down, "down"),
    (Action::Left, "left"),
    (Action::Right, "right"),
    (Action::Use, "use"),
    (Action::TurnLeft, "turn_left"),
    (Action::TurnRight, "turn_right"),
    (Action::ZoomIn, "zoom_in"),
    (Action::ZoomOut, "zoom_out"),
    (Action::Walk, "walk"),
    (Action::Pause, "pause"),
    (Action::Confirm, "confirm"),
    (Action::Back, "back"),
    (Action::Quit, "quit"),
    (Action::Method, "method"),
    (Action::Perf, "perf"),
    (Action::Map, "map"),
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
        .bind(Action::Use, ' ')
        .bind(Action::TurnLeft, 'q')
        .bind(Action::TurnRight, 'e')
        .bind(Action::ZoomIn, '+')
        .bind(Action::ZoomIn, '=')
        .bind(Action::ZoomOut, '-')
        .bind(Action::Walk, MouseButton::Left)
        .bind(Action::Pause, 'p')
        .bind(Action::Pause, KeyCode::Esc)
        .bind(Action::Confirm, KeyCode::Enter)
        .bind(Action::Confirm, ' ')
        .bind(Action::Back, KeyCode::Esc)
        .bind(Action::Quit, 'x')
        .bind(Action::Method, 'g')
        .bind(Action::Perf, 'f')
        .bind(Action::Map, 'm')
}

/// Data shared by every scene.
pub(crate) struct Shared {
    pub(crate) controls: ActionMap<Action>,
    /// Show the performance panel.
    pub(crate) perf: bool,
    /// Show the minimap.
    pub(crate) map: bool,
    /// Volumes in percent.
    pub(crate) music_volume: u32,
    pub(crate) effects_volume: u32,
    pub(crate) sounds: Sounds,
    /// The ocean picture; drawing needs `&mut`, scenes draw with `&Shared`.
    pub(crate) visuals: RefCell<Visuals>,
}

impl Shared {
    /// Shared data with the GPU ocean when `gpu` (and the feature) allow it, and sound when
    /// `sound` is set and there is a device.
    pub(crate) fn new(gpu: bool, sound: bool) -> Self {
        let shared = Self {
            controls: default_controls(),
            perf: false,
            map: true,
            music_volume: 60,
            effects_volume: 80,
            sounds: Sounds::new(sound),
            visuals: RefCell::new(Visuals::new(gpu)),
        };
        shared
            .sounds
            .volumes(shared.music_volume, shared.effects_volume);
        shared
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let shared = Shared::new(!flag("--cpu"), true);
    let mut game = SceneStack::new(play::Play::new(1), shared);
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

/// Tile rows shown from top to bottom at each zoom level.
pub(crate) const ZOOM_ROWS: [f32; 4] = [34.0, 24.0, 17.0, 12.0];

/// The area for the world (between the status line and the two bottom lines) and the
/// size of the world view in world pixels for zoom level `zoom`, with square world
/// pixels.
pub(crate) fn view(ctx: &Context, frame: &Frame<'_>, zoom: usize) -> (Rect, u32, u32) {
    let size = ctx.size();
    let area = Rect::new(0, 1, size.width, size.height.saturating_sub(3));
    let (width, height) = frame.image_pixel_size(area);
    if width == 0 || height == 0 {
        return (area, 0, 0);
    }
    let rows = ZOOM_ROWS[zoom.min(ZOOM_ROWS.len() - 1)];
    let scale = (height as f32 / (rows * TILE_HEIGHT)).round().max(1.0);
    let view_width = (width as f32 * frame.image_pixel_aspect() / scale).round() as u32;
    (
        area,
        view_width.max(1),
        (height as f32 / scale).max(1.0) as u32,
    )
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

/// Writes `canvas` as a PNG, `zoom` times as large (nearest neighbour), for snapshot
/// tests.
#[cfg(test)]
pub(crate) fn write_png(canvas: &Canvas, path: &str, zoom: u32) {
    let (width, height) = (canvas.width() * zoom, canvas.height() * zoom);
    let mut big = Canvas::new(width, height);
    for (i, pixel) in big.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (i as u32 % width / zoom, i as u32 / width / zoom);
        *pixel = canvas.get(x, y).unwrap_or_default();
    }
    let png = big.to_png().expect("a canvas encodes as PNG");
    std::fs::write(path, png).expect("writable");
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vnce::vnce_game::SceneStack;
    use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Rgba};
    use vnce::vnce_runtime::{RunConfig, ScriptedDriver, run_with};
    use vnce::vnce_term::input::{MouseButton, MouseEvent, MouseEventKind};
    use vnce::vnce_term::{
        Capabilities, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, Terminal, TestBackend,
    };

    use crate::island::Island;
    use crate::iso::{Sprites, Terrain};
    use crate::{Shared, bench, play};

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

    /// Walks, turns and zooms on an emulated screen.
    #[test]
    fn smoke_test_walking_turning_zooming() {
        let mut game = bench::Limited {
            game: SceneStack::new(play::Play::new(5), Shared::new(false, false)),
            frames: 420,
        };
        let capabilities = Capabilities {
            key_release_events: true,
            ..Capabilities::default()
        };
        let terminal = Terminal::new(TestBackend::new(100, 34), capabilities)
            .expect("the test backend has a size");
        let mut driver = ScriptedDriver::new();
        press(&mut driver, 100, 900, KeyCode::Up);
        press(&mut driver, 1100, 60, KeyCode::Char('e'));
        press(&mut driver, 1700, 700, KeyCode::Left);
        press(&mut driver, 2500, 60, KeyCode::Char('q'));
        press(&mut driver, 3100, 60, KeyCode::Char('+'));
        press(&mut driver, 3300, 600, KeyCode::Down);
        press(&mut driver, 4000, 60, KeyCode::Char('-'));
        press(&mut driver, 4200, 60, KeyCode::Char('-'));
        // Hover and click a few cells: walks start (or the tile is out of reach).
        for (at, column, row) in [(4500, 30, 10), (4800, 70, 20), (5600, 50, 17)] {
            for (offset, kind) in [
                (0, MouseEventKind::Moved),
                (40, MouseEventKind::Down(MouseButton::Left)),
                (90, MouseEventKind::Up(MouseButton::Left)),
            ] {
                driver.push_event(
                    Duration::from_millis(at + offset),
                    Event::Mouse(MouseEvent {
                        kind,
                        column,
                        row,
                        modifiers: KeyModifiers::NONE,
                    }),
                );
            }
        }
        run_with(&mut game, RunConfig::default(), terminal, driver).expect("the game runs");
    }

    /// Writes the terrain of a few islands from every side to the folder in
    /// `ISLE_SNAPSHOTS` (skipped without it). For looking at the art, not a check.
    #[test]
    fn terrain_snapshots() {
        let Ok(folder) = std::env::var("ISLE_SNAPSHOTS") else {
            return;
        };
        let sprites = Sprites::new();
        for seed in [1, 2] {
            let island = Island::generate(seed);
            let mut terrain = Terrain::new();
            for quarter in 0..4 {
                let picture = terrain.picture(&island, &sprites, quarter);
                let mut shown =
                    Canvas::filled(picture.width(), picture.height(), Rgba::rgb(30, 90, 140));
                shown.set_blend_mode(BlendMode::Alpha);
                shown.blit(picture, 0, 0, BlitOptions::default());
                let png = shown.to_png().expect("a canvas encodes as PNG");
                std::fs::write(format!("{folder}/terrain-{seed}-{quarter}.png"), png)
                    .expect("writable");
                // The middle, three times as large, to see the pixels.
                let (w, h) = (320, 200);
                let (x0, y0) = ((shown.width() - w) / 2, (shown.height() - h) / 2);
                let mut zoom = Canvas::new(w * 3, h * 3);
                for (i, pixel) in zoom.pixels_mut().iter_mut().enumerate() {
                    let (x, y) = (i as u32 % (w * 3) / 3, i as u32 / (w * 3) / 3);
                    *pixel = shown.get(x0 + x, y0 + y).unwrap_or(Rgba::BLACK);
                }
                let png = zoom.to_png().expect("a canvas encodes as PNG");
                std::fs::write(format!("{folder}/zoom-{seed}-{quarter}.png"), png)
                    .expect("writable");
            }
        }
    }
}
