//! Randomized tests: drawing anywhere, converting to cells and encoding for graphics
//! protocols must never panic, never write outside the target, and always produce
//! well-formed output.

// Test code converts small random numbers freely.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "random values are reduced to small ranges first"
)]

use vnce_core::{Buffer, Cell, Rect, Size};
use vnce_raster::protocol::{base64, iterm2, kitty, sixel};
use vnce_raster::{BlendMode, BlitOptions, Canvas, PixelMode, Quality, RenderOptions, Rgba};

/// A small deterministic random generator (xorshift64*), so failures can be reproduced.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n` (0 when `n` is 0).
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }

    /// A coordinate that is often outside the canvas, sometimes far outside.
    fn coordinate(&mut self, size: u32) -> i32 {
        match self.below(10) {
            0 => i32::MIN / 2 + self.below(10) as i32,
            1 => i32::MAX / 2 - self.below(10) as i32,
            _ => self.below(u64::from(size) * 2 + 10) as i32 - (size as i32 / 2) - 5,
        }
    }

    fn color(&mut self) -> Rgba {
        let v = self.next();
        Rgba::new(v as u8, (v >> 8) as u8, (v >> 16) as u8, (v >> 24) as u8)
    }

    fn canvas(&mut self, max: u64) -> Canvas {
        let (width, height) = (self.below(max) as u32, self.below(max) as u32);
        let mut canvas = Canvas::new(width, height);
        let detail = self.below(4);
        for pixel in canvas.pixels_mut() {
            // Few colors, many colors, or noise.
            *pixel = match detail {
                0 => Rgba::rgb(0, 0, 0),
                1 => Rgba::rgb((self.next() % 3) as u8 * 100, 50, 50),
                _ => self.color(),
            };
        }
        canvas
    }
}

#[test]
fn drawing_anywhere_never_panics() {
    let mut rng = Rng::new(11);
    let sprite = Canvas::filled(5, 3, Rgba::rgb(200, 10, 10));
    for _ in 0..400 {
        let (width, height) = (rng.below(40) as u32, rng.below(30) as u32);
        let mut canvas = Canvas::new(width, height);
        let modes = [
            BlendMode::Replace,
            BlendMode::Alpha,
            BlendMode::Add,
            BlendMode::Multiply,
        ];
        canvas.set_blend_mode(modes[rng.below(4) as usize]);
        for _ in 0..20 {
            let (x, y) = (rng.coordinate(width), rng.coordinate(height));
            let (x2, y2) = (rng.coordinate(width), rng.coordinate(height));
            let color = rng.color();
            let size = rng.below(50) as u32;
            match rng.below(9) {
                0 => canvas.pixel(x, y, color),
                1 => canvas.line(x, y, x2, y2, color),
                2 => canvas.line_aa(x as f32, y as f32, x2 as f32, y2 as f32, color),
                3 => canvas.rect(x, y, size, size, color),
                4 => canvas.fill_rect(x, y, size, size, color),
                5 => canvas.circle(x, y, size, color),
                6 => canvas.fill_circle(x, y, size, color),
                7 => canvas.fill_triangle(
                    (x as f32, y as f32),
                    (x2 as f32, y2 as f32),
                    (x as f32 + 3.5, y2 as f32 - 1.25),
                    color,
                ),
                _ => canvas.blit(&sprite, x, y, BlitOptions::default()),
            }
        }
        assert_eq!(canvas.pixels().len(), (width * height) as usize);
    }
}

#[test]
fn cell_modes_stay_inside_their_area() {
    let mut rng = Rng::new(23);
    let marker = Cell::new("@");
    for _ in 0..300 {
        let canvas = rng.canvas(30);
        let size = Size::new(rng.below(20) as u16, rng.below(10) as u16);
        let mut buffer = Buffer::filled(size, &marker);
        let area = Rect::new(
            rng.below(25) as u16,
            rng.below(12) as u16,
            rng.below(20) as u16,
            rng.below(10) as u16,
        );
        let mode = PixelMode::ALL[rng.below(6) as usize];
        let options = RenderOptions {
            quality: if rng.below(2) == 0 {
                Quality::Fast
            } else {
                Quality::Exact
            },
            ..RenderOptions::default()
        };
        canvas.render(&mut buffer, area, mode, &options);
        let target = area.intersection(Rect::from(size));
        for y in 0..size.height {
            for x in 0..size.width {
                let inside = x >= target.left()
                    && x < target.right()
                    && y >= target.top()
                    && y < target.bottom();
                if !inside {
                    assert_eq!(buffer.cell(x, y), Some(&marker), "{mode} wrote ({x}, {y})");
                }
            }
        }
        // Scaling to any size works too.
        let scaled = canvas.scaled(rng.below(40) as u32, rng.below(40) as u32);
        assert_eq!(
            scaled.pixels().len(),
            (scaled.width() * scaled.height()) as usize
        );
    }
}

