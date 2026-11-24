//! # `vnce_ui`
//!
//! A small set of widgets for games and graphics applications in the terminal:
//!
//! - [`Layout`]: splits an area into parts with [`Constraint`]s.
//! - [`Span`], [`Line`] and [`Text`]: styled text; [`Paragraph`] draws it aligned,
//!   wrapped, truncated with `…` or scrolled.
//! - [`Block`]: borders (plain, rounded, double, thick or ASCII), titles and padding.
//!
//! Widgets implement [`Widget`] (or [`StatefulWidget`] when they keep state between
//! frames) and draw into a [`vnce_core::Buffer`], usually `frame.buffer_mut()`, so they mix
//! freely with pixel graphics and images in the same frame. Styles are patches: a widget
//! drawn on top of another keeps the colors it does not set.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod block;
pub mod layout;
pub mod paragraph;
pub mod text;
mod widget;

pub use block::{Block, BorderType, Borders, Padding};
pub use layout::{Constraint, Direction, Layout};
pub use paragraph::Paragraph;
pub use text::{Alignment, Line, Span, Text};
pub use widget::{StatefulWidget, Widget};

/// Helpers for the tests of this crate.
#[cfg(test)]
mod test_support {
    use vnce_core::Buffer;

    /// The rows of a buffer as strings (continuation cells of wide characters are skipped).
    pub fn rows(buffer: &Buffer) -> Vec<String> {
        let size = buffer.size();
        (0..size.height)
            .map(|y| {
                (0..size.width)
                    .filter_map(|x| buffer.cell(x, y))
                    .filter(|cell| !cell.is_continuation())
                    .map(|cell| cell.symbol().to_owned())
                    .collect()
            })
            .collect()
    }
}
