//! Path finding on a grid with A*: the shortest walk between two cells, for enemies that
//! chase, units sent by a click, or a "walk here" cursor.
//!
//! The grid is described by a callback that gives the cost of a step between neighbor
//! cells (or `None` where the step is not allowed), so it fits tile maps, height maps,
//! one-way doors and terrain that is slower to cross. [`find_on_tilemap`] does it for a
//! [`TileMap`] with solid tiles.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::{TileMap, Tileset};

/// A cell of the grid as `(column, row)`.
pub type Cell = (usize, usize);

/// Which neighbors a step can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Neighbors {
    /// Up, right, down and left.
    #[default]
    Four,
    /// The four sides and the four corners.
    Eight,
}

/// Steps to the neighbors: the four sides first (north, east, south, west), then the
/// corners.
const STEPS: [(isize, isize); 8] = [
    (0, -1),
    (1, 0),
    (0, 1),
    (-1, 0),
    (1, -1),
    (1, 1),
    (-1, 1),
    (-1, -1),
];

/// A path found by [`Pathfinder::find`]: the cells from the start to the goal (both
/// included) and the sum of the step costs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path {
    /// The cells in walking order.
    pub cells: Vec<Cell>,
    /// The total cost.
    pub cost: u32,
}

/// Finds shortest paths on a `width` × `height` grid with A*.
///
/// ```
/// use vnce_game::path::Pathfinder;
///
/// // A wall with a gap at the bottom.
/// let map = ["....#....", "....#....", "........."];
/// let free = |(x, y): (usize, usize)| map[y].as_bytes()[x] == b'.';
/// let path = Pathfinder::new(9, 3)
///     .find((0, 0), (8, 0), |_, to| free(to).then_some(1))
///     .unwrap();
/// assert_eq!(path.cells.first(), Some(&(0, 0)));
/// assert_eq!(path.cells.last(), Some(&(8, 0)));
/// assert_eq!(path.cost, 12); // down 2, across 8, up 2
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pathfinder {
    width: usize,
    height: usize,
    neighbors: Neighbors,
    cut_corners: bool,
    min_cost: u32,
    max_cost: u32,
}

