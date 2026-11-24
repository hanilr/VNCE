//! A raymarched scene rendered by a GPU fragment shader (`vnce_gpu::ShaderQuad`), shown with
//! cells or a graphics protocol.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --features gpu --example gpu_shader
//! cargo run --release --features gpu --example gpu_shader -- --bench   # GPU timings
//! ```
//!
//! `G`: next display method · `P`: pipelined readback · `Esc` / `Q`: quit.
//!
//! The demo starts with cells: sending a full-screen Sixel image 60 times per second is
//! too slow for most terminals. Kitty handles it much better.
//! `VNCE_GPU_BACKEND` (`vulkan`, `dx12`, `metal`, `gl`) and `VNCE_GPU_POWER` (`high`, `low`)
//! choose the GPU.

// Demo code converts freely between sizes, counters and floats; every value stays far
// inside the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "frame counters and pixel sizes are small"
)]

use std::time::{Duration, Instant};

use vnce::vnce_core::{Buffer, Color, Rect, Size, Style};
use vnce::vnce_gpu::{Gpu, GpuOptions, Readback, RenderTarget, ShaderQuad};
use vnce::vnce_raster::{Canvas, PixelMode, RenderOptions};
use vnce::vnce_runtime::{self, Context, Game, RunConfig};
use vnce::vnce_term::{Event, Frame, GraphicsMode, KeyCode, KeyEventKind};

/// Spheres over a checkered floor, with soft shadows and fog.
const SCENE: &str = "
// Distance to the closest object (x) and its material (y).
fn scene(p: vec3<f32>, time: f32) -> vec2<f32> {
    let ball = length(p - vec3<f32>(0.0, 0.25 * sin(time * 1.3), 0.0)) - 1.0;
    let orbit = vec3<f32>(2.2 * cos(time), 0.1 + 0.4 * sin(time * 2.0), 2.2 * sin(time));
    let moon = length(p - orbit) - 0.4;
    let ground = p.y + 1.2;
    var best = vec2<f32>(ball, 1.0);
    if moon < best.x {
        best = vec2<f32>(moon, 2.0);
    }
    if ground < best.x {
        best = vec2<f32>(ground, 3.0);
    }
    return best;
}

fn normal_at(p: vec3<f32>, time: f32) -> vec3<f32> {
    let e = vec2<f32>(0.001, 0.0);
    return normalize(vec3<f32>(
        scene(p + e.xyy, time).x - scene(p - e.xyy, time).x,
        scene(p + e.yxy, time).x - scene(p - e.yxy, time).x,
        scene(p + e.yyx, time).x - scene(p - e.yyx, time).x,
    ));
}

fn soft_shadow(origin: vec3<f32>, light: vec3<f32>, time: f32) -> f32 {
    var shadow = 1.0;
    var s = 0.02;
    for (var i = 0; i < 48; i++) {
        let h = scene(origin + light * s, time).x;
        if h < 0.0005 {
            return 0.0;
        }
        shadow = min(shadow, 10.0 * h / s);
        s += h;
        if s > 20.0 {
            break;
        }
    }
    return clamp(shadow, 0.0, 1.0);
}

fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
    // Screen position with y up, corrected for the real aspect ratio of the image.
    let screen = (uv - 0.5) * vec2<f32>(globals.aspect, -1.0) * 2.0;
    let eye = vec3<f32>(4.5 * sin(time * 0.2), 0.8, 4.5 * cos(time * 0.2));
    let forward = normalize(-eye);
    let right = normalize(cross(forward, vec3<f32>(0.0, 1.0, 0.0)));
    let up = cross(right, forward);
    let dir = normalize(forward * 1.6 + right * screen.x + up * screen.y);

    let sky = mix(vec3<f32>(0.55, 0.7, 0.95), vec3<f32>(0.08, 0.12, 0.3), clamp(dir.y * 2.0, 0.0, 1.0));
    var t = 0.0;
    var material = 0.0;
    for (var i = 0; i < 128; i++) {
        let hit = scene(eye + dir * t, time);
        if hit.x < 0.0005 * t {
            material = hit.y;
            break;
        }
        t += hit.x;
        if t > 40.0 {
            break;
        }
    }
    if material == 0.0 {
        return vec4<f32>(sky, 1.0);
    }

    let p = eye + dir * t;
    let n = normal_at(p, time);
    var base = vec3<f32>(0.9, 0.25, 0.15);
    if material == 2.0 {
        base = vec3<f32>(0.2, 0.6, 0.95);
    } else if material == 3.0 {
        let checker = (i32(floor(p.x)) + i32(floor(p.z))) & 1;
        base = select(vec3<f32>(0.85, 0.85, 0.8), vec3<f32>(0.15, 0.15, 0.18), checker == 1);
    }
    let light = normalize(vec3<f32>(0.6, 0.8, 0.4));
    let diffuse = max(dot(n, light), 0.0) * soft_shadow(p + n * 0.002, light, time);
    let half_vector = normalize(light - dir);
    let specular = pow(max(dot(n, half_vector), 0.0), 48.0) * diffuse;
    let ambient = 0.15 + 0.1 * n.y;
    let color = base * (ambient + diffuse) + vec3<f32>(specular);
    // Fog towards the sky color.
    return vec4<f32>(mix(sky, color, exp(-0.003 * t * t)), 1.0);
}
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpu = match Gpu::new(GpuOptions::default()) {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("{error}: this demo needs a GPU (or a software renderer).");
            std::process::exit(1);
        }
    };
    let mut shader = ShaderQuad::new(&gpu, SCENE)?;
    if std::env::args().any(|a| a == "--bench") {
        bench(&gpu, &mut shader);
        return Ok(());
    }
    let target = RenderTarget::new(&gpu, 1, 1);
    let mut demo = Demo {
        gpu,
        shader,
        target,
        canvas: Canvas::new(0, 0),
        modes: Vec::new(),
        gpu_time: Duration::ZERO,
        error: None,
    };
    vnce_runtime::run(&mut demo, RunConfig::default())?;
    Ok(())
}

