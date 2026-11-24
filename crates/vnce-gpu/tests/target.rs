//! Render target tests on a real (or software) GPU. Skipped when no adapter exists.

mod common;

use vnce_gpu::{Readback, RenderTarget};
use vnce_raster::{Canvas, Rgba};

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
