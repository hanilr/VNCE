//! A small platformer built with `vnce_game`: a level from text, an animated player,
//! collision with tiles, a following camera, coins, a goal flag, and title, play, pause and
//! finish scenes.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example platformer
//! cargo run --release --example platformer --features audio   # with sound and music
//! ```
//!
//! `←` `→` / `A` `D`: run · `Space` / `↑` / `W`: jump · `P` / `Esc`: pause ·
//! `G`: display method · `Q`: quit.

// Game code converts freely between pixels, tiles and floats; every value stays far inside
// the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel and tile coordinates are small"
)]

use std::time::Duration;

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{
    Aabb, ActionMap, Animation, Animator, Binding, Camera, Playback, Scene, SceneStack,
    SpriteSheet, TileMap, Tileset, Timer, Transition, ease, move_and_collide,
};
use vnce::vnce_raster::{BlitOptions, Canvas, Rgba};
use vnce::vnce_runtime::{Context, RunConfig};
use vnce::vnce_term::{
    Event, Frame, GamepadAxis, GamepadButton, GraphicsMode, KeyCode, KeyEventKind,
};
use vnce::vnce_ui::{Alignment, Block, BorderType, Clear, Line, Paragraph, Widget};

/// Size of a tile in world pixels.
const TILE: u32 = 8;

/// The level: `#` ground, `=` bricks, `@` player, `o` coin, `F` goal.
const LEVEL: &str = "\
................................................................
................................................................
................................................................
...........................o.o.o................................
..........................=======.........o.o...................
.......................................=======..........o......F
...............o.o..............................................#
..............=====.......................................===...#
.....................................=....................=.....#
...@..............................o..=....o..o..........=.......#
######....=====.......######......=..=...#####.......####.......#
######...............#######.....==..=..######......#####......##
######.......o.o.....#######################################...##
###########=======########################################......#
################################################################
################################################################";

const GRAVITY: f32 = 640.0;
const JUMP_SPEED: f32 = 215.0;
const RUN_SPEED: f32 = 72.0;
const MAX_FALL: f32 = 320.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Left,
    Right,
    Jump,
    Pause,
    Confirm,
    Quit,
    Method,
}

/// Data shared by every scene.
struct Shared {
    controls: ActionMap<Action>,
    /// Display methods to cycle with `G`.
    modes: Vec<GraphicsMode>,
    best: Option<Duration>,
    sounds: Sounds,
}

/// Sound effects of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sfx {
    Jump,
    Coin,
    Goal,
    Blip,
}

/// Sound effects and music, generated in code (with `--features audio`).
#[cfg(feature = "audio")]
struct Sounds {
    /// `None` without an audio device: the game stays silent.
    audio: Option<vnce::vnce_audio::Audio>,
    jump: vnce::vnce_audio::Sound,
    /// A few variations, so collecting many coins does not sound monotonous.
    coins: Vec<vnce::vnce_audio::Sound>,
    goal: vnce::vnce_audio::Sound,
    blip: vnce::vnce_audio::Sound,
    music: vnce::vnce_audio::Sound,
    music_voice: std::cell::RefCell<Option<vnce::vnce_audio::Voice>>,
    next_coin: std::cell::Cell<usize>,
}

#[cfg(feature = "audio")]
impl Sounds {
    const MUSIC_VOLUME: f32 = 0.35;

