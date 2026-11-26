//! Anti-aliased rendering of the layers: 4 x 4 samples per pixel, each layer filled with a
//! scanline pass using the nonzero rule.

use crate::geometry::{Path, Point};
use crate::scene::Scene;

/// A pixel with premultiplied alpha, every channel in `0.0..=1.0`.
pub type Premul = [f32; 4];

/// Samples per pixel along each axis.
const SUPERSAMPLE: usize = 4;

/// Renders `scene` into a `w` x `h` image (row by row, premultiplied); the scene's width
/// fills the image's width.
pub fn render(scene: &Scene, w: usize, h: usize) -> Vec<Premul> {
    let (samples, rows) = (w * SUPERSAMPLE, h * SUPERSAMPLE);
    let scale = samples as f32 / scene.width;
    let mut buffer = vec![[0.0f32; 4]; samples * rows];

    // The samples inside the badge, for the layers clipped to it.
    let badge = scene.clip.as_ref().map(|clip| {
        let mut inside = vec![false; samples * rows];
        fill(clip, samples, rows, scale, |row, col| {
            inside[row * samples + col] = true
        });
        inside
    });

    for layer in &scene.layers {
        fill(&layer.path, samples, rows, scale, |row, col| {
            let i = row * samples + col;
            if layer.in_badge && badge.as_ref().is_some_and(|inside| !inside[i]) {
                return;
            }
            let (x, y) = ((col as f32 + 0.5) / scale, (row as f32 + 0.5) / scale);
            let c = layer.paint.at((x, y));
            let dst = &mut buffer[i];
            let keep = 1.0 - c.a;
            dst[0] = c.r * c.a + dst[0] * keep;
            dst[1] = c.g * c.a + dst[1] * keep;
            dst[2] = c.b * c.a + dst[2] * keep;
            dst[3] = c.a + dst[3] * keep;
        });
    }

    downsample(&buffer, w, h)
}

/// Calls `visit(row, column)` for every sample of a `samples` x `rows` grid whose center
/// lies inside `path` (`scale` samples per design unit).
fn fill(path: &Path, samples: usize, rows: usize, scale: f32, mut visit: impl FnMut(usize, usize)) {
    let polygons = path.flatten();
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..rows {
        let y = (row as f32 + 0.5) / scale;
        crossings.clear();
        for polygon in &polygons {
            collect_crossings(polygon, y, &mut crossings);
        }
        if crossings.is_empty() {
            continue;
        }
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut winding = 0;
        for pair in crossings.windows(2) {
            winding += pair[0].1;
            if winding == 0 {
                continue;
            }
            // Samples whose centers lie inside the span.
            let start = (pair[0].0 * scale - 0.5).ceil().max(0.0) as usize;
            let end = (pair[1].0 * scale - 0.5).floor();
            if end < 0.0 {
                continue;
            }
            let end = (end as usize).min(samples - 1);
            for col in start..=end {
                visit(row, col);
            }
        }
    }
}

/// Adds the points where the horizontal line at `y` crosses the edges of `polygon`, with
/// +1 for edges going down and -1 for edges going up.
fn collect_crossings(polygon: &[Point], y: f32, out: &mut Vec<(f32, i32)>) {
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let (top, bottom, dir) = if a.1 < b.1 { (a, b, 1) } else { (b, a, -1) };
        // Half-open, so a vertex on the line is counted once.
        if y < top.1 || y >= bottom.1 {
            continue;
        }
        let t = (y - top.1) / (bottom.1 - top.1);
        out.push((top.0 + (bottom.0 - top.0) * t, dir));
    }
}

/// Averages each block of samples into one pixel.
fn downsample(buffer: &[Premul], w: usize, h: usize) -> Vec<Premul> {
    let samples = w * SUPERSAMPLE;
    let weight = 1.0 / (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let mut out = vec![[0.0f32; 4]; w * h];
    for (row, line) in buffer.chunks(samples).enumerate() {
        for (col, sample) in line.iter().enumerate() {
            let px = &mut out[(row / SUPERSAMPLE) * w + col / SUPERSAMPLE];
            for k in 0..4 {
                px[k] += sample[k] * weight;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::SIZE;
    use crate::horse::Layer;
    use crate::paint::{Color, Paint};

    /// An unclipped 512 x 512 scene of `layers`.
    fn scene(layers: Vec<Layer>) -> Scene {
        Scene {
            width: SIZE,
            height: SIZE,
            layers,
            clip: None,
            edges: crate::pixelate::EDGES,
        }
    }

    #[test]
    fn a_square_covers_its_pixels_and_half_covers_its_edge() {
        // In an 8 x 8 render a pixel is 64 design units: a rectangle 224 wide and 192 tall
        // covers 3 rows fully, 3 columns fully and half of the fourth column.
        let square = Path::new()
            .move_to((0.0, 0.0))
            .line_to((224.0, 0.0))
            .line_to((224.0, 192.0))
            .line_to((0.0, 192.0))
            .close();
        let layer = Layer::new("square", square, Paint::Solid(Color::hex(0xffffff)));
        let image = render(&scene(vec![layer]), 8, 8);
        assert!((image[0][3] - 1.0).abs() < 1e-5);
        assert!((image[2 * 8 + 2][3] - 1.0).abs() < 1e-5);
        assert!((image[3][3] - 0.5).abs() < 1e-5);
        assert!(image[3 * 8][3].abs() < 1e-5);
    }

    #[test]
    fn a_wide_scene_renders_a_wide_image_and_clips_to_its_shape() {
        let wide = Scene {
            width: 1024.0,
            height: 512.0,
            layers: vec![Layer::in_badge(
                "clipped",
                Path::rect(0.0, 0.0, 1024.0, 512.0),
                Paint::Solid(Color::hex(0xffffff)),
            )],
            clip: Some(Path::rect(0.0, 0.0, 512.0, 512.0)),
            edges: crate::pixelate::EDGES,
        };
        let image = render(&wide, 16, 8);
        assert_eq!(image.len(), 16 * 8);
        assert!((image[7 * 16 + 7][3] - 1.0).abs() < 1e-5);
        assert!(image[7 * 16 + 8][3].abs() < 1e-5);
    }
}
