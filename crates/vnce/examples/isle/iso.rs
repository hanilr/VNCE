//! The isometric view: projection at any camera angle, tiles with textured tops, shaded
//! cliff sides and stairs, props with sun shadows, the terrain cache per camera direction,
//! and picking the tile under a screen point.
//!
//! World positions are in tiles (`x` east, `y` south) with heights in levels. The view
//! turns around the island center; at a turn of 0° north is up-right on the screen.

use std::f32::consts::FRAC_PI_2;

use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Rgba};

use crate::art;
use crate::island::{Dir, Ground, Island, MAX_LEVEL, Prop, SIZE, index};

/// Width of a tile diamond in world pixels.
pub const TILE_WIDTH: f32 = 24.0;
/// Height of a tile diamond in world pixels.
pub const TILE_HEIGHT: f32 = 12.0;
/// Height of one level in world pixels.
pub const LEVEL_HEIGHT: f32 = 6.0;

/// Room above the highest tile for tall props, in world pixels.
const HEADROOM: f32 = 34.0;

/// Size of a terrain cache: the whole island at a right-angle turn.
pub fn cache_size() -> (u32, u32) {
    let width = SIZE as f32 * TILE_WIDTH + 8.0;
    let height = SIZE as f32 * TILE_HEIGHT + f32::from(MAX_LEVEL) * LEVEL_HEIGHT + HEADROOM + 12.0;
    (width as u32, height as u32)
}

/// Where the island center lands in a terrain cache (at level 0).
pub fn cache_origin() -> (f32, f32) {
    let (width, _) = cache_size();
    (
        width as f32 / 2.0,
        SIZE as f32 / 2.0 * TILE_HEIGHT + f32::from(MAX_LEVEL) * LEVEL_HEIGHT + HEADROOM,
    )
}

/// A camera angle and the screen position of the island center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Turn of the camera in radians.
    pub angle: f32,
    sin: f32,
    cos: f32,
    /// Screen position of the island center at level 0.
    pub origin: (f32, f32),
}

impl View {
    pub fn new(angle: f32, origin: (f32, f32)) -> Self {
        let (sin, cos) = angle.sin_cos();
        // Exact values at right angles keep tile edges on whole pixels.
        let snap = |v: f32| {
            if (v.round() - v).abs() < 1e-5 {
                v.round()
            } else {
                v
            }
        };
        Self {
            angle,
            sin: snap(sin),
            cos: snap(cos),
            origin,
        }
    }

    /// The view of turn `quarter` (0–3) at a terrain cache origin.
    pub fn quarter(quarter: u8) -> Self {
        Self::new(f32::from(quarter) * FRAC_PI_2, cache_origin())
    }

    /// The same view moved by `(dx, dy)` screen pixels.
    pub fn shifted(&self, dx: f32, dy: f32) -> Self {
        Self {
            origin: (self.origin.0 + dx, self.origin.1 + dy),
            ..*self
        }
    }

    /// World position turned into view axes (`u` right-down, `v` left-down).
    fn turn(&self, x: f32, y: f32) -> (f32, f32) {
        let c = SIZE as f32 / 2.0;
        let (dx, dy) = (x - c, y - c);
        (dx * self.cos - dy * self.sin, dx * self.sin + dy * self.cos)
    }

    /// Screen position of world point `(x, y)` at height `h` (in levels).
    pub fn project(&self, x: f32, y: f32, h: f32) -> (f32, f32) {
        let (u, v) = self.turn(x, y);
        (
            self.origin.0 + (u - v) * TILE_WIDTH / 2.0,
            self.origin.1 + (u + v) * TILE_HEIGHT / 2.0 - h * LEVEL_HEIGHT,
        )
    }

    /// How far towards the viewer a world point is (larger is nearer).
    pub fn depth(&self, x: f32, y: f32) -> f32 {
        let (u, v) = self.turn(x, y);
        u + v
    }

