//! Asking the terminal what it supports.
//!
//! [`TerminalInfo::REQUEST`] bundles several queries into one write. Every terminal answers
//! the last one (DA1), so its answer marks the end: queries the terminal does not
//! understand simply get no answer.

use std::io;
use std::time::Duration;

use crate::backend::Backend;

/// How long to wait for the answers by default.
pub const QUERY_TIMEOUT: Duration = Duration::from_millis(300);

/// What the terminal reported about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent terminal feature"
)]
pub struct TerminalInfo {
    /// Whether the terminal answered at all (DA1 answer received).
    pub responded: bool,
    /// The raw XTVERSION answer, e.g. `kitty(0.35.2)` or `WezTerm 20240203-110809`.
    pub version_string: Option<String>,
    /// Terminal name from XTVERSION, e.g. `kitty`, `WezTerm`, `foot`.
    pub name: Option<String>,
    /// Terminal version from XTVERSION.
    pub version: Option<String>,
    /// Parameters of the DA1 answer (device attributes).
    pub device_attributes: Vec<u16>,
    /// Sixel graphics (DA1 attribute 4).
    pub sixel: bool,
    /// Number of Sixel color registers (XTSMGRAPHICS).
    pub sixel_colors: Option<u16>,
    /// Largest Sixel image in pixels, `(width, height)` (XTSMGRAPHICS).
    pub sixel_max_size: Option<(u32, u32)>,
    /// Kitty graphics protocol.
    pub kitty_graphics: bool,
    /// Kitty keyboard protocol (key release events on every platform).
    pub kitty_keyboard: bool,
    /// Size of one cell in pixels, `(width, height)`.
    pub cell_pixels: Option<(u16, u16)>,
    /// Size of the text area in pixels, `(width, height)`.
    pub window_pixels: Option<(u32, u32)>,
}

impl TerminalInfo {
    /// All queries, in one write. DA1 must stay last.
    pub const REQUEST: &'static [u8] = concat!(
        // XTVERSION: terminal name and version.
        "\x1b[>0q",
        // Kitty graphics: query support with a 1x1 RGB image that is not stored.
        "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\",
        // Kitty keyboard protocol flags.
        "\x1b[?u",
        // Cell size and text area size in pixels.
        "\x1b[16t",
        "\x1b[14t",
        // XTSMGRAPHICS: Sixel color registers and maximum geometry.
        "\x1b[?1;1;0S",
        "\x1b[?2;1;0S",
        // DA1: device attributes. Every terminal answers it.
        "\x1b[c",
    )
    .as_bytes();

    /// Sends [`TerminalInfo::REQUEST`] through `backend` and parses the answers.
    ///
    /// # Errors
    ///
    /// Returns an error if writing or reading fails.
    ///
    /// Inside tmux, the Kitty graphics query and a second DA1 are also sent to the
    /// terminal around tmux (passthrough), and the answers are read until both DA1 answers
    /// arrived (or the timeout, when passthrough is off).
    pub fn query<B: Backend + ?Sized>(backend: &mut B, timeout: Duration) -> io::Result<Self> {
        let answer = if crate::tmux::inside() {
            let request = Self::tmux_request();
            backend.query(&request, timeout, &|answer| {
                device_attribute_answers(answer) >= 2
            })?
        } else {
            backend.query(Self::REQUEST, timeout, &has_device_attributes)?
        };
        Ok(Self::parse(&answer))
    }

    /// [`TerminalInfo::REQUEST`] followed by the Kitty graphics query and DA1 wrapped for
    /// tmux passthrough.
    #[must_use]
    pub fn tmux_request() -> Vec<u8> {
        let mut request = Self::REQUEST.to_vec();
        crate::tmux::wrap(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\", &mut request);
        crate::tmux::wrap(b"\x1b[c", &mut request);
        request
    }

    /// Parses the terminal's answers. Unknown or malformed sequences are ignored.
    #[must_use]
    pub fn parse(answer: &[u8]) -> Self {
        let mut info = Self::default();
        for sequence in sequences(answer) {
            match sequence {
                Sequence::Csi {
                    private,
                    params,
                    final_byte,
                } => info.csi(private, &params, final_byte),
                Sequence::Dcs(body) => {
                    if let Some(text) = body.strip_prefix(b">|") {
                        info.set_version(&String::from_utf8_lossy(text));
                    }
                }
                Sequence::Apc(body) => {
                    if body.starts_with(b"Gi=31;") && body.ends_with(b"OK") {
                        info.kitty_graphics = true;
                    }
                }
            }
        }
        info
    }

    /// Whether the terminal name (from XTVERSION) contains `needle`, ignoring case.
    #[must_use]
    pub fn name_contains(&self, needle: &str) -> bool {
        self.name.as_deref().is_some_and(|name| {
            name.to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
        })
    }

    fn csi(&mut self, private: Option<u8>, params: &[u16], final_byte: u8) {
        match (private, final_byte, params) {
            // Only the first answer counts: inside tmux, the second one comes from the
            // terminal around it, which does not draw the Sixel images.
            (Some(b'?'), b'c', _) if !self.responded => {
                self.responded = true;
                self.device_attributes = params.to_vec();
                // The first parameter is the terminal class; attributes follow.
                self.sixel = params.iter().skip(1).any(|&p| p == 4);
            }
            (Some(b'?'), b'u', _) => self.kitty_keyboard = true,
            (Some(b'?'), b'S', [1, 0, colors, ..]) => self.sixel_colors = Some(*colors),
            (Some(b'?'), b'S', [2, 0, width, height, ..]) => {
                self.sixel_max_size = Some((u32::from(*width), u32::from(*height)));
            }
            (None, b't', [6, height, width, ..]) if *width > 0 && *height > 0 => {
                self.cell_pixels = Some((*width, *height));
            }
            (None, b't', [4, height, width, ..]) if *width > 0 && *height > 0 => {
                self.window_pixels = Some((u32::from(*width), u32::from(*height)));
            }
            _ => {}
        }
    }

    fn set_version(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.version_string = Some(text.to_owned());
        // Formats: `name(version)`, `name version`, or just `name`.
        let (name, version) = match text.find(['(', ' ']) {
            Some(index) => {
                let version = text[index + 1..].trim_end_matches(')').trim();
                (&text[..index], (!version.is_empty()).then_some(version))
            }
            None => (text, None),
        };
        self.name = Some(name.to_owned());
        self.version = version.map(str::to_owned);
    }
}

