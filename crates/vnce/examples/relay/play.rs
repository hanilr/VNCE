//! The station: moving, drones, cores, the sound puzzle, lighting and the heads-up display.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use vnce::vnce_game::{
    Aabb, Animation, Animator, Camera, Emitter, Playback, Rng, Scene, SpatialGrid, Spread,
    SpriteSheet, Timer, Transition, ease, move_and_collide,
};
use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Font, PixelRect, Rgba};
use vnce::vnce_runtime::Context;
use vnce::vnce_term::{Frame, GraphicsMode};

use crate::audio::{PADS, Sfx};
use crate::visuals::Slot;
use crate::world::{Room, Station, TILE};
use crate::{
    Action, Shared, art, format_time, help_line, present, scenes, shadow_text, showrooms,
    status_line, view,
};

const PLAYER_SPEED: f32 = 62.0;
const ENERGY: u32 = 3;
/// Light everywhere, before lamps.
const AMBIENT: Rgba = Rgba::rgb(52, 54, 76);
/// Notes of the sound lab puzzle.
const SEQUENCE: usize = 4;

struct Player {
    body: Aabb,
    previous: (f32, f32),
    facing_left: bool,
    walk: Animator,
    moving: bool,
    hurt: Timer,
    energy: u32,
}

struct Drone {
    /// The room the drone guards and never leaves.
    home: Room,
    body: Aabb,
    previous: (f32, f32),
    heading: (f32, f32),
    turn: Timer,
    rotor: Animator,
}

struct Core {
    room: Room,
    body: Aabb,
    /// Wing cores behind a task appear when it is done.
    visible: bool,
    taken: bool,
}

/// The sound lab: repeat the notes the pads play.
struct Puzzle {
    sequence: [usize; SEQUENCE],
    progress: usize,
    /// While the pads play the sequence: the step and the time until the next one.
    demo: Option<(usize, f32)>,
    /// How brightly each pad glows (fades out).
    glow: [f32; PADS],
    /// The pad under the player.
    standing: Option<usize>,
    /// The last pad played and when (for the waveform screen).
    last: Option<(usize, f32)>,
    solved: bool,
}

struct Sprites {
    player: SpriteSheet,
    drone: SpriteSheet,
    core: SpriteSheet,
    relay: SpriteSheet,
    pad: SpriteSheet,
    door: Canvas,
    energy: [Canvas; 2],
}

/// What drawing the world needs to know about the frame (kept apart from `Context` so the
/// world can also be drawn in tests).
pub struct FrameInfo {
    /// Interpolation between the last two updates.
    pub alpha: f32,
    pub fps: f64,
    pub ups: f64,
    pub mode: GraphicsMode,
}

pub struct Play {
    station: Station,
    /// The whole map drawn once; every frame copies the visible part.
    map_image: Canvas,
    player: Player,
    drones: Vec<Drone>,
    cores: Vec<Core>,
    carried: u32,
    /// Sparks bursting from cores and pads, in world pixels.
    sparks: Emitter,
    camera: Camera,
    rng: Rng,
    time: f32,
    /// Play time in tenths of a second (stops while paused).
    clock: f32,
    grid: SpatialGrid<usize>,
    room: Room,
    banner: Timer,
    /// The gallery opened from this room, if any (its core appears on return).
    pending: Option<Room>,
    puzzle: Puzzle,
    sprites: Sprites,
    /// Light sprites by radius and color.
    lights: HashMap<(u32, [u8; 3]), Canvas>,
    world: Canvas,
    light: Canvas,
    big: Canvas,
}

