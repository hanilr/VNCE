//! Flying: the craft, the rings of a course or of free flight, crashes and respawns,
//! the three cameras, the time of day, effects, instruments and the text around the
//! picture.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{Emitter, Particle, Rng, Scene, Transition};
use vnce::vnce_raster::{Canvas, Rgba};
use vnce::vnce_runtime::Context;
use vnce::vnce_term::Frame;
use vnce::vnce_term::input::MouseButton;
use vnce::vnce_ui::{Block, BorderType, Clear, Gauge, Line, Paragraph, Widget};

use crate::audio::{Mood, Sfx};
use crate::course::{self, Course, Layout, RING_RADIUS, Ring, start_point};
use crate::flight::{self, Craft, MAX_SPEED, MIN_SPEED, Stick};
use crate::hud;
use crate::math::{
    Camera, Mat4, Vec3, add, add_scaled, cross, direction, dot, flat_distance, lerp, normalize,
    scale, sub,
};
use crate::models::RingKind;
use crate::render::{Model, Object, View};
use crate::scenes::{End, Pause, Title};
use crate::sky::daylight;
use crate::terrain::{ChunkKey, Prop, Terrain, chunk_of, ground_normal};
use crate::{
    ACCENT, Action, COOL, Mode, PANEL, Request, Shared, format_time, help_line, show, status_line,
    view,
};

/// How a flight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The course was finished in this many tenths of a second.
    Finished(u32),
    /// Free flight ran out of time after this many rings.
    TimeUp(u32),
}

/// The three cameras.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraKind {
    /// Behind the craft, leaning into turns.
    Chase,
    /// From the cockpit, banking with the craft.
    Cockpit,
    /// Around the craft, turned with the mouse.
    Orbit,
}

impl CameraKind {
    const ALL: [Self; 3] = [Self::Chase, Self::Cockpit, Self::Orbit];

    fn name(self) -> &'static str {
        match self {
            Self::Chase => "chase",
            Self::Cockpit => "cockpit",
            Self::Orbit => "orbit",
        }
    }
}

/// Seconds of countdown before a flight.
const COUNTDOWN: f32 = 3.0;
/// Seconds between a crash and the respawn.
const CRASH_TIME: f32 = 1.8;
/// Free flight: time at the start, added per ring and per checkpoint, lost per crash.
const FREE_START: f32 = 45.0;
const FREE_RING: f32 = 6.0;
const FREE_CHECKPOINT: f32 = 10.0;
const FREE_CRASH: f32 = 8.0;
/// Free flight keeps this many rings ahead.
const FREE_AHEAD: usize = 3;
/// The trail lasts this long.
const TRAIL_TIME: f32 = 1.3;

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches of a flight"
)]
pub struct Play {
    mode: Mode,
    seed: u64,
    terrain: Terrain,
    /// Course: every ring. Free flight: the rings ahead.
    rings: Vec<Ring>,
    /// Course: the index of the next ring.
    next: usize,
    /// Free flight: where new rings come from, and how many were made.
    layout: Option<Layout>,
    made: usize,
    passed: u32,
    craft: Craft,
    previous: Vec3,
    /// Where to come back after a crash, and the heading.
    respawn: (Vec3, f32),
    /// Seconds until the respawn (0 while flying).
    crashed: f32,
    countdown: f32,
    /// Seconds flown.
    clock: f32,
    /// Free flight: seconds left.
    time_left: f32,
    /// Day progress at the start.
    phase0: f32,
    camera: CameraKind,
    chase_eye: Option<Vec3>,
    /// Orbit camera: yaw, pitch and distance.
    orbit: (f32, f32, f32),
    drag: Option<(u16, u16)>,
    photo: bool,
    /// Flown by the autopilot (title screen and bench).
    autopilot: bool,
    /// The title screen's backdrop: no instruments, no end, no sound effects.
    demo: bool,
    trail: VecDeque<(Vec3, f32)>,
    sparks: Emitter,
    flash: (f32, Rgba),
    message: Option<(String, f32, Color)>,
    props: HashMap<ChunkKey, Vec<Prop>>,
    rng: Rng,
    big: Canvas,
    outcome: Option<Outcome>,
    end_delay: f32,
    warmed: bool,
    /// Seconds since the scene started.
    time: f32,
}

