//! Lantern: an original top-down cave game built with `vnce_game`. Explore dark caves with
//! a lantern, collect every gem, then reach the exit before the bats catch you. Every cave
//! is generated anew; caves get bigger, darker and busier as you go deeper.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example lantern
//! cargo run --release --example lantern --features audio   # with sound and music
//! ```
//!
//! `←↑→↓` / `WASD`: move · `P` / `Esc`: pause · `G`: display method · `Q`: quit.
//!
//! The best score, the settings and the controls are kept in a save file (see
//! `vnce_game::data_dir`), which can also be edited by hand.

// Game code converts freely between pixels, tiles and floats; every value stays far inside
// the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel and tile coordinates are small"
)]

use std::collections::VecDeque;
use std::time::Duration;

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{
    Aabb, ActionMap, Animation, Animator, Binding, Camera, Playback, Rng, SaveFile, Scene,
    SceneStack, SpatialGrid, SpriteSheet, TileMap, Tileset, Timer, Transition, ease,
    move_and_collide,
};
use vnce::vnce_raster::{BlitOptions, Canvas, Font, Rgba};
use vnce::vnce_runtime::{self, Context, FpsLimit, RunConfig};
use vnce::vnce_term::{Event, Frame, GraphicsMode, KeyCode, KeyEventKind};
use vnce::vnce_ui::{
    Alignment, Block, BorderType, Clear, Line, List, ListState, Paragraph, StatefulWidget, Widget,
};

/// Size of a tile in world pixels.
const TILE: u32 = 8;
const PLAYER_SPEED: f32 = 58.0;
const LIVES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Up,
    Down,
    Left,
    Right,
    Pause,
    Confirm,
    Back,
    Quit,
    Method,
}

/// Every action with its name in the save file.
const ACTIONS: [(Action, &str); 9] = [
    (Action::Up, "up"),
    (Action::Down, "down"),
    (Action::Left, "left"),
    (Action::Right, "right"),
    (Action::Pause, "pause"),
    (Action::Confirm, "confirm"),
    (Action::Back, "back"),
    (Action::Quit, "quit"),
    (Action::Method, "method"),
];

/// Data shared by every scene.
struct Shared {
    controls: ActionMap<Action>,
    modes: Vec<GraphicsMode>,
    best: u32,
    /// Volumes in percent.
    music_volume: u32,
    effects_volume: u32,
    sounds: Sounds,
    save: SaveFile,
}

impl Shared {
    /// Writes the best score, the settings and the controls to the save file.
    fn store(&mut self, ctx: &Context) {
        let save = &mut self.save;
        save.set("best", self.best);
        save.set("music_volume", self.music_volume);
        save.set("effects_volume", self.effects_volume);
        save.set("frame_rate", ctx.fps_limit());
        save.set("display", ctx.graphics_mode());
        for (action, name) in ACTIONS {
            save.set_bindings(&format!("keys.{name}"), self.controls.bindings(action));
        }
        // A game must keep running when the disk is full or read-only; the values stay
        // in memory and the next save tries again.
        let _ = save.save();
    }

    /// Restores the frame rate and the display method saved by [`Shared::store`].
    fn restore_screen(&self, ctx: &mut Context) {
        let rate = self.save.get_str("frame_rate");
        if let Some(&limit) = FRAME_RATES
            .iter()
            .find(|l| Some(l.to_string().as_str()) == rate)
        {
            // Every offered rate is valid.
            let _ = ctx.set_fps_limit(limit);
        }
        let mode = self.save.get::<GraphicsMode>("display");
        if let Some(mode) = mode.filter(|mode| self.modes.contains(mode)) {
            ctx.set_graphics_mode(mode);
        }
    }
}

