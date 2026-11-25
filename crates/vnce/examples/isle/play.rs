//! The island: walking, the turning and zooming camera, depth sorting with the terrain
//! cache, and the x-ray outline of the player behind trees and cliffs; the crystals, the
//! beacon, crabs and wisps, the lantern, night falling, rain, light and the minimap.

use std::f32::consts::FRAC_PI_2;
use std::time::{Duration, Instant};

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{
    Aabb, Animation, Animator, Camera, Playback, Rng, Scene, SpatialGrid, SpriteSheet, Timer,
    Transition, ease, move_and_collide,
};
use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Font, PixelRect, Rgba};
use vnce::vnce_runtime::Context;
use vnce::vnce_term::Frame;
use vnce::vnce_ui::{Gauge, Line, Paragraph, Span, Widget};

use crate::art;
use crate::audio::{Mood, Sfx};
use crate::island::{CRYSTALS, Dir, Island, Prop, SIZE, index};
use crate::iso::{
    self, Layer, Sprites, Terrain, View, cache_origin, cache_size, draw_order, draw_tile,
    tile_bounds,
};
use crate::life::{self, Crab, Wisp};
use crate::minimap::{self, Minimap};
use crate::sky::{self, Clouds, Fireflies, Lights, NIGHT, Spark, Weather};
use crate::visuals::{OCEAN_HEIGHT, OCEAN_WIDTH};
use crate::{ACCENT, Action, Shared, ZOOM_ROWS, help_line, present, status_line, view};

/// Walking speed in tiles per second.
const SPEED: f32 = 3.4;
/// Size of the player's feet for collisions, in tiles.
const FEET: f32 = 0.34;
/// How long a quarter turn of the camera takes.
const TURN: Duration = Duration::from_millis(450);

/// The deep sea behind everything (under the ocean picture, shown where it has no pixels).
const SEA: Rgba = Rgba::rgb(28, 86, 132);
/// Lantern oil when full; it burns slowly, crystals refill it and wisps steal it.
const OIL: f32 = 100.0;
/// How close (in tiles) the player must be to take a crystal or light the beacon.
const REACH: f32 = 0.55;
const BEACON_REACH: f32 = 1.45;
/// How long the beacon burns before the game ends with a win.
const CELEBRATION: f32 = 3.5;

/// Which way a sprite looks on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Facing {
    /// Towards the camera (down the screen) or away from it.
    toward: bool,
    /// Mirrored.
    left: bool,
}

struct Player {
    /// Position in world tiles.
    position: (f32, f32),
    previous: (f32, f32),
    facing: Facing,
    walk: Animator,
    moving: bool,
}

/// A camera turn in progress.
struct Turn {
    from: f32,
    to: f32,
    timer: Timer,
}

/// Something drawn between the tiles, sorted by depth.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Actor {
    Player,
    Beacon,
    Crystal(usize),
    Crab(usize),
}

/// A light crystal.
struct Crystal {
    tile: (usize, usize),
    taken: bool,
}

/// How the game ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// The beacon was lit after this many tenths of a second.
    Won(u32),
    /// Night fell with this many crystals collected.
    Lost(usize),
}

/// The tile under the mouse and the walk to it.
struct Hover {
    tile: (usize, usize),
    from: (usize, usize),
    path: Vec<(usize, usize)>,
}

/// Where the player was drawn this frame, to find the parts hidden later.
struct Drawn {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    /// The pixels right after drawing the player, and which of them are the player.
    pixels: Vec<Rgba>,
    mask: Vec<bool>,
}

pub struct Play {
    island: Island,
    terrain: Terrain,
    sprites: Sprites,
    player_sheet: SpriteSheet,
    beacon: [Canvas; 2],
    player: Player,
    camera: Camera,
    /// The camera's direction in quarter turns (0–3).
    quarter: u8,
    turn: Option<Turn>,
    zoom: usize,
    /// Seconds since the game started (stops while paused).
    time: f32,
    beacon_lit: bool,
    world: Canvas,
    big: Canvas,
    /// Pixels of the player hidden by things in front, shown as an outline.
    hidden: Vec<(i32, i32)>,
    drawn: Option<Drawn>,
    /// Where the world was shown last frame: the cell area and the world view size, to
    /// turn mouse positions into world pixels.
    shown: Option<(Rect, u32, u32)>,
    /// The tile under the mouse, the tile the walk to it starts from, and the walk
    /// (empty when it cannot be reached).
    hover: Option<Hover>,
    /// Tiles still to walk through after a click, the next one first.
    route: Vec<(usize, usize)>,
    rng: Rng,
    crystals: Vec<Crystal>,
    crystal_sheet: SpriteSheet,
    crab_sheet: SpriteSheet,
    wisp_sprite: Canvas,
    crabs: Vec<Crab>,
    wisps: Vec<Wisp>,
    /// Creatures by place, to find the ones touching the player.
    grid: SpatialGrid<usize>,
    oil: f32,
    /// Seconds played; night falls at `NIGHT`.
    clock: f32,
    weather: Weather,
    clouds: Clouds,
    fireflies: Fireflies,
    sparks: Vec<Spark>,
    lights: Lights,
    light: Canvas,
    minimap: Minimap,
    /// The edges between land and sea, for the foam: land tile and the side facing the
    /// sea.
    shore: Vec<((usize, usize), Dir)>,
    /// A message on the screen and the seconds it stays.
    message: Option<(String, f32)>,
    /// Seconds the beacon has burned (the celebration before the end).
    lit_for: f32,
    pub outcome: Option<Outcome>,
    /// The walk animation frame of the last footstep sound.
    last_step: usize,
    /// Milliseconds of the last frame's drawing, for the performance panel.
    render_ms: f32,
}