impl Play {
    /// A flight of `mode` on the course of `seed`.
    pub fn new(mode: Mode, seed: u64) -> Self {
        let terrain = Terrain::new(seed);
        let mut rng = Rng::new(seed ^ 0xF1_1E5);
        let (rings, start, yaw, phase0, layout) = match mode {
            Mode::Course => {
                let course = Course::generate(seed, &terrain);
                (course.rings, course.start, course.yaw, course.phase, None)
            }
            Mode::Free => {
                let start = start_point(&terrain, 0.0, 0.0);
                let yaw = rng.unit() * std::f32::consts::TAU;
                (
                    Vec::new(),
                    start,
                    yaw,
                    0.04,
                    Some(Layout::new(seed, start, yaw)),
                )
            }
        };
        let mut play = Self {
            mode,
            seed,
            terrain,
            rings,
            next: 0,
            layout,
            made: 0,
            passed: 0,
            craft: Craft::new(start, yaw),
            previous: start,
            respawn: (start, yaw),
            crashed: 0.0,
            countdown: COUNTDOWN,
            clock: 0.0,
            time_left: FREE_START,
            phase0,
            camera: CameraKind::Chase,
            chase_eye: None,
            orbit: (yaw + 2.4, -0.18, 24.0),
            drag: None,
            photo: false,
            autopilot: false,
            demo: false,
            trail: VecDeque::new(),
            sparks: hud::spark_emitter(),
            flash: (0.0, Rgba::WHITE),
            message: None,
            props: HashMap::new(),
            rng,
            big: Canvas::new(0, 0),
            outcome: None,
            end_delay: 0.0,
            warmed: false,
            time: 0.0,
        };
        play.fill_rings();
        play
    }

    /// The title screen's backdrop: free flight by the autopilot, seen by a slowly
    /// circling camera.
    pub fn demo(seed: u64) -> Self {
        let mut play = Self::new(Mode::Free, seed);
        play.autopilot = true;
        play.demo = true;
        play.countdown = 0.0;
        play.camera = CameraKind::Orbit;
        play
    }

    /// The bench's flight: a course by the autopilot, without a countdown.
    pub fn bench(seed: u64) -> Self {
        let mut play = Self::new(Mode::Course, seed);
        play.autopilot = true;
        play.countdown = 0.0;
        play
    }

    /// Free flight: makes rings until there are enough ahead.
    fn fill_rings(&mut self) {
        let Some(layout) = &mut self.layout else {
            return;
        };
        while self.rings.len() < FREE_AHEAD {
            self.made += 1;
            let kind = if self.made.is_multiple_of(5) {
                RingKind::Checkpoint
            } else {
                RingKind::Normal
            };
            self.rings.push(layout.next(&self.terrain, kind));
        }
    }

    /// Free flight: new rings ahead of the craft (after a crash or a missed ring).
    fn restart_rings(&mut self) {
        if self.mode == Mode::Free {
            let seed = self.seed ^ u64::from(self.passed) ^ (self.made as u64) << 20;
            self.layout = Some(Layout::new(seed, self.craft.position, self.craft.yaw));
            self.rings.clear();
            self.fill_rings();
        }
    }

    /// The ring to fly through next.
    fn target(&self) -> Option<&Ring> {
        match self.mode {
            Mode::Course => self.rings.get(self.next),
            Mode::Free => self.rings.first(),
        }
    }

    /// The day's progress now.
    fn phase(&self) -> f32 {
        if self.demo {
            // Back and forth through the day.
            let t = (0.3 + self.time / 240.0) % 2.0;
            let wave = if t > 1.0 { 2.0 - t } else { t };
            return 0.04 + wave * 0.92;
        }
        match self.mode {
            Mode::Course => (self.phase0 + self.clock / 420.0).min(0.985),
            Mode::Free => (self.phase0 + self.clock / 360.0).min(0.985),
        }
    }

    fn say(&mut self, text: impl Into<String>, seconds: f32, color: Color) {
        self.message = Some((text.into(), seconds, color));
    }

    fn effect(&self, shared: &Shared, sfx: Sfx) {
        if !self.demo {
            shared.sounds.play(sfx);
        }
    }