    fn new() -> Self {
        use vnce::vnce_audio::{Audio, Melody, Preset, Sound, Synth, Wave};

        let audio = Audio::new().ok();
        let rate = audio.as_ref().map_or(44_100, Audio::sample_rate);
        let effect = |preset, seed| Synth::preset(preset, seed).render(rate);
        let lead = Synth {
            wave: Wave::Triangle,
            attack: 0.01,
            decay: 0.05,
            sustain: 0.7,
            release: 0.06,
            volume: 0.5,
            ..Synth::default()
        };
        let bass = Synth {
            wave: Wave::Square,
            duty: 0.25,
            low_pass: 1200.0,
            volume: 0.3,
            ..lead
        };
        let tune = |voice, notes| {
            Melody::new(132.0, voice)
                .render(notes, rate)
                .unwrap_or_else(|_| Sound::mono(rate, Vec::new()))
        };
        // An original 16-beat loop: a melody over a bass line.
        let melody = tune(lead, "E4 G4 A4 G4 E4 D4 C4:2 E4 G4 A4 C5 B4 G4 A4:2");
        let bass_line = tune(bass, "A2:2 A2:2 F2:2 F2:2 C3:2 C3:2 G2:2 G2:2");
        let mixed = melody
            .samples()
            .iter()
            .zip(bass_line.samples())
            .map(|(m, b)| m * 0.7 + b * 0.5)
            .collect();
        let jingle = Synth { duty: 0.3, ..lead };
        Self {
            audio,
            jump: effect(Preset::Jump, 0),
            coins: (0..4).map(|seed| effect(Preset::Coin, seed)).collect(),
            goal: Melody::new(200.0, jingle)
                .render("C5 E5 G5 C6:3", rate)
                .unwrap_or_else(|_| Sound::mono(rate, Vec::new())),
            blip: effect(Preset::Blip, 0),
            music: Sound::mono(rate, mixed),
            music_voice: std::cell::RefCell::new(None),
            next_coin: std::cell::Cell::new(0),
        }
    }

    fn play(&self, sfx: Sfx) {
        let Some(audio) = &self.audio else {
            return;
        };
        let sound = match sfx {
            Sfx::Jump => &self.jump,
            Sfx::Coin => {
                let index = self.next_coin.get();
                self.next_coin.set((index + 1) % self.coins.len());
                &self.coins[index]
            }
            Sfx::Goal => &self.goal,
            Sfx::Blip => &self.blip,
        };
        audio.play(sound);
    }

    /// Starts (fading in) or stops (fading out) the music.
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
                    volume: Self::MUSIC_VOLUME,
                    fade_in: Duration::from_millis(600),
                    ..PlayOptions::music()
                },
            ));
        } else if !on {
            if let Some(voice) = voice.take() {
                voice.stop(Duration::from_millis(400));
            }
        }
    }

    /// Makes the music quieter (while paused) or normal again.
    fn duck(&self, ducked: bool) {
        if let Some(voice) = self.music_voice.borrow().as_ref() {
            let volume = if ducked { 0.1 } else { Self::MUSIC_VOLUME };
            voice.set_volume(volume, Duration::from_millis(200));
        }
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

    fn duck(&self, _ducked: bool) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let controls = ActionMap::new()
        .bind(Action::Left, KeyCode::Left)
        .bind(Action::Left, 'a')
        .bind(Action::Right, KeyCode::Right)
        .bind(Action::Right, 'd')
        .bind(Action::Jump, ' ')
        .bind(Action::Jump, KeyCode::Up)
        .bind(Action::Jump, 'w')
        .bind(Action::Pause, 'p')
        .bind(Action::Pause, KeyCode::Esc)
        .bind(Action::Confirm, KeyCode::Enter)
        .bind(Action::Confirm, ' ')
        .bind(Action::Quit, 'q')
        .bind(Action::Method, 'g')
        // A gamepad: d-pad or left stick to run, South to jump.
        .bind(Action::Left, GamepadButton::DPadLeft)
        .bind(Action::Left, Binding::stick(GamepadAxis::LeftX, false))
        .bind(Action::Right, GamepadButton::DPadRight)
        .bind(Action::Right, Binding::stick(GamepadAxis::LeftX, true))
        .bind(Action::Jump, GamepadButton::South)
        .bind(Action::Pause, GamepadButton::Start)
        .bind(Action::Confirm, GamepadButton::South);
    let shared = Shared {
        controls,
        modes: Vec::new(),
        best: None,
        sounds: Sounds::new(),
    };
    let mut game = SceneStack::new(Title { time: 0.0 }, shared);
    vnce::run(&mut game, RunConfig::default())?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Art (original, drawn in code)
// ---------------------------------------------------------------------------------------

fn player_sheet() -> SpriteSheet {
    let palette = [
        ('k', Rgba::rgb(20, 24, 40)),
        ('b', Rgba::rgb(80, 200, 230)),
        ('w', Rgba::rgb(250, 250, 250)),
        ('y', Rgba::rgb(255, 210, 60)),
    ];
    let body = |eyes: &'static str, legs: [&'static str; 2]| -> Vec<&'static str> {
        vec![
            "...yy...", "..kkkk..", ".kbbbbk.", eyes, ".kbbbbk.", ".kbbbbk.", legs[0], legs[1],
        ]
    };
    let open = ".kbwbwk.";
    let closed = ".kbkbkk.";
    let frames = [
        body(open, ["..kkkk..", "..k..k.."]),   // 0 idle
        body(closed, ["..kkkk..", "..k..k.."]), // 1 idle, blinking
        body(open, ["..kkkk..", ".k....k."]),   // 2 run
        body(open, ["..kkkk..", "..k..k.."]),   // 3 run
        body(open, ["..kkkk..", "...kk..."]),   // 4 run
        body(open, ["..kkkk..", "..k..k.."]),   // 5 run
        body(open, [".kkkkkk.", "k......k"]),   // 6 jump
    ];
    let canvases: Vec<Canvas> = frames
        .iter()
        .map(|rows| Canvas::from_art(rows, &palette))
        .collect();
    SpriteSheet::from_frames(&canvases)
}