impl Pathfinder {
    /// A path finder for a `width` × `height` grid, with four neighbors and no cost
    /// limit.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            neighbors: Neighbors::Four,
            cut_corners: false,
            min_cost: 1,
            max_cost: u32::MAX,
        }
    }

    /// Which neighbors a step can reach.
    #[must_use]
    pub fn neighbors(mut self, neighbors: Neighbors) -> Self {
        self.neighbors = neighbors;
        self
    }

    /// With [`Neighbors::Eight`]: whether a diagonal step may pass a blocked side cell
    /// (off by default, so paths do not squeeze between two walls touching at a corner).
    #[must_use]
    pub fn cut_corners(mut self, cut: bool) -> Self {
        self.cut_corners = cut;
        self
    }

    /// The smallest cost any step can have (1 by default). The search estimates the rest
    /// of the way with it; a value larger than a real step cost can miss the shortest
    /// path, a smaller one only makes the search slower.
    #[must_use]
    pub fn min_cost(mut self, cost: u32) -> Self {
        self.min_cost = cost;
        self
    }

    /// The most a path may cost; farther goals are not found (and the search stops
    /// early, which keeps it fast on big maps).
    #[must_use]
    pub fn max_cost(mut self, cost: u32) -> Self {
        self.max_cost = cost;
        self
    }

    /// The neighbor of `cell` by `step`, if it is on the grid.
    fn neighbor(&self, (x, y): Cell, (dx, dy): (isize, isize)) -> Option<Cell> {
        let nx = x.checked_add_signed(dx)?;
        let ny = y.checked_add_signed(dy)?;
        (nx < self.width && ny < self.height).then_some((nx, ny))
    }

    /// The cost estimate from `cell` to `goal` (never more than the real cost when every
    /// step costs at least `min_cost`).
    fn estimate(&self, (x, y): Cell, (gx, gy): Cell) -> u32 {
        let (dx, dy) = (x.abs_diff(gx), y.abs_diff(gy));
        let steps = match self.neighbors {
            Neighbors::Four => dx + dy,
            Neighbors::Eight => dx.max(dy),
        };
        u32::try_from(steps)
            .unwrap_or(u32::MAX)
            .saturating_mul(self.min_cost)
    }

    /// The cheapest path from `from` to `to`, or `None` if there is none within the cost
    /// limit. `step(a, b)` gives the cost of stepping from cell `a` to its neighbor `b`,
    /// or `None` if that step is not allowed (a wall, water, a cliff).
    pub fn find(
        &self,
        from: Cell,
        to: Cell,
        mut step: impl FnMut(Cell, Cell) -> Option<u32>,
    ) -> Option<Path> {
        let (w, h) = (self.width, self.height);
        if from.0 >= w || from.1 >= h || to.0 >= w || to.1 >= h {
            return None;
        }
        let index = |(x, y): Cell| y * w + x;
        let mut cost = vec![u32::MAX; w * h];
        let mut came = vec![usize::MAX; w * h];
        let mut open = BinaryHeap::new();
        cost[index(from)] = 0;
        open.push(Reverse((self.estimate(from, to), 0_u32, from)));
        let steps = match self.neighbors {
            Neighbors::Four => &STEPS[..4],
            Neighbors::Eight => &STEPS[..],
        };
        while let Some(Reverse((_, so_far, cell))) = open.pop() {
            if cell == to {
                let mut cells = vec![to];
                let mut at = index(to);
                while came[at] != usize::MAX {
                    at = came[at];
                    cells.push((at % w, at / w));
                }
                cells.reverse();
                return Some(Path {
                    cells,
                    cost: so_far,
                });
            }
            if so_far > cost[index(cell)] {
                continue;
            }
            for &(dx, dy) in steps {
                let Some(next) = self.neighbor(cell, (dx, dy)) else {
                    continue;
                };
                if dx != 0 && dy != 0 && !self.cut_corners {
                    // Both side cells must be open for a diagonal step.
                    let sides = [self.neighbor(cell, (dx, 0)), self.neighbor(cell, (0, dy))];
                    if sides
                        .into_iter()
                        .any(|side| side.is_none_or(|side| step(cell, side).is_none()))
                    {
                        continue;
                    }
                }
                let Some(price) = step(cell, next) else {
                    continue;
                };
                let total = so_far.saturating_add(price);
                if total > self.max_cost {
                    continue;
                }
                let i = index(next);
                if total < cost[i] {
                    cost[i] = total;
                    came[i] = index(cell);
                    let guess = total.saturating_add(self.estimate(next, to));
                    open.push(Reverse((guess, total, next)));
                }
            }
        }
        None
    }
}