    /// The world point at height `h` that appears at screen position `(sx, sy)`.
    pub fn unproject(&self, sx: f32, sy: f32, h: f32) -> (f32, f32) {
        let a = (sx - self.origin.0) / (TILE_WIDTH / 2.0);
        let b = (sy - self.origin.1 + h * LEVEL_HEIGHT) / (TILE_HEIGHT / 2.0);
        let (u, v) = (f32::midpoint(a, b), f32::midpoint(b, -a));
        let c = SIZE as f32 / 2.0;
        (
            u * self.cos + v * self.sin + c,
            -u * self.sin + v * self.cos + c,
        )
    }

    /// Whether the outward normal `(nx, ny)` of a face points towards the viewer.
    fn faces_viewer(&self, nx: f32, ny: f32) -> bool {
        let (u, v) = (nx * self.cos - ny * self.sin, nx * self.sin + ny * self.cos);
        u + v > 0.01
    }
}

/// The sun shines from the north-north-west (world direction to the sun), so the two
/// sides a camera sees are lit differently.
const SUN: (f32, f32) = (-0.45, -0.89);

/// Heights of the four corners of a tile's top: north-west, north-east, south-east,
/// south-west (in levels).
pub fn corners(island: &Island, (x, y): (usize, usize)) -> [f32; 4] {
    let level = f32::from(island.level_at((x, y)));
    let Some(dir) = island.stair[index(x, y)] else {
        return [level; 4];
    };
    let up = level + 1.0;
    match dir {
        Dir::North => [up, up, level, level],
        Dir::East => [level, up, up, level],
        Dir::South => [level, level, up, up],
        Dir::West => [up, level, level, up],
    }
}

/// The base color of a tile's top before texture and light.
fn ground_color(ground: Ground, level: u8) -> Rgba {
    let base = match ground {
        Ground::Water => Rgba::rgb(70, 170, 190),
        Ground::Sand => Rgba::rgb(222, 200, 140),
        Ground::Grass => Rgba::rgb(92, 160, 76),
        Ground::Meadow => Rgba::rgb(118, 172, 84),
        Ground::Rock => Rgba::rgb(132, 130, 128),
    };
    scale(base, 0.86 + f32::from(level) * 0.045)
}

/// The color of a cliff side by ground.
fn side_color(ground: Ground) -> Rgba {
    match ground {
        Ground::Sand => Rgba::rgb(176, 140, 96),
        Ground::Rock => Rgba::rgb(132, 124, 120),
        _ => Rgba::rgb(128, 98, 70),
    }
}

/// `color` with its RGB multiplied by `factor`.
pub fn scale(color: Rgba, factor: f32) -> Rgba {
    let channel = |c: u8| (f32::from(c) * factor).clamp(0.0, 255.0) as u8;
    Rgba::new(
        channel(color.r),
        channel(color.g),
        channel(color.b),
        color.a,
    )
}

/// A cheap repeatable hash of two integers into 0..1.
pub fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// How far a prop's shadow reaches (world tiles), and whether it casts one.
fn shadow_radius(prop: Prop) -> Option<f32> {
    match prop {
        Prop::Palm => Some(0.34),
        Prop::Tree => Some(0.42),
        Prop::Pine => Some(0.32),
        Prop::Boulder => Some(0.3),
        Prop::Bush => Some(0.26),
        Prop::Beacon => Some(0.4),
        Prop::Flowers => None,
    }
}

/// The sprites the terrain is drawn with.
pub struct Sprites {
    props: Vec<(Prop, Canvas)>,
}

impl Sprites {
    pub fn new() -> Self {
        let props = [
            Prop::Palm,
            Prop::Tree,
            Prop::Pine,
            Prop::Boulder,
            Prop::Bush,
            Prop::Flowers,
        ]
        .into_iter()
        .map(|prop| (prop, art::prop(prop, false)))
        .collect();
        Self { props }
    }

