//! Time of day, weather and light: the ambient color from late afternoon to night, rain
//! showers with splashes and thunder, cloud shadows, fireflies, sparks, and the light layer
//! that point lights are added into before it multiplies the scene.

use std::collections::HashMap;

use vnce::vnce_game::Rng;
use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Rgba};

use crate::art;
use crate::island::{Island, Prop, SIZE, index};
use crate::iso::View;

/// Seconds from the start until it is fully dark.
pub const NIGHT: f32 = 300.0;

/// The light of the sky at `progress` (0: afternoon, 1: night), as a multiplier color.
pub fn ambient(progress: f32) -> Rgba {
    let stops = [
        (0.0, Rgba::rgb(255, 250, 236)),
        (0.35, Rgba::rgb(255, 226, 190)),
        (0.55, Rgba::rgb(236, 150, 128)),
        (0.7, Rgba::rgb(130, 110, 170)),
        (0.85, Rgba::rgb(70, 78, 140)),
        (1.0, Rgba::rgb(40, 46, 96)),
    ];
    let t = progress.clamp(0.0, 1.0);
    for pair in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
        if t <= t1 {
            return c0.lerp(c1, (t - t0) / (t1 - t0));
        }
    }
    stops[stops.len() - 1].1
}

/// A short-lived glowing particle in the world, falling under gravity.
pub struct Spark {
    /// Position in tiles and height in levels.
    pub position: (f32, f32, f32),
    pub velocity: (f32, f32, f32),
    pub life: f32,
    pub color: Rgba,
}

/// Sparks bursting from a point.
pub fn burst(
    sparks: &mut Vec<Spark>,
    rng: &mut Rng,
    at: (f32, f32, f32),
    color: Rgba,
    count: usize,
) {
    for _ in 0..count {
        let angle = rng.unit() * std::f32::consts::TAU;
        let speed = 0.6 + rng.unit() * 1.6;
        sparks.push(Spark {
            position: at,
            velocity: (
                angle.cos() * speed,
                angle.sin() * speed,
                1.5 + rng.unit() * 3.0,
            ),
            life: 0.5 + rng.unit() * 0.7,
            color,
        });
    }
}

pub fn update_sparks(sparks: &mut Vec<Spark>, seconds: f32) {
    for spark in sparks.iter_mut() {
        spark.life -= seconds;
        spark.position.0 += spark.velocity.0 * seconds;
        spark.position.1 += spark.velocity.1 * seconds;
        spark.position.2 += spark.velocity.2 * seconds;
        spark.velocity.2 -= 9.0 * seconds;
    }
    sparks.retain(|spark| spark.life > 0.0);
}

/// Sparks as added light, fading out.
pub fn draw_sparks(world: &mut Canvas, view: &View, sparks: &[Spark]) {
    world.set_blend_mode(BlendMode::Add);
    for spark in sparks {
        let (x, y) = view.project(spark.position.0, spark.position.1, spark.position.2);
        let fade = (spark.life * 2.0).min(1.0);
        world.pixel(
            x.round() as i32,
            y.round() as i32,
            spark.color.with_alpha((255.0 * fade) as u8),
        );
    }
    world.set_blend_mode(BlendMode::Alpha);
}

/// A raindrop falling across the screen.
struct Drop {
    x: f32,
    y: f32,
    speed: f32,
}

/// Passing rain showers: none at first, then now and then for half a minute.
pub struct Weather {
    /// How hard it rains now (0 to 1).
    pub rain: f32,
    /// Seconds left of the current shower (0: dry).
    shower: f32,
    /// Seconds until the next shower starts.
    next: f32,
    drops: Vec<Drop>,
    /// Splashes: screen position and seconds left.
    splashes: Vec<(f32, f32, f32)>,
    /// A flash of lightning fading out.
    pub flash: f32,
}

/// What the weather did this update.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WeatherEvents {
    pub thunder: bool,
}

