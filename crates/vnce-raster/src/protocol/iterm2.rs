//! The iTerm2 inline image protocol: a base64 PNG file in an OSC 1337 sequence.
//!
//! <https://iterm2.com/documentation-images.html>

use std::io::Write as _;

use crate::canvas::Canvas;
use crate::image::ImageError;
use crate::protocol::base64;

/// Appends `canvas` as an inline image covering `columns × rows` cells at the cursor
/// position. The terminal scales the image to that area.
///
/// # Errors
///
/// Returns an error if the canvas cannot be encoded as PNG.
pub fn encode(
    canvas: &Canvas,
    columns: u16,
    rows: u16,
    out: &mut Vec<u8>,
) -> Result<(), ImageError> {
    let png = canvas.to_png_fast()?;
    let _ = write!(
        out,
        "\x1b]1337;File=inline=1;size={};width={columns};height={rows};preserveAspectRatio=0:",
        png.len()
    );
    base64::encode_into(&png, out);
    out.push(0x07);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    #[test]
    fn inline_png() {
        let canvas = Canvas::from_art(
            &["ab", "ba"],
            &[('a', Rgba::rgb(1, 2, 3)), ('b', Rgba::new(4, 5, 6, 7))],
        );
        let mut out = Vec::new();
        encode(&canvas, 4, 2, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let header = "\x1b]1337;File=inline=1;size=";
        assert!(text.starts_with(header));
        assert!(text.ends_with('\u{7}'));
        let (params, payload) = text[header.len()..].split_once(':').unwrap();
        assert!(params.ends_with(";width=4;height=2;preserveAspectRatio=0"));
        let png = base64::decode(payload.trim_end_matches('\u{7}').as_bytes());
        assert_eq!(
            params.split(';').next().unwrap().parse::<usize>().unwrap(),
            png.len()
        );
        let decoded = Canvas::from_png(&png).unwrap();
        assert_eq!(decoded.pixels(), canvas.pixels());
    }
}
