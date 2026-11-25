//! The island: generated from a seed, with height levels, stairs, plants and rocks, the
//! crystals and the beacon; the walking rules and A* path finding on its tiles.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use vnce::vnce_game::Rng;

/// Tiles per side, with a ring of open sea around the land.
pub const SIZE: usize = 40;
/// The highest level; level 0 is the sea.
pub const MAX_LEVEL: u8 = 5;
/// Light crystals to collect.
pub const CRYSTALS: usize = 7;

/// A direction on the tile grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    /// The step to the neighbor in this direction.
    pub fn step(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }
}

/// What a tile is covered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    Water,
    Sand,
    Grass,
    Meadow,
    Rock,
}

/// Something standing on a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prop {
    Palm,
    Tree,
    Pine,
    Boulder,
    Bush,
    Flowers,
    Beacon,
}

impl Prop {
    /// Whether the prop blocks walking.
    pub fn blocks(self) -> bool {
        !matches!(self, Self::Bush | Self::Flowers)
    }

    /// Whether the prop is tall enough to hide what stands behind it.
    pub fn is_tall(self) -> bool {
        matches!(self, Self::Palm | Self::Tree | Self::Pine | Self::Beacon)
    }
}

pub struct Island {
    pub seed: u64,
    /// Level of every tile, row by row (0: sea).
    pub level: Vec<u8>,
    pub ground: Vec<Ground>,
    /// Stairs climb one level towards their direction.
    pub stair: Vec<Option<Dir>>,
    pub prop: Vec<Option<Prop>>,
    pub crystals: Vec<(usize, usize)>,
    pub beacon: (usize, usize),
    pub start: (usize, usize),
}

/// The index of tile `(x, y)`.
pub fn index(x: usize, y: usize) -> usize {
    y * SIZE + x
}

/// The tile `(x, y)` moved by `step`, if it is on the map.
fn neighbor((x, y): (usize, usize), (dx, dy): (i32, i32)) -> Option<(usize, usize)> {
    let nx = usize::try_from(x as i32 + dx).ok()?;
    let ny = usize::try_from(y as i32 + dy).ok()?;
    (nx < SIZE && ny < SIZE).then_some((nx, ny))
}

impl Island {
    /// A new island for `seed`; the same seed always gives the same island.
    pub fn generate(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let level = heights(&mut rng);
        let mut island = Self {
            seed,
            ground: vec![Ground::Water; SIZE * SIZE],
            stair: vec![None; SIZE * SIZE],
            prop: vec![None; SIZE * SIZE],
            level,
            crystals: Vec::new(),
            beacon: (0, 0),
            start: (0, 0),
        };
        island.keep_largest_land();
        island.start = island.landing();
        island.connect(&mut rng);
        island.paint(&mut rng);
        island.place_beacon();
        island.place_props(&mut rng);
        island.place_crystals(&mut rng);
        island
    }

    pub fn level_at(&self, (x, y): (usize, usize)) -> u8 {
        self.level[index(x, y)]
    }

    pub fn is_land(&self, (x, y): (usize, usize)) -> bool {
        self.level[index(x, y)] > 0
    }

    /// The levels a walker can stand at on a tile: its own, and one more on stairs.
    fn heights_of(&self, tile: (usize, usize)) -> (u8, u8) {
        let level = self.level_at(tile);
        if self.stair[index(tile.0, tile.1)].is_some() {
            (level, level + 1)
        } else {
            (level, level)
        }
    }

    /// Whether a walker can stand on `tile` at all.
    pub fn is_walkable(&self, tile: (usize, usize)) -> bool {
        self.is_land(tile) && !self.prop[index(tile.0, tile.1)].is_some_and(Prop::blocks)
    }

    /// Whether a walker can step from `from` to its neighbor `to`: both walkable, and
    /// sharing a height (stairs connect two levels).
    pub fn can_step(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        if !self.is_walkable(to) || !self.is_land(from) {
            return false;
        }
        let (a_low, a_high) = self.heights_of(from);
        let (b_low, b_high) = self.heights_of(to);
        a_low <= b_high && b_low <= a_high
    }

    /// Whether a walker coming from `from` is blocked by tile `to` (for collision).
    pub fn blocks(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        from != to && !self.can_step(from, to)
    }

