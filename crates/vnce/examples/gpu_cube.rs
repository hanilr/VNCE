//! Spinning 3D cubes over a checkered floor, rendered on the GPU with
//! `vnce_gpu::MeshRenderer` (depth buffer, multisample anti-aliasing, a sun with ambient
//! light, and distance fog) and shown with cells or a graphics protocol.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --features gpu --example gpu_cube
//! cargo run --release --features gpu --example gpu_cube -- --bench   # GPU timings
//! ```
//!
//! `G`: next display method · `M`: anti-aliasing on / off · `L`: light and fog on / off ·
//! `P`: pipelined readback · `Esc` / `Q`: quit.
//!
//! The demo starts with cells: sending a full-screen Sixel image 60 times per second is
//! too slow for most terminals. Kitty handles it much better.

// Demo code converts freely between sizes, counters and floats; every value stays far
// inside the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "counters and pixel sizes are small"
)]

use std::f32::consts::TAU;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_gpu::{
    Fog, Gpu, GpuOptions, Light, Mat4, Mesh, MeshDraw, MeshRenderer, MeshRendererOptions,
    MeshScene, Readback, RenderTarget, Vec3, Vertex,
};
use vnce::vnce_raster::{Canvas, Rgba};
use vnce::vnce_runtime::{Context, Game, RunConfig};
use vnce::vnce_term::{Event, Frame, GraphicsMode, KeyCode, KeyEventKind};

const BACKGROUND: Rgba = Rgba::rgb(12, 14, 28);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpu = match Gpu::new(GpuOptions::default()) {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("{error}: this demo needs a GPU (or a software renderer).");
            std::process::exit(1);
        }
    };
    let scene = Scene::new(&gpu);
    if std::env::args().any(|a| a == "--bench") {
        bench(&gpu, &scene);
        return Ok(());
    }
    let mut demo = Demo {
        renderer: MeshRenderer::new(&gpu, MeshRendererOptions::default()),
        target: RenderTarget::new(&gpu, 1, 1),
        gpu,
        scene,
        canvas: Canvas::new(0, 0),
        modes: Vec::new(),
        gpu_time: Duration::ZERO,
        lit: true,
        error: None,
    };
    vnce::run(&mut demo, RunConfig::default())?;
    Ok(())
}

/// A unit cube centered at the origin, one color per face.
fn cube(gpu: &Gpu, colors: [Rgba; 6]) -> Mesh {
    const X: Vec3 = [1.0, 0.0, 0.0];
    const Y: Vec3 = [0.0, 1.0, 0.0];
    const Z: Vec3 = [0.0, 0.0, 1.0];
    let neg = |v: Vec3| [-v[0], -v[1], -v[2]];
    // Each face: normal and two axes with u × v = normal, so the corners below are
    // counter-clockwise seen from outside.
    let faces = [
        (X, Y, Z),
        (neg(X), Z, Y),
        (Y, Z, X),
        (neg(Y), X, Z),
        (Z, X, Y),
        (neg(Z), Y, X),
    ];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for ((normal, u, v), color) in faces.into_iter().zip(colors) {
        let base = vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let position = [0, 1, 2].map(|i| 0.5 * (normal[i] + su * u[i] + sv * v[i]));
            vertices.push(Vertex::lit(position, normal, color));
        }
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
    Mesh::new(gpu, &vertices, &indices)
}

/// A checkered floor of `tiles` × `tiles` unit squares at y = 0, centered on the origin.
fn floor(gpu: &Gpu, tiles: u32) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let half = tiles as f32 / 2.0;
    for row in 0..tiles {
        for column in 0..tiles {
            let color = if (row + column) % 2 == 0 {
                Rgba::rgb(70, 74, 96)
            } else {
                Rgba::rgb(36, 38, 54)
            };
            let (x, z) = (column as f32 - half, row as f32 - half);
            let base = vertices.len() as u32;
            for (dx, dz) in [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)] {
                vertices.push(Vertex::lit([x + dx, 0.0, z + dz], [0.0, 1.0, 0.0], color));
            }
            indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        }
    }
    Mesh::new(gpu, &vertices, &indices)
}

/// The meshes of the scene.
struct Scene {
    big: Mesh,
    small: Mesh,
    floor: Mesh,
}

impl Scene {
    fn new(gpu: &Gpu) -> Self {
        Self {
            big: cube(
                gpu,
                [
                    Rgba::rgb(235, 80, 70),
                    Rgba::rgb(150, 40, 35),
                    Rgba::rgb(250, 200, 70),
                    Rgba::rgb(120, 90, 30),
                    Rgba::rgb(70, 170, 240),
                    Rgba::rgb(30, 80, 130),
                ],
            ),
            small: cube(
                gpu,
                [
                    Rgba::rgb(120, 230, 140),
                    Rgba::rgb(50, 120, 70),
                    Rgba::rgb(240, 240, 240),
                    Rgba::rgb(110, 110, 110),
                    Rgba::rgb(200, 120, 240),
                    Rgba::rgb(100, 50, 130),
                ],
            ),
            floor: floor(gpu, 16),
        }
    }

    /// The camera position at time `t` (seconds).
    fn eye(t: f32) -> Vec3 {
        [6.0 * (t * 0.15).sin(), 3.0, 6.0 * (t * 0.15).cos()]
    }

    /// The light and fog at time `t`: a sun slowly circling the scene, and fog that hides
    /// the far edge of the floor in the background color.
    fn lighting(t: f32, lit: bool) -> MeshScene {
        if !lit {
            return MeshScene::default();
        }
        MeshScene {
            light: Some(Light {
                direction: [(t * 0.3).cos(), 1.2, (t * 0.3).sin()],
                ..Light::default()
            }),
            fog: Some(Fog {
                color: BACKGROUND,
                start: 6.0,
                end: 14.0,
            }),
            camera: Self::eye(t),
        }
    }

