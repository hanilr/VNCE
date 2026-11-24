//! Full-screen fragment shaders: [`ShaderQuad`].

use crate::{Gpu, GpuError, RenderTarget};

/// The code added after the user's shader: uniforms, a full-screen triangle and the
/// fragment entry point that calls `shade`. It comes after the user's code so that line
/// numbers in error messages match the user's source.
const PRELUDE: &str = "
// ---- added by vnce_gpu::ShaderQuad ----
struct VnceGlobals {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    pixel_aspect: f32,
    aspect: f32,
}

@group(0) @binding(0) var<uniform> globals: VnceGlobals;

@vertex
fn vnce_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that covers the whole target.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn vnce_fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return shade(position.xy / globals.resolution, globals.time, globals.resolution);
}
";

/// The uniforms of [`PRELUDE`].
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    resolution: [f32; 2],
    time: f32,
    frame: u32,
    pixel_aspect: f32,
    aspect: f32,
    // Uniform buffers are sized in steps of 16 bytes.
    padding: [f32; 2],
}

/// The complete WGSL module for a user `shade` function.
#[must_use]
pub(crate) fn wrap_source(source: &str) -> String {
    format!("{source}\n{PRELUDE}")
}

/// Checks a WGSL module and returns a readable error pointing at the problem.
pub(crate) fn validate(source: &str) -> Result<(), GpuError> {
    use wgpu::naga;

    let module = naga::front::wgsl::parse_str(source)
        .map_err(|error| GpuError::Shader(error.emit_to_string(source)))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|error| GpuError::Shader(error.emit_to_string(source)))?;
    Ok(())
}

/// A fragment shader that fills a whole [`RenderTarget`], like a Shadertoy.
///
/// Write one WGSL function:
///
/// ```wgsl
/// fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
///     return vec4<f32>(uv, 0.5 + 0.5 * sin(time), 1.0);
/// }
/// ```
///
/// - `uv` goes from `(0, 0)` at the top-left corner to `(1, 1)` at the bottom-right
///   corner (pixel centers, like canvas coordinates).
/// - `time` is the value passed to [`ShaderQuad::render`], `resolution` the size of the
///   target in pixels.
/// - The result is a linear color; the target stores it as sRGB.
/// - `globals.frame` holds the frame number passed to [`ShaderQuad::render`].
/// - `globals.pixel_aspect` is the width / height of one pixel on the screen (see
///   [`ShaderQuad::set_pixel_aspect`]) and `globals.aspect` the real width / height of the
///   whole image: use it to keep circles round in cell modes, for example
///   `(uv - 0.5) * vec2<f32>(globals.aspect, 1.0)`.
///
/// Helper functions, constants and structs may be defined next to `shade`. Names starting
/// with `vnce_` / `Vnce` and the name `globals` are reserved.
#[derive(Debug)]
pub struct ShaderQuad {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pixel_aspect: f32,
}

impl ShaderQuad {
    /// Compiles the shader.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::Shader`] with the compiler message when the source does not
    /// compile (line numbers refer to `source`).
    pub fn new(gpu: &Gpu, source: &str) -> Result<Self, GpuError> {
        let full = wrap_source(source);
        validate(&full)?;
        let device = gpu.device();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vnce_gpu shader quad"),
            source: wgpu::ShaderSource::Wgsl(full.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vnce_gpu shader quad"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vnce_gpu shader quad"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vnce_gpu shader quad"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vnce_vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("vnce_fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: RenderTarget::FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vnce_gpu shader quad"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vnce_gpu shader quad"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        Ok(Self {
            pipeline,
            uniforms,
            bind_group,
            pixel_aspect: 1.0,
        })
    }

    /// Sets the width / height of one pixel on the screen (1.0 by default), for example
    /// from `Frame::image_pixel_aspect`. Non-positive values are ignored.
    pub fn set_pixel_aspect(&mut self, pixel_aspect: f32) {
        if pixel_aspect > 0.0 && pixel_aspect.is_finite() {
            self.pixel_aspect = pixel_aspect;
        }
    }

    /// The width / height of one pixel on the screen.
    #[must_use]
    pub fn pixel_aspect(&self) -> f32 {
        self.pixel_aspect
    }

    /// Runs the shader for every pixel of `target`.
    #[allow(clippy::cast_precision_loss, reason = "texture sizes fit in f32")]
    pub fn render(&self, gpu: &Gpu, target: &RenderTarget, time: f32, frame: u32) {
        let globals = Globals {
            resolution: [target.width() as f32, target.height() as f32],
            time,
            frame,
            pixel_aspect: self.pixel_aspect,
            aspect: target.aspect() * self.pixel_aspect,
            padding: [0.0; 2],
        };
        gpu.queue()
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&globals));
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vnce_gpu shader quad"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vnce_gpu shader quad"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..wgpu::RenderPassDescriptor::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue().submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_shade_function_compiles() {
        let source = "fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
    let pixel = uv * resolution;
    return vec4<f32>(fract(pixel.x / 8.0), uv.y * globals.aspect, sin(time) + f32(globals.frame), globals.pixel_aspect);
}";
        validate(&wrap_source(source)).unwrap();
    }

    #[test]
    fn errors_point_at_the_user_source() {
        let source = "fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(uv, missing, 1.0);
}";
        let Err(GpuError::Shader(message)) = validate(&wrap_source(source)) else {
            panic!("expected a shader error");
        };
        assert!(message.contains("missing"), "{message}");
        // Line 2 of the user's source.
        assert!(message.contains(":2:"), "{message}");
    }

    #[test]
    fn a_missing_or_wrong_shade_function_is_an_error() {
        assert!(validate(&wrap_source("")).is_err());
        let wrong = "fn shade(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }";
        assert!(validate(&wrap_source(wrong)).is_err());
    }
}
