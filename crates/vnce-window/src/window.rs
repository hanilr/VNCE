//! The window: `winit` for the window and its events, a `wgpu` surface to draw on, and
//! the engine's [`Backend`] and [`Driver`] on top of them.
//!
//! The driver pumps `winit`'s event loop from the engine's own game loop
//! (`pump_app_events`), so the runtime's loop, timing and fixed updates stay as they are.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vnce_core::Size;
use vnce_gpu::wgpu;
use vnce_raster::{Canvas, Rgba};
use vnce_runtime::Driver;
use vnce_term::input::{MouseButton, MouseEvent, MouseEventKind};
use vnce_term::{
    Backend, Event, FrameView, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, TerminalInfo,
    TerminalOptions, WindowRequest, WindowState,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{Fullscreen, Icon, Window, WindowId};

use crate::gamepad::Gamepads;
use crate::keys;
use crate::renderer::{Layout, Renderer};

/// The size a window opens with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSize {
    /// Columns and rows of cells.
    Cells(u16, u16),
    /// Pixels (logical: they are multiplied by the screen's scale factor).
    Pixels(u32, u32),
}

/// How the window looks and opens.
#[derive(Debug, Clone)]
pub struct WindowOptions {
    /// The title bar.
    pub title: String,
    /// The size it opens with.
    pub size: WindowSize,
    /// Font size in logical pixels (multiplied by the screen's scale factor).
    pub font_size: f32,
    /// Start in (borderless) fullscreen. F11 and Alt+Enter switch at any time.
    pub fullscreen: bool,
    /// The window and taskbar icon (RGBA).
    pub icon: Option<Canvas>,
    /// Wait for the screen's refresh before showing a frame (no tearing). Without it
    /// frames are shown at once, as fast as the game's frame limit allows.
    pub vsync: bool,
    /// Save a picture of the window's contents (PNG) at this frame (counted from 1).
    /// `VNCE_WINDOW_SCREENSHOT=file.png` sets it from outside (with
    /// `VNCE_WINDOW_SCREENSHOT_FRAME`, default 30).
    pub screenshot: Option<(PathBuf, u64)>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "vnce".to_owned(),
            size: WindowSize::Cells(120, 40),
            font_size: 16.0,
            fullscreen: false,
            icon: None,
            vsync: true,
            screenshot: std::env::var_os("VNCE_WINDOW_SCREENSHOT").map(|path| {
                let frame = std::env::var("VNCE_WINDOW_SCREENSHOT_FRAME")
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(30);
                (PathBuf::from(path), frame)
            }),
        }
    }
}

impl WindowOptions {
    /// Options with this title.
    #[must_use]
    pub fn titled(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }
}

/// What the backend and the driver share: the window, its surface and renderer, and the
/// events waiting for the game.
struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    view_format: wgpu::TextureFormat,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    font_size: f32,
    /// The font size in physical pixels now.
    font_px: f32,
    /// The grid of cells and where it lies in the window.
    grid: (u16, u16),
    layout: Layout,
    events: VecDeque<Event>,
    modifiers: ModifiersState,
    /// The mouse in cells, and the buttons held.
    mouse: Option<(u16, u16)>,
    buttons: [bool; 3],
    /// Frames shown so far, and a picture to save of one of them (see
    /// [`WindowOptions::screenshot`]).
    presented: u64,
    screenshot: Option<(PathBuf, u64)>,
    /// The gamepads (none without the `gamepad` feature).
    pads: Gamepads,
    /// What the game asked for (see [`WindowRequest`]).
    cursor_visible: bool,
    vsync: bool,
    smooth_scaling: bool,
    /// The present modes the surface supports, to switch vsync.
    present_modes: Vec<wgpu::PresentMode>,
}

