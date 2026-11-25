//! The double-buffered terminal.

use std::io;
use std::time::{Duration, Instant};

use vnce_core::{Buffer, Color, Rect, Size, Style};
use vnce_raster::{Canvas, PixelMode, RenderOptions, ScaleQuality, protocol};

use crate::ansi::{self, StyleState};
use crate::backend::{Backend, CrosstermBackend};
use crate::capabilities::Capabilities;
use crate::graphics::{GraphicsMode, GraphicsProtocol};
use crate::query::TerminalInfo;

/// Which terminal modes are enabled by [`Terminal::enter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent terminal mode"
)]
pub struct TerminalOptions {
    /// Draw on the alternate screen, so the previous terminal contents come back on exit.
    pub alternate_screen: bool,
    /// Hide the text cursor while drawing.
    pub hide_cursor: bool,
    /// Report mouse events.
    pub mouse_capture: bool,
    /// Report pasted text as a single paste event instead of key presses.
    pub bracketed_paste: bool,
    /// Report focus gained / lost events.
    pub focus_events: bool,
    /// Enable the kitty keyboard protocol where supported, to receive key repeat and
    /// release events for every key (needed for "key held down" in games).
    ///
    /// In this mode character keys are reported as the base key plus modifiers: `Shift+=`
    /// arrives as `Char('=')` with [`KeyModifiers::SHIFT`](crate::KeyModifiers::SHIFT), not
    /// as `Char('+')`. On Windows releases are always reported and this option has no
    /// effect.
    pub keyboard_enhancement: bool,
    /// Ask the terminal what it supports when entering ([`Terminal::info`]): name and
    /// version, graphics protocols, cell size in pixels, keyboard protocol. Takes at most
    /// [`QUERY_TIMEOUT`](crate::query::QUERY_TIMEOUT), usually a few milliseconds.
    pub query_terminal: bool,
}

impl Default for TerminalOptions {
    /// Alternate screen, hidden cursor and bracketed paste; no mouse or focus events and no
    /// keyboard enhancement.
    ///
    /// The terminal is queried (see [`TerminalOptions::query_terminal`]).
    fn default() -> Self {
        Self {
            alternate_screen: true,
            hide_cursor: true,
            mouse_capture: false,
            bracketed_paste: true,
            focus_events: false,
            keyboard_enhancement: false,
            query_terminal: true,
        }
    }
}

/// An image drawn in the current frame with a graphics protocol.
#[derive(Debug)]
struct ImageRequest {
    area: Rect,
    /// The image, or `None` when the identical image is already on the screen.
    canvas: Option<Canvas>,
}

/// An image shown on the screen with a graphics protocol.
#[derive(Debug)]
struct PlacedImage {
    area: Rect,
    /// Kitty image id.
    id: u32,
    /// Kept to send the image again (after a clear or when an overlapping image is
    /// removed).
    canvas: Canvas,
    /// Kitty images at the exact cell resolution are split into tiles, so only the tiles
    /// that change are sent again.
    tiles: Option<Tiles>,
    /// The Sixel image as drawn: `canvas` scaled to the pixels of its area. Compared with
    /// the next frame to find the cells that changed.
    /// `None` when `canvas` already has that size (it is then drawn as it is; see
    /// [`PlacedImage::shown`]).
    shown: Option<Canvas>,
}

/// A Kitty image split into a grid of tiles, each an image of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tiles {
    /// Size of a tile in cells (tiles on the right and bottom edges may be smaller).
    columns: u16,
    rows: u16,
    /// Number of tiles across and down.
    across: u16,
    down: u16,
    /// Kitty image id of each tile, row by row.
    ids: Vec<u32>,
}

impl Tiles {
    /// The area (in cells) of tile `index` of an image in `area`.
    fn area(&self, area: Rect, index: usize) -> Rect {
        let index = u16::try_from(index).unwrap_or(u16::MAX);
        let (column, row) = (index % self.across, index / self.across);
        let x = column * self.columns;
        let y = row * self.rows;
        Rect::new(
            area.x + x,
            area.y + y,
            self.columns.min(area.width - x),
            self.rows.min(area.height - y),
        )
    }
}

/// What to send for an image after the cells.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ImageSend {
    /// The whole image (index into `placed`).
    Full(usize),
    /// Only these areas (in cells) of a Sixel or iTerm2 image.
    Patches(usize, Vec<Rect>),
    /// Only these tiles of a tiled Kitty image.
    Tiles(usize, Vec<usize>),
}

/// A drawing surface for one frame, passed to the closure of [`Terminal::draw`].
#[derive(Debug)]
pub struct Frame<'a> {
    buffer: &'a mut Buffer,
    count: u64,
    graphics: GraphicsMode,
    cell_pixels: (u16, u16),
    images: &'a mut Vec<ImageRequest>,
    placed: &'a [PlacedImage],
    spare: &'a mut Vec<Canvas>,
    scale_quality: ScaleQuality,
}

impl Frame<'_> {
    /// Draws `canvas` scaled to fill `area` (in cells), with the terminal's
    /// [`GraphicsMode`]: real pixels through a graphics protocol, or cells.
    ///
    /// With Sixel and iTerm2 the image never covers the last row of the terminal (the
    /// terminal would scroll), and text drawn under an image is hidden by it. Images that
    /// do not change are not sent again.
    pub fn draw_image(&mut self, area: Rect, canvas: &Canvas) {
        let area = self.image_area(area);
        match self.graphics {
            GraphicsMode::Cells(mode) => {
                if area.is_empty() || canvas.width() == 0 || canvas.height() == 0 {
                    return;
                }
                let (columns, rows) = mode.cell_size();
                let scaled = canvas.scaled_with(
                    u32::from(area.width) * columns,
                    u32::from(area.height) * rows,
                    self.scale_quality,
                );
                scaled.render(self.buffer, area, mode, &RenderOptions::default());
            }
            GraphicsMode::Protocol(_) => {
                if area.is_empty() || canvas.width() == 0 || canvas.height() == 0 {
                    return;
                }
                // Compared pixel by pixel: stops at the first difference, so a changed
                // image costs little, and an identical one about one pass over memory.
                let on_screen = self
                    .placed
                    .iter()
                    .any(|placed| placed.area == area && same_pixels(&placed.canvas, canvas));
                // A later image in the same area replaces an earlier one.
                if let Some(index) = self.images.iter().position(|r| r.area == area) {
                    let earlier = self.images.remove(index);
                    recycle(self.spare, earlier.canvas);
                }
                // The copy goes into the memory of an image that left the screen, when
                // there is one, instead of a new allocation.
                let copy = (!on_screen).then(|| match self.spare.pop() {
                    Some(mut spare) => {
                        spare.clone_from(canvas);
                        spare
                    }
                    None => canvas.clone(),
                });
                self.images.push(ImageRequest { area, canvas: copy });
            }
        }
    }

    /// How [`Frame::draw_image`] shows images.
    #[must_use]
    pub fn graphics_mode(&self) -> GraphicsMode {
        self.graphics
    }

    /// The part of `area` that [`Frame::draw_image`] actually covers: inside the terminal
    /// and, with Sixel and iTerm2, above the last row.
    fn image_area(&self, area: Rect) -> Rect {
        let mut area = area.intersection(self.buffer.area());
        if let GraphicsMode::Protocol(protocol) = self.graphics {
            if protocol != GraphicsProtocol::Kitty {
                let last_row = self.buffer.size().height.saturating_sub(1);
                area.height = area.height.min(last_row.saturating_sub(area.y));
            }
        }
        area
    }

    /// The size in pixels at which an image drawn in `area` with [`Frame::draw_image`] is
    /// shown without any scaling: the pixels of the pixel mode for cells, the pixels of the
    /// cells (see [`Frame::cell_pixels`]) for graphics protocols.
    ///
    /// Rendering at this size (for example on the GPU) gives the sharpest image for the
    /// least work. Returns `(0, 0)` when nothing would be drawn.
    #[must_use]
    pub fn image_pixel_size(&self, area: Rect) -> (u32, u32) {
        let area = self.image_area(area);
        let (columns, rows) = match self.graphics {
            GraphicsMode::Cells(mode) => mode.cell_size(),
            GraphicsMode::Protocol(_) => {
                (u32::from(self.cell_pixels.0), u32::from(self.cell_pixels.1))
            }
        };
        (
            u32::from(area.width) * columns,
            u32::from(area.height) * rows,
        )
    }

    /// Width divided by height of one pixel of [`Frame::image_pixel_size`] on the screen.
    ///
    /// Graphics protocols have square pixels (1.0); cell modes usually do not (a quadrant
    /// pixel is half a cell wide and half a cell high, so about 1:2). Multiply the aspect
    /// ratio of an image size in pixels by this value to get its real aspect ratio, for
    /// example for a camera.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "cell sizes are small")]
    pub fn image_pixel_aspect(&self) -> f32 {
        match self.graphics {
            GraphicsMode::Cells(mode) => {
                let (columns, rows) = mode.cell_size();
                let (width, height) = self.cell_pixels;
                (f32::from(width) / columns as f32) / (f32::from(height) / rows as f32)
            }
            GraphicsMode::Protocol(_) => 1.0,
        }
    }

    /// Size of one cell in pixels, as in [`Terminal::cell_pixels`].
    ///
    /// Useful to compute the real aspect ratio of an area, for example for a camera:
    /// `width * cell_width / (height * cell_height)`.
    #[must_use]
    pub fn cell_pixels(&self) -> (u16, u16) {
        self.cell_pixels
    }

    /// The drawable area (the whole terminal).
    #[must_use]
    pub fn area(&self) -> Rect {
        self.buffer.area()
    }

    /// The buffer of this frame. It starts blank on every frame.
    #[must_use]
    pub fn buffer(&self) -> &Buffer {
        self.buffer
    }

    /// Mutable access to the buffer of this frame.
    pub fn buffer_mut(&mut self) -> &mut Buffer {
        self.buffer
    }

    /// Number of frames drawn before this one.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }
}