/// Returns `true` once the DA1 answer (`CSI ? … c`) has arrived.
/// Number of DA1 answers in `answer`.
fn device_attribute_answers(answer: &[u8]) -> usize {
    sequences(answer)
        .filter(|sequence| {
            matches!(
                sequence,
                Sequence::Csi {
                    private: Some(b'?'),
                    final_byte: b'c',
                    ..
                }
            )
        })
        .count()
}

pub(crate) fn has_device_attributes(answer: &[u8]) -> bool {
    sequences(answer).any(|sequence| {
        matches!(
            sequence,
            Sequence::Csi {
                private: Some(b'?'),
                final_byte: b'c',
                ..
            }
        )
    })
}

/// One control sequence of an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Sequence<'a> {
    /// `CSI [private] params final`.
    Csi {
        private: Option<u8>,
        params: Vec<u16>,
        final_byte: u8,
    },
    /// `DCS body ST`.
    Dcs(&'a [u8]),
    /// `APC body ST`.
    Apc(&'a [u8]),
}

/// Iterates over the complete control sequences in `data`; other bytes and incomplete
/// sequences are skipped.
fn sequences(data: &[u8]) -> impl Iterator<Item = Sequence<'_>> {
    let mut rest = data;
    std::iter::from_fn(move || {
        loop {
            let start = rest.iter().position(|&b| b == 0x1b)?;
            rest = &rest[start..];
            let (sequence, length) = match rest.get(1) {
                Some(b'[') => parse_csi(&rest[2..]).map(|(s, len)| (s, len + 2)),
                Some(&kind @ (b'P' | b'_' | b']')) => {
                    parse_string(&rest[2..]).map(|(body, len)| {
                        let sequence = body.and_then(|body| match kind {
                            b'P' => Some(Sequence::Dcs(body)),
                            b'_' => Some(Sequence::Apc(body)),
                            // OSC answers are not used.
                            _ => None,
                        });
                        (sequence, len + 2)
                    })
                }
                // Not a sequence we know: skip only the ESC, the next byte may start one.
                Some(_) => Some((None, 1)),
                None => None,
            }?;
            rest = &rest[length..];
            if sequence.is_some() {
                return sequence;
            }
        }
    })
}

/// Parses the part of a CSI sequence after `ESC [`. Returns the sequence and its length.
///
/// An `ESC` before the final byte cancels the sequence (as in xterm): the result is then
/// `None` with the length up to that `ESC`, where the next sequence starts. Returns `None`
/// when the sequence is incomplete.
fn parse_csi(data: &[u8]) -> Option<(Option<Sequence<'_>>, usize)> {
    let end = data
        .iter()
        .position(|&b| (0x40..=0x7e).contains(&b) || b == 0x1b)?;
    if data[end] == 0x1b {
        return Some((None, end));
    }
    let body = &data[..end];
    let (private, body) = match body.first() {
        Some(&p @ (b'?' | b'>' | b'<' | b'=')) => (Some(p), &body[1..]),
        _ => (None, body),
    };
    let params = body
        .split(|&b| b == b';')
        .map(|p| {
            std::str::from_utf8(p)
                .ok()
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(0)
        })
        .collect();
    let sequence = Sequence::Csi {
        private,
        params,
        final_byte: data[end],
    };
    Some((Some(sequence), end + 1))
}

