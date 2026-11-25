//! Drawing a frame on the GPU: the images as textured quads, then every cell in one
//! instanced draw over them (background, glyph from the atlas, underline and
//! strike-through). Cells with the default background are transparent, so menus and text
//! stay readable over a picture and blank cells let it through.
//!
//! Colors are written as they are (sRGB values into a non-sRGB view), like a terminal
//! mixes them, so a frame looks the same in a window and in a terminal.

use std::collections::HashMap;

use vnce_core::{Buffer, Color, Modifiers, Rect};
use vnce_gpu::wgpu;
use vnce_raster::Canvas;
use vnce_term::{FrameView, ImageView};

use crate::atlas::{ATLAS_SIZE, Atlas, Slot};
use crate::glyphs::{Font, GlyphStyle};

/// Text and background of cells with the default colors.
pub const DEFAULT_FG: [u8; 3] = [229, 229, 229];
pub const DEFAULT_BG: [u8; 3] = [12, 12, 16];

const CELL_SHADER: &str = "
struct Globals {
    cell: vec2<f32>,
    offset: vec2<f32>,
    screen: vec2<f32>,
    line: vec2<f32>,
}

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) glyph: vec4<u32>,
    @location(2) fg: vec4<f32>,
    @location(3) bg: vec4<f32>,
    @location(4) @interpolate(flat) flags: u32,
}

@vertex
fn vs(
    @builtin(vertex_index) index: u32,
    @location(0) cell: vec2<u32>,
    @location(1) glyph: vec4<u32>,
    @location(2) fg: vec4<f32>,
    @location(3) bg: vec4<f32>,
    @location(4) flags: u32,
) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    let span = f32(flags & 3u);
    let size = vec2<f32>(g.cell.x * span, g.cell.y);
    let pixel = g.offset + vec2<f32>(cell) * g.cell + corner * size;
    var out: Out;
    out.position = vec4<f32>(pixel.x / g.screen.x * 2.0 - 1.0, 1.0 - pixel.y / g.screen.y * 2.0, 0.0, 1.0);
    out.local = corner * size;
    out.glyph = glyph;
    out.fg = fg;
    out.bg = bg;
    out.flags = flags;
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    var coverage = 0.0;
    if in.glyph.z > 0u {
        let local = vec2<u32>(clamp(in.local, vec2<f32>(0.0), vec2<f32>(in.glyph.zw) - 1.0));
        coverage = textureLoad(atlas, vec2<i32>(in.glyph.xy + local), 0).r;
    }
    let y = in.local.y;
    let thickness = g.line.x;
    // Underline near the bottom, strike-through in the middle.
    if (in.flags & 4u) != 0u && y >= g.cell.y - thickness * 2.0 && y < g.cell.y - thickness {
        coverage = 1.0;
    }
    if (in.flags & 8u) != 0u && abs(y - g.cell.y * 0.55) < thickness * 0.5 + 0.5 {
        coverage = 1.0;
    }
    // A default background is transparent: only the glyph covers what is below.
    if in.bg.a < 0.5 {
        return vec4<f32>(in.fg.rgb, coverage);
    }
    return vec4<f32>(mix(in.bg.rgb, in.fg.rgb, coverage), 1.0);
}
";

const IMAGE_SHADER: &str = "
struct Globals {
    cell: vec2<f32>,
    offset: vec2<f32>,
    screen: vec2<f32>,
    line: vec2<f32>,
}

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var picture: texture_2d<f32>;
@group(1) @binding(1) var picture_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) index: u32, @location(0) rect: vec4<f32>) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    let pixel = rect.xy + corner * rect.zw;
    var out: Out;
    out.position = vec4<f32>(pixel.x / g.screen.x * 2.0 - 1.0, 1.0 - pixel.y / g.screen.y * 2.0, 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return textureSample(picture, picture_sampler, in.uv);
}
";

