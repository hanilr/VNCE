//! Pixel text: the 5 x 7 font of `vnce-raster`, each lit font pixel turned into a square
//! `cell` design units wide, so the letters stay sharp in the SVG and in every PNG.

use vnce_raster::{Canvas, Font, Rgba};

use crate::geometry::{Path, Point};
use crate::horse::Layer;
use crate::paint::Paint;

/// The size of `text` in design units, with font pixels `cell` units wide (the banner
/// centers its lines with it).
pub fn measure(text: &str, cell: f32) -> (f32, f32) {
    let (w, h) = Font::builtin().measure(text);
    (w as f32 * cell, h as f32 * cell)
}

/// `text` with its top-left corner at `origin`, as one layer of squares.
///
/// Lit pixels next to each other on a row become one rectangle, so the path stays short.
pub fn layer(name: impl Into<String>, text: &str, origin: Point, cell: f32, paint: Paint) -> Layer {
    let font = Font::builtin();
    let (w, h) = font.measure(text);
    let mut canvas = Canvas::new(w, h);
    canvas.text(font, 0, 0, text, Rgba::WHITE);

    let mut path = Path::new();
    for (row, line) in canvas.pixels().chunks(w as usize).enumerate() {
        let mut col = 0;
        while col < line.len() {
            if line[col].a == 0 {
                col += 1;
                continue;
            }
            let start = col;
            while col < line.len() && line[col].a > 0 {
                col += 1;
            }
            let rect = Path::rect(
                origin.0 + start as f32 * cell,
                origin.1 + row as f32 * cell,
                (col - start) as f32 * cell,
                cell,
            );
            path.cmds.extend(rect.cmds);
        }
    }
    Layer::new(name, path, paint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Color;

    #[test]
    fn text_is_measured_in_cells() {
        // 4 letters of 5 pixels and 3 gaps of 1 pixel, 7 pixels tall.
        assert_eq!(measure("VNCE", 10.0), (230.0, 70.0));
    }

    #[test]
    fn a_letter_becomes_squares_inside_its_box() {
        let layer = layer(
            "i",
            "I",
            (100.0, 50.0),
            4.0,
            Paint::Solid(Color::hex(0xffffff)),
        );
        let polygons = layer.path.flatten();
        assert!(!polygons.is_empty());
        for p in polygons.iter().flatten() {
            assert!((100.0..=120.0).contains(&p.0), "x {} outside", p.0);
            assert!((50.0..=78.0).contains(&p.1), "y {} outside", p.1);
        }
    }
}