    pub fn prop(&self, prop: Prop) -> Option<&Canvas> {
        self.props.iter().find(|(p, _)| *p == prop).map(|(_, c)| c)
    }
}

/// Draws a sprite standing with its bottom-center pixel at screen point `(x, y)`.
pub fn stand(canvas: &mut Canvas, sprite: &Canvas, (x, y): (f32, f32), options: BlitOptions) {
    let left = (x - sprite.width() as f32 / 2.0).round() as i32;
    let top = y.round() as i32 - sprite.height() as i32 + 1;
    canvas.blit(sprite, left, top, options);
}

/// Whether two screen rectangles `(x, y, width, height)` overlap.
pub fn overlaps(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// What a tile covers on the screen (top, sides and prop), as `(x, y, width, height)`.
pub fn tile_bounds(
    view: &View,
    island: &Island,
    sprites: &Sprites,
    tile: (usize, usize),
) -> (f32, f32, f32, f32) {
    let (x, y) = (tile.0 as f32, tile.1 as f32);
    let heights = corners(island, tile);
    let points = [(x, y), (x + 1.0, y), (x + 1.0, y + 1.0), (x, y + 1.0)];
    let mut left = f32::MAX;
    let mut right = f32::MIN;
    let mut top = f32::MAX;
    let mut bottom = f32::MIN;
    for (&(px, py), &h) in points.iter().zip(&heights) {
        for level in [h, 0.0] {
            let (sx, sy) = view.project(px, py, level);
            left = left.min(sx);
            right = right.max(sx);
            top = top.min(sy);
            bottom = bottom.max(sy);
        }
    }
    let prop = island.prop[index(tile.0, tile.1)];
    if let Some(sprite) = prop.and_then(|p| sprites.prop(p)) {
        let (cx, cy) = view.project(x + 0.5, y + 0.5, heights[0]);
        left = left.min(cx - sprite.width() as f32 / 2.0);
        right = right.max(cx + sprite.width() as f32 / 2.0);
        top = top.min(cy - sprite.height() as f32);
    }
    (
        left - 1.0,
        top - 1.0,
        right - left + 2.0,
        bottom - top + 2.0,
    )
}

/// Tiles worth drawing (land, and the shallow sea next to it), back to front.
pub fn draw_order(view: &View, island: &Island) -> Vec<(usize, usize)> {
    let mut tiles: Vec<(f32, (usize, usize))> = (0..SIZE * SIZE)
        .map(|i| (i % SIZE, i / SIZE))
        .filter(|&t| island.is_land(t) || is_shallow(island, t))
        .map(|t| {
            let depth = view.depth(t.0 as f32 + 0.5, t.1 as f32 + 0.5);
            (depth, t)
        })
        .collect();
    tiles.sort_by(|a, b| a.0.total_cmp(&b.0));
    tiles.into_iter().map(|(_, t)| t).collect()
}

/// Sea tiles within two steps of land are shallow and drawn lighter.
pub fn is_shallow(island: &Island, (x, y): (usize, usize)) -> bool {
    if island.is_land((x, y)) {
        return false;
    }
    (-2..=2).any(|dy: i32| {
        (-2..=2).any(|dx: i32| {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            dx.abs() + dy.abs() <= 2
                && (0..SIZE as i32).contains(&nx)
                && (0..SIZE as i32).contains(&ny)
                && island.is_land((nx as usize, ny as usize))
        })
    })
}

/// What to include when drawing a tile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layer {
    /// Everything: sides, top with shadows, prop.
    Full,
    /// Only what can hide something standing at height `.0` (in levels) behind the tile:
    /// sides and top when they rise above it, and props that stand up. Shallow sea and
    /// small plants are left out. Every part is opaque and drawn exactly as in `Full`, so
    /// drawing it again over the same tile changes nothing.
    Above(f32),
}

