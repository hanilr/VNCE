//! Particles: sparks, debris, smoke, rain and trails. An [`Emitter`] spawns particles in
//! bursts or at a steady rate, moves them with gravity and drag, ages and removes them, and
//! draws them on a [`Canvas`] through a projection of your choice (2D, isometric or a 3D
//! camera), or hands them out to draw yourself.
//!
//! Positions and velocities are 3D ([`Vec3`]); a 2D game leaves z at zero, an isometric
//! one uses it for height.

use std::time::Duration;

use vnce_core::math::{Vec3, add, add_scaled, cross, normalize, scale};
use vnce_raster::{BlendMode, Canvas, Rgba};

use crate::Rng;

/// One particle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    /// Position.
    pub position: Vec3,
    /// Velocity in units per second.
    pub velocity: Vec3,
    /// Seconds since it was spawned.
    pub age: f32,
    /// Seconds it lives in all.
    pub life: f32,
    /// Its color at birth (see [`Emitter::end_color`] for a change over its life).
    pub color: Rgba,
}

impl Particle {
    /// A particle at `position` moving with `velocity`, living `life` seconds.
    #[must_use]
    pub fn new(position: Vec3, velocity: Vec3, life: f32, color: Rgba) -> Self {
        Self {
            position,
            velocity,
            age: 0.0,
            life,
            color,
        }
    }

    /// How far through its life it is, from `0.0` (just born) to `1.0` (gone).
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.life <= 0.0 {
            1.0
        } else {
            (self.age / self.life).clamp(0.0, 1.0)
        }
    }

    /// Seconds left to live.
    #[must_use]
    pub fn remaining(&self) -> f32 {
        (self.life - self.age).max(0.0)
    }
}

/// The directions new particles fly off in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spread {
    /// Every direction in the x-y plane (2D bursts, or the ground of an isometric world).
    Circle,
    /// Directions in the x-y plane within `width` radians around `angle` (radians from +x
    /// towards +y): a fountain, a jet, a spray.
    Arc {
        /// The middle direction.
        angle: f32,
        /// The whole width of the arc.
        width: f32,
    },
    /// Every direction in 3D.
    Sphere,
    /// 3D directions within `angle` radians of `direction`.
    Cone {
        /// The middle direction (need not be normalized).
        direction: Vec3,
        /// The largest angle from it.
        angle: f32,
    },
}

