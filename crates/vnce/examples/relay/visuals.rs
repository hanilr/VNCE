//! Pictures made on the GPU when there is one (feature `gpu`), and on the CPU otherwise:
//! the nebula behind the observatory window and the title, and the reactor's 3D core.

use vnce::vnce_raster::{Canvas, Rgba};

/// The two nebula pictures kept apart, so neither is resized every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Window,
    Backdrop,
}

impl Slot {
    fn index(self) -> usize {
        match self {
            Self::Window => 0,
            Self::Backdrop => 1,
        }
    }
}

pub struct Visuals {
    cpu: Cpu,
    #[cfg(feature = "gpu")]
    gpu: Option<gpu::GpuVisuals>,
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
                match gpu::GpuVisuals::new() {
                    Ok(gpu) => (Some(gpu), String::new()),
                    Err(error) => (None, error),
                }
            } else {
                (None, "CPU chosen".to_owned())
            };
            Self {
                cpu: Cpu::new(),
                use_gpu: gpu.is_some(),
                gpu,
                note,
            }
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = want_gpu;
            Self {
                cpu: Cpu::new(),
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

    /// The nebula at `width × height` pixels, at `time` seconds.
    pub fn nebula(&mut self, slot: Slot, width: u32, height: u32, time: f32) -> &Canvas {
        #[cfg(feature = "gpu")]
        if self.use_gpu {
            if let Some(gpu) = &mut self.gpu {
                return gpu.nebula(slot, width, height, time);
            }
        }
        self.cpu.nebula(slot, width, height, time)
    }

    /// The reactor core in a `size × size` picture with a transparent background.
    pub fn core(&mut self, size: u32, time: f32) -> &Canvas {
        #[cfg(feature = "gpu")]
        if self.use_gpu {
            if let Some(gpu) = &mut self.gpu {
                return gpu.core(size, time);
            }
        }
        self.cpu.core(size, time)
    }
}

// ---------------------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------------------

/// Side of the tiling noise table.
const NOISE: usize = 64;

struct Cpu {
    /// Tiling fractal noise in 0..1.
    noise: Vec<f32>,
    /// Nebula colors from dark to bright.
    palette: Vec<Rgba>,
    nebulas: [Canvas; 2],
    core: Canvas,
}

impl Cpu {
    fn new() -> Self {
        Self {
            noise: fractal_noise(),
            palette: (0..256).map(|i| nebula_color(i as f32 / 255.0)).collect(),
            nebulas: [Canvas::new(0, 0), Canvas::new(0, 0)],
            core: Canvas::new(0, 0),
        }
    }

    /// Two layers of the noise table drifting apart, through the palette, with stars.
    fn nebula(&mut self, slot: Slot, width: u32, height: u32, time: f32) -> &Canvas {
        let canvas = &mut self.nebulas[slot.index()];
        if (canvas.width(), canvas.height()) != (width, height) {
            *canvas = Canvas::new(width, height);
        }
        let mask = NOISE - 1;
        let near = (time * 5.0) as usize;
        let far = (time * 2.0) as usize;
        let twinkle = (time * 3.0) as usize;
        let noise = &self.noise;
        let palette = &self.palette;
        for (y, row) in canvas
            .pixels_mut()
            .chunks_exact_mut(width as usize)
            .enumerate()
        {
            let near_row = ((y + near / 3) & mask) * NOISE;
            let far_row = ((y / 2 + 17) & mask) * NOISE;
            for (x, pixel) in row.iter_mut().enumerate() {
                let a = noise[near_row + ((x + near) & mask)];
                let b = noise[far_row + ((x / 2).wrapping_sub(far) & mask)];
                let n = (a * 0.55 + b * 0.45).clamp(0.0, 1.0);
                *pixel = palette[(n * n * 255.0) as usize];
                // Stars: a few fixed pixels, some of them blinking.
                let hash =
                    (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663);
                if hash.is_multiple_of(211) && !(hash as usize / 211 + twinkle).is_multiple_of(5) {
                    *pixel = Rgba::rgb(230, 235, 255);
                }
            }
        }
        canvas
    }

