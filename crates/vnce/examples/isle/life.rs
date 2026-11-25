//! The island's creatures: crabs scuttling on the beaches by day, and wisps that drift
//! towards the player at night and steal lantern oil.

use std::time::Duration;

use vnce::vnce_game::{Aabb, Animation, Animator, Playback, Rng, move_and_collide};

use crate::island::{Ground, Island, SIZE, index};

/// A crab on the sand.
pub struct Crab {
    pub position: (f32, f32),
    pub previous: (f32, f32),
    heading: (f32, f32),
    /// Seconds until it picks a new heading.
    turn: f32,
    pub walk: Animator,
    /// Seconds left of running away from the player.
    pub fleeing: f32,
}

/// A wisp of cold light, out at night.
pub struct Wisp {
    pub position: (f32, f32),
    pub previous: (f32, f32),
    pub phase: f32,
    /// Seconds until it appears (again); it is out when this is zero or less.
    pub away: f32,
}

impl Wisp {
    pub fn is_out(&self) -> bool {
        self.away <= 0.0
    }
}

/// Whether a crab may walk on `tile`: free sand.
fn crab_ground(island: &Island, tile: (usize, usize)) -> bool {
    island.ground[index(tile.0, tile.1)] == Ground::Sand
        && island.is_walkable(tile)
        && island.stair[index(tile.0, tile.1)].is_none()
}

/// Up to `count` crabs on beach tiles away from the landing.
pub fn crabs(island: &Island, rng: &mut Rng, count: usize) -> Vec<Crab> {
    let mut beach: Vec<(usize, usize)> = (0..SIZE * SIZE)
        .map(|i| (i % SIZE, i / SIZE))
        .filter(|&t| {
            crab_ground(island, t)
                && t.0.abs_diff(island.start.0) + t.1.abs_diff(island.start.1) > 5
        })
        .collect();
    rng.shuffle(&mut beach);
    beach
        .into_iter()
        .take(count)
        .map(|(x, y)| {
            let position = (x as f32 + 0.5, y as f32 + 0.5);
            Crab {
                position,
                previous: position,
                heading: (0.0, 0.0),
                turn: rng.unit() * 2.0,
                walk: Animator::new(Animation::new(
                    [0, 1],
                    Duration::from_millis(110),
                    Playback::Loop,
                )),
                fleeing: 0.0,
            }
        })
        .collect()
}

/// Wisps waiting to come out (their first appearance is spread over the evening).
pub fn wisps(rng: &mut Rng, count: usize) -> Vec<Wisp> {
    (0..count)
        .map(|i| Wisp {
            position: (0.0, 0.0),
            previous: (0.0, 0.0),
            phase: rng.unit() * 10.0,
            away: 4.0 + i as f32 * 9.0 + rng.unit() * 6.0,
        })
        .collect()
}

/// Crabs wander, pausing now and then, and run from the player after being bumped.
pub fn update_crabs(
    crabs: &mut [Crab],
    island: &Island,
    rng: &mut Rng,
    player: (f32, f32),
    seconds: f32,
) {
    for crab in crabs {
        crab.previous = crab.position;
        crab.turn -= seconds;
        crab.fleeing = (crab.fleeing - seconds).max(0.0);
        let (mut dx, mut dy) = crab.heading;
        let mut speed = 0.9;
        if crab.fleeing > 0.0 {
            let (ax, ay) = (crab.position.0 - player.0, crab.position.1 - player.1);
            let length = (ax * ax + ay * ay).sqrt().max(0.01);
            (dx, dy) = (ax / length, ay / length);
            speed = 2.4;
        } else if crab.turn <= 0.0 {
            crab.turn = 0.8 + rng.unit() * 2.2;
            crab.heading = if rng.chance(0.3) {
                (0.0, 0.0)
            } else {
                let angle = rng.unit() * std::f32::consts::TAU;
                (angle.cos(), angle.sin())
            };
            continue;
        }
        if dx == 0.0 && dy == 0.0 {
            continue;
        }
        crab.walk.update(Duration::from_secs_f32(seconds));
        let body = Aabb::from_center(crab.position.0, crab.position.1, 0.3, 0.3);
        let movement = move_and_collide(
            body,
            dx * speed * seconds,
            dy * speed * seconds,
            1.0,
            |c, r| {
                c < 0
                    || r < 0
                    || c >= SIZE as i32
                    || r >= SIZE as i32
                    || !crab_ground(island, (c as usize, r as usize))
            },
        );
        if movement.hit.any() {
            crab.turn = 0.0;
        }
        crab.position = movement.aabb.center();
    }
}