fn coin_sheet() -> SpriteSheet {
    let palette = [
        ('y', Rgba::rgb(255, 205, 40)),
        ('o', Rgba::rgb(200, 130, 20)),
    ];
    let frames = [
        [".yyyy.", "yyooyy", "yoyyoy", "yoyyoy", "yyooyy", ".yyyy."],
        ["..yy..", ".yooy.", ".yyyy.", ".yyyy.", ".yooy.", "..yy.."],
        ["..yy..", "..yy..", "..oy..", "..yo..", "..yy..", "..yy.."],
    ];
    let canvases: Vec<Canvas> = frames
        .iter()
        .map(|rows| Canvas::from_art(rows, &palette))
        .collect();
    SpriteSheet::from_frames(&canvases)
}

fn flag() -> Canvas {
    Canvas::from_art(
        &[
            "kyyyy...", "kyyyyy..", "kyyyy...", "k.......", "k.......", "k.......", "k.......",
            "k.......",
        ],
        &[('k', Rgba::rgb(60, 60, 70)), ('y', Rgba::rgb(240, 80, 80))],
    )
}

/// Tiles: 0 dirt, 1 bricks, 2 grass on dirt.
fn tileset() -> Tileset {
    let mut tiles = Vec::new();
    for kind in 0..3 {
        let mut tile = Canvas::new(TILE, TILE);
        for y in 0..TILE {
            for x in 0..TILE {
                let speckle = (x * 7 + y * 3 + x * y) % 5 == 0;
                let color = match kind {
                    0 | 2 if kind == 2 && y < 2 => {
                        if (x + y) % 3 == 0 {
                            Rgba::rgb(70, 170, 60)
                        } else {
                            Rgba::rgb(90, 200, 70)
                        }
                    }
                    0 | 2 if speckle => Rgba::rgb(110, 70, 40),
                    0 | 2 => Rgba::rgb(140, 92, 54),
                    _ => {
                        let row = y / 4;
                        let mortar = y % 4 == 3 || (x + row * 4) % 8 == 0;
                        if mortar {
                            Rgba::rgb(110, 50, 40)
                        } else {
                            Rgba::rgb(200, 90, 60)
                        }
                    }
                };
                tile.pixel(x as i32, y as i32, color);
            }
        }
        tiles.push(tile);
    }
    Tileset::new(SpriteSheet::from_frames(&tiles)).with_solid([0, 1, 2])
}

fn level() -> TileMap {
    let mut map = TileMap::from_text(LEVEL, TILE, |c| match c {
        '#' => Some(0),
        '=' => Some(1),
        _ => None,
    });
    // Grass on dirt that has nothing above it.
    for row in 0..i64::from(map.height()) {
        for column in 0..i64::from(map.width()) {
            if map.get(0, column, row) == Some(0) && map.get(0, column, row - 1).is_none() {
                map.set(0, column, row, Some(2));
            }
        }
    }
    map
}

