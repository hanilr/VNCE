//! Escape sequence writer.
//!
//! Every function writes one control sequence to any [`io::Write`]. Coordinates are
//! 0-based (the terminal itself is 1-based; the conversion happens here).

use std::io::{self, Write};

use vnce_core::{Color, ColorDepth, Modifiers};

/// Control Sequence Introducer.
const CSI: &str = "\x1b[";

/// Moves the cursor to column `x`, row `y` (0-based).
///
/// # Errors
///
/// Returns any error from the writer.
pub fn move_to<W: Write>(out: &mut W, x: u16, y: u16) -> io::Result<()> {
    // Written by hand instead of with `write!`: this runs for most changed cells.
    let mut sequence = Sequence::new();
    sequence.push_bytes(CSI.as_bytes());
    sequence.push_number(u32::from(y) + 1);
    sequence.push_bytes(b";");
    sequence.push_number(u32::from(x) + 1);
    sequence.push_bytes(b"H");
    out.write_all(sequence.as_bytes())
}

/// Clears the whole screen. Cleared cells get the *current* background color, so reset
/// the style first to get the default background.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn clear_screen<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}2J")
}

/// Resets all colors and modifiers to the terminal defaults.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn reset_style<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}0m")
}

/// Hides the cursor.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn hide_cursor<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}?25l")
}

/// Shows the cursor.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn show_cursor<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}?25h")
}

/// Switches to the alternate screen.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn enter_alternate_screen<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}?1049h")
}

/// Switches back to the main screen.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn leave_alternate_screen<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}?1049l")
}

/// Starts a synchronized update: the terminal holds the screen until
/// [`end_synchronized_update`]. Terminals that do not support it ignore the sequence.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn begin_synchronized_update<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}?2026h")
}

/// Ends a synchronized update and shows the result.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn end_synchronized_update<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "{CSI}?2026l")
}

/// Writes a cell symbol. Control characters are never written: a symbol containing one is
/// replaced by a space. Returns `true` if the symbol was written unchanged.
///
/// # Errors
///
/// Returns any error from the writer.
pub fn write_symbol<W: Write>(out: &mut W, symbol: &str) -> io::Result<bool> {
    // ASCII symbols (the common case) are checked byte by byte; the ASCII control
    // characters are exactly the ASCII characters `char::is_control` matches.
    let control = if symbol.is_ascii() {
        symbol.bytes().any(|byte| byte.is_ascii_control())
    } else {
        symbol.chars().any(char::is_control)
    };
    if control {
        out.write_all(b" ")?;
        return Ok(false);
    }
    out.write_all(symbol.as_bytes())?;
    Ok(true)
}

/// The colors and modifiers the terminal is currently using.
///
/// [`StyleState::apply`] compares the wanted style with this state and only writes the
/// attributes that differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StyleState {
    /// Current foreground color.
    pub fg: Color,
    /// Current background color.
    pub bg: Color,
    /// Current modifiers.
    pub modifiers: Modifiers,
}

impl StyleState {
    /// The state right after [`reset_style`].
    pub const RESET: Self = Self {
        fg: Color::Reset,
        bg: Color::Reset,
        modifiers: Modifiers::NONE,
    };

    /// Writes one SGR sequence that changes the terminal from the current state to the
    /// given style, then updates the state. Colors are first downsampled to `depth`.
    /// Nothing is written if the style does not change.
    ///
    /// # Errors
    ///
    /// Returns any error from the writer.
    pub fn apply<W: Write>(
        &mut self,
        out: &mut W,
        fg: Color,
        bg: Color,
        modifiers: Modifiers,
        depth: ColorDepth,
    ) -> io::Result<()> {
        let fg = fg.downsample(depth);
        let bg = bg.downsample(depth);
        if (fg, bg, modifiers) == (self.fg, self.bg, self.modifiers) {
            return Ok(());
        }
        let mut sgr = SgrWriter::new(out);

        let mut removed = self.modifiers.difference(modifiers);
        let mut added = modifiers.difference(self.modifiers);
        // Bold and dim are both switched off by the same code (22). Re-enable the one
        // that must stay on.
        let intensity = Modifiers::BOLD | Modifiers::DIM;
        if removed.intersects(intensity) {
            sgr.param(22)?;
            removed.remove(intensity);
            added.insert(modifiers.intersection(intensity));
        }
        if !removed.is_empty() {
            for (modifier, code) in REMOVE_CODES {
                if removed.contains(modifier) {
                    sgr.param(code)?;
                }
            }
        }
        if !added.is_empty() {
            for (modifier, code) in ADD_CODES {
                if added.contains(modifier) {
                    sgr.param(code)?;
                }
            }
        }
        if fg != self.fg {
            write_color(&mut sgr, fg, Layer::Foreground)?;
        }
        if bg != self.bg {
            write_color(&mut sgr, bg, Layer::Background)?;
        }
        sgr.finish()?;

        *self = Self { fg, bg, modifiers };
        Ok(())
    }
}

