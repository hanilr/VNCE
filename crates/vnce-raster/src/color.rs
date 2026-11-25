//! Pixel colors and blending.

use vnce_core::Color;

/// An 8-bit RGBA pixel with straight (not premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha: 0 is fully transparent, 255 fully opaque.
    pub a: u8,
}

impl Rgba {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    /// Creates a color from all four channels.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque color.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    /// The same color with a different alpha.
    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self::new(self.r, self.g, self.b, a)
    }

    /// Returns `true` if the pixel is fully transparent.
    #[must_use]
    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// The terminal color of this pixel (alpha is ignored).
    #[must_use]
    pub const fn to_color(self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }

    /// Relative luminance in `0.0..=1.0` (Rec. 709 weights on the sRGB values).
    #[must_use]
    pub fn luminance(self) -> f32 {
        (0.2126 * f32::from(self.r) + 0.7152 * f32::from(self.g) + 0.0722 * f32::from(self.b))
            / 255.0
    }

    /// Linear interpolation between `self` (`t = 0`) and `other` (`t = 1`), all channels.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| to_u8(f32::from(a) + (f32::from(b) - f32::from(a)) * t);
        Self::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
            mix(self.a, other.a),
        )
    }

    /// Squared distance between two colors (RGB only).
    #[must_use]
    pub fn distance_squared(self, other: Self) -> u32 {
        let d = |a: u8, b: u8| u32::from(a.abs_diff(b)).pow(2);
        d(self.r, other.r) + d(self.g, other.g) + d(self.b, other.b)
    }

    /// Draws `src` over `self` with the given blend mode and returns the result.
    #[must_use]
    pub fn blend(self, src: Self, mode: BlendMode) -> Self {
        match mode {
            BlendMode::Replace => src,
            BlendMode::Alpha => self.composite(src, |_, s| s),
            BlendMode::Add => self.composite(src, u8::saturating_add),
            BlendMode::Multiply => self.composite(src, |d, s| div255(u32::from(d) * u32::from(s))),
        }
    }

    /// Source-over compositing where the source color is first combined with the
    /// destination color by `mix`.
    fn composite(self, src: Self, mix: impl Fn(u8, u8) -> u8) -> Self {
        match src.a {
            0 => return self,
            255 if self.a == 255 || self.a == 0 => {
                // Opaque source: the mixed color replaces the destination. Over a transparent
                // destination the source itself is used.
                if self.a == 0 {
                    return src;
                }
                return Self::rgb(mix(self.r, src.r), mix(self.g, src.g), mix(self.b, src.b));
            }
            _ => {}
        }
        let sa = u32::from(src.a);
        if self.a == 255 {
            // Opaque destination (the common case): the formula below with da = 255, so
            // out_a is the constant 255 * 255 and the division needs no divide instruction.
            let channel = |d: u8, s: u8| {
                let s = mix(d, s);
                let value =
                    (u32::from(s) * sa * 255 + u32::from(d) * 255 * (255 - sa) + 65_025 / 2)
                        / 65_025;
                u8::try_from(value).unwrap_or(u8::MAX)
            };
            return Self::rgb(
                channel(self.r, src.r),
                channel(self.g, src.g),
                channel(self.b, src.b),
            );
        }
        let da = u32::from(self.a);
        // out_a = sa + da * (1 - sa), in 0..=255 * 255.
        let out_a = sa * 255 + da * (255 - sa);
        if out_a == 0 {
            return Self::TRANSPARENT;
        }
        let channel = |d: u8, s: u8| {
            // Over a transparent destination the source color is used unmixed.
            let s = if self.a == 0 { s } else { mix(d, s) };
            let value =
                (u32::from(s) * sa * 255 + u32::from(d) * da * (255 - sa) + out_a / 2) / out_a;
            u8::try_from(value).unwrap_or(u8::MAX)
        };
        Self::new(
            channel(self.r, src.r),
            channel(self.g, src.g),
            channel(self.b, src.b),
            div255(out_a),
        )
    }
}

impl From<(u8, u8, u8)> for Rgba {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        Self::rgb(r, g, b)
    }
}

impl From<(u8, u8, u8, u8)> for Rgba {
    fn from((r, g, b, a): (u8, u8, u8, u8)) -> Self {
        Self::new(r, g, b, a)
    }
}

/// Blends `color` into every pixel of `pixels` (one row of a shape), with the same result
/// as [`Rgba::blend`] on each pixel. Common cases skip the per-pixel work.
pub(crate) fn blend_span(pixels: &mut [Rgba], color: Rgba, mode: BlendMode) {
    match mode {
        BlendMode::Replace => pixels.fill(color),
        // An opaque source replaces any destination.
        BlendMode::Alpha if color.a == 255 => pixels.fill(color),
        // A transparent source leaves every destination unchanged.
        _ if color.a == 0 => {}
        // One arm per mode, so the mode is not matched again for every pixel.
        BlendMode::Alpha => {
            for pixel in pixels {
                *pixel = pixel.blend(color, BlendMode::Alpha);
            }
        }
        BlendMode::Add => {
            for pixel in pixels {
                *pixel = pixel.blend(color, BlendMode::Add);
            }
        }
        BlendMode::Multiply => {
            for pixel in pixels {
                *pixel = pixel.blend(color, BlendMode::Multiply);
            }
        }
    }
}

/// How a drawn pixel is combined with the pixel already on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// The drawn pixel replaces the canvas pixel, including its alpha.
    Replace,
    /// Normal alpha compositing ("source over").
    #[default]
    Alpha,
    /// Colors are added (light, glow, fire). Alpha controls the strength.
    Add,
    /// Colors are multiplied (shadows, tinting). Alpha controls the strength.
    Multiply,
}