/// Sky and two layers of hills that scroll slower than the level (parallax).
fn draw_background(canvas: &mut Canvas, offset: (i32, i32), time: f32) {
    let (width, height) = (canvas.width(), canvas.height());
    for y in 0..height {
        let t = y as f32 / height.max(1) as f32;
        let sky = Rgba::rgb(
            ease::lerp(90.0, 170.0, t) as u8,
            ease::lerp(140.0, 210.0, t) as u8,
            ease::lerp(220.0, 240.0, t) as u8,
        );
        canvas.fill_rect(0, y as i32, width, 1, sky);
    }
    for (depth, color, base) in [
        (0.25, Rgba::rgb(120, 170, 200), 0.55),
        (0.5, Rgba::rgb(90, 140, 120), 0.7),
    ] {
        for x in 0..width {
            let world = x as f32 + offset.0 as f32 * depth + time * 2.0 * depth;
            let hill = (world * 0.05).sin() * 6.0 + (world * 0.013).sin() * 10.0;
            let top = (height as f32 * base + hill - offset.1 as f32 * depth * 0.3) as i32;
            canvas.fill_rect(x as i32, top, 1, height, color);
        }
    }
}

// ---------------------------------------------------------------------------------------
// Presenting a world view in the terminal
// ---------------------------------------------------------------------------------------

/// The area for the world (between the HUD and the help line), the size of the world view
/// in world pixels, and the factor it is enlarged by to fill the area.
fn view(ctx: &Context, frame: &Frame<'_>) -> (Rect, u32, u32, u32) {
    let size = ctx.size();
    let area = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
    let (width, height) = frame.image_pixel_size(area);
    if width == 0 || height == 0 {
        return (area, 0, 0, 1);
    }
    // At most about 15 tiles high, so the level stays readable with real pixels.
    let scale = (height / (TILE * 15)).max(1);
    let view_height = height / scale;
    let view_width = (width as f32 * frame.image_pixel_aspect() / scale as f32).round() as u32;
    (area, view_width.max(1), view_height.max(1), scale)
}

/// Shows `canvas` in `area`, enlarged without blurring the pixels: to the exact pixel size
/// of the area (nearest neighbour), so the terminal never has to scale it again.
fn present(frame: &mut Frame<'_>, area: Rect, canvas: &Canvas) {
    let (width, height) = frame.image_pixel_size(area);
    if (width, height) == (canvas.width(), canvas.height()) || width == 0 || height == 0 {
        frame.draw_image(area, canvas);
        return;
    }
    // The source column of every destination column, computed once.
    let columns: Vec<usize> = (0..width)
        .map(|x| (u64::from(x) * u64::from(canvas.width()) / u64::from(width)) as usize)
        .collect();
    let source = canvas.pixels();
    let stride = canvas.width() as usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let row = (u64::from(y) * u64::from(canvas.height()) / u64::from(height)) as usize;
        let line = &source[row * stride..(row + 1) * stride];
        pixels.extend(columns.iter().map(|&x| line[x]));
    }
    match Canvas::from_pixels(width, height, pixels) {
        Some(big) => frame.draw_image(area, &big),
        None => frame.draw_image(area, canvas),
    }
}

fn status_line(frame: &mut Frame<'_>, text: &str) {
    let width = frame.area().width;
    let style = Style::new()
        .fg(Color::rgb(20, 20, 30))
        .bg(Color::rgb(250, 210, 90));
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

/// Cycles the display method with `G`.
fn cycle_method(shared: &Shared, ctx: &Context) {
    if shared.modes.is_empty() {
        return;
    }
    let current = ctx.graphics_mode();
    let index = shared.modes.iter().position(|&m| m == current).unwrap_or(0);
    ctx.set_graphics_mode(shared.modes[(index + 1) % shared.modes.len()]);
}

fn key_pressed(event: &Event) -> bool {
    matches!(event, Event::Key(key) if key.kind == KeyEventKind::Press)
}

fn popup(frame: &mut Frame<'_>, title: &str, lines: Vec<Line<'_>>, accent: Color) {
    let area = frame.area().centered(40, lines.len() as u16 + 4);
    let buffer = frame.buffer_mut();
    Clear.render(area, buffer);
    Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(Style::new().bg(Color::rgb(25, 28, 50)))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(accent))
                .title(Line::from(format!(" {title} ")).centered())
                .padding(vnce::vnce_ui::Padding::vertical(1)),
        )
        .render(area, buffer);
}