/// The controls before the player changes them.
fn default_controls() -> ActionMap<Action> {
    ActionMap::new()
        .bind(Action::Up, KeyCode::Up)
        .bind(Action::Up, 'w')
        .bind(Action::Down, KeyCode::Down)
        .bind(Action::Down, 's')
        .bind(Action::Left, KeyCode::Left)
        .bind(Action::Left, 'a')
        .bind(Action::Right, KeyCode::Right)
        .bind(Action::Right, 'd')
        .bind(Action::Pause, 'p')
        .bind(Action::Pause, KeyCode::Esc)
        .bind(Action::Confirm, KeyCode::Enter)
        .bind(Action::Confirm, ' ')
        .bind(Action::Back, KeyCode::Esc)
        .bind(Action::Quit, 'q')
        .bind(Action::Method, 'g')
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut controls = default_controls();
    let save = SaveFile::open("lantern");
    for (action, name) in ACTIONS {
        if let Some(bindings) = save.bindings(&format!("keys.{name}")) {
            controls.unbind(action);
            for binding in bindings {
                controls.add(action, binding);
            }
        }
    }
    let shared = Shared {
        controls,
        modes: Vec::new(),
        best: save.get_or("best", 0),
        music_volume: save.get_or("music_volume", 60_u32).min(100),
        effects_volume: save.get_or("effects_volume", 80_u32).min(100),
        sounds: Sounds::new(),
        save,
    };
    let mut game = SceneStack::new(Title::new(), shared);
    vnce_runtime::run(&mut game, RunConfig::default())?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Random numbers and cave generation
// ---------------------------------------------------------------------------------------

/// A cave: `true` cells are walls.
struct Cave {
    width: u32,
    height: u32,
    walls: Vec<bool>,
    start: (u32, u32),
    /// Steps from the start to every cell (`u32::MAX` = unreachable).
    distance: Vec<u32>,
}

impl Cave {
    fn wall(&self, x: i32, y: i32) -> bool {
        x < 0
            || y < 0
            || x >= self.width as i32
            || y >= self.height as i32
            || self.walls[(y as u32 * self.width + x as u32) as usize]
    }

    /// Generates a cave with cellular automata and keeps the part reachable from the
    /// start. Tries new layouts until the cave is big enough.
    fn generate(rng: &mut Rng, width: u32, height: u32) -> Self {
        loop {
            let mut walls: Vec<bool> = (0..width * height)
                .map(|i| {
                    let (x, y) = (i % width, i / width);
                    x == 0 || y == 0 || x == width - 1 || y == height - 1 || rng.unit() < 0.45
                })
                .collect();
            for _ in 0..5 {
                let before = walls.clone();
                for y in 1..height - 1 {
                    for x in 1..width - 1 {
                        let mut around = 0;
                        for dy in -1_i32..=1 {
                            for dx in -1_i32..=1 {
                                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                                if (dx, dy) != (0, 0)
                                    && before[(ny as u32 * width + nx as u32) as usize]
                                {
                                    around += 1;
                                }
                            }
                        }
                        walls[(y * width + x) as usize] = around >= 5;
                    }
                }
            }
            // A floor cell near the middle is the start.
            let middle = (width / 2, height / 2);
            let Some(start) = (0..width * height)
                .filter(|&i| !walls[i as usize])
                .map(|i| (i % width, i / width))
                .min_by_key(|&(x, y)| x.abs_diff(middle.0) + y.abs_diff(middle.1))
            else {
                continue;
            };
            let mut cave = Self {
                width,
                height,
                walls,
                start,
                distance: Vec::new(),
            };
            cave.measure();
            let reachable = cave.distance.iter().filter(|&&d| d != u32::MAX).count();
            if reachable * 100 < (width * height) as usize * 35 {
                continue;
            }
            // Unreachable pockets become rock.
            for (wall, distance) in cave.walls.iter_mut().zip(&cave.distance) {
                if *distance == u32::MAX {
                    *wall = true;
                }
            }
            return cave;
        }
    }

    /// Breadth-first distances from the start.
    fn measure(&mut self) {
        let size = (self.width * self.height) as usize;
        self.distance = vec![u32::MAX; size];
        let index = |x: u32, y: u32| (y * self.width + x) as usize;
        self.distance[index(self.start.0, self.start.1)] = 0;
        let mut queue = VecDeque::from([self.start]);
        while let Some((x, y)) = queue.pop_front() {
            let steps = self.distance[index(x, y)];
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if !self.wall(nx, ny) && self.distance[index(nx as u32, ny as u32)] == u32::MAX {
                    self.distance[index(nx as u32, ny as u32)] = steps + 1;
                    queue.push_back((nx as u32, ny as u32));
                }
            }
        }
    }

    /// Floor cells at least `min` steps from the start, in random order.
    fn far_cells(&self, rng: &mut Rng, min: u32) -> Vec<(u32, u32)> {
        let mut cells: Vec<(u32, u32)> = (0..self.width * self.height)
            .filter(|&i| {
                let d = self.distance[i as usize];
                d != u32::MAX && d >= min
            })
            .map(|i| (i % self.width, i / self.width))
            .collect();
        rng.shuffle(&mut cells);
        cells
    }

    /// The reachable floor cell farthest from the start.
    fn farthest(&self) -> (u32, u32) {
        let index = (0..self.width * self.height)
            .filter(|&i| self.distance[i as usize] != u32::MAX)
            .max_by_key(|&i| self.distance[i as usize])
            .unwrap_or(0);
        (index % self.width, index / self.width)
    }

    /// The cave as a tile map: 0 rock, 1 floor, 2 rock edge (rock next to floor), and
    /// 3 to 5 more floor variants, picked by position so the floor does not look like a grid.
    fn tile_map(&self) -> TileMap {
        let mut text = String::new();
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let c = if !self.wall(x, y) {
                    let mut pick = Rng::new((u64::from(x as u32) << 32) | u64::from(y as u32));
                    ['.', 'a', 'b', 'c'][pick.below(4) as usize]
                } else if [(0, 1), (0, -1), (1, 0), (-1, 0)]
                    .iter()
                    .any(|(dx, dy)| !self.wall(x + dx, y + dy))
                {
                    'e'
                } else {
                    '#'
                };
                text.push(c);
            }
            text.push('\n');
        }
        TileMap::from_text(&text, TILE, |c| match c {
            '#' => Some(0),
            '.' => Some(1),
            'a' => Some(3),
            'b' => Some(4),
            'c' => Some(5),
            _ => Some(2),
        })
    }
}

// ---------------------------------------------------------------------------------------
// Art (original, drawn in code)
// ---------------------------------------------------------------------------------------

/// Tiles: 0 rock, 1 floor, 2 rock edge, 3 to 5 more floor (see `Cave::tile_map`). Specks
/// come from a fixed random sequence per tile, so every floor variant looks different.
fn tileset() -> Tileset {
    let rock = Rgba::rgb(28, 24, 34);
    let floor = Rgba::rgb(84, 74, 66);
    let dark = Rgba::rgb(70, 62, 58);
    let light = Rgba::rgb(98, 88, 78);
    let mut tiles = Vec::new();
    for kind in 0..6_u32 {
        let mut rng = Rng::new(u64::from(kind) + 1);
        let mut tile = Canvas::new(TILE, TILE);
        for y in 0..TILE {
            for x in 0..TILE {
                let color = match kind {
                    0 => rock,
                    2 if y < 2 => Rgba::rgb(120, 104, 92),
                    2 if rng.chance(0.15) => Rgba::rgb(52, 44, 52),
                    2 => Rgba::rgb(64, 54, 62),
                    _ if rng.chance(0.12) => dark,
                    _ if rng.chance(0.04) => light,
                    _ => floor,
                };
                tile.pixel(x as i32, y as i32, color);
            }
        }
        // A pebble on one floor variant and a crack on another.
        let (x, y) = (rng.range(1..6), rng.range(1..6));
        if kind == 4 {
            tile.fill_rect(x, y, 2, 1, light);
            tile.fill_rect(x, y + 1, 2, 1, dark);
        } else if kind == 5 {
            tile.line(x, y, x + 2, y + 1, dark);
        }
        tiles.push(tile);
    }
    Tileset::new(SpriteSheet::from_frames(&tiles)).with_solid([0, 2])
}

