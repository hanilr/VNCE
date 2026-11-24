//! Randomized tests: random frames must reach the (emulated) screen exactly, and random
//! terminal answers must never crash the parser.

// Test code converts small random numbers freely.
#![allow(
    clippy::cast_possible_truncation,
    reason = "random values are reduced to small ranges first"
)]

use vnce_core::{Buffer, Color, Size, Style};
use vnce_term::{Capabilities, Terminal, TerminalInfo, TestBackend};

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

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

const PIECES: [&str; 10] = [
    "a", "Z", " ", "日", "😀", "e\u{301}", "\x1b[2J", "\n", "é", "│",
];

fn random_style(rng: &mut Rng) -> Style {
    let colors = [
        Color::Reset,
        Color::Red,
        Color::Indexed(200),
        Color::rgb(10, 200, 30),
    ];
    let mut style = Style::new().fg(*rng.pick(&colors)).bg(*rng.pick(&colors));
    if rng.below(3) == 0 {
        style = style.bold();
    }
    if rng.below(5) == 0 {
        style = style.underline();
    }
    style
}

/// Compares the visible content and colors of two buffers.
fn assert_same_screen(screen: &Buffer, expected: &Buffer, frame: usize) {
    assert_eq!(screen.size(), expected.size(), "frame {frame}");
    for (index, (actual, wanted)) in screen.cells().iter().zip(expected.cells()).enumerate() {
        if wanted.is_continuation() {
            continue;
        }
        assert_eq!(
            (actual.symbol(), actual.fg, actual.bg, actual.modifiers),
            (wanted.symbol(), wanted.fg, wanted.bg, wanted.modifiers),
            "frame {frame}, cell {index}"
        );
    }
}

#[test]
fn random_frames_reach_the_screen_exactly() {
    let mut rng = Rng::new(7);
    let mut terminal =
        Terminal::new(TestBackend::new(12, 5), Capabilities::default()).expect("terminal");
    for frame in 0..400 {
        // Sometimes resize, sometimes clear.
        match rng.below(25) {
            0 => {
                let (width, height) = (1 + rng.below(15) as u16, 1 + rng.below(6) as u16);
                terminal.backend_mut().resize(width, height);
            }
            1 => terminal.clear(),
            _ => {}
        }
        let mut expected = Buffer::empty(Size::new(0, 0));
        terminal
            .draw(|f| {
                let size = f.area();
                for _ in 0..rng.below(8) {
                    let x = rng.below(u64::from(size.width) + 1) as u16;
                    let y = rng.below(u64::from(size.height)) as u16;
                    let text: String = (0..rng.below(6)).map(|_| *rng.pick(&PIECES)).collect();
                    let style = random_style(&mut rng);
                    f.buffer_mut().set_string(x, y, &text, style);
                }
                expected = f.buffer().clone();
            })
            .expect("draw");
        assert_same_screen(terminal.backend().screen(), &expected, frame);
    }
}

#[test]
fn random_answers_never_crash_the_parser() {
    // Valid answers, broken pieces and noise.
    let fragments: [&[u8]; 16] = [
        b"\x1b[?62;4;22c",
        b"\x1bP>|kitty(0.40.0)\x1b\\",
        b"\x1b_Gi=31;OK\x1b\\",
        b"\x1b[6;20;10t",
        b"\x1b[4;800;1200t",
        b"\x1b[?1;0;256S",
        b"\x1b[?2;0;1000;1000S",
        b"\x1b[?15u",
        b"\x1b",
        b"\x1b[",
        b"\x1bP",
        b"\x1b_G",
        b"\x1b]11;rgb:0000/0000/0000\x07",
        b"\x1b[99999999999999999999;1t",
        b";;;",
        b"\xff\xfe\x00",
    ];
    let mut rng = Rng::new(99);
    for _ in 0..5_000 {
        let mut answer = Vec::new();
        for _ in 0..rng.below(8) {
            if rng.below(4) == 0 {
                answer.push(rng.below(256) as u8);
            } else {
                let fragment = rng.pick(&fragments);
                // Sometimes only a prefix of the fragment.
                let cut = if rng.below(3) == 0 {
                    rng.below(fragment.len() as u64 + 1) as usize
                } else {
                    fragment.len()
                };
                answer.extend_from_slice(&fragment[..cut]);
            }
        }
        let info = TerminalInfo::parse(&answer);
        if let Some((width, height)) = info.cell_pixels {
            assert!(width > 0 && height > 0, "{answer:?}");
        }
        // A complete DA1 answer at the end is always recognized.
        answer.extend_from_slice(b"\x1b[?62;4c");
        assert!(TerminalInfo::parse(&answer).responded, "{answer:?}");
    }
}