/// One cell for the GPU.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct CellInstance {
    cell: [u16; 2],
    glyph: [u16; 4],
    fg: [u8; 4],
    bg: [u8; 4],
    /// Bits 0–1: columns (1 or 2); bit 2: underline; bit 3: strike-through.
    flags: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    cell: [f32; 2],
    offset: [f32; 2],
    target: [f32; 2],
    line: [f32; 2],
}

/// Where the cell grid lies in the target: the cell size and the margin to the top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub cell: (u32, u32),
    pub offset: (u32, u32),
    pub target: (u32, u32),
}

/// A picture's texture, kept for its area while it is shown.
struct Picture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

/// Draws frames into texture views of one format.
pub struct Renderer {
    cells: wgpu::RenderPipeline,
    images: wgpu::RenderPipeline,
    /// For pictures whose colors are premultiplied by their alpha.
    premultiplied: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    cell_group: wgpu::BindGroup,
    image_layout: wgpu::BindGroupLayout,
    globals_group: wgpu::BindGroup,
    atlas_texture: wgpu::Texture,
    atlas: Atlas,
    font: Font,
    instances: Option<(wgpu::Buffer, usize)>,
    image_rects: Option<(wgpu::Buffer, usize)>,
    pictures: HashMap<Rect, Picture>,
    sampler: wgpu::Sampler,
    data: Vec<CellInstance>,
    /// Reused for the bytes of uploaded pictures.
    bytes: Vec<u8>,
}

