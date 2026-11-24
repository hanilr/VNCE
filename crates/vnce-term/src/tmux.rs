//! Running inside tmux: passthrough of escape sequences to the outer terminal.
//!
//! tmux does not understand the Kitty graphics protocol. Wrapped in a passthrough sequence
//! (`ESC P tmux; … ESC \`, with every `ESC` doubled), commands reach the terminal around
//! tmux unchanged. It needs `set -g allow-passthrough on` in the tmux configuration.

/// Returns `true` when running inside tmux (`TMUX` is set).
#[must_use]
pub fn inside() -> bool {
    std::env::var_os("TMUX").is_some()
}

/// Appends `sequence` wrapped for tmux passthrough.
pub fn wrap(sequence: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(b"\x1bPtmux;");
    for &byte in sequence {
        if byte == 0x1b {
            out.push(0x1b);
        }
        out.push(byte);
    }
    out.extend_from_slice(b"\x1b\\");
}

/// Appends every escape sequence of `data` wrapped on its own (tmux limits the size of one
/// passthrough sequence, so long Kitty transfers stay in their chunks). Sequences end
/// with `ESC \`; anything after the last one is wrapped as it is.
pub fn wrap_each(data: &[u8], out: &mut Vec<u8>) {
    let mut rest = data;
    while !rest.is_empty() {
        let end = rest
            .windows(2)
            .position(|w| w == b"\x1b\\")
            .map_or(rest.len(), |at| at + 2);
        wrap(&rest[..end], out);
        rest = &rest[end..];
    }
}

/// Wraps every Kitty graphics command (`ESC _ G … ESC \`) in `data` for passthrough and
/// leaves everything else as it is.
#[must_use]
pub fn wrap_kitty(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 16);
    let mut rest = data;
    while let Some(start) = rest.windows(3).position(|w| w == b"\x1b_G") {
        out.extend_from_slice(&rest[..start]);
        let command = &rest[start..];
        let end = command
            .windows(2)
            .position(|w| w == b"\x1b\\")
            .map_or(command.len(), |at| at + 2);
        wrap(&command[..end], &mut out);
        rest = &command[end..];
    }
    out.extend_from_slice(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_kitty_commands_are_wrapped() {
        let data = b"\x1b[2;3Htext\x1b_Ga=d,i=1\x1b\\\x1b[0m\x1b_Gm=0;AA\x1b\\";
        let wrapped = wrap_kitty(data);
        assert_eq!(
            wrapped,
            [
                &b"\x1b[2;3Htext"[..],
                b"\x1bPtmux;\x1b\x1b_Ga=d,i=1\x1b\x1b\\\x1b\\",
                b"\x1b[0m",
                b"\x1bPtmux;\x1b\x1b_Gm=0;AA\x1b\x1b\\\x1b\\",
            ]
            .concat()
        );
        assert_eq!(wrap_kitty(b"plain \x1b[1m"), b"plain \x1b[1m");
    }

    #[test]
    fn escapes_are_doubled() {
        let mut out = Vec::new();
        wrap(b"\x1b_Ga=d\x1b\\", &mut out);
        assert_eq!(out, b"\x1bPtmux;\x1b\x1b_Ga=d\x1b\x1b\\\x1b\\");
    }

    #[test]
    fn every_sequence_is_wrapped_on_its_own() {
        let mut out = Vec::new();
        wrap_each(b"\x1b_Gm=1;AA\x1b\\\x1b_Gm=0;BB\x1b\\", &mut out);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("\x1bPtmux;").count(), 2);
        assert!(
            text.ends_with("\x1b\x1b_Gm=0;BB\x1b\x1b\\\x1b\\"),
            "{text:?}"
        );
        let mut empty = Vec::new();
        wrap_each(b"", &mut empty);
        assert_eq!(empty.len(), 0);
    }
}