/// SGR codes that switch a modifier on.
const ADD_CODES: [(Modifiers, u8); 8] = [
    (Modifiers::BOLD, 1),
    (Modifiers::DIM, 2),
    (Modifiers::ITALIC, 3),
    (Modifiers::UNDERLINE, 4),
    (Modifiers::BLINK, 5),
    (Modifiers::REVERSE, 7),
    (Modifiers::HIDDEN, 8),
    (Modifiers::STRIKETHROUGH, 9),
];

/// SGR codes that switch a modifier off (bold and dim are handled separately).
const REMOVE_CODES: [(Modifiers, u8); 6] = [
    (Modifiers::ITALIC, 23),
    (Modifiers::UNDERLINE, 24),
    (Modifiers::BLINK, 25),
    (Modifiers::REVERSE, 27),
    (Modifiers::HIDDEN, 28),
    (Modifiers::STRIKETHROUGH, 29),
];

#[derive(Clone, Copy)]
enum Layer {
    Foreground,
    Background,
}

fn write_color<W: Write>(sgr: &mut SgrWriter<'_, W>, color: Color, layer: Layer) -> io::Result<()> {
    let (default, normal, bright, extended) = match layer {
        Layer::Foreground => (39, 30, 90, 38),
        Layer::Background => (49, 40, 100, 48),
    };
    match color {
        Color::Reset => sgr.param(default),
        Color::Indexed(index) => {
            sgr.param(extended)?;
            sgr.param(5)?;
            sgr.param(index)
        }
        Color::Rgb(r, g, b) => {
            sgr.param(extended)?;
            sgr.param(2)?;
            sgr.param(r)?;
            sgr.param(g)?;
            sgr.param(b)
        }
        named => match named.ansi_index() {
            Some(index @ 0..=7) => sgr.param(normal + index),
            Some(index) => sgr.param(bright + (index - 8)),
            None => Ok(()),
        },
    }
}

/// Writes the parameters of one SGR sequence (`CSI p1;p2;... m`), separated by `;`.
///
/// The sequence is assembled on the stack and written with a single `write_all` in
/// [`SgrWriter::finish`].
struct SgrWriter<'a, W: Write> {
    out: &'a mut W,
    sequence: Sequence,
}

impl<'a, W: Write> SgrWriter<'a, W> {
    fn new(out: &'a mut W) -> Self {
        Self {
            out,
            sequence: Sequence::new(),
        }
    }

    // Kept fallible like the writer it used to write to directly, so the callers stay the
    // same.
    #[allow(clippy::unnecessary_wraps, reason = "keeps the callers unchanged")]
    fn param(&mut self, value: u8) -> io::Result<()> {
        if self.sequence.is_empty() {
            self.sequence.push_bytes(CSI.as_bytes());
        } else {
            self.sequence.push_bytes(b";");
        }
        self.sequence.push_number(u32::from(value));
        Ok(())
    }

    fn finish(mut self) -> io::Result<()> {
        if !self.sequence.is_empty() {
            self.sequence.push_bytes(b"m");
            self.out.write_all(self.sequence.as_bytes())?;
        }
        Ok(())
    }
}

/// A short escape sequence assembled on the stack, so it can be written without the
/// formatting machinery and with a single `write_all`.
struct Sequence {
    bytes: [u8; Self::CAPACITY],
    len: usize,
}

impl Sequence {
    /// Enough for the longest SGR sequence `StyleState::apply` writes (about 100 bytes:
    /// every modifier code and two RGB colors).
    const CAPACITY: usize = 128;