impl Play {
    pub fn new(seed: u64) -> Self {
        let station = Station::build();
        let mut rng = Rng::new(seed);
        let bounds = station.map.bounds();
        let mut map_image = Canvas::new(bounds.width as u32, bounds.height as u32);
        station.map.draw(&station.tiles, &mut map_image, (0, 0));
        let body = Aabb::from_center(station.start.0, station.start.1, 6.0, 6.0);
        let player = Player {
            previous: (body.x, body.y),
            body,
            facing_left: false,
            walk: Animator::new(Animation::new(
                [0, 1],
                Duration::from_millis(160),
                Playback::Loop,
            )),
            moving: false,
            hurt: Timer::once(Duration::ZERO),
            energy: ENERGY,
        };
        let rotor = Animation::new([0, 1], Duration::from_millis(90), Playback::Loop);
        let drones = station
            .drones
            .iter()
            .map(|&(x, y)| {
                let body = Aabb::from_center(x, y, 6.0, 5.0);
                Drone {
                    home: station.room_at(x, y),
                    previous: (body.x, body.y),
                    body,
                    heading: (0.0, 0.0),
                    turn: Timer::once(Duration::ZERO),
                    rotor: Animator::new(rotor.clone()),
                }
            })
            .collect();
        let cores = station
            .cores
            .iter()
            .map(|&(room, (x, y))| Core {
                room,
                body: Aabb::from_center(x, y, 5.0, 5.0),
                // The observatory and reactor cores lie in the open (the reactor's is guarded by
                // drones).
                visible: matches!(room, Room::Observatory | Room::Reactor),
                taken: false,
            })
            .collect();
        let mut sequence = [0; SEQUENCE];
        for note in &mut sequence {
            *note = rng.below(PADS as u32) as usize;
        }
        let mut camera = Camera::new(200.0, 120.0)
            .with_bounds(bounds)
            .with_smoothing(0.18)
            .with_dead_zone(18.0, 12.0);
        camera.center_on(station.start.0, station.start.1);
        Self {
            map_image,
            player,
            drones,
            cores,
            carried: 0,
            sparks: Emitter::new()
                .spread(Spread::Circle)
                .speed(20.0, 70.0)
                .life(0.4, 0.9)
                .drag(4.8),
            camera,
            rng,
            time: 0.0,
            clock: 0.0,
            grid: SpatialGrid::new(16.0),
            room: Room::Atrium,
            banner: Timer::once(Duration::from_millis(3500)),
            pending: None,
            puzzle: Puzzle {
                sequence,
                progress: 0,
                demo: None,
                glow: [0.0; PADS],
                standing: None,
                last: None,
                solved: false,
            },
            sprites: Sprites {
                player: art::player_sheet(),
                drone: art::drone_sheet(),
                core: art::core_sheet(),
                relay: art::relay_sheet(),
                pad: art::pad_sheet(),
                door: art::door_panel(),
                energy: [art::energy_icon(false), art::energy_icon(true)],
            },
            lights: HashMap::new(),
            world: Canvas::new(0, 0),
            light: Canvas::new(0, 0),
            big: Canvas::new(0, 0),
            station,
        }
    }

    fn burst(&mut self, (x, y): (f32, f32), color: Rgba, count: usize) {
        self.sparks.set_colors([color]);
        self.sparks.burst(&mut self.rng, [x, y, 0.0], count);
    }

    fn reveal(&mut self, room: Room) {
        if let Some(index) = self.cores.iter().position(|c| c.room == room && !c.visible) {
            self.cores[index].visible = true;
            let at = self.cores[index].body.center();
            self.burst(at, Rgba::rgb(120, 255, 210), 28);
            self.camera.shake(1.5, Duration::from_millis(200));
        }
    }

    fn near(&self, target: Aabb) -> bool {
        self.player.body.intersects(&Aabb::new(
            target.x - 6.0,
            target.y - 6.0,
            target.width + 12.0,
            target.height + 12.0,
        ))
    }

    fn move_player(&mut self, shared: &Shared, ctx: &Context) {
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
        let seconds = ctx.dt_secs();
        let player = &mut self.player;
        if dx != 0.0 {
            player.facing_left = dx < 0.0;
        }
        player.moving = dx != 0.0 || dy != 0.0;
        if player.moving {
            player.walk.update(ctx.dt());
        }
        player.previous = (player.body.x, player.body.y);
        let station = &self.station;
        let movement = move_and_collide(
            player.body,
            dx * PLAYER_SPEED * seconds,
            dy * PLAYER_SPEED * seconds,
            TILE as f32,
            |c, r| station.is_solid(c, r),
        );
        player.body = movement.aabb;
    }

    fn move_drones(&mut self, dt: Duration) {
        let seconds = dt.as_secs_f32();
        let target = self.player.body.center();
        for index in 0..self.drones.len() {
            let drone = &mut self.drones[index];
            drone.previous = (drone.body.x, drone.body.y);
            drone.rotor.update(dt);
            drone.turn.update(dt);
            let (x, y) = drone.body.center();
            let (dx, dy) = (target.0 - x, target.1 - y);
            let distance = (dx * dx + dy * dy).sqrt();
            let (hx, hy, pace) = if distance < 46.0 && distance > 0.5 {
                (dx / distance, dy / distance, 40.0)
            } else {
                if drone.turn.is_finished() {
                    let angle = self.rng.unit() * std::f32::consts::TAU;
                    drone.heading = (angle.cos(), angle.sin());
                    drone.turn =
                        Timer::once(Duration::from_millis(500 + u64::from(self.rng.below(1200))));
                }
                (drone.heading.0, drone.heading.1, 26.0)
            };
            let station = &self.station;
            let body = drone.body;
            let movement = move_and_collide(
                body,
                hx * pace * seconds,
                hy * pace * seconds,
                TILE as f32,
                |c, r| station.is_solid(c, r),
            );
            let (nx, ny) = movement.aabb.center();
            let drone = &mut self.drones[index];
            // Drones stay in the room they start in (the reactor): they turn back at its doors.
            if station.room_at(nx, ny) == drone.home {
                drone.body = movement.aabb;
                if movement.hit.any() {
                    drone.heading = (-drone.heading.0, -drone.heading.1);
                }
            } else {
                drone.heading = (-hx, -hy);
                drone.turn = Timer::once(Duration::from_millis(800));
            }
        }
    }

