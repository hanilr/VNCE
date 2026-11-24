//! A scrolling 2D view: [`Camera`].

use std::time::Duration;

use crate::collision::Aabb;

/// A view into a larger world: it follows a target, stays inside the world, shakes, and
/// converts between world and screen coordinates.
///
/// Call [`Camera::update`] once per game update; draw with [`Camera::offset`] using the
/// interpolation factor of the frame (`ctx.alpha()`), so scrolling stays smooth at any
/// frame rate.
///
/// ```
/// use std::time::Duration;
/// use vnce_game::{Aabb, Camera};
///
/// let mut camera = Camera::new(100.0, 60.0).with_bounds(Aabb::new(0.0, 0.0, 400.0, 60.0));
/// camera.update(Duration::from_millis(16), Some((250.0, 30.0)));
/// assert_eq!(camera.offset(1.0), (200, 0)); // the target is centered
/// camera.update(Duration::from_millis(16), Some((390.0, 30.0)));
/// assert_eq!(camera.offset(1.0), (300, 0)); // stopped at the right edge of the world
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// Top-left corner in world coordinates.
    position: (f32, f32),
    previous: (f32, f32),
    viewport: (f32, f32),
    bounds: Option<Aabb>,
    /// Fraction of the remaining distance covered per 1/60 s (1 = no smoothing).
    smoothing: f32,
    dead_zone: (f32, f32),
    shake: Shake,
}

/// Screen shake state.
#[derive(Debug, Clone, PartialEq, Default)]
struct Shake {
    intensity: f32,
    duration: Duration,
    remaining: Duration,
    /// Deterministic random state for the shake direction.
    seed: u32,
    offset: (f32, f32),
}