impl State {
    /// Fits the surface and the grid to the window's size and scale; returns whether the
    /// grid changed.
    fn resize(&mut self) -> bool {
        let size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        let px = (self.font_size * scale).round();
        if (px - self.font_px).abs() > 0.5 {
            self.font_px = px;
            self.renderer.set_font_px(px);
        }
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.config);
        let (grid, layout) = self
            .renderer
            .layout((self.config.width, self.config.height));
        self.layout = layout;
        let changed = grid != self.grid;
        self.grid = grid;
        changed
    }

    /// The cell under a pixel position.
    fn cell_at(&self, x: f64, y: f64) -> (u16, u16) {
        let column =
            ((x - f64::from(self.layout.offset.0)) / f64::from(self.layout.cell.0)).floor();
        let row = ((y - f64::from(self.layout.offset.1)) / f64::from(self.layout.cell.1)).floor();
        let clamp = |v: f64, max: u16| v.clamp(0.0, f64::from(max.saturating_sub(1))) as u16;
        (clamp(column, self.grid.0), clamp(row, self.grid.1))
    }

    /// The present mode for `vsync`: wait for the refresh, or show frames at once
    /// (mailbox where available, so a frame never waits for the screen either).
    fn present_mode(vsync: bool, modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
        if vsync {
            wgpu::PresentMode::AutoVsync
        } else if modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::AutoNoVsync
        }
    }

    /// Carries out a game's request.
    fn request(&mut self, request: &WindowRequest) {
        match request {
            WindowRequest::Fullscreen(on) => self
                .window
                .set_fullscreen(on.then_some(Fullscreen::Borderless(None))),
            WindowRequest::CursorVisible(on) => {
                self.cursor_visible = *on;
                self.window.set_cursor_visible(*on);
            }
            WindowRequest::Title(title) => self.window.set_title(title),
            WindowRequest::Vsync(on) => {
                if self.vsync != *on {
                    self.vsync = *on;
                    self.config.present_mode = Self::present_mode(*on, &self.present_modes);
                    self.surface.configure(&self.device, &self.config);
                }
            }
            WindowRequest::SmoothScaling(on) => self.smooth_scaling = *on,
            WindowRequest::Rumble { strength, duration } => {
                self.pads.rumble(*strength, *duration);
            }
            // Later kinds of requests are not known to this window.
            _ => {}
        }
    }

    fn window_state(&self) -> WindowState {
        WindowState {
            fullscreen: self.window.fullscreen().is_some(),
            cursor_visible: self.cursor_visible,
            vsync: self.vsync,
            smooth_scaling: self.smooth_scaling,
        }
    }

    fn toggle_fullscreen(&self) {
        let full = self.window.fullscreen().is_some();
        self.window
            .set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
    }

    fn mouse_event(&mut self, kind: MouseEventKind) {
        if let Some((column, row)) = self.mouse {
            self.events.push_back(Event::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: keys::modifiers(self.modifiers),
            }));
        }
    }

    /// Turns a window event into engine events.
    fn handle(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.events.push_back(Event::Closed),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if self.resize() {
                    self.events
                        .push_back(Event::Resize(Size::new(self.grid.0, self.grid.1)));
                }
            }
            WindowEvent::Focused(true) => self.events.push_back(Event::FocusGained),
            WindowEvent::Focused(false) => self.events.push_back(Event::FocusLost),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                let modifiers = keys::modifiers(self.modifiers);
                let Some(code) = keys::key_code(&event.logical_key, modifiers) else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                // F11 and Alt+Enter switch fullscreen; the game does not see them.
                let fullscreen = code == KeyCode::F(11)
                    || (code == KeyCode::Enter && modifiers.contains(KeyModifiers::ALT));
                if fullscreen {
                    if pressed && !event.repeat {
                        self.toggle_fullscreen();
                    }
                    return;
                }
                let kind = match (pressed, event.repeat) {
                    (false, _) => KeyEventKind::Release,
                    (true, true) => KeyEventKind::Repeat,
                    (true, false) => KeyEventKind::Press,
                };
                self.events.push_back(Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                }));
            }
            WindowEvent::CursorMoved { position, .. } => {
                let cell = self.cell_at(position.x, position.y);
                if self.mouse != Some(cell) {
                    self.mouse = Some(cell);
                    let held = [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
                        .into_iter()
                        .zip(self.buttons)
                        .find(|(_, down)| *down)
                        .map(|(button, _)| button);
                    self.mouse_event(held.map_or(MouseEventKind::Moved, MouseEventKind::Drag));
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (button, index) = match button {
                    winit::event::MouseButton::Left => (MouseButton::Left, 0),
                    winit::event::MouseButton::Right => (MouseButton::Right, 1),
                    winit::event::MouseButton::Middle => (MouseButton::Middle, 2),
                    _ => return,
                };
                let pressed = state == ElementState::Pressed;
                self.buttons[index] = pressed;
                self.mouse_event(if pressed {
                    MouseEventKind::Down(button)
                } else {
                    MouseEventKind::Up(button)
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / f64::from(self.layout.cell.1)) as f32,
                };
                let kind = if lines > 0.0 {
                    MouseEventKind::ScrollUp
                } else {
                    MouseEventKind::ScrollDown
                };
                for _ in 0..(lines.abs().round() as u32).clamp(1, 10) {
                    self.mouse_event(kind);
                }
            }
            _ => {}
        }
    }

    /// Draws a frame and shows it.
    fn present(&mut self, frame: &FrameView<'_>) -> io::Result<()> {
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.resize();
                return Ok(());
            }
            // Minimized, hidden or busy: skip this frame.
            _ => return Ok(()),
        };
        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..wgpu::TextureViewDescriptor::default()
        });
        self.renderer
            .render(&self.device, &self.queue, &view, frame, self.layout);
        self.window.pre_present_notify();
        self.queue.present(texture);
        self.presented += 1;
        if let Some((path, at)) = self.screenshot.clone() {
            if self.presented == at {
                self.screenshot = None;
                self.save_screenshot(frame, &path)?;
            }
        }
        Ok(())
    }

    /// Draws `frame` again into a texture of the window's size and saves it as a PNG.
    fn save_screenshot(&mut self, frame: &FrameView<'_>, path: &Path) -> io::Result<()> {
        let error = |e: &dyn std::fmt::Display| io::Error::other(e.to_string());
        let (width, height) = (self.config.width, self.config.height);
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vnce_window screenshot"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.view_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer
            .render(&self.device, &self.queue, &view, frame, self.layout);
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vnce_window screenshot"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            extent,
        );
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| error(&e))?;
        receiver
            .recv()
            .map_err(|e| error(&e))?
            .map_err(|e| error(&e))?;
        let bgra = matches!(self.view_format, wgpu::TextureFormat::Bgra8Unorm);
        let mut canvas = Canvas::new(width, height);
        {
            let data = buffer.get_mapped_range(..).map_err(|e| error(&e))?;
            for (y, line) in canvas.pixels_mut().chunks_mut(width as usize).enumerate() {
                let start = y * row as usize;
                for (x, pixel) in line.iter_mut().enumerate() {
                    let p = &data[start + x * 4..start + x * 4 + 4];
                    *pixel = if bgra {
                        Rgba::rgb(p[2], p[1], p[0])
                    } else {
                        Rgba::rgb(p[0], p[1], p[2])
                    };
                }
            }
        }
        buffer.unmap();
        let png = canvas.to_png().map_err(|e| error(&e))?;
        std::fs::write(path, png)
    }
}

