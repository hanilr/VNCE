//! Drawing the 3D world into a canvas, on the GPU (`MeshRenderer` with light and fog, and a
//! `ShaderQuad` sky) or on the CPU (triangles clipped, lit, fogged and sorted back to
//! front, filled with `vnce_raster`).
//!
//! Both draw the meshes over a sky panorama: every pixel looks the sky up along its view
//! direction. The GPU renders the meshes on a transparent background with fog that fades
//! them out, reads them back one frame late (pipelined) and lays them over the sky; the
//! CPU paints the sky first and fills the triangles over it, far ones partly transparent.

use std::time::Instant;

use vnce::vnce_raster::{BlendMode, Canvas, Rgba};

use crate::math::{Camera, Mat4, NEAR, Vec3, dot, length, sub};
use crate::models::{RingKind, Tri, craft, ring, shadow};
use crate::sky::{self, Daylight};
use crate::terrain::{CHUNK_SIZE, Chunk, ChunkCache, ChunkGeometry, Terrain, chunk_center};

/// A model drawn at a place in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Model {
    Craft,
    /// A ring; `true` glows (the next one to fly through).
    Ring(RingKind, bool),
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Object {
    pub model: Model,
    pub transform: Mat4,
}

/// Everything to draw in one picture besides the terrain.
#[derive(Debug, Clone)]
pub struct View {
    pub camera: Camera,
    pub light: Daylight,
    pub objects: Vec<Object>,
    /// Seconds, for the clouds.
    pub time: f32,
}

/// The models' triangles, shared by both renderers.
pub struct Models {
    pub craft: Vec<Tri>,
    pub rings: Vec<((RingKind, bool), Vec<Tri>)>,
    pub shadow: Vec<Tri>,
}

impl Models {
    pub fn new() -> Self {
        let mut rings = Vec::new();
        for kind in RingKind::ALL {
            for glow in [false, true] {
                rings.push(((kind, glow), ring(kind, glow)));
            }
        }
        Self {
            craft: craft(),
            rings,
            shadow: shadow(),
        }
    }

    pub fn get(&self, model: Model) -> &[Tri] {
        match model {
            Model::Craft => &self.craft,
            Model::Shadow => &self.shadow,
            Model::Ring(kind, glow) => self
                .rings
                .iter()
                .find(|(key, _)| *key == (kind, glow))
                .map_or(&[], |(_, tris)| tris),
        }
    }
}

/// What the last picture cost.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    pub chunks: usize,
    pub triangles: usize,
    pub milliseconds: f32,
}

/// Draws the world with the GPU when there is one (and it is chosen), else the CPU.
pub struct Renderer {
    models: Models,
    cpu: CpuRenderer,
    #[cfg(feature = "gpu")]
    gpu: Option<gpu::GpuRenderer>,
    use_gpu: bool,
    /// Why there is no GPU, when there is none.
    note: String,
    seed: Option<u64>,
    canvas: Canvas,
    /// Chunks built per picture at most (more right after a jump).
    pub budget: usize,
    stats: Stats,
}

impl Renderer {
    /// A renderer; the GPU starts when `want_gpu` is set and the `gpu` feature is on.
    pub fn new(want_gpu: bool, msaa: u32) -> Self {
        let models = Models::new();
        #[cfg(feature = "gpu")]
        let (gpu, note) = if want_gpu {
            match gpu::GpuRenderer::new(&models, msaa) {
                Ok(gpu) => (Some(gpu), String::new()),
                Err(error) => (None, error),
            }
        } else {
            (None, "CPU chosen".to_owned())
        };
        #[cfg(not(feature = "gpu"))]
        let note = {
            let _ = (want_gpu, msaa);
            "build with --features gpu".to_owned()
        };
        Self {
            models,
            cpu: CpuRenderer::new(),
            #[cfg(feature = "gpu")]
            use_gpu: gpu.is_some(),
            #[cfg(not(feature = "gpu"))]
            use_gpu: false,
            #[cfg(feature = "gpu")]
            gpu,
            note,
            seed: None,
            canvas: Canvas::new(0, 0),
            budget: 3,
            stats: Stats::default(),
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

    pub fn set_gpu(&mut self, on: bool) {
        self.use_gpu = on && self.gpu_available();
    }

    /// Anti-aliasing samples of the GPU (1 on the CPU).
    #[cfg_attr(
        not(feature = "gpu"),
        allow(clippy::unused_self, reason = "the GPU version uses `self`")
    )]
    pub fn msaa(&self) -> u32 {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &self.gpu {
            return gpu.msaa();
        }
        1
    }