/// The shortest path between two cells of a [`TileMap`], walking on tiles that are not
/// solid in any layer (see [`TileMap::is_solid`]), every step costing 1.
#[must_use]
pub fn find_on_tilemap(
    map: &TileMap,
    tileset: &Tileset,
    from: Cell,
    to: Cell,
    neighbors: Neighbors,
) -> Option<Path> {
    let free = |(x, y): Cell| match (i32::try_from(x), i32::try_from(y)) {
        (Ok(x), Ok(y)) => !map.is_solid(tileset, x, y),
        _ => false,
    };
    Pathfinder::new(map.width() as usize, map.height() as usize)
        .neighbors(neighbors)
        .find(from, to, |_, to| free(to).then_some(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> (Pathfinder, impl Fn(Cell) -> bool) {
        let rows: Vec<Vec<u8>> = rows.iter().map(|r| r.as_bytes().to_vec()).collect();
        let finder = Pathfinder::new(rows[0].len(), rows.len());
        (finder, move |(x, y): Cell| rows[y][x] != b'#')
    }

    #[test]
    fn walks_around_walls() {
        let (finder, free) = grid(&["..#..", "..#..", "....."]);
        let path = finder
            .find((0, 0), (4, 0), |_, to| free(to).then_some(1))
            .unwrap();
        assert_eq!(path.cost, 8);
        assert_eq!(path.cells.len(), 9);
        // Every step goes to a side neighbor.
        for pair in path.cells.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert_eq!(a.0.abs_diff(b.0) + a.1.abs_diff(b.1), 1);
            assert!(free(b));
        }
        // Start and goal are the same cell.
        let here = finder.find((1, 1), (1, 1), |_, _| Some(1)).unwrap();
        assert_eq!((here.cells, here.cost), (vec![(1, 1)], 0));
    }

    #[test]
    fn no_way_through_and_off_the_grid() {
        let (finder, free) = grid(&["..#..", "..#..", "..#.."]);
        assert_eq!(
            finder.find((0, 0), (4, 0), |_, to| free(to).then_some(1)),
            None
        );
        assert_eq!(finder.find((0, 0), (9, 0), |_, _| Some(1)), None);
    }

    #[test]
    fn diagonals_and_corners() {
        let (finder, free) = grid(&["...", "...", "..."]);
        let eight = finder.neighbors(Neighbors::Eight);
        let path = eight
            .find((0, 0), (2, 2), |_, to| free(to).then_some(1))
            .unwrap();
        assert_eq!(path.cells, vec![(0, 0), (1, 1), (2, 2)]);
        // Two walls touching at a corner: no squeezing through without cut_corners.
        let (finder, free) = grid(&[".#", "#."]);
        let eight = finder.neighbors(Neighbors::Eight);
        assert_eq!(
            eight.find((0, 0), (1, 1), |_, to| free(to).then_some(1)),
            None
        );
        assert!(
            eight
                .cut_corners(true)
                .find((0, 0), (1, 1), |_, to| free(to).then_some(1))
                .is_some()
        );
    }

    #[test]
    fn costs_and_the_limit() {
        // The middle row is mud (cost 5): going around it is cheaper.
        let rows = ["....", "~~~.", "...."];
        let cost = |(x, y): Cell| if rows[y].as_bytes()[x] == b'~' { 5 } else { 1 };
        let finder = Pathfinder::new(4, 3);
        let path = finder.find((0, 0), (0, 2), |_, to| Some(cost(to))).unwrap();
        assert_eq!(path.cost, 6, "{:?}", path.cells); // straight through the mud
        let around = finder
            .find((0, 0), (0, 2), |_, to| {
                Some(if cost(to) == 5 { 9 } else { 1 })
            })
            .unwrap();
        assert_eq!(around.cost, 8, "around the right end");
        assert!(around.cells.contains(&(3, 1)));
        // Too far for the limit.
        assert_eq!(
            finder
                .max_cost(5)
                .find((0, 0), (0, 2), |_, to| Some(cost(to))),
            None
        );
        // Steps can depend on where they come from (a one-way ledge down).
        let down_only = |a: Cell, b: Cell| (b.1 >= a.1).then_some(1);
        assert!(finder.find((0, 0), (0, 2), down_only).is_some());
        assert_eq!(finder.find((0, 2), (0, 0), down_only), None);
    }

    #[test]
    fn tilemaps() {
        use vnce_raster::Canvas;

        use crate::SpriteSheet;

        let text = "\
.....
.###.
.....";
        let map = TileMap::from_text(text, 4, |c| (c == '#').then_some(0));
        let tileset = Tileset::new(SpriteSheet::new(Canvas::new(4, 4), 4, 4)).with_solid([0]);
        let path = find_on_tilemap(&map, &tileset, (0, 1), (4, 1), Neighbors::Four).unwrap();
        assert_eq!(path.cost, 6);
        // It goes around the wall, never through it.
        assert!(path.cells.iter().all(|&(x, y)| y != 1 || x == 0 || x == 4));
        assert_eq!(
            find_on_tilemap(&map, &tileset, (0, 1), (2, 1), Neighbors::Four),
            None,
            "a wall"
        );
    }
}
