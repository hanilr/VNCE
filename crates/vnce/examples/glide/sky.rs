//! The sky and the time of day.
//!
//! A flight runs from sunrise towards sunset: [`daylight`] turns the day's progress into
//! the sun's direction and color, the ambient light and the color of the haze, which the
//! mesh renderer uses for its light and fog.
//!
//! The sky is a panorama (azimuth across, elevation down) that every pixel of the picture
//! looks up with its view direction, so it turns and tilts with the camera. The GPU draws
//! it with a `ShaderQuad`, with clouds; the CPU draws a simpler one without clouds. A
//! `ShaderQuad` only receives the time and a frame number, so the game passes the day's
//! progress in the frame number.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use vnce::vnce_raster::{Canvas, Rgba};

use crate::math::{Vec3, dot, normalize};

/// Panorama size on the CPU (the GPU draws a larger one).
pub const CPU_PANORAMA: (u32, u32) = (256, 96);
#[cfg_attr(
    not(feature = "gpu"),
    allow(dead_code, reason = "the GPU renderer uses it")
)]
pub const GPU_PANORAMA: (u32, u32) = (768, 288);
/// The panorama spans from straight up down to this far below the horizon.
const LOWEST: f32 = 0.35;

/// The frame number the GPU sky receives for a day's progress.
#[cfg_attr(
    not(feature = "gpu"),
    allow(dead_code, reason = "the GPU renderer uses it")
)]
pub fn phase_frame(phase: f32) -> u32 {
    (phase.clamp(0.0, 1.0) * 100_000.0) as u32
}

/// Light and colors at one moment of the day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Daylight {
    /// From 0 (sunrise) to 1 (sunset).
    pub phase: f32,
    /// Unit vector towards the sun.
    pub sun: Vec3,
    /// Linear RGB.
    pub sun_color: [f32; 3],
    pub ambient: [f32; 3],
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    /// How golden the light is (1 at sunrise and sunset, 0 by day).
    pub golden: f32,
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// The sun rises in the east (+x), passes high in the south (+z) and sets in the west.
pub fn sun_direction(phase: f32) -> Vec3 {
    let a = phase * PI;
    normalize([a.cos(), a.sin() * 0.9 + 0.02, a.sin() * 0.45])
}

/// The light at `phase` (0 sunrise, 1 sunset).
pub fn daylight(phase: f32) -> Daylight {
    let sun = sun_direction(phase);
    let e = sun[1];
    let day = smoothstep(-0.02, 0.4, e);
    let golden = 1.0 - smoothstep(0.03, 0.32, e);
    let visible = smoothstep(-0.06, 0.05, e);
    Daylight {
        phase,
        sun,
        sun_color: mix([1.15, 0.62, 0.34], [1.05, 1.0, 0.92], day).map(|c| c * visible),
        ambient: mix([0.26, 0.2, 0.28], [0.34, 0.38, 0.46], day),
        zenith: mix([0.07, 0.09, 0.24], [0.11, 0.27, 0.64], day),
        horizon: mix([0.62, 0.42, 0.36], [0.6, 0.72, 0.86], day),
        golden,
    }
}

impl Daylight {
    /// The sky color (linear) looking along the unit vector `dir`, without clouds.
    pub fn sky(&self, dir: Vec3) -> [f32; 3] {
        let up = dir[1];
        let toward = dot(dir, self.sun).max(0.0);
        let glow = toward.powi(6) * self.golden;
        let height = up.max(0.0).powf(0.45);
        let mut color = mix(self.horizon, self.zenith, height);
        let warm = [0.95, 0.42, 0.18];
        let low = 1.0 - (up * 2.5).clamp(0.0, 1.0);
        color = [0, 1, 2].map(|i| color[i] + warm[i] * glow * low * 0.9);
        if up < 0.0 {
            // Below the horizon: haze over the far land.
            color = mix(color, self.haze(), smoothstep(0.0, -0.08, up));
        }
        // The sun and its halo.
        let halo = toward.powi(300) * 0.7 + toward.powi(40) * 0.18 * (1.0 - self.golden * 0.5);
        let disk = smoothstep(0.9993, 0.9996, toward) * 6.0;
        [0, 1, 2].map(|i| color[i] + self.sun_color[i] * (halo + disk))
    }

    /// The color of the haze far away, which the fog fades to.
    pub fn haze(&self) -> [f32; 3] {
        let warm = [0.95, 0.42, 0.18];
        [0, 1, 2].map(|i| self.horizon[i] * 0.92 + warm[i] * self.golden * 0.12)
    }

    #[cfg_attr(
        not(feature = "gpu"),
        allow(dead_code, reason = "the GPU renderer uses it")
    )]
    pub fn haze_rgba(&self) -> Rgba {
        srgb(self.haze())
    }
}