    fn burst(&mut self, center: Vec3, count: usize, speed: f32, colors: &[Rgba]) {
        for _ in 0..count {
            let dir = normalize([
                self.rng.unit() - 0.5,
                self.rng.unit() - 0.3,
                self.rng.unit() - 0.5,
            ]);
            let color = colors[self.rng.below(colors.len() as u32) as usize];
            self.sparks.spawn(Particle::new(
                center,
                scale(dir, speed * (0.4 + self.rng.unit())),
                0.6 + self.rng.unit() * 0.8,
                color,
            ));
        }
    }

    /// The props near the craft (for collisions), computed once per chunk.
    fn nearby_props(&mut self) -> Vec<Prop> {
        let (cx, cz) = chunk_of(self.craft.position[0], self.craft.position[2]);
        self.props
            .retain(|key, _| (key.0 - cx).abs() <= 2 && (key.1 - cz).abs() <= 2);
        let mut near = Vec::new();
        for dz in -1..=1 {
            for dx in -1..=1 {
                let key = (cx + dx, cz + dz);
                let terrain = &self.terrain;
                let props = self.props.entry(key).or_insert_with(|| terrain.props(key));
                near.extend(props.iter().copied());
            }
        }
        near
    }

    fn crash(&mut self, shared: &Shared) {
        self.effect(shared, Sfx::Crash);
        let at = self.craft.position;
        self.burst(
            at,
            50,
            14.0,
            &[
                Rgba::rgb(255, 170, 60),
                Rgba::rgb(255, 230, 140),
                Rgba::rgb(120, 110, 100),
            ],
        );
        self.crashed = CRASH_TIME;
        self.trail.clear();
        self.flash = (0.8, Rgba::rgb(255, 150, 80));
        match self.mode {
            Mode::Course => self.say(
                "Crash! Back to the last checkpoint",
                2.0,
                Color::rgb(255, 140, 100),
            ),
            Mode::Free => {
                if !self.demo {
                    self.time_left -= FREE_CRASH;
                }
                self.say(
                    format!("Crash! −{FREE_CRASH:.0} s"),
                    2.0,
                    Color::rgb(255, 140, 100),
                );
                let start = start_point(&self.terrain, at[0], at[2]);
                self.respawn = (start, self.craft.yaw);
            }
        }
    }

    fn respawn_now(&mut self) {
        let (position, yaw) = self.respawn;
        self.craft = Craft::new(position, yaw);
        self.previous = position;
        self.chase_eye = None;
        self.restart_rings();
    }

    fn pass_ring(&mut self, shared: &Shared, ring: Ring) {
        self.passed += 1;
        self.craft.boost = flight::BOOST_TIME;
        let color = ring.kind.color();
        self.flash = (0.5, color);
        // Sparks all around the ring.
        for i in 0..28 {
            let a = i as f32 * std::f32::consts::TAU / 28.0;
            let right = normalize(cross(ring.normal, [0.0, 1.0, 0.0]));
            let up = cross(right, ring.normal);
            let point = add(
                ring.center,
                add(
                    scale(right, a.cos() * RING_RADIUS),
                    scale(up, a.sin() * RING_RADIUS),
                ),
            );
            let out = normalize(sub(point, ring.center));
            self.sparks.spawn(Particle::new(
                point,
                add(scale(out, 6.0), scale(ring.normal, 8.0)),
                0.9,
                color,
            ));
        }
        match (self.mode, ring.kind) {
            (Mode::Course, RingKind::Finish) => {
                self.effect(shared, Sfx::Finish);
                let tenths = (self.clock * 10.0).round() as u32;
                self.say(format!("Finish! {}", format_time(tenths)), 3.0, ACCENT);
                self.outcome = Some(Outcome::Finished(tenths));
                self.end_delay = 2.5;
            }
            (Mode::Course, RingKind::Checkpoint) => {
                self.effect(shared, Sfx::Checkpoint);
                self.respawn = (ring.center, ring.yaw());
                self.say(
                    format!("Checkpoint · {}", format_time((self.clock * 10.0) as u32)),
                    2.0,
                    COOL,
                );
            }
            (Mode::Free, kind) => {
                let bonus = if kind == RingKind::Checkpoint {
                    FREE_CHECKPOINT
                } else {
                    FREE_RING
                };
                self.time_left += bonus;
                self.effect(
                    shared,
                    if kind == RingKind::Checkpoint {
                        Sfx::Checkpoint
                    } else {
                        Sfx::Ring
                    },
                );
                self.say(format!("+{bonus:.0} s"), 1.2, Color::rgb(255, 220, 120));
            }
            _ => self.effect(shared, Sfx::Ring),
        }
    }