fn player_sprite() -> Canvas {
    Canvas::from_art(
        &[
            "..kkkk..", ".kssssk.", ".ksksks.", ".kssssk.", "..kbbk..", ".ybbbbk.", "yykbbk..",
            "..k..k..",
        ],
        &[
            ('k', Rgba::rgb(20, 16, 24)),
            ('s', Rgba::rgb(240, 200, 160)),
            ('b', Rgba::rgb(60, 110, 200)),
            ('y', Rgba::rgb(255, 220, 90)),
        ],
    )
}

fn bat_sheet() -> SpriteSheet {
    let palette = [('k', Rgba::rgb(40, 20, 50)), ('r', Rgba::rgb(230, 60, 60))];
    let up = Canvas::from_art(
        &[
            "k......k", "kk....kk", "kkkrrkkk", ".kkkkkk.", "...kk...", "........",
        ],
        &palette,
    );
    let down = Canvas::from_art(
        &[
            "........", "...kk...", ".kkrrkk.", "kkkkkkkk", "kk....kk", "k......k",
        ],
        &palette,
    );
    SpriteSheet::from_frames(&[up, down])
}

fn gem_sheet() -> SpriteSheet {
    let palette = [
        ('c', Rgba::rgb(90, 230, 255)),
        ('w', Rgba::rgb(240, 255, 255)),
        ('d', Rgba::rgb(40, 140, 200)),
    ];
    let a = Canvas::from_art(&["..c..", ".cwc.", "ccwcc", ".cdc.", "..d.."], &palette);
    let b = Canvas::from_art(&["..c..", ".ccc.", "cwccc", ".cdc.", "..d.."], &palette);
    SpriteSheet::from_frames(&[a, b])
}

fn exit_sprite() -> Canvas {
    Canvas::from_art(
        &[
            "..gggg..", ".gYYYYg.", "gYyyyyYg", "gYyWWyYg", "gYyWWyYg", "gYyyyyYg", ".gYYYYg.",
            "..gggg..",
        ],
        &[
            ('g', Rgba::rgb(60, 160, 80)),
            ('Y', Rgba::rgb(120, 230, 120)),
            ('y', Rgba::rgb(190, 255, 170)),
            ('W', Rgba::rgb(255, 255, 255)),
        ],
    )
}

// ---------------------------------------------------------------------------------------
// Sound
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sfx {
    Gem,
    Hit,
    Open,
    Blip,
}

#[cfg(feature = "audio")]
struct Sounds {
    audio: Option<vnce::vnce_audio::Audio>,
    gem: vnce::vnce_audio::Sound,
    hit: vnce::vnce_audio::Sound,
    open: vnce::vnce_audio::Sound,
    blip: vnce::vnce_audio::Sound,
    music: vnce::vnce_audio::Sound,
    music_voice: std::cell::RefCell<Option<vnce::vnce_audio::Voice>>,
}

#[cfg(feature = "audio")]
impl Sounds {
    fn new() -> Self {
        use vnce::vnce_audio::{Audio, Melody, Preset, Sound, Synth, Wave};

        let audio = Audio::new().ok();
        let rate = audio.as_ref().map_or(44_100, Audio::sample_rate);
        let effect = |preset, seed| Synth::preset(preset, seed).render(rate);
        let drone = Synth {
            wave: Wave::Triangle,
            attack: 0.08,
            decay: 0.2,
            sustain: 0.6,
            release: 0.3,
            volume: 0.45,
            low_pass: 1500.0,
            ..Synth::default()
        };
        // An original slow, low loop in A minor.
        let music = Melody::new(76.0, drone)
            .render("A2:2 C3:2 E3:2 D3:2 A2:2 F2:2 G2:2 E2:2", rate)
            .unwrap_or_else(|_| Sound::mono(rate, Vec::new()));
        Self {
            audio,
            gem: effect(Preset::Coin, 3),
            hit: effect(Preset::Hit, 0),
            open: effect(Preset::PowerUp, 0),
            blip: effect(Preset::Blip, 0),
            music,
            music_voice: std::cell::RefCell::new(None),
        }
    }

    fn play(&self, sfx: Sfx) {
        if let Some(audio) = &self.audio {
            let sound = match sfx {
                Sfx::Gem => &self.gem,
                Sfx::Hit => &self.hit,
                Sfx::Open => &self.open,
                Sfx::Blip => &self.blip,
            };
            audio.play(sound);
        }
    }

    fn music(&self, on: bool) {
        use vnce::vnce_audio::PlayOptions;
        let Some(audio) = &self.audio else {
            return;
        };
        let mut voice = self.music_voice.borrow_mut();
        if on && voice.is_none() {
            *voice = Some(audio.play_with(
                &self.music,
                PlayOptions {
                    fade_in: Duration::from_millis(800),
                    ..PlayOptions::music()
                },
            ));
        } else if !on {
            if let Some(voice) = voice.take() {
                voice.stop(Duration::from_millis(500));
            }
        }
    }

    fn volumes(&self, music: u32, effects: u32) {
        use vnce::vnce_audio::Group;
        if let Some(audio) = &self.audio {
            audio.set_group_volume(Group::Music, music as f32 / 100.0);
            audio.set_group_volume(Group::Effects, effects as f32 / 100.0);
        }
    }

    fn available(&self) -> bool {
        self.audio.is_some()
    }
}

/// Without the `audio` feature the game is silent.
#[cfg(not(feature = "audio"))]
struct Sounds;

#[cfg(not(feature = "audio"))]
#[allow(
    clippy::unused_self,
    reason = "same methods as the audio version, which uses `self`"
)]
impl Sounds {
    fn new() -> Self {
        Self
    }

    fn play(&self, _sfx: Sfx) {}

    fn music(&self, _on: bool) {}

    fn volumes(&self, _music: u32, _effects: u32) {}

