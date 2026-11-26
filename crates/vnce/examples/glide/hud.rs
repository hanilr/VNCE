//! Effects and instruments drawn on the CPU over the rendered picture, placed with the
//! same camera as the picture: the exhaust trail, sparks, speed lines, the compass, the
//! radar of the rings, an arrow to the next ring and the cockpit's sight.

use std::f32::consts::{PI, TAU};

use vnce::vnce_game::Emitter;
use vnce::vnce_raster::{BlendMode, Canvas, Rgba};

use crate::flight::angle_between;
use crate::math::{Camera, Vec3, sub};

/// Size of the instruments in picture pixels per "unit", so they look alike at every
/// resolution.
pub fn unit(canvas: &Canvas) -> f32 {
    (canvas.height() as f32 / 140.0).max(1.0)
}

/// Sparks and bits of debris: they fall, slow down and fade out over their last second
/// (spawned by hand with `Emitter::spawn`, with their seconds of life).
pub fn spark_emitter() -> Emitter {
    Emitter::new()
        .gravity([0.0, -9.81 * 0.6, 0.0])
        .drag(1.2)
        .fade(1.0)
}

/// The exhaust trail: points from old to new with their age in seconds, fading out.
pub fn trail(canvas: &mut Canvas, camera: &Camera, points: &[(Vec3, f32)], max_age: f32) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    canvas.set_blend_mode(BlendMode::Alpha);
    let mut previous: Option<(f32, f32)> = None;
    for &(point, age) in points {
        // Points beside or behind the camera would stretch across the picture.
        let Some((x, y, depth)) = camera.project(point, w, h).filter(|p| p.2 > 4.0) else {
            previous = None;
            continue;
        };
        if let Some((px, py)) = previous {
            let fade = (1.0 - age / max_age).clamp(0.0, 1.0);
            // Near the camera the trail is wider.
            let alpha = (fade * 170.0 * (60.0 / depth.max(8.0)).min(1.0)) as u8;
            let color = Rgba::new(245, 248, 255, alpha);
            canvas.line_aa(px, py, x, y, color);
            if depth < 30.0 {
                canvas.line_aa(px, py + 1.0, x, y + 1.0, color);
            }
        }
        previous = Some((x, y));
    }
}

pub fn sparks(canvas: &mut Canvas, camera: &Camera, sparks: &Emitter) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    sparks.draw(canvas, |position| {
        let (x, y, depth) = camera.project(position, w, h)?;
        Some((x, y, (40.0 / depth).clamp(1.0, 3.0)))
    });
}

/// Streaks rushing past the edges of the picture when the craft is fast (`amount` 0..1).
pub fn speed_lines(canvas: &mut Canvas, amount: f32, time: f32) {
    if amount <= 0.0 {
        return;
    }
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let reach = cx.hypot(cy);
    canvas.set_blend_mode(BlendMode::Add);
    for i in 0..28 {
        // Each line moves outwards and comes back at a new angle.
        let seed = i as f32 * 12.9898;
        let cycle = (time * 2.2 + (seed.sin() * 43_758.547).fract()).fract();
        let angle = ((seed + (time * 2.2 + seed).floor() * 7.3).sin() * 43_758.547).fract() * TAU;
        let (sin, cos) = angle.sin_cos();
        let start = reach * (0.45 + cycle * 0.5);
        let length = reach * 0.12 * amount;
        let alpha = (amount * 70.0 * (1.0 - cycle)) as u8;
        canvas.line_aa(
            cx + cos * start,
            cy + sin * start,
            cx + cos * (start + length),
            cy + sin * (start + length),
            Rgba::new(255, 255, 255, alpha),
        );
    }
    canvas.set_blend_mode(BlendMode::Alpha);
}

