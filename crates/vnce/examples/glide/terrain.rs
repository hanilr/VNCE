//! Endless low-poly terrain from a seed: fractal value noise shaped into seas, beaches,
//! valleys and snowy ridges, cut into square chunks of flat-shaded triangles with trees
//! and rocks, and a cache that builds the chunks ahead of the camera a few at a time.

use std::collections::HashMap;

use vnce::vnce_game::Rng;
use vnce::vnce_raster::Rgba;

use crate::math::{Camera, FAR, Mat4, Vec3, dot, flat_distance, sub};
use crate::models::{PropKind, Tri, prop};

/// Meters between two grid points.
pub const CELL: f32 = 8.0;
/// Grid cells along a chunk side.
pub const CHUNK_CELLS: i32 = 32;
/// Meters along a chunk side.
pub const CHUNK_SIZE: f32 = CELL * CHUNK_CELLS as f32;
/// The water surface.
pub const SEA_LEVEL: f32 = 0.0;

/// A chunk's grid position: chunk `(x, z)` covers `x * CHUNK_SIZE ..` on both axes.
pub type ChunkKey = (i32, i32);

/// A hash of a seed and a grid point, in `0..1`.
fn hash(seed: u64, x: i32, z: i32) -> f32 {
    let mut h = seed
        ^ u64::from(x as u32).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(z as u32).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 40) as f32 / (1_u64 << 24) as f32
}

/// Smooth value noise in `0..1`.
fn noise(seed: u64, x: f32, z: f32) -> f32 {
    let (ix, iz) = (x.floor(), z.floor());
    let (fx, fz) = (x - ix, z - iz);
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v) = (fade(fx), fade(fz));
    let (ix, iz) = (ix as i32, iz as i32);
    let a = hash(seed, ix, iz);
    let b = hash(seed, ix + 1, iz);
    let c = hash(seed, ix, iz + 1);
    let d = hash(seed, ix + 1, iz + 1);
    let top = a + (b - a) * u;
    let bottom = c + (d - c) * u;
    top + (bottom - top) * v
}

/// Fractal noise: `octaves` layers, each twice as fine and half as strong, in `0..1`.
fn fbm(seed: u64, x: f32, z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amplitude, mut total, mut frequency) = (0.0, 0.5, 0.0, 1.0);
    for octave in 0..octaves {
        sum += amplitude
            * noise(
                seed.wrapping_add(u64::from(octave) * 7919),
                x * frequency,
                z * frequency,
            );
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.03;
    }
    sum / total
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A tree or rock on the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prop {
    pub kind: PropKind,
    pub position: Vec3,
    pub scale: f32,
    pub yaw: f32,
    pub variant: u32,
}

impl Prop {
    /// Radius and height in meters, for collisions.
    pub fn size(&self) -> (f32, f32) {
        let (radius, height) = self.kind.size();
        (radius * self.scale, height * self.scale)
    }

    pub fn model(&self) -> Mat4 {
        Mat4::translation(self.position) * Mat4::heading(self.yaw) * Mat4::scale([self.scale; 3])
    }
}

/// The triangles and props of one chunk.
#[derive(Debug, Clone, Default)]
pub struct ChunkGeometry {
    /// The ground and the water.
    pub tris: Vec<Tri>,
    /// The trees and rocks (the CPU leaves them out far away).
    pub prop_tris: Vec<Tri>,
    pub props: Vec<Prop>,
    /// Lowest and highest point, for visibility tests.
    pub low: f32,
    pub high: f32,
}

/// The landscape of one seed.
#[derive(Debug, Clone)]
pub struct Terrain {
    pub seed: u64,
}

