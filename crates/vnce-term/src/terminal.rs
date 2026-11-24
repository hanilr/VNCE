//! The double-buffered terminal.

use std::io;

use vnce_core::{Buffer, Rect, Size};
use vnce_raster::{Canvas, PixelMode, RenderOptions, protocol};

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
    hash: u64,
    /// The image, or `None` when the identical image is already on the screen.
    canvas: Option<Canvas>,
}

/// An image shown on the screen with a graphics protocol.
#[derive(Debug)]
struct PlacedImage {
    area: Rect,
    hash: u64,
    /// Kitty image id.
    id: u32,
    /// Kept to send the image again (after a clear or when an overlapping image is
    /// removed).
    canvas: Canvas,
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
                let scaled = canvas.scaled(
                    u32::from(area.width) * columns,
                    u32::from(area.height) * rows,
                );
                scaled.render(self.buffer, area, mode, &RenderOptions::default());
            }
            GraphicsMode::Protocol(_) => {
                if area.is_empty() || canvas.width() == 0 || canvas.height() == 0 {
                    return;
                }
                let hash = image_hash(canvas);
                let on_screen = self
                    .placed
                    .iter()
                    .any(|placed| placed.area == area && placed.hash == hash);
                // A later image in the same area replaces an earlier one.
                self.images.retain(|request| request.area != area);
                self.images.push(ImageRequest {
                    area,
                    hash,
                    canvas: (!on_screen).then(|| canvas.clone()),
                });
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
}