    /// Pads: the demo plays the sequence; stepping on pads repeats it.
    fn update_puzzle(&mut self, shared: &Shared, seconds: f32) {
        for glow in &mut self.puzzle.glow {
            *glow = (*glow - seconds * 2.0).max(0.0);
        }
        let time = self.time;
        if let Some((step, wait)) = self.puzzle.demo {
            let wait = wait - seconds;
            if wait > 0.0 {
                self.puzzle.demo = Some((step, wait));
            } else if step < SEQUENCE {
                let pad = self.puzzle.sequence[step];
                shared.sounds.pad(pad);
                self.puzzle.glow[pad] = 1.0;
                self.puzzle.last = Some((pad, time));
                self.puzzle.demo = Some((step + 1, 0.5));
            } else {
                self.puzzle.demo = None;
            }
            return;
        }
        let (px, py) = self.player.body.center();
        let standing = self
            .station
            .pads
            .iter()
            .position(|pad| pad.contains(px, py));
        if standing != self.puzzle.standing {
            self.puzzle.standing = standing;
            if let Some(pad) = standing {
                shared.sounds.pad(pad);
                self.puzzle.glow[pad] = 1.0;
                self.puzzle.last = Some((pad, time));
                if !self.puzzle.solved {
                    if pad == self.puzzle.sequence[self.puzzle.progress] {
                        self.puzzle.progress += 1;
                        if self.puzzle.progress == SEQUENCE {
                            self.puzzle.solved = true;
                            shared.sounds.play(Sfx::Relay);
                            self.reveal(Room::SoundLab);
                        }
                    } else {
                        self.puzzle.progress = 0;
                        shared.sounds.play(Sfx::Wrong);
                        self.puzzle.demo = Some((0, 0.8));
                    }
                }
            }
        }
    }

    /// Lights of the frame in world pixels: (x, y, radius, color).
    fn light_sources(&self) -> Vec<(f32, f32, u32, Rgba)> {
        let pulse = |speed: f32| 0.75 + 0.25 * (self.time * speed).sin();
        let scaled = |color: Rgba, level: f32| {
            let s = |c: u8| (f32::from(c) * level).min(255.0) as u8;
            Rgba::rgb(s(color.r), s(color.g), s(color.b))
        };
        let mut lights: Vec<(f32, f32, u32, Rgba)> = self
            .station
            .lamps
            .iter()
            .map(|lamp| (lamp.x, lamp.y, 38, scaled(lamp.color, 0.75)))
            .collect();
        let (px, py) = self.player.body.center();
        let flicker = 0.92 + 0.08 * (self.time * 13.0).sin() * (self.time * 7.0).cos();
        lights.push((px, py, 62, scaled(Rgba::rgb(255, 232, 196), flicker)));
        for core in self.cores.iter().filter(|c| c.visible && !c.taken) {
            let (x, y) = core.body.center();
            lights.push((x, y, 20, scaled(Rgba::rgb(90, 255, 200), pulse(5.0))));
        }
        let (rx, ry) = self.station.relay.center();
        let relay = if self.carried == 5 {
            scaled(Rgba::rgb(120, 255, 160), pulse(6.0))
        } else {
            scaled(Rgba::rgb(60, 120, 110), 0.6 + 0.08 * self.carried as f32)
        };
        lights.push((rx, ry, 34, relay));
        let (cx, cy) = self.station.reactor.center();
        lights.push((cx, cy, 50, scaled(Rgba::rgb(255, 140, 60), pulse(2.5))));
        let window = self.station.window;
        for i in 0..3 {
            let x = window.x + window.width * (0.2 + 0.3 * i as f32);
            lights.push((
                x,
                window.y + window.height + 4.0,
                30,
                Rgba::rgb(110, 80, 200),
            ));
        }
        for (pad, glow) in self.station.pads.iter().zip(self.puzzle.glow) {
            if glow > 0.05 {
                let (x, y) = pad.center();
                lights.push((x, y, 18, scaled(Rgba::rgb(120, 255, 255), glow)));
            }
        }
        for drone in &self.drones {
            let (x, y) = drone.body.center();
            lights.push((x, y, 12, Rgba::rgb(150, 30, 40)));
        }
        lights
    }
}

impl Scene<Shared> for Play {
    fn start(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.music(true);
    }

