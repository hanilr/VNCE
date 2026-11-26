//! The SVG logo: the pixel bands as `<rect>` runs, the smooth part as the vector layers
//! clipped to its region (and the farm scene also to the badge).

use std::fmt::Write as _;

use crate::paint::Color;
use crate::pixelate::{Bands, SMOOTH};
use crate::raster::Premul;
use crate::scene::Scene;

/// The SVG is laid out on this grid (the smallest block of the 512 master).
const GRID: usize = 4;

/// A rectangle in grid cells, with its fill (`None` for clip rectangles).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Run {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: Option<[u8; 3]>,
}

/// Writes the SVG from the scene and its composed master (one pixel per design unit: the
/// 512 x 512 master for the logo).
pub fn write(scene: &Scene, master: &[Premul]) -> String {
    let (n, h) = (scene.width as usize, scene.height as usize);
    assert_eq!(
        master.len(),
        n * h,
        "the SVG is made from the master, one pixel per design unit"
    );
    let bands = Bands::of(scene, n, h);
    let (cells, cell_rows) = (n / GRID, h / GRID);

    // Rows of runs: pixel blocks by color, and the smooth region.
    let mut pixel_rows = Vec::with_capacity(cell_rows);
    let mut smooth_rows = Vec::with_capacity(cell_rows);
    for cy in 0..cell_rows {
        let (y, mut pixels, mut smooth) = (cy * GRID, Vec::new(), Vec::new());
        for cx in 0..cells {
            let x = cx * GRID;
            if bands.band_of(x, y) == SMOOTH {
                push_run(&mut smooth, cx, cy, None);
            } else {
                let p = master[y * n + x];
                if p[3] > 0.0 {
                    let to = |v: f32| (v / p[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                    push_run(&mut pixels, cx, cy, Some([to(p[0]), to(p[1]), to(p[2])]));
                }
            }
        }
        pixel_rows.push(pixels);
        smooth_rows.push(smooth);
    }

    let mut defs = String::new();
    defs.push_str("<clipPath id=\"smooth\">");
    for run in merge_rows(smooth_rows) {
        defs.push_str(&rect(run));
    }
    defs.push_str("</clipPath>\n");
    if let Some(clip) = &scene.clip {
        writeln!(
            defs,
            "<clipPath id=\"badge\"><path d=\"{}\"/></clipPath>",
            clip.svg_data()
        )
        .expect("writing to a String cannot fail");
    }

    let mut body = String::from("<g clip-path=\"url(#smooth)\">\n");
    for layer in &scene.layers {
        let fill = layer.paint.svg_fill(&layer.name, &mut defs);
        let clip = if layer.in_badge && scene.clip.is_some() {
            " clip-path=\"url(#badge)\""
        } else {
            ""
        };
        writeln!(
            body,
            "<path id=\"{}-shape\" {fill}{clip} d=\"{}\"/>",
            layer.name,
            layer.path.svg_data()
        )
        .expect("writing to a String cannot fail");
    }
    body.push_str("</g>\n<g shape-rendering=\"crispEdges\">\n");
    for run in merge_rows(pixel_rows) {
        body.push_str(&rect(run));
        body.push('\n');
    }
    body.push_str("</g>\n");

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {n} {h}\" width=\"{n}\" height=\"{h}\">\n\
         <title>VNCE</title>\n<defs>\n{defs}</defs>\n{body}</svg>\n"
    )
}

/// Adds a one-cell run, joining it to the previous run when they touch and match.
fn push_run(row: &mut Vec<Run>, cx: usize, cy: usize, color: Option<[u8; 3]>) {
    if let Some(last) = row.last_mut()
        && last.x + last.w == cx
        && last.color == color
    {
        last.w += 1;
        return;
    }
    row.push(Run {
        x: cx,
        y: cy,
        w: 1,
        h: 1,
        color,
    });
}

/// Joins runs of consecutive rows that have the same position, width and color.
fn merge_rows(rows: Vec<Vec<Run>>) -> Vec<Run> {
    let mut done = Vec::new();
    let mut open: Vec<Run> = Vec::new();
    for row in rows {
        let mut next = Vec::with_capacity(row.len());
        for run in row {
            if let Some(i) = open
                .iter()
                .position(|o| o.x == run.x && o.w == run.w && o.color == run.color)
            {
                let mut grown = open.swap_remove(i);
                grown.h += 1;
                next.push(grown);
            } else {
                next.push(run);
            }
        }
        done.append(&mut open);
        open = next;
    }
    done.append(&mut open);
    done.sort_by_key(|r| (r.y, r.x));
    done
}

fn rect(run: Run) -> String {
    let (x, y, w, h) = (run.x * GRID, run.y * GRID, run.w * GRID, run.h * GRID);
    match run.color {
        Some([r, g, b]) => format!(
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{}\"/>",
            Color {
                r: f32::from(r) / 255.0,
                g: f32::from(g) / 255.0,
                b: f32::from(b) / 255.0,
                a: 1.0
            }
            .svg_rgb()
        ),
        None => format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"/>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_join_across_columns_and_rows() {
        let mut a = Vec::new();
        push_run(&mut a, 0, 0, Some([1, 2, 3]));
        push_run(&mut a, 1, 0, Some([1, 2, 3]));
        push_run(&mut a, 2, 0, Some([9, 9, 9]));
        let mut b = Vec::new();
        push_run(&mut b, 0, 1, Some([1, 2, 3]));
        push_run(&mut b, 1, 1, Some([1, 2, 3]));
        let merged = merge_rows(vec![a, b]);
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged[0],
            Run {
                x: 0,
                y: 0,
                w: 2,
                h: 2,
                color: Some([1, 2, 3])
            }
        );
        assert_eq!(
            merged[1],
            Run {
                x: 2,
                y: 0,
                w: 1,
                h: 1,
                color: Some([9, 9, 9])
            }
        );
    }
}