/// Wisps come out after dusk at a free tile away from the player and drift towards them.
pub fn update_wisps(
    wisps: &mut [Wisp],
    island: &Island,
    rng: &mut Rng,
    player: (f32, f32),
    night: f32,
    seconds: f32,
) {
    // Before late dusk, none come out.
    if night < 0.6 {
        return;
    }
    for wisp in wisps {
        wisp.previous = wisp.position;
        if !wisp.is_out() {
            wisp.away -= seconds;
            if wisp.is_out() {
                // Appear somewhere out of reach, 8 to 14 tiles away.
                for _ in 0..40 {
                    let tile = (
                        rng.below(SIZE as u32) as usize,
                        rng.below(SIZE as u32) as usize,
                    );
                    let (x, y) = (tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
                    let distance = ((x - player.0).powi(2) + (y - player.1).powi(2)).sqrt();
                    if island.is_land(tile) && (8.0..14.0).contains(&distance) {
                        wisp.position = (x, y);
                        wisp.previous = wisp.position;
                        break;
                    }
                }
            }
            continue;
        }
        wisp.phase += seconds;
        let (ax, ay) = (player.0 - wisp.position.0, player.1 - wisp.position.1);
        let length = (ax * ax + ay * ay).sqrt().max(0.01);
        // Slower than the player, wobbling as it comes.
        let wobble = (wisp.phase * 2.3).sin() * 0.6;
        let (dx, dy) = (
            ax / length - ay / length * wobble,
            ay / length + ax / length * wobble,
        );
        let speed = 1.0 + night * 0.6;
        wisp.position.0 += dx * speed * seconds;
        wisp.position.1 += dy * speed * seconds;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crabs_stay_on_the_sand() {
        let island = Island::generate(9);
        let mut rng = Rng::new(1);
        let mut list = crabs(&island, &mut rng, 6);
        assert!(!list.is_empty());
        for step in 0..3000 {
            let player = (20.0, 20.0 + (step % 7) as f32);
            if step % 500 == 0 {
                list[0].fleeing = 1.5;
            }
            update_crabs(&mut list, &island, &mut rng, player, 1.0 / 60.0);
            for crab in &list {
                let tile = (crab.position.0 as usize, crab.position.1 as usize);
                assert!(crab_ground(&island, tile), "{tile:?}");
            }
        }
    }

    #[test]
    fn wisps_come_out_at_night_and_follow() {
        let island = Island::generate(9);
        let mut rng = Rng::new(2);
        let mut list = wisps(&mut rng, 3);
        let player = (island.start.0 as f32 + 0.5, island.start.1 as f32 + 0.5);
        for _ in 0..600 {
            update_wisps(&mut list, &island, &mut rng, player, 0.3, 1.0 / 60.0);
        }
        assert!(list.iter().all(|w| !w.is_out()), "not before dusk");
        for _ in 0..60 * 30 {
            update_wisps(&mut list, &island, &mut rng, player, 0.8, 1.0 / 60.0);
        }
        let out: Vec<&Wisp> = list.iter().filter(|w| w.is_out()).collect();
        assert!(!out.is_empty());
        let distance = |w: &Wisp| {
            ((w.position.0 - player.0).powi(2) + (w.position.1 - player.1).powi(2)).sqrt()
        };
        assert!(out.iter().any(|w| distance(w) < 3.0), "they come close");
    }
}