    fn resume(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        if let Some(room) = self.pending.take() {
            shared.sounds.play(Sfx::Relay);
            self.reveal(room);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one update reads best as the game's steps in order"
    )]
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if std::mem::take(&mut shared.to_title) {
            return Transition::switch(scenes::Title::new());
        }
        let input = ctx.input();
        let controls = &shared.controls;
        if controls.was_pressed(Action::Pause, input) {
            shared.sounds.play(Sfx::Blip);
            return Transition::push(scenes::Pause::new());
        }
        if controls.was_pressed(Action::Quit, input) {
            shared.sounds.music(false);
            shared.store(ctx);
            return Transition::switch(scenes::Title::new());
        }
        if controls.was_pressed(Action::Method, input) {
            shared.cycle_method(ctx);
        }
        if controls.was_pressed(Action::Perf, input) {
            shared.perf = !shared.perf;
        }
        let dt = ctx.dt();
        let seconds = ctx.dt_secs();
        self.time += seconds;
        self.clock += seconds * 10.0;

        self.move_player(shared, ctx);
        self.player.hurt.update(dt);
        self.banner.update(dt);
        let center = self.player.body.center();
        let room = self.station.room_at(center.0, center.1);
        if room != self.room && room != Room::Corridor {
            self.room = room;
            self.banner = Timer::once(Duration::from_millis(2800));
            if room == Room::SoundLab && !self.puzzle.solved {
                self.puzzle.progress = 0;
                self.puzzle.demo = Some((0, 0.8));
            }
        }
        for door in &mut self.station.doors {
            if door.update(dt, center) {
                shared.sounds.play(Sfx::Door);
            }
        }
        self.move_drones(dt);
        self.update_puzzle(shared, seconds);

        // Using the galleries.
        if shared.controls.was_pressed(Action::Use, ctx.input()) {
            if self.near(self.station.screen) {
                shared.sounds.play(Sfx::Blip);
                self.pending = Some(Room::Prism);
                return Transition::push(showrooms::Prism::new());
            }
            if self.near(self.station.terminal) {
                shared.sounds.play(Sfx::Blip);
                self.pending = Some(Room::Archive);
                return Transition::push(showrooms::Archive::new(self.carried, self.player.energy));
            }
        }

        // Cores.
        let player = self.player.body;
        if let Some(index) = self
            .cores
            .iter()
            .position(|c| c.visible && !c.taken && c.body.intersects(&player))
        {
            self.cores[index].taken = true;
            self.carried += 1;
            shared.sounds.play(Sfx::Core);
            let at = self.cores[index].body.center();
            self.burst(at, Rgba::rgb(120, 255, 210), 20);
            if self.carried == 5 {
                self.banner = Timer::once(Duration::from_millis(3000));
            }
        }

        // Drones, through the spatial grid.
        self.grid.clear();
        for (index, drone) in self.drones.iter().enumerate() {
            self.grid.insert(index, drone.body);
        }
        if self.player.hurt.is_finished() && !self.grid.query(&player).is_empty() {
            self.player.energy = self.player.energy.saturating_sub(1);
            self.player.hurt = Timer::once(Duration::from_millis(1500));
            self.camera.shake(3.0, Duration::from_millis(300));
            shared.sounds.play(Sfx::Hit);
            self.burst(center, Rgba::rgb(255, 90, 80), 16);
            if self.player.energy == 0 {
                shared.sounds.music(false);
                shared.store(ctx);
                return Transition::switch(scenes::End::lost(self.carried, self.clock as u32));
            }
        }

        // The relay.
        if self.carried == 5 && self.player.body.intersects(&self.station.relay) {
            let time = self.clock as u32;
            let record = shared.best == 0 || time < shared.best;
            if record {
                shared.best = time;
            }
            shared.sounds.music(false);
            shared.sounds.play(Sfx::Relay);
            shared.store(ctx);
            return Transition::switch(scenes::End::won(time, record));
        }

        // Sparks.
        self.sparks.update(dt);
        self.camera.update(dt, Some(center));
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let started = Instant::now();
        let (area, width, height) = view(ctx, frame);
        if width > 0 {
            let info = FrameInfo {
                alpha: ctx.alpha(),
                fps: ctx.fps(),
                ups: ctx.ups(),
                mode: frame.graphics_mode(),
            };
            self.draw_world(shared, &info, width, height);
            present(frame, area, &self.world, &mut self.big);
        }
        let visuals = shared.visuals.borrow().describe();
        status_line(
            frame,
            &format!(
                " RELAY · {} · cores {}/5 · {} · {} · {} · {:.0} fps",
                self.room.name().to_lowercase(),
                self.carried,
                format_time(self.clock as u32),
                frame.graphics_mode(),
                visuals,
                ctx.fps()
            ),
        );
        let hint = if self.near(self.station.screen) || self.near(self.station.terminal) {
            " E / click: use · "
        } else {
            " "
        };
        help_line(
            frame,
            &format!(
                "{hint}←↑→↓/WASD: move · P: pause · G: display method · F: performance · Q: title"
            ),
        );
        shared
            .stats
            .borrow_mut()
            .record(started.elapsed(), ctx.fps());
    }
}