    fn available(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------------------
// Presenting
// ---------------------------------------------------------------------------------------

/// The area for the world and the size of the world view in world pixels.
fn view(ctx: &Context, frame: &Frame<'_>) -> (Rect, u32, u32) {
    let size = ctx.size();
    let area = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
    let (width, height) = frame.image_pixel_size(area);
    if width == 0 || height == 0 {
        return (area, 0, 0);
    }
    // At most about 14 tiles high.
    let scale = (height / (TILE * 14)).max(1);
    let view_width = (width as f32 * frame.image_pixel_aspect() / scale as f32).round() as u32;
    (area, view_width.max(1), (height / scale).max(1))
}

/// Shows `canvas` enlarged to the exact pixel size of `area` (nearest neighbour).
fn present(frame: &mut Frame<'_>, area: Rect, canvas: &Canvas) {
    let (width, height) = frame.image_pixel_size(area);
    if (width, height) == (canvas.width(), canvas.height()) || width == 0 || height == 0 {
        frame.draw_image(area, canvas);
        return;
    }
    let columns: Vec<usize> = (0..width)
        .map(|x| (u64::from(x) * u64::from(canvas.width()) / u64::from(width)) as usize)
        .collect();
    let stride = canvas.width() as usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let row = (u64::from(y) * u64::from(canvas.height()) / u64::from(height)) as usize;
        let line = &canvas.pixels()[row * stride..(row + 1) * stride];
        pixels.extend(columns.iter().map(|&x| line[x]));
    }
    if let Some(big) = Canvas::from_pixels(width, height, pixels) {
        frame.draw_image(area, &big);
    }
}

fn status_line(frame: &mut Frame<'_>, text: &str) {
    let width = frame.area().width;
    let style = Style::new()
        .fg(Color::rgb(20, 16, 24))
        .bg(Color::rgb(255, 210, 110));
    let buffer = frame.buffer_mut();
    buffer.set_style(Rect::new(0, 0, width, 1), style);
    buffer.set_string(0, 0, text, style.bold());
}

fn help_line(frame: &mut Frame<'_>, text: &str) {
    let y = frame.area().height.saturating_sub(1);
    frame
        .buffer_mut()
        .set_string(0, y, text, Style::new().dim());
}

fn popup(frame: &mut Frame<'_>, title: &str, lines: Vec<Line<'_>>, accent: Color) {
    let area = frame.area().centered(44, lines.len() as u16 + 4);
    let buffer = frame.buffer_mut();
    Clear.render(area, buffer);
    Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(Style::new().bg(Color::rgb(22, 18, 30)))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(accent))
                .title(Line::from(format!(" {title} ")).centered())
                .padding(vnce::vnce_ui::Padding::vertical(1)),
        )
        .render(area, buffer);
}

fn cycle_method(shared: &Shared, ctx: &mut Context) {
    if shared.modes.is_empty() {
        return;
    }
    let current = ctx.graphics_mode();
    let index = shared.modes.iter().position(|&m| m == current).unwrap_or(0);
    ctx.set_graphics_mode(shared.modes[(index + 1) % shared.modes.len()]);
}

/// Darkness outside the lantern light: every pixel is dimmed by its distance from
/// `light` (in view pixels), with a warm tint near the flame.
fn apply_light(canvas: &mut Canvas, light: (f32, f32), radius: f32, flicker: f32) {
    let width = canvas.width() as usize;
    let radius = radius * (1.0 + 0.04 * flicker);
    for (index, pixel) in canvas.pixels_mut().iter_mut().enumerate() {
        let (x, y) = ((index % width) as f32, (index / width) as f32);
        let d = ((x - light.0).powi(2) + (y - light.1).powi(2)).sqrt() / radius;
        let bright = 1.0 - ease::in_out_quad(d.clamp(0.0, 1.0));
        let level = 0.06 + 0.94 * bright;
        let warm = 1.0 + 0.15 * bright;
        pixel.r = (f32::from(pixel.r) * level * warm).min(255.0) as u8;
        pixel.g = (f32::from(pixel.g) * level).min(255.0) as u8;
        pixel.b = (f32::from(pixel.b) * level * (2.0 - warm)).min(255.0) as u8;
    }
}

/// Pixel text with a dark shadow, so it stays readable on light and dark ground.
fn shadow_text(canvas: &mut Canvas, x: i32, y: i32, text: &str, color: Rgba, scale: u32) {
    let font = Font::builtin();
    let shadow = Rgba::rgb(12, 10, 16);
    let step = scale as i32;
    canvas.text_scaled(font, x + step, y + step, text, shadow, scale);
    canvas.text_scaled(font, x, y, text, color, scale);
}

/// The heads-up display drawn into the world view: depth and score on the left, lives as
/// hearts and the gem count on the right.
fn draw_hud(canvas: &mut Canvas, level: u32, score: u32, lives: u32, goal: &str) {
    let gold = Rgba::rgb(255, 210, 110);
    shadow_text(canvas, 2, 2, &format!("DEPTH {level}"), gold, 1);
    shadow_text(canvas, 2, 11, &format!("SCORE {score}"), Rgba::WHITE, 1);
    let heart = Canvas::from_art(
        &[".#.#.", "#####", "#####", ".###.", "..#.."],
        &[('#', Rgba::rgb(235, 70, 80))],
    );
    let right = canvas.width() as i32 - 2;
    for life in 0..lives as i32 {
        canvas.blit(
            &heart,
            right - 6 * (life + 1) + 1,
            3,
            BlitOptions::default(),
        );
    }
    let (goal_width, _) = Font::builtin().measure(goal);
    shadow_text(canvas, right - goal_width as i32, 11, goal, gold, 1);
}

// ---------------------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------------------

struct Title {
    menu: ListState,
    time: f32,
}

impl Title {
    fn new() -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
            time: 0.0,
        }
    }
}

const TITLE_MENU: [&str; 3] = ["Start", "Settings", "Quit"];