/// A terminal that draws frames with double buffering.
///
/// Each call to [`Terminal::draw`] renders into a blank buffer, compares it with the
/// previous frame and writes only the cells that changed, in a single write. The terminal
/// is restored when the value is dropped.
///
/// ```no_run
/// use vnce_core::{Color, Style};
/// use vnce_term::Terminal;
///
/// let mut terminal = Terminal::enter()?;
/// terminal.draw(|frame| {
///     frame
///         .buffer_mut()
///         .set_string(0, 0, "Hello!", Style::new().fg(Color::Green));
/// })?;
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent settings and states of the terminal, not a state machine"
)]
pub struct Terminal<B: Backend> {
    backend: B,
    /// The frame being drawn and the previous frame, indexed by `current`.
    buffers: [Buffer; 2],
    current: usize,
    capabilities: Capabilities,
    /// What the terminal reported about itself when entering.
    info: TerminalInfo,
    entered: bool,
    /// Set when the screen contents are unknown (first frame, resize, [`Terminal::clear`]).
    full_redraw: bool,
    /// Reused output buffer, so drawing does not allocate on every frame.
    output: Vec<u8>,
    frame_count: u64,
    /// How images are shown.
    graphics: GraphicsMode,
    images: ImageState,
    /// Inside tmux: Kitty commands are wrapped for passthrough and images are shown with
    /// Unicode placeholders.
    tmux_passthrough: bool,
    /// Sixel images are sent at half height with rows two pixels tall (terminals that
    /// apply the `"2;1` aspect ratio: Windows Terminal).
    sixel_tall_pixels: bool,
    /// Frames left in which changed images are not sent, because the terminal was slow to
    /// take the last frame (used when tall Sixel pixels are not).
    deferred_frames: u8,
}

/// Bookkeeping for images shown with a graphics protocol.
#[derive(Debug)]
struct ImageState {
    /// Images requested in the frame being drawn.
    requests: Vec<ImageRequest>,
    /// Images currently on the screen (graphics protocols only).
    placed: Vec<PlacedImage>,
    /// Canvases of images that left the screen, reused for the copies of new images (at
    /// most [`SPARE_CANVASES`]).
    spare: Vec<Canvas>,
    /// Whether Kitty images may be on the screen (they survive clearing the screen).
    kitty_shown: bool,
    next_id: u32,
    /// Ids freed when tiled images were rebuilt, used again first.
    free_ids: Vec<u32>,
    /// Kitty images are shown with Unicode placeholders (inside tmux).
    placeholders: bool,
    /// Changed images keep their old content on the screen this frame (the terminal is
    /// catching up).
    defer: bool,
    /// Keeps the Sixel palette from frame to frame (faster, no color flicker).
    sixel: protocol::sixel::Encoder,
    /// How images are scaled to the pixels or cells of their area.
    scale_quality: ScaleQuality,
}

impl Terminal<CrosstermBackend> {
    /// Takes over the real terminal with [`TerminalOptions::default`].
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal cannot be configured (for example when stdout is
    /// not a terminal).
    pub fn enter() -> io::Result<Self> {
        Self::enter_with(TerminalOptions::default())
    }

    /// Takes over the real terminal with the given options.
    ///
    /// Capabilities are detected with [`Capabilities::detect`].
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal cannot be configured (for example when stdout is
    /// not a terminal).
    pub fn enter_with(options: TerminalOptions) -> io::Result<Self> {
        let mut terminal = Self::new(CrosstermBackend::new(), Capabilities::detect())?;
        terminal.enter_mode(options)?;
        Ok(terminal)
    }
}

impl<B: Backend> Terminal<B> {
    /// Creates a terminal on top of a backend, without entering full-screen mode.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot report its size.
    pub fn new(backend: B, capabilities: Capabilities) -> io::Result<Self> {
        let size = backend.size()?;
        Ok(Self {
            backend,
            buffers: [Buffer::empty(size), Buffer::empty(size)],
            current: 0,
            capabilities,
            info: TerminalInfo::default(),
            entered: false,
            full_redraw: true,
            output: Vec::new(),
            frame_count: 0,
            graphics: GraphicsMode::Cells(PixelMode::detect(capabilities.color_depth)),
            tmux_passthrough: false,
            sixel_tall_pixels: false,
            deferred_frames: 0,
            images: ImageState {
                requests: Vec::new(),
                placed: Vec::new(),
                spare: Vec::new(),
                kitty_shown: false,
                next_id: 1,
                free_ids: Vec::new(),
                placeholders: false,
                defer: false,
                sixel: protocol::sixel::Encoder::new(),
                scale_quality: ScaleQuality::default(),
            },
        })
    }

    /// Enters full-screen mode with the given options. The next frame is fully redrawn.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot be configured.
    pub fn enter_mode(&mut self, options: TerminalOptions) -> io::Result<()> {
        self.backend.enter(&options)?;
        self.entered = true;
        self.full_redraw = true;
        if self.backend.key_release_events() {
            self.capabilities.key_release_events = true;
        }
        self.info = self.backend.terminal_info();
        self.graphics = GraphicsMode::detect(&self.info, &self.capabilities);
        self.tmux_passthrough = crate::tmux::inside();
        self.sixel_tall_pixels = sixel_tall_pixels(|name| std::env::var(name).ok());
        if let Some(quality) = scale_quality(|name| std::env::var(name).ok()) {
            self.set_scale_quality(quality);
        }
        Ok(())
    }

    /// How [`Frame::draw_image`] shows images. Detected when entering
    /// ([`GraphicsMode::detect`]); cells otherwise.
    #[must_use]
    pub fn graphics_mode(&self) -> GraphicsMode {
        self.graphics
    }

    /// Changes how images are shown. The next frame is fully redrawn and images shown with
    /// the previous mode are removed.
    pub fn set_graphics_mode(&mut self, mode: GraphicsMode) {
        if mode != self.graphics {
            self.graphics = mode;
            self.full_redraw = true;
        }
    }

    /// Whether Sixel images are sent at half height with rows two pixels tall: half the
    /// data and drawing work for the terminal. Detected when entering: on in Windows
    /// Terminal (which applies the `"2;1` aspect ratio), and set by `VNCE_SIXEL_ASPECT`
    /// (`2` on, `1` off). Used only when the cell height is even.
    #[must_use]
    pub fn sixel_tall_pixels(&self) -> bool {
        self.sixel_tall_pixels
    }

    /// Turns tall Sixel pixels on or off (see [`Terminal::sixel_tall_pixels`]). The next
    /// frame is fully redrawn.
    pub fn set_sixel_tall_pixels(&mut self, on: bool) {
        if on != self.sixel_tall_pixels {
            self.sixel_tall_pixels = on;
            self.full_redraw = true;
        }
    }

    /// How images are scaled to their area ([`Frame::draw_image`] in cells, and Sixel
    /// images): [`ScaleQuality::Fast`] by default, or as set by `VNCE_SCALE_QUALITY`
    /// (`fast` or `exact`) when entering.
    #[must_use]
    pub fn scale_quality(&self) -> ScaleQuality {
        self.images.scale_quality
    }

    /// Changes how images are scaled (see [`Terminal::scale_quality`]). The next frame is
    /// fully redrawn, so images on the screen are scaled again.
    pub fn set_scale_quality(&mut self, quality: ScaleQuality) {
        if quality != self.images.scale_quality {
            self.images.scale_quality = quality;
            self.full_redraw = true;
        }
    }

    /// Whether Kitty commands are wrapped for tmux passthrough and Kitty images are shown
    /// with Unicode placeholders. Set when entering inside tmux.
    #[must_use]
    pub fn tmux_passthrough(&self) -> bool {
        self.tmux_passthrough
    }

    /// Turns tmux passthrough on or off (see [`Terminal::tmux_passthrough`]). The next
    /// frame is fully redrawn.
    pub fn set_tmux_passthrough(&mut self, on: bool) {
        if on != self.tmux_passthrough {
            self.tmux_passthrough = on;
            self.full_redraw = true;
        }
    }

    /// Size of one cell in pixels: reported by the terminal, computed from the window size,
    /// or 10×20 as a fallback.
    #[must_use]
    pub fn cell_pixels(&self) -> (u16, u16) {
        if let Some(size) = self.info.cell_pixels {
            return size;
        }
        let size = self.size();
        if let Some((width, height)) = self.info.window_pixels {
            if size.width > 0 && size.height > 0 {
                let cell_width = u16::try_from(width / u32::from(size.width)).unwrap_or(0);
                let cell_height = u16::try_from(height / u32::from(size.height)).unwrap_or(0);
                if cell_width > 0 && cell_height > 0 {
                    return (cell_width, cell_height);
                }
            }
        }
        (10, 20)
    }

    /// What the terminal reported about itself when entering (empty if it was not queried,
    /// see [`TerminalOptions::query_terminal`]).
    #[must_use]
    pub fn info(&self) -> &TerminalInfo {
        &self.info
    }

    /// Leaves full-screen mode and restores the terminal. Called automatically on drop.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot be restored.
    pub fn leave_mode(&mut self) -> io::Result<()> {
        if self.entered {
            self.entered = false;
            if self.images.kitty_shown {
                // Kitty images are not removed by leaving the screen; free them.
                let mut out = Vec::new();
                protocol::kitty::delete_all(&mut out);
                if self.tmux_passthrough {
                    out = crate::tmux::wrap_kitty(&out);
                }
                self.backend.write_all(&out)?;
                self.backend.flush()?;
                self.images.kitty_shown = false;
            }
            self.images.placed.clear();
            self.backend.leave()?;
        }
        Ok(())
    }

    /// Draws one frame.
    ///
    /// The terminal size is checked first; after a resize the whole screen is redrawn.
    /// `render` receives a blank [`Frame`].
    ///
    /// # Errors
    ///
    /// Returns any error from the backend.
    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.autoresize()?;
        self.images.requests.clear();
        let cell_pixels = self.cell_pixels();
        let mut frame = Frame {
            buffer: &mut self.buffers[self.current],
            count: self.frame_count,
            graphics: self.graphics,
            cell_pixels,
            images: &mut self.images.requests,
            placed: &self.images.placed,
            spare: &mut self.images.spare,
            scale_quality: self.images.scale_quality,
        };
        render(&mut frame);
        self.flush()?;
        self.buffers[1 - self.current].clear();
        self.current = 1 - self.current;
        self.frame_count += 1;
        Ok(())
    }

    /// Resizes the buffers if the backend reports a different size.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot report its size.
    pub fn autoresize(&mut self) -> io::Result<()> {
        let size = self.backend.size()?;
        if size != self.size() {
            self.resize(size);
        }
        Ok(())
    }

    /// Resizes the buffers and schedules a full redraw.
    pub fn resize(&mut self, size: Size) {
        for buffer in &mut self.buffers {
            buffer.resize(size);
        }
        self.full_redraw = true;
    }

    /// Clears the screen and redraws everything on the next frame. Useful when something
    /// else wrote to the terminal.
    pub fn clear(&mut self) {
        self.full_redraw = true;
    }

    /// The current size in cells.
    #[must_use]
    pub fn size(&self) -> Size {
        self.buffers[self.current].size()
    }

    /// The capabilities used to adapt the output.
    #[must_use]
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Changes the capabilities. The next frame is fully redrawn.
    pub fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.capabilities = capabilities;
        self.full_redraw = true;
    }

