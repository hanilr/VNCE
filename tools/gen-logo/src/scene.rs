//! A scene: what is drawn (layers in a design space of some width and height), what the
//! clipped layers are clipped to, and where the pixel bands end.

use crate::geometry::{Path, Point, SIZE};
use crate::horse::Layer;

/// How much the horse is scaled from the banner's coordinates into the logo.
pub const LOGO_HORSE_SCALE: f32 = 0.82;
/// Where the banner's origin lands in the logo: the horse centered in the badge.
pub const LOGO_HORSE_AT: Point = (68.0, -6.0);

/// Everything needed to render one picture.
#[derive(Clone, Debug)]
pub struct Scene {
    /// The design space's width.
    pub width: f32,
    /// The design space's height.
    pub height: f32,
    /// The layers, back to front.
    pub layers: Vec<Layer>,
    /// The shape that layers marked `in_badge` are clipped to (`None`: not clipped).
    pub clip: Option<Path>,
    /// Where each pixel band ends (before jitter), in design units.
    pub edges: [f32; 4],
}

impl Scene {
    /// The logo: the farm on its badge, the horse in front, 512 x 512.
    pub fn logo() -> Self {
        // The farm behind, the horse in front.
        let mut layers = crate::farm::layers();
        layers.extend(crate::horse::layers(LOGO_HORSE_SCALE, LOGO_HORSE_AT));
        Self {
            width: SIZE,
            height: SIZE,
            layers,
            clip: Some(crate::farm::badge()),
            edges: crate::pixelate::EDGES,
        }
    }

    /// The size in pixels of a render `width` pixels wide (same aspect ratio).
    pub fn pixel_size(&self, width: usize) -> (usize, usize) {
        let height = (width as f32 * self.height / self.width).round() as usize;
        (width, height)
    }
}