impl Play {
    #[allow(
        clippy::too_many_lines,
        reason = "the drawing order (lit things, light, glowing things, display) stays in one place"
    )]
    fn draw_world(&mut self, shared: &Shared, info: &FrameInfo, width: u32, height: u32) {
        if (self.world.width(), self.world.height()) != (width, height) {
            self.world = Canvas::new(width, height);
            self.light = Canvas::new(width, height);
        }
        if self.camera.viewport() != (width as f32, height as f32) {
            self.camera.set_viewport(width as f32, height as f32);
        }
        let alpha = info.alpha;
        let offset = self.camera.offset(alpha);
        let screen = |x: f32, y: f32| (x.round() as i32 - offset.0, y.round() as i32 - offset.1);
        let visible = |aabb: Aabb| {
            let (x, y) = screen(aabb.x, aabb.y);
            x < width as i32
                && y < height as i32
                && x + aabb.width as i32 > 0
                && y + aabb.height as i32 > 0
        };

        // The map: one copy of the visible part of the pre-drawn station.
        let world = &mut self.world;
        world.set_blend_mode(BlendMode::Replace);
        world.clear(Rgba::rgb(22, 22, 32));
        world.blit_region(
            &self.map_image,
            PixelRect::new(offset.0, offset.1, width, height),
            offset.0.min(0).abs(),
            offset.1.min(0).abs(),
            BlitOptions::default(),
        );
        world.set_blend_mode(BlendMode::Alpha);

        // Things lit by the lights.
        let station = &self.station;
        for (index, pad) in station.pads.iter().enumerate() {
            let (x, y) = screen(pad.x, pad.y);
            let lit = usize::from(self.puzzle.glow[index] > 0.3);
            self.sprites
                .pad
                .draw(world, lit, x, y, BlitOptions::default());
        }
        let (x, y) = screen(station.relay.x, station.relay.y);
        self.sprites.relay.draw(
            world,
            usize::from(self.carried == 5),
            x,
            y,
            BlitOptions::default(),
        );
        for door in &station.doors {
            let slide = door.slide().round() as i32;
            let (x, y) = screen(door.x, door.y);
            let panel = &self.sprites.door;
            if door.vertical {
                world.blit(panel, x - 4, y - 8 - slide, BlitOptions::default());
                world.blit(panel, x - 4, y + slide, BlitOptions::default());
            } else {
                world.blit(panel, x - 8 - slide, y - 4, BlitOptions::default());
                world.blit(panel, x + slide, y - 4, BlitOptions::default());
            }
        }
        let shine = usize::from((self.time * 3.0).fract() < 0.2);
        for core in self.cores.iter().filter(|c| c.visible && !c.taken) {
            let bob = (self.time * 3.0 + core.body.x).sin() * 1.5;
            let (x, y) = screen(core.body.x, core.body.y + bob);
            self.sprites
                .core
                .draw(world, shine, x, y, BlitOptions::default());
        }
        for drone in &self.drones {
            let dx = ease::lerp(drone.previous.0, drone.body.x, alpha);
            let dy = ease::lerp(drone.previous.1, drone.body.y, alpha);
            let (x, y) = screen(dx - 1.0, dy - 1.0);
            self.sprites
                .drone
                .draw(world, drone.rotor.frame(), x, y, BlitOptions::default());
        }
        let player = &self.player;
        let blink = player.hurt.is_finished() || ((self.time * 12.0) as u32).is_multiple_of(2);
        if blink {
            let px = ease::lerp(player.previous.0, player.body.x, alpha);
            let py = ease::lerp(player.previous.1, player.body.y, alpha);
            let (x, y) = screen(px - 1.0, py - 2.0);
            let frame = if player.moving {
                player.walk.frame()
            } else {
                0
            };
            self.sprites.player.draw(
                world,
                frame,
                x,
                y,
                BlitOptions {
                    flip_x: player.facing_left,
                    ..BlitOptions::default()
                },
            );
        }

        // Light: lights are added into the light layer, which then multiplies the scene.
        let lights = self.light_sources();
        let cache = &mut self.lights;
        let layer = &mut self.light;
        layer.set_blend_mode(BlendMode::Replace);
        layer.clear(AMBIENT);
        layer.set_blend_mode(BlendMode::Add);
        for (lx, ly, radius, color) in lights {
            let (x, y) = screen(lx, ly);
            let r = radius as i32;
            if x + r < 0 || y + r < 0 || x - r > width as i32 || y - r > height as i32 {
                continue;
            }
            // Colors are rounded so pulsing lights reuse a few cached sprites.
            let key = (radius, [color.r & 0xF0, color.g & 0xF0, color.b & 0xF0]);
            let sprite = cache.entry(key).or_insert_with(|| {
                art::light_sprite(radius, Rgba::rgb(key.1[0], key.1[1], key.1[2]))
            });
            layer.blit(sprite, x - r, y - r, BlitOptions::default());
        }
        self.world.set_blend_mode(BlendMode::Multiply);
        self.world.blit(&self.light, 0, 0, BlitOptions::default());
        self.world.set_blend_mode(BlendMode::Alpha);

        // Things that glow on their own, drawn after the light.
        let time = self.time;
        let station = &self.station;
        let world = &mut self.world;
        if visible(station.window) {
            let window = station.window;
            let mut visuals = shared.visuals.borrow_mut();
            let picture = visuals.nebula(
                Slot::Window,
                window.width as u32,
                window.height as u32,
                time,
            );
            let (x, y) = screen(window.x, window.y);
            world.blit(picture, x, y, BlitOptions::default());
            // The window frame and its bars.
            let frame = Rgba::rgb(120, 126, 146);
            world.rect(x, y, window.width as u32, window.height as u32, frame);
            for i in 1..4 {
                let bx = x + (window.width * i as f32 / 4.0) as i32;
                world.fill_rect(bx, y, 1, window.height as u32, frame);
            }
        }
        if visible(station.reactor) {
            let pit = station.reactor;
            let (cx, cy) = screen(pit.center().0, pit.center().1);
            let glow = 0.75 + 0.25 * (time * 2.5).sin();
            world.circle(cx, cy, 15, Rgba::new(255, 140, 60, (glow * 255.0) as u8));
            world.circle(cx, cy, 13, Rgba::new(255, 200, 120, (glow * 160.0) as u8));
            let size = 40;
            let mut visuals = shared.visuals.borrow_mut();
            let core = visuals.core(size, time);
            world.blit(
                core,
                cx - size as i32 / 2,
                cy - size as i32 / 2,
                BlitOptions::default(),
            );
        }
        if visible(station.screen) {
            draw_prism_screen(
                world,
                screen(station.screen.x, station.screen.y),
                station.screen,
                time,
            );
        }
        if visible(station.terminal) {
            draw_terminal(world, screen(station.terminal.x, station.terminal.y), time);
        }
        if visible(station.wave_screen) {
            let (x, y) = screen(station.wave_screen.x, station.wave_screen.y);
            let wave = self
                .puzzle
                .last
                .map(|(pad, at)| (pad, time - at, shared.sounds.pad_wave(pad)));
            draw_wave(world, (x, y), station.wave_screen, wave);
        }
        for (pad, glow) in station.pads.iter().zip(self.puzzle.glow) {
            if glow > 0.05 {
                let (x, y) = screen(pad.x, pad.y);
                world.fill_rect(
                    x + 2,
                    y + 2,
                    3,
                    3,
                    Rgba::new(220, 255, 255, (glow * 255.0) as u8),
                );
            }
        }
        for drone in &self.drones {
            let (x, y) = screen(drone.body.x + 2.0, drone.body.y + 2.0);
            world.fill_rect(x, y, 2, 1, Rgba::rgb(255, 70, 80));
        }
        self.sparks.draw(world, |[x, y, _]| {
            let (x, y) = screen(x, y);
            Some((x as f32, y as f32, 1.0))
        });

        self.draw_hud(shared, info);
    }

    fn draw_hud(&mut self, shared: &Shared, info: &FrameInfo) {
        let world = &mut self.world;
        let (width, height) = (world.width() as i32, world.height() as i32);
        let gold = Rgba::rgb(255, 210, 110);
        shadow_text(world, 3, 3, self.room.name(), gold, 1);
        let time = format_time(self.clock as u32);
        let (time_width, _) = Font::builtin().measure(&time);
        shadow_text(
            world,
            (width - time_width as i32) / 2,
            3,
            &time,
            Rgba::WHITE,
            1,
        );
        // Cores carried, right; energy below them.
        for slot in 0..5 {
            let x = width - 8 * (5 - slot) - 2;
            let frame = usize::from(slot < self.carried as i32);
            if frame == 1 {
                self.sprites
                    .core
                    .draw(world, 0, x, 3, BlitOptions::default());
            } else {
                world.rect(x, 3, 5, 5, Rgba::rgb(80, 90, 100));
            }
        }
        for slot in 0..ENERGY as i32 {
            let full = usize::from(slot < self.player.energy as i32);
            world.blit(
                &self.sprites.energy[full],
                width - 7 * (ENERGY as i32 - slot) - 2,
                11,
                BlitOptions::default(),
            );
        }
        // The room banner, fading out.
        if !self.banner.is_finished() {
            let fade = (1.0 - self.banner.progress()).min(0.35) / 0.35;
            let alpha = (fade * 255.0) as u8;
            let (title, subtitle) = if self.carried == 5 && self.room != Room::Atrium {
                ("ALL CORES FOUND", "BRING THEM TO THE RELAY")
            } else {
                (self.room.name(), self.room.subtitle())
            };
            let font = Font::builtin();
            let scale = if width > 200 { 2 } else { 1 };
            let (tw, th) = font.measure(title);
            let (sw, _) = font.measure(subtitle);
            // Low on the screen: the rooms' screens and windows are on their north walls.
            let y = height * 3 / 5;
            shadow_text(
                world,
                (width - (tw * scale) as i32) / 2,
                y,
                title,
                gold.with_alpha(alpha),
                scale,
            );
            shadow_text(
                world,
                (width - sw as i32) / 2,
                y + (th * scale) as i32 + 3,
                subtitle,
                Rgba::new(220, 230, 255, alpha),
                1,
            );
        }
        if shared.perf {
            draw_perf(world, shared, info);
        }
    }
}