/// `value / 255`, rounded.
fn div255(value: u32) -> u8 {
    u8::try_from((value + 127) / 255).unwrap_or(u8::MAX)
}

/// Rounds and clamps a channel value to `0..=255`.
pub(crate) fn to_u8(value: f32) -> u8 {
    // The value is clamped to the u8 range first, so the conversion cannot truncate.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped to the u8 range"
    )]
    let byte = value.round().clamp(0.0, 255.0) as u8;
    byte
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The general compositing formula, without the opaque-destination shortcut.
    fn composite_reference(dst: Rgba, src: Rgba, mix: impl Fn(u8, u8) -> u8) -> Rgba {
        let (sa, da) = (u32::from(src.a), u32::from(dst.a));
        let out_a = sa * 255 + da * (255 - sa);
        let channel = |d: u8, s: u8| {
            let s = if dst.a == 0 { s } else { mix(d, s) };
            let value =
                (u32::from(s) * sa * 255 + u32::from(d) * da * (255 - sa) + out_a / 2) / out_a;
            u8::try_from(value).unwrap_or(u8::MAX)
        };
        Rgba::new(
            channel(dst.r, src.r),
            channel(dst.g, src.g),
            channel(dst.b, src.b),
            div255(out_a),
        )
    }

    #[test]
    fn opaque_destination_shortcut_matches_the_formula() {
        type Mix = fn(u8, u8) -> u8;
        let mixes: [(BlendMode, Mix); 3] = [
            (BlendMode::Alpha, |_, s| s),
            (BlendMode::Add, u8::saturating_add),
            (BlendMode::Multiply, |d, s| {
                div255(u32::from(d) * u32::from(s))
            }),
        ];
        for (mode, mix) in mixes {
            for sa in 1..=254u8 {
                for d in (0..=255u8).step_by(3) {
                    for s in (0..=255u8).step_by(5) {
                        let dst = Rgba::rgb(d, 255 - d, d / 2);
                        let src = Rgba::new(s, d, 255 - s, sa);
                        assert_eq!(
                            dst.blend(src, mode),
                            composite_reference(dst, src, mix),
                            "{mode:?} {dst:?} {src:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn opaque_alpha_blend_replaces() {
        let dst = Rgba::rgb(10, 20, 30);
        let src = Rgba::rgb(200, 100, 50);
        assert_eq!(dst.blend(src, BlendMode::Alpha), src);
        assert_eq!(dst.blend(Rgba::TRANSPARENT, BlendMode::Alpha), dst);
    }

    #[test]
    fn half_alpha_blend() {
        let dst = Rgba::rgb(0, 0, 0);
        let src = Rgba::new(255, 255, 255, 128);
        let out = dst.blend(src, BlendMode::Alpha);
        assert_eq!(out.a, 255);
        assert!(out.r.abs_diff(128) <= 1, "{out:?}");
    }

    #[test]
    fn blending_over_transparent_keeps_the_source_color() {
        let src = Rgba::new(200, 100, 50, 100);
        let out = Rgba::TRANSPARENT.blend(src, BlendMode::Alpha);
        assert_eq!(out, src);
        let out = Rgba::TRANSPARENT.blend(src, BlendMode::Add);
        assert_eq!(out, src);
    }

    #[test]
    fn semi_transparent_over_semi_transparent() {
        let dst = Rgba::new(255, 0, 0, 128);
        let src = Rgba::new(0, 0, 255, 128);
        let out = dst.blend(src, BlendMode::Alpha);
        // out_a = 0.5 + 0.5 * 0.5 = 0.75
        assert!(out.a.abs_diff(191) <= 1, "{out:?}");
        // Blue weighs 0.5 / 0.75, red 0.25 / 0.75.
        assert!(
            out.b.abs_diff(170) <= 1 && out.r.abs_diff(85) <= 1,
            "{out:?}"
        );
    }

    #[test]
    fn add_and_multiply() {
        let dst = Rgba::rgb(200, 100, 0);
        assert_eq!(
            dst.blend(Rgba::rgb(100, 100, 100), BlendMode::Add),
            Rgba::rgb(255, 200, 100)
        );
        assert_eq!(
            dst.blend(Rgba::rgb(128, 255, 0), BlendMode::Multiply),
            Rgba::rgb(100, 100, 0)
        );
        // Alpha controls the strength: halfway between the original and the full effect.
        let half = dst.blend(Rgba::new(100, 100, 100, 128), BlendMode::Add);
        assert!(
            half.r.abs_diff(228) <= 1 && half.g.abs_diff(150) <= 1,
            "{half:?}"
        );
    }

    #[test]
    fn replace_copies_alpha() {
        let src = Rgba::new(1, 2, 3, 4);
        assert_eq!(Rgba::WHITE.blend(src, BlendMode::Replace), src);
    }

    #[test]
    fn lerp_and_luminance() {
        let mid = Rgba::BLACK.lerp(Rgba::WHITE, 0.5);
        assert_eq!(mid, Rgba::rgb(128, 128, 128));
        assert!((Rgba::WHITE.luminance() - 1.0).abs() < 1e-6);
        assert!(Rgba::BLACK.luminance().abs() < 1e-6);
        assert!(Rgba::rgb(0, 255, 0).luminance() > Rgba::rgb(255, 0, 0).luminance());
        assert_eq!(Rgba::BLACK.distance_squared(Rgba::rgb(3, 4, 0)), 25);
    }
}
