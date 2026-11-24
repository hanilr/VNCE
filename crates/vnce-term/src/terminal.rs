//! The double-buffered terminal.

use std::io;

use vnce_core::{Buffer, Rect, Size};

use crate::ansi::{self, StyleState};
use crate::backend::{Backend, CrosstermBackend};
use crate::capabilities::Capabilities;
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

/// A drawing surface for one frame, passed to the closure of [`Terminal::draw`].
#[derive(Debug)]
pub struct Frame<'a> {
    buffer: &'a mut Buffer,
    count: u64,
}

impl Frame<'_> {
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
        Ok(())
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
        let mut frame = Frame {
            buffer: &mut self.buffers[self.current],
            count: self.frame_count,
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
        let (first, second) = self.buffers.split_at_mut(1);
        let (current, previous) = if self.current == 0 {
            (&first[0], &mut second[0])
        } else {
            (&second[0], &mut first[0])
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
        }

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
}