// ---------------------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------------------

struct Title {
    time: f32,
}

impl Scene<Shared> for Title {
    fn start(&mut self, shared: &mut Shared, ctx: &mut Context) {
        shared.sounds.music(false);
        if shared.modes.is_empty() {
            shared.modes = GraphicsMode::available(ctx.terminal_info(), &ctx.capabilities());
            // Start with cells: animating a full-screen Sixel image is slow.
            if let Some(&cells) = shared.modes.iter().find(|m| !m.is_protocol()) {
                ctx.set_graphics_mode(cells);
            }
        }
    }

    fn event(
        &mut self,
        shared: &mut Shared,
        ctx: &mut Context,
        event: &Event,
    ) -> Transition<Shared> {
        if !key_pressed(event) {
            return Transition::None;
        }
        let input = ctx.input();
        let controls = &shared.controls;
        if controls.was_pressed(Action::Quit, input) || controls.was_pressed(Action::Pause, input) {
            return Transition::Quit;
        }
        Transition::None
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        self.time += ctx.dt_secs();
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Method, input) {
            cycle_method(shared, ctx);
        }
        if shared.controls.was_pressed(Action::Quit, input) {
            return Transition::Quit;
        }
        if shared.controls.was_pressed(Action::Confirm, input) {
            shared.sounds.play(Sfx::Blip);
            return Transition::switch(Play::new());
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let (area, width, height, _) = view(ctx, frame);
        if width > 0 {
            let mut canvas = Canvas::new(width, height);
            draw_background(&mut canvas, ((self.time * 20.0) as i32, 0), self.time);
            present(frame, area, &canvas);
        }
        status_line(frame, " vnce platformer");
        let best = shared.best.map_or_else(String::new, |best| {
            format!("Best time: {:.1} s", best.as_secs_f32())
        });
        popup(
            frame,
            "Platformer",
            vec![
                Line::from("Collect the coins, reach the flag."),
                Line::from(""),
                Line::from("Enter / Space: start · Q: quit"),
                Line::from(best),
            ],
            Color::rgb(250, 210, 90),
        );
        help_line(frame, " G: display method");
    }
}

struct Player {
    body: Aabb,
    previous: (f32, f32),
    velocity: (f32, f32),
    on_ground: bool,
    facing_left: bool,
    /// Jumping is still allowed shortly after running off a ledge.
    coyote: Timer,
    /// A jump pressed shortly before landing still happens.
    buffered_jump: Timer,
    animator: Animator,
}

struct Play {
    map: TileMap,
    tiles: Tileset,
    player_sheet: SpriteSheet,
    coin_sheet: SpriteSheet,
    flag: Canvas,
    idle: Animation,
    run: Animation,
    jump: Animation,
    coin_spin: Animator,
    player: Player,
    coins: Vec<Aabb>,
    total_coins: usize,
    goal: Aabb,
    camera: Camera,
    time: Duration,
}