/// Draws one tile: its visible cliff sides, its top and its prop (not the beacon, which is
/// drawn as an actor).
pub fn draw_tile(
    canvas: &mut Canvas,
    view: &View,
    island: &Island,
    sprites: &Sprites,
    tile: (usize, usize),
    layer: Layer,
) {
    let i = index(tile.0, tile.1);
    if !island.is_land(tile) {
        if layer == Layer::Full {
            draw_shallow(canvas, view, island, tile);
        }
        return;
    }
    let rises = match layer {
        Layer::Full => true,
        Layer::Above(height) => corners(island, tile).iter().any(|&h| h > height + 0.05),
    };
    if rises {
        draw_sides(canvas, view, island, tile);
        draw_top(canvas, view, island, tile);
    }
    if let Some(prop) = island.prop[i].filter(|&p| p != Prop::Beacon) {
        if layer == Layer::Full || prop.blocks() {
            if let Some(sprite) = sprites.prop(prop) {
                let h = f32::from(island.level_at(tile));
                let at = view.project(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5, h);
                // Plants and rocks face either way, differently per tile.
                let flip = hash(tile.0 as i32, tile.1 as i32 + 99) > 0.5;
                stand(
                    canvas,
                    sprite,
                    (at.0, at.1 + 1.0),
                    BlitOptions {
                        flip_x: flip,
                        ..BlitOptions::default()
                    },
                );
            }
        }
    }
}

/// The cliff faces of a tile that look towards the viewer, down to the neighbor's top.
fn draw_sides(canvas: &mut Canvas, view: &View, island: &Island, tile: (usize, usize)) {
    let (x, y) = (tile.0 as f32, tile.1 as f32);
    let heights = corners(island, tile);
    let points = [(x, y), (x + 1.0, y), (x + 1.0, y + 1.0), (x, y + 1.0)];
    let ground = island.ground[index(tile.0, tile.1)];
    let base = side_color(ground);
    // Edges: corner a, corner b, outward normal, neighbor step.
    let edges = [
        (0, 1, (0.0, -1.0), Dir::North),
        (1, 2, (1.0, 0.0), Dir::East),
        (2, 3, (0.0, 1.0), Dir::South),
        (3, 0, (-1.0, 0.0), Dir::West),
    ];
    for (a, b, (nx, ny), dir) in edges {
        if !view.faces_viewer(nx, ny) {
            continue;
        }
        let (dx, dy) = dir.step();
        let (ox, oy) = (tile.0 as i32 + dx, tile.1 as i32 + dy);
        // The neighbor's top along this edge (its matching corners), or the sea floor.
        let (na, nb) = if (0..SIZE as i32).contains(&ox) && (0..SIZE as i32).contains(&oy) {
            let other = corners(island, (ox as usize, oy as usize));
            // The neighbor's corner at the same world point as this tile's corner.
            let point = |k: usize| points[k];
            let neighbor_points = [
                (ox as f32, oy as f32),
                (ox as f32 + 1.0, oy as f32),
                (ox as f32 + 1.0, oy as f32 + 1.0),
                (ox as f32, oy as f32 + 1.0),
            ];
            let at = |p: (f32, f32)| {
                neighbor_points
                    .iter()
                    .position(|&q| q == p)
                    .map_or(0.0, |k| other[k])
            };
            let floor = |h: f32| {
                if island.is_land((ox as usize, oy as usize)) {
                    h
                } else {
                    -0.4
                }
            };
            (floor(at(point(a))), floor(at(point(b))))
        } else {
            (-0.4, -0.4)
        };
        let (ha, hb) = (heights[a], heights[b]);
        if ha <= na && hb <= nb {
            continue;
        }
        let light = nx * SUN.0 + ny * SUN.1;
        let color = scale(base, 0.9 + light * 0.25);
        let (pa, pb) = (points[a], points[b]);
        // The side reaches a pixel into the top (drawn after it), so no gap shows
        // between them.
        let raise = |(x, y): (f32, f32)| (x, y - 1.0);
        let top_a = raise(view.project(pa.0, pa.1, ha));
        let top_b = raise(view.project(pb.0, pb.1, hb));
        let low_a = view.project(pa.0, pa.1, na.min(ha));
        let low_b = view.project(pb.0, pb.1, nb.min(hb));
        canvas.fill_triangle(top_a, top_b, low_b, color);
        canvas.fill_triangle(top_a, low_b, low_a, color);
        // A light rim along the top edge.
        canvas.line(
            top_a.0 as i32,
            top_a.1 as i32,
            top_b.0 as i32,
            top_b.1 as i32,
            scale(color, 1.18),
        );
        // Rock layers: a darker line at every whole level, a light rim at the top.
        let strata = scale(color, 0.82);
        let lowest = na.min(nb).max(0.0).ceil() as i32;
        let highest = ha.max(hb).floor() as i32;
        for level in lowest..highest {
            let level = level as f32;
            if level <= na.max(nb) {
                continue;
            }
            let (sa, sb) = (
                view.project(pa.0, pa.1, level.min(ha)),
                view.project(pb.0, pb.1, level.min(hb)),
            );
            canvas.line(sa.0 as i32, sa.1 as i32, sb.0 as i32, sb.1 as i32, strata);
        }
    }
}

