//! Backends: where the terminal output goes.
//!
//! - [`CrosstermBackend`] talks to the real terminal (stdout) on Windows, Linux and macOS.
//! - [`TestBackend`] keeps everything in memory, for tests.

use std::io::{self, Write};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use vnce_core::{Buffer, Color, Modifiers, Size, Style};

use crate::query::{QUERY_TIMEOUT, TerminalInfo};
use crate::terminal::TerminalOptions;

/// The destination of the terminal output.
pub trait Backend {
    /// The current size of the terminal in cells.
    ///
    /// # Errors
    ///
    /// Returns an error if the size cannot be queried.
    fn size(&self) -> io::Result<Size>;

    /// Writes raw bytes (text and escape sequences).
    ///
    /// # Errors
    ///
    /// Returns any I/O error.
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Flushes written bytes to the terminal.
    ///
    /// # Errors
    ///
    /// Returns any I/O error.
    fn flush(&mut self) -> io::Result<()>;

    /// Prepares the terminal for full-screen drawing (raw mode, alternate screen, ...).
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal cannot be configured.
    fn enter(&mut self, options: &TerminalOptions) -> io::Result<()>;

    /// Restores the terminal to its normal state.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal cannot be restored.
    fn leave(&mut self) -> io::Result<()>;

    /// Whether key release events are reported since the last [`Backend::enter`].
    ///
    /// The default implementation returns `false`.
    fn key_release_events(&self) -> bool {
        false
    }

    /// Sends `request` to the terminal and reads its answer until `done` returns `true` or
    /// `timeout` expires. Used for terminal queries ([`TerminalInfo`]).
    ///
    /// The default implementation sends nothing and returns an empty answer.
    ///
    /// # Errors
    ///
    /// Returns any I/O error.
    fn query(
        &mut self,
        request: &[u8],
        timeout: Duration,
        done: &dyn Fn(&[u8]) -> bool,
    ) -> io::Result<Vec<u8>> {
        let _ = (request, timeout, done);
        Ok(Vec::new())
    }

    /// What the terminal reported about itself during the last [`Backend::enter`] (see
    /// [`TerminalOptions::query_terminal`]).
    ///
    /// The default implementation returns an empty [`TerminalInfo`].
    fn terminal_info(&self) -> TerminalInfo {
        TerminalInfo::default()
    }
}

/// Set while a [`CrosstermBackend`] has the terminal in full-screen mode; used by the
/// panic hook to know whether the terminal must be restored.
static TERMINAL_ENTERED: AtomicBool = AtomicBool::new(false);

/// Set while the kitty keyboard protocol flags are pushed; used to pop them on restore.
static KEYBOARD_ENHANCED: AtomicBool = AtomicBool::new(false);

/// Installs the panic hook only once per process.
static PANIC_HOOK: Once = Once::new();

/// A backend that writes to the real terminal through stdout.
///
/// `crossterm` is only used for platform-specific work: raw mode, terminal size, input
/// modes and (on Windows) enabling virtual terminal processing. All drawing output is
/// produced by [`ansi`](crate::ansi).
#[derive(Debug)]
pub struct CrosstermBackend {
    stdout: io::Stdout,
    /// Whether the kitty keyboard protocol was enabled by [`Backend::enter`].
    keyboard_enhanced: bool,
    /// Answers to the terminal queries of the last [`Backend::enter`].
    info: TerminalInfo,
}

impl CrosstermBackend {
    /// Creates a backend that writes to stdout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            keyboard_enhanced: false,
            info: TerminalInfo::default(),
        }
    }
}

