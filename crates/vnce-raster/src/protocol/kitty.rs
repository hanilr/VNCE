//! The Kitty graphics protocol.
//!
//! Images are sent as raw RGBA, base64 encoded, in chunks of at most 4096 bytes. Each image
//! has an id, so it can be replaced or deleted later. With [`Options`], opaque images are
//! sent as RGB and compressed (PNG with the `png` feature, else zlib), which usually makes
//! them many times smaller. Responses from the terminal are
//! suppressed (`q=2`) so they never show up as input.
//!
//! <https://sw.kovidgoyal.net/kitty/graphics-protocol/>

use std::io::Write as _;

use crate::canvas::Canvas;
use crate::protocol::base64;

/// Largest base64 payload per escape sequence (a multiple of 4, as the protocol requires).
const CHUNK: usize = 4096;

/// How to place a transmitted image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Placement {
    /// Image id (1 or more). Sending an image with an existing id replaces it.
    pub id: u32,
    /// Width of the placement in cells; the terminal scales the image to fit.
    pub columns: u16,
    /// Height of the placement in cells.
    pub rows: u16,
    /// Stacking order: negative values are drawn below text, others above.
    pub z_index: i32,
}

/// How the pixel data is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compression {
    /// Raw pixels.
    None,
    /// Raw pixels compressed with zlib (`o=z`).
    Zlib,
    /// A PNG file (`f=100`): row filters make gradients much smaller than plain zlib,
    /// and the fast encoder is quicker too.
    #[cfg(feature = "png")]
    Png,
}

/// How [`transmit_and_place_with`] encodes the pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Options {
    /// Send RGB instead of RGBA when every pixel is opaque (a quarter less data).
    pub rgb_when_opaque: bool,
    /// How the pixels are compressed.
    pub compression: Compression,
}

impl Default for Options {
    /// RGB when opaque; PNG with the `png` feature, zlib otherwise.
    fn default() -> Self {
        Self {
            rgb_when_opaque: true,
            #[cfg(feature = "png")]
            compression: Compression::Png,
            #[cfg(not(feature = "png"))]
            compression: Compression::Zlib,
        }
    }
}

impl Options {
    /// Raw RGBA without compression (the simplest form).
    pub const RAW: Self = Self {
        rgb_when_opaque: false,
        compression: Compression::None,
    };
}

/// Fastest zlib level: the data is mostly flat colors and gradients, which compress well
/// even at this level.
const COMPRESSION_LEVEL: u8 = 1;

/// Appends the sequences that transmit `canvas` and display it at the cursor position,
/// without moving the cursor.
pub fn transmit_and_place(canvas: &Canvas, placement: Placement, out: &mut Vec<u8>) {
    transmit_and_place_with(canvas, placement, Options::RAW, out);
}

/// Like [`transmit_and_place`], encoding the pixels as `options` say.
pub fn transmit_and_place_with(
    canvas: &Canvas,
    placement: Placement,
    options: Options,
    out: &mut Vec<u8>,
) {
    let (keys, data) = encode(canvas, options);
    let payload = base64::encode(&data);
    let chunks: Vec<&[u8]> = if payload.is_empty() {
        vec![&[]]
    } else {
        payload.chunks(CHUNK).collect()
    };
    let last = chunks.len() - 1;
    for (index, chunk) in chunks.into_iter().enumerate() {
        let more = u8::from(index < last);
        out.extend_from_slice(b"\x1b_G");
        if index == 0 {
            let _ = write!(
                out,
                "a=T,{keys},i={},c={},r={},C=1,q=2,z={},m={more}",
                placement.id, placement.columns, placement.rows, placement.z_index,
            );
        } else {
            let _ = write!(out, "m={more}");
        }
        out.push(b';');
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
}

/// The format keys (`f`, `o`, `s`, `v`) and the bytes to send.
fn encode(canvas: &Canvas, options: Options) -> (String, Vec<u8>) {
    let opaque = options.rgb_when_opaque && canvas.pixels().iter().all(|p| p.a == 255);
    let size = format!("s={},v={}", canvas.width(), canvas.height());
    #[cfg(feature = "png")]
    if options.compression == Compression::Png {
        // PNG carries its own size; an image that cannot be encoded is sent raw.
        if let Ok(png) = canvas.to_png_fast() {
            return ("f=100".to_owned(), png);
        }
    }
    let mut raw = Vec::with_capacity(canvas.pixels().len() * if opaque { 3 } else { 4 });
    for p in canvas.pixels() {
        if opaque {
            raw.extend_from_slice(&[p.r, p.g, p.b]);
        } else {
            raw.extend_from_slice(&[p.r, p.g, p.b, p.a]);
        }
    }
    let format = if opaque { 24 } else { 32 };
    if options.compression == Compression::Zlib {
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, COMPRESSION_LEVEL);
        (format!("f={format},o=z,{size}"), compressed)
    } else {
        (format!("f={format},{size}"), raw)
    }
}

/// Appends the sequence that deletes the image `id` and frees its data.
pub fn delete(id: u32, out: &mut Vec<u8>) {
    let _ = write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
}