    /// One step of the world by `dt` seconds with the pilot's `stick`. Returns how the
    /// flight ended, once the end has been shown for a moment.
    fn advance(&mut self, shared: &Shared, stick: Stick, dt: f32) -> Option<Outcome> {
        self.time += dt;
        self.update_effects(dt);
        self.update_camera(dt);
        if self.photo {
            return None;
        }
        if self.countdown > 0.0 {
            let before = self.countdown.ceil();
            self.countdown -= dt;
            if self.countdown <= 0.0 {
                self.say("GO!", 1.0, Color::rgb(140, 255, 160));
                self.effect(shared, Sfx::Ring);
            } else if self.countdown.ceil() < before || self.message.is_none() {
                self.say(format!("{}", self.countdown.ceil()), 1.0, ACCENT);
            }
            return None;
        }
        if self.crashed > 0.0 {
            self.crashed -= dt;
            if self.crashed <= 0.0 {
                self.respawn_now();
            }
            return None;
        }
        self.fly(shared, stick, dt);
        if let Some(outcome) = self.outcome {
            self.end_delay -= dt;
            if self.end_delay <= 0.0 {
                return Some(outcome);
            }
        }
        None
    }

    /// Sparks, the flash, the message and the trail age.
    fn update_effects(&mut self, dt: f32) {
        self.sparks.update(Duration::from_secs_f32(dt.max(0.0)));
        self.flash.0 = (self.flash.0 - dt * 2.5).max(0.0);
        if let Some((_, left, _)) = &mut self.message {
            *left -= dt;
            if *left <= 0.0 {
                self.message = None;
            }
        }
        for point in &mut self.trail {
            point.1 += dt;
        }
        while self.trail.front().is_some_and(|p| p.1 > TRAIL_TIME) {
            self.trail.pop_front();
        }
    }

    /// Flies one step: the craft, the rings, collisions and free flight's clock.
    fn fly(&mut self, shared: &Shared, stick: Stick, dt: f32) {
        let stick = if self.autopilot || self.outcome.is_some() {
            let target = self.target().map_or_else(
                || add_scaled(self.craft.position, direction(self.craft.yaw, 0.0), 300.0),
                |ring| ring.center,
            );
            flight::autopilot(&self.craft, target, &self.terrain)
        } else {
            stick
        };
        self.previous = self.craft.position;
        self.craft.update(stick, dt);
        if self.outcome.is_none() {
            self.clock += dt;
        }
        self.trail.push_back((self.craft.exhaust(), 0.0));
        if !self.demo {
            let range = MAX_SPEED - MIN_SPEED;
            shared.sounds.flight((self.craft.speed - MIN_SPEED) / range);
        }

        // Rings.
        if let Some(ring) = self.target().copied() {
            if ring.passed(self.previous, self.craft.position) {
                match self.mode {
                    Mode::Course => self.next += 1,
                    Mode::Free => {
                        self.rings.remove(0);
                        self.fill_rings();
                    }
                }
                self.pass_ring(shared, ring);
            } else if self.mode == Mode::Free
                && dot(sub(self.craft.position, ring.center), ring.normal) > 220.0
            {
                // Missed and far past it: new rings ahead.
                self.restart_rings();
            }
        }

        // The ground, the water and the props.
        let position = self.craft.position;
        let ground = self.terrain.surface(position[0], position[2]);
        let mut hit = position[1] < ground + 1.0;
        if !hit {
            hit = self.nearby_props().iter().any(|prop| {
                let (radius, height) = prop.size();
                flat_distance(prop.position, position) < radius + 1.5
                    && position[1] < prop.position[1] + height
            });
        }
        if hit && self.outcome.is_none() {
            self.crash(shared);
        } else if hit {
            // After the finish the autopilot flies on; a crash only ends the show sooner.
            self.end_delay = 0.0;
        }

        // Free flight's clock.
        if self.mode == Mode::Free && !self.demo && self.outcome.is_none() {
            self.time_left -= dt;
            if self.time_left <= 0.0 {
                self.time_left = 0.0;
                self.say("Time!", 2.0, ACCENT);
                self.outcome = Some(Outcome::TimeUp(self.passed));
                self.end_delay = 2.0;
            }
        }
    }

