//! The craft: an arcade flight model (bank to turn, dive to gain speed), the stick that
//! steers it, and an autopilot for the title screen and the bench.

use std::f32::consts::{PI, TAU};

use crate::math::{Mat4, Vec3, add_scaled, cross, direction, normalize, rotate, scale, sub};
use crate::terrain::Terrain;

/// Slowest and fastest speed in m/s.
pub const MIN_SPEED: f32 = 18.0;
pub const MAX_SPEED: f32 = 125.0;
/// Cruise speed at no and full throttle.
pub const CRUISE: (f32, f32) = (30.0, 72.0);
/// Above this the craft is pushed down.
pub const CEILING: f32 = 460.0;
/// A ring's push lasts this long and adds this much speed.
pub const BOOST_TIME: f32 = 1.6;
pub const BOOST_SPEED: f32 = 32.0;

/// What the pilot asks for, each in −1..1.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stick {
    /// Nose up (positive) or down.
    pub pitch: f32,
    /// Bank right (positive) or left.
    pub turn: f32,
    /// Throttle up (positive) or down.
    pub throttle: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Craft {
    pub position: Vec3,
    /// Heading: 0 north, π/2 east.
    pub yaw: f32,
    pub pitch: f32,
    /// Bank: positive is the right wing down.
    pub roll: f32,
    pub speed: f32,
    /// 0..1.
    pub throttle: f32,
    /// Seconds of ring boost left.
    pub boost: f32,
}

impl Craft {
    pub fn new(position: Vec3, yaw: f32) -> Self {
        Self {
            position,
            yaw,
            pitch: 0.0,
            roll: 0.0,
            speed: 46.0,
            throttle: 0.4,
            boost: 0.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        direction(self.yaw, self.pitch)
    }

    /// The craft's right and up, with the bank.
    pub fn axes(&self) -> (Vec3, Vec3) {
        let forward = self.forward();
        let right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let up = cross(right, forward);
        // Banking turns both around the direction of flight.
        (
            rotate(right, forward, self.roll),
            rotate(up, forward, self.roll),
        )
    }

    pub fn up(&self) -> Vec3 {
        self.axes().1
    }

    /// Model to world: the model faces north (−z) with +y up.
    pub fn model(&self) -> Mat4 {
        let (right, up) = self.axes();
        Mat4::translation(self.position) * Mat4::basis(right, up, scale(self.forward(), -1.0))
    }

    /// Where the exhaust leaves the craft.
    pub fn exhaust(&self) -> Vec3 {
        self.model().point([0.0, 0.3, 3.3])
    }

    /// One step of `dt` seconds.
    pub fn update(&mut self, stick: Stick, dt: f32) {
        let ease = |t: f32| (t * dt).min(1.0);
        // Bank towards the stick; the bank turns the craft.
        let target_roll = stick.turn.clamp(-1.0, 1.0) * 1.05;
        self.roll += (target_roll - self.roll) * ease(3.5);
        self.yaw = (self.yaw + self.roll.sin() * 1.2 * dt).rem_euclid(TAU);
        // Pitch with the stick; without it the nose slowly levels.
        if stick.pitch.abs() > 0.05 {
            self.pitch += stick.pitch.clamp(-1.0, 1.0) * 0.95 * dt;
        } else {
            self.pitch -= self.pitch * ease(0.6);
        }
        if self.position[1] > CEILING {
            self.pitch = self.pitch.min(-0.15);
        }
        self.pitch = self.pitch.clamp(-0.95, 0.95);
        self.throttle = (self.throttle + stick.throttle * 0.6 * dt).clamp(0.0, 1.0);
        let mut cruise = CRUISE.0 + (CRUISE.1 - CRUISE.0) * self.throttle;
        if self.boost > 0.0 {
            self.boost = (self.boost - dt).max(0.0);
            cruise += BOOST_SPEED;
        }
        // Diving speeds the craft up, climbing slows it down.
        self.speed += (cruise - self.speed) * ease(0.7) - self.pitch.sin() * 9.81 * 0.55 * dt;
        self.speed = self.speed.clamp(MIN_SPEED, MAX_SPEED);
        self.position = add_scaled(self.position, self.forward(), self.speed * dt);
    }
}

/// The angle from `from` to `to`, in −π..π.
pub fn angle_between(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

/// Steers towards `target`, pulling up when the ground ahead comes close.
pub fn autopilot(craft: &Craft, target: Vec3, terrain: &Terrain) -> Stick {
    let to = sub(target, craft.position);
    let wanted_yaw = to[0].atan2(-to[2]);
    let turn = (angle_between(craft.yaw, wanted_yaw) * 1.6).clamp(-1.0, 1.0);
    let flat = to[0].hypot(to[2]).max(1.0);
    let mut wanted_pitch = (to[1] / flat).atan().clamp(-0.5, 0.5);
    // Look ahead for hills.
    for seconds in [1.0, 2.5, 4.0] {
        let ahead = add_scaled(
            craft.position,
            direction(craft.yaw, 0.0),
            craft.speed * seconds,
        );
        let clearance = craft.position[1] - terrain.surface(ahead[0], ahead[2]);
        if clearance < 35.0 {
            wanted_pitch = wanted_pitch.max(0.45);
        }
    }
    Stick {
        pitch: ((wanted_pitch - craft.pitch) * 3.0).clamp(-1.0, 1.0),
        turn,
        throttle: 0.3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::dot;

    #[test]
    fn banking_turns_and_diving_speeds_up() {
        let mut craft = Craft::new([0.0, 200.0, 0.0], 0.0);
        let right = Stick {
            turn: 1.0,
            ..Stick::default()
        };
        for _ in 0..60 {
            craft.update(right, 1.0 / 60.0);
        }
        assert!(craft.roll > 0.5, "banked right");
        assert!(
            craft.yaw > 0.1 && craft.yaw < 1.5,
            "turned right (east): {}",
            craft.yaw
        );
        // The right wing is down.
        assert!(craft.axes().0[1] < -0.3);
        let speed = craft.speed;
        let dive = Stick {
            pitch: -1.0,
            ..Stick::default()
        };
        for _ in 0..60 {
            craft.update(dive, 1.0 / 60.0);
        }
        assert!(craft.speed > speed);
        assert!(craft.position[1] < 200.0);
    }

    #[test]
    fn the_model_faces_where_the_craft_flies() {
        let mut craft = Craft::new([5.0, 80.0, -3.0], 1.0);
        craft.pitch = 0.3;
        craft.roll = 0.4;
        let model = craft.model();
        let nose = sub(model.point([0.0, 0.0, -1.0]), craft.position);
        assert!(dot(normalize(nose), craft.forward()) > 0.999);
        assert!(
            dot(
                normalize(sub(model.point([0.0, 1.0, 0.0]), craft.position)),
                craft.up()
            ) > 0.999
        );
    }

    #[test]
    fn the_autopilot_reaches_a_point() {
        let terrain = Terrain::new(4);
        let mut craft = Craft::new([0.0, 400.0, 0.0], 0.0);
        let target = [600.0, 420.0, 300.0];
        let mut closest = f32::MAX;
        for _ in 0..60 * 30 {
            let stick = autopilot(&craft, target, &terrain);
            craft.update(stick, 1.0 / 60.0);
            closest = closest.min(crate::math::length(sub(craft.position, target)));
        }
        assert!(closest < 40.0, "{closest}");
        assert!((angle_between(0.1, TAU - 0.1) + 0.2).abs() < 1e-5);
    }
}