/// A tape along the top with the heading (N E S W and ticks) and colored marks at the
/// bearings of `marks`.
pub fn compass(canvas: &mut Canvas, yaw: f32, marks: &[(f32, Rgba)]) {
    let u = unit(canvas);
    let w = canvas.width() as f32;
    let width = (w * 0.5).min(160.0 * u);
    let (left, top) = ((w - width) / 2.0, 3.0 * u);
    let height = 7.0 * u;
    canvas.set_blend_mode(BlendMode::Alpha);
    canvas.fill_rect(
        left as i32,
        top as i32,
        width as u32,
        height as u32,
        Rgba::new(10, 14, 24, 120),
    );
    // The tape shows 120° of heading.
    let span = 2.0 * PI / 3.0;
    let x_of = |bearing: f32| left + width / 2.0 + angle_between(yaw, bearing) / span * width;
    let font = vnce::vnce_raster::Font::builtin();
    for step in 0..24 {
        let bearing = step as f32 * TAU / 24.0;
        let x = x_of(bearing);
        if x < left || x > left + width {
            continue;
        }
        let major = step % 6 == 0;
        let tick = if major { height * 0.35 } else { height * 0.2 };
        canvas.line_aa(
            x,
            top + height - tick,
            x,
            top + height,
            Rgba::new(230, 236, 245, 200),
        );
        if major {
            let label = ["N", "E", "S", "W"][step / 6];
            canvas.text(
                font,
                x as i32 - 2,
                top as i32 + 1,
                label,
                Rgba::rgb(255, 226, 160),
            );
        }
    }
    for &(bearing, color) in marks {
        let x = x_of(bearing).clamp(left, left + width);
        let y = top + height;
        canvas.fill_triangle(
            (x, y + 1.0),
            (x - 2.5 * u, y + 4.0 * u),
            (x + 2.5 * u, y + 4.0 * u),
            color,
        );
    }
    // The heading itself.
    let mid = left + width / 2.0;
    canvas.line_aa(mid, top, mid, top + height, Rgba::rgb(255, 120, 80));
}

/// A round radar in the lower right: the craft in the middle, heading up, and `blips`
/// (position, color, whether it is the next ring) within `range` meters.
pub fn radar(
    canvas: &mut Canvas,
    position: Vec3,
    yaw: f32,
    blips: &[(Vec3, Rgba, bool)],
    range: f32,
    time: f32,
) {
    let u = unit(canvas);
    let radius = 22.0 * u;
    let cx = canvas.width() as f32 - radius - 4.0 * u;
    let cy = canvas.height() as f32 - radius - 4.0 * u;
    canvas.set_blend_mode(BlendMode::Alpha);
    canvas.fill_circle(
        cx as i32,
        cy as i32,
        radius as u32,
        Rgba::new(8, 16, 26, 150),
    );
    canvas.circle(
        cx as i32,
        cy as i32,
        radius as u32,
        Rgba::new(120, 210, 255, 160),
    );
    canvas.circle(
        cx as i32,
        cy as i32,
        (radius / 2.0) as u32,
        Rgba::new(120, 210, 255, 60),
    );
    // The sweep.
    let sweep = time * 2.0;
    canvas.line_aa(
        cx,
        cy,
        cx + sweep.cos() * radius,
        cy + sweep.sin() * radius,
        Rgba::new(120, 210, 255, 70),
    );
    let (sin, cos) = yaw.sin_cos();
    for &(point, color, next) in blips {
        let offset = sub(point, position);
        // Turn the world so the heading points up.
        let right = offset[0] * cos + offset[2] * sin;
        let ahead = offset[0] * sin - offset[2] * cos;
        let distance = right.hypot(ahead);
        let scale = if distance > range {
            range / distance
        } else {
            1.0
        };
        let (x, y) = (
            cx + right / range * radius * scale,
            cy - ahead / range * radius * scale,
        );
        let size = if next { 2.5 * u } else { 1.5 * u };
        let color = if next && (time * 4.0).sin() < 0.0 {
            Rgba::WHITE
        } else {
            color
        };
        canvas.fill_circle(x as i32, y as i32, size.max(1.0) as u32, color);
    }
    // The craft.
    canvas.fill_triangle(
        (cx, cy - 3.0 * u),
        (cx - 2.0 * u, cy + 2.0 * u),
        (cx + 2.0 * u, cy + 2.0 * u),
        Rgba::rgb(255, 240, 220),
    );
}