/// The top of a land tile, pixel by pixel: ground texture, a soft border, stair steps
/// and the shadows of props on this tile and its neighbors.
#[allow(
    clippy::too_many_lines,
    reason = "one pass over the pixels keeps texture, steps and shadows together"
)]
fn draw_top(canvas: &mut Canvas, view: &View, island: &Island, tile: (usize, usize)) {
    let i = index(tile.0, tile.1);
    let level = island.level[i];
    let ground = island.ground[i];
    let stair = island.stair[i];
    let heights = corners(island, tile);
    let (x, y) = (tile.0 as f32, tile.1 as f32);
    let points = [(x, y), (x + 1.0, y), (x + 1.0, y + 1.0), (x, y + 1.0)];
    let projected: Vec<(f32, f32)> = points
        .iter()
        .zip(&heights)
        .map(|(&(px, py), &h)| view.project(px, py, h))
        .collect();
    let left = projected
        .iter()
        .map(|p| p.0)
        .fold(f32::MAX, f32::min)
        .floor() as i32;
    let right = projected
        .iter()
        .map(|p| p.0)
        .fold(f32::MIN, f32::max)
        .ceil() as i32;
    let top = projected
        .iter()
        .map(|p| p.1)
        .fold(f32::MAX, f32::min)
        .floor() as i32;
    let bottom = projected
        .iter()
        .map(|p| p.1)
        .fold(f32::MIN, f32::max)
        .ceil() as i32;
    let base = ground_color(ground, level);
    // Shadows falling on this tile: props here and on neighbors of the same level.
    let mut casters: Vec<(f32, f32, f32)> = Vec::new();
    if stair.is_none() {
        for dy in -1..=1_i32 {
            for dx in -1..=1_i32 {
                let (nx, ny) = (tile.0 as i32 + dx, tile.1 as i32 + dy);
                if !(0..SIZE as i32).contains(&nx) || !(0..SIZE as i32).contains(&ny) {
                    continue;
                }
                let n = (nx as usize, ny as usize);
                if island.level_at(n) != level {
                    continue;
                }
                if let Some(radius) = island.prop[index(n.0, n.1)].and_then(shadow_radius) {
                    casters.push((
                        nx as f32 + 0.5 - SUN.0 * 0.3,
                        ny as f32 + 0.5 - SUN.1 * 0.3,
                        radius,
                    ));
                }
            }
        }
    }
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    for sy in top.max(0)..=bottom.min(height - 1) {
        for sx in left.max(0)..=right.min(width - 1) {
            let (cx, cy) = (sx as f32 + 0.5, sy as f32 + 0.5);
            let (mut wx, mut wy) = view.unproject(cx, cy, f32::from(level));
            if stair.is_some() {
                // Stairs slope: find the height under the pixel in two steps.
                for _ in 0..2 {
                    let h = island.height_at(wx, wy);
                    (wx, wy) = view.unproject(cx, cy, h);
                }
            }
            if wx.floor() as i32 != tile.0 as i32 || wy.floor() as i32 != tile.1 as i32 {
                continue;
            }
            let (fx, fy) = (wx - x, wy - y);
            let texture = hash((wx * 7.0) as i32, (wy * 7.0) as i32);
            let mut color = scale(base, 0.94 + texture * 0.12);
            color = match ground {
                Ground::Grass | Ground::Meadow if texture > 0.93 => scale(base, 1.22),
                Ground::Meadow if texture < 0.025 => Rgba::rgb(240, 220, 120),
                Ground::Meadow if texture < 0.05 => Rgba::rgb(220, 130, 170),
                Ground::Sand if texture < 0.04 => scale(base, 0.8),
                Ground::Rock if texture < 0.06 => scale(base, 0.72),
                _ => color,
            };
            // A soft border so single tiles can be told apart.
            let edge = fx.min(fy).min(1.0 - fx).min(1.0 - fy);
            if edge < 0.045 {
                color = scale(color, 0.93);
            }
            if let Some(dir) = stair {
                let along = match dir {
                    Dir::North => 1.0 - fy,
                    Dir::South => fy,
                    Dir::East => fx,
                    Dir::West => 1.0 - fx,
                };
                // Four steps: each one's front edge is darker.
                if (along * 4.0).fract() < 0.22 {
                    color = scale(Rgba::rgb(160, 150, 136), 0.85);
                } else {
                    color = scale(Rgba::rgb(184, 176, 160), 0.95 + texture * 0.08);
                }
            }
            for &(px, py, radius) in &casters {
                let d = ((wx - px).powi(2) + (wy - py).powi(2) * 1.6).sqrt();
                if d < radius {
                    color = scale(color, 0.7 + 0.3 * (d / radius).powi(3));
                }
            }
            canvas.pixel(sx, sy, color);
        }
    }
}

