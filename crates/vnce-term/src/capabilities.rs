//! Terminal capabilities.

use vnce_core::ColorDepth;

/// Features supported by the terminal, used to adapt the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Capabilities {
    /// Number of colors the terminal can display. Colors are downsampled to this depth.
    pub color_depth: ColorDepth,
    /// Whether synchronized output (`CSI ? 2026 h/l`) is used to avoid tearing.
    pub synchronized_output: bool,
}

impl Default for Capabilities {
    /// Truecolor with synchronized output.
    fn default() -> Self {
        Self {
            color_depth: ColorDepth::TrueColor,
            synchronized_output: true,
        }
    }
}
