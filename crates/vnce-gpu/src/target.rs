//! Offscreen render targets and reading them back into a [`Canvas`].

use std::sync::mpsc;

use vnce_raster::{Canvas, Rgba};

use crate::{Gpu, GpuError, color};

/// How [`RenderTarget::read_into`] reads the image back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Readback {
    /// Waits for the GPU and returns the frame just rendered. Simple and without latency;
    /// the CPU waits while the GPU renders and copies.
    #[default]
    Immediate,
    /// Starts copying the frame just rendered and returns the previous one, so the CPU
    /// never waits for the GPU (one frame of latency). Best for large images.
    Pipelined,
}

/// A buffer the target is copied into, with its row layout.
#[derive(Debug)]
struct ReadBuffer {
    buffer: wgpu::Buffer,
}

/// A copy that was submitted and is being mapped.
#[derive(Debug)]
struct Pending {
    buffer: usize,
    submission: wgpu::SubmissionIndex,
    mapped: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    width: u32,
    height: u32,
}

/// An offscreen image on the GPU that can be rendered to and read back as a [`Canvas`].
///
/// The texture format is [`RenderTarget::FORMAT`] (sRGB): shaders write linear colors and
/// the canvas receives sRGB pixels, like every other canvas. Render to
/// [`RenderTarget::view`] with your own `wgpu` render passes, or with the helpers of this
/// crate, then call [`RenderTarget::read_into`].
#[derive(Debug)]
pub struct RenderTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    readback: Readback,
    buffers: [Option<ReadBuffer>; 2],
    next: usize,
    pending: Option<Pending>,
}

impl RenderTarget {
    /// The texture format of every render target.
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    /// Creates a render target of `width` × `height` pixels. Sizes are clamped to
    /// `1..=` the largest texture size of the device.
    #[must_use]
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Self {
        let (width, height) = clamp_size(gpu, width, height);
        let (texture, view) = create_texture(gpu, width, height);
        Self {
            texture,
            view,
            width,
            height,
            readback: Readback::default(),
            buffers: [None, None],
            next: 0,
            pending: None,
        }
    }

    /// Sets how the image is read back (builder style).
    #[must_use]
    pub fn with_readback(mut self, readback: Readback) -> Self {
        self.set_readback(readback);
        self
    }

    /// Sets how the image is read back. A frame being read in the background is dropped.
    pub fn set_readback(&mut self, readback: Readback) {
        if readback != self.readback {
            self.readback = readback;
            self.drop_pending();
        }
    }

    /// How the image is read back.
    #[must_use]
    pub fn readback(&self) -> Readback {
        self.readback
    }

