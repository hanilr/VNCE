//! Collision: [`Aabb`] boxes, [`move_and_collide`] through solid tiles and a
//! [`SpatialGrid`] for many objects.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// An axis-aligned box in world coordinates (y grows downwards, like the screen).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Aabb {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width (not negative).
    pub width: f32,
    /// Height (not negative).
    pub height: f32,
}

impl Aabb {
    /// A box from its top-left corner and size.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// A box from its center and size.
    #[must_use]
    pub fn from_center(cx: f32, cy: f32, width: f32, height: f32) -> Self {
        Self::new(cx - width / 2.0, cy - height / 2.0, width, height)
    }

    /// Left edge.
    #[must_use]
    pub fn left(&self) -> f32 {
        self.x
    }

    /// Right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Top edge.
    #[must_use]
    pub fn top(&self) -> f32 {
        self.y
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// The center.
    #[must_use]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// The box moved by `(dx, dy)`.
    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    /// Returns `true` if the boxes overlap. Boxes that only touch do not.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    /// Returns `true` if the point is inside (left and top edges included, right and
    /// bottom edges excluded).
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left() && x < self.right() && y >= self.top() && y < self.bottom()
    }

    /// The overlapping part of the boxes, if they overlap.
    #[must_use]
    pub fn overlap(&self, other: &Self) -> Option<Self> {
        if !self.intersects(other) {
            return None;
        }
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Some(Self::new(left, top, right - left, bottom - top))
    }

    /// The shortest move `(dx, dy)` that takes `self` out of `other`, along one axis, or
    /// `None` if they do not overlap. Push `self` by it to resolve a collision.
    #[must_use]
    pub fn penetration(&self, other: &Self) -> Option<(f32, f32)> {
        let overlap = self.overlap(other)?;
        let (cx, cy) = self.center();
        let (ox, oy) = other.center();
        if overlap.width < overlap.height {
            Some((
                if cx < ox {
                    -overlap.width
                } else {
                    overlap.width
                },
                0.0,
            ))
        } else {
            Some((
                0.0,
                if cy < oy {
                    -overlap.height
                } else {
                    overlap.height
                },
            ))
        }
    }
}

/// Which sides of a moving box hit something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(clippy::struct_excessive_bools, reason = "one flag per side of a box")]
pub struct Sides {
    /// Hit something on the left (moving left).
    pub left: bool,
    /// Hit something on the right (moving right).
    pub right: bool,
    /// Hit a ceiling (moving up).
    pub top: bool,
    /// Landed on a floor (moving down).
    pub bottom: bool,
}

impl Sides {
    /// Returns `true` if any side hit something.
    #[must_use]
    pub fn any(&self) -> bool {
        self.left || self.right || self.top || self.bottom
    }
}

/// The result of [`move_and_collide`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Movement {
    /// The box after moving.
    pub aabb: Aabb,
    /// The sides that hit a solid tile.
    pub hit: Sides,
}

/// Distance kept from tile edges when checking which tiles a box covers, so a box that
/// exactly touches a tile does not count as inside it.
const EPSILON: f32 = 1e-4;

/// Moves `aabb` by `(dx, dy)` through a grid of square tiles of `tile_size`, stopping at
/// solid tiles: first along x, then along y. `is_solid(column, row)` tells which tiles are
/// solid (return `true` outside the map to keep objects inside).
///
/// Every tile on the way is checked, so even very fast objects never pass through walls,
/// and tiles the box already overlaps are ignored, so a box that starts inside a wall can
/// move out of it.
///
/// ```
/// use vnce_game::{Aabb, move_and_collide};
///
/// // A floor of solid tiles at row 2 (tiles of 8 pixels).
/// let solid = |_column: i32, row: i32| row == 2;
/// let player = Aabb::new(0.0, 0.0, 6.0, 6.0);
/// let movement = move_and_collide(player, 3.0, 50.0, 8.0, solid);
/// assert!(movement.hit.bottom);
/// assert_eq!(movement.aabb, Aabb::new(3.0, 10.0, 6.0, 6.0));
/// ```
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "tile indices of game worlds are far below 2^23"
)]
pub fn move_and_collide(
    aabb: Aabb,
    dx: f32,
    dy: f32,
    tile_size: f32,
    is_solid: impl Fn(i32, i32) -> bool,
) -> Movement {
    let mut hit = Sides::default();
    let mut aabb = aabb;
    if tile_size <= 0.0 || !tile_size.is_finite() {
        return Movement {
            aabb: aabb.translated(dx, dy),
            hit,
        };
    }
    let tile = |v: f32| (v / tile_size).floor() as i32;

    // Horizontal.
    if dx != 0.0 && dx.is_finite() {
        let rows = tile(aabb.top() + EPSILON)..=tile(aabb.bottom() - EPSILON);
        let blocked = |column: i32| rows.clone().any(|row| is_solid(column, row));
        if dx > 0.0 {
            let first = tile(aabb.right() - EPSILON) + 1;
            let last = tile(aabb.right() + dx - EPSILON);
            match (first..=last).find(|&c| blocked(c)) {
                Some(column) => {
                    aabb.x = column as f32 * tile_size - aabb.width;
                    hit.right = true;
                }
                None => aabb.x += dx,
            }
        } else {
            let first = tile(aabb.left() + EPSILON) - 1;
            let last = tile(aabb.left() + dx + EPSILON);
            match (last..=first).rev().find(|&c| blocked(c)) {
                Some(column) => {
                    aabb.x = (column + 1) as f32 * tile_size;
                    hit.left = true;
                }
                None => aabb.x += dx,
            }
        }
    }

    // Vertical, with the new horizontal position.
    if dy != 0.0 && dy.is_finite() {
        let columns = tile(aabb.left() + EPSILON)..=tile(aabb.right() - EPSILON);
        let blocked = |row: i32| columns.clone().any(|column| is_solid(column, row));
        if dy > 0.0 {
            let first = tile(aabb.bottom() - EPSILON) + 1;
            let last = tile(aabb.bottom() + dy - EPSILON);
            match (first..=last).find(|&r| blocked(r)) {
                Some(row) => {
                    aabb.y = row as f32 * tile_size - aabb.height;
                    hit.bottom = true;
                }
                None => aabb.y += dy,
            }
        } else {
            let first = tile(aabb.top() + EPSILON) - 1;
            let last = tile(aabb.top() + dy + EPSILON);
            match (last..=first).rev().find(|&r| blocked(r)) {
                Some(row) => {
                    aabb.y = (row + 1) as f32 * tile_size;
                    hit.top = true;
                }
                None => aabb.y += dy,
            }
        }
    }
    Movement { aabb, hit }
}

