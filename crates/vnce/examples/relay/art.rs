//! Original art, drawn in code: tiles, sprites and light sprites.

use vnce::vnce_game::{Rng, SpriteSheet, TileId, Tileset};
use vnce::vnce_raster::{Canvas, Rgba};

use crate::world::{TILE, tiles};

/// The station's tiles, in the order of `world::tiles`.
pub fn tileset() -> Tileset {
    let mut list = Vec::new();
    for kind in 0..7 as TileId {
        let mut rng = Rng::new(u64::from(kind) * 31 + 7);
        let mut tile = Canvas::new(TILE, TILE);
        for y in 0..TILE {
            for x in 0..TILE {
                let (xi, yi) = (x as i32, y as i32);
                let color = match kind {
                    tiles::ROCK if rng.chance(0.1) => Rgba::rgb(30, 30, 42),
                    tiles::ROCK => Rgba::rgb(22, 22, 32),
                    // A lit top edge, then panels with a seam.
                    tiles::WALL_FACE if y == 0 => Rgba::rgb(120, 128, 150),
                    tiles::WALL_FACE if y == 7 => Rgba::rgb(36, 38, 52),
                    tiles::WALL_FACE if x == 3 && y > 1 => Rgba::rgb(52, 56, 72),
                    tiles::WALL_FACE => Rgba::rgb(70, 76, 96),
                    tiles::FLOOR if x == 0 || y == 0 => Rgba::rgb(58, 62, 76),
                    tiles::FLOOR if rng.chance(0.06) => Rgba::rgb(84, 88, 104),
                    tiles::FLOOR => Rgba::rgb(72, 76, 92),
                    // Hub plates with rivets in the corners.
                    tiles::PLATE if (x == 1 || x == 6) && (y == 1 || y == 6) => {
                        Rgba::rgb(130, 140, 150)
                    }
                    tiles::PLATE if x == 0 || y == 0 => Rgba::rgb(56, 70, 72),
                    tiles::PLATE => Rgba::rgb(78, 96, 98),
                    // Corridor grating.
                    tiles::CORRIDOR if (x + y) % 4 == 0 => Rgba::rgb(46, 48, 58),
                    tiles::CORRIDOR => Rgba::rgb(66, 68, 80),
                    tiles::MACHINE if y == 0 || x == 0 => Rgba::rgb(110, 116, 132),
                    tiles::MACHINE => Rgba::rgb(48, 52, 66),
                    // Shelves with book spines.
                    tiles::SHELF if y == 0 || y == 7 => Rgba::rgb(96, 70, 48),
                    tiles::SHELF => [
                        Rgba::rgb(170, 60, 60),
                        Rgba::rgb(60, 120, 170),
                        Rgba::rgb(200, 170, 80),
                        Rgba::rgb(80, 150, 90),
                    ][((xi / 2) as usize + kind as usize) % 4],
                    _ => Rgba::rgb(255, 0, 255),
                };
                tile.pixel(xi, yi, color);
            }
        }
        list.push(tile);
    }
    Tileset::new(SpriteSheet::from_frames(&list)).with_solid([
        tiles::ROCK,
        tiles::WALL_FACE,
        tiles::MACHINE,
        tiles::SHELF,
    ])
}

/// The player: two walking frames.
pub fn player_sheet() -> SpriteSheet {
    let palette = [
        ('k', Rgba::rgb(18, 18, 26)),
        ('h', Rgba::rgb(235, 235, 240)),
        ('v', Rgba::rgb(90, 200, 255)),
        ('s', Rgba::rgb(255, 150, 60)),
        ('g', Rgba::rgb(120, 126, 140)),
    ];
    let a = Canvas::from_art(
        &[
            "..kkkk..", ".khhhhk.", ".khvvhk.", ".khhhhk.", "kssggssk", ".kgggk..", ".kk..kk.",
            ".k....k.",
        ],
        &palette,
    );
    let b = Canvas::from_art(
        &[
            "..kkkk..", ".khhhhk.", ".khvvhk.", ".khhhhk.", "kssggssk", "..kgggk.", ".kk..kk.",
            "..k..k..",
        ],
        &palette,
    );
    SpriteSheet::from_frames(&[a, b])
}

/// A patrol drone: rotor up and down; the eye is drawn on top as light.
pub fn drone_sheet() -> SpriteSheet {
    let palette = [
        ('k', Rgba::rgb(20, 20, 28)),
        ('b', Rgba::rgb(110, 116, 130)),
        ('w', Rgba::rgb(200, 205, 215)),
        ('r', Rgba::rgb(255, 60, 70)),
    ];
    let up = Canvas::from_art(
        &[
            "wwk..kww", "..kbbk..", ".kbbbbk.", "kbbrrbbk", ".kbbbbk.", "..k..k..",
        ],
        &palette,
    );
    let down = Canvas::from_art(
        &[
            ".wwkkww.", "..kbbk..", ".kbbbbk.", "kbbrrbbk", ".kbbbbk.", ".k....k.",
        ],
        &palette,
    );
    SpriteSheet::from_frames(&[up, down])
}

