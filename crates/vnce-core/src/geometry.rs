//! Geometry types in terminal cell coordinates.
//!
//! The origin `(0, 0)` is the top-left cell. `x` grows to the right, `y` grows downwards.

/// A cell position (column `x`, row `y`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Position {
    /// Column, starting at 0.
    pub x: u16,
    /// Row, starting at 0.
    pub y: u16,
}

impl Position {
    /// The top-left position `(0, 0)`.
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

    /// Creates a new position.
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// A size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Size {
    /// Number of columns.
    pub width: u16,
    /// Number of rows.
    pub height: u16,
}

impl Size {
    /// A size with no cells.
    pub const ZERO: Self = Self {
        width: 0,
        height: 0,
    };

    /// Creates a new size.
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    /// Total number of cells (`width * height`).
    #[must_use]
    pub const fn area(self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Returns `true` if the size contains no cells.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// An axis-aligned rectangle of cells.
///
/// The rectangle covers columns `x..x + width` and rows `y..y + height`.
/// The constructor clamps the size so that the right and bottom edges never overflow `u16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    /// Left column.
    pub x: u16,
    /// Top row.
    pub y: u16,
    /// Number of columns.
    pub width: u16,
    /// Number of rows.
    pub height: u16,
}

impl Rect {
    /// An empty rectangle at the origin.
    pub const ZERO: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    /// Creates a new rectangle, clamping the size so that `x + width` and `y + height`
    /// fit in a `u16`.
    #[must_use]
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        let max_width = u16::MAX - x;
        let max_height = u16::MAX - y;
        Self {
            x,
            y,
            width: if width > max_width { max_width } else { width },
            height: if height > max_height {
                max_height
            } else {
                height
            },
        }
    }

    /// The top-left position.
    #[must_use]
    pub const fn position(self) -> Position {
        Position::new(self.x, self.y)
    }

    /// The size of the rectangle.
    #[must_use]
    pub const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Total number of cells.
    #[must_use]
    pub const fn area(self) -> usize {
        self.size().area()
    }

    /// Returns `true` if the rectangle contains no cells.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.size().is_empty()
    }

    /// Left column (inclusive).
    #[must_use]
    pub const fn left(self) -> u16 {
        self.x
    }

    /// Right column (exclusive).
    #[must_use]
    pub const fn right(self) -> u16 {
        self.x + self.width
    }

    /// Top row (inclusive).
    #[must_use]
    pub const fn top(self) -> u16 {
        self.y
    }

    /// Bottom row (exclusive).
    #[must_use]
    pub const fn bottom(self) -> u16 {
        self.y + self.height
    }

    /// Returns `true` if `position` lies inside the rectangle.
    #[must_use]
    pub const fn contains(self, position: Position) -> bool {
        position.x >= self.left()
            && position.x < self.right()
            && position.y >= self.top()
            && position.y < self.bottom()
    }

    /// The overlapping part of two rectangles, or an empty rectangle if they do not overlap.
    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if left >= right || top >= bottom {
            return Self::new(left, top, 0, 0);
        }
        Self::new(left, top, right - left, bottom - top)
    }

    /// Returns `true` if the two rectangles share at least one cell.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        !self.intersection(other).is_empty()
    }

    /// Shrinks the rectangle by `horizontal` columns on the left and right and by
    /// `vertical` rows on the top and bottom.
    #[must_use]
    pub fn inner(self, horizontal: u16, vertical: u16) -> Self {
        let width = self.width.saturating_sub(horizontal.saturating_mul(2));
        let height = self.height.saturating_sub(vertical.saturating_mul(2));
        if width == 0 || height == 0 {
            return Self::new(self.x, self.y, 0, 0);
        }
        Self::new(self.x + horizontal, self.y + vertical, width, height)
    }

    /// A rectangle of `width` × `height` centered inside `self` (for popups), shrunk to
    /// fit when it is larger. When the free space is odd, the extra column or row is on the
    /// right or bottom.
    #[must_use]
    pub const fn centered(self, width: u16, height: u16) -> Self {
        let width = if width > self.width {
            self.width
        } else {
            width
        };
        let height = if height > self.height {
            self.height
        } else {
            height
        };
        Self::new(
            self.x + (self.width - width) / 2,
            self.y + (self.height - height) / 2,
            width,
            height,
        )
    }
}

impl From<Size> for Rect {
    fn from(size: Size) -> Self {
        Self::new(0, 0, size.width, size.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_new_clamps_to_u16() {
        let rect = Rect::new(u16::MAX - 2, 10, 100, 5);
        assert_eq!(rect.width, 2);
        assert_eq!(rect.right(), u16::MAX);
    }

    #[test]
    fn rect_contains_is_half_open() {
        let rect = Rect::new(2, 3, 4, 5);
        assert!(rect.contains(Position::new(2, 3)));
        assert!(rect.contains(Position::new(5, 7)));
        assert!(!rect.contains(Position::new(6, 7)));
        assert!(!rect.contains(Position::new(5, 8)));
        assert!(!rect.contains(Position::new(1, 3)));
    }

    #[test]
    fn rect_intersection() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersection(b), Rect::new(5, 5, 5, 5));
        assert!(a.intersects(b));

        let c = Rect::new(20, 20, 2, 2);
        assert!(a.intersection(c).is_empty());
        assert!(!a.intersects(c));
    }

    #[test]
    fn rect_inner() {
        let rect = Rect::new(1, 1, 10, 6);
        assert_eq!(rect.inner(1, 1), Rect::new(2, 2, 8, 4));
        assert!(rect.inner(5, 0).is_empty());
    }

    #[test]
    fn rect_centered() {
        let rect = Rect::new(2, 1, 10, 7);
        assert_eq!(rect.centered(4, 3), Rect::new(5, 3, 4, 3));
        // Odd free space: the extra column / row goes to the right / bottom.
        assert_eq!(rect.centered(5, 2), Rect::new(4, 3, 5, 2));
        // Too large: shrunk to the rectangle.
        assert_eq!(rect.centered(50, 50), rect);
    }

    #[test]
    fn size_area() {
        assert_eq!(Size::new(80, 24).area(), 1920);
        assert!(Size::new(0, 24).is_empty());
    }
}
