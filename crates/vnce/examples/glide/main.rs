//! Glide: the 3D showcase of vnce (work in progress: a flight over the terrain).
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --example glide --features gpu
//! ```
//!
//! `G`: display method · `C`: GPU / CPU · `Esc` / `Q`: quit.

// Game code converts freely between pixels, counters and floats; every value stays far
// inside the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel coordinates and counters are small"
)]
// Geometry reads best with its usual names (x, y, z, u, v).
#![allow(clippy::many_single_char_names, reason = "geometry names")]
// The game is built in steps; parts made for later steps are not used yet.
#![allow(dead_code, reason = "used by the following steps")]

mod math;
mod models;
mod render;
mod sky;
mod terrain;

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_runtime::{self, Context, Game, RunConfig};
use vnce::vnce_term::{Event, Frame, GraphicsMode, KeyCode, KeyEventKind};

use crate::math::{Camera, direction};
use crate::render::{Renderer, View};
use crate::terrain::Terrain;

struct Viewer {
    terrain: Terrain,
    renderer: Renderer,
    modes: Vec<GraphicsMode>,
}

impl Game for Viewer {
    fn init(&mut self, ctx: &mut Context) {
        self.modes = GraphicsMode::available(ctx.terminal_info(), &ctx.capabilities());
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        let Event::Key(key) = event else { return };
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => ctx.quit(),
            KeyCode::Char('g') if !self.modes.is_empty() => {
                let current = ctx.graphics_mode();
                let index = self.modes.iter().position(|&m| m == current).unwrap_or(0);
                ctx.set_graphics_mode(self.modes[(index + 1) % self.modes.len()]);
            }
            KeyCode::Char('c') => {
                let gpu = !self.renderer.using_gpu();
                self.renderer.set_gpu(gpu);
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
            let t = ctx.elapsed().as_secs_f32();
            let yaw = t * 0.05;
            let x = 2000.0 * yaw.sin();
            let z = -2000.0 * yaw.cos() + 2000.0;
            let ground = self.terrain.surface(x, z).max(0.0);
            let aspect = width as f32 / height as f32 * frame.image_pixel_aspect();
            let camera = Camera::new(
                [x, ground + 90.0, z],
                direction(yaw + std::f32::consts::FRAC_PI_2, -0.18),
                [0.0, 1.0, 0.0],
                aspect,
            );
            let view = View {
                camera,
                light: sky::daylight((t / 60.0).fract()),
                objects: Vec::new(),
                time: t,
            };
            self.renderer.render(&self.terrain, &view, width, height);
            frame.draw_image(area, self.renderer.canvas());
        }
        let stats = self.renderer.stats();
        let line = format!(
            " Glide terrain · {} · {} · {} chunks · {} triangles · {:.1} ms · {:.0} fps",
            self.renderer.describe(),
            frame.graphics_mode(),
            stats.chunks,
            stats.triangles,
            stats.milliseconds,
            ctx.fps()
        );
        let style = Style::new()
            .fg(Color::rgb(16, 18, 26))
            .bg(Color::rgb(255, 200, 120));
        let buffer = frame.buffer_mut();
        buffer.set_style(Rect::new(0, 0, size.width, 1), style);
        buffer.set_string(0, 0, &line, style.bold());
        buffer.set_string(
            0,
            size.height.saturating_sub(1),
            " G: display method · C: GPU / CPU · Esc: quit",
            Style::new().dim(),
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut viewer = Viewer {
        terrain: Terrain::new(1),
        renderer: Renderer::new(true, 4),
        modes: Vec::new(),
    };
    vnce_runtime::run(&mut viewer, RunConfig::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::math::{Camera, direction};
    use crate::render::{Renderer, View};
    use crate::sky::daylight;
    use crate::terrain::Terrain;

    /// Writes pictures of the terrain to the folder in `GLIDE_SNAPSHOTS` (skipped without
    /// it), on the CPU and, with `GLIDE_GPU` set, on the GPU. For looking at the art.
    #[test]
    fn terrain_snapshots() {
        let Ok(folder) = std::env::var("GLIDE_SNAPSHOTS") else {
            return;
        };
        let gpu = std::env::var("GLIDE_GPU").is_ok();
        let terrain = Terrain::new(1);
        for (name, phase, yaw, pitch) in [
            ("morning", 0.08, 1.2, -0.15),
            ("noon", 0.5, 3.0, -0.3),
            ("evening", 0.93, 4.4, -0.1),
        ] {
            let mut renderer = Renderer::new(gpu, 4);
            let eye = [300.0, terrain.surface(300.0, 200.0).max(0.0) + 110.0, 200.0];
            let camera = Camera::new(eye, direction(yaw, pitch), [0.0, 1.0, 0.0], 1.6);
            renderer.warm_up(&terrain, &camera);
            let view = View {
                camera,
                light: daylight(phase),
                objects: Vec::new(),
                time: 3.0,
            };
            // The GPU's pictures arrive a frame late.
            for _ in 0..3 {
                renderer.render(&terrain, &view, 640, 400);
            }
            let png = renderer.canvas().to_png().expect("a canvas encodes as PNG");
            let kind = if renderer.using_gpu() { "gpu" } else { "cpu" };
            std::fs::write(format!("{folder}/terrain-{name}-{kind}.png"), png).expect("writable");
        }
    }
}