impl Play {
    pub fn new(seed: u64) -> Self {
        let island = Island::generate(seed);
        let mut rng = Rng::new(seed ^ 0x5EED_0F15_1E00);
        let start = (island.start.0 as f32 + 0.5, island.start.1 as f32 + 0.5);
        let (width, height) = cache_size();
        let mut camera = Camera::new(320.0, 180.0)
            .with_bounds(Aabb::new(0.0, 0.0, width as f32, height as f32))
            .with_smoothing(0.16);
        let h = island.height_at(start.0, start.1);
        let (x, y) = View::quarter(0).project(start.0, start.1, h);
        camera.center_on(x, y);
        Self {
            terrain: Terrain::new(),
            sprites: Sprites::new(),
            player_sheet: art::player_sheet(),
            beacon: [
                art::prop(Prop::Beacon, false),
                art::prop(Prop::Beacon, true),
            ],
            player: Player {
                position: start,
                previous: start,
                facing: Facing {
                    toward: true,
                    left: false,
                },
                walk: Animator::new(Animation::new(
                    [1, 0, 2, 0],
                    Duration::from_millis(130),
                    Playback::Loop,
                )),
                moving: false,
            },
            camera,
            quarter: 0,
            turn: None,
            zoom: 1,
            time: 0.0,
            beacon_lit: false,
            world: Canvas::new(0, 0),
            big: Canvas::new(0, 0),
            hidden: Vec::new(),
            drawn: None,
            shown: None,
            hover: None,
            route: Vec::new(),
            crystals: island
                .crystals
                .iter()
                .map(|&tile| Crystal { tile, taken: false })
                .collect(),
            crystal_sheet: art::crystal_sheet(),
            crab_sheet: art::crab_sheet(),
            wisp_sprite: art::glow_sprite(4, Rgba::rgb(170, 255, 220)),
            crabs: life::crabs(&island, &mut rng, 6),
            wisps: life::wisps(&mut rng, 4),
            grid: SpatialGrid::new(1.0),
            oil: OIL,
            clock: 0.0,
            weather: Weather::new(),
            clouds: Clouds::new(&mut rng),
            fireflies: Fireflies::new(&island, &mut rng),
            sparks: Vec::new(),
            lights: Lights::new(),
            light: Canvas::new(0, 0),
            minimap: Minimap::new(),
            shore: shore_edges(&island),
            message: Some((
                "Collect the light crystals, then light the beacon".to_owned(),
                6.0,
            )),
            lit_for: 0.0,
            outcome: None,
            last_step: 0,
            render_ms: 0.0,
            rng,
            island,
        }
    }

    /// How far night has fallen (0 to 1).
    fn night(&self) -> f32 {
        (self.clock / NIGHT).clamp(0.0, 1.0)
    }

    fn say(&mut self, text: &str, seconds: f32) {
        self.message = Some((text.to_owned(), seconds));
    }

    /// The player's position between the last two updates.
    fn player_at(&self, alpha: f32) -> (f32, f32) {
        let lerp = |a: f32, b: f32| a + (b - a) * alpha;
        let player = &self.player;
        (
            lerp(player.previous.0, player.position.0),
            lerp(player.previous.1, player.position.1),
        )
    }

    fn carried(&self) -> usize {
        self.crystals.iter().filter(|c| c.taken).count()
    }

    /// Crystals within reach are taken; the beacon is lit when all are carried and the
    /// player is beside it (`using`: the use key was pressed).
    fn collect(&mut self, shared: &Shared, using: bool) {
        let (px, py) = self.player.position;
        let h = self.island.height_at(px, py);
        for index in 0..self.crystals.len() {
            let crystal = &mut self.crystals[index];
            let (cx, cy) = (crystal.tile.0 as f32 + 0.5, crystal.tile.1 as f32 + 0.5);
            if crystal.taken || (cx - px).hypot(cy - py) > REACH {
                continue;
            }
            crystal.taken = true;
            self.oil = (self.oil + 30.0).min(OIL);
            shared.sounds.play(Sfx::Crystal);
            let color = Rgba::rgb(140, 230, 255);
            sky::burst(
                &mut self.sparks,
                &mut self.rng,
                (cx, cy, h + 0.6),
                color,
                26,
            );
            let carried = self.carried();
            if carried == CRYSTALS {
                self.say("All crystals! Light the beacon on the hill", 6.0);
            } else {
                self.say(&format!("Crystal {carried} of {CRYSTALS}"), 2.5);
            }
        }
        let beacon = (
            self.island.beacon.0 as f32 + 0.5,
            self.island.beacon.1 as f32 + 0.5,
        );
        let beside = (beacon.0 - px).hypot(beacon.1 - py) <= BEACON_REACH;
        if self.beacon_lit || !beside {
            return;
        }
        if self.carried() == CRYSTALS {
            // Walking up to it is enough; the use key works too.
            self.beacon_lit = true;
            shared.sounds.play(Sfx::Beacon);
            let level = f32::from(self.island.level_at(self.island.beacon));
            let color = Rgba::rgb(255, 220, 120);
            sky::burst(
                &mut self.sparks,
                &mut self.rng,
                (beacon.0, beacon.1, level + 3.5),
                color,
                60,
            );
            self.camera.shake(2.0, Duration::from_millis(300));
            self.say("The beacon is lit!", CELEBRATION);
        } else if using {
            shared.sounds.play(Sfx::Wrong);
            self.say(&format!("The beacon needs all {CRYSTALS} crystals"), 3.0);
        }
    }

    /// Creatures that touch the player: crabs run off, wisps take oil and vanish.
    fn meet_creatures(&mut self, shared: &Shared) {
        self.grid.clear();
        for (i, crab) in self.crabs.iter().enumerate() {
            let body = Aabb::from_center(crab.position.0, crab.position.1, 0.4, 0.4);
            self.grid.insert(i, body);
        }
        for (i, wisp) in self.wisps.iter().enumerate() {
            if wisp.is_out() {
                let body = Aabb::from_center(wisp.position.0, wisp.position.1, 0.5, 0.5);
                self.grid.insert(1000 + i, body);
            }
        }
        let (px, py) = self.player.position;
        let h = self.island.height_at(px, py);
        for id in self.grid.query(&Aabb::from_center(px, py, FEET, FEET)) {
            if id >= 1000 {
                let wisp = &mut self.wisps[id - 1000];
                let at = (wisp.position.0, wisp.position.1, h + 1.0);
                wisp.away = 6.0 + self.rng.unit() * 8.0;
                self.oil = (self.oil - 18.0).max(0.0);
                shared.sounds.play(Sfx::Drain);
                let color = Rgba::rgb(170, 255, 220);
                sky::burst(&mut self.sparks, &mut self.rng, at, color, 18);
                self.say("A wisp took some lantern oil", 2.5);
            } else if self.crabs[id].fleeing <= 0.0 {
                self.crabs[id].fleeing = 1.6;
                shared.sounds.play(Sfx::Pinch);
            }
        }
    }

    /// A footstep sound when a foot touches the ground.
    fn footsteps(&mut self, shared: &Shared) {
        let frame = self.player.walk.frame();
        if !self.player.moving || frame == self.last_step {
            return;
        }
        self.last_step = frame;
        if frame == 1 || frame == 2 {
            let tile = self.player_tile();
            let i = index(tile.0, tile.1);
            let pan = if frame == 1 { -0.15 } else { 0.15 };
            shared
                .sounds
                .step(self.island.ground[i], self.island.stair[i].is_some(), pan);
        }
    }

    /// The camera angle now: eased between quarters while turning.
    fn angle(&self) -> f32 {
        match &self.turn {
            Some(turn) => {
                let t = ease::in_out_cubic(turn.timer.progress());
                turn.from + (turn.to - turn.from) * t
            }
            None => f32::from(self.quarter) * FRAC_PI_2,
        }
    }

    fn start_turn(&mut self, step: i32) {
        let from = self.angle();
        self.quarter = (i32::from(self.quarter) + step).rem_euclid(4) as u8;
        self.turn = Some(Turn {
            from,
            to: from + step as f32 * FRAC_PI_2,
            timer: Timer::once(TURN),
        });
    }