/// The prism screen: color bands sweeping through the spectrum under the word PRISM.
fn draw_prism_screen(world: &mut Canvas, (x, y): (i32, i32), area: Aabb, time: f32) {
    let (w, h) = (area.width as i32, area.height as i32);
    for column in 0..w {
        let hue = (column as f32 / w as f32 + time * 0.15).fract();
        world.fill_rect(x + column, y, 1, h as u32, hue_color(hue, 0.55));
    }
    world.rect(x, y, w as u32, h as u32, Rgba::rgb(200, 200, 220));
    let (tw, _) = Font::builtin().measure("PRISM");
    shadow_text(
        world,
        x + (w - tw as i32) / 2,
        y + 5,
        "PRISM",
        Rgba::WHITE,
        1,
    );
}

/// The archive terminal: scrolling lines of green text and a blinking cursor.
fn draw_terminal(world: &mut Canvas, (x, y): (i32, i32), time: f32) {
    world.fill_rect(x + 1, y + 1, 14, 10, Rgba::rgb(6, 24, 12));
    let scroll = (time * 4.0) as i32;
    for line in 0..4 {
        let length = 3 + ((line + scroll) * 7 % 9);
        world.fill_rect(
            x + 2,
            y + 2 + line * 2,
            length as u32,
            1,
            Rgba::rgb(90, 230, 120),
        );
    }
    if (time * 2.0).fract() < 0.5 {
        world.fill_rect(x + 12, y + 8, 2, 2, Rgba::rgb(160, 255, 170));
    }
}