impl Default for CrosstermBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for CrosstermBackend {
    fn size(&self) -> io::Result<Size> {
        let (width, height) = crossterm::terminal::size()?;
        Ok(Size::new(width, height))
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.stdout.write_all(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }

    fn enter(&mut self, options: &TerminalOptions) -> io::Result<()> {
        install_panic_hook();
        crossterm::terminal::enable_raw_mode()?;
        TERMINAL_ENTERED.store(true, Ordering::SeqCst);
        if let Err(error) = enter_modes(&mut self.stdout, *options) {
            restore_terminal();
            return Err(error);
        }
        // Queries run in raw mode, before any input event is read. A failed query is not an
        // error: the terminal is simply treated as answering nothing.
        let supports_keyboard = if options.query_terminal {
            self.info = TerminalInfo::query(self, QUERY_TIMEOUT).unwrap_or_default();
            Some(self.info.kitty_keyboard)
        } else {
            self.info = TerminalInfo::default();
            None
        };
        self.keyboard_enhanced = options.keyboard_enhancement
            && enable_keyboard_enhancement(&mut self.stdout, supports_keyboard);
        Ok(())
    }

    fn leave(&mut self) -> io::Result<()> {
        restore_terminal();
        self.keyboard_enhanced = false;
        Ok(())
    }

    fn key_release_events(&self) -> bool {
        // The Windows console API always reports key releases.
        cfg!(windows) || self.keyboard_enhanced
    }

    fn query(
        &mut self,
        request: &[u8],
        timeout: Duration,
        done: &dyn Fn(&[u8]) -> bool,
    ) -> io::Result<Vec<u8>> {
        self.stdout.write_all(request)?;
        self.stdout.flush()?;
        crate::sys::read_answer(timeout, done)
    }

    fn terminal_info(&self) -> TerminalInfo {
        self.info.clone()
    }
}

/// Enables the requested terminal modes.
///
/// These go through `crossterm` because on Windows some of them (mouse capture, focus
/// events) are console-mode settings, not escape sequences. Executing a `crossterm`
/// command also enables virtual terminal processing on Windows, which our own escape
/// sequences rely on.
fn enter_modes(stdout: &mut io::Stdout, options: TerminalOptions) -> io::Result<()> {
    use crossterm::{cursor, event, execute, terminal};

    if options.alternate_screen {
        execute!(stdout, terminal::EnterAlternateScreen)?;
    }
    if options.hide_cursor {
        execute!(stdout, cursor::Hide)?;
    } else {
        execute!(stdout, cursor::Show)?;
    }
    if options.mouse_capture {
        execute!(stdout, event::EnableMouseCapture)?;
    }
    // Bracketed paste and focus events are optional: some consoles do not support them,
    // which is not an error for the application.
    if options.bracketed_paste {
        let _ = execute!(stdout, event::EnableBracketedPaste);
    }
    if options.focus_events {
        let _ = execute!(stdout, event::EnableFocusChange);
    }
    Ok(())
}

/// Enables the kitty keyboard protocol if the terminal supports it, so that key repeat and
/// release events are reported for every key. Returns `true` on success.
///
/// It is never enabled on Windows, where the console API already reports releases.
/// The flags are pushed after entering the alternate screen because terminals keep a
/// separate protocol stack per screen.
///
/// `supported` is the answer of our own terminal query; when no query was made (`None`),
/// `crossterm` checks support itself.
fn enable_keyboard_enhancement(stdout: &mut io::Stdout, supported: Option<bool>) -> bool {
    use crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
    use crossterm::execute;

    let supported = supported.unwrap_or_else(|| {
        !cfg!(windows) && crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false)
    });
    if cfg!(windows) || !supported {
        return false;
    }
    // Reporting all keys as escape codes is required to get release events for keys that
    // produce text (letters, digits, ...).
    let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
    if execute!(stdout, PushKeyboardEnhancementFlags(flags)).is_err() {
        return false;
    }
    KEYBOARD_ENHANCED.store(true, Ordering::SeqCst);
    true
}

/// Restores the terminal: disables every mode the engine may have enabled, leaves the
/// alternate screen, shows the cursor and disables raw mode. Errors are ignored because
/// this runs during cleanup (including from the panic hook).
fn restore_terminal() {
    use crossterm::{cursor, event, execute, terminal};

    if !TERMINAL_ENTERED.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut stdout = io::stdout();
    let _ = stdout.write_all(b"\x1b[0m");
    // Popped before leaving the alternate screen, where the flags were pushed.
    if KEYBOARD_ENHANCED.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout, event::PopKeyboardEnhancementFlags);
    }
    let _ = execute!(stdout, event::DisableFocusChange);
    let _ = execute!(stdout, event::DisableBracketedPaste);
    let _ = execute!(stdout, event::DisableMouseCapture);
    let _ = execute!(stdout, cursor::Show);
    let _ = execute!(stdout, terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
    let _ = stdout.flush();
}

/// Installs a panic hook that restores the terminal before the panic message is printed,
/// so that a crash never leaves the terminal in raw mode or on the alternate screen.
fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            previous(info);
        }));
    });
}

/// An in-memory backend for tests.
///
/// It records every byte written and keeps a [`Buffer`] that mirrors what a terminal would
/// show, by interpreting the escape sequences produced by [`Terminal`](crate::Terminal)
/// (cursor movement, screen clearing and SGR styles).
#[derive(Debug, Clone)]
pub struct TestBackend {
    size: Size,
    output: Vec<u8>,
    screen: Buffer,
    cursor: (u16, u16),
    style: (Color, Color, Modifiers),
    entered: bool,
    flushes: usize,
    key_release_events: bool,
    query_answer: Vec<u8>,
    queries: Vec<Vec<u8>>,
    info: TerminalInfo,
}