impl Terrain {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// The natural height at `(x, z)`: below [`SEA_LEVEL`] is under water.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let s = self.seed;
        let land = fbm(s, x / 1400.0, z / 1400.0, 4);
        let hills = fbm(s ^ 0x51, x / 300.0, z / 300.0, 3);
        let ridge = 1.0 - (2.0 * noise(s ^ 0xA7, x / 700.0, z / 700.0) - 1.0).abs();
        let mountains = smoothstep(0.48, 0.72, land) * ridge * ridge * 210.0;
        (land - 0.42) * 300.0 + mountains + (hills - 0.5) * 46.0
    }

    /// The surface (water or ground) at a grid point.
    fn grid_surface(&self, gx: i32, gz: i32) -> f32 {
        self.height(gx as f32 * CELL, gz as f32 * CELL)
            .max(SEA_LEVEL)
    }

    /// The height of the drawn surface at `(x, z)` (on the full-detail triangles, water
    /// flat at sea level): what the craft can hit.
    pub fn surface(&self, x: f32, z: f32) -> f32 {
        let (gx, gz) = ((x / CELL).floor(), (z / CELL).floor());
        let (fx, fz) = (x / CELL - gx, z / CELL - gz);
        let (gx, gz) = (gx as i32, gz as i32);
        let a = self.grid_surface(gx, gz);
        let b = self.grid_surface(gx + 1, gz);
        let c = self.grid_surface(gx + 1, gz + 1);
        let d = self.grid_surface(gx, gz + 1);
        // The same split as the mesh: triangles a-d-c and a-c-b.
        if fz >= fx {
            a + (c - d) * fx + (d - a) * fz
        } else {
            a + (b - a) * fx + (c - b) * fz
        }
    }

    /// The ground's color for a face whose corners have natural heights `heights`, with an
    /// upward normal component `up` and a little variation `jitter` (0..1).
    fn color(heights: [f32; 3], up: f32, jitter: f32) -> Rgba {
        let mean = (heights[0] + heights[1] + heights[2]) / 3.0;
        let wet = heights.iter().filter(|&&h| h < SEA_LEVEL).count();
        let base = if wet == 3 {
            let depth = smoothstep(0.0, 40.0, -mean);
            Rgba::rgb(64, 160, 172).lerp(Rgba::rgb(22, 64, 112), depth)
        } else if wet > 0 || mean < 3.0 {
            Rgba::rgb(218, 200, 146)
        } else if mean > 165.0 + jitter * 20.0 && up > 0.55 {
            Rgba::rgb(236, 240, 246)
        } else if up < 0.72 || mean > 135.0 {
            Rgba::rgb(128, 120, 112).lerp(Rgba::rgb(150, 142, 132), jitter)
        } else if mean > 80.0 {
            Rgba::rgb(104, 124, 76)
        } else {
            Rgba::rgb(92, 152, 68).lerp(Rgba::rgb(66, 124, 58), smoothstep(10.0, 70.0, mean))
        };
        // A little variation from face to face keeps the low-poly look.
        let shade = 0.94 + jitter * 0.12;
        let channel = |c: u8| (f32::from(c) * shade).min(255.0) as u8;
        Rgba::rgb(channel(base.r), channel(base.g), channel(base.b))
    }

    /// The triangles of chunk `key`, one grid point every `step` cells (1: full detail),
    /// with its trees and rocks when `props` is set.
    pub fn chunk(&self, key: ChunkKey, step: i32, props: bool) -> ChunkGeometry {
        let count = (CHUNK_CELLS / step) as usize;
        let (gx0, gz0) = (key.0 * CHUNK_CELLS, key.1 * CHUNK_CELLS);
        // Natural heights of the grid points, and their positions on the surface.
        let mut natural = Vec::with_capacity((count + 1) * (count + 1));
        for j in 0..=count {
            for i in 0..=count {
                let (gx, gz) = (gx0 + i as i32 * step, gz0 + j as i32 * step);
                natural.push(self.height(gx as f32 * CELL, gz as f32 * CELL));
            }
        }
        let point = |i: usize, j: usize| {
            let (gx, gz) = (gx0 + i as i32 * step, gz0 + j as i32 * step);
            [
                gx as f32 * CELL,
                natural[j * (count + 1) + i].max(SEA_LEVEL),
                gz as f32 * CELL,
            ]
        };
        let height = |i: usize, j: usize| natural[j * (count + 1) + i];
        let mut geometry = ChunkGeometry {
            tris: Vec::with_capacity(count * count * 2),
            prop_tris: Vec::new(),
            props: Vec::new(),
            low: f32::MAX,
            high: f32::MIN,
        };
        for j in 0..count {
            for i in 0..count {
                let (a, b, c, d) = (
                    point(i, j),
                    point(i + 1, j),
                    point(i + 1, j + 1),
                    point(i, j + 1),
                );
                let heights = (
                    height(i, j),
                    height(i + 1, j),
                    height(i + 1, j + 1),
                    height(i, j + 1),
                );
                for (half, (corners, natural)) in [
                    ([a, d, c], [heights.0, heights.3, heights.2]),
                    ([a, c, b], [heights.0, heights.2, heights.1]),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut tri = Tri::new(corners[0], corners[1], corners[2], Rgba::BLACK);
                    if tri.n[1] < 0.0 {
                        tri = Tri::new(corners[0], corners[2], corners[1], Rgba::BLACK);
                    }
                    let (gx, gz) = (gx0 + i as i32 * step, gz0 + j as i32 * step);
                    let jitter = hash(self.seed ^ 0x77, gx * 2 + half as i32, gz);
                    tri.color = Self::color(natural, tri.n[1], jitter);
                    geometry.tris.push(tri);
                }
                for p in [a, b, c, d] {
                    geometry.low = geometry.low.min(p[1]);
                    geometry.high = geometry.high.max(p[1]);
                }
            }
        }
        if props {
            geometry.props = self.props(key);
            for item in &geometry.props {
                let model = item.model();
                geometry.prop_tris.extend(
                    prop(item.kind, item.variant)
                        .iter()
                        .map(|t| t.transformed(&model)),
                );
                geometry.high = geometry.high.max(item.position[1] + item.size().1);
            }
        }
        geometry
    }

    /// The trees and rocks of chunk `key` (the same every time).
    pub fn props(&self, key: ChunkKey) -> Vec<Prop> {
        let mut rng = Rng::new(
            self.seed
                ^ (key.0 as u64).wrapping_mul(0x1F1F_1F1F)
                ^ (key.1 as u64).wrapping_mul(0x7A7A_0101_7A7A),
        );
        let mut props = Vec::new();
        for _ in 0..70 {
            let x = (key.0 as f32 + rng.unit()) * CHUNK_SIZE;
            let z = (key.1 as f32 + rng.unit()) * CHUNK_SIZE;
            let h = self.height(x, z);
            if h < 4.0 {
                continue;
            }
            let slope = (self.height(x + 4.0, z) - h)
                .abs()
                .max((self.height(x, z + 4.0) - h).abs())
                / 4.0;
            let forest = fbm(self.seed ^ 0x3C, x / 420.0, z / 420.0, 2);
            let kind = if slope > 0.9 || h > 150.0 {
                if rng.chance(0.12) {
                    PropKind::Rock
                } else {
                    continue;
                }
            } else if forest > 0.52 && rng.chance(0.85) {
                if h > 60.0 || rng.chance(0.3) {
                    PropKind::Pine
                } else {
                    PropKind::Broadleaf
                }
            } else if rng.chance(0.06) {
                if rng.chance(0.5) {
                    PropKind::Rock
                } else {
                    PropKind::Broadleaf
                }
            } else {
                continue;
            };
            props.push(Prop {
                kind,
                position: [x, self.surface(x, z), z],
                scale: 0.8 + rng.unit() * 0.6,
                yaw: rng.unit() * std::f32::consts::TAU,
                variant: rng.next_u32() % 8,
            });
        }
        props
    }
}

