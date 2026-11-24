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
    let first = format!(
        "a=T,{keys},i={},c={},r={},C=1,q=2,z={}",
        placement.id, placement.columns, placement.rows, placement.z_index,
    );
    write_chunks(&first, &payload, out);
}

/// Transmits `canvas` as image `id` with a virtual placement of `columns` × `rows` cells
/// (`U=1`): nothing is drawn until the cells show Unicode placeholders
/// ([`placeholder`]). This works inside tmux, which moves the placeholders like text.
pub fn transmit_virtual(
    canvas: &Canvas,
    id: u32,
    columns: u16,
    rows: u16,
    options: Options,
    out: &mut Vec<u8>,
) {
    let (keys, data) = encode(canvas, options);
    let payload = base64::encode(&data);
    let first = format!("a=T,U=1,{keys},i={id},c={columns},r={rows},q=2");
    write_chunks(&first, &payload, out);
}

/// Writes `payload` in chunks; the first one carries the `first` keys.
fn write_chunks(first: &str, payload: &[u8], out: &mut Vec<u8>) {
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
            let _ = write!(out, "{first},m={more}");
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

/// Unicode placeholder character: a cell showing it displays part of an image with a
/// virtual placement ([`transmit_virtual`]).
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// Combining characters that encode row and column numbers in placeholders, in order
/// (from the Kitty documentation: `rowcolumn-diacritics.txt`).
#[rustfmt::skip]
const DIACRITICS: [u32; 297] = [
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B,
    0x034C, 0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367,
    0x0368, 0x0369, 0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485,
    0x0486, 0x0487, 0x0592, 0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D,
    0x059E, 0x059F, 0x05A0, 0x05A1, 0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610,
    0x0611, 0x0612, 0x0613, 0x0614, 0x0615, 0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A,
    0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8, 0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF,
    0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB, 0x06EC, 0x0730, 0x0732, 0x0733,
    0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743, 0x0745, 0x0747, 0x0749,
    0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3, 0x0816, 0x0817,
    0x0818, 0x0819, 0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822, 0x0823,
    0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954,
    0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75,
    0x1A76, 0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F,
    0x1B70, 0x1B71, 0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0,
    0x1DC1, 0x1DC3, 0x1DC4, 0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1,
    0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5, 0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC,
    0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1, 0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE,
    0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7, 0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9,
    0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2, 0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6,
    0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC, 0x2DED, 0x2DEE, 0x2DEF, 0x2DF0, 0x2DF1,
    0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA, 0x2DFB, 0x2DFC,
    0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1, 0xA8E2,
    0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF,
    0xAAC1, 0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38,
    0x1D185, 0x1D186, 0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242,
    0x1D243, 0x1D244,
];

/// The largest row or column a placeholder can address.
pub const MAX_PLACEHOLDER_CELLS: u16 = 297;

/// The placeholder text for cell `(column, row)` of image `id`: the placeholder
/// character, the row and column diacritics, and a third diacritic with the most
/// significant byte of `id` when it is not zero. The cell's foreground color must hold
/// the lower 24 bits of `id` ([`placeholder_color`]). Returns `None` past
/// [`MAX_PLACEHOLDER_CELLS`].
#[must_use]
pub fn placeholder(id: u32, column: u16, row: u16) -> Option<String> {
    let mark = |n: usize| DIACRITICS.get(n).copied().and_then(char::from_u32);
    let mut text = String::with_capacity(12);
    text.push(PLACEHOLDER);
    text.push(mark(usize::from(row))?);
    text.push(mark(usize::from(column))?);
    let high = id >> 24;
    if high != 0 {
        text.push(mark(high as usize)?);
    }
    Some(text)
}

/// The foreground color `(r, g, b)` of placeholder cells of image `id`.
#[must_use]
pub fn placeholder_color(id: u32) -> (u8, u8, u8) {
    let [_, r, g, b] = id.to_be_bytes();
    (r, g, b)
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
            control.starts_with("\x1b_Ga=T,f=24,o=z,s=50,v=30,i=7,"),
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
                control.starts_with("\x1b_Ga=T,f=100,i=7,c=3,r=2,"),
                "{control}"
            );
            assert_eq!(Canvas::from_png(&png).unwrap(), canvas);
        }
    }

    #[test]
    fn virtual_placements_for_placeholders() {
        let canvas = Canvas::filled(1, 1, Rgba::new(1, 2, 3, 4));
        let mut out = Vec::new();
        transmit_virtual(&canvas, 42, 5, 3, Options::RAW, &mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b_Ga=T,U=1,f=32,s=1,v=1,i=42,c=5,r=3,q=2,m=0;AQIDBA==\x1b\\"
        );
    }

    #[test]
    fn placeholder_cells() {
        use unicode_width::UnicodeWidthStr;

        // Every diacritic is a distinct, zero-width combining character.
        let mut seen = std::collections::HashSet::new();
        for &code in &DIACRITICS {
            let c = char::from_u32(code).unwrap();
            assert!(seen.insert(c));
            assert_eq!(c.to_string().width(), 0, "{code:#x}");
        }
        let cell = placeholder(42, 2, 1).unwrap();
        assert_eq!(cell, "\u{10EEEE}\u{030D}\u{030E}");
        // The placeholder with its diacritics takes one column.
        assert_eq!(cell.width(), 1);
        assert_eq!(placeholder_color(42), (0, 0, 42));
        assert_eq!(placeholder_color(0x0102_0304), (2, 3, 4));
        // A third diacritic carries the most significant byte of large ids.
        assert!(
            placeholder(0x0200_0000, 0, 0)
                .unwrap()
                .ends_with('\u{030E}')
        );
        assert_eq!(placeholder(1, 297, 0), None);
        assert_eq!(placeholder(1, 296, 296).map(|p| p.chars().count()), Some(3));
    }

    #[test]
    fn deletion() {
        let mut out = Vec::new();
        delete(7, &mut out);
        delete_all(&mut out);
        assert_eq!(out, b"\x1b_Ga=d,d=I,i=7,q=2\x1b\\\x1b_Ga=d,d=A,q=2\x1b\\");
    }
}