    #[cfg_attr(
        not(feature = "gpu"),
        allow(clippy::unused_self, reason = "the GPU version uses `self`")
    )]
    pub fn set_msaa(&mut self, msaa: u32) {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &mut self.gpu {
            gpu.set_msaa(msaa);
        }
        #[cfg(not(feature = "gpu"))]
        let _ = msaa;
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

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// The last picture (overlays are drawn into it).
    pub fn canvas(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    /// Builds every chunk around `camera` at once (before a flight starts).
    pub fn warm_up(&mut self, terrain: &Terrain, camera: &Camera) {
        self.forget_other_terrain(terrain);
        #[cfg(feature = "gpu")]
        if self.using_gpu() {
            if let Some(gpu) = &mut self.gpu {
                gpu.build(terrain, camera, usize::MAX);
            }
            return;
        }
        self.cpu.build(terrain, camera, usize::MAX);
    }

    fn forget_other_terrain(&mut self, terrain: &Terrain) {
        if self.seed != Some(terrain.seed) {
            self.seed = Some(terrain.seed);
            self.cpu.chunks.clear();
            #[cfg(feature = "gpu")]
            if let Some(gpu) = &mut self.gpu {
                gpu.chunks.clear();
            }
        }
    }

    /// Draws `view` into the canvas at `width` × `height` and returns the camera the
    /// picture was taken with (the GPU's picture is one frame old).
    pub fn render(&mut self, terrain: &Terrain, view: &View, width: u32, height: u32) -> Camera {
        let start = Instant::now();
        self.forget_other_terrain(terrain);
        if (self.canvas.width(), self.canvas.height()) != (width, height) {
            self.canvas = Canvas::new(width, height);
        }
        let shown = if let Some(camera) = self.render_gpu(terrain, view, width, height) {
            camera
        } else {
            self.stats =
                self.cpu
                    .render(terrain, view, &self.models, self.budget, &mut self.canvas);
            view.camera
        };
        self.stats.milliseconds = start.elapsed().as_secs_f32() * 1000.0;
        shown
    }

    /// Draws with the GPU when it is in use; `None` otherwise.
    #[cfg(feature = "gpu")]
    fn render_gpu(
        &mut self,
        terrain: &Terrain,
        view: &View,
        width: u32,
        height: u32,
    ) -> Option<Camera> {
        let gpu = self.gpu.as_mut().filter(|_| self.use_gpu)?;
        let (camera, stats) = gpu.render(
            terrain,
            view,
            width,
            height,
            self.budget,
            &mut self.cpu.panorama,
            &mut self.canvas,
        );
        self.stats = stats;
        Some(camera)
    }

    #[cfg(not(feature = "gpu"))]
    #[allow(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        reason = "same as the GPU version"
    )]
    fn render_gpu(
        &mut self,
        _terrain: &Terrain,
        _view: &View,
        _width: u32,
        _height: u32,
    ) -> Option<Camera> {
        None
    }
}