impl Spread {
    /// A random unit direction of this spread.
    fn sample(self, rng: &mut Rng) -> Vec3 {
        match self {
            Self::Circle => {
                let (sin, cos) = (rng.unit() * std::f32::consts::TAU).sin_cos();
                [cos, sin, 0.0]
            }
            Self::Arc { angle, width } => {
                let (sin, cos) = (angle + (rng.unit() - 0.5) * width).sin_cos();
                [cos, sin, 0.0]
            }
            Self::Sphere => {
                let z = rng.unit() * 2.0 - 1.0;
                let (sin, cos) = (rng.unit() * std::f32::consts::TAU).sin_cos();
                let r = (1.0 - z * z).max(0.0).sqrt();
                [r * cos, r * sin, z]
            }
            Self::Cone { direction, angle } => {
                let axis = normalize(direction);
                // Any vector not parallel to the axis gives the two sides.
                let other = if axis[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let side = normalize(cross(axis, other));
                let up = cross(side, axis);
                let cos_t = 1.0 - rng.unit() * (1.0 - angle.cos());
                let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
                let (sin_p, cos_p) = (rng.unit() * std::f32::consts::TAU).sin_cos();
                add(
                    scale(axis, cos_t),
                    add(scale(side, sin_t * cos_p), scale(up, sin_t * sin_p)),
                )
            }
        }
    }
}

/// Spawns, moves, ages and draws particles. Set it up once with the builder methods, then
/// call [`Emitter::burst`] (or [`Emitter::emit`] for a steady stream) and
/// [`Emitter::update`] every frame.
///
/// ```
/// use std::time::Duration;
/// use vnce_game::Rng;
/// use vnce_game::particles::{Emitter, Spread};
/// use vnce_raster::{Canvas, Rgba};
///
/// let mut sparks = Emitter::new()
///     .spread(Spread::Circle)
///     .speed(20.0, 70.0)
///     .life(0.4, 0.9)
///     .drag(4.8)
///     .colors([Rgba::rgb(120, 255, 210)]);
/// let mut rng = Rng::new(7);
/// sparks.burst(&mut rng, [40.0, 20.0, 0.0], 28);
/// sparks.update(Duration::from_millis(100));
///
/// let mut canvas = Canvas::new(80, 40);
/// // 2D: x and y are pixels already.
/// sparks.draw(&mut canvas, |[x, y, _]| Some((x, y, 1.0)));
/// assert_eq!(sparks.len(), 28);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    particles: Vec<Particle>,
    /// Particles owed by [`Emitter::emit`] (fractions carried to the next frame).
    owed: f32,
    rate: f32,
    life: (f32, f32),
    speed: (f32, f32),
    spread: Spread,
    velocity: Vec3,
    jitter: Vec3,
    gravity: Vec3,
    drag: f32,
    colors: Vec<Rgba>,
    end_color: Option<Rgba>,
    fade: f32,
    size: (f32, f32),
    max: usize,
    blend: BlendMode,
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter {
    /// An emitter with white particles living 1 s, flying off in every direction of the
    /// x-y plane at 1 to 2 units per second, without gravity or drag, fading out over
    /// their last 0.5 s, one pixel big, added as light, at most 10 000 at a time.
    #[must_use]
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
            owed: 0.0,
            rate: 10.0,
            life: (1.0, 1.0),
            speed: (1.0, 2.0),
            spread: Spread::Circle,
            velocity: [0.0; 3],
            jitter: [0.0; 3],
            gravity: [0.0; 3],
            drag: 0.0,
            colors: vec![Rgba::WHITE],
            end_color: None,
            fade: 0.5,
            size: (1.0, 1.0),
            max: 10_000,
            blend: BlendMode::Add,
        }
    }

    /// Particles per second spawned by [`Emitter::emit`].
    #[must_use]
    pub fn rate(mut self, per_second: f32) -> Self {
        self.rate = per_second.max(0.0);
        self
    }

    /// Life in seconds: random between `min` and `max`.
    #[must_use]
    pub fn life(mut self, min: f32, max: f32) -> Self {
        self.life = (min, max);
        self
    }

    /// Speed in units per second: random between `min` and `max`.
    #[must_use]
    pub fn speed(mut self, min: f32, max: f32) -> Self {
        self.speed = (min, max);
        self
    }

    /// The directions particles fly off in.
    #[must_use]
    pub fn spread(mut self, spread: Spread) -> Self {
        self.spread = spread;
        self
    }

    /// A velocity added to every new particle (wind, an upward kick, the speed of a moving
    /// emitter).
    #[must_use]
    pub fn velocity(mut self, velocity: Vec3) -> Self {
        self.velocity = velocity;
        self
    }

    /// A random extra velocity of up to `±jitter` on each axis.
    #[must_use]
    pub fn jitter(mut self, jitter: Vec3) -> Self {
        self.jitter = jitter;
        self
    }

    /// Acceleration in units per second², for example `[0.0, 9.0, 0.0]` to fall down a
    /// screen whose y grows downwards.
    #[must_use]
    pub fn gravity(mut self, gravity: Vec3) -> Self {
        self.gravity = gravity;
        self
    }

    /// How quickly particles slow down: the fraction of their speed lost per second
    /// (`0.0` = never; `5.0` = most of it within a fraction of a second).
    #[must_use]
    pub fn drag(mut self, per_second: f32) -> Self {
        self.drag = per_second.max(0.0);
        self
    }

    /// The colors of new particles: each picks one at random. An empty list keeps the
    /// colors as they are.
    #[must_use]
    pub fn colors(mut self, colors: impl IntoIterator<Item = Rgba>) -> Self {
        self.set_colors(colors);
        self
    }

    /// Changes the colors of particles spawned from now on (for bursts of different
    /// colors from one emitter); see [`Emitter::colors`].
    pub fn set_colors(&mut self, colors: impl IntoIterator<Item = Rgba>) {
        let colors: Vec<Rgba> = colors.into_iter().collect();
        if !colors.is_empty() {
            self.colors = colors;
        }
    }

    /// The color particles turn into over their life (from their own color at birth).
    #[must_use]
    pub fn end_color(mut self, color: Rgba) -> Self {
        self.end_color = Some(color);
        self
    }

    /// Seconds at the end of a particle's life over which it fades out (`0.0`: no fading).
    #[must_use]
    pub fn fade(mut self, seconds: f32) -> Self {
        self.fade = seconds.max(0.0);
        self
    }

    /// Size in pixels at birth and at the end of life.
    #[must_use]
    pub fn size(mut self, start: f32, end: f32) -> Self {
        self.size = (start, end);
        self
    }

    /// The most particles alive at a time; new ones beyond it are not spawned.
    #[must_use]
    pub fn max(mut self, max: usize) -> Self {
        self.max = max;
        self
    }

    /// How particles are drawn: [`BlendMode::Add`] (glowing light, the default) or
    /// [`BlendMode::Alpha`] (smoke, dust, rain).
    #[must_use]
    pub fn blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }

    /// Spawns `count` particles at `at`.
    pub fn burst(&mut self, rng: &mut Rng, at: Vec3, count: usize) {
        for _ in 0..count {
            if self.particles.len() >= self.max {
                break;
            }
            let particle = self.new_particle(rng, at);
            self.particles.push(particle);
        }
    }

    /// Spawns particles at `at` for a steady stream of [`Emitter::rate`] per second over
    /// `dt` (call it every frame while the stream runs).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small, non-negative whole count"
    )]
    pub fn emit(&mut self, rng: &mut Rng, at: Vec3, dt: Duration) {
        self.owed += self.rate * dt.as_secs_f32();
        let count = self.owed.floor();
        self.owed -= count;
        self.burst(rng, at, count as usize);
    }

    /// Adds a particle made by hand (for shapes the settings do not cover, such as a ring).
    pub fn spawn(&mut self, particle: Particle) {
        if self.particles.len() < self.max {
            self.particles.push(particle);
        }
    }

    fn new_particle(&self, rng: &mut Rng, at: Vec3) -> Particle {
        let between = |rng: &mut Rng, (min, max): (f32, f32)| min + (max - min) * rng.unit();
        let direction = self.spread.sample(rng);
        let speed = between(rng, self.speed);
        // Axes without jitter take no random number, so adding jitter to one axis keeps
        // the others' sequence.
        let jitter = self.jitter.map(|j| {
            if j == 0.0 {
                0.0
            } else {
                (rng.unit() * 2.0 - 1.0) * j
            }
        });
        let velocity = add(add(scale(direction, speed), self.velocity), jitter);
        let life = between(rng, self.life);
        let color = if self.colors.len() == 1 {
            self.colors[0]
        } else {
            *rng.pick(&self.colors).unwrap_or(&Rgba::WHITE)
        };
        Particle::new(at, velocity, life, color)
    }

    /// Moves and ages every particle by `dt` and removes those whose life is over.
    pub fn update(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32();
        let keep = (1.0 - self.drag * dt).max(0.0);
        for p in &mut self.particles {
            p.velocity = scale(add_scaled(p.velocity, self.gravity, dt), keep);
            p.position = add_scaled(p.position, p.velocity, dt);
            p.age += dt;
        }
        self.particles.retain(|p| p.age < p.life);
    }

    /// The particles alive now.
    #[must_use]
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// The particles alive now, to change by hand.
    pub fn particles_mut(&mut self) -> &mut Vec<Particle> {
        &mut self.particles
    }

    /// How many particles are alive.
    #[must_use]
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    /// Returns `true` if no particle is alive.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Removes every particle.
    pub fn clear(&mut self) {
        self.particles.clear();
        self.owed = 0.0;
    }

    /// The color of `particle` now: its own color, moved towards [`Emitter::end_color`]
    /// and faded out at the end of its life.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "alpha times a fade in 0..=1 stays in 0..=255"
    )]
    #[must_use]
    pub fn color_of(&self, particle: &Particle) -> Rgba {
        let mut color = match self.end_color {
            Some(end) => particle.color.lerp(end, particle.progress()),
            None => particle.color,
        };
        if self.fade > 0.0 {
            let fade = (particle.remaining() / self.fade).min(1.0);
            color.a = (f32::from(color.a) * fade) as u8;
        }
        color
    }

    /// The size of `particle` now in pixels (before the projection's scale).
    #[must_use]
    pub fn size_of(&self, particle: &Particle) -> f32 {
        let (start, end) = self.size;
        start + (end - start) * particle.progress()
    }

    /// Draws every particle as a square of its size. `project` maps a position to the
    /// canvas: pixel x and y and a scale for the size (for example a 3D camera's
    /// perspective), or `None` when it is not on the picture.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "pixel positions and sizes; casts saturate off the canvas"
    )]
    pub fn draw(&self, canvas: &mut Canvas, project: impl Fn(Vec3) -> Option<(f32, f32, f32)>) {
        let before = canvas.blend_mode();
        canvas.set_blend_mode(self.blend);
        for p in &self.particles {
            let Some((x, y, s)) = project(p.position) else {
                continue;
            };
            let size = (self.size_of(p) * s).max(1.0) as u32;
            canvas.fill_rect(
                x.floor() as i32,
                y.floor() as i32,
                size,
                size,
                self.color_of(p),
            );
        }
        canvas.set_blend_mode(before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn bursts_move_age_and_die() {
        let mut rng = Rng::new(1);
        let mut e = Emitter::new().speed(10.0, 10.0).life(0.5, 0.5);
        e.burst(&mut rng, [5.0, 5.0, 0.0], 12);
        assert_eq!(e.len(), 12);
        e.update(100 * MS);
        for p in e.particles() {
            let moved = vnce_core::math::distance(p.position, [5.0, 5.0, 0.0]);
            assert!((moved - 1.0).abs() < 1e-3, "{moved}");
            assert!((p.progress() - 0.2).abs() < 1e-4);
        }
        e.update(400 * MS);
        assert!(e.is_empty());
    }

    #[test]
    fn gravity_drag_and_the_steady_stream() {
        let mut rng = Rng::new(2);
        let mut e = Emitter::new()
            .speed(0.0, 0.0)
            .gravity([0.0, 10.0, 0.0])
            .rate(30.0)
            .life(5.0, 5.0);
        // 30 per second for 0.1 s at a time: 3 each time.
        e.emit(&mut rng, [0.0; 3], 100 * MS);
        assert_eq!(e.len(), 3);
        e.update(100 * MS);
        assert!(e.particles().iter().all(|p| p.position[1] > 0.0));
        // Fractions carry over: 10 per second, 50 ms at a time.
        let mut e = Emitter::new().rate(10.0);
        e.emit(&mut rng, [0.0; 3], 50 * MS);
        assert_eq!(e.len(), 0);
        e.emit(&mut rng, [0.0; 3], 50 * MS);
        assert_eq!(e.len(), 1);
        // Drag slows down.
        let mut e = Emitter::new().speed(10.0, 10.0).drag(5.0);
        e.burst(&mut rng, [0.0; 3], 1);
        e.update(100 * MS);
        let speed = vnce_core::math::length(e.particles()[0].velocity);
        assert!((speed - 5.0).abs() < 1e-3, "{speed}");
    }

    #[test]
    fn spreads_point_where_asked() {
        let mut rng = Rng::new(3);
        for _ in 0..100 {
            let d = Spread::Sphere.sample(&mut rng);
            assert!((vnce_core::math::length(d) - 1.0).abs() < 1e-4);
            let c = Spread::Circle.sample(&mut rng);
            assert!(c[2] == 0.0 && (vnce_core::math::length(c) - 1.0).abs() < 1e-4);
            let a = Spread::Arc {
                angle: 0.0,
                width: 0.5,
            }
            .sample(&mut rng);
            assert!(a[0] > 0.96);
            let cone = Spread::Cone {
                direction: [0.0, 2.0, 0.0],
                angle: 0.3,
            }
            .sample(&mut rng);
            assert!(cone[1] >= 0.3_f32.cos() - 1e-4, "{cone:?}");
        }
    }

    #[test]
    fn colors_sizes_and_limits() {
        let mut rng = Rng::new(4);
        let red = Rgba::rgb(255, 0, 0);
        let blue = Rgba::rgb(0, 0, 255);
        let mut e = Emitter::new()
            .colors([red])
            .end_color(blue)
            .fade(0.0)
            .size(4.0, 2.0)
            .life(1.0, 1.0)
            .max(5);
        e.burst(&mut rng, [0.0; 3], 10);
        assert_eq!(e.len(), 5, "at most 5");
        e.update(500 * MS);
        let p = e.particles()[0];
        assert_eq!(e.color_of(&p), red.lerp(blue, 0.5));
        assert!((e.size_of(&p) - 3.0).abs() < 1e-4);
        // Fading: alpha falls over the last half second.
        let e = Emitter::new().fade(0.5);
        let mut p = Particle::new([0.0; 3], [0.0; 3], 1.0, Rgba::WHITE);
        p.age = 0.75;
        assert_eq!(e.color_of(&p).a, 127);
        // Picking among colors uses only those colors.
        let mut e = Emitter::new().colors([red, blue]);
        e.burst(&mut rng, [0.0; 3], 20);
        assert!(
            e.particles()
                .iter()
                .all(|p| p.color == red || p.color == blue)
        );
        e.set_colors([Rgba::WHITE]);
        e.burst(&mut rng, [0.0; 3], 1);
        assert_eq!(e.particles()[20].color, Rgba::WHITE);
        e.spawn(Particle::new([1.0; 3], [0.0; 3], 1.0, red));
        assert_eq!(e.len(), 22);
        e.clear();
        assert!(e.is_empty());
    }

    #[test]
    fn draws_through_the_projection() {
        let mut rng = Rng::new(5);
        let mut e = Emitter::new()
            .speed(0.0, 0.0)
            .colors([Rgba::rgb(200, 100, 50)])
            .blend(BlendMode::Alpha)
            .size(2.0, 2.0);
        e.burst(&mut rng, [3.0, 4.0, 0.0], 1);
        let mut canvas = Canvas::new(10, 10);
        e.draw(&mut canvas, |[x, y, _]| Some((x, y, 1.0)));
        // A 2 × 2 square from (3, 4).
        assert_eq!(canvas.get(4, 5), Some(Rgba::rgb(200, 100, 50)));
        assert_eq!(canvas.get(5, 5).map(|c| c.a), Some(0));
        let before = canvas.blend_mode();
        e.draw(&mut canvas, |_| None);
        assert_eq!(canvas.blend_mode(), before, "the blend mode is restored");
        // Off the picture: not drawn.
        let mut empty = Canvas::new(10, 10);
        e.draw(&mut empty, |_| None);
        assert!(empty.pixels().iter().all(|p| p.a == 0));
    }
}
