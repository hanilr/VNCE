//! Colored triangle meshes: [`Mesh`] and [`MeshRenderer`], lit by a sun and ambient light
//! ([`Light`]) and faded into distance fog ([`Fog`]).

use bytemuck::Zeroable as _;
use vnce_raster::Rgba;
use wgpu::util::DeviceExt;

use crate::math::{cross, dot, normalize, sub};
use crate::{Gpu, Mat4, RenderTarget, Vec3, color};

const SHADER: &str = "
struct Draw {
    transform: mat4x4<f32>,
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
}

struct Scene {
    // xyz: towards the sun (normalized); w: 1 when lighting is on.
    sun_direction: vec4<f32>,
    sun_color: vec4<f32>,
    ambient: vec4<f32>,
    // Linear RGBA.
    fog_color: vec4<f32>,
    // xyz: camera position; w: 1 when fog is on.
    camera: vec4<f32>,
    // x: start distance, y: 1 / (end - start).
    fog_range: vec4<f32>,
}

@group(0) @binding(0) var<uniform> draw: Draw;
@group(0) @binding(1) var<uniform> scene: Scene;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world: vec3<f32>,
    @location(2) normal: vec3<f32>,
}

@vertex
fn vnce_vertex(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = draw.transform * vec4<f32>(position, 1.0);
    out.world = (draw.model * vec4<f32>(position, 1.0)).xyz;
    out.normal = (draw.normal * vec4<f32>(normal, 0.0)).xyz;
    out.color = color;
    return out;
}

@fragment
fn vnce_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;
    // Vertices without a normal are never lit.
    if scene.sun_direction.w > 0.5 && dot(in.normal, in.normal) > 1e-12 {
        let facing = max(dot(normalize(in.normal), scene.sun_direction.xyz), 0.0);
        color = vec4<f32>(color.rgb * (scene.ambient.rgb + scene.sun_color.rgb * facing), color.a);
    }
    if scene.camera.w > 0.5 {
        let distance = length(in.world - scene.camera.xyz);
        let amount = clamp((distance - scene.fog_range.x) * scene.fog_range.y, 0.0, 1.0);
        color = mix(color, scene.fog_color, amount);
    }
    return color;
}
";

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The per-draw uniform: transforms to clip space and to world space, and the matrix for
/// normals.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    transform: Mat4,
    model: Mat4,
    normal: Mat4,
}

/// The scene uniform (see `Scene` in the shader).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniform {
    sun_direction: [f32; 4],
    sun_color: [f32; 4],
    ambient: [f32; 4],
    fog_color: [f32; 4],
    camera: [f32; 4],
    fog_range: [f32; 4],
}

/// Size of one draw's uniform (its transforms) in the uniform buffer.
const DRAW_SIZE: u32 = 192;
const _: () = assert!(DRAW_SIZE as usize == size_of::<DrawUniform>());

/// A mesh vertex: a position, a normal and a linear RGBA color.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Position in model space.
    pub position: [f32; 3],
    /// Normal in model space, pointing out of the front face. Its length does not matter;
    /// a zero normal means the vertex is never lit (its color is drawn as it is).
    pub normal: [f32; 3],
    /// Linear color (see [`Vertex::new`] to use a canvas color).
    pub color: [f32; 4],
}

impl Vertex {
    /// A vertex with an sRGB canvas color and no normal: drawn in exactly this color, with
    /// or without a [`Light`].
    #[must_use]
    pub fn new(position: [f32; 3], color: Rgba) -> Self {
        Self::lit(position, [0.0; 3], color)
    }