    /// A shaded octahedron with eight orbiting sparks, painted back to front.
    fn core(&mut self, size: u32, time: f32) -> &Canvas {
        let canvas = &mut self.core;
        if (canvas.width(), canvas.height()) != (size, size) {
            *canvas = Canvas::new(size, size);
        }
        canvas.clear(Rgba::TRANSPARENT);
        let half = size as f32 / 2.0;
        let (sy, cy) = (time * 0.9).sin_cos();
        let (sx, cx) = (time * 0.55).sin_cos();
        let rotate = |[x, y, z]: [f32; 3]| {
            let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
            let (y, z) = (y * cx - z * sx, y * sx + z * cx);
            [x, y, z]
        };
        let project = |[x, y, z]: [f32; 3]| {
            let scale = half * 0.62 / (1.0 + z * 0.25 + 1.2) * 2.2;
            (half + x * scale, half - y * scale, z)
        };
        let corners = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.3, 0.0],
            [0.0, -1.3, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ]
        .map(rotate);
        let faces: [[usize; 3]; 8] = [
            [0, 2, 4],
            [4, 2, 1],
            [1, 2, 5],
            [5, 2, 0],
            [0, 4, 3],
            [4, 1, 3],
            [1, 5, 3],
            [5, 0, 3],
        ];
        let light = [0.4_f32, 0.7, -0.6];
        // Sparks behind the core first, then the faces, then sparks in front.
        let sparks: Vec<(f32, f32, f32)> = (0..8)
            .map(|i| {
                let angle = time * 1.6 + i as f32 * std::f32::consts::TAU / 8.0;
                let tilt = 0.35 * (time * 0.7 + i as f32).sin();
                project(rotate([1.7 * angle.cos(), tilt, 1.7 * angle.sin()]))
            })
            .collect();
        let spark = |canvas: &mut Canvas, (x, y, _): (f32, f32, f32)| {
            canvas.fill_circle(x as i32, y as i32, 1, Rgba::rgb(255, 230, 140));
        };
        for &s in sparks.iter().filter(|s| s.2 > 0.0) {
            spark(canvas, s);
        }
        let mut order: Vec<(f32, [usize; 3])> = faces
            .iter()
            .map(|&f| {
                (
                    (corners[f[0]][2] + corners[f[1]][2] + corners[f[2]][2]) / 3.0,
                    f,
                )
            })
            .collect();
        order.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (index, (_, [a, b, c])) in order.into_iter().enumerate() {
            let (pa, pb, pc) = (corners[a], corners[b], corners[c]);
            let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            let lit = ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / length)
                .abs()
                .mul_add(0.75, 0.25);
            let shade = |base: (f32, f32, f32)| {
                Rgba::rgb(
                    (base.0 * lit).min(255.0) as u8,
                    (base.1 * lit).min(255.0) as u8,
                    (base.2 * lit).min(255.0) as u8,
                )
            };
            let (top, bottom) = if index % 2 == 0 {
                ((255.0, 200.0, 90.0), (255.0, 110.0, 40.0))
            } else {
                ((255.0, 170.0, 70.0), (200.0, 70.0, 30.0))
            };
            let (xa, ya, _) = project(pa);
            let (xb, yb, _) = project(pb);
            let (xc, yc, _) = project(pc);
            canvas.fill_triangle_gradient(
                ((xa, ya), shade(top)),
                ((xb, yb), shade(bottom)),
                ((xc, yc), shade(bottom)),
            );
            canvas.line_aa(xa, ya, xb, yb, Rgba::new(255, 240, 200, 120));
        }
        for &s in sparks.iter().filter(|s| s.2 <= 0.0) {
            spark(canvas, s);
        }
        canvas
    }
}

/// Fractal value noise that tiles over `NOISE × NOISE` cells.
fn fractal_noise() -> Vec<f32> {
    let lattice = |x: usize, y: usize, period: usize| {
        let (x, y) = (x % period, y % period);
        let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        (h & 0xFFFF) as f32 / 65_535.0
    };
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let mut table = vec![0.0; NOISE * NOISE];
    let mut amplitude = 0.55;
    for octave in 0..4 {
        let period = 4 << octave;
        let cell = NOISE / period;
        for y in 0..NOISE {
            for x in 0..NOISE {
                let (cx, cy) = (x / cell, y / cell);
                let fx = smooth((x % cell) as f32 / cell as f32);
                let fy = smooth((y % cell) as f32 / cell as f32);
                let top = lattice(cx, cy, period) * (1.0 - fx) + lattice(cx + 1, cy, period) * fx;
                let bottom =
                    lattice(cx, cy + 1, period) * (1.0 - fx) + lattice(cx + 1, cy + 1, period) * fx;
                table[y * NOISE + x] += amplitude * (top * (1.0 - fy) + bottom * fy);
            }
        }
        amplitude *= 0.5;
    }
    // Stretch to the full range.
    let (min, max) = table
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    table
        .iter()
        .map(|v| (v - min) / (max - min).max(1e-6))
        .collect()
}

