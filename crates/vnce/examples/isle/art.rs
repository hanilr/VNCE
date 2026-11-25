//! The pixel art of Isle, drawn as text: plants, rocks, the beacon, crystals, the player
//! and the creatures. Each sprite stands on its bottom-center pixel.

use vnce::vnce_game::SpriteSheet;
use vnce::vnce_raster::{Canvas, Rgba};

use crate::island::Prop;

const PALETTE: [(char, Rgba); 30] = [
    // Wood and trunks.
    ('t', Rgba::rgb(96, 62, 38)),
    ('T', Rgba::rgb(130, 88, 52)),
    // Leaves: dark, middle, light.
    ('g', Rgba::rgb(34, 92, 48)),
    ('G', Rgba::rgb(58, 138, 62)),
    ('L', Rgba::rgb(112, 184, 82)),
    // Palm leaves.
    ('p', Rgba::rgb(46, 120, 60)),
    ('P', Rgba::rgb(96, 170, 70)),
    // Pine needles.
    ('n', Rgba::rgb(22, 70, 58)),
    ('N', Rgba::rgb(40, 108, 82)),
    // Stone: dark, middle, light.
    ('s', Rgba::rgb(86, 88, 100)),
    ('S', Rgba::rgb(128, 130, 142)),
    ('W', Rgba::rgb(176, 178, 186)),
    // Flowers.
    ('r', Rgba::rgb(230, 80, 90)),
    ('y', Rgba::rgb(250, 214, 90)),
    ('v', Rgba::rgb(170, 120, 230)),
    // Beacon: stone, roof, lamp glass.
    ('b', Rgba::rgb(198, 190, 172)),
    ('B', Rgba::rgb(150, 140, 124)),
    ('R', Rgba::rgb(170, 60, 52)),
    ('o', Rgba::rgb(70, 74, 92)),
    ('O', Rgba::rgb(255, 236, 160)),
    // Crystal: deep, middle, bright.
    ('c', Rgba::rgb(40, 120, 200)),
    ('C', Rgba::rgb(90, 200, 255)),
    ('w', Rgba::rgb(230, 250, 255)),
    // Player: skin, coat, coat dark, boots, hair, lantern.
    ('k', Rgba::rgb(240, 196, 160)),
    ('j', Rgba::rgb(222, 120, 60)),
    ('J', Rgba::rgb(160, 76, 40)),
    ('d', Rgba::rgb(60, 44, 40)),
    ('h', Rgba::rgb(80, 50, 36)),
    ('l', Rgba::rgb(255, 220, 120)),
    // Crab.
    ('x', Rgba::rgb(220, 90, 60)),
];

fn art(rows: &[&str]) -> Canvas {
    Canvas::from_art(rows, &PALETTE)
}

/// The sprite of a prop (`lit` only matters for the beacon).
#[allow(clippy::too_many_lines, reason = "the art is drawn as text")]
pub fn prop(prop: Prop, lit: bool) -> Canvas {
    match prop {
        Prop::Palm => art(&[
            "...PPp.......",
            ".PPPppPP.....",
            "PP..pPPpPPP..",
            "P...TpP...PP.",
            "...T..pP....P",
            "...T...P.....",
            "...T.........",
            "....T........",
            "....T........",
            "....T........",
            "....T........",
            "....tT.......",
            "....tT.......",
            "....tT.......",
            ".....T.......",
            ".....T.......",
            ".....tT......",
            ".....tT......",
            ".....tT......",
            "....ttTT.....",
        ]),
        Prop::Tree => art(&[
            "....LLLL......",
            "..LLGGGLLL....",
            ".LGGGGGGGLL...",
            "LGGGGgGGGGGL..",
            "LGGgGGGGgGGG..",
            "GGGGGGGgGGGGL.",
            "GgGGGGGGGGgGG.",
            "gGGGgGGGGGGGg.",
            ".gGGGGGgGGGg..",
            "..ggGGGGGgg...",
            "....gggtg.....",
            "......tT......",
            "......tT......",
            "......tT......",
            ".....ttTT.....",
        ]),
        Prop::Pine => art(&[
            "....N.....",
            "....N.....",
            "...NnN....",
            "...NNn....",
            "..NnNNN...",
            "..NNnNn...",
            ".NnNNNnN..",
            "...NnN....",
            "..NNnNNn..",
            ".NnNNNnNN.",
            "NNnNNNNnNN",
            "..NnNNn...",
            ".NNnNNNnN.",
            "NnNNNnNNnN",
            "nNNnNNNNnn",
            "....tT....",
            "....tT....",
            "...ttTT...",
        ]),
        Prop::Boulder => art(&[
            "...WWS....",
            ".WWSSSSs..",
            "WWSSSSSss.",
            "WSSSSSSsss",
            "SSSSSssss.",
            ".sssssss..",
        ]),
        Prop::Bush => art(&[
            "..LGGL...",
            ".LGGGGGL.",
            "GGgGGGgGG",
            "gGGgGgGGg",
            ".ggggggg.",
        ]),
        Prop::Flowers => art(&[".r...y.", "rGr.yGy", ".G..vG.", "G.Gv.vG", "...G..."]),
        Prop::Beacon => {
            let lamp = if lit { 'O' } else { 'o' };
            let rows: Vec<String> = [
                "....RR....",
                "...RRRR...",
                "..RRRRRR..",
                "..b####b..",
                "..b####b..",
                "..b####b..",
                ".RRRRRRRR.",
                "..bbbBbb..",
                "..bbbBbb..",
                "..RRRRRR..",
                "..bbbbBb..",
                "..bbbbBb..",
                "..bbbbBb..",
                ".bbbbbBBb.",
                ".RRRRRRRR.",
                ".bbbbbbBb.",
                ".bbbbbbBb.",
                ".bbbBbbBb.",
                ".bbbbbbBb.",
                ".RRRRRRRR.",
                "bbbbbbbbBB",
                "bbbbbbbbBB",
                "bbbbBbbbBB",
                "BBBBBBBBBB",
            ]
            .iter()
            .map(|row| row.replace('#', &lamp.to_string()))
            .collect();
            let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
            art(&rows)
        }
    }
}

