//! Color conversions between canvas pixels (sRGB) and shader colors (linear).

use vnce_raster::Rgba;

/// Converts an sRGB channel (`0..=255`) to linear light (`0.0..=1.0`).
#[must_use]
pub fn srgb_to_linear(value: u8) -> f32 {
    let c = f32::from(value) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A canvas pixel as linear RGBA, as shaders writing to a [`RenderTarget`] expect it.
/// Alpha is not converted.
///
/// [`RenderTarget`]: crate::RenderTarget
#[must_use]
pub fn linear_rgba(color: Rgba) -> [f32; 4] {
    [
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
        f32::from(color.a) / 255.0,
    ]
}

/// A canvas pixel as a `wgpu` clear color (linear).
#[must_use]
pub fn clear_color(color: Rgba) -> wgpu::Color {
    let [r, g, b, a] = linear_rgba(color);
    wgpu::Color {
        r: f64::from(r),
        g: f64::from(g),
        b: f64::from(b),
        a: f64::from(a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_to_linear_endpoints_and_midpoint() {
        assert!(srgb_to_linear(0).abs() < f32::EPSILON);
        assert!((srgb_to_linear(255) - 1.0).abs() < 1e-6);
        // sRGB 188 is about 50% linear light.
        assert!((srgb_to_linear(188) - 0.5).abs() < 0.01);
        // The linear segment near black.
        assert!((srgb_to_linear(10) - 10.0 / 255.0 / 12.92).abs() < 1e-6);
    }

    #[test]
    fn alpha_is_not_converted() {
        let [_, _, _, a] = linear_rgba(Rgba::new(0, 0, 0, 128));
        assert!((a - 128.0 / 255.0).abs() < 1e-6);
        let clear = clear_color(Rgba::WHITE);
        assert!((clear.r - 1.0).abs() < 1e-6 && (clear.a - 1.0).abs() < 1e-6);
    }
}
