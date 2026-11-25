//! Terminal colors.
//!
//! A [`Color`] can be one of the 16 standard ANSI colors, an entry of the 256-color
//! palette, or a 24-bit truecolor value. Colors can be downsampled to whatever the
//! terminal supports with [`Color::downsample`].

use std::fmt;
use std::str::FromStr;

/// How many colors a terminal can display.
///
/// Variants are ordered from the least to the most capable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum ColorDepth {
    /// No color support; only text modifiers are used.
    NoColor,
    /// The 16 standard ANSI colors.
    Ansi16,
    /// The 256-color palette.
    Indexed256,
    /// 24-bit RGB colors.
    #[default]
    TrueColor,
}

/// A terminal color.
///
/// The 16 named variants map to the standard ANSI colors. Their exact appearance depends
/// on the terminal's theme. Note that ANSI `White` is usually rendered as light gray;
/// `BrightWhite` is the actual white.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// The terminal's default color.
    #[default]
    Reset,
    /// ANSI color 0.
    Black,
    /// ANSI color 1.
    Red,
    /// ANSI color 2.
    Green,
    /// ANSI color 3.
    Yellow,
    /// ANSI color 4.
    Blue,
    /// ANSI color 5.
    Magenta,
    /// ANSI color 6.
    Cyan,
    /// ANSI color 7 (usually light gray).
    White,
    /// ANSI color 8 (usually dark gray).
    BrightBlack,
    /// ANSI color 9.
    BrightRed,
    /// ANSI color 10.
    BrightGreen,
    /// ANSI color 11.
    BrightYellow,
    /// ANSI color 12.
    BrightBlue,
    /// ANSI color 13.
    BrightMagenta,
    /// ANSI color 14.
    BrightCyan,
    /// ANSI color 15.
    BrightWhite,
    /// An entry of the 256-color palette.
    Indexed(u8),
    /// A 24-bit RGB color.
    Rgb(u8, u8, u8),
}

/// The 16 named colors in ANSI index order.
const ANSI_COLORS: [Color; 16] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::White,
    Color::BrightBlack,
    Color::BrightRed,
    Color::BrightGreen,
    Color::BrightYellow,
    Color::BrightBlue,
    Color::BrightMagenta,
    Color::BrightCyan,
    Color::BrightWhite,
];

/// Approximate RGB values of the 16 ANSI colors (xterm defaults).
///
/// Real terminals use their own themes, so these are only used for color conversion.
const ANSI_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 0, 0),
    (0, 205, 0),
    (205, 205, 0),
    (0, 0, 238),
    (205, 0, 205),
    (0, 205, 205),
    (229, 229, 229),
    (127, 127, 127),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (92, 92, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

/// Channel levels of the 6x6x6 color cube (palette entries 16..=231).
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

impl Color {
    /// Creates a 24-bit RGB color.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb(r, g, b)
    }

    /// Parses a hexadecimal color: `#rgb` or `#rrggbb` (the `#` is optional).
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid hexadecimal color.
    pub fn from_hex(hex: &str) -> Result<Self, ParseColorError> {
        let digits = hex.strip_prefix('#').unwrap_or(hex);
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseColorError::new(hex));
        }
        let channel = |range: std::ops::Range<usize>| {
            u8::from_str_radix(&digits[range], 16).map_err(|_| ParseColorError::new(hex))
        };
        match digits.len() {
            3 => {
                // Each digit is repeated: `#abc` means `#aabbcc`.
                let r = channel(0..1)? * 0x11;
                let g = channel(1..2)? * 0x11;
                let b = channel(2..3)? * 0x11;
                Ok(Self::Rgb(r, g, b))
            }
            6 => Ok(Self::Rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?)),
            _ => Err(ParseColorError::new(hex)),
        }
    }

    /// The ANSI index (0..=15) of a named color, or `None` for other colors.
    #[must_use]
    pub fn ansi_index(self) -> Option<u8> {
        ANSI_COLORS
            .iter()
            .position(|&color| color == self)
            .and_then(|index| u8::try_from(index).ok())
    }

    /// Approximate RGB value of the color, or `None` for [`Color::Reset`].
    ///
    /// Named colors and palette entries 0..=15 use the xterm default palette.
    #[must_use]
    pub fn to_rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            Self::Reset => None,
            Self::Rgb(r, g, b) => Some((r, g, b)),
            Self::Indexed(index) => Some(indexed_to_rgb(index)),
            named => named.ansi_index().map(|index| ANSI_RGB[usize::from(index)]),
        }
    }

    /// Converts the color to the closest color that a terminal with the given depth can
    /// display.
    ///
    /// - With [`ColorDepth::TrueColor`] the color is unchanged.
    /// - With [`ColorDepth::Indexed256`] RGB colors become the nearest palette entry.
    /// - With [`ColorDepth::Ansi16`] RGB and palette colors become the nearest named color.
    /// - With [`ColorDepth::NoColor`] the result is always [`Color::Reset`].
    #[must_use]
    pub fn downsample(self, depth: ColorDepth) -> Self {
        match depth {
            ColorDepth::TrueColor => self,
            ColorDepth::Indexed256 => match self {
                Self::Rgb(r, g, b) => Self::Indexed(nearest_indexed(r, g, b)),
                other => other,
            },
            ColorDepth::Ansi16 => match self {
                Self::Rgb(r, g, b) => nearest_ansi(r, g, b),
                Self::Indexed(index) if index < 16 => ANSI_COLORS[usize::from(index)],
                Self::Indexed(index) => {
                    let (r, g, b) = indexed_to_rgb(index);
                    nearest_ansi(r, g, b)
                }
                other => other,
            },
            ColorDepth::NoColor => Self::Reset,
        }
    }
}