impl Weather {
    pub fn new() -> Self {
        Self {
            rain: 0.0,
            shower: 0.0,
            next: 70.0,
            drops: Vec::new(),
            splashes: Vec::new(),
            flash: 0.0,
        }
    }

    /// Starts a shower now (used by the bench and the snapshots).
    pub fn start_shower(&mut self) {
        self.shower = 30.0;
    }

    pub fn update(&mut self, rng: &mut Rng, seconds: f32) -> WeatherEvents {
        let mut events = WeatherEvents::default();
        self.flash = (self.flash - seconds * 4.0).max(0.0);
        if self.shower > 0.0 {
            self.shower -= seconds;
            // Lightning now and then while it pours.
            if self.rain > 0.7 && rng.chance(seconds * 0.06) {
                self.flash = 1.0;
                events.thunder = true;
            }
            if self.shower <= 0.0 {
                self.next = 60.0 + rng.unit() * 70.0;
            }
        } else {
            self.next -= seconds;
            if self.next <= 0.0 {
                self.shower = 25.0 + rng.unit() * 15.0;
            }
        }
        // Rain eases in and out.
        let target = if self.shower > 3.0 { 1.0 } else { 0.0 };
        self.rain += (target - self.rain) * (seconds * 0.4).min(1.0);
        events
    }

    /// Falling streaks and splashes over the view (`width` × `height` world pixels).
    pub fn draw_rain(
        &mut self,
        world: &mut Canvas,
        rng: &mut Rng,
        seconds: f32,
        width: u32,
        height: u32,
    ) {
        let wanted = (self.rain * width as f32 * height as f32 / 220.0) as usize;
        while self.drops.len() < wanted {
            self.drops.push(Drop {
                x: rng.unit() * width as f32,
                y: rng.unit() * height as f32,
                speed: 160.0 + rng.unit() * 80.0,
            });
        }
        self.drops.truncate(wanted);
        let color = Rgba::new(190, 210, 240, 120);
        for drop in &mut self.drops {
            drop.y += drop.speed * seconds;
            drop.x -= drop.speed * 0.25 * seconds;
            if drop.y > height as f32 || drop.x < 0.0 {
                drop.y -= height as f32;
                drop.x = rng.unit() * width as f32;
            }
            let (x, y) = (drop.x as i32, drop.y as i32);
            world.line(x, y, x + 1, y - 3, color);
        }
        // Splashes appear where drops land.
        let spawn = (self.rain * width as f32 * height as f32 / 4000.0 * seconds * 60.0) as usize;
        for _ in 0..spawn {
            self.splashes
                .push((rng.unit() * width as f32, rng.unit() * height as f32, 0.12));
        }
        let splash = Rgba::new(220, 235, 255, 150);
        for (x, y, life) in &mut self.splashes {
            *life -= seconds;
            let (x, y) = (*x as i32, *y as i32);
            world.pixel(x - 1, y, splash);
            world.pixel(x + 1, y, splash);
            world.pixel(x, y - 1, splash);
        }
        self.splashes.retain(|s| s.2 > 0.0);
    }
}

/// Clouds drifting over the island: their shadows darken the light layer by day.
pub struct Clouds {
    /// Position in tiles and radius in world pixels.
    clouds: Vec<(f32, f32, u32)>,
    sprites: HashMap<u32, Canvas>,
}

impl Clouds {
    pub fn new(rng: &mut Rng) -> Self {
        let clouds = (0..7)
            .map(|_| {
                (
                    rng.unit() * 60.0 - 10.0,
                    rng.unit() * SIZE as f32,
                    28 + rng.below(24),
                )
            })
            .collect();
        Self {
            clouds,
            sprites: HashMap::new(),
        }
    }

    pub fn update(&mut self, seconds: f32) {
        for cloud in &mut self.clouds {
            // The wind blows from the west.
            cloud.0 += seconds * 0.35;
            cloud.1 += seconds * 0.08;
            if cloud.0 > SIZE as f32 + 12.0 {
                cloud.0 -= SIZE as f32 + 24.0;
            }
            if cloud.1 > SIZE as f32 + 8.0 {
                cloud.1 -= SIZE as f32 + 16.0;
            }
        }
    }