/// The waveform of the last pad, scrolling with its playhead. Without sound, a sine of the
/// pad's pitch.
fn draw_wave(
    world: &mut Canvas,
    (x, y): (i32, i32),
    area: Aabb,
    wave: Option<(usize, f32, Option<&[f32]>)>,
) {
    let (w, h) = (area.width as i32, area.height as i32);
    world.fill_rect(x, y, w as u32, h as u32, Rgba::rgb(6, 16, 22));
    for gx in (0..w).step_by(8) {
        world.fill_rect(x + gx, y, 1, h as u32, Rgba::rgb(16, 40, 50));
    }
    world.fill_rect(x, y + h / 2, w as u32, 1, Rgba::rgb(20, 60, 70));
    let middle = (y + h / 2) as f32;
    let amplitude = h as f32 / 2.0 - 1.0;
    let mut previous: Option<(f32, f32)> = None;
    let sample = |column: i32| -> f32 {
        match wave {
            Some((_, elapsed, Some(samples))) if !samples.is_empty() => {
                // About 1.5 ms per column, starting at the playhead (44.1 kHz).
                let start = (elapsed.max(0.0) * 44_100.0) as usize;
                samples
                    .get(start + column as usize * 64)
                    .copied()
                    .unwrap_or(0.0)
            }
            Some((pad, elapsed, _)) => {
                let fade = (1.0 - elapsed * 2.0).max(0.0);
                let cycles = 2.0 + pad as f32 * 0.5;
                fade * (column as f32 / w as f32 * cycles * std::f32::consts::TAU).sin()
            }
            None => 0.0,
        }
    };
    for column in 0..w {
        let value = sample(column).clamp(-1.0, 1.0);
        let point = ((x + column) as f32, middle - value * amplitude);
        if let Some(last) = previous {
            world.line_aa(last.0, last.1, point.0, point.1, Rgba::rgb(110, 255, 240));
        }
        previous = Some(point);
    }
    world.rect(x, y, w as u32, h as u32, Rgba::rgb(110, 116, 132));
}

