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