impl Scene<Shared> for Title {
    fn start(&mut self, shared: &mut Shared, ctx: &mut Context) {
        shared.sounds.music(false);
        shared
            .sounds
            .volumes(shared.music_volume, shared.effects_volume);
        if shared.modes.is_empty() {
            shared.modes = GraphicsMode::available(ctx.terminal_info(), &ctx.capabilities());
            if let Some(&cells) = shared.modes.iter().find(|m| !m.is_protocol()) {
                ctx.set_graphics_mode(cells);
            }
            shared.restore_screen(ctx);
        }
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        self.time += ctx.dt_secs();
        let input = ctx.input();
        let controls = &shared.controls;
        if controls.was_pressed(Action::Up, input) {
            self.menu.select_previous(TITLE_MENU.len(), true);
        }
        if controls.was_pressed(Action::Down, input) {
            self.menu.select_next(TITLE_MENU.len(), true);
        }
        if controls.was_pressed(Action::Method, input) {
            cycle_method(shared, ctx);
        }
        if shared.controls.was_pressed(Action::Quit, ctx.input()) {
            shared.store(ctx);
            return Transition::Quit;
        }
        if shared.controls.was_pressed(Action::Confirm, ctx.input()) {
            shared.sounds.play(Sfx::Blip);
            return match self.menu.selected() {
                Some(0) => Transition::switch(Play::new(1, 0, LIVES, self.time.to_bits().into())),
                Some(1) => Transition::push(Settings::new()),
                _ => {
                    shared.store(ctx);
                    Transition::Quit
                }
            };
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let (area, width, height) = view(ctx, frame);
        if width > 0 {
            // A slowly drifting glow over rock.
            let mut canvas = Canvas::filled(width, height, Rgba::rgb(64, 54, 62));
            let light = (
                width as f32 / 2.0 + (self.time * 0.7).sin() * width as f32 * 0.25,
                height as f32 / 2.0 + (self.time * 0.5).cos() * height as f32 * 0.15,
            );
            apply_light(
                &mut canvas,
                light,
                height as f32 * 0.6,
                (self.time * 9.0).sin(),
            );
            // The title in big pixel letters, above the menu.
            let (text_width, text_height) = Font::builtin().measure("LANTERN");
            let scale = (width * 3 / 5 / text_width)
                .min(height / 5 / text_height)
                .max(1);
            let x = (width - (text_width * scale).min(width)) / 2;
            let y = height / 12;
            shadow_text(
                &mut canvas,
                x as i32,
                y as i32,
                "LANTERN",
                Rgba::rgb(255, 210, 110),
                scale,
            );
            present(frame, area, &canvas);
        }
        status_line(frame, " Lantern");
        let menu_area = frame.area().centered(30, 9);
        let buffer = frame.buffer_mut();
        Clear.render(menu_area, buffer);
        let best = if shared.best > 0 {
            format!(" best {} ", shared.best)
        } else {
            String::new()
        };
        List::new(TITLE_MENU)
            .highlight_symbol("▶ ")
            .highlight_style(
                Style::new()
                    .fg(Color::rgb(20, 16, 24))
                    .bg(Color::rgb(255, 210, 110)),
            )
            .style(Style::new().bg(Color::rgb(22, 18, 30)))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(Color::rgb(255, 210, 110)))
                    .title(Line::from(" LANTERN ").centered())
                    .title_bottom(Line::from(best).centered())
                    .padding(vnce::vnce_ui::Padding::new(2, 2, 1, 1)),
            )
            .render(menu_area, buffer, &mut self.menu);
        help_line(
            frame,
            " ↑↓: choose · Enter: select · G: display method · Q: quit",
        );
    }
}

struct Settings {
    menu: ListState,
}

impl Settings {
    fn new() -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
        }
    }
}

/// Settings rows: music, effects, frame rate, display method, controls, back.
const SETTINGS_ROWS: usize = 6;

/// Frame rate limits offered in the settings, in order.
const FRAME_RATES: [FpsLimit; 5] = [
    FpsLimit::Fixed(30),
    FpsLimit::Fixed(60),
    FpsLimit::Fixed(90),
    FpsLimit::Fixed(120),
    FpsLimit::Unlimited,
];

impl Scene<Shared> for Settings {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        let controls = &shared.controls;
        if controls.was_pressed(Action::Up, input) {
            self.menu.select_previous(SETTINGS_ROWS, true);
        }
        if controls.was_pressed(Action::Down, input) {
            self.menu.select_next(SETTINGS_ROWS, true);
        }
        let step = if controls.was_pressed(Action::Left, input) {
            -10
        } else if controls.was_pressed(Action::Right, input) {
            10
        } else {
            0
        };
        if step != 0 {
            let volume = match self.menu.selected() {
                Some(0) => Some(&mut shared.music_volume),
                Some(1) => Some(&mut shared.effects_volume),
                _ => None,
            };
            if let Some(volume) = volume {
                *volume = (*volume as i32 + step).clamp(0, 100) as u32;
                shared
                    .sounds
                    .volumes(shared.music_volume, shared.effects_volume);
                shared.sounds.play(Sfx::Blip);
            }
        }
        let confirm = controls.was_pressed(Action::Confirm, input);
        if confirm && self.menu.selected() == Some(4) {
            shared.sounds.play(Sfx::Blip);
            return Transition::push(Controls::new());
        }
        let back = controls.was_pressed(Action::Back, input)
            || (confirm && self.menu.selected() == Some(5));
        if step != 0 {
            let direction = step.signum();
            match self.menu.selected() {
                Some(2) => {
                    let current = FRAME_RATES
                        .iter()
                        .position(|&rate| rate == ctx.fps_limit())
                        .unwrap_or(1) as i32;
                    let next = (current + direction).rem_euclid(FRAME_RATES.len() as i32);
                    // Every offered rate is valid.
                    let _ = ctx.set_fps_limit(FRAME_RATES[next as usize]);
                    shared.sounds.play(Sfx::Blip);
                }
                Some(3) if !shared.modes.is_empty() => {
                    let modes = &shared.modes;
                    let current = modes
                        .iter()
                        .position(|&mode| mode == ctx.graphics_mode())
                        .unwrap_or(0) as i32;
                    let next = (current + direction).rem_euclid(modes.len() as i32);
                    ctx.set_graphics_mode(modes[next as usize]);
                    shared.sounds.play(Sfx::Blip);
                }
                _ => {}
            }
        }
        if back {
            shared.store(ctx);
            return Transition::Pop;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let area = frame.area().centered(44, 12);
        let bar = |value: u32| {
            let filled = (value / 10) as usize;
            format!(
                "{}{} {value:>3}%",
                "█".repeat(filled),
                "·".repeat(10 - filled)
            )
        };
        let items = [
            format!("Music    {}", bar(shared.music_volume)),
            format!("Effects  {}", bar(shared.effects_volume)),
            match ctx.fps_limit() {
                FpsLimit::Fixed(rate) => format!("Frames   ◀ {rate} fps ▶"),
                FpsLimit::Unlimited => "Frames   ◀ unlimited ▶".to_owned(),
            },
            format!("Display  ◀ {} ▶", ctx.graphics_mode()),
            "Controls ▸".to_owned(),
            "Back".to_owned(),
        ];
        let note = if shared.sounds.available() {
            " ←→: change · Esc: back "
        } else {
            " ←→: change · sound: --features audio "
        };
        let buffer = frame.buffer_mut();
        Clear.render(area, buffer);
        List::new(items)
            .highlight_symbol("▶ ")
            .highlight_style(
                Style::new()
                    .fg(Color::rgb(20, 16, 24))
                    .bg(Color::rgb(140, 200, 255)),
            )
            .style(Style::new().bg(Color::rgb(22, 18, 30)))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(Color::rgb(140, 200, 255)))
                    .title(Line::from(" Settings ").centered())
                    .title_bottom(Line::from(note).centered())
                    .padding(vnce::vnce_ui::Padding::new(2, 2, 1, 1)),
            )
            .render(area, buffer, &mut self.menu);
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

