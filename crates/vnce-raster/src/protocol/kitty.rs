//! The Kitty graphics protocol.
//!
//! Images are sent as raw RGBA, base64 encoded, in chunks of at most 4096 bytes. Each image
//! has an id, so it can be replaced or deleted later. Responses from the terminal are
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

/// Appends the sequences that transmit `canvas` and display it at the cursor position,
/// without moving the cursor.
pub fn transmit_and_place(canvas: &Canvas, placement: Placement, out: &mut Vec<u8>) {
    let mut raw = Vec::with_capacity(canvas.pixels().len() * 4);
    for p in canvas.pixels() {
        raw.extend_from_slice(&[p.r, p.g, p.b, p.a]);
    }
    let payload = base64::encode(&raw);
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
                "a=T,f=32,s={},v={},i={},c={},r={},C=1,q=2,z={},m={more}",
                canvas.width(),
                canvas.height(),
                placement.id,
                placement.columns,
                placement.rows,
                placement.z_index,
            );
        } else {
            let _ = write!(out, "m={more}");
        }
        out.push(b';');
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
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

    #[test]
    fn deletion() {
        let mut out = Vec::new();
        delete(7, &mut out);
        delete_all(&mut out);
        assert_eq!(out, b"\x1b_Ga=d,d=I,i=7,q=2\x1b\\\x1b_Ga=d,d=A,q=2\x1b\\");
    }
}
