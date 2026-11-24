//! Randomized tests: widgets with random content in random areas never panic and never
//! draw outside their area; layouts never leave their area.

// Test code converts small random numbers freely.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "random values are reduced to small ranges first"
)]

use vnce_core::{Buffer, Cell, Rect, Size};
use vnce_ui::{
    Alignment, Block, BorderType, Borders, Constraint, FpsOverlay, Gauge, Layout, Line, List,
    ListState, Padding, Paragraph, StatefulWidget, Widget,
};

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

    fn text(&mut self) -> String {
        const PIECES: [&str; 12] = [
            "word",
            " ",
            "  ",
            "a",
            "日本",
            "😀",
            "e\u{301}",
            "\n",
            "\t",
            "\x1b",
            "superlongword",
            "é",
        ];
        (0..self.below(15)).map(|_| *self.pick(&PIECES)).collect()
    }

    fn area(&mut self, size: Size) -> Rect {
        Rect::new(
            self.below(u64::from(size.width) + 3) as u16,
            self.below(u64::from(size.height) + 3) as u16,
            self.below(u64::from(size.width) + 3) as u16,
            self.below(u64::from(size.height) + 3) as u16,
        )
    }

    fn block(&mut self) -> Block<'static> {
        let borders = [
            Borders::NONE,
            Borders::ALL,
            Borders::TOP | Borders::LEFT,
            Borders::BOTTOM,
        ];
        let types = [
            BorderType::Plain,
            BorderType::Rounded,
            BorderType::Double,
            BorderType::Thick,
            BorderType::Ascii,
        ];
        let mut block = Block::new()
            .borders(*self.pick(&borders))
            .border_type(*self.pick(&types))
            .padding(Padding::new(
                self.below(3) as u16,
                self.below(3) as u16,
                self.below(2) as u16,
                self.below(2) as u16,
            ));
        if self.below(2) == 0 {
            block = block.title(Line::from(self.text()).alignment(Alignment::Center));
        }
        if self.below(3) == 0 {
            block = block.title_bottom(self.text());
        }
        block
    }
}

/// Renders into a buffer full of markers and checks that only `area` changed.
fn assert_stays_inside(size: Size, area: Rect, render: impl FnOnce(&mut Buffer)) {
    let marker = Cell::new("@");
    let mut buffer = Buffer::filled(size, &marker);
    render(&mut buffer);
    let inside = area.intersection(Rect::from(size));
    for y in 0..size.height {
        for x in 0..size.width {
            let within = x >= inside.left()
                && x < inside.right()
                && y >= inside.top()
                && y < inside.bottom();
            if !within {
                assert_eq!(
                    buffer.cell(x, y),
                    Some(&marker),
                    "wrote ({x}, {y}) for {area:?}"
                );
            }
        }
    }
}

#[test]
fn widgets_stay_inside_their_area() {
    let mut rng = Rng::new(3);
    for _ in 0..1_000 {
        let size = Size::new(rng.below(30) as u16, rng.below(12) as u16);
        let area = rng.area(size);
        match rng.below(5) {
            0 => {
                let paragraph = Paragraph::new(rng.text())
                    .wrap(rng.below(2) == 0)
                    .scroll(rng.below(4) as u16)
                    .alignment(*rng.pick(&[Alignment::Left, Alignment::Center, Alignment::Right]))
                    .block(rng.block());
                assert_stays_inside(size, area, |b| paragraph.render(area, b));
            }
            1 => {
                let block = rng.block();
                assert_stays_inside(size, area, |b| block.render(area, b));
                let inner = block.inner(area);
                assert!(inner.is_empty() || inner.intersection(area) == inner);
            }
            2 => {
                let gauge = Gauge::new()
                    .ratio(rng.below(1200) as f64 / 1000.0)
                    .label(rng.text())
                    .ascii(rng.below(2) == 0)
                    .block(rng.block());
                assert_stays_inside(size, area, |b| gauge.render(area, b));
            }
            3 => {
                let items: Vec<String> = (0..rng.below(10)).map(|_| rng.text()).collect();
                let list = List::new(items.clone())
                    .highlight_symbol(rng.text())
                    .block(rng.block());
                let mut state = ListState::with_selected(Some(rng.below(15) as usize));
                assert_stays_inside(size, area, |b| list.render(area, b, &mut state));
                if let Some(selected) = state.selected() {
                    assert!(selected < items.len());
                    assert!(state.offset() <= selected);
                }
            }
            _ => {
                let overlay = FpsOverlay::new(rng.below(500) as f64)
                    .ups(60.0)
                    .history((0..rng.below(30)).map(|i| i as f32))
                    .ascii(rng.below(2) == 0);
                assert_stays_inside(size, area, |b| overlay.render(area, b));
            }
        }
    }
}

