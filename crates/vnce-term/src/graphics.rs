//! How images are shown: a terminal graphics protocol or cell-based pixel modes.

use std::fmt;
use std::str::FromStr;

use vnce_raster::PixelMode;

use crate::capabilities::Capabilities;
use crate::query::TerminalInfo;

/// A terminal graphics protocol that shows real pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphicsProtocol {
    /// The Kitty graphics protocol (Kitty, `WezTerm`, Ghostty, Konsole, …).
    Kitty,
    /// Sixel graphics (Windows Terminal 1.22+, foot, `WezTerm`, xterm, mlterm, …).
    Sixel,
    /// The iTerm2 inline image protocol (iTerm2, `WezTerm`, …).
    Iterm2,
}

/// How [`Frame::draw_image`](crate::Frame::draw_image) shows images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphicsMode {
    /// Real pixels through a graphics protocol.
    Protocol(GraphicsProtocol),
    /// Characters and colors, with a pixel mode (works everywhere).
    Cells(PixelMode),
}

impl GraphicsMode {
    /// The best mode for the terminal described by `info` and `capabilities`:
    ///
    /// 1. `VNCE_GRAPHICS` overrides everything (`kitty`, `sixel`, `iterm2`, `cells` or a
    ///    pixel mode name such as `halfblock`).
    /// 2. Inside tmux or screen: cells (protocols need special wrapping there).
    /// 3. Kitty, then Sixel, then iTerm2, if supported.
    /// 4. Otherwise cells with [`PixelMode::detect`].
    #[must_use]
    pub fn detect(info: &TerminalInfo, capabilities: &Capabilities) -> Self {
        Self::from_env(|name| std::env::var(name).ok(), info, capabilities)
    }

    /// Like [`GraphicsMode::detect`], with a custom environment lookup (for tests).
    pub fn from_env(
        env: impl Fn(&str) -> Option<String>,
        info: &TerminalInfo,
        capabilities: &Capabilities,
    ) -> Self {
        let cells = Self::Cells(PixelMode::detect(capabilities.color_depth));
        if let Some(value) = env("VNCE_GRAPHICS") {
            if value.trim().eq_ignore_ascii_case("cells") {
                return cells;
            }
            if let Ok(mode) = value.parse() {
                return mode;
            }
        }
        Self::protocols(&env, info)
            .first()
            .map_or(cells, |&protocol| Self::Protocol(protocol))
    }

    /// Every mode expected to work on this terminal, best first: the supported protocols,
    /// then the available pixel modes ([`PixelMode::available`]).
    #[must_use]
    pub fn available(info: &TerminalInfo, capabilities: &Capabilities) -> Vec<Self> {
        let env = |name: &str| std::env::var(name).ok();
        let mut modes: Vec<Self> = Self::protocols(&env, info)
            .into_iter()
            .map(Self::Protocol)
            .collect();
        modes.extend(
            PixelMode::available(capabilities.color_depth)
                .into_iter()
                .map(Self::Cells),
        );
        modes
    }

    /// The protocols the terminal supports, best first.
    fn protocols(
        env: &dyn Fn(&str) -> Option<String>,
        info: &TerminalInfo,
    ) -> Vec<GraphicsProtocol> {
        let term = env("TERM").unwrap_or_default().to_ascii_lowercase();
        let multiplexer = env("TMUX").is_some()
            || term.starts_with("screen")
            || term.starts_with("tmux")
            || info.name_contains("tmux");
        if multiplexer {
            return Vec::new();
        }
        let program = env("TERM_PROGRAM").unwrap_or_default();
        let iterm2 = cfg!(feature = "png")
            && (["iTerm.app", "WezTerm"].contains(&program.as_str())
                || env("LC_TERMINAL").is_some_and(|t| t == "iTerm2")
                || info.name_contains("iterm2")
                || info.name_contains("wezterm"));
        let mut protocols = Vec::new();
        if info.kitty_graphics {
            protocols.push(GraphicsProtocol::Kitty);
        }
        if info.sixel {
            protocols.push(GraphicsProtocol::Sixel);
        }
        if iterm2 {
            protocols.push(GraphicsProtocol::Iterm2);
        }
        protocols
    }

