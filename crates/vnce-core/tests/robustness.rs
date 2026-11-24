//! Randomized tests: thousands of random writes must keep the buffer consistent.

// Test code converts small random numbers freely.
#![allow(
    clippy::cast_possible_truncation,
    reason = "random values are reduced to small ranges first"
)]

use vnce_core::{Buffer, Color, Rect, Size, Style};

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

/// Text pieces that stress width handling: wide, combining, zero-width, control.
const PIECES: [&str; 14] = [
    "a",
    "Z",
    " ",
    "日",
    "本",
    "😀",
    "e\u{301}",
    "\u{200d}",
    "\x1b[31m",
    "\n",
    "\t",
    "é",
    "│",
    "👩\u{200d}💻",
];

fn random_text(rng: &mut Rng) -> String {
    (0..rng.below(12)).map(|_| *rng.pick(&PIECES)).collect()
}

fn random_style(rng: &mut Rng) -> Style {
    let mut style = Style::new();
    if rng.below(2) == 0 {
        style = style.fg(Color::rgb(rng.below(256) as u8, 0, 0));
    }
    if rng.below(2) == 0 {
        style = style.bg(Color::rgb(0, 0, rng.below(256) as u8));
    }
    style
}

/// Every continuation cell belongs to a wide character that starts on its left, and no
/// cell holds a control character.
fn assert_consistent(buffer: &Buffer) {
    let size = buffer.size();
    for y in 0..size.height {
        let mut covered = 0_usize;
        for x in 0..size.width {
            let cell = buffer.cell(x, y).unwrap();
            assert!(
                !cell.symbol().chars().any(char::is_control),
                "control character at ({x}, {y})"
            );
            if cell.is_continuation() {
                assert!(covered > 0, "orphan continuation cell at ({x}, {y})");
                covered -= 1;
            } else {
                assert_eq!(covered, 0, "wide character cut at ({x}, {y})");
                covered = cell.width().saturating_sub(1);
            }
        }
        assert_eq!(covered, 0, "wide character past the end of row {y}");
    }
}

#[test]
fn random_writes_keep_wide_characters_whole() {
    let mut rng = Rng::new(0x5EED);
    for _ in 0..300 {
        let size = Size::new(1 + rng.below(20) as u16, 1 + rng.below(6) as u16);
        let mut buffer = Buffer::empty(size);
        for _ in 0..40 {
            let x = rng.below(u64::from(size.width) + 3) as u16;
            let y = rng.below(u64::from(size.height) + 2) as u16;
            let style = random_style(&mut rng);
            match rng.below(4) {
                0 => {
                    buffer.set_string(x, y, &random_text(&mut rng), style);
                }
                1 => {
                    let max = rng.below(8) as u16;
                    buffer.set_stringn(x, y, &random_text(&mut rng), max, style);
                }
                2 => {
                    buffer.set_symbol(x, y, rng.pick(&PIECES), style);
                }
                _ => {
                    let area = Rect::new(x, y, rng.below(6) as u16, rng.below(4) as u16);
                    buffer.set_style(area, style);
                }
            }
            assert_consistent(&buffer);
        }
    }
}

#[test]
fn diffs_are_in_bounds_and_in_order() {
    let mut rng = Rng::new(42);
    for _ in 0..300 {
        let size = Size::new(1 + rng.below(16) as u16, 1 + rng.below(5) as u16);
        let mut previous = Buffer::empty(size);
        let mut current = Buffer::empty(size);
        for buffer in [&mut previous, &mut current] {
            for _ in 0..10 {
                let x = rng.below(u64::from(size.width)) as u16;
                let y = rng.below(u64::from(size.height)) as u16;
                buffer.set_string(x, y, &random_text(&mut rng), random_style(&mut rng));
            }
        }
        let updates = current.diff(&previous);
        let mut last = None;
        for update in &updates {
            assert!(update.x < size.width && update.y < size.height);
            assert!(!update.cell.is_continuation());
            let index = (update.y, update.x);
            assert!(last < Some(index), "updates out of order");
            last = Some(index);
        }
        // Identical buffers have no differences.
        assert_eq!(current.diff(&current.clone()).len(), 0);
        // A different size updates every cell that is not a continuation.
        let other = Buffer::empty(Size::new(size.width + 1, size.height));
        let all = current.diff(&other).len();
        let cells = current
            .cells()
            .iter()
            .filter(|c| !c.is_continuation())
            .count();
        assert_eq!(all, cells);
    }
}
