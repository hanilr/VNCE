//! Terminal capabilities.

use vnce_core::ColorDepth;

/// Features supported by the terminal, used to adapt the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Capabilities {
    /// Number of colors the terminal can display. Colors are downsampled to this depth.
    pub color_depth: ColorDepth,
    /// Whether synchronized output (`CSI ? 2026 h/l`) is used to avoid tearing.
    pub synchronized_output: bool,
    /// Whether key release events are reported
    /// ([`KeyEventKind::Release`](crate::input::KeyEventKind::Release)).
    ///
    /// Always `true` on Windows. On other platforms it becomes `true` after
    /// [`Terminal::enter_mode`](crate::Terminal::enter_mode) when
    /// [`TerminalOptions::keyboard_enhancement`](crate::TerminalOptions) is enabled and the
    /// terminal supports the kitty keyboard protocol.
    pub key_release_events: bool,
}

impl Default for Capabilities {
    /// Truecolor with synchronized output, without key release events.
    fn default() -> Self {
        Self {
            color_depth: ColorDepth::TrueColor,
            synchronized_output: true,
            key_release_events: false,
        }
    }
}

/// Values of `TERM_PROGRAM` set by terminals that support truecolor.
const TRUECOLOR_PROGRAMS: [&str; 7] = [
    "iTerm.app",
    "WezTerm",
    "vscode",
    "ghostty",
    "Hyper",
    "Tabby",
    "rio",
];

/// Parts of `TERM` that indicate truecolor support.
const TRUECOLOR_TERMS: [&str; 10] = [
    "truecolor",
    "24bit",
    "direct",
    "kitty",
    "alacritty",
    "wezterm",
    "foot",
    "ghostty",
    "contour",
    "rio",
];

impl Capabilities {
    /// Detects the capabilities of the current terminal from environment variables.
    ///
    /// Color depth, in order of priority:
    ///
    /// 1. `VNCE_COLOR_DEPTH` (`truecolor`, `256`, `16` or `none`) overrides everything.
    /// 2. A non-empty `NO_COLOR` disables colors (<https://no-color.org>).
    /// 3. `COLORTERM=truecolor` or `COLORTERM=24bit` means truecolor.
    /// 4. `TERM=dumb` means no colors.
    /// 5. Windows Terminal (`WT_SESSION`), known `TERM_PROGRAM` values and known `TERM`
    ///    values mean truecolor; a `TERM` containing `256color` means 256 colors.
    /// 6. Otherwise: truecolor on Windows (the console supports it since Windows 10),
    ///    16 colors elsewhere.
    ///
    /// Synchronized output is disabled for `TERM=dumb`, the Linux console and the classic
    /// Windows console, which do not support it.
    ///
    /// Key release events are reported as available on Windows only; on other platforms
    /// [`Terminal::enter_mode`](crate::Terminal::enter_mode) enables them when possible.
    #[must_use]
    pub fn detect() -> Self {
        Self::from_env(|name| std::env::var(name).ok(), cfg!(windows))
    }

    /// Detects capabilities from the given environment lookup function, as if running on
    /// Windows when `windows` is `true`. Used by [`Capabilities::detect`] and by tests.
    pub fn from_env(env: impl Fn(&str) -> Option<String>, windows: bool) -> Self {
        let var = |name: &str| env(name).filter(|value| !value.is_empty());
        let term = var("TERM").unwrap_or_default();
        let term_program = var("TERM_PROGRAM");
        let windows_terminal = var("WT_SESSION").is_some();

        let color_depth = if let Some(depth) = var("VNCE_COLOR_DEPTH").and_then(|v| parse_depth(&v)) {
            depth
        } else if var("NO_COLOR").is_some() {
            ColorDepth::NoColor
        } else if var("COLORTERM").is_some_and(|v| v == "truecolor" || v == "24bit") {
            ColorDepth::TrueColor
        } else if term == "dumb" {
            ColorDepth::NoColor
        } else if windows_terminal
            || term_program
                .as_deref()
                .is_some_and(|p| TRUECOLOR_PROGRAMS.contains(&p))
            || TRUECOLOR_TERMS.iter().any(|t| term.contains(t))
        {
            ColorDepth::TrueColor
        } else if term_program.as_deref() == Some("Apple_Terminal") || term.contains("256color") {
            ColorDepth::Indexed256
        } else if windows && term.is_empty() {
            ColorDepth::TrueColor
        } else {
            ColorDepth::Ansi16
        };

        let classic_windows_console =
            windows && !windows_terminal && term_program.is_none() && term.is_empty();
        let synchronized_output =
            !(term == "dumb" || term.starts_with("linux") || classic_windows_console);

        Self {
            color_depth,
            synchronized_output,
            // The Windows console API reports key releases. Elsewhere this is only known
            // after the keyboard protocol has been negotiated.
            key_release_events: windows,
        }
    }
}