    /// A vertex with a normal and an sRGB canvas color, lit by the renderer's [`Light`].
    #[must_use]
    pub fn lit(position: [f32; 3], normal: [f32; 3], color: Rgba) -> Self {
        Self {
            position,
            normal,
            color: color::linear_rgba(color),
        }
    }

    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Flat shading: gives every triangle its own three vertices, all with the triangle's
/// normal (counter-clockwise triangles face the viewer), so each face is lit evenly and
/// its edges stay sharp. Returns the new vertices and indices; colors are kept per vertex.
/// Triangles with an index out of range, and leftover indices (fewer than three), are
/// skipped.
///
/// # Panics
///
/// Panics if the result has more than `u32::MAX` vertices.
#[must_use]
pub fn flat_shaded(vertices: &[Vertex], indices: &[u32]) -> (Vec<Vertex>, Vec<u32>) {
    let mut flat = Vec::with_capacity(indices.len());
    for triangle in indices.chunks_exact(3) {
        let corner = |k: usize| vertices.get(triangle[k] as usize).copied();
        let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else {
            continue;
        };
        let normal = normalize(cross(
            sub(b.position, a.position),
            sub(c.position, a.position),
        ));
        flat.extend([a, b, c].map(|v| Vertex { normal, ..v }));
    }
    let count = u32::try_from(flat.len()).expect("too many vertices");
    (flat, (0..count).collect())
}

/// Triangles stored on the GPU: vertices and indices (three per triangle,
/// counter-clockwise when seen from the front).
#[derive(Debug)]
pub struct Mesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

impl Mesh {
    /// Uploads a mesh.
    ///
    /// # Panics
    ///
    /// Panics if there are more than `u32::MAX` indices.
    #[must_use]
    pub fn new(gpu: &Gpu, vertices: &[Vertex], indices: &[u32]) -> Self {
        let device = gpu.device();
        // Empty buffers are not allowed on every backend.
        let vertex_bytes: &[u8] = if vertices.is_empty() {
            &[0; size_of::<Vertex>()]
        } else {
            bytemuck::cast_slice(vertices)
        };
        let index_bytes: &[u8] = if indices.is_empty() {
            &[0; 4]
        } else {
            bytemuck::cast_slice(indices)
        };
        Self {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vnce_gpu mesh vertices"),
                contents: vertex_bytes,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vnce_gpu mesh indices"),
                contents: index_bytes,
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: u32::try_from(indices.len()).expect("too many indices"),
        }
    }

    /// Number of indices (three per triangle).
    #[must_use]
    pub fn index_count(&self) -> u32 {
        self.index_count
    }
}

/// Options for [`MeshRenderer::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshRendererOptions {
    /// Multisample anti-aliasing: samples per pixel (1 = off). Lowered automatically to
    /// what the GPU supports.
    pub msaa: u32,
    /// Skip triangles seen from the back (counter-clockwise triangles are the front).
    pub cull_back_faces: bool,
}

impl Default for MeshRendererOptions {
    fn default() -> Self {
        Self {
            msaa: 4,
            cull_back_faces: false,
        }
    }
}

/// One mesh to draw with its transforms.
#[derive(Debug, Clone, Copy)]
pub struct MeshDraw<'a> {
    /// The mesh.
    pub mesh: &'a Mesh,
    /// Transform from model space to clip space (usually `projection * view * model`).
    pub transform: Mat4,
    /// Transform from model space to world space (`model`), for lighting (normals) and fog
    /// (the distance to the camera).
    pub model: Mat4,
}

impl<'a> MeshDraw<'a> {
    /// A draw of `mesh` placed in the world by `model` and seen through `view_projection`
    /// (`projection * view`).
    #[must_use]
    pub fn new(mesh: &'a Mesh, view_projection: Mat4, model: Mat4) -> Self {
        Self {
            mesh,
            transform: view_projection * model,
            model,
        }
    }
}

/// A sun (a directional light) and ambient light for vertices with a normal: their color
/// is multiplied by `ambient + sun_color * max(0, normal · direction)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    /// Direction from the scene towards the sun, in world space (any length).
    pub direction: Vec3,
    /// Linear RGB of the sun; values above 1 are allowed.
    pub sun_color: [f32; 3],
    /// Linear RGB light that reaches every face, also those facing away from the sun.
    pub ambient: [f32; 3],
}