    /// The draws for time `t` (seconds) and an image `aspect` ratio (width / height).
    fn draws(&self, t: f32, aspect: f32) -> Vec<MeshDraw<'_>> {
        let camera = Mat4::perspective(50_f32.to_radians(), aspect, 0.1, 100.0)
            * Mat4::look_at(Self::eye(t), [0.0, 0.8, 0.0], [0.0, 1.0, 0.0]);
        let mut draws = vec![
            MeshDraw::new(&self.floor, camera, Mat4::IDENTITY),
            MeshDraw::new(
                &self.big,
                camera,
                Mat4::translation([0.0, 1.4 + 0.2 * (t * 1.7).sin(), 0.0])
                    * Mat4::rotation_y(t * 0.9)
                    * Mat4::rotation_x(t * 0.6)
                    * Mat4::scale([1.6, 1.6, 1.6]),
            ),
        ];
        for i in 0..8 {
            let angle = t * 0.7 + i as f32 * TAU / 8.0;
            draws.push(MeshDraw::new(
                &self.small,
                camera,
                Mat4::translation([3.0 * angle.cos(), 0.5, 3.0 * angle.sin()])
                    * Mat4::rotation_y(-angle * 2.0)
                    * Mat4::rotation_z(t * 1.5 + i as f32)
                    * Mat4::scale([0.5, 0.5, 0.5]),
            ));
        }
        draws
    }
}

struct Demo {
    gpu: Gpu,
    scene: Scene,
    renderer: MeshRenderer,
    target: RenderTarget,
    canvas: Canvas,
    modes: Vec<GraphicsMode>,
    /// Render and readback time of the last frame.
    gpu_time: Duration,
    /// Light and fog on.
    lit: bool,
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
            KeyCode::Char('m' | 'M') => {
                let msaa = if self.renderer.samples() > 1 { 1 } else { 4 };
                self.renderer = MeshRenderer::new(
                    &self.gpu,
                    MeshRendererOptions {
                        msaa,
                        ..MeshRendererOptions::default()
                    },
                );
            }
            KeyCode::Char('l' | 'L') => self.lit = !self.lit,
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
            let aspect = self.target.aspect() * frame.image_pixel_aspect();
            let t = ctx.elapsed().as_secs_f32();
            let draws = self.scene.draws(t, aspect);
            let start = Instant::now();
            self.renderer
                .set_scene(&self.gpu, Scene::lighting(t, self.lit));
            self.renderer
                .render(&self.gpu, &self.target, BACKGROUND, &draws);
            // In a native window the texture is shown as it is; elsewhere it is read
            // back into the canvas and drawn.
            if let Err(error) = self.target.show(&self.gpu, frame, area, &mut self.canvas) {
                self.error = Some(error.to_string());
            }
            self.gpu_time = start.elapsed();
        }

        let readback = match self.target.readback() {
            Readback::Immediate => "immediate",
            Readback::Pipelined => "pipelined",
        };
        let status = self.error.clone().unwrap_or_else(|| {
            format!(
                " {} · {} · {}×{} px · msaa {}× · gpu {:.2} ms ({readback}) · {:.0} fps",
                self.gpu.info(),
                frame.graphics_mode(),
                self.target.width(),
                self.target.height(),
                self.renderer.samples(),
                self.gpu_time.as_secs_f64() * 1000.0,
                ctx.fps(),
            )
        });
        let style = Style::new()
            .fg(Color::rgb(20, 20, 30))
            .bg(Color::rgb(240, 190, 110));
        let buffer = frame.buffer_mut();
        buffer.set_style(Rect::new(0, 0, size.width, 1), style);
        buffer.set_string(0, 0, &status, style.bold());
        let modes: Vec<&str> = self.modes.iter().map(|m| m.name()).collect();
        let help = format!(
            " G: method ({}) · M: anti-aliasing · L: light and fog · P: readback · Esc: quit",
            modes.join(", ")
        );
        buffer.set_string(0, size.height.saturating_sub(1), &help, Style::new().dim());
    }
}

/// Prints render + readback times at several sizes, with and without anti-aliasing.
fn bench(gpu: &Gpu, scene: &Scene) {
    const FRAMES: u32 = 60;
    println!("GPU: {}", gpu.info());
    println!("10 cubes and a floor, {FRAMES} frames per size (render + immediate readback):");
    for (width, height) in [(160, 96), (400, 240), (1280, 720), (1920, 1080)] {
        let mut canvas = Canvas::new(0, 0);
        let mut line = format!("  {width:>4}×{height:<4} px");
        for msaa in [1, 4] {
            let mut renderer = MeshRenderer::new(
                gpu,
                MeshRendererOptions {
                    msaa,
                    ..MeshRendererOptions::default()
                },
            );
            let mut target = RenderTarget::new(gpu, width, height);
            let aspect = target.aspect();
            renderer.set_scene(gpu, Scene::lighting(0.0, true));
            renderer.render(gpu, &target, BACKGROUND, &scene.draws(0.0, aspect));
            let _ = target.read_into(gpu, &mut canvas);
            let start = Instant::now();
            for frame in 0..FRAMES {
                let draws = scene.draws(frame as f32 / 60.0, aspect);
                renderer.set_scene(gpu, Scene::lighting(frame as f32 / 60.0, true));
                renderer.render(gpu, &target, BACKGROUND, &draws);
                target.read_into(gpu, &mut canvas).expect("readback");
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(FRAMES);
            // Writing to a String cannot fail.
            let _ = write!(line, "  msaa {}× {ms:>6.2} ms", renderer.samples());
        }
        println!("{line}");
    }
}
