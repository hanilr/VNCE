//! Shared helpers for the GPU tests.

use vnce_gpu::{Gpu, GpuOptions};
use vnce_raster::Rgba;

/// A GPU for tests, or `None` (the test is skipped) on machines without any adapter.
pub fn gpu() -> Option<Gpu> {
    match Gpu::new(GpuOptions::default()) {
        Ok(gpu) => {
            eprintln!("using {}", gpu.info());
            Some(gpu)
        }
        Err(error) => {
            eprintln!("skipped: {error}");
            None
        }
    }
}

/// Asserts that two pixels differ by at most one step per channel (sRGB rounding).
#[allow(dead_code, reason = "not every test file uses every helper")]
pub fn assert_close(actual: Rgba, expected: Rgba) {
    let close = |a: u8, b: u8| a.abs_diff(b) <= 1;
    assert!(
        close(actual.r, expected.r)
            && close(actual.g, expected.g)
            && close(actual.b, expected.b)
            && close(actual.a, expected.a),
        "{actual:?} != {expected:?}"
    );
}