    /// The ground height (in levels, fractional on stairs) at world position `(x, y)`.
    pub fn height_at(&self, x: f32, y: f32) -> f32 {
        let tile = (
            (x.max(0.0) as usize).min(SIZE - 1),
            (y.max(0.0) as usize).min(SIZE - 1),
        );
        let level = f32::from(self.level_at(tile));
        match self.stair[index(tile.0, tile.1)] {
            Some(dir) => {
                let (fx, fy) = (x.fract(), y.fract());
                let t = match dir {
                    Dir::North => 1.0 - fy,
                    Dir::South => fy,
                    Dir::East => fx,
                    Dir::West => 1.0 - fx,
                };
                level + t.clamp(0.0, 1.0)
            }
            None => level,
        }
    }

    /// Tiles reachable on foot from `start`.
    fn reachable(&self, start: (usize, usize)) -> Vec<bool> {
        let mut seen = vec![false; SIZE * SIZE];
        let mut queue = VecDeque::from([start]);
        seen[index(start.0, start.1)] = true;
        while let Some(tile) = queue.pop_front() {
            for dir in Dir::ALL {
                if let Some(next) = neighbor(tile, dir.step()) {
                    let i = index(next.0, next.1);
                    if !seen[i] && self.can_step(tile, next) {
                        seen[i] = true;
                        queue.push_back(next);
                    }
                }
            }
        }
        seen
    }

    /// Turns every piece of land but the largest into sea.
    fn keep_largest_land(&mut self) {
        let mut label = vec![usize::MAX; SIZE * SIZE];
        let mut sizes = Vec::new();
        for start in 0..SIZE * SIZE {
            if self.level[start] == 0 || label[start] != usize::MAX {
                continue;
            }
            let id = sizes.len();
            let mut count = 0;
            let mut queue = VecDeque::from([(start % SIZE, start / SIZE)]);
            label[start] = id;
            while let Some(tile) = queue.pop_front() {
                count += 1;
                for dir in Dir::ALL {
                    if let Some(next) = neighbor(tile, dir.step()) {
                        let i = index(next.0, next.1);
                        if self.level[i] > 0 && label[i] == usize::MAX {
                            label[i] = id;
                            queue.push_back(next);
                        }
                    }
                }
            }
            sizes.push(count);
        }
        let largest = (0..sizes.len()).max_by_key(|&id| sizes[id]).unwrap_or(0);
        for (level, &id) in self.level.iter_mut().zip(&label) {
            if id != largest {
                *level = 0;
            }
        }
    }

    /// The landing beach: the lowest land tile nearest to the south-east (the side the
    /// first camera looks from).
    fn landing(&self) -> (usize, usize) {
        let mut best = (SIZE / 2, SIZE / 2);
        let mut best_score = i64::MIN;
        for y in 0..SIZE {
            for x in 0..SIZE {
                if self.level_at((x, y)) == 1 {
                    let score = (x + y) as i64 * 4 - (x as i64 - y as i64).abs() * 3;
                    if score > best_score {
                        best_score = score;
                        best = (x, y);
                    }
                }
            }
        }
        best
    }

    /// Adds stairs (or lowers cliffs) until every piece of land can be reached on foot.
    fn connect(&mut self, rng: &mut Rng) {
        for _ in 0..3000 {
            let seen = self.reachable(self.start);
            // Pairs of a reached tile and an unreached neighbor of another level.
            let mut stairs = Vec::new();
            let mut cliffs = Vec::new();
            for y in 0..SIZE {
                for x in 0..SIZE {
                    if !seen[index(x, y)] {
                        continue;
                    }
                    for dir in Dir::ALL {
                        let Some(next) = neighbor((x, y), dir.step()) else {
                            continue;
                        };
                        if !self.is_land(next) || seen[index(next.0, next.1)] {
                            continue;
                        }
                        let (a, b) = (self.level_at((x, y)), self.level_at(next));
                        if a.abs_diff(b) == 1 {
                            // The stair goes on the lower tile, climbing towards the higher.
                            let (low, toward) = if a < b {
                                ((x, y), dir)
                            } else {
                                (next, opposite(dir))
                            };
                            if self.stair_fits(low, toward) {
                                stairs.push((low, toward));
                            } else {
                                cliffs.push(((x, y), next));
                            }
                        } else {
                            cliffs.push(((x, y), next));
                        }
                    }
                }
            }
            if let Some(&(tile, dir)) = rng.pick(&stairs) {
                self.stair[index(tile.0, tile.1)] = Some(dir);
            } else if let Some(&(from, to)) = rng.pick(&cliffs) {
                // No stair fits: the higher side comes one level closer.
                let (a, b) = (self.level_at(from), self.level_at(to));
                let i = if a > b {
                    index(from.0, from.1)
                } else {
                    index(to.0, to.1)
                };
                self.level[i] -= 1;
            } else {
                return;
            }
        }
    }