/// Parses a `VNCE_COLOR_DEPTH` value.
fn parse_depth(value: &str) -> Option<ColorDepth> {
    match value.trim().to_ascii_lowercase().as_str() {
        "truecolor" | "24bit" | "24" => Some(ColorDepth::TrueColor),
        "256" | "256color" => Some(ColorDepth::Indexed256),
        "16" | "ansi" => Some(ColorDepth::Ansi16),
        "none" | "0" | "nocolor" => Some(ColorDepth::NoColor),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(vars: &[(&str, &str)], windows: bool) -> Capabilities {
        Capabilities::from_env(
            |name| {
                vars.iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            },
            windows,
        )
    }

    fn depth(vars: &[(&str, &str)]) -> ColorDepth {
        detect(vars, false).color_depth
    }

    #[test]
    fn color_depth_detection() {
        assert_eq!(depth(&[("COLORTERM", "truecolor")]), ColorDepth::TrueColor);
        assert_eq!(depth(&[("TERM", "xterm-256color")]), ColorDepth::Indexed256);
        assert_eq!(depth(&[("TERM", "xterm-kitty")]), ColorDepth::TrueColor);
        assert_eq!(depth(&[("TERM", "xterm")]), ColorDepth::Ansi16);
        assert_eq!(depth(&[("TERM", "dumb")]), ColorDepth::NoColor);
        assert_eq!(depth(&[]), ColorDepth::Ansi16);
        assert_eq!(
            depth(&[("TERM", "xterm-256color"), ("TERM_PROGRAM", "iTerm.app")]),
            ColorDepth::TrueColor
        );
        assert_eq!(
            depth(&[("TERM_PROGRAM", "Apple_Terminal")]),
            ColorDepth::Indexed256
        );
    }

    #[test]
    fn overrides() {
        assert_eq!(
            depth(&[("COLORTERM", "truecolor"), ("NO_COLOR", "1")]),
            ColorDepth::NoColor
        );
        assert_eq!(
            depth(&[("NO_COLOR", ""), ("COLORTERM", "truecolor")]),
            ColorDepth::TrueColor
        );
        assert_eq!(
            depth(&[("NO_COLOR", "1"), ("VNCE_COLOR_DEPTH", "256")]),
            ColorDepth::Indexed256
        );
        assert_eq!(
            depth(&[("VNCE_COLOR_DEPTH", "nonsense"), ("TERM", "xterm")]),
            ColorDepth::Ansi16
        );
    }

    #[test]
    fn windows_detection() {
        let conhost = detect(&[], true);
        assert_eq!(conhost.color_depth, ColorDepth::TrueColor);
        assert!(!conhost.synchronized_output);

        let windows_terminal = detect(&[("WT_SESSION", "abc")], true);
        assert_eq!(windows_terminal.color_depth, ColorDepth::TrueColor);
        assert!(windows_terminal.synchronized_output);
        assert!(windows_terminal.key_release_events);
    }

    #[test]
    fn key_releases_are_unknown_before_entering_on_unix() {
        assert!(!detect(&[("TERM", "xterm-kitty")], false).key_release_events);
        assert!(detect(&[], true).key_release_events);
    }

    #[test]
    fn synchronized_output_detection() {
        assert!(detect(&[("TERM", "xterm-256color")], false).synchronized_output);
        assert!(!detect(&[("TERM", "linux")], false).synchronized_output);
        assert!(!detect(&[("TERM", "dumb")], false).synchronized_output);
    }
}
