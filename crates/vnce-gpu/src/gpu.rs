//! Connecting to a GPU: [`Gpu`], [`GpuOptions`] and [`GpuInfo`].

use std::fmt;

use crate::GpuError;

/// Which GPU to prefer on machines with more than one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Power {
    /// The fastest GPU (usually a discrete one).
    #[default]
    High,
    /// The most power-efficient GPU (usually an integrated one).
    Low,
}

/// Options for [`Gpu::new`].
///
/// Environment variables override them (see [`GpuOptions::apply_env`]), so users can
/// choose another GPU or graphics API without changing the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuOptions {
    /// Which GPU to prefer.
    pub power: Power,
    /// Allow a software adapter (WARP on Windows, llvmpipe on Linux) when no hardware GPU
    /// is available. Slow, but it works on machines without a GPU.
    pub allow_software: bool,
    /// The graphics APIs that may be used.
    pub backends: wgpu::Backends,
}

impl Default for GpuOptions {
    fn default() -> Self {
        Self {
            power: Power::High,
            allow_software: true,
            backends: native_backends(),
        }
    }
}

/// Every native graphics API (no browser APIs: this is a terminal).
fn native_backends() -> wgpu::Backends {
    wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12 | wgpu::Backends::GL
}

impl GpuOptions {
    /// Applies the environment variables of the process (see [`GpuOptions::apply_env`]).
    #[must_use]
    pub fn with_env(self) -> Self {
        self.apply_env(|name| std::env::var(name).ok())
    }

    /// Applies environment variables read with `env`:
    ///
    /// - `VNCE_GPU_BACKEND`: comma-separated list of `vulkan`, `dx12`, `metal` and `gl`.
    /// - `VNCE_GPU_POWER`: `high` or `low`.
    ///
    /// Unknown values are ignored.
    #[must_use]
    pub fn apply_env(mut self, env: impl Fn(&str) -> Option<String>) -> Self {
        if let Some(value) = env("VNCE_GPU_BACKEND") {
            let mut backends = wgpu::Backends::empty();
            for name in value.split(',') {
                backends |= match name.trim().to_ascii_lowercase().as_str() {
                    "vulkan" | "vk" => wgpu::Backends::VULKAN,
                    "dx12" | "d3d12" => wgpu::Backends::DX12,
                    "metal" | "mtl" => wgpu::Backends::METAL,
                    "gl" | "gles" | "opengl" => wgpu::Backends::GL,
                    _ => wgpu::Backends::empty(),
                };
            }
            if !backends.is_empty() {
                self.backends = backends;
            }
        }
        if let Some(value) = env("VNCE_GPU_POWER") {
            match value.trim().to_ascii_lowercase().as_str() {
                "high" => self.power = Power::High,
                "low" => self.power = Power::Low,
                _ => {}
            }
        }
        self
    }
}

/// What kind of GPU is used, for status lines and logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    /// The adapter name, for example `NVIDIA GeForce RTX 5070 Laptop GPU`.
    pub name: String,
    /// The graphics API.
    pub backend: wgpu::Backend,
    /// The kind of device.
    pub device_type: wgpu::DeviceType,
    /// The driver name and version, when known.
    pub driver: String,
}

impl GpuInfo {
    /// Returns `true` for software adapters (rendering on the CPU).
    #[must_use]
    pub fn is_software(&self) -> bool {
        self.device_type == wgpu::DeviceType::Cpu
    }
}

impl fmt::Display for GpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}", self.name, self.backend)?;
        if self.is_software() {
            f.write_str(", software")?;
        }
        f.write_str(")")
    }
}

/// A connection to a GPU: the device and its command queue.
///
/// Create one at start-up and keep it; creating a device is slow.
///
/// ```no_run
/// use vnce_gpu::{Gpu, GpuOptions};
///
/// match Gpu::new(GpuOptions::default()) {
///     Ok(gpu) => println!("rendering on {}", gpu.info()),
///     Err(error) => println!("no GPU, using the CPU renderer: {error}"),
/// }
/// ```
#[derive(Debug)]
pub struct Gpu {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: GpuInfo,
}

