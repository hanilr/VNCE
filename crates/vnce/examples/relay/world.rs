//! The station: rooms carved out of rock, corridors, doors and the places of every
//! feature. The layout is fixed, so every run shows every room.

use std::time::Duration;

use vnce::vnce_game::{Aabb, TileMap, Tileset, ease};
use vnce::vnce_raster::Rgba;

use crate::art;

/// Size of a tile in world pixels.
pub const TILE: u32 = 8;
/// Size of the station in tiles.
const COLUMNS: u32 = 64;
const ROWS: u32 = 44;

/// Tile ids (see `art::tileset`).
pub mod tiles {
    use vnce::vnce_game::TileId;

    pub const ROCK: TileId = 0;
    pub const WALL_FACE: TileId = 1;
    pub const FLOOR: TileId = 2;
    pub const PLATE: TileId = 3;
    pub const CORRIDOR: TileId = 4;
    pub const MACHINE: TileId = 5;
    pub const SHELF: TileId = 6;
}

/// The rooms of the station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Room {
    Atrium,
    Observatory,
    Reactor,
    Prism,
    SoundLab,
    Archive,
    Corridor,
}

impl Room {
    pub fn name(self) -> &'static str {
        match self {
            Self::Atrium => "ATRIUM",
            Self::Observatory => "OBSERVATORY",
            Self::Reactor => "REACTOR",
            Self::Prism => "PRISM HALL",
            Self::SoundLab => "SOUND LAB",
            Self::Archive => "ARCHIVE",
            Self::Corridor => "CORRIDOR",
        }
    }

    /// What the room shows, for the banner shown when entering it.
    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Atrium => "BRING 5 CORES TO THE RELAY",
            Self::Observatory => "GPU SHADER OR CPU NEBULA",
            Self::Reactor => "3D MESH - MIND THE DRONES",
            Self::Prism => "EVERY PIXEL MODE - PRESS E",
            Self::SoundLab => "REPEAT THE PADS - SYNTH",
            Self::Archive => "WIDGETS AND TEXT - PRESS E",
            Self::Corridor => "",
        }
    }

    /// Color of the room's ceiling lamps.
    pub fn lamp(self) -> Rgba {
        match self {
            Self::Atrium => Rgba::rgb(150, 220, 170),
            Self::Observatory => Rgba::rgb(110, 140, 255),
            Self::Reactor => Rgba::rgb(255, 150, 80),
            Self::Prism => Rgba::rgb(230, 120, 255),
            Self::SoundLab => Rgba::rgb(90, 230, 230),
            Self::Archive => Rgba::rgb(255, 215, 130),
            Self::Corridor => Rgba::rgb(110, 110, 130),
        }
    }
}

/// A rectangle of tiles.
#[derive(Debug, Clone, Copy)]
pub struct TileRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl TileRect {
    const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// The rectangle in world pixels.
    pub fn aabb(self) -> Aabb {
        Aabb::new(
            (self.x * TILE) as f32,
            (self.y * TILE) as f32,
            (self.w * TILE) as f32,
            (self.h * TILE) as f32,
        )
    }

    fn contains(self, column: u32, row: u32) -> bool {
        (self.x..self.x + self.w).contains(&column) && (self.y..self.y + self.h).contains(&row)
    }
}

/// Center of tile `(column, row)` in world pixels.
pub fn tile_center(column: u32, row: u32) -> (f32, f32) {
    (
        (column * TILE) as f32 + TILE as f32 / 2.0,
        (row * TILE) as f32 + TILE as f32 / 2.0,
    )
}

const ROOMS: [(Room, TileRect); 6] = [
    (Room::Prism, TileRect::new(2, 2, 14, 9)),
    (Room::Observatory, TileRect::new(22, 2, 20, 9)),
    (Room::Archive, TileRect::new(2, 16, 14, 12)),
    (Room::Atrium, TileRect::new(24, 16, 16, 12)),
    (Room::Reactor, TileRect::new(48, 15, 14, 14)),
    (Room::SoundLab, TileRect::new(20, 33, 24, 9)),
];

const CORRIDORS: [TileRect; 5] = [
    TileRect::new(16, 6, 6, 2),
    TileRect::new(31, 11, 2, 5),
    TileRect::new(16, 21, 8, 2),
    TileRect::new(40, 21, 8, 2),
    TileRect::new(31, 28, 2, 5),
];

