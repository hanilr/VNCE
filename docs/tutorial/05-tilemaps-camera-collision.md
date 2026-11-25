# 5. Tile maps, camera and collision

## Tile maps

Levels are grids of tiles. `TileMap::from_text` reads one row per line and one tile per
character; the legend turns characters into tile numbers (`None` for empty cells). A
`Tileset` holds the tile images (a `SpriteSheet`, frame `n` is tile `n`) and says which
tiles are solid.

```rust
use vnce::vnce_game::{SpriteSheet, TileMap, Tileset};
use vnce::vnce_raster::{Canvas, Rgba};

const TILE: u32 = 8;

let level = "\
##########
#........#
#..o.....#
#....##..#
##########";
let map = TileMap::from_text(level, TILE, |c| match c {
    '#' => Some(0),
    _ => Some(1),
});
let tiles = Tileset::new(SpriteSheet::from_frames(&[
    Canvas::filled(TILE, TILE, Rgba::rgb(90, 80, 100)), // 0: wall
    Canvas::filled(TILE, TILE, Rgba::rgb(30, 26, 38)),  // 1: floor
]))
.with_solid([0]);

let start = TileMap::find_in_text(level, 'o'); // where the player starts
assert_eq!(start, [(3, 2)]);
assert!(map.is_solid(&tiles, 0, 0));
assert!(!map.is_solid(&tiles, 3, 2));
assert!(map.is_solid(&tiles, -1, 0)); // outside the map counts as solid
```

Maps can have more layers (`add_layer`, then `set`), for example decorations over the
floor. `map.draw(&tiles, &mut canvas, offset)` draws only the visible tiles.

## Moving against walls

Objects are boxes (`Aabb`: x, y, width, height in world pixels). `move_and_collide` moves
a box through the tiles and stops it at solid ones, first along x, then along y; even very
fast objects never pass through a wall. `hit` tells which sides touched something, for
example `hit.bottom` means "standing on the ground".

```rust
use vnce::vnce_game::{Aabb, move_and_collide};

// A wall in column 5 (tiles of 8 pixels).
let solid = |column: i32, _row: i32| column == 5;
let player = Aabb::new(20.0, 8.0, 6.0, 6.0);
let moved = move_and_collide(player, 30.0, 0.0, 8.0, solid);
assert!(moved.hit.right);
assert_eq!(moved.aabb.right(), 40.0); // stopped at the wall
```

With a tile map: `move_and_collide(player, dx, dy, 8.0, |c, r| map.is_solid(&tiles, c, r))`.

## The camera

A `Camera` shows part of a larger world. It follows a target, stays inside the world
bounds and can smooth its movement, ignore small moves (a dead zone) and shake.

```rust
use std::time::Duration;
use vnce::vnce_game::{Aabb, Camera};
use vnce::vnce_raster::Canvas;

let world = Aabb::new(0.0, 0.0, 800.0, 200.0);
let mut camera = Camera::new(160.0, 90.0)
    .with_bounds(world)
    .with_smoothing(0.2) // cover 20 % of the distance every 1/60 s
    .with_dead_zone(24.0, 16.0);

// In `update`: follow the player.
let player = (400.0, 100.0);
for _ in 0..120 {
    camera.update(Duration::from_millis(16), Some(player));
}
// Centered would be (320, 55); the dead zone lets the player stay 12 × 8 pixels off
// center, so the camera stops a little earlier.
assert_eq!(camera.offset(1.0), (308, 47));
camera.shake(3.0, Duration::from_millis(300)); // when the player is hit

// In `render`: draw the world shifted by the camera offset.
let alpha = 1.0; // `ctx.alpha()` in a game
let (ox, oy) = camera.offset(alpha);
let mut view = Canvas::new(160, 90);
// map.draw(&tiles, &mut view, (ox, oy));
// view.blit(&hero, player_x - ox, player_y - oy, BlitOptions::default());
assert_eq!((ox, oy, view.width()), (308, 47, 160)); // the shake starts next update
```

When the terminal is resized, call `camera.set_viewport(width, height)` with the new view
size. `screen_to_world` turns a mouse position into world coordinates.

## Many objects

Checking every enemy against every bullet gets slow. A `SpatialGrid` finds the objects
near a box quickly. Rebuild it every update (clear, insert), then query:

```rust
use vnce::vnce_game::{Aabb, SpatialGrid};

let bats = [Aabb::new(10.0, 10.0, 6.0, 4.0), Aabb::new(300.0, 40.0, 6.0, 4.0)];
let mut grid = SpatialGrid::new(16.0); // about the size of an object
for (index, bat) in bats.iter().enumerate() {
    grid.insert(index, *bat);
}
let player = Aabb::new(12.0, 8.0, 6.0, 6.0);
assert_eq!(grid.query(&player), [0]); // only the first bat touches the player
```

`grid.pairs()` returns every pair of objects that touch, for collisions among objects.

## Random levels

`Rng` is a small seeded random generator: the same seed gives the same numbers, so a level
can be generated again from its seed (and bugs can be reproduced).

```rust
use vnce::vnce_game::Rng;

let mut rng = Rng::new(2024);
let width = 20;
let mut level = String::new();
for _row in 0..5 {
    for _column in 0..width {
        level.push(if rng.chance(0.2) { '#' } else { '.' });
    }
    level.push('\n');
}
assert_eq!(level.lines().count(), 5);

let damage = rng.range(3..7); // 3, 4, 5 or 6
let mut loot = ["gem", "key", "coin"];
rng.shuffle(&mut loot);
let reward = rng.pick(&loot);
assert!((3..7).contains(&damage) && reward.is_some());

// Different numbers on every run:
let _ = Rng::from_time();
```

Next: [Scenes, UI and saving](06-scenes-and-ui.md).