impl Renderer {
    /// A renderer for targets of `format` with the font at `font_px` pixels.
    #[allow(
        clippy::too_many_lines,
        reason = "the pipelines and their bindings, in order"
    )]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, font_px: f32) -> Self {
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vnce_window globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vnce_window atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let uniform_entry = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture_entry = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let both = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let cell_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vnce_window cells"),
            entries: &[uniform_entry(0, both), texture_entry(1, false)],
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vnce_window globals"),
            entries: &[uniform_entry(0, both)],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vnce_window picture"),
            entries: &[
                texture_entry(0, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let cell_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vnce_window cells"),
            layout: &cell_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
            ],
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vnce_window globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let cells = pipeline(
            device,
            format,
            "vnce_window cells",
            CELL_SHADER,
            &[Some(&cell_layout)],
            wgpu::VertexBufferLayout {
                array_stride: size_of::<CellInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Uint16x2, 1 => Uint16x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Uint32
                ],
            },
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let image_pipeline = |blend| {
            pipeline(
                device,
                format,
                "vnce_window images",
                IMAGE_SHADER,
                &[Some(&globals_layout), Some(&image_layout)],
                wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4],
                },
                Some(blend),
            )
        };
        let images = image_pipeline(wgpu::BlendState::ALPHA_BLENDING);
        let premultiplied = image_pipeline(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vnce_window pictures"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            cells,
            images,
            premultiplied,
            globals,
            cell_group,
            image_layout,
            globals_group,
            atlas_texture,
            atlas: Atlas::default(),
            font: Font::new(font_px),
            instances: None,
            image_rects: None,
            pictures: HashMap::new(),
            sampler,
            data: Vec::new(),
            bytes: Vec::new(),
        }
    }

    /// Size of one cell in pixels.
    pub fn cell_size(&self) -> (u32, u32) {
        self.font.cell_size()
    }

    /// Changes the font size (after a change of the window's scale factor).
    pub fn set_font_px(&mut self, px: f32) {
        self.font = Font::new(px);
        self.atlas.clear();
    }

    /// The grid of cells that fits in `target` pixels, centered, and its layout.
    pub fn layout(&self, target: (u32, u32)) -> ((u16, u16), Layout) {
        let cell = self.cell_size();
        let columns = (target.0 / cell.0).clamp(1, u32::from(u16::MAX));
        let rows = (target.1 / cell.1).clamp(1, u32::from(u16::MAX));
        let offset = (
            target.0.saturating_sub(columns * cell.0) / 2,
            target.1.saturating_sub(rows * cell.1) / 2,
        );
        (
            (columns as u16, rows as u16),
            Layout {
                cell,
                offset,
                target,
            },
        )
    }

    /// Draws `frame` into `view` (cleared to the default background first).
    #[allow(
        clippy::too_many_lines,
        reason = "one frame's uploads and draws, in order"
    )]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        frame: &FrameView<'_>,
        layout: Layout,
    ) {
        self.build_cells(frame.buffer);
        if self.atlas.cleared {
            // The atlas filled up while the frame was built: build it again from scratch.
            self.atlas.cleared = false;
            self.atlas.clear();
            self.build_cells(frame.buffer);
        }
        self.upload_glyphs(queue);
        let line = (layout.cell.1 as f32 / 16.0).round().max(1.0);
        let globals = Globals {
            cell: [layout.cell.0 as f32, layout.cell.1 as f32],
            offset: [layout.offset.0 as f32, layout.offset.1 as f32],
            target: [layout.target.0 as f32, layout.target.1 as f32],
            line: [line, 0.0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        // The images: their textures (uploaded when they changed) and rectangles.
        let mut rects: Vec<[f32; 4]> = Vec::new();
        let mut draws: Vec<Rect> = Vec::new();
        let mut external: Vec<(usize, wgpu::BindGroup, bool)> = Vec::new();
        for image in &frame.images {
            let (area, rect) = match *image {
                ImageView::Pixels {
                    area,
                    canvas,
                    changed,
                } => {
                    self.picture(device, queue, area, canvas, changed);
                    (area, cell_rect(area, layout))
                }
                ImageView::External { area, image } => {
                    let Some(handle) = image.handle().downcast_ref::<wgpu::TextureView>() else {
                        continue;
                    };
                    let group = self.bind_picture(device, handle);
                    external.push((rects.len(), group, image.is_premultiplied()));
                    (area, cell_rect(area, layout))
                }
            };
            rects.push(rect);
            draws.push(area);
        }
        self.pictures.retain(|area, _| draws.contains(area));
        let instances = upload(
            device,
            queue,
            &mut self.instances,
            bytemuck::cast_slice(&self.data),
            self.data.len(),
            "vnce_window cells",
        );
        let rect_buffer = upload(
            device,
            queue,
            &mut self.image_rects,
            bytemuck::cast_slice(&rects),
            rects.len(),
            "vnce_window image rectangles",
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vnce_window frame"),
        });
        {
            let [r, g, b] = DEFAULT_BG.map(|c| f64::from(c) / 255.0);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vnce_window frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..wgpu::RenderPassDescriptor::default()
            });
            if let Some(buffer) = rect_buffer {
                pass.set_bind_group(0, &self.globals_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                for (index, area) in draws.iter().enumerate() {
                    let found = external.iter().find(|(i, _, _)| *i == index);
                    let premultiplied = found.is_some_and(|(_, _, p)| *p);
                    let group = found
                        .map(|(_, group, _)| group)
                        .or_else(|| self.pictures.get(area).map(|p| &p.bind_group));
                    if let Some(group) = group {
                        pass.set_pipeline(if premultiplied {
                            &self.premultiplied
                        } else {
                            &self.images
                        });
                        pass.set_bind_group(1, group, &[]);
                        let i = index as u32;
                        pass.draw(0..6, i..i + 1);
                    }
                }
            }
            if let Some(buffer) = instances {
                pass.set_pipeline(&self.cells);
                pass.set_bind_group(0, &self.cell_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..6, 0..self.data.len() as u32);
            }
        }
        queue.submit([encoder.finish()]);
    }

    /// One instance per cell (wide symbols span two cells and skip the next).
    fn build_cells(&mut self, buffer: &Buffer) {
        self.data.clear();
        let size = buffer.size();
        for y in 0..size.height {
            for x in 0..size.width {
                let Some(cell) = buffer.cell(x, y) else {
                    continue;
                };
                if cell.is_continuation() {
                    continue;
                }
                let columns = cell.width().clamp(1, 2) as u8;
                let modifiers = cell.modifiers;
                let rgb = |color: Color, default: [u8; 3]| {
                    color.to_rgb().map_or(default, |(r, g, b)| [r, g, b])
                };
                let mut fg = rgb(cell.fg, DEFAULT_FG);
                let mut bg = rgb(cell.bg, DEFAULT_BG);
                let reverse = modifiers.contains(Modifiers::REVERSE);
                // The default background lets images through.
                let opaque = cell.bg != Color::Reset || reverse;
                if reverse {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if modifiers.contains(Modifiers::DIM) {
                    fg = [0, 1, 2]
                        .map(|i| ((u16::from(fg[i]) * 11 + u16::from(bg[i]) * 9) / 20) as u8);
                }
                let hidden = modifiers.contains(Modifiers::HIDDEN);
                if hidden {
                    fg = bg;
                }
                let style = GlyphStyle {
                    bold: modifiers.contains(Modifiers::BOLD),
                    italic: modifiers.contains(Modifiers::ITALIC),
                };
                let symbol = cell.symbol();
                let slot = if symbol == " " || hidden {
                    Slot::default()
                } else {
                    self.atlas.slot(&self.font, symbol, style, columns)
                };
                let mut flags = u32::from(columns);
                if modifiers.contains(Modifiers::UNDERLINE) {
                    flags |= 4;
                }
                if modifiers.contains(Modifiers::STRIKETHROUGH) {
                    flags |= 8;
                }
                self.data.push(CellInstance {
                    cell: [x, y],
                    glyph: [slot.x, slot.y, slot.width, slot.height],
                    fg: [fg[0], fg[1], fg[2], 255],
                    bg: [bg[0], bg[1], bg[2], if opaque { 255 } else { 0 }],
                    flags,
                });
            }
        }
    }

    fn upload_glyphs(&mut self, queue: &wgpu::Queue) {
        for (slot, glyph) in self.atlas.take_uploads() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: u32::from(slot.x),
                        y: u32::from(slot.y),
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &glyph.coverage,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(glyph.width),
                    rows_per_image: Some(glyph.height),
                },
                wgpu::Extent3d {
                    width: glyph.width,
                    height: glyph.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Makes the texture of the picture in `area` hold `canvas`.
    fn picture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        area: Rect,
        canvas: &Canvas,
        changed: bool,
    ) {
        let size = (canvas.width(), canvas.height());
        let fresh = self.pictures.get(&area).is_none_or(|p| p.size != size);
        if fresh {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vnce_window picture"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.bind_picture(device, &view);
            self.pictures.insert(
                area,
                Picture {
                    texture,
                    bind_group,
                    size,
                },
            );
        }
        if fresh || changed {
            self.bytes.clear();
            self.bytes
                .extend(canvas.pixels().iter().flat_map(|p| [p.r, p.g, p.b, p.a]));
            if let Some(picture) = self.pictures.get(&area) {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &picture.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &self.bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size.0 * 4),
                        rows_per_image: Some(size.1),
                    },
                    wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
    }

    fn bind_picture(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vnce_window picture"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

/// The pixel rectangle `[x, y, width, height]` of cell area `area`.
fn cell_rect(area: Rect, layout: Layout) -> [f32; 4] {
    let (cw, ch) = (layout.cell.0 as f32, layout.cell.1 as f32);
    [
        layout.offset.0 as f32 + f32::from(area.x) * cw,
        layout.offset.1 as f32 + f32::from(area.y) * ch,
        f32::from(area.width) * cw,
        f32::from(area.height) * ch,
    ]
}

/// Writes `bytes` into the vertex buffer in `slot`, growing it when needed. Returns the
/// buffer, or `None` when there is nothing to draw.
fn upload<'a>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    slot: &'a mut Option<(wgpu::Buffer, usize)>,
    bytes: &[u8],
    count: usize,
    label: &str,
) -> Option<&'a wgpu::Buffer> {
    if count == 0 {
        return None;
    }
    if slot.as_ref().is_none_or(|(_, size)| *size < bytes.len()) {
        let size = bytes.len().next_power_of_two();
        *slot = Some((
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            size,
        ));
    }
    let (buffer, _) = slot.as_ref()?;
    queue.write_buffer(buffer, 0, bytes);
    Some(buffer)
}