impl TestBackend {
    /// Creates a backend with the given terminal size.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let size = Size::new(width, height);
        Self {
            size,
            output: Vec::new(),
            screen: Buffer::empty(size),
            cursor: (0, 0),
            style: (Color::Reset, Color::Reset, Modifiers::NONE),
            entered: false,
            flushes: 0,
            key_release_events: false,
            query_answer: Vec::new(),
            queries: Vec::new(),
            info: TerminalInfo::default(),
        }
    }

    /// Sets the value returned by [`Backend::key_release_events`].
    pub fn set_key_release_events(&mut self, enabled: bool) {
        self.key_release_events = enabled;
    }

    /// Sets the answer returned to every [`Backend::query`], as a terminal would send it.
    pub fn set_query_answer(&mut self, answer: &[u8]) {
        self.query_answer = answer.to_vec();
    }

    /// Every query request received so far.
    #[must_use]
    pub fn queries(&self) -> &[Vec<u8>] {
        &self.queries
    }

    /// Changes the reported terminal size, like a window resize. The screen is cleared.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.size = Size::new(width, height);
        self.screen.resize(self.size);
    }

    /// Every byte written so far.
    #[must_use]
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Returns and clears the bytes written so far.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// What the terminal screen would show.
    #[must_use]
    pub fn screen(&self) -> &Buffer {
        &self.screen
    }

    /// The symbols of one screen row as a string (continuation cells are skipped).
    #[must_use]
    pub fn screen_row(&self, y: u16) -> String {
        (0..self.size.width)
            .filter_map(|x| self.screen.cell(x, y))
            .map(vnce_core::Cell::symbol)
            .collect()
    }

    /// Whether [`Backend::enter`] was called without a matching [`Backend::leave`].
    #[must_use]
    pub fn is_entered(&self) -> bool {
        self.entered
    }

    /// Number of calls to [`Backend::flush`].
    #[must_use]
    pub fn flush_count(&self) -> usize {
        self.flushes
    }

    /// Updates the emulated screen with newly written bytes.
    fn interpret(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let mut rest: &str = &text;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("\x1b[") {
                let end = after
                    .find(|c: char| ('\x40'..='\x7e').contains(&c))
                    .unwrap_or(after.len());
                let (params, final_and_rest) = after.split_at(end);
                let mut chars = final_and_rest.chars();
                let final_char = chars.next();
                self.control_sequence(params, final_char);
                rest = chars.as_str();
            } else {
                let end = rest.find('\x1b').unwrap_or(rest.len());
                let (printable, remaining) = rest.split_at(end);
                self.print(printable);
                rest = remaining;
            }
        }
    }

    fn control_sequence(&mut self, params: &str, final_char: Option<char>) {
        if params.starts_with('?') {
            // Private modes (cursor visibility, alternate screen, synchronized output)
            // do not change the screen contents.
            return;
        }
        let numbers: Vec<u16> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
        match final_char {
            Some('H') => {
                let row = numbers.first().copied().unwrap_or(1).max(1);
                let column = numbers.get(1).copied().unwrap_or(1).max(1);
                self.cursor = (column - 1, row - 1);
            }
            Some('J') if numbers.first() == Some(&2) => {
                let mut blank = vnce_core::Cell::EMPTY;
                blank.bg = self.style.1;
                self.screen = Buffer::filled(self.size, &blank);
            }
            Some('m') => self.select_graphic_rendition(&numbers),
            _ => {}
        }
    }

    fn select_graphic_rendition(&mut self, numbers: &[u16]) {
        let mut iter = numbers.iter().copied();
        while let Some(code) = iter.next() {
            let (fg, bg, modifiers) = &mut self.style;
            match code {
                0 => {
                    *fg = Color::Reset;
                    *bg = Color::Reset;
                    *modifiers = Modifiers::NONE;
                }
                1 => modifiers.insert(Modifiers::BOLD),
                2 => modifiers.insert(Modifiers::DIM),
                3 => modifiers.insert(Modifiers::ITALIC),
                4 => modifiers.insert(Modifiers::UNDERLINE),
                5 => modifiers.insert(Modifiers::BLINK),
                7 => modifiers.insert(Modifiers::REVERSE),
                8 => modifiers.insert(Modifiers::HIDDEN),
                9 => modifiers.insert(Modifiers::STRIKETHROUGH),
                22 => modifiers.remove(Modifiers::BOLD | Modifiers::DIM),
                23 => modifiers.remove(Modifiers::ITALIC),
                24 => modifiers.remove(Modifiers::UNDERLINE),
                25 => modifiers.remove(Modifiers::BLINK),
                27 => modifiers.remove(Modifiers::REVERSE),
                28 => modifiers.remove(Modifiers::HIDDEN),
                29 => modifiers.remove(Modifiers::STRIKETHROUGH),
                30..=37 => *fg = ansi_color(code - 30),
                90..=97 => *fg = ansi_color(code - 90 + 8),
                39 => *fg = Color::Reset,
                40..=47 => *bg = ansi_color(code - 40),
                100..=107 => *bg = ansi_color(code - 100 + 8),
                49 => *bg = Color::Reset,
                38 | 48 => {
                    let color = match iter.next() {
                        Some(5) => iter.next().map(|i| Color::Indexed(low_byte(i))),
                        Some(2) => {
                            let r = iter.next().map_or(0, low_byte);
                            let g = iter.next().map_or(0, low_byte);
                            let b = iter.next().map_or(0, low_byte);
                            Some(Color::Rgb(r, g, b))
                        }
                        _ => None,
                    };
                    if let Some(color) = color {
                        if code == 38 {
                            *fg = color;
                        } else {
                            *bg = color;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn print(&mut self, text: &str) {
        let (fg, bg, modifiers) = self.style;
        let style = Style {
            fg: Some(fg),
            bg: Some(bg),
            add_modifiers: modifiers,
            sub_modifiers: Modifiers::ALL.difference(modifiers),
        };
        for grapheme in text.graphemes(true) {
            let (x, y) = self.cursor;
            self.screen.set_symbol(x, y, grapheme, style);
            let width = u16::try_from(grapheme.width()).unwrap_or(1);
            self.cursor.0 = x.saturating_add(width);
        }
    }
}

fn ansi_color(index: u16) -> Color {
    Color::Indexed(low_byte(index)).downsample(vnce_core::ColorDepth::Ansi16)
}

fn low_byte(value: u16) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

impl Backend for TestBackend {
    fn size(&self) -> io::Result<Size> {
        Ok(self.size)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.output.extend_from_slice(bytes);
        self.interpret(bytes);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }

    fn enter(&mut self, options: &TerminalOptions) -> io::Result<()> {
        self.entered = true;
        self.info = if options.query_terminal {
            TerminalInfo::query(self, QUERY_TIMEOUT)?
        } else {
            TerminalInfo::default()
        };
        Ok(())
    }

    fn leave(&mut self) -> io::Result<()> {
        self.entered = false;
        Ok(())
    }

    fn key_release_events(&self) -> bool {
        self.key_release_events
    }

    fn query(
        &mut self,
        request: &[u8],
        _timeout: Duration,
        _done: &dyn Fn(&[u8]) -> bool,
    ) -> io::Result<Vec<u8>> {
        // Queries are recorded separately so they never appear on the emulated screen.
        self.queries.push(request.to_vec());
        Ok(self.query_answer.clone())
    }

    fn terminal_info(&self) -> TerminalInfo {
        self.info.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_interprets_output() {
        let mut backend = TestBackend::new(6, 2);
        backend
            .write_all(b"\x1b[2;2H\x1b[31;1mab\x1b[0m\x1b[1;5H\xe7\x95\x8c")
            .unwrap();
        assert_eq!(backend.screen_row(0), "    界");
        assert_eq!(backend.screen_row(1), " ab   ");
        let cell = backend.screen().cell(1, 1).unwrap();
        assert_eq!(cell.fg, Color::Red);
        assert_eq!(cell.modifiers, Modifiers::BOLD);
        assert_eq!(backend.screen().cell(4, 0).unwrap().fg, Color::Reset);
    }

    #[test]
    fn test_backend_extended_colors_and_clear() {
        let mut backend = TestBackend::new(2, 1);
        backend
            .write_all(b"\x1b[38;5;200;48;2;1;2;3mx\x1b[0;44m\x1b[2J")
            .unwrap();
        let cell = backend.screen().cell(0, 0).unwrap();
        assert_eq!(cell.symbol(), " ");
        assert_eq!(cell.bg, Color::Blue);

        backend
            .write_all(b"\x1b[38;5;200;48;2;1;2;3m\x1b[1;1Hx")
            .unwrap();
        let cell = backend.screen().cell(0, 0).unwrap();
        assert_eq!(cell.fg, Color::Indexed(200));
        assert_eq!(cell.bg, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn test_backend_resize_and_flags() {
        let mut backend = TestBackend::new(2, 2);
        assert!(!backend.is_entered());
        backend.enter(&TerminalOptions::default()).unwrap();
        assert!(backend.is_entered());
        backend.leave().unwrap();
        assert!(!backend.is_entered());

        backend.resize(5, 3);
        assert_eq!(backend.size().unwrap(), Size::new(5, 3));
        assert_eq!(backend.screen().size(), Size::new(5, 3));

        backend.write_all(b"x").unwrap();
        backend.flush().unwrap();
        assert_eq!(backend.take_output(), b"x");
        assert_eq!(backend.output(), b"");
        assert_eq!(backend.flush_count(), 1);
    }
}