    /// Changes the size, keeping the target when the size is the same. The contents are
    /// undefined until the next render; a frame being read in the background is dropped.
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        let (width, height) = clamp_size(gpu, width, height);
        if (width, height) == (self.width, self.height) {
            return;
        }
        self.drop_pending();
        let (texture, view) = create_texture(gpu, width, height);
        self.texture = texture;
        self.view = view;
        self.width = width;
        self.height = height;
        self.buffers = [None, None];
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Width divided by height, for projection matrices (assumes square pixels).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "texture sizes fit in f32")]
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }

    /// The texture, for copies and advanced use.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// The view to render to (color attachment of a render pass).
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Fills the target with `color`.
    pub fn clear(&self, gpu: &Gpu, color: Rgba) {
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vnce_gpu clear"),
            });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vnce_gpu clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color::clear_color(color)),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..wgpu::RenderPassDescriptor::default()
        });
        gpu.queue().submit([encoder.finish()]);
    }

    /// Reads the image back into `canvas`, resizing it to the size of the target.
    ///
    /// Returns `true` when `canvas` was updated. With [`Readback::Pipelined`] the canvas
    /// gets the previous frame, so the first call after creating, resizing or switching
    /// modes returns `false` and leaves the canvas unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::Readback`] when the GPU cannot map the buffer (for example after
    /// a device loss).
    pub fn read_into(&mut self, gpu: &Gpu, canvas: &mut Canvas) -> Result<bool, GpuError> {
        let buffer = self.next;
        if self.readback == Readback::Pipelined {
            self.next = 1 - self.next;
        }
        let started = self.start_copy(gpu, buffer);
        match self.readback {
            Readback::Immediate => {
                self.finish(gpu, &started, canvas)?;
                Ok(true)
            }
            Readback::Pipelined => match self.pending.replace(started) {
                Some(previous) => {
                    self.finish(gpu, &previous, canvas)?;
                    Ok(true)
                }
                None => Ok(false),
            },
        }
    }

    /// Copies the texture into buffer `index` and starts mapping it.
    fn start_copy(&mut self, gpu: &Gpu, index: usize) -> Pending {
        let bytes_per_row = padded_bytes_per_row(self.width);
        let read = self.buffers[index].get_or_insert_with(|| ReadBuffer {
            buffer: gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("vnce_gpu readback"),
                size: u64::from(bytes_per_row) * u64::from(self.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        });
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vnce_gpu readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &read.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        let submission = gpu.queue().submit([encoder.finish()]);
        let (sender, mapped) = mpsc::channel();
        read.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                // The receiver is gone only when the target was dropped.
                let _ = sender.send(result);
            });
        Pending {
            buffer: index,
            submission,
            mapped,
            width: self.width,
            height: self.height,
        }
    }

    /// Waits for a copy and moves its pixels into `canvas`.
    fn finish(&self, gpu: &Gpu, pending: &Pending, canvas: &mut Canvas) -> Result<(), GpuError> {
        let error = |message: String| GpuError::Readback(message);
        gpu.device()
            .poll(wgpu::PollType::Wait {
                submission_index: Some(pending.submission.clone()),
                timeout: None,
            })
            .map_err(|e| error(e.to_string()))?;
        let mapped = match pending.mapped.try_recv() {
            Ok(result) => result,
            // The callback may run on a later poll on some backends.
            Err(mpsc::TryRecvError::Empty) => {
                gpu.device()
                    .poll(wgpu::PollType::wait_indefinitely())
                    .map_err(|e| error(e.to_string()))?;
                pending
                    .mapped
                    .recv()
                    .map_err(|_| error("the mapping was cancelled".to_owned()))?
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(error("the mapping was cancelled".to_owned()));
            }
        };
        mapped.map_err(|e| error(e.to_string()))?;
        let Some(read) = &self.buffers[pending.buffer] else {
            return Err(error("the readback buffer is gone".to_owned()));
        };
        {
            let data = read
                .buffer
                .get_mapped_range(..)
                .map_err(|e| error(e.to_string()))?;
            unpad_into(&data, pending.width, pending.height, canvas);
        }
        read.buffer.unmap();
        Ok(())
    }

    /// Forgets a frame being read in the background.
    fn drop_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            if let Some(read) = &self.buffers[pending.buffer] {
                // Unmapping cancels a mapping that is still in progress.
                read.buffer.unmap();
            }
        }
        self.next = 0;
    }
}

fn clamp_size(gpu: &Gpu, width: u32, height: u32) -> (u32, u32) {
    let max = gpu.device().limits().max_texture_dimension_2d.max(1);
    (width.clamp(1, max), height.clamp(1, max))
}

fn create_texture(gpu: &Gpu, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("vnce_gpu render target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: RenderTarget::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Bytes per row in a readback buffer: 4 bytes per pixel, rounded up to the 256-byte
/// alignment required for texture copies.
#[must_use]
pub(crate) fn padded_bytes_per_row(width: u32) -> u32 {
    (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
}

/// Copies padded RGBA rows into `canvas`, resizing it when needed.
fn unpad_into(data: &[u8], width: u32, height: u32, canvas: &mut Canvas) {
    if (canvas.width(), canvas.height()) != (width, height) {
        canvas.resize(width, height);
    }
    let stride = padded_bytes_per_row(width) as usize;
    let row_bytes = width as usize * 4;
    let pixels = canvas.pixels_mut();
    for (row, out) in data
        .chunks(stride)
        .zip(pixels.chunks_mut(width as usize))
        .take(height as usize)
    {
        for (pixel, bytes) in out.iter_mut().zip(row[..row_bytes].chunks_exact(4)) {
            *pixel = Rgba::new(bytes[0], bytes[1], bytes[2], bytes[3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_padded_to_256_bytes() {
        assert_eq!(padded_bytes_per_row(1), 256);
        assert_eq!(padded_bytes_per_row(64), 256);
        assert_eq!(padded_bytes_per_row(65), 512);
        assert_eq!(padded_bytes_per_row(1920), 7680);
    }

    #[test]
    fn unpadding_skips_the_padding_and_resizes() {
        // 2×2 image: rows of 8 bytes padded to 256.
        let mut data = vec![0xEE; 512];
        data[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        data[256..264].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);
        let mut canvas = Canvas::new(1, 1);
        unpad_into(&data, 2, 2, &mut canvas);
        assert_eq!((canvas.width(), canvas.height()), (2, 2));
        assert_eq!(
            canvas.pixels(),
            [
                Rgba::new(1, 2, 3, 4),
                Rgba::new(5, 6, 7, 8),
                Rgba::new(9, 10, 11, 12),
                Rgba::new(13, 14, 15, 16),
            ]
        );
    }
}