impl Play {
    fn new() -> Self {
        let map = level();
        let start = TileMap::find_in_text(LEVEL, '@')
            .first()
            .copied()
            .unwrap_or((1, 1));
        let tile = TILE as f32;
        let body = Aabb::new(
            start.0 as f32 * tile + 1.0,
            start.1 as f32 * tile + 1.0,
            6.0,
            7.0,
        );
        let coins: Vec<Aabb> = TileMap::find_in_text(LEVEL, 'o')
            .into_iter()
            .map(|(c, r)| Aabb::new(c as f32 * tile + 1.0, r as f32 * tile + 1.0, 6.0, 6.0))
            .collect();
        let goal = TileMap::find_in_text(LEVEL, 'F')
            .first()
            .map_or(Aabb::default(), |&(c, r)| {
                Aabb::new(c as f32 * tile, r as f32 * tile, tile, tile)
            });
        let idle = Animation::with_durations(
            [
                (0, Duration::from_millis(1800)),
                (1, Duration::from_millis(150)),
            ],
            Playback::Loop,
        );
        let mut camera = Camera::new(160.0, 96.0)
            .with_bounds(map.bounds())
            .with_smoothing(0.25)
            .with_dead_zone(24.0, 16.0);
        camera.center_on(body.center().0, body.center().1);
        Self {
            map,
            tiles: tileset(),
            player_sheet: player_sheet(),
            coin_sheet: coin_sheet(),
            flag: flag(),
            run: Animation::new(2..=5, Duration::from_millis(90), Playback::Loop),
            jump: Animation::still(6),
            coin_spin: Animator::new(Animation::new(
                0..3,
                Duration::from_millis(140),
                Playback::PingPong,
            )),
            player: Player {
                body,
                previous: (body.x, body.y),
                velocity: (0.0, 0.0),
                on_ground: false,
                facing_left: false,
                coyote: Timer::once(Duration::from_millis(100)),
                buffered_jump: Timer::once(Duration::ZERO),
                animator: Animator::new(idle.clone()),
            },
            idle,
            total_coins: coins.len(),
            coins,
            goal,
            camera,
            time: Duration::ZERO,
        }
    }

    fn move_player(&mut self, shared: &Shared, ctx: &Context) {
        let dt = ctx.dt_secs();
        let input = ctx.input();
        let player = &mut self.player;
        player.previous = (player.body.x, player.body.y);

        let direction = shared.controls.axis(Action::Left, Action::Right, input);
        player.velocity.0 = direction * RUN_SPEED;
        if direction != 0.0 {
            player.facing_left = direction < 0.0;
        }
        if shared.controls.was_pressed(Action::Jump, input) {
            player.buffered_jump = Timer::once(Duration::from_millis(120));
        }
        player.buffered_jump.update(ctx.dt());
        player.coyote.update(ctx.dt());
        let can_jump = player.on_ground || !player.coyote.is_finished();
        if can_jump && !player.buffered_jump.is_finished() {
            player.velocity.1 = -JUMP_SPEED;
            shared.sounds.play(Sfx::Jump);
            player.on_ground = false;
            player.coyote = Timer::once(Duration::ZERO);
            player.buffered_jump = Timer::once(Duration::ZERO);
        }
        // Releasing jump early makes a lower jump.
        if !shared.controls.is_down(Action::Jump, input) && player.velocity.1 < -JUMP_SPEED / 2.0 {
            player.velocity.1 = -JUMP_SPEED / 2.0;
        }
        player.velocity.1 = (player.velocity.1 + GRAVITY * dt).min(MAX_FALL);

        let map = &self.map;
        let tiles = &self.tiles;
        let movement = move_and_collide(
            player.body,
            player.velocity.0 * dt,
            player.velocity.1 * dt,
            TILE as f32,
            |column, row| map.is_solid(tiles, column, row),
        );
        player.body = movement.aabb;
        if movement.hit.bottom || movement.hit.top {
            player.velocity.1 = 0.0;
        }
        let was_on_ground = player.on_ground;
        player.on_ground = movement.hit.bottom;
        if was_on_ground && !player.on_ground {
            player.coyote = Timer::once(Duration::from_millis(100));
        }

        let animation = if !player.on_ground {
            &self.jump
        } else if direction != 0.0 {
            &self.run
        } else {
            &self.idle
        };
        player.animator.play(animation);
        player.animator.update(ctx.dt());
    }
}

impl Scene<Shared> for Play {
    fn start(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.music(true);
    }

    fn resume(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.duck(false);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Pause, input) {
            shared.sounds.play(Sfx::Blip);
            shared.sounds.duck(true);
            return Transition::push(Pause);
        }
        if shared.controls.was_pressed(Action::Method, input) {
            cycle_method(shared, ctx);
        }
        self.time += ctx.dt();
        self.move_player(shared, ctx);
        self.coin_spin.update(ctx.dt());

