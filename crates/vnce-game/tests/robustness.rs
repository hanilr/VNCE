//! Randomized tests: whatever moves, boxes never end up inside a wall.

// Test code converts small random numbers freely.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "random values are reduced to small ranges first"
)]

use vnce_game::{Aabb, move_and_collide};

/// A small deterministic random generator (xorshift64*), so failures can be reproduced.
struct Rng(u64);

impl Rng {
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

    /// A float in `-range..range`.
    fn signed(&mut self, range: f32) -> f32 {
        (self.below(20_001) as f32 / 10_000.0 - 1.0) * range
    }
}

const TILE: f32 = 8.0;
const SIZE: i32 = 24;

#[test]
fn boxes_never_end_inside_solid_tiles() {
    let mut rng = Rng(0x00C0_FFEE);
    for _ in 0..200 {
        // A random map with a solid border.
        let mut solid = vec![false; (SIZE * SIZE) as usize];
        for row in 0..SIZE {
            for column in 0..SIZE {
                let border = row == 0 || column == 0 || row == SIZE - 1 || column == SIZE - 1;
                solid[(row * SIZE + column) as usize] = border || rng.below(5) == 0;
            }
        }
        let is_solid = |column: i32, row: i32| {
            !(0..SIZE).contains(&column)
                || !(0..SIZE).contains(&row)
                || solid[(row * SIZE + column) as usize]
        };
        let overlaps_solid = |aabb: &Aabb| {
            let tile = |v: f32| (v / TILE).floor() as i32;
            let inset = 1e-3;
            (tile(aabb.top() + inset)..=tile(aabb.bottom() - inset)).any(|row| {
                (tile(aabb.left() + inset)..=tile(aabb.right() - inset))
                    .any(|column| is_solid(column, row))
            })
        };
        // Start in a free tile.
        let mut start = None;
        while start.is_none() {
            let (column, row) = (rng.below(SIZE as u64) as i32, rng.below(SIZE as u64) as i32);
            if !is_solid(column, row) {
                start = Some((column, row));
            }
        }
        let (column, row) = start.unwrap();
        let size = 1.0 + rng.below(7) as f32;
        let mut aabb = Aabb::new(column as f32 * TILE, row as f32 * TILE, size, size);
        for _ in 0..100 {
            // Small steps, huge jumps and exact tile multiples.
            let (dx, dy) = match rng.below(3) {
                0 => (rng.signed(4.0), rng.signed(4.0)),
                1 => (rng.signed(400.0), rng.signed(400.0)),
                _ => (
                    (rng.below(9) as f32 - 4.0) * TILE,
                    (rng.below(9) as f32 - 4.0) * TILE,
                ),
            };
            let movement = move_and_collide(aabb, dx, dy, TILE, is_solid);
            assert!(
                !overlaps_solid(&movement.aabb),
                "{aabb:?} moved by ({dx}, {dy}) ended in a wall at {:?}",
                movement.aabb
            );
            aabb = movement.aabb;
        }
    }
}