    fn new() -> Self {
        Self {
            bytes: [0; Self::CAPACITY],
            len: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Appends `bytes`; bytes that do not fit are dropped (never reached by the sequences
    /// of this module).
    fn push_bytes(&mut self, bytes: &[u8]) {
        let end = self.len + bytes.len();
        if let Some(target) = self.bytes.get_mut(self.len..end) {
            target.copy_from_slice(bytes);
            self.len = end;
        }
    }

    /// Appends `value` in decimal.
    fn push_number(&mut self, mut value: u32) {
        // Color channels and SGR codes (the vast majority) come from a table.
        if let Some((digits, len)) = usize::try_from(value).ok().and_then(|v| DECIMAL.get(v)) {
            self.push_bytes(&digits[..usize::from(*len)]);
            return;
        }
        let mut digits = [0u8; 10];
        let mut start = digits.len();
        loop {
            start -= 1;
            // `value % 10` is a single digit.
            digits[start] = b'0' + u8::try_from(value % 10).unwrap_or(0);
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.push_bytes(&digits[start..]);
    }
}

/// The decimal digits of 0..=255 and their count.
const DECIMAL: [([u8; 3], u8); 256] = {
    let mut table = [([0u8; 3], 0u8); 256];
    let mut value = 0;
    while value < 256 {
        // `value < 256`, so every digit fits in a `u8`.
        #[allow(clippy::cast_possible_truncation, reason = "value < 256")]
        let (hundreds, tens, ones) = (
            (value / 100) as u8,
            (value / 10 % 10) as u8,
            (value % 10) as u8,
        );
        table[value] = if value >= 100 {
            ([b'0' + hundreds, b'0' + tens, b'0' + ones], 3)
        } else if value >= 10 {
            ([b'0' + tens, b'0' + ones, 0], 2)
        } else {
            ([b'0' + ones, 0, 0], 1)
        };
        value += 1;
    }
    table
};

#[cfg(test)]
mod tests {
    use super::*;

    fn written(f: impl FnOnce(&mut Vec<u8>) -> io::Result<()>) -> String {
        let mut out = Vec::new();
        f(&mut out).expect("writing to a Vec never fails");
        String::from_utf8(out).expect("escape sequences are valid UTF-8")
    }

    fn sgr(state: &mut StyleState, fg: Color, bg: Color, modifiers: Modifiers) -> String {
        written(|out| state.apply(out, fg, bg, modifiers, ColorDepth::TrueColor))
    }

    #[test]
    fn simple_sequences() {
        assert_eq!(written(|o| move_to(o, 0, 0)), "\x1b[1;1H");
        assert_eq!(written(|o| move_to(o, 9, 4)), "\x1b[5;10H");
        assert_eq!(written(|o| move_to(o, u16::MAX, 0)), "\x1b[1;65536H");
        assert_eq!(written(clear_screen), "\x1b[2J");
        assert_eq!(written(reset_style), "\x1b[0m");
        assert_eq!(written(hide_cursor), "\x1b[?25l");
        assert_eq!(written(show_cursor), "\x1b[?25h");
        assert_eq!(written(enter_alternate_screen), "\x1b[?1049h");
        assert_eq!(written(leave_alternate_screen), "\x1b[?1049l");
        assert_eq!(written(begin_synchronized_update), "\x1b[?2026h");
        assert_eq!(written(end_synchronized_update), "\x1b[?2026l");
    }

    #[test]
    fn numbers_are_written_in_decimal() {
        for value in [0, 7, 10, 99, 100, 255, 65_536, u32::MAX] {
            let mut sequence = Sequence::new();
            sequence.push_number(value);
            assert_eq!(sequence.as_bytes(), value.to_string().as_bytes());
        }
    }

    #[test]
    fn longest_sgr_sequence_fits() {
        // Every modifier switched off then on, with two RGB colors of three-digit values.
        let mut state = StyleState {
            fg: Color::Reset,
            bg: Color::Reset,
            modifiers: Modifiers::ALL,
        };
        let out = sgr(
            &mut state,
            Color::Rgb(255, 255, 255),
            Color::Rgb(255, 255, 255),
            Modifiers::BOLD | Modifiers::DIM,
        );
        assert!(out.len() < Sequence::CAPACITY, "{}", out.len());
        assert!(out.ends_with("48;2;255;255;255m"), "{out:?}");
        let mut state = StyleState::RESET;
        let out = sgr(
            &mut state,
            Color::Rgb(255, 255, 255),
            Color::Rgb(255, 255, 255),
            Modifiers::ALL,
        );
        assert_eq!(
            out,
            "\x1b[1;2;3;4;5;7;8;9;38;2;255;255;255;48;2;255;255;255m"
        );
    }

    #[test]
    fn symbols_never_contain_control_characters() {
        let mut out = Vec::new();
        assert!(write_symbol(&mut out, "界").unwrap());
        assert!(!write_symbol(&mut out, "\x1b").unwrap());
        assert!(!write_symbol(&mut out, "\x7f").unwrap());
        assert!(!write_symbol(&mut out, "\u{9b}").unwrap());
        assert!(write_symbol(&mut out, "a").unwrap());
        assert_eq!(String::from_utf8(out).unwrap(), "界   a");
    }

    #[test]
    fn unchanged_style_writes_nothing() {
        let mut state = StyleState::RESET;
        assert_eq!(
            sgr(&mut state, Color::Reset, Color::Reset, Modifiers::NONE),
            ""
        );
    }

    #[test]
    fn colors() {
        let mut state = StyleState::RESET;
        assert_eq!(
            sgr(&mut state, Color::Red, Color::Reset, Modifiers::NONE),
            "\x1b[31m"
        );
        assert_eq!(
            sgr(&mut state, Color::BrightRed, Color::Blue, Modifiers::NONE),
            "\x1b[91;44m"
        );
        assert_eq!(
            sgr(
                &mut state,
                Color::Indexed(200),
                Color::BrightWhite,
                Modifiers::NONE
            ),
            "\x1b[38;5;200;107m"
        );
        assert_eq!(
            sgr(
                &mut state,
                Color::Rgb(1, 2, 3),
                Color::Rgb(4, 5, 6),
                Modifiers::NONE
            ),
            "\x1b[38;2;1;2;3;48;2;4;5;6m"
        );
        assert_eq!(
            sgr(&mut state, Color::Reset, Color::Reset, Modifiers::NONE),
            "\x1b[39;49m"
        );
    }

    #[test]
    fn modifiers_only_emit_differences() {
        let mut state = StyleState::RESET;
        let bold_italic = Modifiers::BOLD | Modifiers::ITALIC;
        assert_eq!(
            sgr(&mut state, Color::Reset, Color::Reset, bold_italic),
            "\x1b[1;3m"
        );
        assert_eq!(
            sgr(&mut state, Color::Reset, Color::Reset, Modifiers::BOLD),
            "\x1b[23m"
        );
        assert_eq!(
            sgr(
                &mut state,
                Color::Reset,
                Color::Reset,
                Modifiers::BOLD | Modifiers::UNDERLINE
            ),
            "\x1b[4m"
        );
    }

    #[test]
    fn removing_bold_keeps_dim() {
        let mut state = StyleState::RESET;
        let both = Modifiers::BOLD | Modifiers::DIM;
        assert_eq!(
            sgr(&mut state, Color::Reset, Color::Reset, both),
            "\x1b[1;2m"
        );
        assert_eq!(
            sgr(&mut state, Color::Reset, Color::Reset, Modifiers::DIM),
            "\x1b[22;2m"
        );
        assert_eq!(
            sgr(&mut state, Color::Reset, Color::Reset, Modifiers::NONE),
            "\x1b[22m"
        );
    }

    #[test]
    fn colors_are_downsampled() {
        let mut state = StyleState::RESET;
        let out = written(|o| {
            state.apply(
                o,
                Color::Rgb(255, 0, 0),
                Color::Reset,
                Modifiers::NONE,
                ColorDepth::Indexed256,
            )
        });
        assert_eq!(out, "\x1b[38;5;196m");
        assert_eq!(state.fg, Color::Indexed(196));

        let mut state = StyleState::RESET;
        let out = written(|o| {
            state.apply(
                o,
                Color::Rgb(255, 0, 0),
                Color::Blue,
                Modifiers::BOLD,
                ColorDepth::NoColor,
            )
        });
        assert_eq!(out, "\x1b[1m");
    }
}