/// The performance panel: frame rate, render time with a graph, and the drawing paths.
fn draw_perf(world: &mut Canvas, shared: &Shared, info: &FrameInfo) {
    let stats = shared.stats.borrow();
    let visuals = shared.visuals.borrow();
    let lines = [
        format!("FPS {:.0}  UPS {:.0}", info.fps, info.ups),
        format!("RENDER {:.2} MS", stats.average_render()),
        format!("DRAW {}", info.mode.name().to_uppercase()),
        format!(
            "PICTURES {}",
            if visuals.using_gpu() { "GPU" } else { "CPU" }
        ),
        format!(
            "SOUND {}",
            if shared.sounds.available() {
                "ON"
            } else {
                "OFF"
            }
        ),
    ];
    let height = world.height() as i32;
    let panel_height = lines.len() as i32 * 9 + 22;
    let (x, y) = (3, height - panel_height - 3);
    world.fill_rect(x, y, 104, panel_height as u32, Rgba::new(8, 10, 18, 200));
    world.rect(x, y, 104, panel_height as u32, Rgba::rgb(90, 110, 140));
    for (i, line) in lines.iter().enumerate() {
        shadow_text(
            world,
            x + 3,
            y + 3 + i as i32 * 9,
            line,
            Rgba::rgb(200, 230, 255),
            1,
        );
    }
    // Render time of the last frames: bars up to 8 ms.
    let base = y + panel_height - 4;
    for (i, ms) in stats.render_ms.iter().enumerate() {
        let bar = ((ms / 8.0).min(1.0) * 14.0).max(1.0) as u32;
        let color = if *ms < 4.0 {
            Rgba::rgb(110, 230, 140)
        } else {
            Rgba::rgb(255, 170, 90)
        };
        world.fill_rect(x + 4 + i as i32 * 2, base - bar as i32, 1, bar, color);
    }
}

/// A fully saturated color for `hue` in 0..1, at brightness `value`.
pub fn hue_color(hue: f32, value: f32) -> Rgba {
    let sector = hue * 6.0;
    let rising = 1.0 - (sector % 2.0 - 1.0).abs();
    let (red, green, blue) = match sector as u32 {
        0 => (1.0, rising, 0.0),
        1 => (rising, 1.0, 0.0),
        2 => (0.0, 1.0, rising),
        3 => (0.0, rising, 1.0),
        4 => (rising, 0.0, 1.0),
        _ => (1.0, 0.0, rising),
    };
    let byte = |level: f32| (level * value * 255.0) as u8;
    Rgba::rgb(byte(red), byte(green), byte(blue))
}

#[cfg(test)]
mod tests {
    use vnce::vnce_game::{Aabb, SaveFile};
    use vnce::vnce_raster::PixelMode;
    use vnce::vnce_term::GraphicsMode;

    use super::{FrameInfo, Play};
    use crate::Shared;

    /// Writes a picture of every room to the folder in `RELAY_SNAPSHOTS` (skipped without
    /// it), with the GPU when `RELAY_GPU` is set. For looking at the art, not a check.
    #[test]
    fn snapshots() {
        let Ok(folder) = std::env::var("RELAY_SNAPSHOTS") else {
            return;
        };
        let shared = Shared::new(SaveFile::new(), std::env::var("RELAY_GPU").is_ok(), false);
        let mut play = Play::new(3);
        let spots = [
            ("atrium", (256.0, 190.0)),
            ("observatory", (270.0, 56.0)),
            ("reactor", (430.0, 180.0)),
            ("prism", (72.0, 44.0)),
            ("soundlab", (256.0, 296.0)),
            ("archive", (72.0, 160.0)),
        ];
        let info = FrameInfo {
            alpha: 1.0,
            fps: 60.0,
            ups: 60.0,
            mode: GraphicsMode::Cells(PixelMode::HalfBlock),
        };
        for (name, (x, y)) in spots {
            play.player.body = Aabb::from_center(x, y, 6.0, 6.0);
            play.player.previous = (play.player.body.x, play.player.body.y);
            play.camera.center_on(x, y);
            play.room = play.station.room_at(x, y);
            play.time += 1.3;
            play.puzzle.glow = [0.0, 1.0, 0.0, 0.6, 0.0, 0.0, 0.3, 0.0];
            play.puzzle.last = Some((1, play.time - 0.05));
            for (size, (width, height)) in [("small", (200, 116)), ("large", (320, 180))] {
                play.camera.set_viewport(width as f32, height as f32);
                play.camera.center_on(x, y);
                play.camera
                    .update(std::time::Duration::from_millis(16), Some((x, y)));
                play.draw_world(&shared, &info, width, height);
                let png = play.world.to_png().expect("a canvas encodes as PNG");
                std::fs::write(format!("{folder}/{name}-{size}.png"), png).expect("writable");
            }
        }
    }
}