    /// The backend.
    #[must_use]
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable access to the backend.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Number of frames drawn so far.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Writes the difference between the current and the previous frame.
    fn flush(&mut self) -> io::Result<()> {
        let capabilities = self.capabilities;
        let cell_pixels = self.cell_pixels();
        // Tall Sixel pixels need an even cell height, so patches start on a whole row.
        let tall_pixels = self.sixel_tall_pixels && cell_pixels.1.is_multiple_of(2);
        let sixel_options = protocol::sixel::SixelOptions {
            max_colors: self.info.sixel_colors.unwrap_or(256).clamp(2, 256),
            dither: false,
            pixel_aspect: if tall_pixels { 2 } else { 1 },
        };
        let (first, second) = self.buffers.split_at_mut(1);
        let (current, previous) = if self.current == 0 {
            (&mut first[0], &mut second[0])
        } else {
            (&mut second[0], &mut first[0])
        };
        let out = &mut self.output;
        out.clear();

        if capabilities.synchronized_output {
            ansi::begin_synchronized_update(out)?;
        }
        ansi::reset_style(out)?;
        let mut style = StyleState::RESET;
        if self.full_redraw {
            // After a reset and a clear, every cell shows a blank with default colors,
            // which is exactly `Cell::EMPTY`.
            ansi::clear_screen(out)?;
            previous.resize(current.size());
            if self.images.kitty_shown {
                protocol::kitty::delete_all(out);
                self.images.kitty_shown = false;
            }
        }

        self.images.placeholders = self.tmux_passthrough;
        self.images.defer = self.deferred_frames > 0 && !tall_pixels && !self.full_redraw;
        self.deferred_frames = self.deferred_frames.saturating_sub(1);
        let sends = self.images.plan(
            self.graphics,
            self.full_redraw,
            cell_pixels,
            current,
            previous,
            out,
        );

        // Where the terminal cursor is, if known.
        let mut cursor: Option<(u16, u16)> = None;
        for update in current.diff_iter(previous) {
            match cursor {
                Some(at) if at == (update.x, update.y) => {}
                // A jump forward on the same row: `CSI n C` is shorter than a full move.
                Some((x, y)) if y == update.y && x < update.x => {
                    ansi::move_right(out, update.x - x)?;
                }
                _ => ansi::move_to(out, update.x, update.y)?,
            }
            let cell = update.cell;
            style.apply(
                out,
                cell.fg,
                cell.bg,
                cell.modifiers,
                capabilities.color_depth,
            )?;
            let exact = ansi::write_symbol(out, cell.symbol())?;
            let width = cell.width();
            // Terminals do not always agree on the width of wide characters (emoji in
            // particular), so the cursor position is re-sent after them.
            cursor = if exact && width == 1 {
                Some((update.x.saturating_add(1), update.y))
            } else {
                None
            };
        }

        // Images go on top of the cells written above.
        if let GraphicsMode::Protocol(graphics) = self.graphics {
            self.images
                .write(graphics, sends, cell_pixels, sixel_options, out)?;
        }

        ansi::reset_style(out)?;
        if capabilities.synchronized_output {
            ansi::end_synchronized_update(out)?;
        }
        // Also after switching away from Kitty, when its images are deleted.
        if self.tmux_passthrough {
            *out = crate::tmux::wrap_kitty(out);
        }
        let started = Instant::now();
        self.backend.write_all(out)?;
        self.backend.flush()?;
        // A slow write means the terminal could not keep up (its input was full): changed
        // images wait a few frames. Not needed with tall Sixel pixels, which halve the work.
        if !tall_pixels && matches!(self.graphics, GraphicsMode::Protocol(_)) {
            self.deferred_frames = self.deferred_frames.max(frames_to_defer(started.elapsed()));
        }
        self.full_redraw = false;
        Ok(())
    }
}

impl ImageState {
    /// Decides which requested images must be sent and updates `placed`. Returns the
    /// indices (into `placed`) of the images to send after the cells.
    ///
    /// Before the cells are diffed it also:
    /// - deletes Kitty images that are gone,
    /// - marks the cells of removed Sixel / iTerm2 images as changed, so the text under
    ///   them is redrawn,
    /// - keeps the cells under unchanged Sixel / iTerm2 images as they are, because
    ///   writing them would erase the image.
    #[allow(
        clippy::too_many_lines,
        reason = "one pass over the requests keeps every case of image updates together"
    )]
    fn plan(
        &mut self,
        graphics: GraphicsMode,
        full_redraw: bool,
        cell_pixels: (u16, u16),
        current: &mut Buffer,
        previous: &mut Buffer,
        out: &mut Vec<u8>,
    ) -> Vec<ImageSend> {
        let old = std::mem::take(&mut self.placed);
        let requests = std::mem::take(&mut self.requests);
        let GraphicsMode::Protocol(protocol) = graphics else {
            // Cell mode draws images into the buffer. Images of a previous protocol were
            // removed by the full redraw that switching modes causes.
            return Vec::new();
        };
        let kitty = protocol == GraphicsProtocol::Kitty;

        // Areas of old images that will not stay exactly as they are.
        // (An image replaced by a new one in the same area keeps its area covered, so only
        // the parts that changed are sent.)
        let removed: Vec<Rect> = old
            .iter()
            .filter(|placed| full_redraw || !requests.iter().any(|r| r.area == placed.area))
            .map(|placed| placed.area)
            .collect();

        let mut old: Vec<Option<PlacedImage>> = old.into_iter().map(Some).collect();
        let mut sends = Vec::new();
        for request in requests {
            let same_area = old
                .iter()
                .position(|placed| placed.as_ref().is_some_and(|p| p.area == request.area));
            let previous_image = same_area.and_then(|index| old[index].take());
            // Removing a Sixel / iTerm2 image redraws the text under it, which also erases
            // any image it overlapped; Kitty images are independent of the text.
            let overlaps_removed =
                !kitty && removed.iter().any(|area| area.intersects(request.area));
            let can_update = !full_redraw && !overlaps_removed;
            let index = self.placed.len();
            match (previous_image, request.canvas) {
                // No canvas: the same pixels are already on the screen.
                (Some(placed), None) => {
                    if can_update {
                        if !kitty {
                            copy_cells(previous, current, request.area);
                        }
                    } else {
                        sends.push(ImageSend::Full(index));
                    }
                    self.placed.push(placed);
                }
                // The terminal is catching up: the old image stays for now.
                (Some(placed), Some(canvas)) if self.defer && can_update => {
                    if !kitty {
                        copy_cells(previous, current, request.area);
                    }
                    self.placed.push(placed);
                    recycle(&mut self.spare, Some(canvas));
                }
                // The image in this area changed: send only what changed when possible.
                (Some(mut placed), Some(canvas)) => {
                    // Scaling is costly: the image as drawn is made once, for the comparison
                    // and for sending.
                    let shown = sixel_shown(
                        protocol,
                        &canvas,
                        request.area,
                        cell_pixels,
                        self.scale_quality,
                    );
                    let update = can_update
                        .then(|| {
                            changed_parts(protocol, &placed, &canvas, shown.as_ref(), cell_pixels)
                        })
                        .flatten();
                    let old_canvas = std::mem::replace(&mut placed.canvas, canvas);
                    recycle(&mut self.spare, Some(old_canvas));
                    placed.shown = shown;
                    match update {
                        Some(Change::Patches(areas)) => {
                            if !kitty {
                                copy_cells(previous, current, request.area);
                            }
                            if !areas.is_empty() {
                                sends.push(ImageSend::Patches(index, areas));
                            }
                        }
                        Some(Change::Tiles(tiles)) => {
                            if !tiles.is_empty() {
                                sends.push(ImageSend::Tiles(index, tiles));
                            }
                        }
                        None => {
                            // A full resend under the same ids: the old data is freed first
                            // (inside the synchronized update, so nothing flickers), and
                            // tiles are rebuilt for the new size.
                            if kitty {
                                for id in placed.kitty_ids() {
                                    protocol::kitty::delete(id, out);
                                }
                            }
                            if let Some(tiles) = placed.tiles.take() {
                                self.free_ids.extend(tiles.ids);
                            }
                            placed.tiles =
                                self.tiles_for(protocol, &placed.canvas, request.area, cell_pixels);
                            if !kitty && !full_redraw {
                                // Old pixels outside the new image would otherwise stay.
                                invalidate_cells(previous, request.area);
                            }
                            sends.push(ImageSend::Full(index));
                        }
                    }
                    self.placed.push(placed);
                }
                (_, Some(canvas)) => {
                    let id = self.allocate_id();
                    let tiles = self.tiles_for(protocol, &canvas, request.area, cell_pixels);
                    let shown = sixel_shown(
                        protocol,
                        &canvas,
                        request.area,
                        cell_pixels,
                        self.scale_quality,
                    );
                    sends.push(ImageSend::Full(index));
                    self.placed.push(PlacedImage {
                        area: request.area,
                        id,
                        canvas,
                        tiles,
                        shown,
                    });
                }
                (_, None) => {}
            }
        }

        // Images that are gone.
        for placed in old.into_iter().flatten() {
            if !full_redraw {
                if kitty {
                    for id in placed.kitty_ids() {
                        protocol::kitty::delete(id, out);
                    }
                } else {
                    invalidate_cells(previous, placed.area);
                }
            }
            recycle(&mut self.spare, Some(placed.canvas));
        }
        if kitty && self.placeholders {
            // The cells of placeholder images show their placeholders, on every frame (the
            // buffer starts blank), so they only reach the terminal when they change.
            for placed in &self.placed {
                write_placeholders(current, placed);
            }
        }
        sends
    }

    /// Splits a Kitty image into tiles when it is exactly at the resolution of its cells.
    fn tiles_for(
        &mut self,
        protocol: GraphicsProtocol,
        canvas: &Canvas,
        area: Rect,
        cell_pixels: (u16, u16),
    ) -> Option<Tiles> {
        if protocol != GraphicsProtocol::Kitty
            || !exact_cells(canvas, area, cell_pixels)
            || area.area() < 16
        {
            return None;
        }
        // At most 8 × 8 tiles, at least 4 × 2 cells each.
        let columns = area.width.div_ceil(8).max(4);
        let rows = area.height.div_ceil(8).max(2);
        let across = area.width.div_ceil(columns);
        let down = area.height.div_ceil(rows);
        let ids = (0..usize::from(across) * usize::from(down))
            .map(|_| self.allocate_id())
            .collect();
        Some(Tiles {
            columns,
            rows,
            across,
            down,
            ids,
        })
    }

    fn allocate_id(&mut self) -> u32 {
        // Ids of tiles that were rebuilt are used again first.
        if let Some(id) = self.free_ids.pop() {
            return id;
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        id
    }
}