/// The chunk that contains `(x, z)`.
pub fn chunk_of(x: f32, z: f32) -> ChunkKey {
    (
        (x / CHUNK_SIZE).floor() as i32,
        (z / CHUNK_SIZE).floor() as i32,
    )
}

/// The middle of a chunk at height 0.
pub fn chunk_center(key: ChunkKey) -> Vec3 {
    [
        (key.0 as f32 + 0.5) * CHUNK_SIZE,
        0.0,
        (key.1 as f32 + 0.5) * CHUNK_SIZE,
    ]
}

/// A built chunk with the bounds of its contents.
pub struct Chunk<T> {
    pub data: T,
    pub low: f32,
    pub high: f32,
}

/// Chunks built around the camera, the nearest and the ones ahead first.
pub struct ChunkCache<T> {
    chunks: HashMap<ChunkKey, Chunk<T>>,
    /// Chunks within this many chunk sizes are kept and drawn.
    pub radius: f32,
}

impl<T> ChunkCache<T> {
    pub fn new(radius: f32) -> Self {
        Self {
            chunks: HashMap::new(),
            radius,
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    /// Builds up to `budget` missing chunks around `eye` (those ahead of `forward` first)
    /// and forgets chunks that are far away. Returns how many were built.
    pub fn update(
        &mut self,
        eye: Vec3,
        forward: Vec3,
        budget: usize,
        mut build: impl FnMut(ChunkKey) -> Chunk<T>,
    ) -> usize {
        let reach = self.radius * CHUNK_SIZE;
        self.chunks
            .retain(|&key, _| flat_distance(chunk_center(key), eye) <= reach + CHUNK_SIZE * 1.5);
        let center = chunk_of(eye[0], eye[2]);
        let span = self.radius.ceil() as i32 + 1;
        let flat = {
            let length = forward[0].hypot(forward[2]);
            if length > 1e-3 {
                [forward[0] / length, 0.0, forward[2] / length]
            } else {
                [0.0; 3]
            }
        };
        let mut missing = Vec::new();
        for dz in -span..=span {
            for dx in -span..=span {
                let key = (center.0 + dx, center.1 + dz);
                let offset = sub(chunk_center(key), [eye[0], 0.0, eye[2]]);
                let distance = offset[0].hypot(offset[2]);
                if distance > reach || self.chunks.contains_key(&key) {
                    continue;
                }
                let ahead = if distance > 1.0 {
                    dot(offset, flat) / distance
                } else {
                    1.0
                };
                missing.push((distance * (1.5 - 0.5 * ahead), key));
            }
        }
        missing.sort_by(|a, b| a.0.total_cmp(&b.0));
        let count = missing.len().min(budget);
        for &(_, key) in &missing[..count] {
            let chunk = build(key);
            self.chunks.insert(key, chunk);
        }
        count
    }

    /// The chunks the camera can see, with their keys.
    pub fn visible<'a>(
        &'a self,
        camera: &'a Camera,
    ) -> impl Iterator<Item = (ChunkKey, &'a T)> + 'a {
        let reach = self.radius * CHUNK_SIZE;
        self.chunks.iter().filter_map(move |(&key, chunk)| {
            let mut center = chunk_center(key);
            if flat_distance(center, camera.eye) > reach + CHUNK_SIZE {
                return None;
            }
            center[1] = f32::midpoint(chunk.low, chunk.high);
            let half_height = (chunk.high - chunk.low) / 2.0;
            let radius = (CHUNK_SIZE * CHUNK_SIZE / 2.0 + half_height * half_height).sqrt();
            camera_sees(camera, center, radius).then_some((key, &chunk.data))
        })
    }
}

/// Whether a sphere is (at least partly) inside the camera's view.
pub fn camera_sees(camera: &Camera, center: Vec3, radius: f32) -> bool {
    let offset = sub(center, camera.eye);
    let depth = dot(offset, camera.forward);
    if depth < -radius || depth > FAR + radius {
        return false;
    }
    let tan = (crate::math::FOV_Y / 2.0).tan();
    let (tx, ty) = (tan * camera.aspect, tan);
    // Distance from a side plane: the sphere is out when it is wholly beyond it.
    let x = dot(offset, camera.right()).abs();
    let y = dot(offset, camera.up).abs();
    let out_x = (x - depth * tx) / (1.0 + tx * tx).sqrt();
    let out_y = (y - depth * ty) / (1.0 + ty * ty).sqrt();
    out_x <= radius && out_y <= radius
}

/// The highest surface in a circle (sampled), for placing things above the ground.
pub fn highest_around(terrain: &Terrain, center: Vec3, radius: f32) -> f32 {
    let mut high = terrain.surface(center[0], center[2]);
    for ring in 1..=2 {
        let r = radius * ring as f32 / 2.0;
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            let p = [center[0] + r * a.cos(), 0.0, center[2] + r * a.sin()];
            high = high.max(terrain.surface(p[0], p[2]));
        }
    }
    high
}

/// A unit vector along the ground's slope normal at `(x, z)`.
pub fn ground_normal(terrain: &Terrain, x: f32, z: f32) -> Vec3 {
    let d = 2.0;
    let dx = terrain.surface(x + d, z) - terrain.surface(x - d, z);
    let dz = terrain.surface(x, z + d) - terrain.surface(x, z - d);
    crate::math::normalize([-dx, 2.0 * d, -dz])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::scale;

    #[test]
    fn the_same_seed_gives_the_same_land() {
        let (a, b, c) = (Terrain::new(7), Terrain::new(7), Terrain::new(8));
        let points = [(0.0, 0.0), (1234.5, -987.0), (-5000.0, 300.0)];
        for (x, z) in points {
            assert!((a.height(x, z) - b.height(x, z)).abs() < f32::EPSILON);
        }
        assert!(
            points
                .iter()
                .any(|&(x, z)| (a.height(x, z) - c.height(x, z)).abs() > 1.0)
        );
    }

    #[test]
    fn heights_make_seas_land_and_peaks() {
        let terrain = Terrain::new(3);
        let mut heights = Vec::new();
        for j in 0..120 {
            for i in 0..120 {
                heights.push(terrain.height(i as f32 * 90.0 - 5400.0, j as f32 * 90.0 - 5400.0));
            }
        }
        let water =
            heights.iter().filter(|&&h| h < SEA_LEVEL).count() as f32 / heights.len() as f32;
        let high = heights.iter().copied().fold(f32::MIN, f32::max);
        assert!((0.08..0.6).contains(&water), "water {water}");
        assert!(high > 120.0 && high < 520.0, "highest {high}");
    }

    #[test]
    fn surface_matches_the_mesh_and_chunks_meet() {
        let terrain = Terrain::new(11);
        let key = (2, -3);
        let chunk = terrain.chunk(key, 1, false);
        assert_eq!(chunk.tris.len(), (CHUNK_CELLS * CHUNK_CELLS * 2) as usize);
        // Every triangle faces up and its middle lies on the surface.
        for tri in &chunk.tris {
            assert!(tri.n[1] > 0.0);
            let m = scale(
                crate::math::add(crate::math::add(tri.p[0], tri.p[1]), tri.p[2]),
                1.0 / 3.0,
            );
            assert!((terrain.surface(m[0], m[2]) - m[1]).abs() < 1e-2, "{m:?}");
        }
        // The east edge of this chunk is the west edge of the next one.
        let next = terrain.chunk((3, -3), 1, false);
        let x = 3.0 * CHUNK_SIZE;
        let edge = |geometry: &ChunkGeometry| {
            let mut points: Vec<(i32, i32)> = geometry
                .tris
                .iter()
                .flat_map(|t| t.p)
                .filter(|p| (p[0] - x).abs() < 1e-3)
                .map(|p| ((p[2] * 10.0).round() as i32, (p[1] * 100.0).round() as i32))
                .collect();
            points.sort_unstable();
            points.dedup();
            points
        };
        assert_eq!(edge(&chunk), edge(&next));
        // Half detail has a quarter of the triangles.
        assert_eq!(
            terrain.chunk(key, 2, false).tris.len(),
            chunk.tris.len() / 4
        );
    }

    #[test]
    fn props_stand_on_dry_ground() {
        let terrain = Terrain::new(5);
        let mut count = 0;
        for key in [(0, 0), (1, 0), (0, 1), (-2, 3), (4, -1)] {
            for item in terrain.props(key) {
                assert!(item.position[1] >= SEA_LEVEL);
                assert_eq!(chunk_of(item.position[0], item.position[2]), key);
                count += 1;
            }
            assert_eq!(terrain.props(key), terrain.props(key));
        }
        assert!(count > 0);
    }

    #[test]
    fn the_cache_builds_ahead_and_forgets_behind() {
        let mut cache = ChunkCache::new(2.0);
        let build = |_| Chunk {
            data: (),
            low: 0.0,
            high: 1.0,
        };
        let first = cache.update([0.0, 50.0, 0.0], [0.0, 0.0, -1.0], 3, build);
        assert_eq!(first, 3);
        while cache.update([0.0, 50.0, 0.0], [0.0, 0.0, -1.0], 3, build) > 0 {}
        let near = cache.len();
        assert!(near >= 9, "{near}");
        // Far away: the old chunks are dropped, new ones built.
        cache.update([20.0 * CHUNK_SIZE, 50.0, 0.0], [1.0, 0.0, 0.0], 100, build);
        assert_eq!(cache.len(), near);
        let camera = crate::math::camera(
            [20.0 * CHUNK_SIZE, 50.0, 0.0],
            [1.0, -0.2, 0.0],
            [0.0, 1.0, 0.0],
            1.5,
        );
        let seen = cache.visible(&camera).count();
        assert!(seen > 0 && seen < near, "{seen} of {near}");
    }
}