/// Parses a string sequence body terminated by ST (`ESC \`) or BEL. Returns the body and
/// the total length including the terminator.
///
/// Any other `ESC` cancels the string (as in xterm): the body is then `None` and the
/// length stops before that `ESC`, where the next sequence starts. Returns `None` when the
/// string is incomplete.
fn parse_string(data: &[u8]) -> Option<(Option<&[u8]>, usize)> {
    for (index, &byte) in data.iter().enumerate() {
        match byte {
            0x07 => return Some((Some(&data[..index]), index + 1)),
            0x1b => {
                return match data.get(index + 1) {
                    Some(b'\\') => Some((Some(&data[..index]), index + 2)),
                    Some(_) => Some((None, index)),
                    None => None,
                };
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_tmux_the_outer_terminal_is_asked_too() {
        let request = TerminalInfo::tmux_request();
        assert!(request.starts_with(TerminalInfo::REQUEST));
        let tail = &request[TerminalInfo::REQUEST.len()..];
        assert!(tail.starts_with(b"\x1bPtmux;\x1b\x1b_Gi=31,"));
        assert!(tail.ends_with(b"\x1bPtmux;\x1b\x1b[c\x1b\\"));
        // tmux answers first (with Sixel), the terminal around it second (without).
        let answer = b"\x1b[?62;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c";
        assert_eq!(device_attribute_answers(answer), 2);
        let info = TerminalInfo::parse(answer);
        assert!(info.sixel, "only the first DA1 counts");
        assert!(info.kitty_graphics);
        assert_eq!(info.device_attributes, [62, 4]);
    }

    #[test]
    fn an_esc_inside_a_sequence_cancels_it() {
        // An unterminated APC and a broken CSI must not hide the answers after them.
        let answer = b"\x1b_Gi=31;\x1b[6;20;10t\x1b[12\x1b[?62;4c";
        let info = TerminalInfo::parse(answer);
        assert!(info.responded);
        assert!(info.sixel);
        assert_eq!(info.cell_pixels, Some((10, 20)));
        assert!(!info.kitty_graphics);
        // ST still ends a string, and an incomplete string at the end is ignored.
        let info = TerminalInfo::parse(b"\x1b_Gi=31;OK\x1b\\\x1b[?62c\x1b_Gi=31;");
        assert!(info.kitty_graphics && info.responded);
    }

    #[test]
    fn kitty_answers() {
        let answer = b"\x1bP>|kitty(0.35.2)\x1b\\\x1b_Gi=31;OK\x1b\\\x1b[?15u\x1b[6;20;10t\
                       \x1b[4;600;800t\x1b[?62;22;52c";
        let info = TerminalInfo::parse(answer);
        assert!(info.responded);
        assert_eq!(info.name.as_deref(), Some("kitty"));
        assert_eq!(info.version.as_deref(), Some("0.35.2"));
        assert!(info.kitty_graphics);
        assert!(info.kitty_keyboard);
        assert!(!info.sixel);
        assert_eq!(info.cell_pixels, Some((10, 20)));
        assert_eq!(info.window_pixels, Some((800, 600)));
        assert!(info.name_contains("KITTY"));
    }

    #[test]
    fn sixel_answers() {
        // Windows Terminal style: no XTVERSION, Sixel in DA1, XTSMGRAPHICS answers.
        let answer = b"\x1b[?1;0;256S\x1b[?2;0;1000;1000S\x1b[?61;4;6;7;14;21;22;23;24;28;32;42c";
        let info = TerminalInfo::parse(answer);
        assert!(info.responded && info.sixel);
        assert_eq!(info.sixel_colors, Some(256));
        assert_eq!(info.sixel_max_size, Some((1000, 1000)));
        assert_eq!(info.name, None);
        assert!(!info.kitty_graphics);
    }

    #[test]
    fn class_parameter_4_is_not_sixel() {
        // `CSI ? 4 ; … c` would be a terminal class, not an attribute.
        let info = TerminalInfo::parse(b"\x1b[?4;6c");
        assert!(!info.sixel);
    }

    #[test]
    fn version_formats() {
        let info = TerminalInfo::parse(b"\x1bP>|WezTerm 20240203-110809-5046fc22\x1b\\");
        assert_eq!(info.name.as_deref(), Some("WezTerm"));
        assert_eq!(info.version.as_deref(), Some("20240203-110809-5046fc22"));
        let info = TerminalInfo::parse(b"\x1bP>|tmux 3.4\x07");
        assert_eq!(info.name.as_deref(), Some("tmux"));
        let info = TerminalInfo::parse(b"\x1bP>|foot\x1b\\");
        assert_eq!((info.name.as_deref(), info.version), (Some("foot"), None));
    }

    #[test]
    fn noise_and_incomplete_sequences_are_ignored() {
        let info = TerminalInfo::parse(b"abc\x1b[?6\x1b_Gi=31;ENOTSUPPORTED\x1b\\\x1b\x1b[?1;2c");
        assert!(info.responded);
        assert!(!info.kitty_graphics);
        assert!(!TerminalInfo::parse(b"\x1b[?62;4").responded);
        assert_eq!(TerminalInfo::parse(b""), TerminalInfo::default());
        assert!(has_device_attributes(b"xx\x1b[?62;4c"));
        assert!(!has_device_attributes(b"\x1b[>0;1c"));
    }

    #[test]
    fn request_ends_with_device_attributes() {
        assert!(TerminalInfo::REQUEST.ends_with(b"\x1b[c"));
    }
}