        let body = self.player.body;
        let before = self.coins.len();
        self.coins.retain(|coin| !coin.intersects(&body));
        if self.coins.len() < before {
            self.camera.shake(1.5, Duration::from_millis(150));
            shared.sounds.play(Sfx::Coin);
        }
        self.camera.update(ctx.dt(), Some(body.center()));

        if body.intersects(&self.goal) {
            let collected = self.total_coins - self.coins.len();
            if shared.best.is_none_or(|best| self.time < best) {
                shared.best = Some(self.time);
            }
            shared.sounds.music(false);
            shared.sounds.play(Sfx::Goal);
            return Transition::switch(Finish {
                time: self.time,
                collected,
                total: self.total_coins,
            });
        }
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let (area, width, height, _) = view(ctx, frame);
        if width > 0 {
            if self.camera.viewport() != (width as f32, height as f32) {
                self.camera.set_viewport(width as f32, height as f32);
            }
            let alpha = ctx.alpha();
            let offset = self.camera.offset(alpha);
            let mut canvas = Canvas::new(width, height);
            draw_background(&mut canvas, offset, ctx.elapsed().as_secs_f32());
            self.map.draw(&self.tiles, &mut canvas, offset);

            let screen =
                |x: f32, y: f32| (x.round() as i32 - offset.0, y.round() as i32 - offset.1);
            let (fx, fy) = screen(self.goal.x, self.goal.y);
            canvas.blit(&self.flag, fx, fy, BlitOptions::default());
            for coin in &self.coins {
                let (x, y) = screen(coin.x, coin.y);
                self.coin_sheet.draw(
                    &mut canvas,
                    self.coin_spin.frame(),
                    x,
                    y,
                    BlitOptions::default(),
                );
            }
            let player = &self.player;
            let x = ease::lerp(player.previous.0, player.body.x, alpha);
            let y = ease::lerp(player.previous.1, player.body.y, alpha);
            let (px, py) = screen(x - 1.0, y - 1.0);
            let options = BlitOptions {
                flip_x: player.facing_left,
                ..BlitOptions::default()
            };
            self.player_sheet
                .draw(&mut canvas, player.animator.frame(), px, py, options);
            present(frame, area, &canvas);
        }
        let collected = self.total_coins - self.coins.len();
        status_line(
            frame,
            &format!(
                " Coins {collected}/{} · Time {:.1} s · {} · {:.0} fps",
                self.total_coins,
                self.time.as_secs_f32(),
                frame.graphics_mode(),
                ctx.fps()
            ),
        );
        help_line(
            frame,
            " ←→/AD: run · Space/↑/W: jump · P: pause · G: display method",
        );
    }
}

struct Pause;

impl Scene<Shared> for Pause {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Quit, input) {
            return Transition::switch(Title { time: 0.0 });
        }
        if shared.controls.was_pressed(Action::Pause, input)
            || shared.controls.was_pressed(Action::Confirm, input)
        {
            shared.sounds.play(Sfx::Blip);
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
            Color::rgb(120, 180, 255),
        );
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

struct Finish {
    time: Duration,
    collected: usize,
    total: usize,
}

impl Scene<Shared> for Finish {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if shared.controls.was_pressed(Action::Confirm, ctx.input()) {
            return Transition::switch(Title { time: 0.0 });
        }
        if shared.controls.was_pressed(Action::Quit, ctx.input()) {
            return Transition::Quit;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        status_line(frame, " vnce platformer");
        let best = shared.best.map_or(0.0, |best| best.as_secs_f32());
        popup(
            frame,
            "Goal!",
            vec![
                Line::from(format!(
                    "Time: {:.1} s (best {best:.1} s)",
                    self.time.as_secs_f32()
                )),
                Line::from(format!("Coins: {}/{}", self.collected, self.total)),
                Line::from(""),
                Line::from("Enter: title · Q: quit"),
            ],
            Color::rgb(120, 220, 120),
        );
    }
}