impl Default for Light {
    /// A white sun high up, to the right and in front (+z), and a little blue-gray ambient
    /// light.
    fn default() -> Self {
        Self {
            direction: [0.4, 1.0, 0.6],
            sun_color: [0.85, 0.85, 0.8],
            ambient: [0.25, 0.27, 0.32],
        }
    }
}

/// Distance fog: past `start` (world units from the camera) colors fade towards `color`,
/// reaching it at `end`.
///
/// The alpha of `color` is faded to as well: with an opaque color far meshes disappear in
/// the fog; with alpha 0 they fade out, so what is behind the image (a sky under a
/// transparent background) shows through. Partly transparent pixels then hold
/// premultiplied colors, as with any blended drawing on a transparent background.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fog {
    /// The color far away (usually the background or horizon color).
    pub color: Rgba,
    /// Distance where the fog begins.
    pub start: f32,
    /// Distance where only the fog color is left.
    pub end: f32,
}

/// Lighting and fog for a [`MeshRenderer`] (see [`MeshRenderer::set_scene`]). The default
/// draws every color as it is.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeshScene {
    /// Sun and ambient light; `None` draws colors unlit.
    pub light: Option<Light>,
    /// Distance fog; `None` for none.
    pub fog: Option<Fog>,
    /// Camera position in world space (the fog is measured from it).
    pub camera: Vec3,
}

impl MeshScene {
    fn uniform(&self) -> SceneUniform {
        let mut uniform = SceneUniform::zeroed();
        if let Some(light) = self.light {
            uniform.sun_direction = extend(normalize(light.direction), 1.0);
            uniform.sun_color = extend(light.sun_color, 0.0);
            uniform.ambient = extend(light.ambient, 0.0);
        }
        uniform.camera = extend(self.camera, 0.0);
        if let Some(fog) = self.fog {
            uniform.camera[3] = 1.0;
            uniform.fog_color = color::linear_rgba(fog.color);
            // A fog without depth (end at or before start) is a hard edge at `start`.
            uniform.fog_range = [fog.start, 1.0 / (fog.end - fog.start).max(1e-6), 0.0, 0.0];
        }
        uniform
    }
}

/// `v` with a fourth component `w`.
fn extend(v: Vec3, w: f32) -> [f32; 4] {
    [v[0], v[1], v[2], w]
}

/// The matrix that turns model-space normals into world-space normals: the inverse
/// transpose of the upper 3×3 of `model`, up to a positive factor (the shader normalizes).
fn normal_matrix(model: &Mat4) -> Mat4 {
    let column = |i: usize| [model.cols[i][0], model.cols[i][1], model.cols[i][2]];
    let (a, b, c) = (column(0), column(1), column(2));
    // The columns of the cofactor matrix, which is det × the inverse transpose.
    let cofactors = [cross(b, c), cross(c, a), cross(a, b)];
    let sign = if dot(a, cofactors[0]) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let mut m = Mat4::IDENTITY;
    for (col, cofactor) in m.cols.iter_mut().zip(cofactors) {
        *col = [
            sign * cofactor[0],
            sign * cofactor[1],
            sign * cofactor[2],
            0.0,
        ];
    }
    m
}

/// Multisampled color and depth textures matching a target size.
#[derive(Debug)]
struct Attachments {
    size: (u32, u32),
    color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
}

/// Draws colored meshes with a depth buffer and multisample anti-aliasing, optionally lit
/// and fogged ([`MeshScene`]).
#[derive(Debug)]
pub struct MeshRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    samples: u32,
    /// Distance between the transforms of two draws in the uniform buffer.
    stride: u32,
    scene: MeshScene,
    scene_buffer: wgpu::Buffer,
    uniforms: Option<(wgpu::Buffer, wgpu::BindGroup, usize)>,
    attachments: Option<Attachments>,
}