    /// Where the chase camera wants to be.
    fn chase_target(&self) -> Vec3 {
        let back = direction(self.craft.yaw, self.craft.pitch * 0.5);
        add(
            add_scaled(self.craft.position, back, -15.0),
            [0.0, 4.0, 0.0],
        )
    }

    fn update_camera(&mut self, dt: f32) {
        if self.demo {
            self.orbit.0 += dt * 0.12;
        }
        let target = self.chase_target();
        let eye = self
            .chase_eye
            .map_or(target, |eye| lerp(eye, target, (dt * 5.0).min(1.0)));
        let ground = self.terrain.surface(eye[0], eye[2]);
        self.chase_eye = Some([eye[0], eye[1].max(ground + 2.0), eye[2]]);
    }

    /// The camera for a picture of `aspect` (width / height).
    fn camera(&self, aspect: f32) -> Camera {
        let craft = &self.craft;
        match self.camera {
            CameraKind::Chase => {
                let eye = self.chase_eye.unwrap_or_else(|| self.chase_target());
                let look = add_scaled(craft.position, craft.forward(), 12.0);
                let up = normalize(lerp([0.0, 1.0, 0.0], craft.up(), 0.35));
                crate::math::camera(eye, sub(look, eye), up, aspect)
            }
            CameraKind::Cockpit => crate::math::camera(
                craft.model().point([0.0, 0.85, -1.4]),
                craft.forward(),
                craft.up(),
                aspect,
            ),
            CameraKind::Orbit => {
                let (yaw, pitch, distance) = self.orbit;
                let forward = direction(yaw, pitch);
                let mut eye = add_scaled(craft.position, forward, -distance);
                eye[1] = eye[1].max(self.terrain.surface(eye[0], eye[2]) + 2.0);
                crate::math::camera(eye, sub(craft.position, eye), [0.0, 1.0, 0.0], aspect)
            }
        }
    }

    /// Everything to draw besides the terrain.
    fn objects(&self) -> Vec<Object> {
        let mut objects = Vec::new();
        let craft = &self.craft;
        if self.crashed <= 0.0 {
            if self.camera != CameraKind::Cockpit {
                objects.push(Object {
                    model: Model::Craft,
                    transform: craft.model(),
                });
            }
            // A soft shadow on the ground under the craft, larger higher up.
            let (x, z) = (craft.position[0], craft.position[2]);
            let ground = self.terrain.surface(x, z);
            let height = craft.position[1] - ground;
            if height < 150.0 {
                let up = ground_normal(&self.terrain, x, z);
                let back = normalize(cross([1.0, 0.0, 0.0], up));
                let right = cross(up, back);
                objects.push(Object {
                    model: Model::Shadow,
                    transform: Mat4::translation([x, ground + 0.35, z])
                        * Mat4::basis(right, up, back)
                        * Mat4::scale([4.0 + height * 0.03; 3]),
                });
            }
        }
        let first = match self.mode {
            Mode::Course => self.next,
            Mode::Free => 0,
        };
        for (index, ring) in self.rings.iter().enumerate().skip(first) {
            let x = normalize(cross([0.0, 1.0, 0.0], ring.normal));
            let y = cross(ring.normal, x);
            objects.push(Object {
                model: Model::Ring(ring.kind, index == first && !self.demo),
                transform: Mat4::translation(ring.center)
                    * Mat4::basis(x, y, ring.normal)
                    * Mat4::scale([RING_RADIUS; 3]),
            });
        }
        objects
    }

    /// Renders the world and the effects and instruments into the renderer's canvas.
    #[cfg(test)]
    fn draw_picture(&mut self, shared: &Shared, width: u32, height: u32, aspect: f32) {
        let view = self.world_view(shared, aspect);
        let mut renderer = shared.renderer.borrow_mut();
        let shown = renderer.render(&self.terrain, &view, width, height);
        record_time(shared, renderer.stats().milliseconds);
        self.draw_overlays(renderer.canvas(), &shown);
    }

    /// The camera, light and objects of a picture of `aspect`; builds the chunks around
    /// the camera before the first one.
    fn world_view(&mut self, shared: &Shared, aspect: f32) -> View {
        let camera = self.camera(aspect);
        if !self.warmed {
            shared.renderer.borrow_mut().warm_up(&self.terrain, &camera);
            self.warmed = true;
        }
        View {
            camera,
            light: daylight(self.phase()),
            objects: self.objects(),
            time: self.time,
        }
    }

