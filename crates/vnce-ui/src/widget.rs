//! The [`Widget`] and [`StatefulWidget`] traits.

use vnce_core::{Buffer, Rect};

/// Something that draws itself into an area of a [`Buffer`].
///
/// Widgets are immediate mode: build them (cheaply) and render them on every frame.
/// Drawing outside `area` or outside the buffer never happens.
///
/// ```
/// use vnce_core::{Buffer, Rect, Size, Style};
/// use vnce_ui::Widget;
///
/// /// Fills its area with a character.
/// struct Fill(char);
///
/// impl Widget for Fill {
///     fn render(&self, area: Rect, buffer: &mut Buffer) {
///         let symbol = self.0.to_string();
///         for y in area.top()..area.bottom() {
///             for x in area.left()..area.right() {
///                 buffer.set_symbol(x, y, &symbol, Style::new());
///             }
///         }
///     }
/// }
///
/// let mut buffer = Buffer::empty(Size::new(4, 2));
/// Fill('#').render(Rect::new(1, 0, 2, 1), &mut buffer);
/// assert_eq!(buffer.cell(1, 0).unwrap().symbol(), "#");
/// assert_eq!(buffer.cell(0, 0).unwrap().symbol(), " ");
/// ```
pub trait Widget {
    /// Draws the widget into `area` of `buffer`.
    fn render(&self, area: Rect, buffer: &mut Buffer);
}

/// A widget whose state outlives a frame (for example the selected row of a list).
pub trait StatefulWidget {
    /// The state kept between frames.
    type State;

    /// Draws the widget into `area` of `buffer`, reading and updating `state` (for
    /// example to keep the selected row visible).
    fn render(&self, area: Rect, buffer: &mut Buffer, state: &mut Self::State);
}

impl<W: Widget + ?Sized> Widget for &W {
    fn render(&self, area: Rect, buffer: &mut Buffer) {
        (**self).render(area, buffer);
    }
}