    /// The tile the player stands on.
    fn player_tile(&self) -> (usize, usize) {
        let (x, y) = self.player.position;
        ((x as usize).min(SIZE - 1), (y as usize).min(SIZE - 1))
    }

    /// Moves the player by the arrow keys: up is up the screen, whatever the camera angle.
    fn walk_by_keys(&mut self, shared: &Shared, ctx: &Context) {
        let input = ctx.input();
        let controls = &shared.controls;
        let (sx, sy) = (
            controls.axis(Action::Left, Action::Right, input),
            controls.axis(Action::Up, Action::Down, input),
        );
        self.player.previous = self.player.position;
        if sx == 0.0 && sy == 0.0 {
            self.player.moving = false;
            return;
        }
        // Keys take over from a clicked walk.
        self.route.clear();
        // Screen direction to view axes (a tile is twice as wide as high on the screen),
        // then turned back into the world.
        let (u, v) = (sx * 0.5 + sy, sy - sx * 0.5);
        let angle = self.angle();
        let (sin, cos) = angle.sin_cos();
        let (wx, wy) = (u * cos + v * sin, -u * sin + v * cos);
        let length = (wx * wx + wy * wy).sqrt().max(1e-6);
        let step = SPEED * ctx.dt_secs() / length;
        self.step_player((wx * step, wy * step), (sx, sy));
    }

    /// Moves the player by `(dx, dy)` world tiles, stopped by cliffs, water and props.
    /// `screen` is the direction on the screen, for the sprite.
    fn step_player(&mut self, (dx, dy): (f32, f32), screen: (f32, f32)) {
        let from = self.player_tile();
        let island = &self.island;
        let (x, y) = self.player.position;
        let feet = Aabb::from_center(x, y, FEET, FEET);
        let movement = move_and_collide(feet, dx, dy, 1.0, |column, row| {
            if column < 0 || row < 0 || column >= SIZE as i32 || row >= SIZE as i32 {
                return true;
            }
            island.blocks(from, (column as usize, row as usize))
        });
        self.player.position = movement.aabb.center();
        self.player.moving = true;
        if screen.1.abs() > 0.1 {
            self.player.facing.toward = screen.1 > 0.0;
        }
        if screen.0.abs() > 0.1 {
            self.player.facing.left = screen.0 < 0.0;
        }
    }
}

impl Play {
    /// Everything that goes on by itself: the clock and night, the weather, sparks,
    /// creatures, and the end of the game.
    fn advance_world(&mut self, shared: &Shared, seconds: f32) {
        // The world goes on (water, weather, sparks) after the end, but play stops.
        sky::update_sparks(&mut self.sparks, seconds);
        self.clouds.update(seconds);
        if self.weather.update(&mut self.rng, seconds).thunder {
            shared.sounds.play(Sfx::Thunder);
        }
        shared.sounds.rain(self.weather.rain);
        if self.beacon_lit {
            self.lit_for += seconds;
            if self.lit_for >= CELEBRATION && self.outcome.is_none() {
                self.outcome = Some(Outcome::Won((self.clock * 10.0) as u32));
            }
        } else if self.outcome.is_none() {
            self.clock += seconds;
            if self.clock >= NIGHT {
                self.outcome = Some(Outcome::Lost(self.carried()));
            }
            self.oil = (self.oil - seconds / 6.0).max(0.0);
            let night = self.night();
            shared
                .sounds
                .music(if night < 0.55 { Mood::Day } else { Mood::Night });
            let before = (self.clock - seconds) / NIGHT;
            if before < 0.6 && night >= 0.6 {
                self.say("Night is falling. Wisps come out in the dark", 4.0);
            }
        }
        let player = self.player.position;
        life::update_crabs(
            &mut self.crabs,
            &self.island,
            &mut self.rng,
            player,
            seconds,
        );
        let night = self.night();
        life::update_wisps(
            &mut self.wisps,
            &self.island,
            &mut self.rng,
            player,
            night,
            seconds,
        );
    }
}

/// Every edge between a land tile and the sea, for the foam.
fn shore_edges(island: &Island) -> Vec<((usize, usize), Dir)> {
    let mut edges = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            if !island.is_land((x, y)) {
                continue;
            }
            for dir in Dir::ALL {
                let (dx, dy) = dir.step();
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                let sea = !(0..SIZE as i32).contains(&nx)
                    || !(0..SIZE as i32).contains(&ny)
                    || !island.is_land((nx as usize, ny as usize));
                if sea {
                    edges.push(((x, y), dir));
                }
            }
        }
    }
    edges
}

impl Scene<Shared> for Play {
    fn start(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.ambience(true);
        shared.sounds.music(Mood::Day);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let dt = ctx.dt();
        let seconds = dt.as_secs_f32();
        self.time += seconds;
        if let Some((_, left)) = &mut self.message {
            *left -= seconds;
            if *left <= 0.0 {
                self.message = None;
            }
        }
        self.advance_world(shared, seconds);
        if self.outcome.is_some() {
            self.player.previous = self.player.position;
            self.player.moving = false;
            return Transition::None;
        }
        let input = ctx.input();
        let controls = &shared.controls;
        if self.turn.is_none() {
            if controls.was_pressed(Action::TurnLeft, input) {
                self.start_turn(-1);
            } else if controls.was_pressed(Action::TurnRight, input) {
                self.start_turn(1);
            }
        }
        let scroll = input.scroll().1;
        if controls.was_pressed(Action::ZoomIn, input) || scroll < 0 {
            self.zoom = (self.zoom + 1).min(ZOOM_ROWS.len() - 1);
        }
        if controls.was_pressed(Action::ZoomOut, input) || scroll > 0 {
            self.zoom = self.zoom.saturating_sub(1);
        }
        if controls.was_pressed(Action::Perf, input) {
            shared.perf = !shared.perf;
        }
        if controls.was_pressed(Action::Map, input) {
            shared.map = !shared.map;
        }
        if let Some(turn) = &mut self.turn {
            turn.timer.update(dt);
            if turn.timer.is_finished() {
                self.turn = None;
            }
        }
        self.update_mouse(shared, ctx);
        self.walk_by_keys(shared, ctx);
        if !self.player.moving {
            self.follow_route(ctx.dt_secs());
        }
        if self.player.moving {
            self.player.walk.update(dt);
        }
        self.footsteps(shared);
        let using = shared.controls.was_pressed(Action::Use, ctx.input());
        self.collect(shared, using);
        self.meet_creatures(shared);

        // The camera follows the player's place on the screen; while turning it stays on
        // the player exactly, so the island turns around them.
        let (x, y) = self.player.position;
        let h = self.island.height_at(x, y);
        let target = View::new(self.angle(), cache_origin()).project(x, y, h);
        if self.turn.is_some() {
            self.camera.center_on(target.0, target.1);
        }
        self.camera.update(dt, Some(target));
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let started = Instant::now();
        let (area, width, height) = view(ctx, frame, self.zoom);
        self.shown = Some((area, width, height));
        if width > 0 {
            self.draw_world(shared, ctx.alpha(), width, height);
            if shared.perf {
                let mode = frame.graphics_mode().to_string();
                self.draw_perf(shared, ctx.fps(), &mode);
            }
            present(frame, area, &self.world, &mut self.big);
        }
        self.render_ms = started.elapsed().as_secs_f32() * 1000.0;
        status_line(
            frame,
            &format!(
                " ISLE · island {} · {} · {} · {:.0} fps",
                self.island.seed,
                frame.graphics_mode(),
                shared.visuals.borrow().describe(),
                ctx.fps(),
            ),
        );
        self.draw_hud(frame);
        help_line(
            frame,
            " ←↑→↓/WASD: move · click: walk · Q/E: turn · wheel/+/-: zoom · M: map · P: pause · G: display",
        );
    }
}