    /// Shows the world in `area`. In a native window that draws with the GPU, the sky and
    /// the meshes stay on the GPU as two textures and only the instruments are drawn on
    /// the CPU, on a transparent picture over them; elsewhere the world is read back,
    /// composed and drawn as one picture.
    fn show_world(&mut self, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        let (_, width, height, aspect) = view(shared, frame);
        if width == 0 || height == 0 {
            return;
        }
        let world = self.world_view(shared, aspect);
        let mut renderer = shared.renderer.borrow_mut();
        if frame.accepts_textures() {
            if let Some(layers) = renderer.render_layers(&self.terrain, &world, width, height) {
                record_time(shared, renderer.stats().milliseconds);
                for layer in layers {
                    frame.draw_texture(area, layer);
                }
                let mut instruments = std::mem::replace(&mut self.big, Canvas::new(0, 0));
                if (instruments.width(), instruments.height()) == (width, height) {
                    instruments.clear(Rgba::TRANSPARENT);
                } else {
                    instruments = Canvas::new(width, height);
                }
                self.draw_overlays(&mut instruments, &world.camera);
                frame.draw_image(area, &instruments);
                self.big = instruments;
                return;
            }
        }
        let shown = renderer.render(&self.terrain, &world, width, height);
        record_time(shared, renderer.stats().milliseconds);
        self.draw_overlays(renderer.canvas(), &shown);
        show(shared, frame, area, renderer.canvas(), &mut self.big);
    }

    /// The trail, sparks, speed lines and flash, and (when flying) the instruments, drawn
    /// with the camera of the picture they go over.
    fn draw_overlays(&self, canvas: &mut Canvas, shown: &Camera) {
        let shown = *shown;
        let points: Vec<(Vec3, f32)> = self.trail.iter().copied().collect();
        hud::trail(canvas, &shown, &points, TRAIL_TIME);
        hud::sparks(canvas, &shown, &self.sparks);
        let fast = ((self.craft.speed - 72.0) / 35.0).clamp(0.0, 1.0);
        let boost = if self.craft.boost > 0.0 { 0.7 } else { 0.0 };
        if self.crashed <= 0.0 && !self.photo {
            hud::speed_lines(canvas, fast.max(boost), self.time);
        }
        hud::flash(canvas, self.flash.0, self.flash.1);
        if self.demo || self.photo {
            return;
        }
        let target = self.target().copied();
        let marks: Vec<(f32, Rgba)> = target
            .map(|ring| {
                let to = sub(ring.center, self.craft.position);
                (to[0].atan2(-to[2]), ring.kind.color())
            })
            .into_iter()
            .collect();
        hud::compass(canvas, self.craft.yaw, &marks);
        let first = match self.mode {
            Mode::Course => self.next,
            Mode::Free => 0,
        };
        let blips: Vec<(Vec3, Rgba, bool)> = self
            .rings
            .iter()
            .enumerate()
            .skip(first)
            .map(|(i, ring)| (ring.center, ring.kind.color(), i == first))
            .collect();
        hud::radar(
            canvas,
            self.craft.position,
            self.craft.yaw,
            &blips,
            900.0,
            self.time,
        );
        if let Some(ring) = target {
            hud::arrow(canvas, &shown, ring.center, ring.kind.color());
        }
        if self.camera == CameraKind::Cockpit {
            hud::sight(canvas, self.craft.roll);
        }
    }

    /// Flies `seconds` at 60 steps per second (for snapshots).
    #[cfg(test)]
    pub fn fly_for(&mut self, seconds: f32, shared: &Shared) {
        for _ in 0..(seconds * 60.0) as u32 {
            self.advance(shared, Stick::default(), 1.0 / 60.0);
        }
    }

    #[cfg(test)]
    pub fn set_camera(&mut self, index: usize) {
        self.camera = CameraKind::ALL[index % CameraKind::ALL.len()];
    }

    /// A picture of `width` × `height` as PNG (rendered twice: the GPU's is a frame late).
    #[cfg(test)]
    pub fn snapshot(&mut self, shared: &Shared, width: u32, height: u32) -> Vec<u8> {
        let aspect = width as f32 / height as f32;
        self.draw_picture(shared, width, height, aspect);
        self.draw_picture(shared, width, height, aspect);
        shared
            .renderer
            .borrow_mut()
            .canvas()
            .to_png()
            .expect("a canvas encodes as PNG")
    }