impl ImageState {
    /// Sends the images planned by [`ImageState::plan`].
    fn write(
        &mut self,
        graphics: GraphicsProtocol,
        sends: Vec<ImageSend>,
        cell_pixels: (u16, u16),
        sixel_options: protocol::sixel::SixelOptions,
        out: &mut Vec<u8>,
    ) -> io::Result<()> {
        let sixel = &mut self.sixel;
        for send in sends {
            match send {
                ImageSend::Full(index) => {
                    let image = &self.placed[index];
                    if let Some(tiles) = &image.tiles {
                        for tile in 0..tiles.ids.len() {
                            write_tile(image, tile, self.placeholders, out)?;
                        }
                    } else {
                        ansi::move_to(out, image.area.x, image.area.y)?;
                        write_image(
                            graphics,
                            image,
                            cell_pixels,
                            sixel_options,
                            sixel,
                            self.placeholders,
                            self.scale_quality,
                            out,
                        );
                    }
                }
                ImageSend::Patches(index, areas) => {
                    let image = &self.placed[index];
                    for area in areas {
                        ansi::move_to(out, area.x, area.y)?;
                        write_patch(
                            graphics,
                            image,
                            area,
                            cell_pixels,
                            sixel_options,
                            sixel,
                            out,
                        );
                    }
                }
                ImageSend::Tiles(index, tiles) => {
                    let image = &self.placed[index];
                    for tile in tiles {
                        if let Some(id) = image.tiles.as_ref().map(|t| t.ids[tile]) {
                            protocol::kitty::delete(id, out);
                        }
                        write_tile(image, tile, self.placeholders, out)?;
                    }
                }
            }
            if graphics == GraphicsProtocol::Kitty {
                self.kitty_shown = true;
            }
        }
        Ok(())
    }
}

impl PlacedImage {
    /// The Sixel image as drawn: the scaled copy, or the canvas when it needs no scaling.
    fn shown(&self) -> &Canvas {
        self.shown.as_ref().unwrap_or(&self.canvas)
    }

    /// Every Kitty id of the image: its tiles, or the image itself.
    fn kitty_ids(&self) -> Vec<u32> {
        self.tiles
            .as_ref()
            .map_or_else(|| vec![self.id], |tiles| tiles.ids.clone())
    }
}

/// The parts of an image that changed.
enum Change {
    /// Areas (in cells) of a Sixel or iTerm2 image.
    Patches(Vec<Rect>),
    /// Tiles of a Kitty image.
    Tiles(Vec<usize>),
}

/// How many canvases of images that left the screen are kept for reuse: enough for a few
/// images changing every frame, without holding on to much memory.
const SPARE_CANVASES: usize = 2;

/// Keeps `canvas` for reuse if there is room among the spare canvases.
fn recycle(spare: &mut Vec<Canvas>, canvas: Option<Canvas>) {
    if let Some(canvas) = canvas {
        if spare.len() < SPARE_CANVASES {
            spare.push(canvas);
        }
    }
}

/// Sends patches only while they cover less than this share of the image; beyond it, one
/// whole image is cheaper.
const PATCH_LIMIT_PERCENT: usize = 70;

/// Whether to use tall Sixel pixels: `VNCE_SIXEL_ASPECT` (`2` or `1`) decides, otherwise
/// only Windows Terminal (`WT_SESSION`), the terminal known to apply the `"2;1` ratio.
fn sixel_tall_pixels(env: impl Fn(&str) -> Option<String>) -> bool {
    match env("VNCE_SIXEL_ASPECT").as_deref().map(str::trim) {
        Some("2") => true,
        Some("1") => false,
        _ => env("WT_SESSION").is_some(),
    }
}

/// The scale quality chosen with `VNCE_SCALE_QUALITY` (`fast` or `exact`), if any.
fn scale_quality(env: impl Fn(&str) -> Option<String>) -> Option<ScaleQuality> {
    env("VNCE_SCALE_QUALITY").and_then(|value| value.parse().ok())
}

/// Writing a frame that takes longer than this means the terminal is behind.
const SLOW_WRITE: Duration = Duration::from_millis(25);

/// How many frames to skip changed images for after a write that took `write`: none
/// when the terminal keeps up, one per 16 ms of delay otherwise (at most 3), so it can catch
/// up while text, input and game logic keep their pace.
fn frames_to_defer(write: Duration) -> u8 {
    if write < SLOW_WRITE {
        return 0;
    }
    u8::try_from(write.as_millis() / 16)
        .unwrap_or(u8::MAX)
        .clamp(1, 3)
}

/// Returns `true` if `canvas` has exactly the pixels of the cells of `area`, so its parts
/// map onto whole cells.
fn exact_cells(canvas: &Canvas, area: Rect, cell_pixels: (u16, u16)) -> bool {
    canvas.width() == u32::from(area.width) * u32::from(cell_pixels.0)
        && canvas.height() == u32::from(area.height) * u32::from(cell_pixels.1)
}

/// For Sixel, the image as drawn: scaled to the pixels of its area. `None` for other
/// protocols, and when `canvas` already has exactly those pixels (no copy is made).
fn sixel_shown(
    protocol: GraphicsProtocol,
    canvas: &Canvas,
    area: Rect,
    cell_pixels: (u16, u16),
    quality: ScaleQuality,
) -> Option<Canvas> {
    (protocol == GraphicsProtocol::Sixel && !exact_cells(canvas, area, cell_pixels)).then(|| {
        canvas.scaled_with(
            u32::from(area.width) * u32::from(cell_pixels.0),
            u32::from(area.height) * u32::from(cell_pixels.1),
            quality,
        )
    })
}

/// Which cells of `area` differ between two images of its exact cell resolution.
fn changed_cells(old: &Canvas, new: &Canvas, area: Rect, cell_pixels: (u16, u16)) -> Vec<bool> {
    let (cell_width, cell_height) = (usize::from(cell_pixels.0), usize::from(cell_pixels.1));
    let columns = usize::from(area.width);
    let mut changed = vec![false; columns * usize::from(area.height)];
    let width = new.width() as usize;
    for (y, (old_row, new_row)) in old
        .pixels()
        .chunks(width)
        .zip(new.pixels().chunks(width))
        .enumerate()
    {
        if old_row == new_row {
            continue;
        }
        let row = y / cell_height;
        for (x, (a, b)) in old_row.iter().zip(new_row).enumerate() {
            if a != b {
                changed[row * columns + x / cell_width] = true;
            }
        }
    }
    changed
}

/// What to send for an image that changed in place, or `None` for the whole image.
fn changed_parts(
    protocol: GraphicsProtocol,
    placed: &PlacedImage,
    canvas: &Canvas,
    shown: Option<&Canvas>,
    cell_pixels: (u16, u16),
) -> Option<Change> {
    let area = placed.area;
    match protocol {
        GraphicsProtocol::Kitty => {
            let tiles = placed.tiles.as_ref()?;
            if !exact_cells(canvas, area, cell_pixels)
                || (placed.canvas.width(), placed.canvas.height())
                    != (canvas.width(), canvas.height())
            {
                return None;
            }
            let changed = changed_cells(&placed.canvas, canvas, area, cell_pixels);
            let columns = usize::from(area.width);
            let dirty = (0..tiles.ids.len())
                .filter(|&index| {
                    let tile = tiles.area(area, index);
                    (tile.top()..tile.bottom()).any(|y| {
                        (tile.left()..tile.right()).any(|x| {
                            let (dx, dy) = (usize::from(x - area.x), usize::from(y - area.y));
                            changed[dy * columns + dx]
                        })
                    })
                })
                .collect();
            Some(Change::Tiles(dirty))
        }
        GraphicsProtocol::Sixel => {
            let old = placed.shown();
            let new = shown.unwrap_or(canvas);
            if (old.width(), old.height()) != (new.width(), new.height()) {
                return None;
            }
            patches(&changed_cells(old, new, area, cell_pixels), area)
        }
        GraphicsProtocol::Iterm2 => {
            if !exact_cells(canvas, area, cell_pixels)
                || !exact_cells(&placed.canvas, area, cell_pixels)
            {
                return None;
            }
            patches(
                &changed_cells(&placed.canvas, canvas, area, cell_pixels),
                area,
            )
        }
    }
}

/// Groups changed cells into areas: consecutive rows with changes become one area, as wide
/// as their changes. `None` when the areas would cover most of the image.
fn patches(changed: &[bool], area: Rect) -> Option<Change> {
    let columns = usize::from(area.width);
    let mut areas = Vec::new();
    let mut covered = 0;
    let mut group: Option<(usize, usize, usize, usize)> = None;
    let rows = changed.chunks(columns).map(|row| {
        let first = row.iter().position(|&c| c)?;
        let last = row.iter().rposition(|&c| c)?;
        Some((first, last))
    });
    for (y, row) in rows.chain(std::iter::once(None)).enumerate() {
        if let Some((first, last)) = row {
            group = Some(match group {
                Some((top, _, left, right)) => (top, y, left.min(first), right.max(last)),
                None => (y, y, first, last),
            });
        } else if let Some((top, bottom, left, right)) = group.take() {
            let patch = Rect::new(
                area.x + u16::try_from(left).unwrap_or(0),
                area.y + u16::try_from(top).unwrap_or(0),
                u16::try_from(right - left + 1).unwrap_or(0),
                u16::try_from(bottom - top + 1).unwrap_or(0),
            );
            covered += patch.area();
            areas.push(patch);
        }
    }
    (covered * 100 < area.area() * PATCH_LIMIT_PERCENT).then_some(Change::Patches(areas))
}