/// A linear color as an opaque sRGB pixel.
pub fn srgb(linear: [f32; 3]) -> Rgba {
    let c = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.003_130_8 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0 + 0.5) as u8
    };
    Rgba::rgb(c(linear[0]), c(linear[1]), c(linear[2]))
}

/// An sRGB channel in linear light, by table.
pub fn linear(channel: u8) -> f32 {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let s = i as f32 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        })
    })[usize::from(channel)]
}

/// The direction of panorama point `(u, v)`, both in `0..1`.
pub fn panorama_direction(u: f32, v: f32) -> Vec3 {
    let azimuth = u * TAU;
    let elevation = FRAC_PI_2 - v * (FRAC_PI_2 + LOWEST);
    let (se, ce) = elevation.sin_cos();
    [azimuth.sin() * ce, se, -azimuth.cos() * ce]
}

/// Where the (not necessarily unit) direction `dir` is in the panorama.
pub fn panorama_point(dir: Vec3) -> (f32, f32) {
    let azimuth = dir[0].atan2(-dir[2]);
    let elevation = dir[1].atan2(dir[0].hypot(dir[2]));
    (
        (azimuth / TAU).rem_euclid(1.0),
        ((FRAC_PI_2 - elevation) / (FRAC_PI_2 + LOWEST)).clamp(0.0, 1.0),
    )
}

/// Draws the CPU panorama for `light`.
pub fn cpu_panorama(canvas: &mut Canvas, light: &Daylight) {
    let (width, height) = CPU_PANORAMA;
    if (canvas.width(), canvas.height()) != (width, height) {
        *canvas = Canvas::new(width, height);
    }
    for (index, pixel) in canvas.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (index as u32 % width, index as u32 / width);
        let dir = panorama_direction(
            (x as f32 + 0.5) / width as f32,
            (y as f32 + 0.5) / height as f32,
        );
        *pixel = srgb(light.sky(dir));
    }
}

/// The panorama color at `(u, v)`, blended between the four nearest pixels (wrapping
/// around in u).
pub fn sample(panorama: &Canvas, u: f32, v: f32) -> Rgba {
    let (width, height) = (panorama.width() as i32, panorama.height() as i32);
    if width == 0 || height == 0 {
        return Rgba::BLACK;
    }
    let x = u * width as f32 - 0.5;
    let y = (v * height as f32 - 0.5).clamp(0.0, (height - 1) as f32);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i32, y0 as i32);
    let pixels = panorama.pixels();
    let at = |x: i32, y: i32| pixels[(y.min(height - 1) * width + x.rem_euclid(width)) as usize];
    let top = at(x0, y0).lerp(at(x0 + 1, y0), fx);
    let bottom = at(x0, y0 + 1).lerp(at(x0 + 1, y0 + 1), fx);
    top.lerp(bottom, fy)
}

/// The GPU sky: the same colors as [`Daylight::sky`], with drifting clouds lit by the sun.
/// The day's progress arrives in `globals.frame` (see [`phase_frame`]).
#[cfg_attr(
    not(feature = "gpu"),
    allow(dead_code, reason = "the GPU renderer uses it")
)]
pub const SKY_SHADER: &str = "
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
               mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        value += amplitude * noise(q);
        q = q * 2.07 + vec2<f32>(3.1, 1.7);
        amplitude *= 0.5;
    }
    return value;
}

fn ease(a: f32, b: f32, x: f32) -> f32 {
    return smoothstep(a, b, x);
}

// The sky (linear color) along the unit vector `dir` at day progress `phase`.
fn sky(dir: vec3<f32>, phase: f32, time: f32) -> vec3<f32> {
    let pi = 3.14159265;
    let a = phase * pi;
    let sun = normalize(vec3<f32>(cos(a), sin(a) * 0.9 + 0.02, sin(a) * 0.45));
    let e = sun.y;
    let day = ease(-0.02, 0.4, e);
    let golden = 1.0 - ease(0.03, 0.32, e);
    let visible = ease(-0.06, 0.05, e);
    let sun_color = mix(vec3<f32>(1.15, 0.62, 0.34), vec3<f32>(1.05, 1.0, 0.92), day) * visible;
    let ambient = mix(vec3<f32>(0.26, 0.2, 0.28), vec3<f32>(0.34, 0.38, 0.46), day);
    let zenith = mix(vec3<f32>(0.07, 0.09, 0.24), vec3<f32>(0.11, 0.27, 0.64), day);
    let horizon = mix(vec3<f32>(0.62, 0.42, 0.36), vec3<f32>(0.6, 0.72, 0.86), day);
    let warm = vec3<f32>(0.95, 0.42, 0.18);

    let up = dir.y;
    let toward = max(dot(dir, sun), 0.0);
    let glow = pow(toward, 6.0) * golden;
    var color = mix(horizon, zenith, pow(max(up, 0.0), 0.45));
    color += warm * glow * (1.0 - clamp(up * 2.5, 0.0, 1.0)) * 0.9;
    let haze = horizon * 0.92 + warm * golden * 0.12;
    if (up < 0.0) {
        color = mix(color, haze, ease(0.0, -0.08, up));
    }

    // Clouds on a flat layer above, drifting with the wind, thinning towards the horizon.
    if (up > 0.01) {
        let p = dir.xz / (up + 0.08) * 1.4 + vec2<f32>(time * 0.012, time * 0.004);
        let n = fbm(p);
        let cover = ease(0.5, 0.78, n) * ease(0.01, 0.22, up);
        let lit = 0.55 + 0.45 * ease(0.4, 0.8, fbm(p + sun.xz * 0.35));
        let cloud = ambient * 1.5 + sun_color * lit * 0.75 + warm * glow * 0.6;
        color = mix(color, cloud, cover * 0.9);
    }

    let halo = pow(toward, 300.0) * 0.7 + pow(toward, 40.0) * 0.18 * (1.0 - golden * 0.5);
    let disk = ease(0.9993, 0.9996, toward) * 6.0;
    color += sun_color * (halo + disk);
    return color;
}

fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
    let pi = 3.14159265;
    let azimuth = uv.x * 2.0 * pi;
    let elevation = pi / 2.0 - uv.y * (pi / 2.0 + 0.35);
    let dir = vec3<f32>(sin(azimuth) * cos(elevation), sin(elevation), -cos(azimuth) * cos(elevation));
    return vec4<f32>(sky(dir, f32(globals.frame) / 100000.0, time), 1.0);
}
";

/// The same sky drawn straight into the picture (for a native window, where nothing is
/// read back): every pixel's view direction comes from the camera in `globals.params`
/// (see [`screen_params`]).
#[cfg_attr(
    not(feature = "gpu"),
    allow(dead_code, reason = "the GPU renderer uses it")
)]
pub fn screen_sky_shader() -> String {
    let common = SKY_SHADER.split("fn shade(").next().unwrap_or(SKY_SHADER);
    format!(
        "{common}
fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {{
    let s = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let dir = normalize(globals.params[0].xyz + globals.params[1].xyz * s.x + globals.params[2].xyz * s.y);
    return vec4<f32>(sky(dir, globals.params[3].x, time), 1.0);
}}
"
    )
}

/// The camera and the day's progress for [`screen_sky_shader`]: forward, right and up
/// (scaled to the picture's edges), and the phase.
#[cfg_attr(
    not(feature = "gpu"),
    allow(dead_code, reason = "the GPU renderer uses it")
)]
pub fn screen_params(camera: &crate::math::Camera, phase: f32) -> [[f32; 4]; 4] {
    let tan = (crate::math::FOV_Y / 2.0).tan();
    let right = crate::math::scale(camera.right(), tan * camera.aspect);
    let up = crate::math::scale(camera.up, tan);
    let f = camera.forward;
    [
        [f[0], f[1], f[2], 0.0],
        [right[0], right[1], right[2], 0.0],
        [up[0], up[1], up[2], 0.0],
        [phase, 0.0, 0.0, 0.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_crosses_the_sky_from_east_to_west() {
        let rise = sun_direction(0.0);
        let noon = sun_direction(0.5);
        let set = sun_direction(1.0);
        assert!(rise[0] > 0.9 && set[0] < -0.9);
        assert!(noon[1] > 0.8);
        assert!(daylight(0.5).golden < 0.1 && daylight(0.02).golden > 0.9);
    }

    #[test]
    fn panorama_points_and_directions_agree() {
        for (u, v) in [(0.1, 0.2), (0.6, 0.5), (0.9, 0.8)] {
            let dir = panorama_direction(u, v);
            let (u2, v2) = panorama_point(dir);
            assert!((u - u2).abs() < 1e-4 && (v - v2).abs() < 1e-4);
        }
        // North is at u = 0, east at a quarter.
        assert!(panorama_point([0.0, 0.0, -1.0]).0 < 1e-4);
        assert!((panorama_point([1.0, 0.0, 0.0]).0 - 0.25).abs() < 1e-4);
    }

    #[test]
    fn the_sky_is_brighter_up_by_day_and_near_the_sun() {
        let light = daylight(0.5);
        let mut canvas = Canvas::new(0, 0);
        cpu_panorama(&mut canvas, &light);
        let (u, v) = panorama_point(light.sun);
        let sun = sample(&canvas, u, v);
        let away = sample(&canvas, (u + 0.5) % 1.0, v);
        assert!(sun.luminance() > away.luminance());
        assert_eq!(srgb([2.0, -1.0, 0.5]), Rgba::rgb(255, 0, 188));
        assert!((linear(188) - 0.5).abs() < 0.01);
    }
}
