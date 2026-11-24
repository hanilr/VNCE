//! Colored triangle meshes: [`Mesh`] and [`MeshRenderer`].

use vnce_raster::Rgba;
use wgpu::util::DeviceExt;

use crate::{Gpu, Mat4, RenderTarget, color};

const SHADER: &str = "
@group(0) @binding(0) var<uniform> transform: mat4x4<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vnce_vertex(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = transform * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn vnce_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
";

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Size of one transform in the uniform buffer.
const MAT4_SIZE: u32 = 64;
const _: () = assert!(MAT4_SIZE as usize == size_of::<Mat4>());

/// A mesh vertex: a position and a linear RGBA color.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Position in model space.
    pub position: [f32; 3],
    /// Linear color (see [`Vertex::new`] to use a canvas color).
    pub color: [f32; 4],
}

impl Vertex {
    /// A vertex with an sRGB canvas color.
    #[must_use]
    pub fn new(position: [f32; 3], color: Rgba) -> Self {
        Self {
            position,
            color: color::linear_rgba(color),
        }
    }

    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
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

/// One mesh to draw with its transform (usually `projection * view * model`).
#[derive(Debug, Clone, Copy)]
pub struct MeshDraw<'a> {
    /// The mesh.
    pub mesh: &'a Mesh,
    /// Transform from model space to clip space.
    pub transform: Mat4,
}

/// Multisampled color and depth textures matching a target size.
#[derive(Debug)]
struct Attachments {
    size: (u32, u32),
    color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
}

/// Draws colored meshes with a depth buffer and multisample anti-aliasing.
#[derive(Debug)]
pub struct MeshRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    samples: u32,
    /// Distance between the transforms of two draws in the uniform buffer.
    stride: u32,
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
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<Mat4>() as u64),
                },
                count: None,
            }],
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
        Self {
            pipeline,
            layout,
            samples,
            stride: MAT4_SIZE.next_multiple_of(alignment),
            uniforms: None,
            attachments: None,
        }
    }

    /// The multisample count actually used (1 when anti-aliasing is off or unsupported).
    #[must_use]
    pub fn samples(&self) -> u32 {
        self.samples
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
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<Mat4>() as u64),
                    }),
                }],
            });
            self.uniforms = Some((buffer, bind_group, capacity));
        }
        let mut bytes = vec![0_u8; self.stride as usize * draws.len()];
        for (chunk, draw) in bytes.chunks_mut(self.stride as usize).zip(draws) {
            chunk[..size_of::<Mat4>()].copy_from_slice(bytemuck::bytes_of(&draw.transform));
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