/// Crystal frames: a slow glint running up the facets.
pub fn crystal_sheet() -> SpriteSheet {
    let base = [
        "..w..", ".cCw.", ".cCC.", "ccCCw", "cCCCC", "cCCCw", ".cCC.", ".cCC.", "..c..",
    ];
    let frames: Vec<Canvas> = (0..4)
        .map(|frame| {
            let rows: Vec<String> = base
                .iter()
                .enumerate()
                .map(|(y, row)| {
                    // The glint: one bright row moving up.
                    if y == 7 - frame * 2 {
                        row.replace('C', "w")
                    } else {
                        (*row).to_owned()
                    }
                })
                .collect();
            let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
            art(&rows)
        })
        .collect();
    SpriteSheet::from_frames(&frames)
}

/// The player: frames 0–2 facing the camera (standing, two steps), 3–5 facing away.
/// Facing left is the same art mirrored.
pub fn player_sheet() -> SpriteSheet {
    let front = [
        [
            "..hhhh..", ".hhhhhh.", ".hkkkkh.", "..kkkk..", ".jjjjjj.", "jjjJjjjl", "jjjJjjll",
            "kjjJjj.l", ".jjjjj..", ".JJJJJ..", ".d...d..", ".d...d..",
        ],
        [
            "..hhhh..", ".hhhhhh.", ".hkkkkh.", "..kkkk..", ".jjjjjj.", "jjjJjjjl", "jjjJjjll",
            "kjjJjj.l", ".jjjjj..", ".JJJJJ..", ".d..d...", "dd...d..",
        ],
        [
            "..hhhh..", ".hhhhhh.", ".hkkkkh.", "..kkkk..", ".jjjjjj.", "jjjJjjjl", "jjjJjjll",
            "kjjJjj.l", ".jjjjj..", ".JJJJJ..", "..d..d..", ".d...dd.",
        ],
    ];
    let back = [
        [
            "..hhhh..", ".hhhhhh.", ".hhhhhh.", "..hhhh..", ".jjjjjj.", "ljjJjjjj", "lljJjjjj",
            "l.jJjjjk", "..jjjjj.", "..JJJJJ.", "..d...d.", "..d...d.",
        ],
        [
            "..hhhh..", ".hhhhhh.", ".hhhhhh.", "..hhhh..", ".jjjjjj.", "ljjJjjjj", "lljJjjjj",
            "l.jJjjjk", "..jjjjj.", "..JJJJJ.", "...d..d.", "..d...dd",
        ],
        [
            "..hhhh..", ".hhhhhh.", ".hhhhhh.", "..hhhh..", ".jjjjjj.", "ljjJjjjj", "lljJjjjj",
            "l.jJjjjk", "..jjjjj.", "..JJJJJ.", "..d..d..", ".dd...d.",
        ],
    ];
    let frames: Vec<Canvas> = front.iter().chain(&back).map(|rows| art(rows)).collect();
    SpriteSheet::from_frames(&frames)
}

/// A crab scuttling sideways (two frames).
pub fn crab_sheet() -> SpriteSheet {
    let frames = [
        art(&["x.....x", ".x.d.x.", "xxxxxxx", ".x.x.x."]),
        art(&[".x...x.", "x..d..x", "xxxxxxx", "x.x.x.x"]),
    ];
    SpriteSheet::from_frames(&frames)
}

/// A soft round light: `color` at the center fading to black at `radius`, for the light
/// layer (added there, so black adds nothing).
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

/// A small fuzzy glow for wisps and fireflies (alpha falls off from the center).
pub fn glow_sprite(radius: u32, color: Rgba) -> Canvas {
    let size = radius * 2 + 1;
    let mut sprite = Canvas::new(size, size);
    let r = radius as f32 + 0.5;
    for (index, pixel) in sprite.pixels_mut().iter_mut().enumerate() {
        let (x, y) = ((index as u32 % size) as f32, (index as u32 / size) as f32);
        let d = ((x + 0.5 - r).powi(2) + (y + 0.5 - r).powi(2)).sqrt() / r;
        let level = (1.0 - d).max(0.0).powf(1.5);
        *pixel = color.with_alpha((level * 255.0) as u8);
    }
    sprite
}