/// Shallow sea: a light, half transparent tint over the ocean picture.
fn draw_shallow(canvas: &mut Canvas, view: &View, island: &Island, tile: (usize, usize)) {
    let (x, y) = (tile.0 as f32, tile.1 as f32);
    // Closer to land, lighter.
    let near = Dir::ALL.iter().any(|d| {
        let (dx, dy) = d.step();
        let (nx, ny) = (tile.0 as i32 + dx, tile.1 as i32 + dy);
        (0..SIZE as i32).contains(&nx)
            && (0..SIZE as i32).contains(&ny)
            && island.is_land((nx as usize, ny as usize))
    });
    let color = if near {
        Rgba::new(120, 220, 210, 110)
    } else {
        Rgba::new(90, 190, 200, 60)
    };
    let a = view.project(x, y, 0.0);
    let b = view.project(x + 1.0, y, 0.0);
    let c = view.project(x + 1.0, y + 1.0, 0.0);
    let d = view.project(x, y + 1.0, 0.0);
    canvas.fill_triangle(a, b, c, color);
    canvas.fill_triangle(a, c, d, color);
}

/// The island drawn once per camera direction (without actors and the beacon).
pub struct Terrain {
    caches: [Option<Canvas>; 4],
}

impl Terrain {
    pub fn new() -> Self {
        Self {
            caches: [None, None, None, None],
        }
    }

    /// The picture of the island seen at turn `quarter`, drawn on first use.
    pub fn picture(&mut self, island: &Island, sprites: &Sprites, quarter: u8) -> &Canvas {
        let slot = &mut self.caches[usize::from(quarter % 4)];
        slot.get_or_insert_with(|| {
            let (width, height) = cache_size();
            let mut canvas = Canvas::new(width, height);
            canvas.set_blend_mode(BlendMode::Alpha);
            let view = View::quarter(quarter % 4);
            for tile in draw_order(&view, island) {
                draw_tile(&mut canvas, &view, island, sprites, tile, Layer::Full);
            }
            canvas
        })
    }
}