/// The controls screen: every action with its keys. Enter waits for a new key.
struct Controls {
    menu: ListState,
    /// Waiting for the key of the selected action.
    waiting: bool,
    /// A key was just bound: skip one update so that key does not act at once.
    settle: bool,
}

impl Controls {
    /// Rows: one per action, then "reset all" and "back".
    const ROWS: usize = ACTIONS.len() + 2;

    fn new() -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
            waiting: false,
            settle: false,
        }
    }
}

/// The keys of `action` as text (`Up / w`).
fn binding_names(controls: &ActionMap<Action>, action: Action) -> String {
    let names: Vec<String> = controls.bindings(action).map(|b| b.to_string()).collect();
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join(" / ")
    }
}

/// Puts the default keys of `action` back.
fn reset_binding(controls: &mut ActionMap<Action>, action: Action) {
    controls.unbind(action);
    for binding in default_controls().bindings(action) {
        controls.add(action, binding);
    }
}

/// Whether a key of `action` is also used by another action. Esc is left out: pause and
/// back share it on purpose.
fn has_clash(controls: &ActionMap<Action>, action: Action) -> bool {
    controls.bindings(action).any(|binding| {
        binding != Binding::Key(KeyCode::Esc)
            && ACTIONS.iter().any(|&(other, _)| {
                other != action && controls.bindings(other).any(|b| b == binding)
            })
    })
}

impl Scene<Shared> for Controls {
    fn event(
        &mut self,
        shared: &mut Shared,
        _ctx: &mut Context,
        event: &Event,
    ) -> Transition<Shared> {
        let Event::Key(key) = event else {
            return Transition::None;
        };
        if !self.waiting || key.kind != KeyEventKind::Press {
            return Transition::None;
        }
        self.waiting = false;
        self.settle = true;
        // Esc cancels, so it cannot be chosen here.
        if key.code == KeyCode::Esc {
            return Transition::None;
        }
        let code = match key.code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        };
        if let Some(&(action, _)) = self.menu.selected().and_then(|row| ACTIONS.get(row)) {
            shared.controls.rebind(action, code);
            shared.sounds.play(Sfx::Blip);
        }
        Transition::None
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if self.waiting || std::mem::take(&mut self.settle) {
            return Transition::None;
        }
        let input = ctx.input();
        let controls = &shared.controls;
        if controls.was_pressed(Action::Up, input) {
            self.menu.select_previous(Self::ROWS, true);
        }
        if controls.was_pressed(Action::Down, input) {
            self.menu.select_next(Self::ROWS, true);
        }
        let row = self.menu.selected().unwrap_or(0);
        let confirm = controls.was_pressed(Action::Confirm, input);
        let back = controls.was_pressed(Action::Back, input);
        if input.was_pressed(KeyCode::Backspace) {
            if let Some(&(action, _)) = ACTIONS.get(row) {
                reset_binding(&mut shared.controls, action);
                shared.sounds.play(Sfx::Blip);
            }
            return Transition::None;
        }
        if back || (confirm && row == Self::ROWS - 1) {
            shared.store(ctx);
            return Transition::Pop;
        }
        if confirm {
            shared.sounds.play(Sfx::Blip);
            if row < ACTIONS.len() {
                self.waiting = true;
            } else {
                shared.controls = default_controls();
            }
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        let area = frame.area().centered(48, Self::ROWS as u16 + 4);
        let warning = Style::new().fg(Color::rgb(255, 120, 100));
        let mut items: Vec<Line<'_>> = ACTIONS
            .iter()
            .enumerate()
            .map(|(row, &(action, name))| {
                let keys = if self.waiting && self.menu.selected() == Some(row) {
                    "press a key… (Esc: cancel)".to_owned()
                } else {
                    binding_names(&shared.controls, action)
                };
                let line = Line::from(format!("{name:<9}{keys}"));
                if has_clash(&shared.controls, action) {
                    line.style(warning)
                } else {
                    line
                }
            })
            .collect();
        items.push(Line::from("Reset all"));
        items.push(Line::from("Back"));
        let buffer = frame.buffer_mut();
        Clear.render(area, buffer);
        List::new(items)
            .highlight_symbol("▶ ")
            .highlight_style(
                Style::new()
                    .fg(Color::rgb(20, 16, 24))
                    .bg(Color::rgb(140, 200, 255)),
            )
            .style(Style::new().bg(Color::rgb(22, 18, 30)))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(Color::rgb(140, 200, 255)))
                    .title(Line::from(" Controls ").centered())
                    .title_bottom(Line::from(" Enter: change · Backspace: default ").centered())
                    .padding(vnce::vnce_ui::Padding::new(2, 2, 1, 1)),
            )
            .render(area, buffer, &mut self.menu);
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

struct Bat {
    body: Aabb,
    previous: (f32, f32),
    heading: (f32, f32),
    turn: Timer,
    animator: Animator,
}