/// The `winit` side: creates the window when the event loop starts and passes window
/// events to the shared state.
struct App {
    options: WindowOptions,
    window: Option<Arc<Window>>,
    state: Option<Rc<RefCell<State>>>,
    error: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let options = &self.options;
        let mut attributes = Window::default_attributes().with_title(options.title.clone());
        // Cells are sized once the font is known; until then, roughly 9 × 20 pixels.
        let size = match options.size {
            WindowSize::Cells(columns, rows) => {
                let font = crate::glyphs::Font::new(options.font_size);
                let (cw, ch) = font.cell_size();
                LogicalSize::new(u32::from(columns) * cw, u32::from(rows) * ch)
            }
            WindowSize::Pixels(width, height) => LogicalSize::new(width, height),
        };
        attributes = attributes.with_inner_size(size);
        if options.fullscreen {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        if let Some(icon) = &options.icon {
            let rgba: Vec<u8> = icon
                .pixels()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect();
            if let Ok(icon) = Icon::from_rgba(rgba, icon.width(), icon.height()) {
                attributes = attributes.with_window_icon(Some(icon));
            }
        }
        match event_loop.create_window(attributes) {
            Ok(window) => self.window = Some(Arc::new(window)),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(state) = &self.state {
            state.borrow_mut().handle(event);
        }
    }
}

/// The engine's backend for the window: it draws frames itself and never receives
/// escape sequences.
pub struct WindowBackend {
    state: Rc<RefCell<State>>,
}

impl Backend for WindowBackend {
    fn size(&self) -> io::Result<Size> {
        let grid = self.state.borrow().grid;
        Ok(Size::new(grid.0, grid.1))
    }