#[allow(clippy::too_many_arguments, reason = "the parts of a pipeline")]
fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    label: &str,
    source: &str,
    layouts: &[Option<&wgpu::BindGroupLayout>],
    vertices: wgpu::VertexBufferLayout<'_>,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertices)],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use vnce_core::{Cell, Size, Style};
    use vnce_gpu::{Gpu, GpuOptions, RenderTarget};
    use vnce_raster::Rgba;

    use super::*;

    /// Draws a small frame into a texture (on a software adapter when there is no GPU)
    /// and checks its pixels. Skipped without any adapter.
    #[test]
    fn cells_and_images_are_drawn_where_they_belong() {
        let Ok(gpu) = Gpu::new(GpuOptions::default()) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let mut renderer = Renderer::new(gpu.device(), RenderTarget::FORMAT, 16.0);
        let (cw, ch) = renderer.cell_size();
        let (grid, layout) = renderer.layout((cw * 4 + 3, ch * 2 + 1));
        assert_eq!(grid, (4, 2));
        assert_eq!(layout.offset, (1, 0));
        let mut buffer = Buffer::empty(Size::new(4, 2));
        let red = Style::new().fg(Color::rgb(255, 0, 0));
        buffer.set_string(0, 0, "█", red);
        if let Some(cell) = buffer.cell_mut(1, 0) {
            *cell = Cell::EMPTY;
            cell.bg = Color::rgb(0, 0, 255);
        }
        buffer.set_string(
            2,
            0,
            "▀",
            Style::new()
                .fg(Color::rgb(255, 255, 255))
                .bg(Color::rgb(0, 0, 0)),
        );
        let picture = Canvas::filled(4, 4, Rgba::rgb(0, 255, 0));
        let frame = FrameView {
            buffer: &buffer,
            images: vec![ImageView::Pixels {
                area: Rect::new(0, 1, 2, 1),
                canvas: &picture,
                changed: true,
            }],
            full_redraw: true,
            cell_pixels: (cw as u16, ch as u16),
        };
        let mut target = RenderTarget::new(&gpu, layout.target.0, layout.target.1);
        renderer.render(gpu.device(), gpu.queue(), target.view(), &frame, layout);
        let mut canvas = Canvas::new(0, 0);
        target.read_into(&gpu, &mut canvas).unwrap();
        let at = |column: u32, row: u32, fx: f32, fy: f32| {
            let x = layout.offset.0 + column * cw + (fx * cw as f32) as u32;
            let y = layout.offset.1 + row * ch + (fy * ch as f32) as u32;
            canvas.get(x, y).unwrap()
        };
        assert_eq!(
            at(0, 0, 0.5, 0.5),
            Rgba::rgb(255, 0, 0),
            "a full block in red"
        );
        assert_eq!(
            at(1, 0, 0.5, 0.5),
            Rgba::rgb(0, 0, 255),
            "a blue background"
        );
        assert_eq!(at(2, 0, 0.5, 0.2), Rgba::rgb(255, 255, 255), "upper half");
        assert_eq!(at(2, 0, 0.5, 0.8), Rgba::rgb(0, 0, 0), "lower half");
        assert_eq!(
            at(1, 1, 0.5, 0.5),
            Rgba::rgb(0, 255, 0),
            "the picture over the cells"
        );
        // An unchanged picture stays; one that is gone is forgotten.
        let frame = FrameView {
            images: Vec::new(),
            ..frame
        };
        renderer.render(gpu.device(), gpu.queue(), target.view(), &frame, layout);
        assert!(renderer.pictures.is_empty());
    }
}
