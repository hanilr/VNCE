//! The banner for the repository's social preview (1280 x 640), in the dark style of the
//! variation-ui banner: the horse on the left where that banner has it, the farm as dim
//! outlines along the bottom, the name and the taglines in pixel letters on the right with
//! red, green and blue accents, and the change from big pixels to the smooth drawing,
//! running from the left.

use crate::farm::{FARM, barn, curve, fence, silo};
use crate::geometry::Path;
use crate::horse::{BACKGROUND, BLUE, GREEN, Layer, RED, WHITE};
use crate::paint::Paint;
use crate::pixelate::EDGES;
use crate::scene::{LOGO_HORSE_AT, LOGO_HORSE_SCALE, Scene};
use crate::text;

/// The banner's width (GitHub's recommended social preview size).
pub const WIDTH: f32 = 1280.0;
/// The banner's height.
pub const HEIGHT: f32 = 640.0;

/// The middle of the text column (where the variation-ui banner centers its text).
const TEXT_CENTER: f32 = 835.0;

/// The lines of text: name, text, top edge, font pixel size, and one color code per
/// character (`w` white, `r` red, `g` green, `b` blue, space for none).
const LINES: [(&str, &str, f32, f32, &str); 4] = [
    ("title", "VNCE", 70.0, 22.0, "wwww"),
    (
        "subtitle",
        "VARIATION CLI ENGINE",
        280.0,
        5.0,
        "rwwwwwwww gww bwwwww",
    ),
    ("pixel", "Make it PIXEL", 400.0, 5.0, "wwww ww rgbrg"),
    ("smooth", "Make it SMOOTH", 470.0, 5.0, "rrgg bb wwwwww"),
];

/// The banner scene.
pub fn scene() -> Scene {
    let mut layers = vec![Layer::new(
        "background",
        Path::rect(0.0, 0.0, WIDTH, HEIGHT),
        Paint::Solid(BACKGROUND),
    )];
    layers.push(Layer::new("farm", farm(), Paint::Solid(FARM)));
    // The horse where the variation-ui banner has it.
    layers.extend(crate::horse::layers(1.0, (0.0, 0.0)));
    layers.extend(lettering());
    Scene {
        width: WIDTH,
        height: HEIGHT,
        layers,
        clip: None,
        // The logo's band edges, at the same places on the horse.
        edges: EDGES.map(|e| (e - LOGO_HORSE_AT.0) / LOGO_HORSE_SCALE),
    }
}

/// The text: each line split into one layer per color, centered on [`TEXT_CENTER`].
fn lettering() -> Vec<Layer> {
    let mut layers = Vec::new();
    for (name, line, top, cell, colors) in LINES {
        let (w, _) = text::measure(line, cell);
        let left = (TEXT_CENTER - w / 2.0).round();
        for (code, ink, color) in [
            ('w', "white", WHITE),
            ('r', "red", RED),
            ('g', "green", GREEN),
            ('b', "blue", BLUE),
        ] {
            // The characters of this color; the rest become spaces, so they keep their
            // places in the fixed-width font.
            let part: String = line
                .chars()
                .zip(colors.chars())
                .map(|(ch, c)| if c == code { ch } else { ' ' })
                .collect();
            if part.trim().is_empty() {
                continue;
            }
            layers.push(text::layer(
                format!("{name}-{ink}"),
                &part,
                (left, top),
                cell,
                Paint::Solid(color),
            ));
        }
    }
    layers
}

/// The farm along the bottom, as one path of outlines.
fn farm() -> Path {
    let line = 3.0;
    let mut farm = Path::new();
    // Hills across the whole width.
    farm.cmds.extend(
        Path::polyline(
            &curve(
                (0.0, 566.0),
                (380.0, 520.0),
                (880.0, 600.0),
                (1280.0, 548.0),
            ),
            line,
        )
        .cmds,
    );
    // The barn and the silo, bottom right, below the text.
    farm.cmds
        .extend(barn(1100.0, 1210.0, 590.0, 520.0, 486.0, line).cmds);
    farm.cmds
        .extend(silo(1226.0, 1262.0, 590.0, 512.0, line).cmds);
    // The fence across the whole width (the horse stands in front of it).
    let posts: Vec<f32> = (0..11).map(|i| 30.0 + i as f32 * 122.0).collect();
    farm.cmds
        .extend(fence(0.0, WIDTH, [604.0, 626.0], &posts, line).cmds);
    // The moon, top right.
    farm.cmds
        .extend(Path::ring((1210.0, 78.0), 26.0, line).cmds);
    farm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_fits_right_of_the_horse() {
        for (_, line, top, cell, colors) in LINES {
            assert_eq!(line.chars().count(), colors.chars().count(), "{line:?}");
            let (w, h) = text::measure(line, cell);
            assert!(TEXT_CENTER - w / 2.0 > 440.0, "{line:?} reaches the horse");
            assert!(TEXT_CENTER + w / 2.0 < WIDTH - 24.0, "{line:?} is too wide");
            assert!(top + h < HEIGHT, "{line:?} is too low");
        }
    }

    #[test]
    fn layer_names_are_unique() {
        let scene = scene();
        let mut names: Vec<&str> = scene.layers.iter().map(|l| l.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before);
    }
}