#[test]
fn wrapped_rows_fit_the_width() {
    let mut rng = Rng::new(8);
    for _ in 0..1_000 {
        let text = rng.text();
        let width = 1 + rng.below(12) as u16;
        let paragraph = Paragraph::new(text.as_str()).wrap(true);
        let rows = paragraph.line_count(width);
        let mut buffer = Buffer::empty(Size::new(width, rows as u16 + 1));
        paragraph.render(Rect::new(0, 0, width, rows as u16 + 1), &mut buffer);
        // The last row (after all text) stays empty.
        let last = rows as u16;
        for x in 0..width {
            assert_eq!(buffer.cell(x, last).unwrap().symbol(), " ", "{text:?}");
        }
        // Every non-space character of the text is shown somewhere (nothing is lost),
        // except wide characters in a one-column area.
        if width > 1 {
            let shown: String = buffer
                .cells()
                .iter()
                .filter(|c| !c.is_continuation())
                .map(Cell::symbol)
                .collect::<String>()
                .replace(' ', "");
            let expected: String = text
                .chars()
                .filter(|c| !c.is_whitespace() && !c.is_control())
                .collect();
            assert_eq!(shown, expected, "{text:?} in {width} columns");
        }
    }
}

#[test]
fn random_layouts_never_leave_the_area() {
    let mut rng = Rng::new(13);
    for _ in 0..2_000 {
        let constraints: Vec<Constraint> = (0..rng.below(6))
            .map(|_| match rng.below(6) {
                0 => Constraint::Length(rng.below(30) as u16),
                1 => Constraint::Percentage(rng.below(150) as u16),
                2 => Constraint::Ratio(rng.below(5) as u32, rng.below(5) as u32),
                3 => Constraint::Min(rng.below(20) as u16),
                4 => Constraint::Max(rng.below(20) as u16),
                _ => Constraint::Fill(rng.below(4) as u16),
            })
            .collect();
        let area = Rect::new(
            rng.below(100) as u16,
            rng.below(100) as u16,
            rng.below(60) as u16,
            rng.below(60) as u16,
        );
        let layout = if rng.below(2) == 0 {
            Layout::horizontal(constraints.iter().copied())
        } else {
            Layout::vertical(constraints.iter().copied())
        }
        .spacing(rng.below(4) as u16)
        .margin(rng.below(3) as u16, rng.below(3) as u16);
        let parts = layout.split(area);
        assert_eq!(parts.len(), constraints.len());
        // Parts follow each other without overlapping.
        for pair in parts.windows(2) {
            assert!(!pair[0].intersects(pair[1]), "{constraints:?} {pair:?}");
        }
        for part in parts {
            assert!(
                part.left() >= area.left()
                    && part.right() <= area.right()
                    && part.top() >= area.top()
                    && part.bottom() <= area.bottom(),
                "{constraints:?} {area:?} {part:?}"
            );
        }
        // Same input, same output.
        assert_eq!(layout.split(area), layout.split(area));
    }
}