/// Decodes standard base64 (test helper).
fn decode_base64(text: &[u8]) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("invalid base64 byte {c}"),
    };
    let mut out = Vec::new();
    for group in text.chunks(4) {
        let digits: Vec<u8> = group
            .iter()
            .filter(|&&c| c != b'=')
            .map(|&c| value(c))
            .collect();
        let bits = digits
            .iter()
            .enumerate()
            .fold(0_u32, |acc, (i, &d)| acc | (u32::from(d) << (18 - 6 * i)));
        out.extend_from_slice(&bits.to_be_bytes()[1..digits.len()]);
    }
    out
}

#[test]
fn base64_round_trips() {
    let mut rng = Rng::new(5);
    for _ in 0..500 {
        let data: Vec<u8> = (0..rng.below(100)).map(|_| rng.next() as u8).collect();
        let encoded = base64::encode(&data);
        assert_eq!(encoded.len(), data.len().div_ceil(3) * 4);
        assert_eq!(decode_base64(&encoded), data);
    }
}

#[test]
fn kitty_chunks_carry_the_exact_pixels() {
    let mut rng = Rng::new(17);
    for _ in 0..100 {
        let canvas = rng.canvas(60);
        let mut out = Vec::new();
        let placement = kitty::Placement {
            id: 1 + rng.below(1000) as u32,
            columns: rng.below(80) as u16,
            rows: rng.below(40) as u16,
            z_index: rng.below(3) as i32 - 1,
        };
        kitty::transmit_and_place(&canvas, placement, &mut out);
        let mut payload = Vec::new();
        let mut rest = out.as_slice();
        let mut chunks = 0;
        while !rest.is_empty() {
            assert!(rest.starts_with(b"\x1b_G"));
            let end = rest.windows(2).position(|w| w == b"\x1b\\").expect("ST");
            let body = &rest[3..end];
            let separator = body.iter().position(|&b| b == b';').expect("';'");
            let (control, data) = (&body[..separator], &body[separator + 1..]);
            assert!(data.len() <= 4096, "chunk too long");
            let more = control.ends_with(b"m=1");
            payload.extend_from_slice(data);
            rest = &rest[end + 2..];
            chunks += 1;
            assert_eq!(more, !rest.is_empty(), "m= flag of chunk {chunks}");
        }
        let raw: Vec<u8> = canvas
            .pixels()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect();
        assert_eq!(decode_base64(&payload), raw);
    }
}

#[test]
fn sixel_output_is_well_formed() {
    let mut rng = Rng::new(31);
    for _ in 0..150 {
        let canvas = rng.canvas(40);
        let options = sixel::SixelOptions {
            max_colors: 1 + rng.below(256) as u16,
            dither: rng.below(2) == 0,
        };
        let mut out = Vec::new();
        sixel::encode(&canvas, options, &mut out);
        if canvas.width() == 0 || canvas.height() == 0 {
            // Nothing is written for an empty canvas.
            assert_eq!(out.len(), 0);
            continue;
        }
        assert!(out.starts_with(b"\x1bP"), "missing DCS");
        assert!(out.ends_with(b"\x1b\\"), "missing ST");
        let body = &out[2..out.len() - 2];
        // Only one ESC (the ST) and only printable sixel bytes.
        assert!(!body.contains(&0x1b), "ESC inside the image");
        assert!(
            body.iter().all(|&b| (0x20..=0x7e).contains(&b)),
            "non-printable byte"
        );
        // Palette entries stay within the limit.
        let text = String::from_utf8_lossy(body);
        for definition in text.split('#').skip(1) {
            if let Some((index, rest)) = definition.split_once(';') {
                if rest.starts_with("2;") {
                    let index: u16 = index.parse().expect("color index");
                    assert!(index < options.max_colors, "color {index}");
                }
            }
        }
    }
}

#[test]
fn iterm2_output_is_well_formed() {
    let mut rng = Rng::new(47);
    for _ in 0..50 {
        let canvas = rng.canvas(30);
        let mut out = Vec::new();
        if iterm2::encode(
            &canvas,
            1 + rng.below(50) as u16,
            1 + rng.below(20) as u16,
            &mut out,
        )
        .is_err()
        {
            // Only empty canvases cannot become a PNG.
            assert!(canvas.width() == 0 || canvas.height() == 0);
            continue;
        }
        assert!(out.starts_with(b"\x1b]1337;File=inline=1;"));
        assert_eq!(out.last(), Some(&0x07));
        let colon = out.iter().position(|&b| b == b':').expect("':'");
        let png = decode_base64(&out[colon + 1..out.len() - 1]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let size = format!("size={};", png.len());
        assert!(String::from_utf8_lossy(&out[..colon]).contains(&size));
        let decoded = Canvas::from_png(&png).expect("valid PNG");
        assert_eq!(decoded, canvas);
    }
}
