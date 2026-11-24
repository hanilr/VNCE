//! The error type of this crate.

use std::fmt;

/// Errors from GPU setup, shaders and readback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GpuError {
    /// No usable GPU adapter was found (no GPU, no driver, or only a software adapter
    /// while software adapters are not allowed). Fall back to CPU rendering.
    NoAdapter,
    /// The device could not be created.
    Device(String),
    /// A shader did not compile; the message points at the error in the source.
    Shader(String),
    /// Reading a render target back to the CPU failed (for example when the device was
    /// lost).
    Readback(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no usable GPU adapter found"),
            Self::Device(message) => write!(f, "cannot create the GPU device: {message}"),
            Self::Shader(message) => write!(f, "shader error: {message}"),
            Self::Readback(message) => write!(f, "cannot read the GPU image back: {message}"),
        }
    }
}

impl std::error::Error for GpuError {}