/// Fills `canvas` with the sky seen by `camera`, under the premultiplied picture `over`
/// (pixels with alpha below 255 let the sky through), or alone without one.
pub fn compose_sky(canvas: &mut Canvas, panorama: &Canvas, camera: &Camera, over: Option<&Canvas>) {
    let (width, height) = (canvas.width(), canvas.height());
    if width == 0 || height == 0 {
        return;
    }
    let over = over.filter(|o| (o.width(), o.height()) == (width, height));
    let (w, h) = (width as f32, height as f32);
    let pixels = canvas.pixels_mut();
    for y in 0..height {
        let sy = 1.0 - 2.0 * (y as f32 + 0.5) / h;
        for x in 0..width {
            let index = (y * width + x) as usize;
            let front = over.map_or(Rgba::TRANSPARENT, |o| o.pixels()[index]);
            if front.a == 255 {
                pixels[index] = front;
                continue;
            }
            let sx = 2.0 * (x as f32 + 0.5) / w - 1.0;
            let (u, v) = sky::panorama_point(camera.ray(sx, sy));
            let back = sky::sample(panorama, u, v);
            let rest = u32::from(255 - front.a);
            let mix =
                |f: u8, b: u8| (u32::from(f) + (u32::from(b) * rest + 127) / 255).min(255) as u8;
            pixels[index] = Rgba::rgb(
                mix(front.r, back.r),
                mix(front.g, back.g),
                mix(front.b, back.b),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------------------

/// A triangle ready to fill: picture points, distance and color.
struct Face {
    points: [(f32, f32); 3],
    depth: f32,
    color: Rgba,
}

/// The CPU renderer: lower detail (every other grid point, a shorter view, trees only
/// nearby).
pub struct CpuRenderer {
    chunks: ChunkCache<ChunkGeometry>,
    panorama: Canvas,
    panorama_phase: f32,
    faces: Vec<Face>,
}

/// The CPU draws trees and rocks up to this distance.
const CPU_PROP_REACH: f32 = 520.0;

impl CpuRenderer {
    fn new() -> Self {
        Self {
            chunks: ChunkCache::new(3.3),
            panorama: Canvas::new(0, 0),
            panorama_phase: -1.0,
            faces: Vec::new(),
        }
    }

    fn build(&mut self, terrain: &Terrain, camera: &Camera, budget: usize) {
        self.chunks
            .update(camera.eye, camera.forward, budget, |key| {
                let geometry = terrain.chunk(key, 2, true);
                Chunk {
                    low: geometry.low,
                    high: geometry.high,
                    data: geometry,
                }
            });
    }

    /// Redraws the sky panorama when the light changed.
    fn update_panorama(&mut self, light: &Daylight) {
        if (light.phase - self.panorama_phase).abs() > 0.002 || self.panorama.width() == 0 {
            sky::cpu_panorama(&mut self.panorama, light);
            self.panorama_phase = light.phase;
        }
    }

    fn render(
        &mut self,
        terrain: &Terrain,
        view: &View,
        models: &Models,
        budget: usize,
        canvas: &mut Canvas,
    ) -> Stats {
        let camera = &view.camera;
        self.build(terrain, camera, budget);
        self.update_panorama(&view.light);
        compose_sky(canvas, &self.panorama, camera, None);
        let reach = self.chunks.radius * CHUNK_SIZE;
        let fog = (reach * 0.42, reach * 0.95);
        let size = (canvas.width() as f32, canvas.height() as f32);
        let projector = Projector::new(camera, size, &view.light, fog);
        let mut faces = std::mem::take(&mut self.faces);
        faces.clear();
        let mut chunks = 0;
        for (key, geometry) in self.chunks.visible(camera) {
            chunks += 1;
            for tri in &geometry.tris {
                projector.add(tri, &mut faces);
            }
            if crate::math::flat_distance(chunk_center(key), camera.eye)
                < CPU_PROP_REACH + CHUNK_SIZE * 0.7
            {
                for tri in &geometry.prop_tris {
                    projector.add(tri, &mut faces);
                }
            }
        }
        for object in &view.objects {
            for tri in models.get(object.model) {
                projector.add(&tri.transformed(&object.transform), &mut faces);
            }
        }
        faces.sort_unstable_by(|a, b| b.depth.total_cmp(&a.depth));
        canvas.set_blend_mode(BlendMode::Alpha);
        for face in &faces {
            canvas.fill_triangle(face.points[0], face.points[1], face.points[2], face.color);
        }
        let triangles = faces.len();
        self.faces = faces;
        Stats {
            chunks,
            triangles,
            milliseconds: 0.0,
        }
    }
}

/// Turns world triangles into filled faces for one camera.
struct Projector {
    view: Mat4,
    eye: Vec3,
    /// Picture size and the projection's scale factors.
    size: (f32, f32),
    scale: (f32, f32),
    light: Daylight,
    fog: (f32, f32),
}

impl Projector {
    fn new(camera: &Camera, size: (f32, f32), light: &Daylight, fog: (f32, f32)) -> Self {
        let tan = (crate::math::FOV_Y / 2.0).tan();
        Self {
            view: camera.view(),
            eye: camera.eye,
            size,
            scale: (1.0 / (tan * camera.aspect), 1.0 / tan),
            light: *light,
            fog,
        }
    }

    /// A view-space point on the picture.
    fn screen(&self, p: Vec3) -> (f32, f32) {
        let depth = -p[2];
        let x = p[0] / depth * self.scale.0;
        let y = p[1] / depth * self.scale.1;
        (
            f32::midpoint(x, 1.0) * self.size.0,
            f32::midpoint(1.0, -y) * self.size.1,
        )
    }

    fn add(&self, tri: &Tri, faces: &mut Vec<Face>) {
        let lit = tri.is_lit();
        // Faces turned away from the camera are hidden by the front ones.
        if lit && dot(tri.n, sub(self.eye, tri.p[0])) <= 0.0 {
            return;
        }
        let center = [0, 1, 2].map(|i| (tri.p[0][i] + tri.p[1][i] + tri.p[2][i]) / 3.0);
        let distance = length(sub(center, self.eye));
        let fogged = ((distance - self.fog.0) / (self.fog.1 - self.fog.0)).clamp(0.0, 1.0);
        if fogged >= 0.99 {
            return;
        }
        let v = tri.p.map(|p| self.view.point(p));
        // Clip against the near plane: keep the part in front (z < -NEAR).
        let limit = -NEAR;
        let inside = v.map(|p| p[2] < limit);
        let mut polygon: [Vec3; 4] = [[0.0; 3]; 4];
        let mut count = 0;
        for i in 0..3 {
            let (a, b) = (v[i], v[(i + 1) % 3]);
            if inside[i] {
                polygon[count] = a;
                count += 1;
            }
            if inside[i] != inside[(i + 1) % 3] {
                let t = (limit - a[2]) / (b[2] - a[2]);
                polygon[count] = [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t);
                count += 1;
            }
        }
        if count < 3 {
            return;
        }
        let points: Vec<(f32, f32)> = polygon[..count].iter().map(|&p| self.screen(p)).collect();
        let (w, h) = self.size;
        if points.iter().all(|p| p.0 < 0.0)
            || points.iter().all(|p| p.0 > w)
            || points.iter().all(|p| p.1 < 0.0)
            || points.iter().all(|p| p.1 > h)
        {
            return;
        }
        let color = if lit {
            let facing = dot(tri.n, self.light.sun).max(0.0);
            let k = [0, 1, 2].map(|i| self.light.ambient[i] + self.light.sun_color[i] * facing);
            let c = tri.color;
            let shaded = sky::srgb([
                sky::linear(c.r) * k[0],
                sky::linear(c.g) * k[1],
                sky::linear(c.b) * k[2],
            ]);
            shaded.with_alpha(c.a)
        } else {
            tri.color
        };
        let alpha = (f32::from(color.a) * (1.0 - fogged)) as u8;
        let color = color.with_alpha(alpha);
        let depth = -(v[0][2] + v[1][2] + v[2][2]) / 3.0;
        faces.push(Face {
            points: [points[0], points[1], points[2]],
            depth,
            color,
        });
        if count == 4 {
            faces.push(Face {
                points: [points[0], points[2], points[3]],
                depth,
                color,
            });
        }
    }
}

// ---------------------------------------------------------------------------------------
// GPU
// ---------------------------------------------------------------------------------------

#[cfg(feature = "gpu")]
mod gpu {
    use vnce::vnce_gpu::{
        Fog, Gpu, GpuOptions, Light, Mat4 as GpuMat4, Mesh, MeshDraw, MeshRenderer,
        MeshRendererOptions, MeshScene, Readback, RenderTarget, ShaderQuad, Vertex,
    };
    use vnce::vnce_raster::{Canvas, Rgba};

    use super::{Models, Stats, View, compose_sky};
    use crate::math::{Camera, Mat4};
    use crate::models::Tri;
    use crate::sky::{self, GPU_PANORAMA};
    use crate::terrain::{CHUNK_SIZE, Chunk, ChunkCache, Terrain};

    fn mesh(gpu: &Gpu, tris: &[Tri]) -> Mesh {
        let vertices: Vec<Vertex> = tris
            .iter()
            .flat_map(|t| t.p.map(|p| Vertex::lit(p, t.n, t.color)))
            .collect();
        let indices: Vec<u32> = (0..vertices.len() as u32).collect();
        Mesh::new(gpu, &vertices, &indices)
    }

    fn convert(m: &Mat4) -> GpuMat4 {
        GpuMat4 { cols: m.cols }
    }

    pub struct GpuRenderer {
        gpu: Gpu,
        renderer: MeshRenderer,
        target: RenderTarget,
        image: Canvas,
        /// The camera of the picture being rendered, and of the one in `image`.
        pending: Option<Camera>,
        image_camera: Option<Camera>,
        pub chunks: ChunkCache<Mesh>,
        models: Vec<(super::Model, Mesh)>,
        sky: ShaderQuad,
        sky_target: RenderTarget,
        sky_canvas: Canvas,
        pub name: String,
    }

    impl GpuRenderer {
        pub fn new(models: &Models, msaa: u32) -> Result<Self, String> {
            let gpu = Gpu::new(GpuOptions::default().with_env()).map_err(|e| e.to_string())?;
            let sky = ShaderQuad::new(&gpu, sky::SKY_SHADER).map_err(|e| e.to_string())?;
            let mut list = vec![
                (super::Model::Craft, mesh(&gpu, &models.craft)),
                (super::Model::Shadow, mesh(&gpu, &models.shadow)),
            ];
            for ((kind, glow), tris) in &models.rings {
                list.push((super::Model::Ring(*kind, *glow), mesh(&gpu, tris)));
            }
            Ok(Self {
                name: gpu.info().to_string(),
                renderer: Self::mesh_renderer(&gpu, msaa),
                // Pipelined readback: the CPU never waits for the GPU (one frame of
                // latency).
                target: RenderTarget::new(&gpu, 1, 1).with_readback(Readback::Pipelined),
                image: Canvas::new(0, 0),
                pending: None,
                image_camera: None,
                chunks: ChunkCache::new(5.5),
                models: list,
                sky,
                sky_target: RenderTarget::new(&gpu, GPU_PANORAMA.0, GPU_PANORAMA.1)
                    .with_readback(Readback::Pipelined),
                sky_canvas: Canvas::new(0, 0),
                gpu,
            })
        }

        fn mesh_renderer(gpu: &Gpu, msaa: u32) -> MeshRenderer {
            MeshRenderer::new(
                gpu,
                MeshRendererOptions {
                    msaa,
                    cull_back_faces: false,
                },
            )
        }

        pub fn msaa(&self) -> u32 {
            self.renderer.samples()
        }

        pub fn set_msaa(&mut self, msaa: u32) {
            if msaa != self.renderer.samples() {
                self.renderer = Self::mesh_renderer(&self.gpu, msaa);
            }
        }

        pub fn build(&mut self, terrain: &Terrain, camera: &Camera, budget: usize) {
            let gpu = &self.gpu;
            self.chunks
                .update(camera.eye, camera.forward, budget, |key| {
                    let geometry = terrain.chunk(key, 1, true);
                    let mut tris = geometry.tris;
                    tris.extend(geometry.prop_tris);
                    Chunk {
                        low: geometry.low,
                        high: geometry.high,
                        data: mesh(gpu, &tris),
                    }
                });
        }

        #[allow(clippy::too_many_arguments, reason = "the renderer's parts")]
        pub fn render(
            &mut self,
            terrain: &Terrain,
            view: &View,
            width: u32,
            height: u32,
            budget: usize,
            cpu_panorama: &mut Canvas,
            canvas: &mut Canvas,
        ) -> (Camera, Stats) {
            let camera = view.camera;
            self.build(terrain, &camera, budget);
            let reach = self.chunks.radius * CHUNK_SIZE;
            let light = &view.light;
            self.renderer.set_scene(
                &self.gpu,
                MeshScene {
                    light: Some(Light {
                        direction: light.sun,
                        sun_color: light.sun_color,
                        ambient: light.ambient,
                    }),
                    fog: Some(Fog {
                        color: light.haze_rgba().with_alpha(0),
                        start: reach * 0.4,
                        end: reach * 0.96,
                    }),
                    camera: camera.eye,
                },
            );
            let view_projection = convert(&camera.view_projection());
            let mut draws = Vec::new();
            let mut stats = Stats::default();
            for (_, mesh) in self.chunks.visible(&camera) {
                stats.chunks += 1;
                stats.triangles += mesh.index_count() as usize / 3;
                draws.push(MeshDraw::new(mesh, view_projection, GpuMat4::IDENTITY));
            }
            for object in &view.objects {
                if let Some((_, mesh)) = self.models.iter().find(|(m, _)| *m == object.model) {
                    stats.triangles += mesh.index_count() as usize / 3;
                    draws.push(MeshDraw::new(
                        mesh,
                        view_projection,
                        convert(&object.transform),
                    ));
                }
            }
            if (self.target.width(), self.target.height()) != (width, height) {
                self.target.resize(&self.gpu, width, height);
                self.pending = None;
                self.image_camera = None;
            }
            self.renderer
                .render(&self.gpu, &self.target, Rgba::TRANSPARENT, &draws);
            // The picture that arrives is the one rendered last time, with its camera.
            if matches!(self.target.read_into(&self.gpu, &mut self.image), Ok(true)) {
                self.image_camera = self.pending;
            }
            self.pending = Some(camera);

            self.sky.render(
                &self.gpu,
                &self.sky_target,
                view.time,
                sky::phase_frame(light.phase),
            );
            let _ = self.sky_target.read_into(&self.gpu, &mut self.sky_canvas);
            let sky_ready = (self.sky_canvas.width(), self.sky_canvas.height()) == GPU_PANORAMA;
            let panorama = if sky_ready {
                &self.sky_canvas
            } else {
                sky::cpu_panorama(cpu_panorama, light);
                &*cpu_panorama
            };
            let image_ready = (self.image.width(), self.image.height()) == (width, height);
            let shown = self.image_camera.filter(|_| image_ready).unwrap_or(camera);
            compose_sky(canvas, panorama, &shown, image_ready.then_some(&self.image));
            (shown, stats)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::direction;
    use crate::sky::daylight;

    #[test]
    fn the_cpu_draws_land_below_and_sky_above() {
        let terrain = Terrain::new(4);
        let mut renderer = Renderer::new(false, 1);
        let eye = [0.0, terrain.surface(0.0, 0.0).max(0.0) + 120.0, 0.0];
        let camera = Camera::new(eye, direction(0.4, -0.25), [0.0, 1.0, 0.0], 1.6);
        renderer.warm_up(&terrain, &camera);
        let view = View {
            camera,
            light: daylight(0.4),
            objects: vec![Object {
                model: Model::Craft,
                transform: Mat4::translation(crate::math::add_scaled(eye, camera.forward, 20.0)),
            }],
            time: 0.0,
        };
        let shown = renderer.render(&terrain, &view, 160, 100);
        assert_eq!(shown, camera);
        let stats = renderer.stats();
        assert!(stats.chunks > 0 && stats.triangles > 100, "{stats:?}");
        let canvas = renderer.canvas();
        if let Ok(folder) = std::env::var("GLIDE_SNAPSHOTS") {
            let png = canvas.to_png().expect("a canvas encodes as PNG");
            std::fs::write(format!("{folder}/cpu-test.png"), png).expect("writable");
        }
        let top = canvas.get(80, 2).unwrap();
        // The sky is blue-ish by day.
        assert!(top.b > top.r, "{top:?}");
        assert!(canvas.pixels().iter().all(|p| p.a == 255));
        // The white craft is in the middle.
        let bright = (74..86)
            .flat_map(|x| (44..56).map(move |y| (x, y)))
            .any(|(x, y)| canvas.get(x, y).unwrap().luminance() > 0.7);
        assert!(bright);
    }
}