impl Camera {
    /// A camera showing `width` × `height` world pixels, at the origin.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            position: (0.0, 0.0),
            previous: (0.0, 0.0),
            viewport: (width.max(0.0), height.max(0.0)),
            bounds: None,
            smoothing: 1.0,
            dead_zone: (0.0, 0.0),
            shake: Shake {
                seed: 0x9E37_79B9,
                ..Shake::default()
            },
        }
    }

    /// Keeps the view inside `bounds` (usually the world, for example
    /// [`TileMap::bounds`](crate::TileMap::bounds)). A world smaller than the view is
    /// centered.
    #[must_use]
    pub fn with_bounds(mut self, bounds: Aabb) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Follows the target smoothly: `smoothing` is the fraction of the remaining distance
    /// covered per 1/60 s, from `0.0` (never moves) to `1.0` (no smoothing, the default).
    #[must_use]
    pub fn with_smoothing(mut self, smoothing: f32) -> Self {
        self.smoothing = if smoothing.is_finite() {
            smoothing.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self
    }

    /// The target can move this far (in world pixels) from the center, horizontally and
    /// vertically, before the camera follows.
    #[must_use]
    pub fn with_dead_zone(mut self, width: f32, height: f32) -> Self {
        self.dead_zone = (width.max(0.0) / 2.0, height.max(0.0) / 2.0);
        self
    }

    /// Changes the size of the view (for example when the terminal is resized).
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = (width.max(0.0), height.max(0.0));
        self.position = self.clamped(self.position);
        self.previous = self.position;
    }

    /// Size of the view in world pixels.
    #[must_use]
    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    /// Changes the world bounds (`None` for no bounds).
    pub fn set_bounds(&mut self, bounds: Option<Aabb>) {
        self.bounds = bounds;
    }

    /// Centers the view on `(x, y)` at once, without smoothing or interpolation (for
    /// example when a level starts).
    pub fn center_on(&mut self, x: f32, y: f32) {
        self.position = self.clamped((x - self.viewport.0 / 2.0, y - self.viewport.1 / 2.0));
        self.previous = self.position;
    }

    /// Shakes the view by up to `intensity` world pixels, fading out over `duration`.
    pub fn shake(&mut self, intensity: f32, duration: Duration) {
        self.shake.intensity = intensity.max(0.0);
        self.shake.duration = duration;
        self.shake.remaining = duration;
    }

    /// Advances one update: follows `target` (a world position to keep in view, usually
    /// the player's center) and updates the shake.
    pub fn update(&mut self, dt: Duration, target: Option<(f32, f32)>) {
        self.previous = self.position;
        if let Some((x, y)) = target {
            let (width, height) = self.viewport;
            let center = (
                self.position.0 + width / 2.0,
                self.position.1 + height / 2.0,
            );
            // Only move by what is outside the dead zone.
            let follow = |target: f32, center: f32, zone: f32| {
                if target > center + zone {
                    target - zone
                } else if target < center - zone {
                    target + zone
                } else {
                    center
                }
            };
            let wanted = (
                follow(x, center.0, self.dead_zone.0) - width / 2.0,
                follow(y, center.1, self.dead_zone.1) - height / 2.0,
            );
            let fraction = if self.smoothing >= 1.0 {
                1.0
            } else {
                1.0 - (1.0 - self.smoothing).powf(dt.as_secs_f32() * 60.0)
            };
            self.position = self.clamped((
                self.position.0 + (wanted.0 - self.position.0) * fraction,
                self.position.1 + (wanted.1 - self.position.1) * fraction,
            ));
        }
        self.update_shake(dt);
    }

    fn update_shake(&mut self, dt: Duration) {
        let shake = &mut self.shake;
        shake.remaining = shake.remaining.saturating_sub(dt);
        if shake.remaining.is_zero() || shake.duration.is_zero() {
            shake.offset = (0.0, 0.0);
            return;
        }
        let strength =
            shake.intensity * shake.remaining.as_secs_f32() / shake.duration.as_secs_f32();
        let mut random = || {
            // xorshift32
            shake.seed ^= shake.seed << 13;
            shake.seed ^= shake.seed >> 17;
            shake.seed ^= shake.seed << 5;
            // 16 random bits: exact in f32.
            f32::from(u16::try_from(shake.seed >> 16).unwrap_or(0)) / 65_536.0 * 2.0 - 1.0
        };
        let offset = (random() * strength, random() * strength);
        shake.offset = offset;
    }

    /// The position clamped to the bounds; a world smaller than the view is centered.
    fn clamped(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let Some(bounds) = self.bounds else {
            return (x, y);
        };
        let axis = |value: f32, start: f32, size: f32, view: f32| {
            if size <= view {
                start + (size - view) / 2.0
            } else {
                value.clamp(start, start + size - view)
            }
        };
        (
            axis(x, bounds.x, bounds.width, self.viewport.0),
            axis(y, bounds.y, bounds.height, self.viewport.1),
        )
    }

    /// The top-left corner of the view between the previous and the current update
    /// (`alpha` from `0.0` to `1.0`, usually `ctx.alpha()`), without shake.
    #[must_use]
    pub fn position(&self, alpha: f32) -> (f32, f32) {
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        (
            self.previous.0 + (self.position.0 - self.previous.0) * alpha,
            self.previous.1 + (self.position.1 - self.previous.1) * alpha,
        )
    }

    /// The world position drawn at the top-left pixel of the screen, rounded to whole
    /// pixels (so the picture does not shimmer), with shake. Draw a world position
    /// `(x, y)` at `(x - offset.0, y - offset.1)`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "world positions fit in i32"
    )]
    pub fn offset(&self, alpha: f32) -> (i32, i32) {
        let (x, y) = self.position(alpha);
        (
            (x + self.shake.offset.0).round() as i32,
            (y + self.shake.offset.1).round() as i32,
        )
    }

    /// Converts a world position to a screen position (in pixels of the view).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "offsets are small")]
    pub fn world_to_screen(&self, x: f32, y: f32, alpha: f32) -> (f32, f32) {
        let (ox, oy) = self.offset(alpha);
        (x - ox as f32, y - oy as f32)
    }

    /// Converts a screen position (in pixels of the view) to a world position.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "offsets are small")]
    pub fn screen_to_world(&self, x: f32, y: f32, alpha: f32) -> (f32, f32) {
        let (ox, oy) = self.offset(alpha);
        (x + ox as f32, y + oy as f32)
    }

    /// The part of the world in view.
    #[must_use]
    pub fn visible(&self, alpha: f32) -> Aabb {
        let (x, y) = self.position(alpha);
        Aabb::new(x, y, self.viewport.0, self.viewport.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: Duration = Duration::from_millis(16);

    #[test]
    fn follows_and_clamps() {
        let world = Aabb::new(0.0, 0.0, 200.0, 100.0);
        let mut camera = Camera::new(50.0, 40.0).with_bounds(world);
        camera.update(DT, Some((100.0, 50.0)));
        assert_eq!(camera.offset(1.0), (75, 30));
        camera.update(DT, Some((-500.0, -500.0)));
        assert_eq!(camera.offset(1.0), (0, 0));
        camera.update(DT, Some((500.0, 500.0)));
        assert_eq!(camera.offset(1.0), (150, 60));
        assert_eq!(camera.visible(1.0), Aabb::new(150.0, 60.0, 50.0, 40.0));
    }

    #[test]
    fn small_worlds_are_centered() {
        let mut camera = Camera::new(100.0, 100.0).with_bounds(Aabb::new(0.0, 0.0, 60.0, 200.0));
        camera.center_on(30.0, 0.0);
        assert_eq!(camera.offset(1.0), (-20, 0));
    }

    #[test]
    fn interpolation_between_updates() {
        let mut camera = Camera::new(10.0, 10.0);
        camera.center_on(5.0, 5.0);
        camera.update(DT, Some((25.0, 5.0)));
        assert_eq!(camera.position(0.0), (0.0, 0.0));
        assert_eq!(camera.position(0.5), (10.0, 0.0));
        assert_eq!(camera.position(1.0), (20.0, 0.0));
        assert_eq!(camera.position(f32::NAN), (20.0, 0.0));
    }

    #[test]
    fn dead_zone_and_smoothing() {
        let mut camera = Camera::new(100.0, 100.0).with_dead_zone(20.0, 20.0);
        camera.center_on(50.0, 50.0);
        // Inside the dead zone (±10 around the center): no movement.
        camera.update(DT, Some((58.0, 45.0)));
        assert_eq!(camera.offset(1.0), (0, 0));
        // Outside: the target is kept at the edge of the zone.
        camera.update(DT, Some((80.0, 50.0)));
        assert_eq!(camera.offset(1.0), (20, 0));

        let mut smooth = Camera::new(100.0, 100.0).with_smoothing(0.5);
        smooth.center_on(50.0, 50.0);
        smooth.update(Duration::from_secs_f32(1.0 / 60.0), Some((150.0, 50.0)));
        assert_eq!(smooth.offset(1.0), (50, 0), "half of the way in 1/60 s");
        let mut frozen = Camera::new(10.0, 10.0).with_smoothing(0.0);
        frozen.update(DT, Some((100.0, 100.0)));
        assert_eq!(frozen.offset(1.0), (0, 0));
    }

    #[test]
    fn shake_fades_out() {
        let mut camera = Camera::new(10.0, 10.0);
        camera.shake(4.0, Duration::from_millis(100));
        camera.update(DT, None);
        let (x, y) = camera.offset(1.0);
        assert!(x.abs() <= 4 && y.abs() <= 4);
        let moved = (0..5).any(|_| {
            camera.update(Duration::from_millis(1), None);
            camera.offset(1.0) != (0, 0)
        });
        assert!(moved);
        camera.update(Duration::from_millis(200), None);
        assert_eq!(camera.offset(1.0), (0, 0));
    }

    #[test]
    fn coordinate_conversions() {
        let mut camera = Camera::new(10.0, 10.0);
        camera.center_on(105.0, 55.0);
        assert_eq!(camera.world_to_screen(103.0, 52.0, 1.0), (3.0, 2.0));
        assert_eq!(camera.screen_to_world(3.0, 2.0, 1.0), (103.0, 52.0));
        camera.set_viewport(20.0, 20.0);
        assert_eq!(camera.viewport(), (20.0, 20.0));
        camera.set_bounds(None);
    }
}
