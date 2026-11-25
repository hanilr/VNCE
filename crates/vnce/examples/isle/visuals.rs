//! The ocean around the island: a seamless picture of waves that tiles the sea, made by a
//! GPU fragment shader with the `gpu` feature (read back pipelined) and on the CPU
//! otherwise. The game places the tiles under the camera, so the sea moves with the world
//! (a `ShaderQuad` only knows the time).

use vnce::vnce_raster::{Canvas, Rgba};

/// Size of the ocean picture in world pixels; it repeats seamlessly in both directions.
pub const OCEAN_WIDTH: u32 = 128;
pub const OCEAN_HEIGHT: u32 = 64;

/// The ocean's colors from deep to foam, shared by the CPU and the GPU (pixel art steps).
const SHADES: [Rgba; 5] = [
    Rgba::rgb(22, 72, 118),
    Rgba::rgb(28, 86, 136),
    Rgba::rgb(36, 104, 154),
    Rgba::rgb(60, 136, 178),
    Rgba::rgb(170, 214, 228),
];

pub struct Visuals {
    cpu: Canvas,
    #[cfg(feature = "gpu")]
    gpu: Option<gpu::GpuOcean>,
    use_gpu: bool,
    /// Why there is no GPU, when there is none.
    note: String,
}

impl Visuals {
    /// Starts the GPU when `want_gpu` is set and the `gpu` feature is on.
    pub fn new(want_gpu: bool) -> Self {
        #[cfg(feature = "gpu")]
        {
            let (gpu, note) = if want_gpu {
                match gpu::GpuOcean::new() {
                    Ok(gpu) => (Some(gpu), String::new()),
                    Err(error) => (None, error),
                }
            } else {
                (None, "CPU chosen".to_owned())
            };
            Self {
                cpu: Canvas::new(OCEAN_WIDTH, OCEAN_HEIGHT),
                use_gpu: gpu.is_some(),
                gpu,
                note,
            }
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = want_gpu;
            Self {
                cpu: Canvas::new(OCEAN_WIDTH, OCEAN_HEIGHT),
                use_gpu: false,
                note: "build with --features gpu".to_owned(),
            }
        }
    }

    #[cfg_attr(
        not(feature = "gpu"),
        allow(clippy::unused_self, reason = "the GPU version uses `self`")
    )]
    pub fn gpu_available(&self) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
        }
        #[cfg(not(feature = "gpu"))]
        {
            false
        }
    }

    pub fn using_gpu(&self) -> bool {
        self.use_gpu && self.gpu_available()
    }

    /// Switches between GPU and CPU (only to the GPU when there is one).
    pub fn set_gpu(&mut self, on: bool) {
        self.use_gpu = on && self.gpu_available();
    }

    /// The renderer in use, for the screen.
    pub fn describe(&self) -> String {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = self.gpu.as_ref().filter(|_| self.use_gpu) {
            return format!("GPU {}", gpu.name);
        }
        if self.gpu_available() {
            "CPU (GPU off)".to_owned()
        } else {
            format!("CPU ({})", self.note)
        }
    }

    /// The ocean picture at `time` seconds.
    pub fn ocean(&mut self, time: f32) -> &Canvas {
        #[cfg(feature = "gpu")]
        if self.use_gpu {
            if let Some(gpu) = &mut self.gpu {
                return gpu.ocean(time);
            }
        }
        cpu_ocean(&mut self.cpu, time);
        &self.cpu
    }
}