impl Gpu {
    /// Connects to a GPU.
    ///
    /// A hardware adapter is preferred; without one, a software adapter is used when
    /// [`GpuOptions::allow_software`] is set. The environment overrides `options` (see
    /// [`GpuOptions::apply_env`]).
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::NoAdapter`] when no usable adapter exists (the program can then
    /// fall back to CPU rendering) and [`GpuError::Device`] when the device cannot be
    /// created.
    pub fn new(options: GpuOptions) -> Result<Self, GpuError> {
        let options = options.with_env();
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = options.backends;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(find_adapter(&instance, options))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("vnce_gpu device"),
            required_features: wgpu::Features::empty(),
            // Everything the adapter offers, so large render targets work where possible.
            required_limits: adapter.limits(),
            ..wgpu::DeviceDescriptor::default()
        }))
        .map_err(|error| GpuError::Device(error.to_string()))?;
        let raw = adapter.get_info();
        let driver = [raw.driver.as_str(), raw.driver_info.as_str()]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        let info = GpuInfo {
            name: raw.name,
            backend: raw.backend,
            device_type: raw.device_type,
            driver,
        };
        Ok(Self {
            adapter,
            device,
            queue,
            info,
        })
    }

    /// What kind of GPU is used.
    #[must_use]
    pub fn info(&self) -> &GpuInfo {
        &self.info
    }

    /// The `wgpu` adapter, for advanced use (features, limits, formats).
    #[must_use]
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// The `wgpu` device: creates buffers, textures, shaders and pipelines.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The `wgpu` queue: submits command buffers and writes buffers and textures.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The largest supported multisample count up to `wanted` (1, 2, 4, 8 or 16) for
    /// `format`, for anti-aliasing that works on every adapter.
    #[must_use]
    pub fn sample_count(&self, format: wgpu::TextureFormat, wanted: u32) -> u32 {
        let features = self.adapter.get_texture_format_features(format);
        [16, 8, 4, 2]
            .into_iter()
            .filter(|&count| count <= wanted)
            .find(|&count| features.flags.sample_count_supported(count))
            .unwrap_or(1)
    }
}

/// A hardware adapter, else (when allowed) a software adapter.
async fn find_adapter(
    instance: &wgpu::Instance,
    options: GpuOptions,
) -> Result<wgpu::Adapter, GpuError> {
    let power_preference = match options.power {
        Power::High => wgpu::PowerPreference::HighPerformance,
        Power::Low => wgpu::PowerPreference::LowPower,
    };
    let request = |force_fallback_adapter| wgpu::RequestAdapterOptions {
        power_preference,
        force_fallback_adapter,
        compatible_surface: None,
        ..wgpu::RequestAdapterOptions::default()
    };
    if let Ok(adapter) = instance.request_adapter(&request(false)).await {
        let software = adapter.get_info().device_type == wgpu::DeviceType::Cpu;
        if !software || options.allow_software {
            return Ok(adapter);
        }
    }
    if options.allow_software {
        if let Ok(adapter) = instance.request_adapter(&request(true)).await {
            return Ok(adapter);
        }
    }
    Err(GpuError::NoAdapter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn defaults_use_every_native_backend() {
        let options = GpuOptions::default();
        assert_eq!(options.power, Power::High);
        assert!(options.allow_software);
        assert!(!options.backends.contains(wgpu::Backends::BROWSER_WEBGPU));
        assert!(
            options
                .backends
                .contains(wgpu::Backends::VULKAN | wgpu::Backends::GL)
        );
    }

    #[test]
    fn environment_overrides_options() {
        let options = GpuOptions::default().apply_env(env(&[
            ("VNCE_GPU_BACKEND", "Vulkan, gl"),
            ("VNCE_GPU_POWER", "LOW"),
        ]));
        assert_eq!(
            options.backends,
            wgpu::Backends::VULKAN | wgpu::Backends::GL
        );
        assert_eq!(options.power, Power::Low);
    }

    #[test]
    fn unknown_environment_values_are_ignored() {
        let defaults = GpuOptions::default();
        let options = defaults.apply_env(env(&[
            ("VNCE_GPU_BACKEND", "glide"),
            ("VNCE_GPU_POWER", "turbo"),
        ]));
        assert_eq!(options, defaults);
        assert_eq!(defaults.apply_env(env(&[])), defaults);
    }

    #[test]
    fn info_display() {
        let mut info = GpuInfo {
            name: "Test GPU".to_owned(),
            backend: wgpu::Backend::Vulkan,
            device_type: wgpu::DeviceType::DiscreteGpu,
            driver: String::new(),
        };
        assert_eq!(info.to_string(), "Test GPU (vulkan)");
        info.device_type = wgpu::DeviceType::Cpu;
        assert!(info.is_software());
        assert_eq!(info.to_string(), "Test GPU (vulkan, software)");
    }
}