    /// Soft shadows of `strength` (0 to 1) in the light layer.
    pub fn draw(&mut self, layer: &mut Canvas, view: &View, strength: f32) {
        if strength <= 0.01 {
            return;
        }
        for &(x, y, radius) in &self.clouds {
            let sprite = self
                .sprites
                .entry(radius)
                .or_insert_with(|| art::glow_sprite(radius, Rgba::rgb(20, 24, 40)));
            let (sx, sy) = view.project(x, y, 2.0);
            let size = sprite.width() as i32;
            layer.blit(
                sprite,
                sx as i32 - size / 2,
                sy as i32 - size / 2,
                BlitOptions {
                    opacity: (strength * 110.0) as u8,
                    ..BlitOptions::default()
                },
            );
        }
    }
}

/// Fireflies around the trees, out at dusk.
pub struct Fireflies {
    /// Home tile center, level, phase.
    flies: Vec<(f32, f32, f32, f32)>,
}

impl Fireflies {
    pub fn new(island: &Island, rng: &mut Rng) -> Self {
        let mut trees: Vec<(usize, usize)> = (0..SIZE * SIZE)
            .map(|i| (i % SIZE, i / SIZE))
            .filter(|&(x, y)| matches!(island.prop[index(x, y)], Some(Prop::Tree | Prop::Bush)))
            .collect();
        rng.shuffle(&mut trees);
        let flies = trees
            .iter()
            .take(28)
            .map(|&(x, y)| {
                (
                    x as f32 + 0.5,
                    y as f32 + 0.5,
                    f32::from(island.level_at((x, y))),
                    rng.unit() * 40.0,
                )
            })
            .collect();
        Self { flies }
    }

    /// Positions (tiles, height) and brightness (0 to 1) at `time`.
    pub fn positions(&self, time: f32) -> impl Iterator<Item = ((f32, f32, f32), f32)> + '_ {
        self.flies.iter().map(move |&(x, y, level, phase)| {
            let t = time * 0.6 + phase;
            (
                (
                    x + (t * 1.3).sin() * 0.7,
                    y + (t * 0.9).cos() * 0.7,
                    level + 1.2 + (t * 2.1).sin() * 0.5,
                ),
                (0.5 + 0.5 * (t * 3.7).sin()).powi(2),
            )
        })
    }
}

/// The light layer: the ambient color, then lights added in.
pub struct Lights {
    /// Light sprites by radius and rounded color.
    sprites: HashMap<(u32, [u8; 3]), Canvas>,
}

impl Lights {
    pub fn new() -> Self {
        Self {
            sprites: HashMap::new(),
        }
    }

    /// Adds a round light at screen point `(x, y)`.
    pub fn add(&mut self, layer: &mut Canvas, (x, y): (f32, f32), radius: u32, color: Rgba) {
        if radius == 0 {
            return;
        }
        // Colors are rounded so pulsing lights reuse a few cached sprites.
        let key = (radius, [color.r & 0xF0, color.g & 0xF0, color.b & 0xF0]);
        let sprite = self
            .sprites
            .entry(key)
            .or_insert_with(|| art::light_sprite(radius, Rgba::rgb(key.1[0], key.1[1], key.1[2])));
        layer.blit(
            sprite,
            x as i32 - radius as i32,
            y as i32 - radius as i32,
            BlitOptions::default(),
        );
    }
}

/// `color` scaled by `factor` (0 to 1).
pub fn dim(color: Rgba, factor: f32) -> Rgba {
    let channel = |c: u8| (f32::from(c) * factor.clamp(0.0, 1.0)) as u8;
    Rgba::new(
        channel(color.r),
        channel(color.g),
        channel(color.b),
        color.a,
    )
}