/// The land tile whose top is seen at screen point `(sx, sy)`: of all tops under the point,
/// the nearest one (it is drawn last).
pub fn pick(view: &View, island: &Island, sx: f32, sy: f32) -> Option<(usize, usize)> {
    let mut best: Option<(f32, (usize, usize))> = None;
    for level in 1..=MAX_LEVEL {
        let (mut wx, mut wy) = view.unproject(sx, sy, f32::from(level));
        if !(0.0..SIZE as f32).contains(&wx) || !(0.0..SIZE as f32).contains(&wy) {
            continue;
        }
        let mut tile = (wx as usize, wy as usize);
        if island.stair[index(tile.0, tile.1)].is_some() || island.level_at(tile) + 1 == level {
            // A stair top lies between two levels: settle on its height.
            for _ in 0..3 {
                let h = island.height_at(wx, wy);
                (wx, wy) = view.unproject(sx, sy, h);
            }
            if !(0.0..SIZE as f32).contains(&wx) || !(0.0..SIZE as f32).contains(&wy) {
                continue;
            }
            tile = (wx as usize, wy as usize);
            if island.stair[index(tile.0, tile.1)].is_none() {
                continue;
            }
        } else if island.level_at(tile) != level {
            continue;
        }
        let depth = view.depth(wx, wy);
        if best.is_none_or(|(d, _)| depth > d) {
            best = Some((depth, tile));
        }
    }
    best.map(|(_, tile)| tile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_round_trips_at_any_angle() {
        for angle in [0.0, 0.3, FRAC_PI_2, 2.0, 4.0] {
            let view = View::new(angle, (100.0, 50.0));
            for (x, y, h) in [(3.0, 4.0, 0.0), (20.5, 10.25, 2.0), (39.0, 0.5, 5.0)] {
                let (sx, sy) = view.project(x, y, h);
                let (bx, by) = view.unproject(sx, sy, h);
                assert!(
                    (bx - x).abs() < 1e-3 && (by - y).abs() < 1e-3,
                    "{angle}: {bx} {by}"
                );
            }
        }
    }

    #[test]
    fn nearer_tiles_are_lower_on_the_screen() {
        for quarter in 0..4 {
            let view = View::quarter(quarter);
            let order = draw_order(&view, &Island::generate(1));
            let first = order[0];
            let last = order[order.len() - 1];
            let y = |t: (usize, usize)| view.project(t.0 as f32 + 0.5, t.1 as f32 + 0.5, 0.0).1;
            assert!(y(first) < y(last));
        }
    }

    #[test]
    fn picking_finds_the_tile_under_its_center() {
        let island = Island::generate(3);
        for quarter in 0..4 {
            let view = View::quarter(quarter);
            let mut checked = 0;
            for y in 0..SIZE {
                for x in 0..SIZE {
                    if !island.is_land((x, y)) || island.stair[index(x, y)].is_some() {
                        continue;
                    }
                    let h = f32::from(island.level_at((x, y)));
                    let (sx, sy) = view.project(x as f32 + 0.5, y as f32 + 0.5, h);
                    // Tiles hidden behind nearer ones pick the nearer tile; the rest
                    // pick themselves.
                    let picked = pick(&view, &island, sx, sy).expect("a top is there");
                    if picked == (x, y) {
                        checked += 1;
                    } else {
                        let depth =
                            |t: (usize, usize)| view.depth(t.0 as f32 + 0.5, t.1 as f32 + 0.5);
                        assert!(depth(picked) > depth((x, y)), "{picked:?} {x} {y}");
                    }
                }
            }
            assert!(checked > 200, "{checked}");
        }
    }
}
