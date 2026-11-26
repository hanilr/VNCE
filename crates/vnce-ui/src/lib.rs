//! # `vnce_ui`
//!
//! A small set of widgets for games and graphics applications in the terminal:
//!
//! - [`Layout`]: splits an area into parts with [`Constraint`]s.
//! - [`Span`], [`Line`] and [`Text`]: styled text; [`Paragraph`] draws it aligned,
//!   wrapped, truncated with `…` or scrolled.
//! - [`Block`]: borders (plain, rounded, double, thick or ASCII), titles and padding.
//! - [`Gauge`]: a progress bar with 1/8-cell precision.
//! - [`List`] with [`ListState`]: selectable rows for menus.
//! - [`Table`] with [`TableState`]: rows in columns, with a header and a selected row.
//! - [`Tabs`]: a row of page titles.
//! - [`TextInput`] with [`TextInputState`]: a one-line text field with a cursor, a
//!   character filter and a length limit, edited by key and paste events.
//! - [`Slider`], [`Checkbox`] and [`Button`]: controls for settings screens and menus.
//! - [`FpsOverlay`]: FPS, UPS, frame time and a sparkline in a corner, for debugging.
//! - [`Clear`]: resets an area, for example under a popup.
//!
//! Widgets implement [`Widget`] (or [`StatefulWidget`] when they keep state between
//! frames) and draw into a [`vnce_core::Buffer`], usually `frame.buffer_mut()`, so they mix
//! freely with pixel graphics and images in the same frame. Styles are patches: a widget
//! drawn on top of another keeps the colors it does not set.
//!
//! For the mouse, [`List::item_at`], [`Table::row_at`], [`Tabs::tab_at`],
//! [`Slider::value_at`] and [`Button::contains`] tell what is under a cell.
//!
//! This crate is part of the **Variation** ecosystem.

pub mod block;
pub mod controls;
pub mod gauge;
pub mod input;
pub mod layout;
pub mod list;
pub mod overlay;
pub mod paragraph;
pub mod table;
pub mod tabs;
pub mod text;
mod widget;

pub use block::{Block, BorderType, Borders, Padding};
pub use controls::{Button, Checkbox, Slider};
pub use gauge::Gauge;
pub use input::{TextInput, TextInputState};
pub use layout::{Constraint, Direction, Layout};
pub use list::{List, ListState};
pub use overlay::{Clear, Corner, FpsOverlay};
pub use paragraph::Paragraph;
pub use table::{Table, TableState};
pub use tabs::Tabs;
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
