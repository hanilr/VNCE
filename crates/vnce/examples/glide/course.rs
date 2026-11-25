//! Courses: rings laid out over the terrain from a six-letter code, ending in a finish
//! ring, with a checkpoint every few rings; and an endless line of rings for free flight.

use std::f32::consts::TAU;

use vnce::vnce_game::Rng;

use crate::math::{Vec3, add_scaled, direction, normalize, sub};
use crate::models::RingKind;
use crate::terrain::{Terrain, highest_around};

/// Letters of a course code (no I or O, which look like 1 and 0).
pub const CODE_LETTERS: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_LENGTH: usize = 6;
/// Number of different courses: 32^6 = 2^30.
pub const CODES: u64 = 1 << 30;
/// Rings of a course, the last one the finish.
pub const RINGS: usize = 16;
/// A checkpoint every this many rings.
pub const CHECKPOINT_EVERY: usize = 4;
/// Ring radius in meters (the model is scaled to it).
pub const RING_RADIUS: f32 = 13.0;

/// The code of course `seed`.
pub fn code(seed: u64) -> String {
    let letters = CODE_LETTERS.as_bytes();
    (0..CODE_LENGTH)
        .map(|i| char::from(letters[((seed >> (5 * (CODE_LENGTH - 1 - i))) & 31) as usize]))
        .collect()
}

/// The course of a code (case does not matter), or `None` for anything else.
pub fn parse_code(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_uppercase();
    if text.chars().count() != CODE_LENGTH {
        return None;
    }
    text.chars().try_fold(0_u64, |seed, c| {
        CODE_LETTERS.find(c).map(|index| seed << 5 | index as u64)
    })
}

/// A ring to fly through along `normal`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    pub center: Vec3,
    /// Unit vector of the flight direction through the ring.
    pub normal: Vec3,
    pub kind: RingKind,
}

impl Ring {
    /// Whether the flight from `from` to `to` passes through the ring the right way.
    pub fn passed(&self, from: Vec3, to: Vec3) -> bool {
        let before = crate::math::dot(sub(from, self.center), self.normal);
        let after = crate::math::dot(sub(to, self.center), self.normal);
        if before >= 0.0 || after < 0.0 {
            return false;
        }
        let t = before / (before - after);
        let hit = crate::math::lerp(from, to, t);
        crate::math::length(sub(hit, self.center)) < RING_RADIUS * 0.95
    }

    /// The flight's heading through the ring.
    pub fn yaw(&self) -> f32 {
        self.normal[0].atan2(-self.normal[2])
    }
}

/// Lays out rings one after another: each a little turned from the last, high enough
/// above the ground around it.
pub struct Layout {
    rng: Rng,
    position: Vec3,
    yaw: f32,
    count: usize,
}

impl Layout {
    /// Starts at `position`, heading `yaw`.
    pub fn new(seed: u64, position: Vec3, yaw: f32) -> Self {
        Self {
            rng: Rng::new(seed ^ 0x5EED_F0C0_u64),
            position,
            yaw,
            count: 0,
        }
    }

    /// The next ring; `kind` decides what it is.
    pub fn next(&mut self, terrain: &Terrain, kind: RingKind) -> Ring {
        self.count += 1;
        self.yaw += (self.rng.unit() - 0.5) * 1.1;
        let distance = 230.0 + self.rng.unit() * 120.0;
        let flat = direction(self.yaw, 0.0);
        let mut center = add_scaled(self.position, flat, distance);
        let ground = highest_around(terrain, center, RING_RADIUS * 3.0);
        let wanted = ground + RING_RADIUS + 14.0 + self.rng.unit() * 40.0;
        // Climb and dive gently, but never into the ground.
        let step = distance * 0.4;
        center[1] = wanted
            .clamp(self.position[1] - step, self.position[1] + step)
            .max(ground + RING_RADIUS + 10.0)
            .min(430.0);
        let normal = normalize(sub(center, self.position));
        self.position = center;
        Ring {
            center,
            normal,
            kind,
        }
    }
}

/// A course: its rings, where the flight starts, and the time of day it starts at.
#[derive(Debug, Clone)]
pub struct Course {
    pub rings: Vec<Ring>,
    pub start: Vec3,
    pub yaw: f32,
    /// Day progress at the start (0 sunrise, 1 sunset).
    pub phase: f32,
}

/// A place to start flying near `(x, z)` with the given heading: high above the ground.
pub fn start_point(terrain: &Terrain, x: f32, z: f32) -> Vec3 {
    let ground = highest_around(terrain, [x, 0.0, z], 160.0);
    [x, ground + 55.0, z]
}

impl Course {
    pub fn generate(seed: u64, terrain: &Terrain) -> Self {
        let mut rng = Rng::new(seed ^ 0xC0_u64);
        let yaw = rng.unit() * TAU;
        let start = start_point(terrain, 0.0, 0.0);
        let mut layout = Layout::new(seed, start, yaw);
        let rings = (0..RINGS)
            .map(|i| {
                let kind = if i + 1 == RINGS {
                    RingKind::Finish
                } else if (i + 1) % CHECKPOINT_EVERY == 0 {
                    RingKind::Checkpoint
                } else {
                    RingKind::Normal
                };
                layout.next(terrain, kind)
            })
            .collect();
        Self {
            rings,
            start,
            yaw,
            phase: 0.05 + rng.unit() * 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for seed in [0, 1, 31, 32, 12_345_678, CODES - 1] {
            let text = code(seed);
            assert_eq!(text.len(), CODE_LENGTH);
            assert_eq!(parse_code(&text), Some(seed));
            assert_eq!(parse_code(&text.to_lowercase()), Some(seed));
        }
        assert_eq!(parse_code("ABC"), None);
        assert_eq!(parse_code("ABCDE1"), None);
        assert_eq!(parse_code("ABCDEO"), None);
    }

    #[test]
    fn courses_stay_above_the_ground() {
        for seed in [1, 2, 99] {
            let terrain = Terrain::new(seed);
            let course = Course::generate(seed, &terrain);
            assert_eq!(course.rings.len(), RINGS);
            assert_eq!(course.rings.last().map(|r| r.kind), Some(RingKind::Finish));
            assert_eq!(
                course
                    .rings
                    .iter()
                    .filter(|r| r.kind == RingKind::Checkpoint)
                    .count(),
                RINGS / CHECKPOINT_EVERY - 1
            );
            for ring in &course.rings {
                let ground = terrain.surface(ring.center[0], ring.center[2]);
                assert!(
                    ring.center[1] - RING_RADIUS > ground,
                    "{ring:?} over {ground}"
                );
            }
            let again = Course::generate(seed, &terrain);
            assert_eq!(again.rings, course.rings);
        }
    }

    #[test]
    fn rings_count_only_the_right_way_through() {
        let ring = Ring {
            center: [0.0, 50.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            kind: RingKind::Normal,
        };
        assert!(ring.passed([1.0, 52.0, 5.0], [1.0, 52.0, -5.0]));
        assert!(
            !ring.passed([1.0, 52.0, -5.0], [1.0, 52.0, 5.0]),
            "backwards"
        );
        assert!(
            !ring.passed([30.0, 50.0, 5.0], [30.0, 50.0, -5.0]),
            "beside it"
        );
        assert!(
            !ring.passed([1.0, 52.0, 9.0], [1.0, 52.0, 5.0]),
            "not there yet"
        );
        assert!((ring.yaw()).abs() < 1e-5);
    }
}