    /// The name used by [`FromStr`] and `VNCE_GRAPHICS`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Protocol(GraphicsProtocol::Kitty) => "kitty",
            Self::Protocol(GraphicsProtocol::Sixel) => "sixel",
            Self::Protocol(GraphicsProtocol::Iterm2) => "iterm2",
            Self::Cells(mode) => mode.name(),
        }
    }

    /// Whether images draw real pixels (a protocol) rather than cells.
    #[must_use]
    pub fn is_protocol(self) -> bool {
        matches!(self, Self::Protocol(_))
    }
}

impl fmt::Display for GraphicsMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for GraphicsMode {
    type Err = vnce_raster::ParsePixelModeError;

    /// Parses `kitty`, `sixel`, `iterm2` or a pixel mode name (case-insensitive).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "kitty" => Ok(Self::Protocol(GraphicsProtocol::Kitty)),
            "sixel" => Ok(Self::Protocol(GraphicsProtocol::Sixel)),
            "iterm2" | "iterm" => Ok(Self::Protocol(GraphicsProtocol::Iterm2)),
            other => other.parse().map(Self::Cells),
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_core::ColorDepth;

    use super::*;

    fn detect(vars: &[(&str, &str)], info: &TerminalInfo) -> GraphicsMode {
        let capabilities = Capabilities {
            color_depth: ColorDepth::TrueColor,
            ..Capabilities::default()
        };
        GraphicsMode::from_env(
            |name| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            },
            info,
            &capabilities,
        )
    }

    #[test]
    fn protocol_priority() {
        let mut info = TerminalInfo {
            sixel: true,
            ..TerminalInfo::default()
        };
        assert_eq!(
            detect(&[], &info),
            GraphicsMode::Protocol(GraphicsProtocol::Sixel)
        );
        info.kitty_graphics = true;
        assert_eq!(
            detect(&[], &info),
            GraphicsMode::Protocol(GraphicsProtocol::Kitty)
        );
        assert_eq!(
            detect(&[("TERM_PROGRAM", "iTerm.app")], &TerminalInfo::default()),
            GraphicsMode::Protocol(GraphicsProtocol::Iterm2)
        );
        assert!(!detect(&[], &TerminalInfo::default()).is_protocol());
    }

    #[test]
    fn multiplexers_use_cells() {
        let info = TerminalInfo {
            sixel: true,
            kitty_graphics: true,
            ..TerminalInfo::default()
        };
        assert!(!detect(&[("TMUX", "/tmp/tmux-1000/default,1,0")], &info).is_protocol());
        assert!(!detect(&[("TERM", "screen-256color")], &info).is_protocol());
    }

    #[test]
    fn override_and_names() {
        let info = TerminalInfo {
            kitty_graphics: true,
            ..TerminalInfo::default()
        };
        assert_eq!(
            detect(&[("VNCE_GRAPHICS", "sixel")], &info),
            GraphicsMode::Protocol(GraphicsProtocol::Sixel)
        );
        assert!(!detect(&[("VNCE_GRAPHICS", "cells")], &info).is_protocol());
        assert_eq!(
            detect(&[("VNCE_GRAPHICS", "braille")], &info),
            GraphicsMode::Cells(PixelMode::Braille)
        );
        assert_eq!(
            detect(&[("VNCE_GRAPHICS", "nonsense")], &info),
            GraphicsMode::Protocol(GraphicsProtocol::Kitty)
        );
        for mode in [
            GraphicsMode::Protocol(GraphicsProtocol::Kitty),
            GraphicsMode::Protocol(GraphicsProtocol::Sixel),
            GraphicsMode::Protocol(GraphicsProtocol::Iterm2),
            GraphicsMode::Cells(PixelMode::Quadrant),
        ] {
            assert_eq!(mode.to_string().parse(), Ok(mode));
        }
    }
}