/// A sliding door across a corridor: `vertical` doors close a horizontal corridor.
#[derive(Debug, Clone, Copy)]
pub struct Door {
    /// Center in world pixels.
    pub x: f32,
    pub y: f32,
    pub vertical: bool,
    /// 0 closed, 1 open.
    pub open: f32,
}

impl Door {
    /// Opens when the player is close, closes otherwise. Returns `true` when the door
    /// starts to open (for its sound).
    pub fn update(&mut self, dt: Duration, player: (f32, f32)) -> bool {
        let near = (player.0 - self.x).abs() < 22.0 && (player.1 - self.y).abs() < 22.0;
        let step = dt.as_secs_f32() * 3.5;
        let was_closed = self.open == 0.0;
        self.open = if near {
            (self.open + step).min(1.0)
        } else {
            (self.open - step).max(0.0)
        };
        was_closed && self.open > 0.0
    }

    /// How far each panel has slid, in pixels, with easing.
    pub fn slide(&self) -> f32 {
        ease::in_out_cubic(self.open) * 7.0
    }
}

/// A ceiling lamp: position and color.
#[derive(Debug, Clone, Copy)]
pub struct Lamp {
    pub x: f32,
    pub y: f32,
    pub color: Rgba,
}

/// The station and the place of everything in it.
pub struct Station {
    pub map: TileMap,
    pub tiles: Tileset,
    pub start: (f32, f32),
    /// Where the cores are brought.
    pub relay: Aabb,
    /// The observatory window (shows the nebula).
    pub window: Aabb,
    /// The reactor pit (shows the 3D core).
    pub reactor: Aabb,
    /// The prism screen (opens the pixel mode gallery).
    pub screen: Aabb,
    /// The archive terminal (opens the widget gallery).
    pub terminal: Aabb,
    /// The sound lab's waveform screen.
    pub wave_screen: Aabb,
    /// The sound lab's floor pads.
    pub pads: Vec<Aabb>,
    pub doors: Vec<Door>,
    pub lamps: Vec<Lamp>,
    /// Where each wing's core appears.
    pub cores: Vec<(Room, (f32, f32))>,
    /// Where the drones start (all in the reactor, which they never leave).
    pub drones: Vec<(f32, f32)>,
}