/// Deep space, violet clouds, teal gas, a warm glow at the brightest.
fn nebula_color(t: f32) -> Rgba {
    let stops = [
        (0.0, Rgba::rgb(4, 3, 14)),
        (0.35, Rgba::rgb(40, 12, 70)),
        (0.6, Rgba::rgb(120, 40, 150)),
        (0.8, Rgba::rgb(40, 150, 190)),
        (1.0, Rgba::rgb(255, 210, 150)),
    ];
    for pair in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
        if t <= t1 {
            return c0.lerp(c1, (t - t0) / (t1 - t0));
        }
    }
    stops[stops.len() - 1].1
}

// ---------------------------------------------------------------------------------------
// GPU
// ---------------------------------------------------------------------------------------

#[cfg(feature = "gpu")]
mod gpu {
    use vnce::vnce_gpu::{
        Gpu, GpuOptions, Mat4, Mesh, MeshDraw, MeshRenderer, MeshRendererOptions, Readback,
        RenderTarget, ShaderQuad, Vertex,
    };
    use vnce::vnce_raster::{Canvas, Rgba};

    use super::Slot;

    /// The nebula: domain-warped fractal noise, a warm glow and twinkling stars.
    const NEBULA: &str = "
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        value += amplitude * noise(q);
        q = q * 2.03 + vec2<f32>(1.7, 9.2);
        amplitude *= 0.5;
    }
    return value;
}

fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
    let p = (uv - 0.5) * vec2<f32>(globals.aspect, 1.0) * 3.0;
    let drift = vec2<f32>(time * 0.05, time * 0.02);
    let q = vec2<f32>(fbm(p + drift), fbm(p - drift + vec2<f32>(5.2, 1.3)));
    let n = fbm(p + 2.5 * q + drift);
    var color = mix(vec3<f32>(0.02, 0.01, 0.06), vec3<f32>(0.45, 0.12, 0.55), clamp(n * n * 1.8, 0.0, 1.0));
    color = mix(color, vec3<f32>(0.1, 0.55, 0.75), clamp(q.y * q.y * 0.9, 0.0, 1.0));
    color += vec3<f32>(1.0, 0.75, 0.45) * pow(n, 6.0) * 2.5;
    let cell = floor(uv * resolution / 2.0);
    let star = hash(cell);
    let twinkle = 0.6 + 0.4 * sin(time * 3.0 + star * 40.0);
    color += vec3<f32>(step(0.985, star) * twinkle);
    return vec4<f32>(color, 1.0);
}
";

    pub struct GpuVisuals {
        gpu: Gpu,
        nebula: ShaderQuad,
        nebula_targets: [RenderTarget; 2],
        nebula_canvases: [Canvas; 2],
        renderer: MeshRenderer,
        gem: Mesh,
        cube: Mesh,
        core_target: RenderTarget,
        core_canvas: Canvas,
        pub name: String,
    }

    impl GpuVisuals {
        pub fn new() -> Result<Self, String> {
            let gpu = Gpu::new(GpuOptions::default().with_env()).map_err(|e| e.to_string())?;
            let nebula = ShaderQuad::new(&gpu, NEBULA).map_err(|e| e.to_string())?;
            // Pipelined readback: the CPU never waits for the GPU (one frame of latency).
            let target = || RenderTarget::new(&gpu, 1, 1).with_readback(Readback::Pipelined);
            let renderer = MeshRenderer::new(
                &gpu,
                MeshRendererOptions {
                    msaa: 4,
                    cull_back_faces: false,
                },
            );
            Ok(Self {
                name: gpu.info().to_string(),
                nebula_targets: [target(), target()],
                core_target: target(),
                nebula,
                nebula_canvases: [Canvas::new(0, 0), Canvas::new(0, 0)],
                renderer,
                gem: gem(&gpu),
                cube: cube(&gpu),
                core_canvas: Canvas::new(0, 0),
                gpu,
            })
        }

        pub fn nebula(&mut self, slot: Slot, width: u32, height: u32, time: f32) -> &Canvas {
            let index = slot.index();
            let target = &mut self.nebula_targets[index];
            if (target.width(), target.height()) != (width, height) {
                target.resize(&self.gpu, width, height);
            }
            self.nebula.render(&self.gpu, target, time, 0);
            let canvas = &mut self.nebula_canvases[index];
            // A failed readback keeps the last picture.
            let _ = target.read_into(&self.gpu, canvas);
            // Pipelined readback delivers its first picture a frame later (and a resize
            // starts over): until then, deep space of the right size.
            if (canvas.width(), canvas.height()) != (width, height) {
                *canvas = Canvas::filled(width, height, Rgba::rgb(4, 3, 14));
            }
            canvas
        }

        pub fn core(&mut self, size: u32, time: f32) -> &Canvas {
            if (self.core_target.width(), self.core_target.height()) != (size, size) {
                self.core_target.resize(&self.gpu, size, size);
            }
            let camera = Mat4::perspective(40_f32.to_radians(), 1.0, 0.1, 50.0)
                * Mat4::look_at([0.0, 1.2, 5.2], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
            let mut draws = vec![MeshDraw {
                mesh: &self.gem,
                transform: camera
                    * Mat4::rotation_y(time * 0.9)
                    * Mat4::rotation_x(time * 0.55)
                    * Mat4::scale([1.0, 1.3, 1.0]),
            }];
            for i in 0..8 {
                let angle = time * 1.6 + i as f32 * std::f32::consts::TAU / 8.0;
                draws.push(MeshDraw {
                    mesh: &self.cube,
                    transform: camera
                        * Mat4::translation([
                            1.8 * angle.cos(),
                            0.35 * (time * 0.7 + i as f32).sin(),
                            1.8 * angle.sin(),
                        ])
                        * Mat4::rotation_y(angle * 2.0)
                        * Mat4::scale([0.22, 0.22, 0.22]),
                });
            }
            self.renderer
                .render(&self.gpu, &self.core_target, Rgba::TRANSPARENT, &draws);
            let _ = self.core_target.read_into(&self.gpu, &mut self.core_canvas);
            // Until the first picture arrives: nothing (transparent) of the right size.
            if (self.core_canvas.width(), self.core_canvas.height()) != (size, size) {
                self.core_canvas = Canvas::new(size, size);
            }
            &self.core_canvas
        }
    }

    /// An octahedron with faceted warm colors (one color per face).
    fn gem(gpu: &Gpu) -> Mesh {
        let corners: [[f32; 3]; 6] = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        let faces: [[usize; 3]; 8] = [
            [0, 2, 4],
            [4, 2, 1],
            [1, 2, 5],
            [5, 2, 0],
            [0, 4, 3],
            [4, 1, 3],
            [1, 5, 3],
            [5, 0, 3],
        ];
        let colors = [
            Rgba::rgb(255, 210, 100),
            Rgba::rgb(240, 140, 50),
            Rgba::rgb(255, 180, 80),
            Rgba::rgb(210, 100, 40),
            Rgba::rgb(230, 120, 50),
            Rgba::rgb(170, 70, 30),
            Rgba::rgb(200, 90, 40),
            Rgba::rgb(150, 60, 25),
        ];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for (face, color) in faces.iter().zip(colors) {
            let base = vertices.len() as u32;
            for &corner in face {
                vertices.push(Vertex::new(corners[corner], color));
            }
            indices.extend([base, base + 1, base + 2]);
        }
        Mesh::new(gpu, &vertices, &indices)
    }

    /// A unit cube with one color per face.
    fn cube(gpu: &Gpu) -> Mesh {
        let colors = [
            Rgba::rgb(255, 240, 170),
            Rgba::rgb(200, 170, 90),
            Rgba::rgb(255, 230, 140),
            Rgba::rgb(160, 130, 60),
            Rgba::rgb(240, 210, 120),
            Rgba::rgb(180, 150, 70),
        ];
        let axes: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for ((normal, u, v), color) in axes.into_iter().zip(colors) {
            let base = vertices.len() as u32;
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let position = [0, 1, 2].map(|i| 0.5 * (normal[i] + su * u[i] + sv * v[i]));
                vertices.push(Vertex::new(position, color));
            }
            indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        }
        Mesh::new(gpu, &vertices, &indices)
    }
}
