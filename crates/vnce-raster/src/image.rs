//! Loading images into a [`Canvas`] (PNG, behind the `png` feature).

use std::fmt;
use std::io::{self, Cursor};
use std::path::Path;

use crate::canvas::Canvas;
use crate::color::Rgba;

/// An error while loading an image.
#[derive(Debug)]
pub enum ImageError {
    /// The file could not be read.
    Io(io::Error),
    /// The data is not a valid or supported image.
    Decode(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cannot read image: {error}"),
            Self::Decode(message) => write!(f, "cannot decode image: {message}"),
        }
    }
}

impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Decode(_) => None,
        }
    }
}

impl From<io::Error> for ImageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<png::DecodingError> for ImageError {
    fn from(error: png::DecodingError) -> Self {
        match error {
            png::DecodingError::IoError(error) => Self::Io(error),
            other => Self::Decode(other.to_string()),
        }
    }
}

impl Canvas {
    /// Decodes a PNG image.
    ///
    /// Every PNG color type and bit depth is supported (palette, grayscale, RGB, with or
    /// without alpha or a transparency chunk, 1 to 16 bits); the result is 8-bit RGBA.
    /// Animated PNGs load their first frame.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is not a valid PNG image or is too large for the
    /// decoder's default memory limit.
    pub fn from_png(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info()?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| ImageError::Decode("image is too large".into()))?;
        let mut data = vec![0; size];
        let info = reader.next_frame(&mut data)?;
        let data = &data[..info.buffer_size()];

        let pixels: Vec<Rgba> = match info.color_type {
            png::ColorType::Rgba => data
                .chunks_exact(4)
                .map(|p| Rgba::new(p[0], p[1], p[2], p[3]))
                .collect(),
            png::ColorType::Rgb => data
                .chunks_exact(3)
                .map(|p| Rgba::rgb(p[0], p[1], p[2]))
                .collect(),
            png::ColorType::GrayscaleAlpha => data
                .chunks_exact(2)
                .map(|p| Rgba::new(p[0], p[0], p[0], p[1]))
                .collect(),
            png::ColorType::Grayscale => data.iter().map(|&v| Rgba::rgb(v, v, v)).collect(),
            png::ColorType::Indexed => {
                return Err(ImageError::Decode("palette was not expanded".into()));
            }
        };
        Self::from_pixels(info.width, info.height, pixels)
            .ok_or_else(|| ImageError::Decode("unexpected pixel data size".into()))
    }

    /// Reads and decodes a PNG file. See [`Canvas::from_png`].
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is not a valid PNG image.
    pub fn load_png(path: impl AsRef<Path>) -> Result<Self, ImageError> {
        let bytes = std::fs::read(path)?;
        Self::from_png(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a 2×1 PNG with the given format and raw data.
    fn encode(
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
        setup: impl FnOnce(&mut png::Encoder<'_, &mut Vec<u8>>),
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(color);
            encoder.set_depth(depth);
            setup(&mut encoder);
            let mut writer = encoder.write_header().expect("valid header");
            writer.write_image_data(data).expect("valid data");
        }
        bytes
    }

    fn pixels(bytes: &[u8]) -> Vec<Rgba> {
        Canvas::from_png(bytes).expect("decodes").pixels().to_vec()
    }

    #[test]
    fn decodes_every_color_type() {
        use png::{BitDepth::Eight, ColorType};

        let rgba = encode(ColorType::Rgba, Eight, &[1, 2, 3, 4, 5, 6, 7, 8], |_| {});
        assert_eq!(
            pixels(&rgba),
            [Rgba::new(1, 2, 3, 4), Rgba::new(5, 6, 7, 8)]
        );

        let rgb = encode(ColorType::Rgb, Eight, &[1, 2, 3, 4, 5, 6], |_| {});
        assert_eq!(pixels(&rgb), [Rgba::rgb(1, 2, 3), Rgba::rgb(4, 5, 6)]);

        let gray = encode(ColorType::Grayscale, Eight, &[10, 200], |_| {});
        assert_eq!(
            pixels(&gray),
            [Rgba::rgb(10, 10, 10), Rgba::rgb(200, 200, 200)]
        );

        let gray_alpha = encode(ColorType::GrayscaleAlpha, Eight, &[10, 0, 200, 255], |_| {});
        assert_eq!(
            pixels(&gray_alpha),
            [Rgba::new(10, 10, 10, 0), Rgba::rgb(200, 200, 200)]
        );
    }

    #[test]
    fn decodes_palettes_with_transparency() {
        let bytes = encode(
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            &[1, 0],
            |encoder| {
                encoder.set_palette(vec![255, 0, 0, 0, 255, 0]);
                encoder.set_trns(vec![0, 255]);
            },
        );
        assert_eq!(
            pixels(&bytes),
            [Rgba::rgb(0, 255, 0), Rgba::new(255, 0, 0, 0)]
        );
    }

    #[test]
    fn decodes_16_bit_and_1_bit_images() {
        let wide = encode(
            png::ColorType::Rgb,
            png::BitDepth::Sixteen,
            &[0xff, 0xff, 0x80, 0x00, 0x00, 0x00, 0, 0, 0, 0, 0xff, 0xff],
            |_| {},
        );
        assert_eq!(
            pixels(&wide),
            [Rgba::rgb(255, 128, 0), Rgba::rgb(0, 0, 255)]
        );

        let bits = encode(
            png::ColorType::Grayscale,
            png::BitDepth::One,
            &[0b1000_0000],
            |_| {},
        );
        assert_eq!(pixels(&bits), [Rgba::WHITE, Rgba::BLACK]);
    }

    #[test]
    fn invalid_data_is_an_error() {
        assert!(matches!(
            Canvas::from_png(b"not a png"),
            Err(ImageError::Decode(_) | ImageError::Io(_))
        ));
        let truncated = &encode(png::ColorType::Rgb, png::BitDepth::Eight, &[0; 6], |_| {})[..30];
        assert!(Canvas::from_png(truncated).is_err());
        assert!(matches!(
            Canvas::load_png("this/file/does/not/exist.png"),
            Err(ImageError::Io(_))
        ));
    }
}