struct Demo {
    gpu: Gpu,
    shader: ShaderQuad,
    target: RenderTarget,
    canvas: Canvas,
    modes: Vec<GraphicsMode>,
    /// Render and readback time of the last frame.
    gpu_time: Duration,
    error: Option<String>,
}

impl Game for Demo {
    fn init(&mut self, ctx: &mut Context) {
        self.modes = GraphicsMode::available(ctx.terminal_info(), &ctx.capabilities());
        // Start with the best cell mode: full-screen animation is too much for Sixel.
        if let Some(&cells) = self.modes.iter().find(|m| !m.is_protocol()) {
            ctx.set_graphics_mode(cells);
        }
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        let Event::Key(key) = event else {
            return;
        };
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => ctx.quit(),
            KeyCode::Char('g' | 'G') if !self.modes.is_empty() => {
                let current = ctx.graphics_mode();
                let index = self.modes.iter().position(|&m| m == current).unwrap_or(0);
                ctx.set_graphics_mode(self.modes[(index + 1) % self.modes.len()]);
            }
            KeyCode::Char('p' | 'P') => {
                let next = match self.target.readback() {
                    Readback::Immediate => Readback::Pipelined,
                    Readback::Pipelined => Readback::Immediate,
                };
                self.target.set_readback(next);
            }
            _ => {}
        }
    }

    fn update(&mut self, _ctx: &mut Context) {}

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let size = ctx.size();
        let area = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
        let (width, height) = frame.image_pixel_size(area);
        if width > 0 && height > 0 {
            self.target.resize(&self.gpu, width, height);
            self.shader.set_pixel_aspect(frame.image_pixel_aspect());
            let start = Instant::now();
            self.shader.render(
                &self.gpu,
                &self.target,
                ctx.elapsed().as_secs_f32(),
                frame.count() as u32,
            );
            if let Err(error) = self.target.read_into(&self.gpu, &mut self.canvas) {
                self.error = Some(error.to_string());
            }
            self.gpu_time = start.elapsed();
            frame.draw_image(area, &self.canvas);
        }

        let readback = match self.target.readback() {
            Readback::Immediate => "immediate",
            Readback::Pipelined => "pipelined",
        };
        let status = self.error.clone().unwrap_or_else(|| {
            format!(
                " {} · {} · {}×{} px · gpu {:.2} ms ({readback}) · {:.0} fps",
                self.gpu.info(),
                frame.graphics_mode(),
                self.target.width(),
                self.target.height(),
                self.gpu_time.as_secs_f64() * 1000.0,
                ctx.fps(),
            )
        });
        let style = Style::new()
            .fg(Color::rgb(20, 20, 30))
            .bg(Color::rgb(140, 200, 240));
        let buffer = frame.buffer_mut();
        buffer.set_style(Rect::new(0, 0, size.width, 1), style);
        buffer.set_string(0, 0, &status, style.bold());
        let modes: Vec<&str> = self.modes.iter().map(|m| m.name()).collect();
        let help = format!(
            " G: method ({}) · P: readback · Esc: quit",
            modes.join(", ")
        );
        buffer.set_string(0, size.height.saturating_sub(1), &help, Style::new().dim());
    }
}

/// Prints render + readback times at several sizes, and the time to turn the image into
/// half-block cells.
fn bench(gpu: &Gpu, shader: &mut ShaderQuad) {
    const FRAMES: u32 = 60;
    println!("GPU: {}", gpu.info());
    println!("Raymarched scene, {FRAMES} frames per size (render + readback):");
    for (width, height) in [(160, 96), (400, 240), (1280, 720), (1920, 1080)] {
        let mut canvas = Canvas::new(0, 0);
        let mut times = Vec::new();
        for readback in [Readback::Immediate, Readback::Pipelined] {
            let mut target = RenderTarget::new(gpu, width, height).with_readback(readback);
            // Warm up (pipelines, buffers).
            shader.render(gpu, &target, 0.0, 0);
            let _ = target.read_into(gpu, &mut canvas);
            let start = Instant::now();
            for frame in 0..FRAMES {
                shader.render(gpu, &target, frame as f32 / 60.0, frame);
                target.read_into(gpu, &mut canvas).expect("readback");
            }
            times.push(start.elapsed().as_secs_f64() * 1000.0 / f64::from(FRAMES));
        }
        // Converting the image into half blocks (1×2 pixels per cell).
        let cells = Size::new(width as u16, (height / 2) as u16);
        let mut buffer = Buffer::empty(cells);
        let start = Instant::now();
        canvas.render(
            &mut buffer,
            Rect::from(cells),
            PixelMode::HalfBlock,
            &RenderOptions::default(),
        );
        let cells_time = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  {width:>4}×{height:<4} px  immediate {:>6.2} ms  pipelined {:>6.2} ms  \
             to cells {cells_time:>6.2} ms",
            times[0], times[1],
        );
    }
}