/// RGB value of a 256-color palette entry.
fn indexed_to_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => ANSI_RGB[usize::from(index)],
        16..=231 => {
            let i = index - 16;
            (
                CUBE_LEVELS[usize::from(i / 36)],
                CUBE_LEVELS[usize::from((i / 6) % 6)],
                CUBE_LEVELS[usize::from(i % 6)],
            )
        }
        232..=255 => {
            let level = 8 + (index - 232) * 10;
            (level, level, level)
        }
    }
}

/// Index of the nearest cube level for one channel.
fn nearest_cube_level(value: u8) -> u8 {
    NEAREST_CUBE_LEVEL[usize::from(value)]
}

/// [`nearest_cube_level`] for every channel value, computed at compile time (it runs for
/// every RGB color of every frame on 256-color terminals).
const NEAREST_CUBE_LEVEL: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut value = 0;
    while value < 256 {
        let mut best = 0;
        let mut best_distance = u8::MAX;
        let mut index = 0;
        while index < CUBE_LEVELS.len() {
            // `value < 256` and `index < 6`: both fit in a `u8`.
            #[allow(clippy::cast_possible_truncation, reason = "value < 256, index < 6")]
            let distance = (value as u8).abs_diff(CUBE_LEVELS[index]);
            if distance < best_distance {
                #[allow(clippy::cast_possible_truncation, reason = "index < 6")]
                let level = index as u8;
                best = level;
                best_distance = distance;
            }
            index += 1;
        }
        table[value] = best;
        value += 1;
    }
    table
};

/// Nearest entry of the 256-color palette, ignoring entries 0..=15 (their real colors
/// depend on the terminal theme).
fn nearest_indexed(r: u8, g: u8, b: u8) -> u8 {
    // Candidate from the 6x6x6 color cube.
    let (ri, gi, bi) = (
        nearest_cube_level(r),
        nearest_cube_level(g),
        nearest_cube_level(b),
    );
    let cube_index = 16 + 36 * ri + 6 * gi + bi;
    let cube_rgb = indexed_to_rgb(cube_index);

    // Candidate from the 24-step grayscale ramp (8, 18, ..., 238).
    let average = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
    let gray_step = (average.saturating_sub(3) / 10).min(23);
    let gray_index = 232 + u8::try_from(gray_step).unwrap_or(23);
    let gray_rgb = indexed_to_rgb(gray_index);

    if distance((r, g, b), gray_rgb) < distance((r, g, b), cube_rgb) {
        gray_index
    } else {
        cube_index
    }
}

/// Nearest of the 16 named colors.
fn nearest_ansi(r: u8, g: u8, b: u8) -> Color {
    let mut best = 0;
    let mut best_distance = u32::MAX;
    for (index, &rgb) in ANSI_RGB.iter().enumerate() {
        let d = distance((r, g, b), rgb);
        if d < best_distance {
            best = index;
            best_distance = d;
        }
    }
    ANSI_COLORS[best]
}

/// Perceptually weighted squared distance between two colors ("redmean" approximation).
fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let mean_red = u32::midpoint(u32::from(a.0), u32::from(b.0));
    let dr = u32::from(a.0.abs_diff(b.0));
    let dg = u32::from(a.1.abs_diff(b.1));
    let db = u32::from(a.2.abs_diff(b.2));
    (((512 + mean_red) * dr * dr) >> 8) + 4 * dg * dg + (((767 - mean_red) * db * db) >> 8)
}

impl FromStr for Color {
    type Err = ParseColorError;

    /// Parses a color from:
    ///
    /// - a hexadecimal value: `#rgb` or `#rrggbb`,
    /// - a color name (case-insensitive; `-`, `_` and spaces are ignored):
    ///   `reset`, `black`, `red`, ..., `bright-red`, ..., `bright-white`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.starts_with('#') {
            return Self::from_hex(trimmed);
        }
        let name: String = trimmed
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' '))
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let color = match name.as_str() {
            "reset" | "default" => Self::Reset,
            "black" => Self::Black,
            "red" => Self::Red,
            "green" => Self::Green,
            "yellow" => Self::Yellow,
            "blue" => Self::Blue,
            "magenta" => Self::Magenta,
            "cyan" => Self::Cyan,
            "white" => Self::White,
            "brightblack" => Self::BrightBlack,
            "brightred" => Self::BrightRed,
            "brightgreen" => Self::BrightGreen,
            "brightyellow" => Self::BrightYellow,
            "brightblue" => Self::BrightBlue,
            "brightmagenta" => Self::BrightMagenta,
            "brightcyan" => Self::BrightCyan,
            "brightwhite" => Self::BrightWhite,
            _ => return Self::from_hex(trimmed),
        };
        Ok(color)
    }
}