    /// A stair on `tile` climbing towards `dir` needs a free tile of its own level behind
    /// it, no higher ground beside it (its sides stay open), and must not sit next to
    /// another stair.
    fn stair_fits(&self, tile: (usize, usize), dir: Dir) -> bool {
        let level = self.level_at(tile);
        let open_sides = Dir::ALL
            .iter()
            .filter(|&&d| d != dir && d != opposite(dir))
            .all(|d| neighbor(tile, d.step()).is_none_or(|n| self.level_at(n) <= level));
        if !open_sides {
            return false;
        }
        let behind = neighbor(tile, opposite(dir).step());
        behind.is_some_and(|b| self.level_at(b) == level && self.stair[index(b.0, b.1)].is_none())
            && self.stair[index(tile.0, tile.1)].is_none()
            && Dir::ALL.iter().all(|d| {
                neighbor(tile, d.step()).is_none_or(|n| self.stair[index(n.0, n.1)].is_none())
            })
    }

    /// Ground by level, with meadows where a second noise is high.
    fn paint(&mut self, rng: &mut Rng) {
        let meadow = value_noise(rng, 6);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = index(x, y);
                self.ground[i] = match self.level[i] {
                    0 => Ground::Water,
                    1 => Ground::Sand,
                    2 | 3 if meadow(x as f32, y as f32) > 0.62 => Ground::Meadow,
                    2 | 3 => Ground::Grass,
                    4 if meadow(x as f32, y as f32) < 0.45 => Ground::Grass,
                    _ => Ground::Rock,
                };
            }
        }
    }

    /// The beacon stands on the highest reachable tile far from the landing.
    fn place_beacon(&mut self) {
        let seen = self.reachable(self.start);
        let mut best = self.start;
        let mut best_score = i64::MIN;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = index(x, y);
                if !seen[i] || self.stair[i].is_some() || (x, y) == self.start {
                    continue;
                }
                let distance = (x.abs_diff(self.start.0) + y.abs_diff(self.start.1)) as i64;
                let score = i64::from(self.level[i]) * 100 + distance;
                if score > best_score && self.keeps_connected((x, y)) {
                    best_score = score;
                    best = (x, y);
                }
            }
        }
        self.beacon = best;
        self.prop[index(best.0, best.1)] = Some(Prop::Beacon);
    }

    /// Whether a blocking prop on `tile` leaves every other reachable tile reachable.
    fn keeps_connected(&mut self, tile: (usize, usize)) -> bool {
        let i = index(tile.0, tile.1);
        let before = self.reachable(self.start).iter().filter(|&&s| s).count();
        let old = self.prop[i].replace(Prop::Boulder);
        let after = self.reachable(self.start).iter().filter(|&&s| s).count();
        self.prop[i] = old;
        after + 1 == before
    }

    /// Trees, rocks and plants by ground; blocking ones only where they cut no path.
    fn place_props(&mut self, rng: &mut Rng) {
        let forest = value_noise(rng, 5);
        let mut tiles: Vec<(usize, usize)> = (0..SIZE * SIZE)
            .map(|i| (i % SIZE, i / SIZE))
            .filter(|&t| self.is_land(t))
            .collect();
        rng.shuffle(&mut tiles);
        for tile in tiles {
            let i = index(tile.0, tile.1);
            let near_start = tile.0.abs_diff(self.start.0) + tile.1.abs_diff(self.start.1) < 3;
            let near_beacon = tile.0.abs_diff(self.beacon.0) + tile.1.abs_diff(self.beacon.1) < 2;
            if self.prop[i].is_some() || self.stair[i].is_some() || near_start || near_beacon {
                continue;
            }
            // Next to a stair, only small plants (the way up stays clear).
            let by_stair = Dir::ALL.iter().any(|d| {
                neighbor(tile, d.step()).is_some_and(|n| self.stair[index(n.0, n.1)].is_some())
            });
            let dense = forest(tile.0 as f32, tile.1 as f32);
            let roll = rng.unit();
            let prop = match self.ground[i] {
                Ground::Sand if roll < 0.07 => Some(Prop::Palm),
                Ground::Sand if roll < 0.10 => Some(Prop::Boulder),
                Ground::Grass | Ground::Meadow if roll < dense * dense * 0.75 => {
                    Some(if self.level[i] >= 3 {
                        Prop::Pine
                    } else {
                        Prop::Tree
                    })
                }
                Ground::Grass if roll < 0.62 && roll > 0.55 => Some(Prop::Bush),
                Ground::Meadow if roll > 0.6 => Some(Prop::Flowers),
                Ground::Rock if roll < 0.16 => Some(Prop::Boulder),
                Ground::Rock if roll < 0.24 => Some(Prop::Pine),
                _ => None,
            };
            let Some(prop) = prop else {
                continue;
            };
            if prop.blocks() && (by_stair || !self.keeps_connected(tile)) {
                continue;
            }
            self.prop[i] = Some(prop);
        }
    }

    /// Crystals on free reachable tiles, spread over the island and its levels.
    fn place_crystals(&mut self, rng: &mut Rng) {
        let seen = self.reachable(self.start);
        let mut free: Vec<(usize, usize)> = (0..SIZE * SIZE)
            .map(|i| (i % SIZE, i / SIZE))
            .filter(|&(x, y)| {
                let i = index(x, y);
                seen[i]
                    && self.prop[i].is_none()
                    && self.stair[i].is_none()
                    && x.abs_diff(self.start.0) + y.abs_diff(self.start.1) > 4
            })
            .collect();
        rng.shuffle(&mut free);
        // Higher tiles first, so the climb is part of the hunt.
        free.sort_by_key(|&t| Reverse(self.level_at(t).min(4)));
        let mut spacing = 7;
        while self.crystals.len() < CRYSTALS && spacing > 0 {
            for &tile in &free {
                if self.crystals.len() == CRYSTALS {
                    break;
                }
                let far = self.crystals.iter().all(|c: &(usize, usize)| {
                    c.0.abs_diff(tile.0) + c.1.abs_diff(tile.1) >= spacing
                });
                if far {
                    self.crystals.push(tile);
                }
            }
            spacing -= 1;
        }
    }

    /// The shortest walk from `from` to `to` (both included), found with A*, or `None`.
    pub fn path(&self, from: (usize, usize), to: (usize, usize)) -> Option<Vec<(usize, usize)>> {
        if !self.is_walkable(to) {
            return None;
        }
        let estimate = |t: (usize, usize)| (t.0.abs_diff(to.0) + t.1.abs_diff(to.1)) as u32;
        let mut cost = vec![u32::MAX; SIZE * SIZE];
        let mut came = vec![usize::MAX; SIZE * SIZE];
        let mut open = BinaryHeap::new();
        cost[index(from.0, from.1)] = 0;
        open.push(Reverse((estimate(from), 0_u32, from)));
        while let Some(Reverse((_, so_far, tile))) = open.pop() {
            if tile == to {
                let mut path = vec![to];
                let mut at = index(to.0, to.1);
                while came[at] != usize::MAX {
                    at = came[at];
                    path.push((at % SIZE, at / SIZE));
                }
                path.reverse();
                return Some(path);
            }
            if so_far > cost[index(tile.0, tile.1)] {
                continue;
            }
            for dir in Dir::ALL {
                let Some(next) = neighbor(tile, dir.step()) else {
                    continue;
                };
                if !self.can_step(tile, next) {
                    continue;
                }
                let i = index(next.0, next.1);
                let step = so_far + 1;
                if step < cost[i] {
                    cost[i] = step;
                    came[i] = index(tile.0, tile.1);
                    open.push(Reverse((step + estimate(next), step, next)));
                }
            }
        }
        None
    }
}