/// Appends the sequence that deletes every image and frees their data.
pub fn delete_all(out: &mut Vec<u8>) {
    out.extend_from_slice(b"\x1b_Ga=d,d=A,q=2\x1b\\");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    fn placement() -> Placement {
        Placement {
            id: 7,
            columns: 3,
            rows: 2,
            z_index: -1,
        }
    }

    #[test]
    fn small_image_in_one_chunk() {
        let canvas = Canvas::filled(1, 1, Rgba::new(1, 2, 3, 4));
        let mut out = Vec::new();
        transmit_and_place(&canvas, placement(), &mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b_Ga=T,f=32,s=1,v=1,i=7,c=3,r=2,C=1,q=2,z=-1,m=0;AQIDBA==\x1b\\"
        );
    }

    #[test]
    fn large_images_are_chunked() {
        let canvas = Canvas::filled(40, 40, Rgba::WHITE);
        let mut out = Vec::new();
        transmit_and_place(&canvas, placement(), &mut out);
        let text = String::from_utf8(out).unwrap();
        let sequences: Vec<&str> = text.split("\x1b\\").filter(|s| !s.is_empty()).collect();
        // 6400 raw bytes -> 8536 base64 bytes -> 3 chunks.
        assert_eq!(sequences.len(), 3);
        assert!(sequences[0].contains(",m=1;"));
        assert!(sequences[1].starts_with("\x1b_Gm=1;"));
        assert!(sequences[2].starts_with("\x1b_Gm=0;"));
        let payload: String = sequences
            .iter()
            .map(|s| s.split_once(';').unwrap().1)
            .collect();
        assert_eq!(base64::decode(payload.as_bytes()), vec![255; 6400]);
        for sequence in &sequences {
            assert!(sequence.split_once(';').unwrap().1.len() <= CHUNK);
        }
    }

    /// The control data and the decoded (and inflated) payload of transmitted images.
    fn decode(out: &[u8]) -> (String, Vec<u8>) {
        let text = String::from_utf8(out.to_vec()).unwrap();
        let mut control = String::new();
        let mut payload = String::new();
        for sequence in text.split("\x1b\\").filter(|s| !s.is_empty()) {
            let (keys, data) = sequence.split_once(';').unwrap();
            if control.is_empty() {
                control = keys.to_owned();
            }
            payload.push_str(data);
        }
        let bytes = base64::decode(payload.as_bytes());
        if control.contains("o=z") {
            (
                control,
                miniz_oxide::inflate::decompress_to_vec_zlib(&bytes).unwrap(),
            )
        } else {
            (control, bytes)
        }
    }

    fn options(compression: Compression) -> Options {
        Options {
            compression,
            ..Options::default()
        }
    }

    #[test]
    fn opaque_images_are_sent_as_compressed_rgb() {
        let mut canvas = Canvas::filled(50, 30, Rgba::rgb(10, 20, 30));
        canvas.pixel(3, 4, Rgba::rgb(200, 100, 0));
        let mut out = Vec::new();
        transmit_and_place_with(&canvas, placement(), options(Compression::Zlib), &mut out);
        let (control, pixels) = decode(&out);
        assert!(
            control.starts_with("_Ga=T,f=24,o=z,s=50,v=30,i=7,"),
            "{control}"
        );
        let expected: Vec<u8> = canvas
            .pixels()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b])
            .collect();
        assert_eq!(pixels, expected);
        // Flat images shrink a lot.
        assert!(out.len() < 500, "{} bytes", out.len());
    }

    #[test]
    fn transparent_images_keep_their_alpha() {
        let mut canvas = Canvas::filled(5, 5, Rgba::rgb(1, 2, 3));
        canvas.pixels_mut()[0] = Rgba::new(9, 9, 9, 100);
        for compression in [Compression::None, Compression::Zlib] {
            let mut out = Vec::new();
            transmit_and_place_with(&canvas, placement(), options(compression), &mut out);
            let (control, pixels) = decode(&out);
            assert!(control.contains("f=32"), "{control}");
            assert_eq!(control.contains("o=z"), compression == Compression::Zlib);
            let expected: Vec<u8> = canvas
                .pixels()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect();
            assert_eq!(pixels, expected);
        }
    }

    #[cfg(feature = "png")]
    #[test]
    fn png_is_the_default_and_keeps_every_pixel() {
        let mut canvas = Canvas::filled(40, 20, Rgba::rgb(10, 20, 30));
        canvas.pixel(5, 5, Rgba::rgb(250, 0, 0));
        for transparent in [false, true] {
            if transparent {
                canvas.pixels_mut()[0] = Rgba::TRANSPARENT;
            }
            let mut out = Vec::new();
            transmit_and_place_with(&canvas, placement(), Options::default(), &mut out);
            let (control, png) = decode(&out);
            assert!(
                control.starts_with("_Ga=T,f=100,i=7,c=3,r=2,"),
                "{control}"
            );
            assert_eq!(Canvas::from_png(&png).unwrap(), canvas);
        }
    }

    #[test]
    fn deletion() {
        let mut out = Vec::new();
        delete(7, &mut out);
        delete_all(&mut out);
        assert_eq!(out, b"\x1b_Ga=d,d=I,i=7,q=2\x1b\\\x1b_Ga=d,d=A,q=2\x1b\\");
    }
}