#[test]
fn random_image_updates_stay_consistent() {
    use std::collections::HashSet;
    use vnce_core::Rect;
    use vnce_raster::{Canvas, Rgba};
    use vnce_term::{GraphicsMode, GraphicsProtocol};

    let mut rng = Rng::new(2024);
    for protocol in [
        GraphicsProtocol::Kitty,
        GraphicsProtocol::Sixel,
        GraphicsProtocol::Iterm2,
    ] {
        let mut terminal =
            Terminal::new(TestBackend::new(30, 12), Capabilities::default()).expect("terminal");
        terminal.set_graphics_mode(GraphicsMode::Protocol(protocol));
        // Kitty ids on the screen.
        let mut live: HashSet<String> = HashSet::new();
        let mut pictures: Vec<(Rect, Canvas)> = Vec::new();
        for _ in 0..150 {
            match rng.below(6) {
                // A new image, often at the exact cell resolution (10 × 20 pixels).
                0 if pictures.len() < 3 => {
                    let area = Rect::new(
                        rng.below(20) as u16,
                        rng.below(8) as u16,
                        1 + rng.below(12) as u16,
                        1 + rng.below(5) as u16,
                    );
                    let (width, height) = if rng.below(3) == 0 {
                        (1 + rng.below(50) as u32, 1 + rng.below(50) as u32)
                    } else {
                        (u32::from(area.width) * 10, u32::from(area.height) * 20)
                    };
                    pictures.push((area, Canvas::filled(width, height, Rgba::rgb(9, 9, 9))));
                }
                1 if !pictures.is_empty() => {
                    pictures.remove(rng.below(pictures.len() as u64) as usize);
                }
                // Change a few pixels or everything.
                _ if !pictures.is_empty() => {
                    let index = rng.below(pictures.len() as u64) as usize;
                    let canvas = &mut pictures[index].1;
                    let color = Rgba::rgb(rng.below(256) as u8, 0, 0);
                    if rng.below(5) == 0 {
                        canvas.clear(color);
                    } else {
                        let (x, y) = (
                            rng.below(u64::from(canvas.width())),
                            rng.below(u64::from(canvas.height())),
                        );
                        canvas.pixel(x as i32, y as i32, color);
                    }
                }
                _ => {}
            }
            if rng.below(30) == 0 {
                terminal.clear();
                live.clear();
            }
            terminal
                .draw(|f| {
                    for (area, canvas) in &pictures {
                        f.draw_image(*area, canvas);
                    }
                })
                .expect("draw");
            for sequence in terminal.backend().graphics_sequences() {
                let text = String::from_utf8_lossy(sequence);
                let id = text
                    .split([',', ';'])
                    .find_map(|key| key.strip_prefix("i="))
                    .map(str::to_owned);
                if text.starts_with("\x1b_Ga=d,d=A") {
                    live.clear();
                } else if let Some(id) = id {
                    if text.starts_with("\x1b_Ga=d") {
                        live.remove(&id);
                    } else if text.starts_with("\x1b_Ga=T") {
                        live.insert(id);
                    }
                }
            }
            terminal.backend_mut().clear_graphics_sequences();
        }
        // Without images, no Kitty image is left on the screen.
        terminal.draw(|_| {}).expect("draw");
        for sequence in terminal.backend().graphics_sequences() {
            let text = String::from_utf8_lossy(sequence);
            if let Some(id) = text.split(',').find_map(|k| k.strip_prefix("i=")) {
                live.remove(id);
            }
        }
        assert!(live.is_empty(), "{protocol:?} left images {live:?}");
    }
}