    fn write_all(&mut self, _bytes: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn enter(&mut self, _options: &TerminalOptions) -> io::Result<()> {
        Ok(())
    }

    fn leave(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn key_release_events(&self) -> bool {
        true
    }

    fn terminal_info(&self) -> TerminalInfo {
        let state = self.state.borrow();
        let (width, height) = state.layout.cell;
        TerminalInfo {
            responded: true,
            name: Some("vnce-window".to_owned()),
            native_window: true,
            cell_pixels: Some((width as u16, height as u16)),
            window_pixels: Some(state.layout.target),
            ..TerminalInfo::default()
        }
    }

    fn present(&mut self, frame: &FrameView<'_>) -> Option<io::Result<()>> {
        Some(self.state.borrow_mut().present(frame))
    }

    fn window(&mut self, request: &WindowRequest) {
        self.state.borrow_mut().request(request);
    }

    fn window_state(&self) -> Option<WindowState> {
        Some(self.state.borrow().window_state())
    }
}

/// The engine's driver for the window: the clock and the window's events.
pub struct WindowDriver {
    event_loop: EventLoop<()>,
    app: App,
    state: Rc<RefCell<State>>,
}

impl Driver for WindowDriver {
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn poll_event(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        {
            let mut state = self.state.borrow_mut();
            let state = &mut *state;
            state.pads.poll(&mut state.events);
            if let Some(event) = state.events.pop_front() {
                return Ok(Some(event));
            }
        }
        self.event_loop
            .pump_app_events(Some(timeout), &mut self.app);
        let mut state = self.state.borrow_mut();
        let state = &mut *state;
        // Gamepads are read after the wait too, so a button pressed during it arrives at
        // once (a wait is at most one frame).
        state.pads.poll(&mut state.events);
        Ok(state.events.pop_front())
    }
}

/// Why the window could not be opened.
#[derive(Debug)]
pub struct WindowError(pub String);

impl std::fmt::Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot open the window: {}", self.0)
    }
}

impl std::error::Error for WindowError {}

impl From<WindowError> for io::Error {
    fn from(error: WindowError) -> Self {
        io::Error::other(error)
    }
}

/// Opens a window and returns the engine's backend and driver for it, ready for
/// [`vnce_runtime::run_with`] (through a [`vnce_term::Terminal`]).
///
/// Must be called on the main thread (required by some platforms).
///
/// # Errors
///
/// Returns an error when there is no display, no GPU that can draw to the window, or the
/// window cannot be created.
pub fn open(options: &WindowOptions) -> Result<(WindowBackend, WindowDriver), WindowError> {
    let error = |e: &dyn std::fmt::Display| WindowError(e.to_string());
    let mut event_loop = EventLoop::new().map_err(|e| error(&e))?;
    let mut app = App {
        options: options.clone(),
        window: None,
        state: None,
        error: None,
    };
    // Run the loop until the window exists.
    let started = Instant::now();
    while app.window.is_none() && app.error.is_none() {
        event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut app);
        if started.elapsed() > Duration::from_secs(10) {
            return Err(WindowError("the window did not appear".to_owned()));
        }
    }
    if let Some(message) = app.error.take() {
        return Err(WindowError(message));
    }
    let window = app
        .window
        .clone()
        .ok_or_else(|| WindowError("no window".to_owned()))?;
    let state = gpu_state(&window, options).map_err(WindowError)?;
    let state = Rc::new(RefCell::new(state));
    app.state = Some(Rc::clone(&state));
    window.set_visible(true);
    Ok((
        WindowBackend {
            state: Rc::clone(&state),
        },
        WindowDriver {
            event_loop,
            app,
            state,
        },
    ))
}