/// A uniform grid that finds the objects near a box quickly, for collision between many
/// objects. Rebuild it (clear, then insert) when objects move.
///
/// ```
/// use vnce_game::{Aabb, SpatialGrid};
///
/// let mut grid = SpatialGrid::new(16.0);
/// grid.insert(1, Aabb::new(0.0, 0.0, 4.0, 4.0));
/// grid.insert(2, Aabb::new(100.0, 100.0, 4.0, 4.0));
/// assert_eq!(grid.query(&Aabb::new(2.0, 2.0, 4.0, 4.0)), [1]);
/// ```
#[derive(Debug, Clone)]
pub struct SpatialGrid<T> {
    cell_size: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
    items: Vec<(T, Aabb)>,
}

impl<T: Copy + Eq + Hash> SpatialGrid<T> {
    /// An empty grid. `cell_size` should be about the size of a typical object.
    #[must_use]
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size: if cell_size > 0.0 && cell_size.is_finite() {
                cell_size
            } else {
                1.0
            },
            cells: HashMap::new(),
            items: Vec::new(),
        }
    }

    /// Removes every object.
    pub fn clear(&mut self) {
        self.cells.clear();
        self.items.clear();
    }

    /// Number of objects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the grid holds no objects.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Adds an object with its box.
    pub fn insert(&mut self, id: T, aabb: Aabb) {
        let index = self.items.len();
        self.items.push((id, aabb));
        for cell in self.cells_of(&aabb) {
            self.cells.entry(cell).or_default().push(index);
        }
    }

    /// The objects whose boxes intersect `aabb`, each once, in insertion order.
    #[must_use]
    pub fn query(&self, aabb: &Aabb) -> Vec<T> {
        let mut seen = HashSet::new();
        let mut found: Vec<usize> = self
            .cells_of(aabb)
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .copied()
            .filter(|&index| seen.insert(index) && self.items[index].1.intersects(aabb))
            .collect();
        found.sort_unstable();
        found.into_iter().map(|index| self.items[index].0).collect()
    }

    /// Every pair of objects whose boxes intersect, each pair once.
    #[must_use]
    pub fn pairs(&self) -> Vec<(T, T)> {
        let mut seen = HashSet::new();
        let mut pairs = Vec::new();
        for indices in self.cells.values() {
            for (n, &a) in indices.iter().enumerate() {
                for &b in &indices[n + 1..] {
                    let key = (a.min(b), a.max(b));
                    if seen.insert(key) && self.items[a].1.intersects(&self.items[b].1) {
                        pairs.push(key);
                    }
                }
            }
        }
        pairs.sort_unstable();
        pairs
            .into_iter()
            .map(|(a, b)| (self.items[a].0, self.items[b].0))
            .collect()
    }

    /// The grid cells covered by `aabb` (at most 4096, so huge boxes stay cheap).
    #[allow(
        clippy::cast_possible_truncation,
        reason = "cell indices of game worlds are small"
    )]
    fn cells_of(&self, aabb: &Aabb) -> impl Iterator<Item = (i32, i32)> + use<T> {
        let cell = |v: f32| (v / self.cell_size).floor() as i32;
        let (x0, y0) = (cell(aabb.left()), cell(aabb.top()));
        let x1 = cell(aabb.right()).min(x0.saturating_add(63));
        let y1 = cell(aabb.bottom()).min(y0.saturating_add(63));
        (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_boxes_do_not_intersect() {
        let a = Aabb::new(0.0, 0.0, 10.0, 10.0);
        assert!(!a.intersects(&Aabb::new(10.0, 0.0, 5.0, 5.0)));
        assert!(a.intersects(&Aabb::new(9.5, 9.5, 5.0, 5.0)));
        assert!(a.contains(0.0, 0.0) && !a.contains(10.0, 5.0));
        assert_eq!(
            a.overlap(&Aabb::new(8.0, 6.0, 5.0, 5.0)),
            Some(Aabb::new(8.0, 6.0, 2.0, 4.0))
        );
        assert_eq!(
            Aabb::from_center(5.0, 5.0, 4.0, 2.0),
            Aabb::new(3.0, 4.0, 4.0, 2.0)
        );
    }

    #[test]
    fn penetration_uses_the_shortest_axis() {
        let wall = Aabb::new(10.0, 0.0, 10.0, 100.0);
        let player = Aabb::new(7.0, 20.0, 5.0, 5.0);
        assert_eq!(player.penetration(&wall), Some((-2.0, 0.0)));
        let floor = Aabb::new(0.0, 10.0, 100.0, 10.0);
        let falling = Aabb::new(20.0, 7.0, 5.0, 5.0);
        assert_eq!(falling.penetration(&floor), Some((0.0, -2.0)));
        assert_eq!(player.penetration(&Aabb::new(50.0, 50.0, 1.0, 1.0)), None);
    }

    /// A box of walls around a 10×10 room (tiles 0 and 11 are walls).
    fn room(column: i32, row: i32) -> bool {
        !(1..=10).contains(&column) || !(1..=10).contains(&row)
    }

    #[test]
    fn stops_at_walls_on_every_side() {
        let start = Aabb::new(40.0, 40.0, 8.0, 8.0);
        let right = move_and_collide(start, 1000.0, 0.0, 8.0, room);
        assert!(right.hit.right && !right.hit.left);
        assert!((right.aabb.right() - 88.0).abs() < 1e-3);
        let left = move_and_collide(start, -1000.0, 0.0, 8.0, room);
        assert!(left.hit.left && (left.aabb.left() - 8.0).abs() < 1e-3);
        let up = move_and_collide(start, 0.0, -1000.0, 8.0, room);
        assert!(up.hit.top && (up.aabb.top() - 8.0).abs() < 1e-3);
        let down = move_and_collide(start, 0.0, 1000.0, 8.0, room);
        assert!(down.hit.bottom && (down.aabb.bottom() - 88.0).abs() < 1e-3);
        // Free movement.
        let free = move_and_collide(start, 3.0, -2.0, 8.0, room);
        assert!(!free.hit.any());
        assert_eq!(free.aabb, start.translated(3.0, -2.0));
    }

    #[test]
    fn sliding_along_a_floor_and_exact_contact() {
        // Standing exactly on the floor (row 11 is solid): moving sideways is free.
        let standing = Aabb::new(16.0, 80.0, 8.0, 8.0);
        let slide = move_and_collide(standing, 20.0, 0.0, 8.0, room);
        assert!(!slide.hit.any());
        // Gravity keeps it on the floor and reports the landing.
        let fall = move_and_collide(standing, 0.0, 1.0, 8.0, room);
        assert!(fall.hit.bottom);
        assert_eq!(fall.aabb, standing);
    }

    #[test]
    fn corners_are_handled_axis_by_axis() {
        // Moving diagonally into the bottom-right corner stops on both axes.
        let start = Aabb::new(70.0, 70.0, 8.0, 8.0);
        let movement = move_and_collide(start, 50.0, 50.0, 8.0, room);
        assert!(movement.hit.right && movement.hit.bottom);
        assert_eq!(movement.aabb, Aabb::new(80.0, 80.0, 8.0, 8.0));
    }

    #[test]
    fn spatial_grid_queries_and_pairs() {
        let mut grid = SpatialGrid::new(10.0);
        assert!(grid.is_empty());
        grid.insert('a', Aabb::new(0.0, 0.0, 15.0, 15.0));
        grid.insert('b', Aabb::new(12.0, 12.0, 5.0, 5.0));
        grid.insert('c', Aabb::new(40.0, 40.0, 5.0, 5.0));
        grid.insert('d', Aabb::new(43.0, 41.0, 5.0, 5.0));
        assert_eq!(grid.len(), 4);
        assert_eq!(grid.query(&Aabb::new(10.0, 10.0, 4.0, 4.0)), ['a', 'b']);
        assert_eq!(grid.pairs(), [('a', 'b'), ('c', 'd')]);
        grid.clear();
        assert_eq!(
            grid.query(&Aabb::new(0.0, 0.0, 100.0, 100.0)),
            Vec::<char>::new()
        );
    }
}