/// An arrow at the edge of the picture pointing to `target` when it is not in view.
pub fn arrow(canvas: &mut Canvas, camera: &Camera, target: Vec3, color: Rgba) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    if let Some((x, y, _)) = camera.project(target, w, h) {
        if (0.0..w).contains(&x) && (0.0..h).contains(&y) {
            return;
        }
    }
    let offset = sub(target, camera.eye);
    let right = crate::math::dot(offset, camera.right());
    let up = crate::math::dot(offset, camera.up);
    let angle = (-up).atan2(right);
    let (sin, cos) = angle.sin_cos();
    let u = unit(canvas);
    let (cx, cy) = (w / 2.0, h / 2.0);
    // On an ellipse inside the picture's edges.
    let (x, y) = (cx + cos * (cx - 12.0 * u), cy + sin * (cy - 12.0 * u));
    let size = 6.0 * u;
    let tip = (x + cos * size, y + sin * size);
    let wing = |s: f32| {
        (
            x - cos * size * 0.4 - sin * size * 0.6 * s,
            y - sin * size * 0.4 + cos * size * 0.6 * s,
        )
    };
    canvas.set_blend_mode(BlendMode::Alpha);
    canvas.fill_triangle(tip, wing(1.0), wing(-1.0), color.with_alpha(220));
}

/// The cockpit's sight: a small circle and a level line that tilts with the bank.
pub fn sight(canvas: &mut Canvas, roll: f32) {
    let u = unit(canvas);
    let (cx, cy) = (canvas.width() as f32 / 2.0, canvas.height() as f32 / 2.0);
    let color = Rgba::new(160, 255, 190, 200);
    canvas.set_blend_mode(BlendMode::Alpha);
    canvas.circle(cx as i32, cy as i32, (3.0 * u) as u32, color);
    let (sin, cos) = (-roll).sin_cos();
    for side in [-1.0_f32, 1.0] {
        let (a, b) = (8.0 * u * side, 22.0 * u * side);
        canvas.line_aa(
            cx + cos * a,
            cy + sin * a,
            cx + cos * b,
            cy + sin * b,
            color,
        );
    }
}

/// A white flash over the whole picture (ring passed), `strength` 0..1.
pub fn flash(canvas: &mut Canvas, strength: f32, color: Rgba) {
    if strength <= 0.0 {
        return;
    }
    canvas.set_blend_mode(BlendMode::Add);
    let (w, h) = (canvas.width(), canvas.height());
    canvas.fill_rect(0, 0, w, h, color.with_alpha((strength * 90.0) as u8));
    canvas.set_blend_mode(BlendMode::Alpha);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::direction;
    use vnce::vnce_game::Particle;

    #[test]
    fn instruments_draw_inside_the_picture() {
        let mut canvas = Canvas::filled(200, 120, Rgba::rgb(40, 80, 120));
        let camera =
            crate::math::camera([0.0, 50.0, 0.0], direction(0.0, 0.0), [0.0, 1.0, 0.0], 1.6);
        compass(&mut canvas, 0.3, &[(1.0, Rgba::rgb(255, 200, 60))]);
        radar(
            &mut canvas,
            [0.0; 3],
            0.3,
            &[([100.0, 0.0, -300.0], Rgba::rgb(255, 200, 60), true)],
            900.0,
            1.0,
        );
        arrow(
            &mut canvas,
            &camera,
            [0.0, 50.0, 100.0],
            Rgba::rgb(255, 200, 60),
        );
        sight(&mut canvas, 0.4);
        speed_lines(&mut canvas, 1.0, 0.5);
        trail(
            &mut canvas,
            &camera,
            &[([1.0, 49.0, -20.0], 0.5), ([1.0, 49.0, -30.0], 0.2)],
            1.0,
        );
        let mut emitter = spark_emitter();
        emitter.spawn(Particle::new(
            [0.0, 50.0, -20.0],
            [1.0, 2.0, 0.0],
            1.0,
            Rgba::rgb(255, 160, 60),
        ));
        emitter.update(std::time::Duration::from_millis(100));
        sparks(&mut canvas, &camera, &emitter);
        flash(&mut canvas, 0.5, Rgba::WHITE);
        let changed = canvas
            .pixels()
            .iter()
            .filter(|&&p| p != Rgba::rgb(40, 80, 120))
            .count();
        assert!(changed > 1000, "{changed}");
    }
}