/// A repeating hash on a grid of `period` cells, in 0..1.
fn cell_hash(x: i32, y: i32, period: (i32, i32)) -> f32 {
    let (x, y) = (x.rem_euclid(period.0), y.rem_euclid(period.1));
    let mut h = (x as u32).wrapping_mul(0x27D4_EB2D) ^ (y as u32).wrapping_mul(0x1656_67B1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// Smooth noise that repeats every `period` cells.
fn periodic_noise(x: f32, y: f32, period: (i32, i32)) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let a = cell_hash(ix, iy, period);
    let b = cell_hash(ix + 1, iy, period);
    let c = cell_hash(ix, iy + 1, period);
    let d = cell_hash(ix + 1, iy + 1, period);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// Waves running diagonally like the island's edges, broken up by drifting noise, with
/// glints on the crests; quantized to the ocean's shades.
fn cpu_ocean(canvas: &mut Canvas, time: f32) {
    use std::f32::consts::TAU;
    let (width, height) = (OCEAN_WIDTH as f32, OCEAN_HEIGHT as f32);
    for (index, pixel) in canvas.pixels_mut().iter_mut().enumerate() {
        let x = (index as u32 % OCEAN_WIDTH) as f32;
        let y = (index as u32 / OCEAN_WIDTH) as f32;
        let (u, v) = (x / width, y / height);
        // Two wave trains whose periods divide the picture, so it repeats seamlessly.
        let wave = (TAU * (u * 2.0 + v * 2.0) - time * 1.1).sin() * 0.5
            + (TAU * (u * 3.0 - v * 1.0) + time * 0.7).sin() * 0.3;
        let drift = periodic_noise(u * 8.0 + time * 0.15, v * 4.0, (8, 4));
        let n = wave * 0.35 + drift * 0.75;
        let mut shade = ((n + 0.35) * 2.6).clamp(0.0, 3.0) as usize;
        // Glints: a few crest pixels flash white.
        let glint = cell_hash(
            x as i32,
            y as i32,
            (OCEAN_WIDTH as i32, OCEAN_HEIGHT as i32),
        );
        if shade == 3 && glint > 0.97 && (time * 2.0 + glint * 40.0).sin() > 0.6 {
            shade = 4;
        }
        *pixel = SHADES[shade];
    }
}

// ---------------------------------------------------------------------------------------
// GPU
// ---------------------------------------------------------------------------------------

#[cfg(feature = "gpu")]
mod gpu {
    use vnce::vnce_gpu::color::linear_rgba;
    use vnce::vnce_gpu::{Gpu, GpuOptions, Readback, RenderTarget, ShaderQuad};
    use vnce::vnce_raster::Canvas;

    use super::{OCEAN_HEIGHT, OCEAN_WIDTH, SHADES, cpu_ocean};

    /// The same ocean as the CPU's, with finer drifting noise (three octaves) and softer
    /// glints; it repeats over the picture like the CPU one.
    const OCEAN: &str = "
fn cell_hash(c: vec2<f32>, period: vec2<f32>) -> f32 {
    let p = c - period * floor(c / period);
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>, period: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = cell_hash(i, period);
    let b = cell_hash(i + vec2<f32>(1.0, 0.0), period);
    let c = cell_hash(i + vec2<f32>(0.0, 1.0), period);
    let d = cell_hash(i + vec2<f32>(1.0, 1.0), period);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
    let tau = 6.2831853;
    let wave = sin(tau * (uv.x * 2.0 + uv.y * 2.0) - time * 1.1) * 0.5
        + sin(tau * (uv.x * 3.0 - uv.y) + time * 0.7) * 0.3;
    var drift = noise(vec2<f32>(uv.x * 8.0 + time * 0.15, uv.y * 4.0), vec2<f32>(8.0, 4.0)) * 0.55;
    drift += noise(vec2<f32>(uv.x * 16.0 - time * 0.25, uv.y * 8.0 + 3.0), vec2<f32>(16.0, 8.0)) * 0.3;
    drift += noise(vec2<f32>(uv.x * 32.0, uv.y * 16.0 - time * 0.4), vec2<f32>(32.0, 16.0)) * 0.15;
    let n = wave * 0.35 + drift * 0.8;
    let level = clamp(floor((n + 0.35) * 2.6), 0.0, 3.0);
    var color = ocean_shade(u32(level));
    let pixel = floor(uv * resolution);
    let glint = cell_hash(pixel, resolution);
    if (level >= 3.0 && glint > 0.965 && sin(time * 2.0 + glint * 40.0) > 0.5) {
        color = ocean_shade(4u);
    }
    return vec4<f32>(color, 1.0);
}
";

    pub struct GpuOcean {
        gpu: Gpu,
        quad: ShaderQuad,
        target: RenderTarget,
        canvas: Canvas,
        pub name: String,
    }

    impl GpuOcean {
        pub fn new() -> Result<Self, String> {
            let gpu = Gpu::new(GpuOptions::default().with_env()).map_err(|e| e.to_string())?;
            // The shades are written into the shader (as linear colors, which the target
            // stores as sRGB), so both renderers use the same colors.
            let cases: Vec<String> = SHADES
                .iter()
                .enumerate()
                .map(|(index, &color)| {
                    let [r, g, b, _] = linear_rgba(color);
                    format!(
                        "        case {index}u: {{ return vec3<f32>({r:.5}, {g:.5}, {b:.5}); }}"
                    )
                })
                .collect();
            let source = format!(
                "fn ocean_shade(index: u32) -> vec3<f32> {{\n    switch index {{\n{}\n        default: {{ return vec3<f32>(0.0); }}\n    }}\n}}\n{OCEAN}",
                cases.join("\n")
            );
            let quad = ShaderQuad::new(&gpu, &source).map_err(|e| e.to_string())?;
            // Pipelined readback: the CPU never waits for the GPU (one frame of latency).
            let target = RenderTarget::new(&gpu, OCEAN_WIDTH, OCEAN_HEIGHT)
                .with_readback(Readback::Pipelined);
            Ok(Self {
                name: gpu.info().to_string(),
                gpu,
                quad,
                target,
                canvas: Canvas::new(0, 0),
            })
        }

        pub fn ocean(&mut self, time: f32) -> &Canvas {
            self.quad.render(&self.gpu, &self.target, time, 0);
            // A failed readback keeps the last picture.
            let _ = self.target.read_into(&self.gpu, &mut self.canvas);
            // The first picture arrives a frame later: until then, the CPU's.
            if (self.canvas.width(), self.canvas.height()) != (OCEAN_WIDTH, OCEAN_HEIGHT) {
                self.canvas = Canvas::new(OCEAN_WIDTH, OCEAN_HEIGHT);
                cpu_ocean(&mut self.canvas, time);
            }
            &self.canvas
        }
    }
}