/// Error returned when a string cannot be parsed as a [`Color`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseColorError {
    input: String,
}

impl ParseColorError {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_owned(),
        }
    }

    /// The string that failed to parse.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }
}

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid color `{}` (expected `#rgb`, `#rrggbb` or a color name)",
            self.input
        )
    }
}

impl std::error::Error for ParseColorError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_cube_level_table_matches_the_levels() {
        for value in 0..=255u8 {
            let best = CUBE_LEVELS
                .iter()
                .enumerate()
                .min_by_key(|(_, level)| value.abs_diff(**level))
                .map(|(index, _)| index);
            assert_eq!(
                Some(usize::from(nearest_cube_level(value))),
                best,
                "{value}"
            );
        }
    }

    #[test]
    fn parse_hex() {
        assert_eq!(Color::from_hex("#e93737"), Ok(Color::Rgb(0xe9, 0x37, 0x37)));
        assert_eq!(Color::from_hex("e93737"), Ok(Color::Rgb(0xe9, 0x37, 0x37)));
        assert_eq!(Color::from_hex("#abc"), Ok(Color::Rgb(0xaa, 0xbb, 0xcc)));
        assert!(Color::from_hex("#abcd").is_err());
        assert!(Color::from_hex("#gggggg").is_err());
        assert!(Color::from_hex("").is_err());
        // Multi-byte characters must not cause a panic when slicing.
        assert!(Color::from_hex("#ééé").is_err());
    }

    #[test]
    fn parse_names() {
        assert_eq!("red".parse(), Ok(Color::Red));
        assert_eq!("Bright-Red".parse(), Ok(Color::BrightRed));
        assert_eq!("bright_white".parse(), Ok(Color::BrightWhite));
        assert_eq!(" reset ".parse(), Ok(Color::Reset));
        assert_eq!("#fff".parse(), Ok(Color::Rgb(255, 255, 255)));
        assert!("not-a-color".parse::<Color>().is_err());
    }

    #[test]
    fn ansi_index() {
        assert_eq!(Color::Black.ansi_index(), Some(0));
        assert_eq!(Color::BrightWhite.ansi_index(), Some(15));
        assert_eq!(Color::Rgb(1, 2, 3).ansi_index(), None);
        assert_eq!(Color::Reset.ansi_index(), None);
    }

    #[test]
    fn indexed_palette_rgb() {
        assert_eq!(Color::Indexed(16).to_rgb(), Some((0, 0, 0)));
        assert_eq!(Color::Indexed(231).to_rgb(), Some((255, 255, 255)));
        assert_eq!(Color::Indexed(196).to_rgb(), Some((255, 0, 0)));
        assert_eq!(Color::Indexed(232).to_rgb(), Some((8, 8, 8)));
        assert_eq!(Color::Indexed(255).to_rgb(), Some((238, 238, 238)));
    }

    #[test]
    fn downsample_to_256() {
        let depth = ColorDepth::Indexed256;
        assert_eq!(Color::Rgb(255, 0, 0).downsample(depth), Color::Indexed(196));
        assert_eq!(Color::Rgb(0, 0, 0).downsample(depth), Color::Indexed(16));
        assert_eq!(
            Color::Rgb(128, 128, 128).downsample(depth),
            Color::Indexed(244)
        );
        assert_eq!(Color::Red.downsample(depth), Color::Red);
        assert_eq!(Color::Indexed(42).downsample(depth), Color::Indexed(42));
    }

    #[test]
    fn downsample_to_16() {
        let depth = ColorDepth::Ansi16;
        assert_eq!(Color::Rgb(250, 10, 10).downsample(depth), Color::BrightRed);
        assert_eq!(Color::Rgb(0, 0, 0).downsample(depth), Color::Black);
        assert_eq!(Color::Indexed(9).downsample(depth), Color::BrightRed);
        assert_eq!(Color::Indexed(231).downsample(depth), Color::BrightWhite);
        assert_eq!(Color::Reset.downsample(depth), Color::Reset);
    }

    #[test]
    fn downsample_to_no_color() {
        assert_eq!(Color::Red.downsample(ColorDepth::NoColor), Color::Reset);
        assert_eq!(
            Color::Rgb(1, 2, 3).downsample(ColorDepth::TrueColor),
            Color::Rgb(1, 2, 3)
        );
    }

    #[test]
    fn color_depth_ordering() {
        assert!(ColorDepth::TrueColor > ColorDepth::Indexed256);
        assert!(ColorDepth::Indexed256 > ColorDepth::Ansi16);
        assert!(ColorDepth::Ansi16 > ColorDepth::NoColor);
    }
}