    /// The title screen's backdrop: one step and a picture.
    pub fn backdrop(&mut self, shared: &Shared, frame: &mut Frame<'_>) {
        let (area, _, _, _) = view(shared, frame);
        self.show_world(shared, frame, area);
    }

    pub fn step_backdrop(&mut self, shared: &Shared, dt: f32) {
        self.advance(shared, Stick::default(), dt);
    }

    /// Turns the orbit camera by dragging with the mouse and zooms with the wheel.
    fn handle_mouse(&mut self, ctx: &Context) {
        let input = ctx.input();
        let position = input.mouse_position();
        if input.is_mouse_down(MouseButton::Left) {
            if let (Some((x0, y0)), Some(p)) = (self.drag, position) {
                let dx = f32::from(p.x) - f32::from(x0);
                let dy = f32::from(p.y) - f32::from(y0);
                if dx != 0.0 || dy != 0.0 {
                    self.camera = CameraKind::Orbit;
                }
                self.orbit.0 += dx * 0.05;
                self.orbit.1 = (self.orbit.1 - dy * 0.08).clamp(-1.3, 1.3);
            }
            self.drag = position.map(|p| (p.x, p.y));
        } else {
            self.drag = None;
        }
        let (_, wheel) = input.scroll();
        if wheel != 0 {
            self.orbit.2 = (self.orbit.2 * 1.15_f32.powi(wheel)).clamp(8.0, 90.0);
        }
    }

    fn draw_text(&self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let size = ctx.size();
        let renderer = shared.renderer.borrow();
        let code = course::code(self.seed);
        let progress = match self.mode {
            Mode::Course => format!(
                "Course {code} · {} · ring {}/{}",
                format_time((self.clock * 10.0) as u32),
                self.next.min(self.rings.len()),
                self.rings.len()
            ),
            Mode::Free => format!(
                "Free flight {code} · {:.1} s left · {} rings",
                self.time_left, self.passed
            ),
        };
        status_line(
            frame,
            &format!(
                " Glide · {progress} · {} camera · {} · {:.0} fps",
                self.camera.name(),
                renderer.describe(),
                ctx.fps()
            ),
        );
        // Gauges: speed, altitude, throttle.
        let y = size.height.saturating_sub(2);
        let third = size.width / 3;
        let speed = self.craft.speed;
        let altitude = self.craft.position[1]
            - self
                .terrain
                .surface(self.craft.position[0], self.craft.position[2]);
        let gauges = [
            (
                f64::from((speed - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)),
                format!("speed {:.0} km/h", speed * 3.6),
                Color::rgb(255, 190, 90),
            ),
            (
                f64::from(altitude / 300.0),
                format!("altitude {altitude:.0} m"),
                Color::rgb(120, 200, 255),
            ),
            (
                f64::from(self.craft.throttle),
                format!(
                    "throttle {:.0}%{}",
                    self.craft.throttle * 100.0,
                    if self.craft.boost > 0.0 {
                        " · BOOST"
                    } else {
                        ""
                    }
                ),
                Color::rgb(150, 240, 160),
            ),
        ];
        let buffer = frame.buffer_mut();
        for (i, (ratio, label, color)) in gauges.into_iter().enumerate() {
            let area = Rect::new(third * i as u16, y, third.saturating_sub(1), 1);
            Gauge::new()
                .ratio(ratio.clamp(0.0, 1.0))
                .label(label)
                .gauge_style(Style::new().fg(color).bg(PANEL))
                .render(area, buffer);
        }
        let help = if self.photo {
            " Photo: drag / ←→↑↓: turn · wheel: zoom · O: fly on"
        } else {
            " ↑↓: climb/dive · ←→: bank · E/Q: throttle · C: camera · drag: orbit · O: photo · P: pause"
        };
        help_line(frame, help);
        if let Some((text, _, color)) = &self.message {
            let width = text.chars().count() as u16 + 4;
            let area = Rect::new(
                size.width.saturating_sub(width) / 2,
                1 + size.height / 4,
                width.min(size.width),
                1,
            );
            let buffer = frame.buffer_mut();
            let style = Style::new().fg(*color).bg(PANEL).bold();
            buffer.set_style(area, style);
            buffer.set_string(area.x + 2, area.y, text, style);
        }
        if shared.perf {
            draw_perf(shared, ctx, frame);
        }
    }
}