struct Play {
    level: u32,
    score: u32,
    lives: u32,
    rng: Rng,
    map: TileMap,
    tiles: Tileset,
    player: Aabb,
    previous: (f32, f32),
    facing_left: bool,
    hurt: Timer,
    gems: Vec<Aabb>,
    gems_total: usize,
    bats: Vec<Bat>,
    exit: Aabb,
    camera: Camera,
    player_art: Canvas,
    bat_art: SpriteSheet,
    gem_art: SpriteSheet,
    gem_shine: Animator,
    exit_art: Canvas,
    time: f32,
    grid: SpatialGrid<usize>,
}

impl Play {
    fn new(level: u32, score: u32, lives: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed ^ (u64::from(level) * 7919));
        // Deeper caves are bigger.
        let (width, height) = (44 + level * 6, 30 + level * 3);
        let cave = Cave::generate(&mut rng, width, height);
        let map = cave.tile_map();
        let tile = TILE as f32;
        let at = |(x, y): (u32, u32), size: f32| {
            Aabb::new(
                x as f32 * tile + (tile - size) / 2.0,
                y as f32 * tile + (tile - size) / 2.0,
                size,
                size,
            )
        };
        let player = at(cave.start, 6.0);
        let gem_count = 6 + level as usize * 2;
        let gems: Vec<Aabb> = cave
            .far_cells(&mut rng, 6)
            .into_iter()
            .take(gem_count)
            .map(|cell| at(cell, 5.0))
            .collect();
        let bat_art = bat_sheet();
        let flap = Animation::new([0, 1], Duration::from_millis(120), Playback::Loop);
        let bats = cave
            .far_cells(&mut rng, 14)
            .into_iter()
            .take(2 + level as usize * 2)
            .map(|cell| {
                let body = at(cell, 6.0);
                Bat {
                    previous: (body.x, body.y),
                    body,
                    heading: (0.0, 0.0),
                    turn: Timer::once(Duration::ZERO),
                    animator: Animator::new(flap.clone()),
                }
            })
            .collect();
        let exit = at(cave.farthest(), 8.0);
        let mut camera = Camera::new(200.0, 112.0)
            .with_bounds(map.bounds())
            .with_smoothing(0.2)
            .with_dead_zone(20.0, 14.0);
        camera.center_on(player.center().0, player.center().1);
        Self {
            level,
            score,
            lives,
            rng,
            tiles: tileset(),
            map,
            previous: (player.x, player.y),
            player,
            facing_left: false,
            hurt: Timer::once(Duration::ZERO),
            gems_total: gems.len(),
            gems,
            bats,
            exit,
            camera,
            player_art: player_sprite(),
            bat_art,
            gem_art: gem_sheet(),
            gem_shine: Animator::new(Animation::with_durations(
                [
                    (0, Duration::from_millis(900)),
                    (1, Duration::from_millis(150)),
                ],
                Playback::Loop,
            )),
            exit_art: exit_sprite(),
            time: 0.0,
            grid: SpatialGrid::new(16.0),
        }
    }

    /// Light radius in world pixels: smaller deeper down.
    fn light_radius(&self) -> f32 {
        (64.0 - self.level as f32 * 4.0).max(34.0)
    }

    fn solid(&self) -> impl Fn(i32, i32) -> bool + '_ {
        |column, row| self.map.is_solid(&self.tiles, column, row)
    }

    fn move_bats(&mut self, dt: Duration) {
        let seconds = dt.as_secs_f32();
        let target = self.player.center();
        let chase_range = self.light_radius() * 1.2;
        let speed = 26.0 + self.level as f32 * 4.0;
        for index in 0..self.bats.len() {
            let (bx, by) = self.bats[index].body.center();
            let (dx, dy) = (target.0 - bx, target.1 - by);
            let distance = (dx * dx + dy * dy).sqrt();
            let bat = &mut self.bats[index];
            bat.previous = (bat.body.x, bat.body.y);
            bat.animator.update(dt);
            bat.turn.update(dt);
            let (hx, hy, pace) = if distance < chase_range && distance > 0.5 {
                (dx / distance, dy / distance, speed * 1.5)
            } else {
                if bat.turn.is_finished() {
                    let angle = self.rng.unit() * std::f32::consts::TAU;
                    bat.heading = (angle.cos(), angle.sin());
                    bat.turn =
                        Timer::once(Duration::from_millis(600 + u64::from(self.rng.below(900))));
                }
                (bat.heading.0, bat.heading.1, speed)
            };
            let body = bat.body;
            let movement = move_and_collide(
                body,
                hx * pace * seconds,
                hy * pace * seconds,
                TILE as f32,
                |c, r| self.map.is_solid(&self.tiles, c, r),
            );
            let bat = &mut self.bats[index];
            bat.body = movement.aabb;
            if movement.hit.any() {
                // Bounce off walls while wandering.
                bat.heading = (-bat.heading.0, -bat.heading.1);
            }
        }
    }
}

impl Scene<Shared> for Play {
    fn start(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.music(true);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Pause, input) {
            shared.sounds.play(Sfx::Blip);
            return Transition::push(Pause);
        }
        if shared.controls.was_pressed(Action::Method, input) {
            cycle_method(shared, ctx);
        }
        let dt = ctx.dt();
        let seconds = ctx.dt_secs();
        self.time += seconds;

        // The player.
        let input = ctx.input();
        let controls = &shared.controls;
        let (mut dx, mut dy) = (
            controls.axis(Action::Left, Action::Right, input),
            controls.axis(Action::Up, Action::Down, input),
        );
        if dx != 0.0 && dy != 0.0 {
            dx *= std::f32::consts::FRAC_1_SQRT_2;
            dy *= std::f32::consts::FRAC_1_SQRT_2;
        }
        if dx != 0.0 {
            self.facing_left = dx < 0.0;
        }
        self.previous = (self.player.x, self.player.y);
        let movement = move_and_collide(
            self.player,
            dx * PLAYER_SPEED * seconds,
            dy * PLAYER_SPEED * seconds,
            TILE as f32,
            self.solid(),
        );
        self.player = movement.aabb;
        self.hurt.update(dt);
        self.gem_shine.update(dt);
        self.move_bats(dt);