/// A power core: two shine frames.
pub fn core_sheet() -> SpriteSheet {
    let palette = [
        ('c', Rgba::rgb(80, 255, 200)),
        ('w', Rgba::rgb(240, 255, 250)),
        ('d', Rgba::rgb(30, 150, 120)),
    ];
    let a = Canvas::from_art(&["..c..", ".cwc.", "cwwwc", ".cdc.", "..d.."], &palette);
    let b = Canvas::from_art(&["..c..", ".ccc.", "cwccc", ".cdc.", "..d.."], &palette);
    SpriteSheet::from_frames(&[a, b])
}

/// The relay: off and on.
pub fn relay_sheet() -> SpriteSheet {
    let off = [
        ('k', Rgba::rgb(20, 22, 30)),
        ('m', Rgba::rgb(90, 96, 110)),
        ('l', Rgba::rgb(140, 146, 160)),
        ('c', Rgba::rgb(50, 70, 70)),
    ];
    let on = [
        ('k', Rgba::rgb(20, 22, 30)),
        ('m', Rgba::rgb(90, 96, 110)),
        ('l', Rgba::rgb(170, 255, 200)),
        ('c', Rgba::rgb(90, 255, 170)),
    ];
    let shape = [
        "....kkkkkkkk....",
        "..kkmmmmmmmmkk..",
        ".kmmllllllllmmk.",
        ".kmlkkkkkkkklmk.",
        "kmlk..cccc..klmk",
        "kmlk.cc..cc.klmk",
        "kmlkcc.cc.cckmlk",
        "kmlkc.cccc.ckmlk",
        "kmlkc.cccc.ckmlk",
        "kmlkcc.cc.cckmlk",
        "kmlk.cc..cc.klmk",
        "kmlk..cccc..klmk",
        ".kmlkkkkkkkklmk.",
        ".kmmllllllllmmk.",
        "..kkmmmmmmmmkk..",
        "....kkkkkkkk....",
    ];
    SpriteSheet::from_frames(&[
        Canvas::from_art(&shape, &off),
        Canvas::from_art(&shape, &on),
    ])
}

/// A sound pad: dark and lit.
pub fn pad_sheet() -> SpriteSheet {
    let shape = [
        "kkkkkkk", "kmmmmmk", "kmlllmk", "kmlllmk", "kmlllmk", "kmmmmmk", "kkkkkkk",
    ];
    let dark = [
        ('k', Rgba::rgb(24, 26, 34)),
        ('m', Rgba::rgb(60, 80, 90)),
        ('l', Rgba::rgb(40, 60, 70)),
    ];
    let lit = [
        ('k', Rgba::rgb(24, 26, 34)),
        ('m', Rgba::rgb(120, 255, 255)),
        ('l', Rgba::rgb(220, 255, 255)),
    ];
    SpriteSheet::from_frames(&[
        Canvas::from_art(&shape, &dark),
        Canvas::from_art(&shape, &lit),
    ])
}

/// One panel of a sliding door (a tile).
pub fn door_panel() -> Canvas {
    let mut panel = Canvas::filled(TILE, TILE, Rgba::rgb(150, 120, 60));
    panel.fill_rect(0, 0, TILE, 1, Rgba::rgb(220, 190, 110));
    panel.fill_rect(0, 7, TILE, 1, Rgba::rgb(80, 60, 30));
    for i in 0..4 {
        panel.line(i * 2, 1, i * 2 + 2, 6, Rgba::rgb(40, 34, 24));
    }
    panel
}

/// A filled heart for the energy display.
pub fn energy_icon(full: bool) -> Canvas {
    let color = if full {
        Rgba::rgb(255, 90, 100)
    } else {
        Rgba::rgb(70, 50, 60)
    };
    Canvas::from_art(
        &[".#.#.", "#####", "#####", ".###.", "..#.."],
        &[('#', color)],
    )
}

/// A round light of `radius` pixels: `color` in the middle fading to black (opaque, for
/// adding into the light layer).
pub fn light_sprite(radius: u32, color: Rgba) -> Canvas {
    let size = radius * 2;
    let mut sprite = Canvas::filled(size, size, Rgba::BLACK);
    let r = radius as f32;
    for (index, pixel) in sprite.pixels_mut().iter_mut().enumerate() {
        let (x, y) = ((index as u32 % size) as f32, (index as u32 / size) as f32);
        let d = ((x + 0.5 - r).powi(2) + (y + 0.5 - r).powi(2)).sqrt() / r;
        let falloff = (1.0 - d).max(0.0);
        let level = falloff * falloff;
        let scale = |c: u8| (f32::from(c) * level) as u8;
        *pixel = Rgba::rgb(scale(color.r), scale(color.g), scale(color.b));
    }
    sprite
}