/// Bookkeeping for images shown with a graphics protocol.
#[derive(Debug)]
struct ImageState {
    /// Images requested in the frame being drawn.
    requests: Vec<ImageRequest>,
    /// Images currently on the screen (graphics protocols only).
    placed: Vec<PlacedImage>,
    /// Whether Kitty images may be on the screen (they survive clearing the screen).
    kitty_shown: bool,
    next_id: u32,
    /// Keeps the Sixel palette from frame to frame (faster, no color flicker).
    sixel: protocol::sixel::Encoder,
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
            images: ImageState {
                requests: Vec::new(),
                placed: Vec::new(),
                kitty_shown: false,
                next_id: 1,
                sixel: protocol::sixel::Encoder::new(),
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
        let sixel_colors = self.info.sixel_colors.unwrap_or(256);
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

        let sends = self
            .images
            .plan(self.graphics, self.full_redraw, current, previous, out);

        // Where the terminal cursor is, if known.
        let mut cursor: Option<(u16, u16)> = None;
        for update in current.diff(previous) {
            if cursor != Some((update.x, update.y)) {
                ansi::move_to(out, update.x, update.y)?;
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
            for index in sends {
                let image = &self.images.placed[index];
                ansi::move_to(out, image.area.x, image.area.y)?;
                write_image(
                    graphics,
                    image,
                    cell_pixels,
                    sixel_colors,
                    &mut self.images.sixel,
                    out,
                );
                if graphics == GraphicsProtocol::Kitty {
                    self.images.kitty_shown = true;
                }
            }
        }

        ansi::reset_style(out)?;
        if capabilities.synchronized_output {
            ansi::end_synchronized_update(out)?;
        }
        self.backend.write_all(out)?;
        self.backend.flush()?;
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
    fn plan(
        &mut self,
        graphics: GraphicsMode,
        full_redraw: bool,
        current: &mut Buffer,
        previous: &mut Buffer,
        out: &mut Vec<u8>,
    ) -> Vec<usize> {
        let old = std::mem::take(&mut self.placed);
        let requests = std::mem::take(&mut self.requests);
        let GraphicsMode::Protocol(protocol) = graphics else {
            // Cell mode draws images into the buffer. Images of a previous protocol were
            // removed by the full redraw that switching modes causes.
            return Vec::new();
        };
        let kitty = protocol == GraphicsProtocol::Kitty;

        // Areas of old images that will not stay exactly as they are.
        let removed: Vec<Rect> = old
            .iter()
            .filter(|placed| {
                full_redraw
                    || !requests
                        .iter()
                        .any(|r| r.area == placed.area && r.hash == placed.hash)
            })
            .map(|placed| placed.area)
            .collect();

        let mut old: Vec<Option<PlacedImage>> = old.into_iter().map(Some).collect();
        let mut sends = Vec::new();
        for request in requests {
            let matching = old.iter().position(|placed| {
                placed
                    .as_ref()
                    .is_some_and(|p| p.area == request.area && p.hash == request.hash)
            });
            let previous_image = matching.and_then(|index| old[index].take());
            // Removing a Sixel / iTerm2 image redraws the text under it, which also erases
            // any image it overlapped; Kitty images are independent of the text.
            let overlaps_removed =
                !kitty && removed.iter().any(|area| area.intersects(request.area));
            let unchanged = previous_image.is_some() && !full_redraw && !overlaps_removed;
            let (id, canvas) = match (previous_image, request.canvas) {
                // Same hash: the same pixels are already known.
                (Some(placed), _) => (placed.id, placed.canvas),
                (None, Some(canvas)) => (self.allocate_id(), canvas),
                (None, None) => continue,
            };
            if unchanged {
                if !kitty {
                    copy_cells(previous, current, request.area);
                }
            } else {
                sends.push(self.placed.len());
            }
            self.placed.push(PlacedImage {
                area: request.area,
                hash: request.hash,
                id,
                canvas,
            });
        }

        // Images that are gone.
        if !full_redraw {
            for placed in old.into_iter().flatten() {
                if kitty {
                    protocol::kitty::delete(placed.id, out);
                } else {
                    invalidate_cells(previous, placed.area);
                }
            }
        }
        sends
    }

    fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        id
    }
}

/// Encodes one image for `graphics` at the cursor position.
fn write_image(
    graphics: GraphicsProtocol,
    image: &PlacedImage,
    cell_pixels: (u16, u16),
    sixel_colors: u16,
    sixel: &mut protocol::sixel::Encoder,
    out: &mut Vec<u8>,
) {
    let area = image.area;
    match graphics {
        GraphicsProtocol::Kitty => {
            let placement = protocol::kitty::Placement {
                id: image.id,
                columns: area.width,
                rows: area.height,
                z_index: 0,
            };
            protocol::kitty::transmit_and_place_with(
                &image.canvas,
                placement,
                protocol::kitty::Options::default(),
                out,
            );
        }
        GraphicsProtocol::Sixel => {
            // Sixel draws pixels 1:1, so the image is scaled to the exact size of its area.
            let width = u32::from(area.width) * u32::from(cell_pixels.0);
            let height = u32::from(area.height) * u32::from(cell_pixels.1);
            let options = protocol::sixel::SixelOptions {
                max_colors: sixel_colors.clamp(2, 256),
                dither: false,
            };
            sixel.encode(&image.canvas.scaled(width, height), options, out);
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

/// A 64-bit FNV-1a hash of the image size and pixels.
fn image_hash(canvas: &Canvas) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    };
    for byte in canvas
        .width()
        .to_le_bytes()
        .into_iter()
        .chain(canvas.height().to_le_bytes())
    {
        feed(byte);
    }
    for p in canvas.pixels() {
        for byte in [p.r, p.g, p.b, p.a] {
            feed(byte);
        }
    }
    hash
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
            "\x1b[?2026h\x1b[0m\x1b[1;3HX\x1b[1;6HY\x1b[0m\x1b[?2026l"
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
            terminal
                .draw(|f| f.draw_image(area, &image(Rgba::rgb(0, 0, 255))))
                .unwrap();
            let sent = sequences(&mut terminal);
            assert_eq!(sent.len(), 2, "{sent:?}");
            assert_eq!(sent[0], "\x1b_Ga=d,d=I,i=1,q=2\x1b\\");
            assert!(sent[1].contains(",i=2,"));

            // No image: deleted.
            terminal.draw(|_| {}).unwrap();
            assert_eq!(sequences(&mut terminal), ["\x1b_Ga=d,d=I,i=2,q=2\x1b\\"]);
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
