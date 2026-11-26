//! Generates the VNCE logo: a horse's face, seen from the front, that starts as big pixels
//! on the left and becomes a smooth drawing on the right.
//!
//! The style is the dark one of the Variation ecosystem: the horse of the variation-ui
//! banner (white shards and lines with red, green and blue accents) in front of a farm drawn
//! as dim outlines, on a dark badge with round corners.
//!
//! It also generates the banner for the repository's social preview: the same horse and
//! farm, wide, with the name and taglines in pixel letters.
//!
//! Usage:
//!
//! ```sh
//! cargo run --release --manifest-path tools/gen-logo/Cargo.toml -- assets/logo
//! ```
//!
//! Writes `vnce-logo.svg` and `vnce-logo-{16,32,64,256,512}.png` into the given folder,
//! and `vnce-banner.svg` and `vnce-banner.png` (1280 x 640).

mod banner;
mod farm;
mod geometry;
mod horse;
mod paint;
mod pixelate;
mod raster;
mod scene;
mod svg;
mod text;

use std::path::Path;
use std::process::ExitCode;

use vnce_raster::{Canvas, Rgba};

use crate::pixelate::Bands;
use crate::raster::Premul;
use crate::scene::Scene;

/// The PNG sizes written, in pixels.
const PNG_SIZES: [usize; 5] = [16, 32, 64, 256, 512];

fn main() -> ExitCode {
    let Some(out_dir) = std::env::args().nth(1) else {
        eprintln!("usage: gen-logo <output folder>");
        return ExitCode::FAILURE;
    };
    match generate(Path::new(&out_dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gen-logo: {e}");
            ExitCode::FAILURE
        }
    }
}

fn generate(out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(out_dir)?;
    // The farm behind, the horse in front.
    let scene = Scene::logo();
    for n in PNG_SIZES {
        let logo = draw(&scene, n, n);
        let path = out_dir.join(format!("vnce-logo-{n}.png"));
        std::fs::write(&path, to_canvas(&logo, n, n).to_png()?)?;
        println!("wrote {}", path.display());
        if n == geometry::SIZE as usize {
            let path = out_dir.join("vnce-logo.svg");
            std::fs::write(&path, svg::write(&scene, &logo))?;
            println!("wrote {}", path.display());
        }
    }

    // The banner, one pixel per design unit.
    let scene = banner::scene();
    let (w, h) = scene.pixel_size(banner::WIDTH as usize);
    let image = draw(&scene, w, h);
    let path = out_dir.join("vnce-banner.png");
    std::fs::write(&path, to_canvas(&image, w, h).to_png()?)?;
    println!("wrote {}", path.display());
    let path = out_dir.join("vnce-banner.svg");
    std::fs::write(&path, svg::write(&scene, &image))?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Renders `scene` at `w` x `h` and pixelates its bands.
fn draw(scene: &Scene, w: usize, h: usize) -> Vec<Premul> {
    pixelate::compose(&raster::render(scene, w, h), &Bands::of(scene, w, h))
}

/// Converts premultiplied pixels into an 8-bit canvas.
fn to_canvas(pixels: &[Premul], w: usize, h: usize) -> Canvas {
    let side = |v: usize| u32::try_from(v).expect("logo sizes are small");
    let mut canvas = Canvas::new(side(w), side(h));
    for (dst, p) in canvas.pixels_mut().iter_mut().zip(pixels) {
        let a = p[3].clamp(0.0, 1.0);
        let to = |v: f32| {
            if a > 0.0 {
                (v / a * 255.0).round().clamp(0.0, 255.0) as u8
            } else {
                0
            }
        };
        *dst = Rgba::new(to(p[0]), to(p[1]), to(p[2]), (a * 255.0).round() as u8);
    }
    canvas
}
