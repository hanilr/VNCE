//! Render target tests on a real (or software) GPU. Skipped when no adapter exists.

mod common;

use vnce_gpu::{Readback, RenderTarget};
use vnce_raster::{Canvas, Rgba};

use std::io;
use std::sync::Arc;

use vnce_core::Size;
use vnce_term::{Backend, FrameView, ImageView, TerminalInfo, TerminalOptions};

use common::{assert_close, gpu};

#[test]
fn clear_and_read_back() {
    let Some(gpu) = gpu() else { return };
    // 70 pixels wide: rows need padding.
    let mut target = RenderTarget::new(&gpu, 70, 3);
    let color = Rgba::rgb(200, 100, 50);
    target.clear(&gpu, color);
    let mut canvas = Canvas::new(0, 0);
    assert!(target.read_into(&gpu, &mut canvas).unwrap());
    assert_eq!((canvas.width(), canvas.height()), (70, 3));
    for &pixel in canvas.pixels() {
        assert_close(pixel, color);
    }
}

#[test]
fn pipelined_readback_returns_the_previous_frame() {
    let Some(gpu) = gpu() else { return };
    let mut target = RenderTarget::new(&gpu, 8, 8).with_readback(Readback::Pipelined);
    let mut canvas = Canvas::new(0, 0);
    let colors = [
        Rgba::rgb(255, 0, 0),
        Rgba::rgb(0, 255, 0),
        Rgba::rgb(0, 0, 255),
    ];
    target.clear(&gpu, colors[0]);
    assert!(!target.read_into(&gpu, &mut canvas).unwrap());
    assert_eq!(canvas.width(), 0);
    for pair in colors.windows(2) {
        target.clear(&gpu, pair[1]);
        assert!(target.read_into(&gpu, &mut canvas).unwrap());
        assert_close(canvas.pixels()[0], pair[0]);
    }

    // Resizing drops the frame in flight.
    target.resize(&gpu, 4, 2);
    assert!(!target.read_into(&gpu, &mut canvas).unwrap());
    target.clear(&gpu, Rgba::WHITE);
    assert!(target.read_into(&gpu, &mut canvas).unwrap());
    assert_eq!((canvas.width(), canvas.height()), (4, 2));
}

#[test]
fn sizes_are_clamped() {
    let Some(gpu) = gpu() else { return };
    let mut target = RenderTarget::new(&gpu, 0, 0);
    assert_eq!((target.width(), target.height()), (1, 1));
    target.resize(&gpu, 5, u32::MAX);
    assert_eq!(target.width(), 5);
    assert_eq!(
        target.height(),
        gpu.device().limits().max_texture_dimension_2d
    );
    target.resize(&gpu, 2, 2);
    assert!((target.aspect() - 1.0).abs() < f32::EPSILON);
}

/// A window that counts the images it receives, by kind.
struct Window(Arc<std::sync::Mutex<(usize, usize)>>);
impl Backend for Window {
    fn size(&self) -> io::Result<Size> {
        Ok(Size::new(8, 4))
    }
    fn write_all(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn enter(&mut self, _: &TerminalOptions) -> io::Result<()> {
        Ok(())
    }
    fn leave(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn terminal_info(&self) -> TerminalInfo {
        TerminalInfo {
            native_window: true,
            cell_pixels: Some((4, 8)),
            ..TerminalInfo::default()
        }
    }
    fn present(&mut self, frame: &FrameView<'_>) -> Option<io::Result<()>> {
        let mut counts = self.0.lock().unwrap();
        for image in &frame.images {
            match image {
                ImageView::Pixels { .. } => counts.0 += 1,
                ImageView::External { image, .. } => {
                    assert!(
                        image
                            .handle()
                            .downcast_ref::<vnce_gpu::wgpu::TextureView>()
                            .is_some()
                    );
                    counts.1 += 1;
                }
            }
        }
        Some(Ok(()))
    }
}

#[test]
fn one_device_per_process_and_textures_shown_directly_in_a_window() {
    use vnce_core::Rect;
    use vnce_term::{Capabilities, Terminal};

    let Some(gpu) = gpu() else { return };
    let again = vnce_gpu::Gpu::new(vnce_gpu::GpuOptions::default()).unwrap();
    assert_eq!(
        gpu.device(),
        again.device(),
        "the process shares one device"
    );

    let counts = Arc::new(std::sync::Mutex::new((0, 0)));
    let mut terminal = Terminal::new(Window(Arc::clone(&counts)), Capabilities::default()).unwrap();
    terminal.enter_mode(TerminalOptions::default()).unwrap();
    let mut target = RenderTarget::new(&gpu, 32, 32);
    target.clear(&gpu, Rgba::rgb(0, 128, 255));
    let mut canvas = Canvas::new(0, 0);
    let area = Rect::new(0, 0, 8, 4);
    // Before a window draws with this device: read back and drawn as pixels.
    terminal
        .draw(|frame| assert!(target.show(&gpu, frame, area, &mut canvas).unwrap()))
        .unwrap();
    assert_eq!(*counts.lock().unwrap(), (1, 0));
    assert_eq!(canvas.get(0, 0), Some(Rgba::rgb(0, 128, 255)));
    // Once the window draws with it: the texture itself.
    gpu.set_presenting();
    terminal
        .draw(|frame| assert!(target.show(&gpu, frame, area, &mut canvas).unwrap()))
        .unwrap();
    assert_eq!(*counts.lock().unwrap(), (1, 1));
}
