//! Isle: an isometric 2.5D showcase of vnce (work in progress).

// Game code converts freely between pixels, tiles and floats; every value stays far inside
// the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel and tile coordinates are small"
)]
// Geometry reads best with its usual names (x, y, u, v, h).
#![allow(clippy::many_single_char_names, reason = "geometry names")]
#![allow(dead_code, reason = "the game is built up step by step")]

mod art;
mod island;
mod iso;

fn main() {
    let island = island::Island::generate(1);
    let sprites = iso::Sprites::new();
    let mut terrain = iso::Terrain::new();
    let picture = terrain.picture(&island, &sprites, 0);
    println!("{}×{}", picture.width(), picture.height());
}

#[cfg(test)]
mod tests {
    use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Rgba};

    use crate::island::Island;
    use crate::iso::{Sprites, Terrain};

    /// Writes the terrain of a few islands from every side to the folder in
    /// `ISLE_SNAPSHOTS` (skipped without it). For looking at the art, not a check.
    #[test]
    fn terrain_snapshots() {
        let Ok(folder) = std::env::var("ISLE_SNAPSHOTS") else {
            return;
        };
        let sprites = Sprites::new();
        for seed in [1, 2] {
            let island = Island::generate(seed);
            let mut terrain = Terrain::new();
            for quarter in 0..4 {
                let picture = terrain.picture(&island, &sprites, quarter);
                let mut shown =
                    Canvas::filled(picture.width(), picture.height(), Rgba::rgb(30, 90, 140));
                shown.set_blend_mode(BlendMode::Alpha);
                shown.blit(picture, 0, 0, BlitOptions::default());
                let png = shown.to_png().expect("a canvas encodes as PNG");
                std::fs::write(format!("{folder}/terrain-{seed}-{quarter}.png"), png)
                    .expect("writable");
                // The middle, three times as large, to see the pixels.
                let (w, h) = (320, 200);
                let (x0, y0) = ((shown.width() - w) / 2, (shown.height() - h) / 2);
                let mut zoom = Canvas::new(w * 3, h * 3);
                for (i, pixel) in zoom.pixels_mut().iter_mut().enumerate() {
                    let (x, y) = (i as u32 % (w * 3) / 3, i as u32 / (w * 3) / 3);
                    *pixel = shown.get(x0 + x, y0 + y).unwrap_or(Rgba::BLACK);
                }
                let png = zoom.to_png().expect("a canvas encodes as PNG");
                std::fs::write(format!("{folder}/zoom-{seed}-{quarter}.png"), png)
                    .expect("writable");
            }
        }
    }
}
