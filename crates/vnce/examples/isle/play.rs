//! The island: walking, the turning and zooming camera, depth sorting with the terrain
//! cache, and the x-ray outline of the player behind trees and cliffs.

use std::f32::consts::FRAC_PI_2;
use std::time::{Duration, Instant};

use vnce::vnce_game::{
    Aabb, Animation, Animator, Camera, Playback, Scene, SpriteSheet, Timer, Transition, ease,
    move_and_collide,
};
use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, PixelRect, Rgba};
use vnce::vnce_runtime::Context;
use vnce::vnce_term::Frame;

use crate::art;
use crate::island::{Island, Prop, SIZE};
use crate::iso::{
    self, Layer, Sprites, Terrain, View, cache_origin, cache_size, draw_order, draw_tile,
    tile_bounds,
};
use crate::{Action, Shared, ZOOM_ROWS, help_line, present, status_line, view};

/// Walking speed in tiles per second.
const SPEED: f32 = 3.4;
/// Size of the player's feet for collisions, in tiles.
const FEET: f32 = 0.34;
/// How long a quarter turn of the camera takes.
const TURN: Duration = Duration::from_millis(450);

/// The deep sea behind everything (the ocean picture comes later).
const SEA: Rgba = Rgba::rgb(28, 86, 132);

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
}

impl Play {
    pub fn new(seed: u64) -> Self {
        let island = Island::generate(seed);
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
            island,
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

impl Scene<Shared> for Play {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let dt = ctx.dt();
        self.time += dt.as_secs_f32();
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
        self.walk_by_keys(shared, ctx);
        if self.player.moving {
            self.player.walk.update(dt);
        }

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
        if width > 0 {
            self.draw_world(ctx.alpha(), width, height);
            present(frame, area, &self.world, &mut self.big);
        }
        let render = started.elapsed();
        status_line(
            frame,
            &format!(
                " ISLE · island {} · {} · {:.0} fps · {:.1} ms",
                self.island.seed,
                frame.graphics_mode(),
                ctx.fps(),
                render.as_secs_f32() * 1000.0
            ),
        );
        let _ = shared;
        help_line(
            frame,
            " ←↑→↓/WASD: move · Q/E: turn · wheel/+/-: zoom · P: pause · G: display method",
        );
    }
}

impl Play {
    /// Draws the island, the actors in depth order and the outline of the hidden player
    /// into `self.world` (`width` × `height` world pixels).
    fn draw_world(&mut self, alpha: f32, width: u32, height: u32) {
        if (self.world.width(), self.world.height()) != (width, height) {
            self.world = Canvas::new(width, height);
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
        world.set_blend_mode(BlendMode::Alpha);

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
        self.draw_outline();
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

    /// Writes frames of the game to the folder in `ISLE_SNAPSHOTS` (skipped without it):
    /// the start, half way through a turn, and the player behind a tree.
    #[test]
    fn snapshots() {
        let Ok(folder) = std::env::var("ISLE_SNAPSHOTS") else {
            return;
        };
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
        play.draw_world(1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-start.png"), 4);

        play.turn = Some(Turn {
            from: 0.0,
            to: std::f32::consts::FRAC_PI_2,
            timer: vnce::vnce_game::Timer::once(Duration::from_millis(450)),
        });
        if let Some(turn) = &mut play.turn {
            turn.timer.update(Duration::from_millis(225));
        }
        settle(&mut play);
        play.draw_world(1.0, 240, 140);
        write_png(&play.world, &format!("{folder}/play-turning.png"), 4);
        play.turn = None;

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
            play.draw_world(1.0, 240, 140);
            write_png(&play.world, &format!("{folder}/play-behind-tree.png"), 4);
            assert!(!play.hidden.is_empty(), "the tree hides part of the player");
        }
    }
}