/// Encodes one image for `graphics` at the cursor position.
#[allow(
    clippy::too_many_arguments,
    reason = "each protocol needs a different part of the terminal state"
)]
fn write_image(
    graphics: GraphicsProtocol,
    image: &PlacedImage,
    cell_pixels: (u16, u16),
    sixel_options: protocol::sixel::SixelOptions,
    sixel: &mut protocol::sixel::Encoder,
    placeholders: bool,
    scale_quality: ScaleQuality,
    out: &mut Vec<u8>,
) {
    let area = image.area;
    match graphics {
        GraphicsProtocol::Kitty => send_kitty(&image.canvas, image.id, area, placeholders, out),
        GraphicsProtocol::Sixel => {
            // Sixel draws pixels 1:1, so the image is scaled to the exact size of its area.
            let width = u32::from(area.width) * u32::from(cell_pixels.0);
            let height = u32::from(area.height) * u32::from(cell_pixels.1);
            let options = sixel_options;
            let shown = image.shown();
            if (shown.width(), shown.height()) == (width, height) {
                sixel.encode(shown, options, out);
            } else {
                let scaled = image.canvas.scaled_with(width, height, scale_quality);
                sixel.encode(&scaled, options, out);
            }
        }
        GraphicsProtocol::Iterm2 => {
            #[cfg(feature = "png")]
            {
                // An image that cannot be encoded is simply not shown.
                let _ = protocol::iterm2::encode(&image.canvas, area.width, area.height, out);
            }
        }
    }
}

/// Sends one area (in cells) of a Sixel or iTerm2 image at the cursor.
fn write_patch(
    graphics: GraphicsProtocol,
    image: &PlacedImage,
    area: Rect,
    cell_pixels: (u16, u16),
    sixel_options: protocol::sixel::SixelOptions,
    sixel: &mut protocol::sixel::Encoder,
    out: &mut Vec<u8>,
) {
    let (cell_width, cell_height) = (u32::from(cell_pixels.0), u32::from(cell_pixels.1));
    let x = u32::from(area.x - image.area.x) * cell_width;
    let y = u32::from(area.y - image.area.y) * cell_height;
    let (width, height) = (
        u32::from(area.width) * cell_width,
        u32::from(area.height) * cell_height,
    );
    match graphics {
        GraphicsProtocol::Sixel => {
            let options = sixel_options;
            sixel.encode(&crop(image.shown(), x, y, width, height), options, out);
        }
        GraphicsProtocol::Iterm2 => {
            #[cfg(feature = "png")]
            {
                let part = crop(&image.canvas, x, y, width, height);
                // An image that cannot be encoded is simply not shown.
                let _ = protocol::iterm2::encode(&part, area.width, area.height, out);
            }
        }
        GraphicsProtocol::Kitty => {}
    }
}

/// Sends tile `tile` of a tiled Kitty image at its position.
fn write_tile(
    image: &PlacedImage,
    tile: usize,
    placeholders: bool,
    out: &mut Vec<u8>,
) -> io::Result<()> {
    let Some(tiles) = &image.tiles else {
        return Ok(());
    };
    let area = tiles.area(image.area, tile);
    // Tiled images have whole cells of pixels.
    let cell_width = image.canvas.width() / u32::from(image.area.width.max(1));
    let cell_height = image.canvas.height() / u32::from(image.area.height.max(1));
    let part = crop(
        &image.canvas,
        u32::from(area.x - image.area.x) * cell_width,
        u32::from(area.y - image.area.y) * cell_height,
        u32::from(area.width) * cell_width,
        u32::from(area.height) * cell_height,
    );
    ansi::move_to(out, area.x, area.y)?;
    send_kitty(&part, tiles.ids[tile], area, placeholders, out);
    Ok(())
}

/// Transmits a Kitty image for `area`: placed at the cursor, or with a virtual placement
/// shown by the placeholders in its cells.
fn send_kitty(canvas: &Canvas, id: u32, area: Rect, placeholders: bool, out: &mut Vec<u8>) {
    let options = protocol::kitty::Options::default();
    if placeholders {
        protocol::kitty::transmit_virtual(canvas, id, area.width, area.height, options, out);
    } else {
        let placement = protocol::kitty::Placement {
            id,
            columns: area.width,
            rows: area.height,
            z_index: 0,
        };
        protocol::kitty::transmit_and_place_with(canvas, placement, options, out);
    }
}

/// Writes the Unicode placeholders of a Kitty image (or of each of its tiles) into its
/// cells: the image id is the foreground color, the diacritics give row and column.
fn write_placeholders(buffer: &mut Buffer, image: &PlacedImage) {
    let parts: Vec<(Rect, u32)> = match &image.tiles {
        Some(tiles) => (0..tiles.ids.len())
            .map(|index| (tiles.area(image.area, index), tiles.ids[index]))
            .collect(),
        None => vec![(image.area, image.id)],
    };
    for (area, id) in parts {
        let (r, g, b) = protocol::kitty::placeholder_color(id);
        let style = Style::new().fg(Color::Rgb(r, g, b));
        for row in 0..area.height {
            for column in 0..area.width {
                if let Some(text) = protocol::kitty::placeholder(id, column, row) {
                    buffer.set_symbol(area.x + column, area.y + row, &text, style);
                }
            }
        }
    }
}

/// The `width` × `height` pixels of `canvas` starting at `(x, y)` (clipped).
fn crop(canvas: &Canvas, x: u32, y: u32, width: u32, height: u32) -> Canvas {
    let x = x.min(canvas.width());
    let y = y.min(canvas.height());
    let width = width.min(canvas.width() - x);
    let height = height.min(canvas.height() - y);
    let stride = canvas.width() as usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for row in y..y + height {
        let start = row as usize * stride + x as usize;
        pixels.extend_from_slice(&canvas.pixels()[start..start + width as usize]);
    }
    Canvas::from_pixels(width, height, pixels).unwrap_or_else(|| Canvas::new(0, 0))
}

/// Copies the cells of `area` from `from` to `to`.
fn copy_cells(from: &Buffer, to: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let (Some(cell), Some(target)) = (from.cell(x, y), to.cell_mut(x, y)) {
                target.clone_from(cell);
            }
        }
    }
}

/// Makes the cells of `area` differ from anything that can be drawn, so they are written
/// on the next diff.
fn invalidate_cells(buffer: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell_mut(x, y) {
                // A noncharacter: never produced by drawing.
                cell.set_symbol("\u{FFFF}");
            }
        }
    }
}

/// Returns `true` if two images have the same size and pixels (clip and blend mode are
/// not part of the picture).
fn same_pixels(a: &Canvas, b: &Canvas) -> bool {
    (a.width(), a.height()) == (b.width(), b.height()) && a.pixels() == b.pixels()
}