/// The performance panel in the top right corner.
fn draw_perf(shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
    let size = ctx.size();
    let renderer = shared.renderer.borrow();
    let stats = renderer.stats();
    let lines = vec![
        Line::from(renderer.describe()),
        Line::from(format!("{}", frame.graphics_mode())),
        Line::from(format!(
            "picture {}×{} ({}%)",
            renderer.size().0,
            renderer.size().1,
            shared.resolution
        )),
        Line::from(format!("msaa {}×", renderer.msaa())),
        Line::from(format!("{} chunks", stats.chunks)),
        Line::from(format!("{} triangles", stats.triangles)),
        Line::from(format!("render {:.1} ms", stats.milliseconds)),
        Line::from(format!("{:.0} fps", ctx.fps())),
    ];
    let width = 30.min(size.width);
    let area = Rect::new(size.width - width, 1, width, lines.len() as u16 + 2);
    let buffer = frame.buffer_mut();
    Clear.render(area, buffer);
    Paragraph::new(lines)
        .style(Style::new().bg(PANEL))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(COOL))
                .title(" performance "),
        )
        .render(area, buffer);
}

/// Adds a world picture's drawing time to the bench's sum.
fn record_time(shared: &Shared, milliseconds: f32) {
    let (sum, frames) = shared.render_time.get();
    shared
        .render_time
        .set((sum + f64::from(milliseconds), frames + 1));
}

impl Scene<Shared> for Play {
    fn start(&mut self, shared: &mut Shared, ctx: &mut Context) {
        shared.detect_modes(ctx);
        shared.last = self.seed;
        shared.mode = self.mode;
        shared.sounds.music(Mood::Flight);
        shared.sounds.engine(true);
    }

    fn resume(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.engine(true);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        match shared.request.take() {
            Some(Request::Play(mode, seed)) => return Transition::switch(Play::new(mode, seed)),
            Some(Request::Title) => {
                shared.sounds.engine(false);
                return Transition::switch(Title::new(shared));
            }
            None => {}
        }
        let input = ctx.input();
        let controls = &shared.controls;
        if controls.was_pressed(Action::Pause, input) {
            shared.sounds.engine(false);
            return Transition::push(Pause::new());
        }
        if controls.was_pressed(Action::Method, input) {
            shared.cycle_method(ctx);
        }
        if controls.was_pressed(Action::Perf, input) {
            shared.perf = !shared.perf;
        }
        if controls.was_pressed(Action::Camera, input) {
            let index = CameraKind::ALL
                .iter()
                .position(|&c| c == self.camera)
                .unwrap_or(0);
            self.camera = CameraKind::ALL[(index + 1) % CameraKind::ALL.len()];
            shared.sounds.play(Sfx::Camera);
        }
        if controls.was_pressed(Action::Photo, input) {
            self.photo = !self.photo;
            if self.photo {
                self.camera = CameraKind::Orbit;
            }
            shared.sounds.play(Sfx::Camera);
        }
        let mut stick = Stick {
            pitch: controls.axis(Action::Down, Action::Up, input),
            turn: controls.axis(Action::Left, Action::Right, input),
            throttle: controls.axis(Action::Slower, Action::Faster, input),
        };
        let dt = ctx.dt_secs();
        if self.photo {
            // The keys turn the frozen camera.
            self.orbit.0 += stick.turn * dt * 1.2;
            self.orbit.1 = (self.orbit.1 + stick.pitch * dt).clamp(-1.3, 1.3);
            stick = Stick::default();
        }
        self.handle_mouse(ctx);
        if let Some(outcome) = self.advance(shared, stick, dt) {
            shared.sounds.engine(false);
            let record = match outcome {
                Outcome::Finished(tenths) => {
                    let best = shared.best_time(self.seed);
                    let record = best == 0 || tenths < best;
                    if record {
                        shared.set_best_time(self.seed, tenths);
                    }
                    record
                }
                Outcome::TimeUp(rings) => {
                    let record = rings > shared.best_free;
                    if record {
                        shared.best_free = rings;
                    }
                    record
                }
            };
            shared.store(ctx);
            return Transition::switch(End::new(outcome, self.mode, self.seed, record));
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let (area, _, _, _) = view(shared, frame);
        self.show_world(shared, frame, area);
        self.draw_text(shared, ctx, frame);
    }
}