        // Gems.
        let before = self.gems.len();
        let player = self.player;
        self.gems.retain(|gem| !gem.intersects(&player));
        let collected = before - self.gems.len();
        if collected > 0 {
            self.score += 10 * collected as u32;
            shared.sounds.play(Sfx::Gem);
            if self.gems.is_empty() {
                shared.sounds.play(Sfx::Open);
                self.camera.shake(1.5, Duration::from_millis(250));
            }
        }

        // Bats, found through the spatial grid.
        self.grid.clear();
        for (index, bat) in self.bats.iter().enumerate() {
            self.grid.insert(index, bat.body);
        }
        if self.hurt.is_finished()
            && !self
                .grid
                .query(&self.player.translated(1.0, 1.0))
                .is_empty()
        {
            self.lives = self.lives.saturating_sub(1);
            self.hurt = Timer::once(Duration::from_millis(1500));
            self.camera.shake(3.0, Duration::from_millis(300));
            shared.sounds.play(Sfx::Hit);
            if self.lives == 0 {
                shared.best = shared.best.max(self.score);
                shared.store(ctx);
                shared.sounds.music(false);
                return Transition::switch(GameOver {
                    score: self.score,
                    level: self.level,
                });
            }
        }

        // The exit, once every gem is collected.
        if self.gems.is_empty() && self.player.intersects(&self.exit) {
            shared.sounds.play(Sfx::Open);
            let bonus = 50 * self.level;
            return Transition::switch(Play::new(
                self.level + 1,
                self.score + bonus,
                self.lives,
                self.rng.next_u64(),
            ));
        }
        self.camera.update(dt, Some(self.player.center()));
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let (area, width, height) = view(ctx, frame);
        if width > 0 {
            if self.camera.viewport() != (width as f32, height as f32) {
                self.camera.set_viewport(width as f32, height as f32);
            }
            let alpha = ctx.alpha();
            let offset = self.camera.offset(alpha);
            let mut canvas = Canvas::filled(width, height, Rgba::rgb(28, 24, 34));
            self.map.draw(&self.tiles, &mut canvas, offset);
            let screen =
                |x: f32, y: f32| (x.round() as i32 - offset.0, y.round() as i32 - offset.1);
            if self.gems.is_empty() {
                let (x, y) = screen(self.exit.x, self.exit.y);
                canvas.blit(&self.exit_art, x, y, BlitOptions::default());
            }
            for gem in &self.gems {
                let (x, y) = screen(gem.x, gem.y);
                self.gem_art.draw(
                    &mut canvas,
                    self.gem_shine.frame(),
                    x,
                    y,
                    BlitOptions::default(),
                );
            }
            for bat in &self.bats {
                let x = ease::lerp(bat.previous.0, bat.body.x, alpha);
                let y = ease::lerp(bat.previous.1, bat.body.y, alpha);
                let (sx, sy) = screen(x - 1.0, y);
                self.bat_art.draw(
                    &mut canvas,
                    bat.animator.frame(),
                    sx,
                    sy,
                    BlitOptions::default(),
                );
            }
            let px = ease::lerp(self.previous.0, self.player.x, alpha);
            let py = ease::lerp(self.previous.1, self.player.y, alpha);
            // Blink while hurt.
            let visible = self.hurt.is_finished() || ((self.time * 12.0) as u32).is_multiple_of(2);
            if visible {
                let (sx, sy) = screen(px - 1.0, py - 1.0);
                let options = BlitOptions {
                    flip_x: self.facing_left,
                    ..BlitOptions::default()
                };
                canvas.blit(&self.player_art, sx, sy, options);
            }
            let light = screen(px + 3.0, py + 3.0);
            apply_light(
                &mut canvas,
                (light.0 as f32, light.1 as f32),
                self.light_radius(),
                (self.time * 11.0).sin() * (self.time * 7.0).cos(),
            );
            let found = self.gems_total - self.gems.len();
            let goal = if self.gems.is_empty() {
                "EXIT!".to_owned()
            } else {
                format!("GEMS {found}/{}", self.gems_total)
            };
            draw_hud(&mut canvas, self.level, self.score, self.lives, &goal);
            present(frame, area, &canvas);
        }
        let found = self.gems_total - self.gems.len();
        let hearts = "♥".repeat(self.lives as usize);
        let goal = if self.gems.is_empty() {
            "find the exit!".to_owned()
        } else {
            format!("gems {found}/{}", self.gems_total)
        };
        status_line(
            frame,
            &format!(
                " Lantern · depth {} · {goal} · score {} · {hearts} · {} · {:.0} fps",
                self.level,
                self.score,
                frame.graphics_mode(),
                ctx.fps()
            ),
        );
        help_line(frame, " ←↑→↓/WASD: move · P: pause · G: display method");
    }
}

struct Pause;

impl Scene<Shared> for Pause {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Quit, input) {
            shared.sounds.music(false);
            return Transition::switch(Title::new());
        }
        if shared.controls.was_pressed(Action::Pause, input)
            || shared.controls.was_pressed(Action::Confirm, input)
        {
            return Transition::Pop;
        }
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        popup(
            frame,
            "Paused",
            vec![
                Line::from("P / Enter: continue"),
                Line::from("Q: back to the title"),
            ],
            Color::rgb(140, 200, 255),
        );
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

struct GameOver {
    score: u32,
    level: u32,
}

impl Scene<Shared> for GameOver {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if shared.controls.was_pressed(Action::Confirm, ctx.input()) {
            return Transition::switch(Title::new());
        }
        if shared.controls.was_pressed(Action::Quit, ctx.input()) {
            shared.store(ctx);
            return Transition::Quit;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        status_line(frame, " Lantern");
        popup(
            frame,
            "The lantern went out",
            vec![
                Line::from(format!("Depth {} · score {}", self.level, self.score)),
                Line::from(format!("Best score {}", shared.best)),
                Line::from(""),
                Line::from("Enter: title · Q: quit"),
            ],
            Color::rgb(255, 120, 100),
        );
    }
}