impl<B: Backend> Drop for Terminal<B> {
    fn drop(&mut self) {
        let _ = self.leave_mode();
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::{Color, ColorDepth, Modifiers, Style};

    use super::*;
    use crate::backend::TestBackend;

    fn terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(width, height), Capabilities::default())
            .expect("the test backend always reports a size")
    }

    fn output(terminal: &mut Terminal<TestBackend>) -> String {
        String::from_utf8(terminal.backend_mut().take_output()).expect("valid UTF-8")
    }

    #[test]
    fn first_frame_clears_and_draws() {
        let mut terminal = terminal(5, 2);
        terminal
            .draw(|f| {
                f.buffer_mut()
                    .set_string(1, 1, "hi", Style::new().fg(Color::Red));
            })
            .unwrap();
        assert_eq!(
            output(&mut terminal),
            "\x1b[?2026h\x1b[0m\x1b[2J\x1b[2;2H\x1b[31mhi\x1b[0m\x1b[?2026l"
        );
        assert_eq!(terminal.backend().screen_row(1), " hi  ");
        assert_eq!(
            terminal.backend().screen().cell(1, 1).unwrap().fg,
            Color::Red
        );
        assert_eq!(terminal.backend().flush_count(), 1);
        assert_eq!(terminal.frame_count(), 1);
    }

    #[test]
    fn unchanged_frames_write_no_cells() {
        let mut terminal = terminal(5, 1);
        let draw = |f: &mut Frame<'_>| {
            f.buffer_mut().set_string(0, 0, "abc", Style::new());
        };
        terminal.draw(draw).unwrap();
        output(&mut terminal);
        terminal.draw(draw).unwrap();
        assert_eq!(
            output(&mut terminal),
            "\x1b[?2026h\x1b[0m\x1b[0m\x1b[?2026l"
        );
    }

    #[test]
    fn only_changed_cells_are_written() {
        let mut terminal = terminal(6, 1);
        terminal
            .draw(|f| {
                f.buffer_mut().set_string(0, 0, "abcdef", Style::new());
            })
            .unwrap();
        output(&mut terminal);
        terminal
            .draw(|f| {
                f.buffer_mut().set_string(0, 0, "abXdeY", Style::new());
            })
            .unwrap();
        assert_eq!(
            output(&mut terminal),
            "\x1b[?2026h\x1b[0m\x1b[1;3HX\x1b[2CY\x1b[0m\x1b[?2026l"
        );
        assert_eq!(terminal.backend().screen_row(0), "abXdeY");
    }

    #[test]
    fn consecutive_cells_need_no_cursor_movement() {
        let mut terminal = terminal(4, 2);
        terminal
            .draw(|f| {
                f.buffer_mut().set_string(0, 0, "abcd", Style::new());
                f.buffer_mut().set_string(0, 1, "e", Style::new());
            })
            .unwrap();
        let out = output(&mut terminal);
        assert!(out.contains("\x1b[1;1Habcd\x1b[2;1He"), "{out:?}");
    }

    #[test]
    fn jumps_on_the_same_row_move_right() {
        let mut terminal = terminal(10, 2);
        terminal.draw(|_| {}).unwrap();
        output(&mut terminal);
        terminal
            .draw(|f| {
                f.buffer_mut().set_string(1, 0, "a", Style::new());
                f.buffer_mut().set_string(3, 0, "b", Style::new());
                f.buffer_mut().set_string(9, 0, "c", Style::new());
                f.buffer_mut().set_string(0, 1, "d", Style::new());
            })
            .unwrap();
        let out = output(&mut terminal);
        assert!(
            out.contains("\x1b[1;2Ha\x1b[Cb\x1b[5Cc\x1b[2;1Hd"),
            "{out:?}"
        );
        assert_eq!(
            terminal
                .backend()
                .screen()
                .cell(9, 0)
                .map(vnce_core::Cell::symbol),
            Some("c")
        );
    }

    #[test]
    fn removed_content_is_erased() {
        let mut terminal = terminal(3, 1);
        terminal
            .draw(|f| {
                f.buffer_mut().set_string(0, 0, "abc", Style::new());
            })
            .unwrap();
        terminal.draw(|_| {}).unwrap();
        assert_eq!(terminal.backend().screen_row(0), "   ");
    }

    #[test]
    fn wide_characters_resend_the_cursor() {
        let mut terminal = terminal(4, 1);
        terminal
            .draw(|f| {
                f.buffer_mut().set_string(0, 0, "界ab", Style::new());
            })
            .unwrap();
        let out = output(&mut terminal);
        assert!(out.contains("\x1b[1;1H界\x1b[1;3Hab"), "{out:?}");
        assert_eq!(terminal.backend().screen_row(0), "界ab");
    }

    #[test]
    fn styles_are_only_sent_when_they_change() {
        let mut terminal = terminal(4, 1);
        terminal
            .draw(|f| {
                let bold = Style::new().bold();
                f.buffer_mut().set_string(0, 0, "ab", bold);
                f.buffer_mut().set_string(2, 0, "cd", Style::new());
            })
            .unwrap();
        let out = output(&mut terminal);
        assert!(out.contains("\x1b[1mab\x1b[22mcd"), "{out:?}");
        let screen = terminal.backend().screen();
        assert_eq!(screen.cell(1, 0).unwrap().modifiers, Modifiers::BOLD);
        assert_eq!(screen.cell(2, 0).unwrap().modifiers, Modifiers::NONE);
    }

    #[test]
    fn resize_triggers_a_full_redraw() {
        let mut terminal = terminal(3, 1);
        let draw = |f: &mut Frame<'_>| {
            let width = f.area().width;
            f.buffer_mut()
                .set_string(0, 0, &"x".repeat(width.into()), Style::new());
        };
        terminal.draw(draw).unwrap();
        output(&mut terminal);

        terminal.backend_mut().resize(5, 2);
        terminal.draw(draw).unwrap();
        let out = output(&mut terminal);
        assert!(out.contains("\x1b[2J"), "{out:?}");
        assert_eq!(terminal.size(), Size::new(5, 2));
        assert_eq!(terminal.backend().screen_row(0), "xxxxx");
    }

    #[test]
    fn clear_forces_a_full_redraw() {
        let mut terminal = terminal(2, 1);
        let draw = |f: &mut Frame<'_>| {
            f.buffer_mut().set_string(0, 0, "ab", Style::new());
        };
        terminal.draw(draw).unwrap();
        output(&mut terminal);
        terminal.clear();
        terminal.draw(draw).unwrap();
        assert!(output(&mut terminal).contains("\x1b[2J\x1b[1;1Hab"));
    }

    #[test]
    fn capabilities_adapt_the_output() {
        let mut terminal = terminal(1, 1);
        terminal.set_capabilities(Capabilities {
            color_depth: ColorDepth::Indexed256,
            synchronized_output: false,
            key_release_events: false,
        });
        terminal
            .draw(|f| {
                f.buffer_mut()
                    .set_string(0, 0, "x", Style::new().fg(Color::Rgb(255, 0, 0)));
            })
            .unwrap();
        assert_eq!(
            output(&mut terminal),
            "\x1b[0m\x1b[2J\x1b[1;1H\x1b[38;5;196mx\x1b[0m"
        );
    }

    #[test]
    fn control_characters_in_cells_are_not_sent() {
        let mut terminal = terminal(2, 1);
        terminal
            .draw(|f| {
                if let Some(cell) = f.buffer_mut().cell_mut(0, 0) {
                    cell.set_symbol("\x1b[2J");
                }
            })
            .unwrap();
        let out = output(&mut terminal);
        assert_eq!(
            out, "\x1b[?2026h\x1b[0m\x1b[2J\x1b[1;1H \x1b[0m\x1b[?2026l",
            "the injected sequence must be replaced by a space"
        );
    }

    #[test]
    fn enter_and_leave_on_drop() {
        let mut terminal = terminal(1, 1);
        terminal.enter_mode(TerminalOptions::default()).unwrap();
        assert!(terminal.backend().is_entered());
        terminal.leave_mode().unwrap();
        assert!(!terminal.backend().is_entered());
    }

    #[test]
    fn key_release_capability_comes_from_the_backend() {
        let mut terminal = terminal(1, 1);
        assert!(!terminal.capabilities().key_release_events);
        terminal.backend_mut().set_key_release_events(true);
        terminal.enter_mode(TerminalOptions::default()).unwrap();
        assert!(terminal.capabilities().key_release_events);
    }

    #[test]
    fn entering_queries_the_terminal() {
        let mut terminal = terminal(1, 1);
        terminal
            .backend_mut()
            .set_query_answer(b"\x1bP>|foot(1.18.0)\x1b\\\x1b[?62;4;22c");
        terminal.enter_mode(TerminalOptions::default()).unwrap();
        assert_eq!(
            terminal.backend().queries(),
            [TerminalInfo::REQUEST.to_vec()]
        );
        assert!(terminal.info().sixel);
        assert_eq!(terminal.info().name.as_deref(), Some("foot"));
        // The query never shows up on the screen.
        assert_eq!(terminal.backend().output(), b"");
    }

    #[test]
    fn querying_can_be_disabled() {
        let mut terminal = terminal(1, 1);
        terminal.backend_mut().set_query_answer(b"\x1b[?62;4c");
        let options = TerminalOptions {
            query_terminal: false,
            ..TerminalOptions::default()
        };
        terminal.enter_mode(options).unwrap();
        assert_eq!(terminal.backend().queries(), &[] as &[Vec<u8>]);
        assert_eq!(terminal.info(), &TerminalInfo::default());
    }

    mod images {
        use vnce_raster::{Canvas, PixelMode, Rgba};

        use super::*;

        const KITTY: GraphicsMode = GraphicsMode::Protocol(GraphicsProtocol::Kitty);
        const SIXEL: GraphicsMode = GraphicsMode::Protocol(GraphicsProtocol::Sixel);
        const ITERM2: GraphicsMode = GraphicsMode::Protocol(GraphicsProtocol::Iterm2);

        fn image(color: Rgba) -> Canvas {
            Canvas::filled(4, 4, color)
        }

        fn sequences(terminal: &mut Terminal<TestBackend>) -> Vec<String> {
            let sequences = terminal
                .backend()
                .graphics_sequences()
                .iter()
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect();
            terminal.backend_mut().clear_graphics_sequences();
            sequences
        }

        fn with_mode(mode: GraphicsMode, width: u16, height: u16) -> Terminal<TestBackend> {
            let mut terminal = terminal(width, height);
            terminal.set_graphics_mode(mode);
            terminal
        }

        #[test]
        fn cell_mode_draws_into_the_buffer() {
            let mut terminal = with_mode(GraphicsMode::Cells(PixelMode::HalfBlock), 4, 2);
            terminal
                .draw(|f| f.draw_image(Rect::new(0, 0, 2, 1), &image(Rgba::rgb(255, 0, 0))))
                .unwrap();
            assert_eq!(sequences(&mut terminal), Vec::<String>::new());
            let cell = terminal.backend().screen().cell(1, 0).unwrap().clone();
            assert_eq!(cell.bg, Color::Rgb(255, 0, 0));
            assert_eq!(
                terminal.backend().screen().cell(2, 0).unwrap().bg,
                Color::Reset
            );
        }

        #[test]
        fn kitty_images_are_sent_once_and_deleted_when_gone() {
            let mut terminal = with_mode(KITTY, 10, 5);
            let area = Rect::new(1, 1, 3, 2);
            let red = image(Rgba::rgb(255, 0, 0));
            terminal.draw(|f| f.draw_image(area, &red)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1);
            assert!(
                sent[0].starts_with("\x1b_Ga=T,f=100,i=1,c=3,r=2,"),
                "{sent:?}"
            );
            // The image is placed at the top-left cell of its area.
            let output = String::from_utf8_lossy(terminal.backend().output()).into_owned();
            assert!(output.contains("\x1b[2;2H\x1b_G"), "{output:?}");

            // Same image again: nothing is sent.
            terminal.draw(|f| f.draw_image(area, &red)).unwrap();
            assert_eq!(sequences(&mut terminal), Vec::<String>::new());

            // A different image: the new one is sent, the old one deleted.
            // (The id is kept: the old data is freed, then the new image takes its place.)
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::rgb(0, 0, 255))))
                .unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 2, "{sent:?}");
            assert_eq!(sent[0], "\x1b_Ga=d,d=I,i=1,q=2\x1b\\");
            assert!(sent[1].contains(",i=1,"));

            // No image: deleted.
            terminal.draw(|_| {}).unwrap();
            assert_eq!(sequences(&mut terminal), ["\x1b_Ga=d,d=I,i=1,q=2\x1b\\"]);
        }

        #[test]
        fn clearing_resends_images() {
            let mut terminal = with_mode(KITTY, 10, 5);
            let area = Rect::new(0, 0, 2, 2);
            let red = image(Rgba::rgb(255, 0, 0));
            terminal.draw(|f| f.draw_image(area, &red)).unwrap();
            sequences(&mut terminal);
            terminal.clear();
            terminal.draw(|f| f.draw_image(area, &red)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 2, "{sent:?}");
            assert_eq!(sent[0], "\x1b_Ga=d,d=A,q=2\x1b\\");
            assert!(sent[1].starts_with("\x1b_Ga=T"));
        }

        #[test]
        fn sixel_images_protect_their_cells() {
            let mut terminal = with_mode(SIXEL, 10, 5);
            let area = Rect::new(0, 0, 4, 2);
            let green = image(Rgba::rgb(0, 255, 0));
            terminal.draw(|f| f.draw_image(area, &green)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1);
            // 4×2 cells of 10×20 pixels (the fallback cell size).
            assert!(
                sent[0].starts_with("\x1bP0;1;0q\"1;1;40;40"),
                "{}",
                &sent[0][..20]
            );

            // Text under an unchanged image is not written (it would erase the image).
            terminal
                .draw(|f| {
                    f.buffer_mut().set_string(0, 0, "hidden", Style::new());
                    f.draw_image(area, &green);
                })
                .unwrap();
            assert_eq!(sequences(&mut terminal), Vec::<String>::new());
            assert_eq!(terminal.backend().screen_row(0), "    en    ");

            // When the image goes away, the text under it is drawn.
            terminal
                .draw(|f| {
                    f.buffer_mut().set_string(0, 0, "hidden", Style::new());
                })
                .unwrap();
            assert_eq!(terminal.backend().screen_row(0), "hidden    ");
        }

        #[test]
        fn sixel_and_iterm2_images_avoid_the_last_row() {
            for mode in [SIXEL, ITERM2] {
                let mut terminal = with_mode(mode, 10, 3);
                terminal
                    .draw(|f| f.draw_image(Rect::new(0, 1, 2, 5), &image(Rgba::WHITE)))
                    .unwrap();
                let sent = sequences(&mut terminal);
                assert_eq!(sent.len(), 1, "{mode}");
                if mode == SIXEL {
                    // Rows 1..2 only: one row of 20 pixels.
                    assert!(sent[0].contains("\"1;1;20;20"), "{}", &sent[0][..20]);
                } else {
                    assert!(sent[0].contains(";width=2;height=1;"), "{}", sent[0]);
                }
            }
            // Kitty does not move the cursor, so it may use the last row.
            let mut terminal = with_mode(KITTY, 10, 3);
            terminal
                .draw(|f| f.draw_image(Rect::new(0, 1, 2, 5), &image(Rgba::WHITE)))
                .unwrap();
            assert!(sequences(&mut terminal)[0].contains(",c=2,r=2,"));
        }

        #[test]
        fn image_pixel_size_matches_the_drawn_size() {
            let area = Rect::new(0, 1, 4, 5);
            let cases = [
                // Half blocks: 1×2 pixels per cell, clipped to the 3 rows of the terminal.
                (GraphicsMode::Cells(PixelMode::HalfBlock), (4, 4)),
                (GraphicsMode::Cells(PixelMode::Octant), (8, 8)),
                // Fallback cells of 10×20 pixels; Kitty may use the last row.
                (KITTY, (40, 40)),
                // Sixel and iTerm2 stop above the last row.
                (SIXEL, (40, 20)),
                (ITERM2, (40, 20)),
            ];
            for (mode, expected) in cases {
                let mut terminal = with_mode(mode, 10, 3);
                terminal
                    .draw(|f| {
                        assert_eq!(f.cell_pixels(), (10, 20));
                        assert_eq!(f.image_pixel_size(area), expected, "{mode}");
                        assert_eq!(f.image_pixel_size(Rect::new(20, 0, 2, 2)), (0, 0));
                        // Cells of 10×20 pixels: half blocks and octants have square
                        // pixels, like graphics protocols (quadrants are tested below).
                        assert!((f.image_pixel_aspect() - 1.0).abs() < 1e-6, "{mode}");
                    })
                    .unwrap();
            }
        }

        #[test]
        fn quadrant_pixels_are_twice_as_high_as_wide() {
            let mut terminal = with_mode(GraphicsMode::Cells(PixelMode::Quadrant), 4, 4);
            terminal
                .draw(|f| assert!((f.image_pixel_aspect() - 0.5).abs() < 1e-6))
                .unwrap();
        }

        #[test]
        fn switching_modes_removes_kitty_images() {
            let mut terminal = with_mode(KITTY, 10, 5);
            let area = Rect::new(0, 0, 2, 2);
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::WHITE)))
                .unwrap();
            sequences(&mut terminal);
            terminal.set_graphics_mode(GraphicsMode::Cells(PixelMode::HalfBlock));
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::WHITE)))
                .unwrap();
            assert_eq!(sequences(&mut terminal), ["\x1b_Ga=d,d=A,q=2\x1b\\"]);
            assert_eq!(
                terminal.backend().screen().cell(0, 0).unwrap().bg,
                Color::Rgb(255, 255, 255)
            );
        }

        #[test]
        fn leaving_frees_kitty_images() {
            let mut terminal = with_mode(KITTY, 10, 5);
            terminal.enter_mode(TerminalOptions::default()).unwrap();
            terminal.set_graphics_mode(KITTY);
            terminal
                .draw(|f| f.draw_image(Rect::new(0, 0, 2, 2), &image(Rgba::WHITE)))
                .unwrap();
            sequences(&mut terminal);
            terminal.leave_mode().unwrap();
            assert_eq!(sequences(&mut terminal), ["\x1b_Ga=d,d=A,q=2\x1b\\"]);
        }

        /// A canvas of `columns` × `rows` cells of 10 × 20 pixels (the fallback cell size).
        fn exact(columns: u32, rows: u32, color: Rgba) -> Canvas {
            Canvas::filled(columns * 10, rows * 20, color)
        }

        /// The cursor positions (1-based `row;column`) that start image sequences.
        fn image_positions(terminal: &Terminal<TestBackend>, start: &str) -> Vec<String> {
            let output = String::from_utf8_lossy(terminal.backend().output()).into_owned();
            output
                .match_indices(start)
                .filter_map(|(at, _)| {
                    let before = &output[..at];
                    let csi = before.rfind("\x1b[")?;
                    before[csi..].strip_suffix('H').map(|p| p[2..].to_owned())
                })
                .collect()
        }

        #[test]
        fn scale_quality_is_used_for_cell_images_and_redraws() {
            // A gradient scaled down to cells: both qualities stay within one per channel.
            let mut canvas = Canvas::new(37, 23);
            for (i, pixel) in canvas.pixels_mut().iter_mut().enumerate() {
                let v = u8::try_from(i * 7 % 256).unwrap_or(0);
                *pixel = Rgba::new(v, 255 - v, v / 2, 200);
            }
            let area = Rect::new(0, 0, 5, 3);
            let mut cells = Vec::new();
            for quality in ScaleQuality::ALL {
                let mut terminal = with_mode(GraphicsMode::Cells(PixelMode::HalfBlock), 5, 3);
                assert_eq!(terminal.scale_quality(), ScaleQuality::Fast);
                terminal.set_scale_quality(quality);
                assert_eq!(terminal.scale_quality(), quality);
                terminal.draw(|f| f.draw_image(area, &canvas)).unwrap();
                cells.push(terminal.backend().screen().clone());

                // Changing the quality redraws the whole screen.
                terminal.backend_mut().take_output();
                let other = ScaleQuality::ALL.into_iter().find(|q| *q != quality);
                terminal.set_scale_quality(other.unwrap_or(quality));
                terminal.draw(|f| f.draw_image(area, &canvas)).unwrap();
                let out = String::from_utf8(terminal.backend_mut().take_output()).unwrap();
                assert!(out.contains("\x1b[2J"), "{out:?}");
            }
            let close = |a: Color, b: Color| match (a, b) {
                (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                    r1.abs_diff(r2) <= 1 && g1.abs_diff(g2) <= 1 && b1.abs_diff(b2) <= 1
                }
                (a, b) => a == b,
            };
            for (fast, exact) in cells[0].cells().iter().zip(cells[1].cells()) {
                assert!(close(fast.fg, exact.fg) && close(fast.bg, exact.bg));
            }
        }

        #[test]
        fn sixel_images_send_only_the_cells_that_changed() {
            let mut terminal = with_mode(SIXEL, 12, 6);
            let area = Rect::new(1, 1, 6, 3);
            let mut picture = exact(6, 3, Rgba::rgb(10, 20, 30));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1);
            assert!(sent[0].contains("\"1;1;60;60"), "the whole image first");

            // One pixel in cell (2, 1) of the image: only that cell is sent.
            picture.pixel(25, 30, Rgba::rgb(250, 0, 0));
            terminal.backend_mut().take_output();
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1, "{sent:?}");
            assert!(sent[0].contains("\"1;1;10;20"), "one cell");
            assert_eq!(image_positions(&terminal, "\x1bP"), ["3;4"]);

            // Changes in two separate rows of cells: two patches.
            picture.pixel(5, 5, Rgba::rgb(0, 250, 0));
            picture.pixel(55, 45, Rgba::rgb(0, 0, 250));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 2, "{sent:?}");

            // Most of the image changed: the whole image again.
            let other = exact(6, 3, Rgba::rgb(200, 200, 200));
            terminal.draw(|f| f.draw_image(area, &other)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1);
            assert!(sent[0].contains("\"1;1;60;60"));
        }

        #[test]
        fn sixel_images_at_cell_resolution_keep_no_scaled_copy() {
            let mut terminal = with_mode(SIXEL, 12, 6);
            let area = Rect::new(1, 1, 6, 3);
            let mut picture = exact(6, 3, Rgba::rgb(10, 20, 30));
            picture.fill_rect(12, 7, 30, 25, Rgba::rgb(200, 40, 90));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            assert!(terminal.images.placed[0].shown.is_none());
            // Sent exactly as the encoder writes the canvas itself.
            let mut expected = Vec::new();
            let options = protocol::sixel::SixelOptions {
                max_colors: 256,
                dither: false,
                pixel_aspect: 1,
            };
            protocol::sixel::Encoder::new().encode(&picture, options, &mut expected);
            assert_eq!(
                sequences(&mut terminal),
                [String::from_utf8_lossy(&expected).into_owned()]
            );

            // An image of another size is scaled once and the scaled copy kept.
            terminal
                .draw(|f| f.draw_image(area, &Canvas::filled(7, 7, Rgba::WHITE)))
                .unwrap();
            let shown = terminal.images.placed[0].shown.as_ref().unwrap();
            assert_eq!((shown.width(), shown.height()), (60, 60));
        }

        #[test]
        fn kitty_images_at_cell_resolution_are_tiled() {
            let mut terminal = with_mode(KITTY, 20, 10);
            let area = Rect::new(0, 0, 16, 8);
            let mut picture = exact(16, 8, Rgba::rgb(10, 20, 30));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            // Tiles of 4 × 2 cells: 4 × 4 tiles.
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 16, "{}", sent.len());
            assert!(sent.iter().all(|s| s.contains(",c=4,r=2,")));

            // A pixel in the third tile of the second row: only that tile.
            picture.pixel(85, 50, Rgba::rgb(255, 0, 0));
            terminal.backend_mut().take_output();
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 2, "{sent:?}");
            assert!(
                sent[0].starts_with("\x1b_Ga=d,d=I,i="),
                "the old tile is freed"
            );
            assert!(sent[1].starts_with("\x1b_Ga=T,f=100,"));
            assert_eq!(image_positions(&terminal, "\x1b_Ga=T"), ["3;9"]);

            // Removed: every tile is deleted.
            terminal.draw(|_| {}).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 16);
            assert!(sent.iter().all(|s| s.starts_with("\x1b_Ga=d,d=I,")));

            // Images that are not at cell resolution stay one image.
            terminal
                .draw(|f| f.draw_image(area, &Canvas::filled(7, 7, Rgba::WHITE)))
                .unwrap();
            assert_eq!(sequences(&mut terminal).len(), 1);
        }

        #[test]
        fn iterm2_images_at_cell_resolution_send_patches() {
            let mut terminal = with_mode(ITERM2, 12, 6);
            let area = Rect::new(0, 0, 4, 2);
            let mut picture = exact(4, 2, Rgba::rgb(10, 20, 30));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            assert!(sequences(&mut terminal)[0].contains(";width=4;height=2;"));
            picture.pixel(35, 5, Rgba::rgb(250, 0, 0));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1);
            assert!(sent[0].contains(";width=1;height=1;"), "{}", &sent[0][..60]);
        }

        #[test]
        fn tall_sixel_pixels_halve_the_rows() {
            let mut terminal = with_mode(SIXEL, 12, 6);
            terminal.set_sixel_tall_pixels(true);
            assert!(terminal.sixel_tall_pixels());
            let area = Rect::new(1, 1, 6, 3);
            let mut picture = exact(6, 3, Rgba::rgb(10, 20, 30));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            let sent = sequences(&mut terminal);
            // 60 × 30 pixels sent, drawn 60 × 60 on the screen.
            assert!(sent[0].contains("\"2;1;60;60"), "{}", &sent[0][..30]);
            picture.pixel(25, 30, Rgba::rgb(250, 0, 0));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1);
            assert!(sent[0].contains("\"2;1;10;20"), "one cell");
        }

        #[test]
        fn changed_images_wait_while_the_terminal_catches_up() {
            let mut terminal = with_mode(SIXEL, 12, 6);
            let area = Rect::new(0, 0, 6, 3);
            let mut picture = exact(6, 3, Rgba::rgb(10, 20, 30));
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            sequences(&mut terminal);
            // As after a slow write: two frames without image updates.
            terminal.deferred_frames = 2;
            picture.pixel(5, 5, Rgba::rgb(250, 0, 0));
            for _ in 0..2 {
                terminal
                    .draw(|f| {
                        f.draw_image(area, &picture);
                        f.buffer_mut().set_string(0, 5, "text", Style::new());
                    })
                    .unwrap();
                assert_eq!(sequences(&mut terminal), Vec::<String>::new());
                // Text is still drawn at once.
                assert_eq!(&terminal.backend().screen_row(5)[..4], "text");
            }
            // Then the change is sent.
            terminal.draw(|f| f.draw_image(area, &picture)).unwrap();
            assert_eq!(sequences(&mut terminal).len(), 1);
        }

        #[test]
        fn detection_of_tall_pixels_and_pacing() {
            let env = |pairs: &'static [(&'static str, &'static str)]| {
                move |name: &str| {
                    pairs
                        .iter()
                        .find(|(k, _)| *k == name)
                        .map(|(_, v)| (*v).to_owned())
                }
            };
            assert!(sixel_tall_pixels(env(&[("WT_SESSION", "x")])));
            assert!(!sixel_tall_pixels(env(&[])));
            assert!(!sixel_tall_pixels(env(&[
                ("WT_SESSION", "x"),
                ("VNCE_SIXEL_ASPECT", "1")
            ])));
            assert!(sixel_tall_pixels(env(&[("VNCE_SIXEL_ASPECT", "2")])));
            assert_eq!(
                scale_quality(env(&[("VNCE_SCALE_QUALITY", "exact")])),
                Some(ScaleQuality::Exact)
            );
            assert_eq!(scale_quality(env(&[("VNCE_SCALE_QUALITY", "slow")])), None);
            assert_eq!(scale_quality(env(&[])), None);
            assert_eq!(frames_to_defer(Duration::from_millis(10)), 0);
            assert_eq!(frames_to_defer(Duration::from_millis(30)), 1);
            assert_eq!(frames_to_defer(Duration::from_millis(40)), 2);
            assert_eq!(frames_to_defer(Duration::from_secs(1)), 3);
        }

        #[test]
        fn kitty_inside_tmux_uses_passthrough_and_placeholders() {
            let mut terminal = with_mode(KITTY, 10, 5);
            terminal.set_tmux_passthrough(true);
            assert!(terminal.tmux_passthrough());
            let area = Rect::new(1, 1, 3, 2);
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::rgb(255, 0, 0))))
                .unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 1, "{sent:?}");
            // Wrapped for tmux, with a virtual placement.
            assert!(
                sent[0].starts_with("\x1bPtmux;\x1b\x1b_Ga=T,U=1,f=100,i=1,c=3,r=2,q=2"),
                "{}",
                &sent[0][..40]
            );
            // The cells show the placeholders, the id in the foreground color.
            for (x, y, column, row) in [(1, 1, 0, 0), (3, 2, 2, 1)] {
                let cell = terminal.backend().screen().cell(x, y).unwrap().clone();
                let expected = protocol::kitty::placeholder(1, column, row).unwrap();
                assert_eq!(cell.symbol(), expected);
                assert_eq!(cell.fg, Color::Rgb(0, 0, 1));
            }

            // Unchanged: nothing is sent, the placeholders stay.
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::rgb(255, 0, 0))))
                .unwrap();
            assert_eq!(sequences(&mut terminal), Vec::<String>::new());

            // Removed: a wrapped delete, and the cells are blank again.
            terminal.draw(|_| {}).unwrap();
            assert_eq!(
                sequences(&mut terminal),
                ["\x1bPtmux;\x1b\x1b_Ga=d,d=I,i=1,q=2\x1b\x1b\\\x1b\\"]
            );
            assert_eq!(terminal.backend().screen_row(1), "          ");
        }

        #[test]
        fn the_last_image_in_an_area_wins() {
            let mut terminal = with_mode(KITTY, 10, 5);
            let area = Rect::new(0, 0, 2, 2);
            terminal
                .draw(|f| {
                    f.draw_image(area, &image(Rgba::rgb(255, 0, 0)));
                    f.draw_image(area, &image(Rgba::rgb(0, 0, 255)));
                })
                .unwrap();
            assert_eq!(sequences(&mut terminal).len(), 1);
        }

        #[test]
        fn clip_and_blend_mode_do_not_count_as_a_change() {
            let mut terminal = with_mode(KITTY, 10, 5);
            let area = Rect::new(0, 0, 2, 2);
            let red = image(Rgba::rgb(255, 0, 0));
            terminal.draw(|f| f.draw_image(area, &red)).unwrap();
            sequences(&mut terminal);
            let mut same = red.clone();
            same.set_blend_mode(vnce_raster::BlendMode::Add);
            same.set_clip(Some(vnce_raster::PixelRect::new(0, 0, 1, 1)));
            terminal.draw(|f| f.draw_image(area, &same)).unwrap();
            assert_eq!(sequences(&mut terminal), Vec::<String>::new());
        }

        #[test]
        fn a_change_in_the_last_pixel_is_sent() {
            let mut terminal = with_mode(KITTY, 10, 5);
            let area = Rect::new(0, 0, 2, 2);
            let red = image(Rgba::rgb(255, 0, 0));
            terminal.draw(|f| f.draw_image(area, &red)).unwrap();
            sequences(&mut terminal);
            let mut changed = red.clone();
            changed.pixel(3, 3, Rgba::rgb(0, 0, 255));
            terminal.draw(|f| f.draw_image(area, &changed)).unwrap();
            assert_ne!(sequences(&mut terminal), Vec::<String>::new());
            // A different size with the same pixel count is a change too.
            let wide = Canvas::filled(8, 2, Rgba::rgb(255, 0, 0));
            terminal.draw(|f| f.draw_image(area, &changed)).unwrap();
            sequences(&mut terminal);
            terminal.draw(|f| f.draw_image(area, &wide)).unwrap();
            assert_ne!(sequences(&mut terminal), Vec::<String>::new());
        }

        #[test]
        fn changed_images_reuse_the_memory_of_replaced_ones() {
            let mut terminal = with_mode(KITTY, 10, 5);
            let area = Rect::new(0, 0, 2, 2);
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::rgb(255, 0, 0))))
                .unwrap();
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::rgb(0, 0, 255))))
                .unwrap();
            // The red canvas was replaced and kept for reuse.
            assert_eq!(terminal.images.spare.len(), 1);
            let memory = terminal.images.spare[0].pixels().as_ptr();
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::rgb(0, 255, 0))))
                .unwrap();
            assert_eq!(terminal.images.placed[0].canvas.pixels().as_ptr(), memory);
            assert_eq!(
                terminal.images.placed[0].canvas,
                image(Rgba::rgb(0, 255, 0))
            );
            // Images that leave the screen are kept too, up to the limit.
            terminal
                .draw(|f| {
                    for x in 0..4 {
                        f.draw_image(Rect::new(x * 2, 3, 2, 2), &image(Rgba::WHITE));
                    }
                })
                .unwrap();
            terminal.draw(|_| {}).unwrap();
            assert_eq!(terminal.images.spare.len(), SPARE_CANVASES);
        }

        #[test]
        fn graphics_mode_is_detected_when_entering() {
            let mut terminal = terminal(10, 5);
            terminal.backend_mut().set_query_answer(b"\x1b[?62;4c");
            terminal.enter_mode(TerminalOptions::default()).unwrap();
            // Sixel is detected unless the environment overrides it.
            if std::env::var("VNCE_GRAPHICS").is_err()
                && std::env::var("TMUX").is_err()
                && !std::env::var("TERM")
                    .is_ok_and(|t| t.starts_with("screen") || t.starts_with("tmux"))
            {
                assert_eq!(terminal.graphics_mode(), SIXEL);
            }
        }
    }
}