pub fn opposite(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::South,
        Dir::East => Dir::West,
        Dir::South => Dir::North,
        Dir::West => Dir::East,
    }
}

/// Smooth noise in 0..1 with features about `cell` tiles apart.
fn value_noise(rng: &mut Rng, cell: usize) -> impl Fn(f32, f32) -> f32 + use<> {
    let side = SIZE / cell + 2;
    let lattice: Vec<f32> = (0..side * side).map(|_| rng.unit()).collect();
    move |x, y| {
        let (gx, gy) = (x / cell as f32, y / cell as f32);
        let (ix, iy) = (gx as usize, gy as usize);
        let (fx, fy) = (gx.fract(), gy.fract());
        let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
        let (sx, sy) = (smooth(fx), smooth(fy));
        let at = |x: usize, y: usize| lattice[(y.min(side - 1)) * side + x.min(side - 1)];
        let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * sx;
        let bottom = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * sx;
        top + (bottom - top) * sy
    }
}

/// Levels from two octaves of noise under a round falloff: high in the middle, sea at the
/// edges.
fn heights(rng: &mut Rng) -> Vec<u8> {
    let coarse = value_noise(rng, 9);
    let fine = value_noise(rng, 4);
    let center = SIZE as f32 / 2.0;
    let radius = center - 3.0;
    let mut level = vec![0; SIZE * SIZE];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((fx - center).powi(2) + (fy - center).powi(2)).sqrt() / radius;
            let n = coarse(fx, fy) * 0.7 + fine(fx, fy) * 0.3;
            let h = (1.0 - d * d) * 1.25 + (n - 0.5) * 1.1;
            let edge = x < 3 || y < 3 || x >= SIZE - 3 || y >= SIZE - 3;
            level[index(x, y)] = if edge || h <= 0.18 {
                0
            } else {
                ((h - 0.18) * 4.2)
                    .floor()
                    .clamp(0.0, f32::from(MAX_LEVEL - 1)) as u8
                    + 1
            };
        }
    }
    level
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn islands_are_the_same_for_a_seed_and_fully_reachable() {
        for seed in 0..40 {
            let island = Island::generate(seed);
            let again = Island::generate(seed);
            assert_eq!(island.level, again.level);
            assert_eq!(island.prop, again.prop);
            let seen = island.reachable(island.start);
            let land = (0..SIZE * SIZE).filter(|&i| island.level[i] > 0).count();
            assert!(land > 250, "seed {seed}: only {land} land tiles");
            for (i, &reached) in seen.iter().enumerate() {
                let tile = (i % SIZE, i / SIZE);
                if island.is_walkable(tile) {
                    assert!(reached, "seed {seed}: {tile:?} cannot be reached");
                }
            }
            assert_eq!(island.crystals.len(), CRYSTALS, "seed {seed}");
            assert!(
                island.level_at(island.beacon) >= 3,
                "seed {seed}: low beacon"
            );
            // The beacon can be walked up to.
            let beside = Dir::ALL
                .iter()
                .any(|d| neighbor(island.beacon, d.step()).is_some_and(|n| seen[index(n.0, n.1)]));
            assert!(beside, "seed {seed}");
        }
    }

    #[test]
    fn paths_follow_the_walking_rules() {
        let island = Island::generate(7);
        for &crystal in &island.crystals {
            let path = island
                .path(island.start, crystal)
                .expect("every crystal is reachable");
            assert_eq!(path.first(), Some(&island.start));
            assert_eq!(path.last(), Some(&crystal));
            for pair in path.windows(2) {
                assert!(island.can_step(pair[0], pair[1]), "{pair:?}");
            }
        }
        assert_eq!(island.path(island.start, (0, 0)), None, "the sea");
    }

    #[test]
    fn stairs_climb_one_level() {
        let island = (0..20)
            .map(Island::generate)
            .find(|island| island.stair.iter().any(Option::is_some))
            .expect("some island has stairs");
        let i = island.stair.iter().position(Option::is_some).unwrap();
        let tile = (i % SIZE, i / SIZE);
        let dir = island.stair[i].unwrap();
        let above = neighbor(tile, dir.step()).unwrap();
        assert_eq!(island.level_at(above), island.level_at(tile) + 1);
        assert!(island.can_step(tile, above));
        let (cx, cy) = (tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
        let middle = island.height_at(cx, cy);
        assert!((middle - (f32::from(island.level_at(tile)) + 0.5)).abs() < 0.01);
    }
}