impl Station {
    #[allow(
        clippy::too_many_lines,
        reason = "the whole layout is easier to check in one place"
    )]
    pub fn build() -> Self {
        let mut map = TileMap::new(COLUMNS, ROWS, TILE);
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                let tile = if ROOMS
                    .iter()
                    .any(|&(room, rect)| room == Room::Atrium && rect.contains(column, row))
                {
                    tiles::PLATE
                } else if ROOMS.iter().any(|&(_, rect)| rect.contains(column, row)) {
                    tiles::FLOOR
                } else if CORRIDORS.iter().any(|rect| rect.contains(column, row)) {
                    tiles::CORRIDOR
                } else {
                    tiles::ROCK
                };
                map.set(0, i64::from(column), i64::from(row), Some(tile));
            }
        }
        let window = TileRect::new(25, 2, 14, 3);
        let reactor = TileRect::new(53, 20, 4, 4);
        let screen = TileRect::new(5, 2, 8, 2);
        let terminal = TileRect::new(8, 16, 2, 2);
        // On the south wall, so the corridor from the atrium stays open.
        let wave_screen = TileRect::new(26, 40, 12, 2);
        for rect in [window, reactor, screen, terminal, wave_screen] {
            fill(&mut map, rect, tiles::MACHINE);
        }
        for (x, y) in [(3, 20), (11, 20), (3, 24), (11, 24)] {
            fill(&mut map, TileRect::new(x, y, 4, 1), tiles::SHELF);
        }
        // Rock above floor shows its face.
        for row in 0..ROWS - 1 {
            for column in 0..COLUMNS {
                let (c, r) = (i64::from(column), i64::from(row));
                let below = map.get(0, c, r + 1);
                if map.get(0, c, r) == Some(tiles::ROCK)
                    && matches!(below, Some(tiles::FLOOR | tiles::PLATE | tiles::CORRIDOR))
                {
                    map.set(0, c, r, Some(tiles::WALL_FACE));
                }
            }
        }

        let door = |column: u32, row: u32, vertical: bool| {
            let (x, y) = if vertical {
                ((column * TILE) as f32 + 4.0, ((row + 1) * TILE) as f32)
            } else {
                (((column + 1) * TILE) as f32, (row * TILE) as f32 + 4.0)
            };
            Door {
                x,
                y,
                vertical,
                open: 0.0,
            }
        };
        let doors = vec![
            door(19, 6, true),
            door(31, 13, false),
            door(20, 21, true),
            door(44, 21, true),
            door(31, 30, false),
        ];

        let mut lamps = Vec::new();
        for &(room, rect) in &ROOMS {
            let corners = [
                (rect.x + 2, rect.y + 2),
                (rect.x + rect.w - 3, rect.y + 2),
                (rect.x + 2, rect.y + rect.h - 3),
                (rect.x + rect.w - 3, rect.y + rect.h - 3),
            ];
            for (column, row) in corners {
                let (x, y) = tile_center(column, row);
                lamps.push(Lamp {
                    x,
                    y,
                    color: room.lamp(),
                });
            }
        }
        for rect in CORRIDORS {
            let (x, y) = tile_center(rect.x + rect.w / 2, rect.y + rect.h / 2);
            lamps.push(Lamp {
                x,
                y,
                color: Room::Corridor.lamp(),
            });
        }

        let tiles = art::tileset();
        Self {
            map,
            tiles,
            start: tile_center(32, 25),
            relay: TileRect::new(31, 21, 2, 2).aabb(),
            window: window.aabb(),
            reactor: reactor.aabb(),
            screen: screen.aabb(),
            terminal: terminal.aabb(),
            wave_screen: wave_screen.aabb(),
            pads: (0..8)
                .map(|i| {
                    let (x, y) = tile_center(23 + i * 2, 38);
                    Aabb::from_center(x, y, 7.0, 7.0)
                })
                .collect(),
            doors,
            lamps,
            cores: vec![
                (Room::Observatory, tile_center(40, 9)),
                (Room::Reactor, tile_center(60, 27)),
                (Room::Prism, tile_center(9, 6)),
                (Room::SoundLab, tile_center(32, 36)),
                (Room::Archive, tile_center(9, 22)),
            ],
            // Only the reactor is guarded; the other wings are safe.
            drones: vec![
                tile_center(50, 17),
                tile_center(58, 26),
                tile_center(51, 27),
            ],
        }
    }

    /// The room at a world position.
    #[allow(clippy::unused_self, reason = "the rooms belong to the station")]
    pub fn room_at(&self, x: f32, y: f32) -> Room {
        if x < 0.0 || y < 0.0 {
            return Room::Corridor;
        }
        let (column, row) = ((x / TILE as f32) as u32, (y / TILE as f32) as u32);
        ROOMS
            .iter()
            .find(|(_, rect)| rect.contains(column, row))
            .map_or(Room::Corridor, |&(room, _)| room)
    }

    pub fn is_solid(&self, column: i32, row: i32) -> bool {
        self.map.is_solid(&self.tiles, column, row)
    }
}

fn fill(map: &mut TileMap, rect: TileRect, tile: vnce::vnce_game::TileId) {
    for row in rect.y..rect.y + rect.h {
        for column in rect.x..rect.x + rect.w {
            map.set(0, i64::from(column), i64::from(row), Some(tile));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wing_is_reachable_and_its_core_is_on_floor() {
        let station = Station::build();
        // Flood fill from the start over walkable tiles.
        let start = (
            (station.start.0 / TILE as f32) as i32,
            (station.start.1 / TILE as f32) as i32,
        );
        let mut seen = vec![false; (COLUMNS * ROWS) as usize];
        let mut stack = vec![start];
        while let Some((c, r)) = stack.pop() {
            if c < 0 || r < 0 || c >= COLUMNS as i32 || r >= ROWS as i32 {
                continue;
            }
            let index = (r as u32 * COLUMNS + c as u32) as usize;
            if seen[index] || station.is_solid(c, r) {
                continue;
            }
            seen[index] = true;
            stack.extend([(c + 1, r), (c - 1, r), (c, r + 1), (c, r - 1)]);
        }
        let reachable = |(x, y): (f32, f32)| {
            seen[((y / TILE as f32) as u32 * COLUMNS + (x / TILE as f32) as u32) as usize]
        };
        for &(room, at) in &station.cores {
            assert!(reachable(at), "{room:?} core");
            assert_eq!(station.room_at(at.0, at.1), room);
        }
        for &at in &station.drones {
            assert!(reachable(at), "drone at {at:?}");
            assert_eq!(
                station.room_at(at.0, at.1),
                Room::Reactor,
                "drone at {at:?}"
            );
        }
        for pad in &station.pads {
            assert!(reachable(pad.center()));
        }
        assert!(reachable(station.relay.center()));
        // One core in each of the five wings.
        assert_eq!(station.cores.len(), 5);
    }
}