impl Play {
    /// Draws the island, the actors in depth order and the outline of the hidden player
    /// into `self.world` (`width` × `height` world pixels).
    #[allow(
        clippy::too_many_lines,
        reason = "the drawing order (sea, island, actors, weather, light, glow) stays in one place"
    )]
    fn draw_world(&mut self, shared: &Shared, alpha: f32, width: u32, height: u32) {
        if (self.world.width(), self.world.height()) != (width, height) {
            self.world = Canvas::new(width, height);
            self.light = Canvas::new(width, height);
        }
        if self.camera.viewport() != (width as f32, height as f32) {
            self.camera.set_viewport(width as f32, height as f32);
        }
        let offset = self.camera.offset(alpha);
        let angle = self.angle();
        let view = View::new(angle, cache_origin()).shifted(-offset.0 as f32, -offset.1 as f32);
        let world = &mut self.world;
        world.set_blend_mode(BlendMode::Replace);
        world.clear(SEA);
        // The ocean picture repeats, anchored to the island (it moves with the camera).
        {
            let mut visuals = shared.visuals.borrow_mut();
            let ocean = visuals.ocean(self.time);
            let (ow, oh) = (OCEAN_WIDTH as i32, OCEAN_HEIGHT as i32);
            let mut y = -offset.1.rem_euclid(oh);
            while y < height as i32 {
                let mut x = -offset.0.rem_euclid(ow);
                while x < width as i32 {
                    world.blit(ocean, x, y, BlitOptions::default());
                    x += ow;
                }
                y += oh;
            }
        }
        world.set_blend_mode(BlendMode::Alpha);
        self.draw_foam(&view);

        let mut actors = self.actors(alpha, &view);
        actors.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.drawn = None;
        if self.turn.is_some() {
            // Turning: the island is drawn at the current angle.
            for tile in draw_order(&view, &self.island) {
                draw_tile(
                    &mut self.world,
                    &view,
                    &self.island,
                    &self.sprites,
                    tile,
                    Layer::Full,
                );
            }
        } else {
            // Still: the cached island.
            let picture = self
                .terrain
                .picture(&self.island, &self.sprites, self.quarter);
            self.world.blit_region(
                picture,
                PixelRect::new(offset.0, offset.1, width, height),
                offset.0.min(0).abs(),
                offset.1.min(0).abs(),
                BlitOptions::default(),
            );
        }
        // Each actor, back to front, then the tiles in front of it that hide it.
        for &(depth, actor, position, h) in &actors {
            let bounds = self.draw_actor(&view, actor, position, h);
            self.cover(&view, depth, position, h, bounds);
        }
        self.find_hidden();
        let seconds = 1.0 / 60.0;
        self.weather
            .draw_rain(&mut self.world, &mut self.rng, seconds, width, height);
        self.draw_light(&view, alpha);
        self.draw_glow(&view);
        sky::draw_sparks(&mut self.world, &view, &self.sparks);
        self.draw_outline();
        self.draw_hover(&view);
        if shared.map {
            let player = self.player_at(alpha);
            let crystals: Vec<(f32, f32)> = self
                .crystals
                .iter()
                .filter(|c| !c.taken)
                .map(|c| (c.tile.0 as f32 + 0.5, c.tile.1 as f32 + 0.5))
                .collect();
            let wisps: Vec<(f32, f32)> = self
                .wisps
                .iter()
                .filter(|w| w.is_out())
                .map(|w| w.position)
                .collect();
            let markers = minimap::Markers {
                player,
                crystals,
                wisps,
                beacon_lit: self.beacon_lit,
            };
            let map = self
                .minimap
                .draw(&self.island, self.quarter, self.time, &markers);
            minimap::place(&mut self.world, map);
        }
        self.draw_message();
    }

    /// Foam along the shore: white specks that come and go with the waves.
    fn draw_foam(&mut self, view: &View) {
        let time = self.time;
        for &((x, y), dir) in &self.shore {
            let (dx, dy) = dir.step();
            let (dx, dy) = (dx as f32, dy as f32);
            // Along the edge: from one corner of the side to the other.
            let (cx, cy) = (x as f32 + 0.5 + dx * 0.5, y as f32 + 0.5 + dy * 0.5);
            let (ax, ay) = (-dy, dx);
            let phase = iso::hash(x as i32, y as i32) * 6.0;
            for k in 0..6 {
                let t = (k as f32 + 0.5) / 6.0 - 0.5;
                let reach = 0.08 + 0.1 * (0.5 + 0.5 * (time * 1.7 + phase + t * 4.0).sin());
                let (px, py) = (cx + ax * t + dx * reach, cy + ay * t + dy * reach);
                let (sx, sy) = view.project(px, py, 0.0);
                let alpha = (120.0 + 100.0 * (time * 2.3 + phase * 2.0 + k as f32).sin()) as u8;
                self.world.pixel(
                    sx.round() as i32,
                    sy.round() as i32,
                    Rgba::new(235, 248, 255, alpha),
                );
            }
        }
    }

    /// The light layer: the sky's color (darker in rain, bright in lightning), cloud
    /// shadows, then every light added in; it multiplies the scene.
    fn draw_light(&mut self, view: &View, alpha: f32) {
        let night = self.night();
        let mut sky = sky::ambient(night);
        sky = sky::dim(sky, 1.0 - self.weather.rain * 0.22);
        if self.weather.flash > 0.0 {
            sky = sky.lerp(Rgba::rgb(240, 245, 255), self.weather.flash * 0.8);
        }
        let layer = &mut self.light;
        layer.set_blend_mode(BlendMode::Replace);
        layer.clear(sky);
        layer.set_blend_mode(BlendMode::Alpha);
        // Clouds cast shadows while the sun is up.
        let sun = (1.0 - night / 0.6).clamp(0.0, 1.0);
        self.clouds
            .draw(layer, view, sun * (1.0 - self.weather.rain * 0.5));
        layer.set_blend_mode(BlendMode::Add);
        let lights = self.light_sources(view, alpha);
        for (at, radius, color) in lights {
            self.lights.add(&mut self.light, at, radius, color);
        }
        self.world.set_blend_mode(BlendMode::Multiply);
        self.world.blit(&self.light, 0, 0, BlitOptions::default());
        self.world.set_blend_mode(BlendMode::Alpha);
    }

    /// Every light: screen position, radius in world pixels and color.
    fn light_sources(&self, view: &View, alpha: f32) -> Vec<((f32, f32), u32, Rgba)> {
        let mut lights = Vec::new();
        let time = self.time;
        let (px, py) = self.player_at(alpha);
        let h = self.island.height_at(px, py);
        if self.oil > 0.0 {
            // The lantern flickers a little and shrinks as the oil runs low.
            let flicker = 0.92 + 0.08 * (time * 13.0).sin() * (time * 7.3).cos();
            let radius = (26.0 + 34.0 * (self.oil / OIL).sqrt()) as u32;
            let (x, y) = view.project(px, py, h);
            lights.push((
                (x, y - 7.0),
                radius,
                sky::dim(Rgba::rgb(255, 214, 150), flicker),
            ));
        }
        for (i, crystal) in self.crystals.iter().enumerate() {
            if crystal.taken {
                continue;
            }
            let (cx, cy) = (crystal.tile.0 as f32 + 0.5, crystal.tile.1 as f32 + 0.5);
            let level = f32::from(self.island.level_at(crystal.tile));
            let pulse = 0.7 + 0.3 * (time * 2.0 + i as f32).sin();
            let (x, y) = view.project(cx, cy, level);
            lights.push(((x, y - 5.0), 22, sky::dim(Rgba::rgb(110, 210, 255), pulse)));
        }
        let beacon = (
            self.island.beacon.0 as f32 + 0.5,
            self.island.beacon.1 as f32 + 0.5,
        );
        let level = f32::from(self.island.level_at(self.island.beacon));
        let (bx, by) = view.project(beacon.0, beacon.1, level);
        if self.beacon_lit {
            let grow = (self.lit_for / 1.2).min(1.0);
            lights.push((
                (bx, by - 20.0),
                (40.0 + 90.0 * grow) as u32,
                Rgba::rgb(255, 222, 150),
            ));
        } else {
            let blink = if (time * 1.5).fract() < 0.5 { 1.0 } else { 0.4 };
            lights.push(((bx, by - 20.0), 10, sky::dim(Rgba::rgb(255, 90, 70), blink)));
        }
        for wisp in self.wisps.iter().filter(|w| w.is_out()) {
            let (wx, wy) = wisp.position;
            let wh = self.island.height_at(wx, wy) + 1.2 + (wisp.phase * 3.0).sin() * 0.3;
            lights.push((view.project(wx, wy, wh), 18, Rgba::rgb(120, 220, 180)));
        }
        if self.night() > 0.45 {
            for ((x, y, fh), brightness) in self.fireflies.positions(time) {
                if brightness > 0.3 {
                    let color = sky::dim(Rgba::rgb(200, 255, 120), brightness);
                    lights.push((view.project(x, y, fh), 7, color));
                }
            }
        }
        lights
    }

    /// Things that shine on their own, drawn over the light: crystal glints, wisps,
    /// fireflies, the beacon's beam.
    fn draw_glow(&mut self, view: &View) {
        let time = self.time;
        let night = self.night();
        let world = &mut self.world;
        world.set_blend_mode(BlendMode::Add);
        for (i, crystal) in self.crystals.iter().enumerate() {
            if crystal.taken {
                continue;
            }
            let (cx, cy) = (crystal.tile.0 as f32 + 0.5, crystal.tile.1 as f32 + 0.5);
            let level = f32::from(self.island.level_at(crystal.tile));
            let (x, y) = view.project(cx, cy, level + 0.9 + (time * 2.0 + i as f32).sin() * 0.15);
            let glow = (90.0 + 60.0 * (time * 3.0 + i as f32).sin()) as u8;
            world.fill_circle(x as i32, y as i32, 2, Rgba::new(80, 180, 255, glow));
        }
        world.set_blend_mode(BlendMode::Alpha);
        for wisp in self.wisps.iter().filter(|w| w.is_out()) {
            let (wx, wy) = wisp.position;
            let wh = self.island.height_at(wx, wy) + 1.2 + (wisp.phase * 3.0).sin() * 0.3;
            let (x, y) = view.project(wx, wy, wh);
            let size = self.wisp_sprite.width() as i32;
            world.blit(
                &self.wisp_sprite,
                x as i32 - size / 2,
                y as i32 - size / 2,
                BlitOptions::default(),
            );
            world.pixel(x as i32, y as i32, Rgba::WHITE);
        }
        if night > 0.45 {
            let fade = ((night - 0.45) / 0.15).min(1.0);
            for ((x, y, fh), brightness) in self.fireflies.positions(time) {
                let (sx, sy) = view.project(x, y, fh);
                let alpha = (brightness * fade * 255.0) as u8;
                world.pixel(sx as i32, sy as i32, Rgba::new(230, 255, 140, alpha));
            }
        }
        if self.beacon_lit {
            // Two beams sweeping around, as a lighthouse's.
            let beacon = (
                self.island.beacon.0 as f32 + 0.5,
                self.island.beacon.1 as f32 + 0.5,
            );
            let level = f32::from(self.island.level_at(self.island.beacon));
            let (bx, by) = view.project(beacon.0, beacon.1, level);
            let lamp = (bx, by - 19.0);
            let grow = (self.lit_for / 1.2).min(1.0);
            world.set_blend_mode(BlendMode::Add);
            for side in [0.0, std::f32::consts::PI] {
                let angle = time * 1.4 + side;
                let length = 160.0 * grow;
                let (dx, dy) = (angle.cos() * length, angle.sin() * length * 0.5);
                let (wx, wy) = (-angle.sin() * 14.0 * grow, angle.cos() * 7.0 * grow);
                let tip_a = (lamp.0 + dx + wx, lamp.1 + dy + wy);
                let tip_b = (lamp.0 + dx - wx, lamp.1 + dy - wy);
                world.fill_triangle_gradient(
                    (lamp, Rgba::new(255, 236, 170, 150)),
                    (tip_a, Rgba::new(255, 236, 170, 0)),
                    (tip_b, Rgba::new(255, 236, 170, 0)),
                );
            }
            world.fill_circle(
                lamp.0 as i32,
                lamp.1 as i32,
                2,
                Rgba::new(255, 250, 220, 220),
            );
            world.set_blend_mode(BlendMode::Alpha);
        }
    }

    /// The current message in pixel letters near the bottom of the view, wrapped to fit.
    fn draw_message(&mut self) {
        let Some((text, left)) = &self.message else {
            return;
        };
        let font = Font::builtin();
        let room = self.world.width().saturating_sub(16);
        let mut lines: Vec<String> = Vec::new();
        for word in text.to_uppercase().split(' ') {
            match lines.last_mut() {
                Some(line) if font.measure(&format!("{line} {word}")).0 <= room => {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_owned()),
            }
        }
        let line_height = font.measure("A").1 as i32 + 3;
        let fade = (left * 2.0).min(1.0);
        let bottom = self.world.height() as i32 - 8;
        let top = bottom - line_height * lines.len() as i32;
        for (i, line) in lines.iter().enumerate() {
            let (width, _) = font.measure(line);
            let x = (self.world.width() as i32 - width as i32) / 2;
            let y = top + i as i32 * line_height;
            self.world.fill_rect(
                x - 3,
                y - 2,
                width + 6,
                line_height as u32,
                Rgba::new(10, 14, 24, (150.0 * fade) as u8),
            );
            let color = Rgba::new(255, 236, 190, (255.0 * fade) as u8);
            crate::shadow_text(&mut self.world, x, y, line, color, 1);
        }
    }

    /// The performance panel: frame rate, drawing time and how the frame was made.
    fn draw_perf(&mut self, shared: &Shared, fps: f64, mode: &str) {
        let path = if self.turn.is_some() {
            "TURNING: FULL DRAW"
        } else {
            "TERRAIN CACHE"
        };
        let lines = [
            format!("{fps:.0} FPS  {:.1} MS", self.render_ms),
            mode.to_uppercase(),
            path.to_owned(),
            format!(
                "OCEAN {}",
                shared.visuals.borrow().describe().to_uppercase()
            ),
        ];
        let font = Font::builtin();
        let line_height = font.measure("A").1 as i32 + 2;
        let width = lines.iter().map(|l| font.measure(l).0).max().unwrap_or(0);
        let height = line_height as u32 * lines.len() as u32 + 4;
        let top = 3;
        self.world
            .fill_rect(3, top, width + 6, height, Rgba::new(8, 12, 20, 180));
        for (i, line) in lines.iter().enumerate() {
            self.world.text(
                font,
                6,
                top + 3 + i as i32 * line_height,
                line,
                Rgba::rgb(170, 230, 255),
            );
        }
    }

    /// The bottom line: crystals, the night and the lantern oil as gauges.
    fn draw_hud(&self, frame: &mut Frame<'_>) {
        let size = frame.area();
        if size.height < 3 {
            return;
        }
        let y = size.height - 2;
        let row = Rect::new(0, y, size.width, 1);
        let buffer = frame.buffer_mut();
        let dark = Style::new().bg(Color::rgb(14, 18, 28));
        buffer.set_style(row, dark);
        let carried = self.carried();
        let crystals = Line::from(vec![
            Span::styled(" ◆ ", Style::new().fg(Color::rgb(120, 220, 255)).bold()),
            Span::styled(
                format!("{carried}/{CRYSTALS} crystals "),
                Style::new().fg(Color::rgb(220, 235, 255)),
            ),
        ]);
        let left = 22.min(size.width);
        Paragraph::new(crystals)
            .style(dark)
            .render(Rect::new(0, y, left, 1), buffer);
        let rest = size.width.saturating_sub(left);
        let half = rest / 2;
        let night = self.night();
        let sky = sky::ambient(night);
        Gauge::new()
            .ratio(f64::from(night))
            .label(if night < 0.55 { "dusk" } else { "night" })
            .gauge_style(
                Style::new()
                    .fg(Color::rgb(sky.r / 2 + 40, sky.g / 2 + 30, sky.b / 2 + 60))
                    .bg(Color::rgb(30, 34, 50)),
            )
            .render(Rect::new(left, y, half.saturating_sub(1), 1), buffer);
        Gauge::new()
            .ratio(f64::from(self.oil / OIL))
            .label(format!("lantern {:.0}%", self.oil))
            .gauge_style(Style::new().fg(ACCENT).bg(Color::rgb(40, 32, 24)))
            .render(Rect::new(left + half, y, rest - half, 1), buffer);
    }

    /// The view of the last frame at the camera's current place (for the mouse).
    fn current_view(&self) -> View {
        let offset = self.camera.offset(1.0);
        View::new(self.angle(), cache_origin()).shifted(-offset.0 as f32, -offset.1 as f32)
    }

    /// Finds the tile under the mouse and the walk to it; a click starts that walk.
    fn update_mouse(&mut self, shared: &Shared, ctx: &Context) {
        let input = ctx.input();
        let (Some(mouse), Some((area, width, height))) = (input.mouse_position(), self.shown)
        else {
            self.hover = None;
            return;
        };
        if !area.contains(mouse) || width == 0 || self.turn.is_some() {
            self.hover = None;
            return;
        }
        // The middle of the cell under the mouse, in world pixels.
        let sx = (f32::from(mouse.x - area.x) + 0.5) * width as f32 / f32::from(area.width);
        let sy = (f32::from(mouse.y - area.y) + 0.5) * height as f32 / f32::from(area.height);
        let view = self.current_view();
        let Some(tile) = iso::pick(&view, &self.island, sx, sy) else {
            self.hover = None;
            return;
        };
        let from = self.player_tile();
        if self
            .hover
            .as_ref()
            .is_none_or(|h| h.tile != tile || h.from != from)
        {
            let path = self.island.path(from, tile).unwrap_or_default();
            self.hover = Some(Hover { tile, from, path });
        }
        if shared.controls.was_pressed(Action::Walk, input) {
            if let Some(hover) = &self.hover {
                // The first tile is where the player stands.
                self.route = hover.path.iter().skip(1).copied().collect();
            }
        }
    }

    /// Walks towards the next tile of a clicked route.
    fn follow_route(&mut self, seconds: f32) {
        let Some(&next) = self.route.first() else {
            return;
        };
        let target = (next.0 as f32 + 0.5, next.1 as f32 + 0.5);
        let (x, y) = self.player.position;
        let (dx, dy) = (target.0 - x, target.1 - y);
        let distance = (dx * dx + dy * dy).sqrt();
        let step = SPEED * seconds;
        if distance < 1e-4 {
            self.route.remove(0);
        } else if distance <= step {
            let before = self.player.position;
            self.step_player((dx, dy), self.screen_direction((dx, dy)));
            if self.player.position == before {
                // Something is in the way (it should not be): give up the walk.
                self.route.clear();
            } else {
                self.route.remove(0);
            }
        } else {
            let (mx, my) = (dx / distance * step, dy / distance * step);
            self.step_player((mx, my), self.screen_direction((dx, dy)));
        }
    }

    /// The screen direction of a world direction, for the sprite.
    fn screen_direction(&self, (dx, dy): (f32, f32)) -> (f32, f32) {
        let view = View::new(self.angle(), (0.0, 0.0));
        let c = SIZE as f32 / 2.0;
        let (ax, ay) = view.project(c, c, 0.0);
        let (bx, by) = view.project(c + dx, c + dy, 0.0);
        (bx - ax, by - ay)
    }

    /// The tile under the mouse as an outline on its top and the walk to it as dots; the
    /// end of a clicked walk as a smaller pulsing outline.
    fn draw_hover(&mut self, view: &View) {
        let pulse = 0.75 + 0.25 * (self.time * 5.0).sin();
        if let Some(&goal) = self.route.last() {
            self.outline(view, goal, Rgba::new(255, 214, 120, (230.0 * pulse) as u8));
        }
        let Some(hover) = &self.hover else {
            return;
        };
        let (tile, reachable) = (hover.tile, !hover.path.is_empty());
        let dots: Vec<(usize, usize)> = hover.path.iter().skip(1).copied().collect();
        let color = if reachable {
            Rgba::new(255, 250, 220, (200.0 * pulse) as u8)
        } else {
            Rgba::new(255, 110, 90, (170.0 * pulse) as u8)
        };
        self.outline(view, tile, color);
        // Dots along the walk, skipping the player's own tile.
        for (px, py) in dots {
            let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
            let h = self.island.height_at(cx, cy);
            let (sx, sy) = view.project(cx, cy, h);
            let (dx, dy) = (sx.round() as i32 - 1, sy.round() as i32 - 1);
            self.world
                .fill_rect(dx, dy + 1, 3, 1, Rgba::new(40, 30, 20, 90));
            self.world
                .fill_rect(dx, dy, 3, 1, Rgba::new(255, 250, 220, 220));
        }
    }

    /// An outline around the top of `tile`.
    fn outline(&mut self, view: &View, tile: (usize, usize), color: Rgba) {
        let heights = iso::corners(&self.island, tile);
        let (x, y) = (tile.0 as f32, tile.1 as f32);
        let points = [(x, y), (x + 1.0, y), (x + 1.0, y + 1.0), (x, y + 1.0)];
        let projected: Vec<(i32, i32)> = points
            .iter()
            .zip(heights)
            .map(|(&(px, py), h)| {
                let (sx, sy) = view.project(px, py, h);
                (sx.round() as i32, sy.round() as i32)
            })
            .collect();
        for k in 0..4 {
            let (a, b) = (projected[k], projected[(k + 1) % 4]);
            self.world.line(a.0, a.1, b.0, b.1, color);
        }
    }

    /// The actors with their depth, position and height.
    fn actors(&self, alpha: f32, view: &View) -> Vec<(f32, Actor, (f32, f32), f32)> {
        let lerp = |a: f32, b: f32| a + (b - a) * alpha;
        let player = &self.player;
        let position = (
            lerp(player.previous.0, player.position.0),
            lerp(player.previous.1, player.position.1),
        );
        let mut actors = vec![(
            view.depth(position.0, position.1),
            Actor::Player,
            position,
            self.island.height_at(position.0, position.1),
        )];
        let beacon = (
            self.island.beacon.0 as f32 + 0.5,
            self.island.beacon.1 as f32 + 0.5,
        );
        actors.push((
            view.depth(beacon.0, beacon.1),
            Actor::Beacon,
            beacon,
            f32::from(self.island.level_at(self.island.beacon)),
        ));
        for (i, crystal) in self.crystals.iter().enumerate() {
            if crystal.taken {
                continue;
            }
            let at = (crystal.tile.0 as f32 + 0.5, crystal.tile.1 as f32 + 0.5);
            let level = f32::from(self.island.level_at(crystal.tile));
            actors.push((view.depth(at.0, at.1), Actor::Crystal(i), at, level));
        }
        for (i, crab) in self.crabs.iter().enumerate() {
            let at = (
                lerp(crab.previous.0, crab.position.0),
                lerp(crab.previous.1, crab.position.1),
            );
            let h = self.island.height_at(at.0, at.1);
            actors.push((view.depth(at.0, at.1), Actor::Crab(i), at, h));
        }
        actors
    }

    /// Draws an actor standing at `position` and height `h`; returns its screen bounds.
    fn draw_actor(
        &mut self,
        view: &View,
        actor: Actor,
        position: (f32, f32),
        h: f32,
    ) -> (f32, f32, f32, f32) {
        let at = view.project(position.0, position.1, h);
        let world = &mut self.world;
        match actor {
            Actor::Player => {
                // A soft shadow under the feet.
                world.fill_circle(
                    at.0.round() as i32,
                    at.1.round() as i32,
                    3,
                    Rgba::new(0, 0, 0, 70),
                );
                let player = &self.player;
                let frame = if player.moving {
                    player.walk.frame()
                } else {
                    0
                };
                let frame = frame + if player.facing.toward { 0 } else { 3 };
                let (fw, fh) = self.player_sheet.frame_size();
                let left = (at.0 - fw as f32 / 2.0).round() as i32;
                let top = at.1.round() as i32 - fh as i32 + 1;
                let options = BlitOptions {
                    flip_x: player.facing.left == player.facing.toward,
                    ..BlitOptions::default()
                };
                self.player_sheet.draw(world, frame, left, top, options);
                // Remember the player's pixels, to find what covers them later.
                let mut pixels = Vec::with_capacity((fw * fh) as usize);
                let mut mask = Vec::with_capacity((fw * fh) as usize);
                let mut sprite = Canvas::new(fw, fh);
                self.player_sheet.draw(&mut sprite, frame, 0, 0, options);
                for y in 0..fh {
                    for x in 0..fw {
                        let (px, py) = (left + x as i32, top + y as i32);
                        let inside = px >= 0 && py >= 0;
                        pixels.push(if inside {
                            world.get(px as u32, py as u32).unwrap_or(Rgba::TRANSPARENT)
                        } else {
                            Rgba::TRANSPARENT
                        });
                        mask.push(sprite.get(x, y).is_some_and(|p| p.a > 0));
                    }
                }
                self.drawn = Some(Drawn {
                    left,
                    top,
                    width: fw,
                    height: fh,
                    pixels,
                    mask,
                });
                (left as f32, top as f32, fw as f32, fh as f32)
            }
            Actor::Beacon => {
                let sprite = &self.beacon[usize::from(self.beacon_lit)];
                iso::stand(world, sprite, (at.0, at.1 + 1.0), BlitOptions::default());
                let (w, h) = (sprite.width() as f32, sprite.height() as f32);
                (at.0 - w / 2.0, at.1 - h, w, h + 1.0)
            }
            Actor::Crystal(i) => {
                // Floating and bobbing, with a small shadow below.
                let bob = (self.time * 2.0 + i as f32).sin() * 1.5;
                world.fill_rect(at.0 as i32 - 2, at.1 as i32, 5, 1, Rgba::new(0, 0, 0, 60));
                let frame = ((self.time * 6.0 + i as f32 * 1.7) as usize) % 4;
                let (fw, fh) = self.crystal_sheet.frame_size();
                let left = (at.0 - fw as f32 / 2.0).round() as i32;
                let top = (at.1 - 3.0 + bob).round() as i32 - fh as i32;
                self.crystal_sheet
                    .draw(world, frame, left, top, BlitOptions::default());
                (left as f32, top as f32, fw as f32, fh as f32 + 4.0)
            }
            Actor::Crab(i) => {
                let crab = &self.crabs[i];
                let frame = crab.walk.frame();
                let (fw, fh) = self.crab_sheet.frame_size();
                let left = (at.0 - fw as f32 / 2.0).round() as i32;
                let top = at.1.round() as i32 - fh as i32 + 1;
                self.crab_sheet
                    .draw(world, frame, left, top, BlitOptions::default());
                (left as f32, top as f32, fw as f32, fh as f32)
            }
        }
    }

    /// Draws again the tiles in front of an actor that rise above it or carry a prop and
    /// overlap it on the screen, so they hide it.
    fn cover(
        &mut self,
        view: &View,
        depth: f32,
        position: (f32, f32),
        h: f32,
        bounds: (f32, f32, f32, f32),
    ) {
        let (cx, cy) = (position.0 as i32, position.1 as i32);
        let mut tiles = Vec::new();
        for y in cy - 4..=cy + 4 {
            for x in cx - 4..=cx + 4 {
                if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
                    continue;
                }
                let tile = (x as usize, y as usize);
                if !self.island.is_land(tile) {
                    continue;
                }
                let tile_depth = view.depth(x as f32 + 0.5, y as f32 + 0.5);
                if tile_depth <= depth {
                    continue;
                }
                if iso::overlaps(bounds, tile_bounds(view, &self.island, &self.sprites, tile)) {
                    tiles.push((tile_depth, tile));
                }
            }
        }
        tiles.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, tile) in tiles {
            draw_tile(
                &mut self.world,
                view,
                &self.island,
                &self.sprites,
                tile,
                Layer::Above(h),
            );
        }
    }

    /// The player's pixels that something drawn later covered.
    fn find_hidden(&mut self) {
        self.hidden.clear();
        let Some(drawn) = &self.drawn else {
            return;
        };
        for y in 0..drawn.height {
            for x in 0..drawn.width {
                let i = (y * drawn.width + x) as usize;
                if !drawn.mask[i] {
                    continue;
                }
                let (px, py) = (drawn.left + x as i32, drawn.top + y as i32);
                if px < 0 || py < 0 {
                    continue;
                }
                if self
                    .world
                    .get(px as u32, py as u32)
                    .is_some_and(|now| now != drawn.pixels[i])
                {
                    self.hidden.push((px, py));
                }
            }
        }
    }

    /// The hidden part of the player as a bright see-through silhouette.
    fn draw_outline(&mut self) {
        if self.hidden.len() < 4 {
            return;
        }
        let color = Rgba::new(255, 236, 170, 150);
        for &(x, y) in &self.hidden {
            self.world.pixel(x, y, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Play, Turn};
    use crate::island::{Prop, index};
    use crate::write_png;

    /// A clicked walk reaches every crystal, step by step at 60 updates a second.
    #[test]
    fn clicked_walks_arrive() {
        let mut play = Play::new(4);
        for crystal in play.island.crystals.clone() {
            let path = play
                .island
                .path(play.player_tile(), crystal)
                .expect("reachable");
            play.route = path.into_iter().skip(1).collect();
            for _ in 0..60 * 60 {
                if play.route.is_empty() {
                    break;
                }
                play.follow_route(1.0 / 60.0);
            }
            assert!(play.route.is_empty(), "stuck on the way to {crystal:?}");
            assert_eq!(play.player_tile(), crystal);
        }
    }

    /// Writes frames of the game to the folder in `ISLE_SNAPSHOTS` (skipped without it):
    /// the start, half way through a turn, and the player behind a tree.
    #[test]
    fn snapshots() {
        let Ok(folder) = std::env::var("ISLE_SNAPSHOTS") else {
            return;
        };
        let shared = crate::Shared::new(std::env::var("ISLE_GPU").is_ok(), false);
        let mut play = Play::new(2);
        let settle = |play: &mut Play| {
            let (x, y) = play.player.position;
            let h = play.island.height_at(x, y);
            let target = super::View::new(play.angle(), super::cache_origin()).project(x, y, h);
            play.camera.set_viewport(240.0, 140.0);
            play.camera.center_on(target.0, target.1);
            play.camera.update(Duration::from_millis(16), Some(target));
        };
        settle(&mut play);
        play.draw_world(&shared, 1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-start.png"), 4);

        // Hovering a tile a few steps up the beach.
        let from = play.player_tile();
        let tile = (from.0.saturating_sub(3), from.1.saturating_sub(4));
        let path = play.island.path(from, tile).unwrap_or_default();
        play.hover = Some(super::Hover { tile, from, path });
        play.draw_world(&shared, 1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-hover.png"), 4);
        play.hover = None;

        play.turn = Some(Turn {
            from: 0.0,
            to: std::f32::consts::FRAC_PI_2,
            timer: vnce::vnce_game::Timer::once(Duration::from_millis(450)),
        });
        if let Some(turn) = &mut play.turn {
            turn.timer.update(Duration::from_millis(225));
        }
        settle(&mut play);
        play.draw_world(&shared, 1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-turning.png"), 4);
        play.turn = None;

        // Dusk, a rainy night, and the lit beacon at night.
        settle(&mut play);
        play.clock = super::NIGHT * 0.5;
        play.draw_world(&shared, 1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-dusk.png"), 4);
        play.clock = super::NIGHT * 0.85;
        play.weather.start_shower();
        play.weather.rain = 1.0;
        for wisp in &mut play.wisps {
            wisp.away = 0.0;
            wisp.position = (play.player.position.0 + 2.0, play.player.position.1 - 1.0);
        }
        play.draw_world(&shared, 1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-night-rain.png"), 4);
        play.weather = super::Weather::new();
        let beacon = play.island.beacon;
        play.player.position = (beacon.0 as f32 + 1.5, beacon.1 as f32 + 0.5);
        play.player.previous = play.player.position;
        play.beacon_lit = true;
        play.lit_for = 2.0;
        settle(&mut play);
        play.draw_world(&shared, 1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-beacon.png"), 4);
        play.beacon_lit = false;
        play.clock = 0.0;

        // Behind a tree: the first tree whose tile behind it (up the screen) is walkable.
        let island = &play.island;
        let spot = (0..crate::island::SIZE * crate::island::SIZE).find_map(|i| {
            let (x, y) = (i % crate::island::SIZE, i / crate::island::SIZE);
            let tall = matches!(island.prop[i], Some(Prop::Tree | Prop::Pine));
            (tall
                && x > 0
                && y > 0
                && island.is_walkable((x - 1, y - 1))
                && island.level_at((x - 1, y - 1)) == island.level_at((x, y))
                && island.prop[index(x - 1, y - 1)].is_none())
            .then_some((x as f32 - 0.5, y as f32 - 0.5))
        });
        if let Some(spot) = spot {
            play.player.position = spot;
            play.player.previous = spot;
            settle(&mut play);
            play.draw_world(&shared, 1.0, 240, 140);
            write_png(&play.world, &format!("{folder}/play-behind-tree.png"), 4);
            assert!(!play.hidden.is_empty(), "the tree hides part of the player");
        }
    }
}