impl MeshRenderer {
    /// Creates a renderer.
    #[must_use]
    pub fn new(gpu: &Gpu, options: MeshRendererOptions) -> Self {
        let samples = gpu
            .sample_count(RenderTarget::FORMAT, options.msaa)
            .min(gpu.sample_count(DEPTH_FORMAT, options.msaa));
        let device = gpu.device();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vnce_gpu mesh"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vnce_gpu mesh"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(u64::from(DRAW_SIZE)),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<SceneUniform>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vnce_gpu mesh"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vnce_gpu mesh"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vnce_vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: options.cull_back_faces.then_some(wgpu::Face::Back),
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..wgpu::MultisampleState::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("vnce_fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: RenderTarget::FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let alignment = device.limits().min_uniform_buffer_offset_alignment.max(1);
        let scene = MeshScene::default();
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vnce_gpu mesh scene"),
            contents: bytemuck::bytes_of(&scene.uniform()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            layout,
            samples,
            stride: DRAW_SIZE.next_multiple_of(alignment),
            scene,
            scene_buffer,
            uniforms: None,
            attachments: None,
        }
    }

    /// The multisample count actually used (1 when anti-aliasing is off or unsupported).
    #[must_use]
    pub fn samples(&self) -> u32 {
        self.samples
    }

    /// Sets the lighting and fog of the following renders.
    pub fn set_scene(&mut self, gpu: &Gpu, scene: MeshScene) {
        if scene != self.scene {
            self.scene = scene;
            gpu.queue()
                .write_buffer(&self.scene_buffer, 0, bytemuck::bytes_of(&scene.uniform()));
        }
    }

    /// The lighting and fog in use.
    #[must_use]
    pub fn scene(&self) -> &MeshScene {
        &self.scene
    }

    /// Clears `target` to `background` and draws `draws` in order, with depth testing.
    ///
    /// # Panics
    ///
    /// Panics if `draws` is so long that the transforms do not fit in 4 GiB.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        target: &RenderTarget,
        background: Rgba,
        draws: &[MeshDraw<'_>],
    ) {
        self.prepare(gpu, target, draws);
        let Some(attachments) = &self.attachments else {
            return;
        };
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vnce_gpu mesh"),
            });
        {
            let (view, resolve_target) = match &attachments.color {
                Some(multisampled) => (multisampled, Some(target.view())),
                None => (target.view(), None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vnce_gpu mesh"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color::clear_color(background)),
                        // The multisampled image is only needed until it is resolved.
                        store: if resolve_target.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &attachments.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..wgpu::RenderPassDescriptor::default()
            });
            if let Some((_, bind_group, _)) = &self.uniforms {
                pass.set_pipeline(&self.pipeline);
                for (index, draw) in draws.iter().enumerate() {
                    if draw.mesh.index_count == 0 {
                        continue;
                    }
                    let offset = u32::try_from(index)
                        .ok()
                        .and_then(|index| index.checked_mul(self.stride))
                        .expect("too many draws in one call");
                    pass.set_bind_group(0, bind_group, &[offset]);
                    pass.set_vertex_buffer(0, draw.mesh.vertices.slice(..));
                    pass.set_index_buffer(draw.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..draw.mesh.index_count, 0, 0..1);
                }
            }
        }
        gpu.queue().submit([encoder.finish()]);
    }

    /// Makes the attachments match the target and uploads the transforms.
    fn prepare(&mut self, gpu: &Gpu, target: &RenderTarget, draws: &[MeshDraw<'_>]) {
        let size = (target.width(), target.height());
        if self.attachments.as_ref().is_none_or(|a| a.size != size) {
            self.attachments = Some(self.create_attachments(gpu, size));
        }
        if draws.is_empty() {
            return;
        }
        let device = gpu.device();
        if self
            .uniforms
            .as_ref()
            .is_none_or(|(_, _, capacity)| *capacity < draws.len())
        {
            let capacity = draws.len().next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vnce_gpu mesh transforms"),
                size: u64::from(self.stride) * capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vnce_gpu mesh transforms"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(u64::from(DRAW_SIZE)),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.scene_buffer.as_entire_binding(),
                    },
                ],
            });
            self.uniforms = Some((buffer, bind_group, capacity));
        }
        let mut bytes = vec![0_u8; self.stride as usize * draws.len()];
        for (chunk, draw) in bytes.chunks_mut(self.stride as usize).zip(draws) {
            let uniform = DrawUniform {
                transform: draw.transform,
                model: draw.model,
                normal: normal_matrix(&draw.model),
            };
            chunk[..size_of::<DrawUniform>()].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        if let Some((buffer, _, _)) = &self.uniforms {
            gpu.queue().write_buffer(buffer, 0, &bytes);
        }
    }

    fn create_attachments(&self, gpu: &Gpu, size: (u32, u32)) -> Attachments {
        let texture = |label, format| {
            gpu.device()
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: self.samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        Attachments {
            size,
            color: (self.samples > 1).then(|| texture("vnce_gpu mesh msaa", RenderTarget::FORMAT)),
            depth: texture("vnce_gpu mesh depth", DEPTH_FORMAT),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: Vec3, expected: Vec3) {
        for (a, e) in actual.iter().zip(&expected) {
            assert!((a - e).abs() < 1e-5, "{actual:?} != {expected:?}");
        }
    }

    /// Applies the upper 3×3 of `m` to `v` and normalizes, as the shader does.
    fn apply(m: &Mat4, v: Vec3) -> Vec3 {
        normalize([0, 1, 2].map(|row| (0..3).map(|k| m.cols[k][row] * v[k]).sum()))
    }

    #[test]
    fn normals_follow_rotation_and_ignore_translation() {
        let model =
            Mat4::translation([5.0, 6.0, 7.0]) * Mat4::rotation_z(std::f32::consts::FRAC_PI_2);
        assert_near(
            apply(&normal_matrix(&model), [1.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0],
        );
    }

    #[test]
    fn normals_stay_perpendicular_under_uneven_scale() {
        // The slope x = y stretched by 2 along x becomes x = 2y: its normal (1, -1) turns
        // into (1, -2), not (2, -1).
        let model = Mat4::scale([2.0, 1.0, 1.0]);
        assert_near(
            apply(&normal_matrix(&model), [1.0, -1.0, 0.0]),
            normalize([1.0, -2.0, 0.0]),
        );
        // A mirror keeps normals pointing out.
        let mirror = Mat4::scale([-1.0, 1.0, 1.0]);
        assert_near(
            apply(&normal_matrix(&mirror), [1.0, 0.0, 0.0]),
            [-1.0, 0.0, 0.0],
        );
    }

    #[test]
    fn flat_shading_splits_vertices_and_sets_face_normals() {
        let white = Rgba::rgb(255, 255, 255);
        let vertices = [
            Vertex::new([0.0, 0.0, 0.0], white),
            Vertex::new([1.0, 0.0, 0.0], white),
            Vertex::new([0.0, 1.0, 0.0], white),
            Vertex::new([0.0, 0.0, 1.0], white),
        ];
        // Counter-clockwise seen from +z, then from +x; a bad index and a leftover are
        // skipped.
        let (flat, indices) = flat_shaded(&vertices, &[0, 1, 2, 0, 2, 3, 0, 1, 9, 0]);
        assert_eq!(flat.len(), 6);
        assert_eq!(indices, vec![0, 1, 2, 3, 4, 5]);
        for (i, vertex) in flat.iter().enumerate() {
            let expected = if i < 3 {
                [0.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            assert_near(vertex.normal, expected);
        }
        assert_near(flat[4].position, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn the_default_scene_is_unlit_without_fog() {
        let uniform = MeshScene::default().uniform();
        assert!(uniform.sun_direction[3] == 0.0 && uniform.camera[3] == 0.0);
        let hard_edge = MeshScene {
            fog: Some(Fog {
                color: Rgba::rgb(255, 255, 255),
                start: 10.0,
                end: 10.0,
            }),
            ..MeshScene::default()
        };
        assert!(hard_edge.uniform().fog_range[1].is_finite());
    }
}