/// The GPU device, the surface and the renderer for `window`.
fn gpu_state(window: &Arc<Window>, options: &WindowOptions) -> Result<State, String> {
    let (gpu, surface) = window_gpu(window)?;
    // The game's GPU pictures can now be shown without reading them back.
    gpu.set_presenting();
    let (adapter, device, queue) = (gpu.adapter(), gpu.device().clone(), gpu.queue().clone());
    let capabilities = surface.get_capabilities(adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .or_else(|| capabilities.formats.first().copied())
        .ok_or("the window has no surface format")?;
    // Colors are written as sRGB values: draw through a view without sRGB conversion.
    let view_format = format.remove_srgb_suffix();
    let present_mode = State::present_mode(options.vsync, &capabilities.present_modes);
    let size = window.inner_size();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode,
        desired_maximum_frame_latency: 1,
        alpha_mode: capabilities.alpha_modes[0],
        view_formats: if view_format == format {
            Vec::new()
        } else {
            vec![view_format]
        },
    };
    let px = (options.font_size * window.scale_factor() as f32).round();
    let renderer = Renderer::new(&device, view_format, px);
    let mut state = State {
        window: Arc::clone(window),
        surface,
        config,
        view_format,
        device,
        queue,
        renderer,
        font_size: options.font_size,
        font_px: px,
        grid: (0, 0),
        layout: Layout {
            cell: (1, 1),
            offset: (0, 0),
            target: (size.width.max(1), size.height.max(1)),
        },
        events: VecDeque::new(),
        modifiers: ModifiersState::empty(),
        mouse: None,
        buttons: [false; 3],
        presented: 0,
        screenshot: options.screenshot.clone(),
        pads: Gamepads::empty(),
        cursor_visible: true,
        vsync: options.vsync,
        smooth_scaling: true,
        present_modes: capabilities.present_modes.clone(),
    };
    state.pads = Gamepads::new(&mut state.events);
    state.resize();
    Ok(state)
}

/// The GPU to draw the window with, and its surface: the process's GPU when it can draw
/// to the window (so the game's textures can be shown directly), else a new one that
/// becomes the process's GPU.
fn window_gpu(window: &Arc<Window>) -> Result<(vnce_gpu::Gpu, wgpu::Surface<'static>), String> {
    if let Some(gpu) = vnce_gpu::Gpu::shared() {
        if let Ok(surface) = gpu.instance().create_surface(Arc::clone(window)) {
            if gpu.adapter().is_surface_supported(&surface) {
                return Ok((gpu, surface));
            }
        }
    }
    let descriptor =
        wgpu::InstanceDescriptor::new_with_display_handle(Box::new(Arc::clone(window))).with_env();
    let instance = wgpu::Instance::new(descriptor);
    let surface = instance
        .create_surface(Arc::clone(window))
        .map_err(|e| e.to_string())?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        ..wgpu::RequestAdapterOptions::default()
    }))
    .map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("vnce_window device"),
        required_limits: adapter.limits(),
        ..wgpu::DeviceDescriptor::default()
    }))
    .map_err(|e| e.to_string())?;
    let gpu = vnce_gpu::Gpu::from_wgpu(instance, adapter, device, queue);
    gpu.share();
    Ok((gpu, surface))
}
