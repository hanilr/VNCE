//! The minimap: the island as a small diamond turned like the camera, drawn into its own
//! canvas once per direction, with the player, the crystals, the beacon and the wisps put
//! on a copy every frame.

use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Rgba};

use crate::island::{Ground, Island, Prop, SIZE, index};

/// Size of the minimap picture in pixels (two pixels per tile across, one down).
pub const WIDTH: u32 = SIZE as u32 * 2 + 4;
pub const HEIGHT: u32 = SIZE as u32 + 4;

/// What moves on the minimap, in world tiles.
pub struct Markers {
    pub player: (f32, f32),
    pub crystals: Vec<(f32, f32)>,
    pub wisps: Vec<(f32, f32)>,
    pub beacon_lit: bool,
}

pub struct Minimap {
    /// The island seen from each quarter turn, drawn on first use.
    bases: [Option<Canvas>; 4],
    /// The base with this frame's markers.
    pub canvas: Canvas,
}

/// The minimap pixel of world point `(x, y)` at turn `quarter`.
pub fn locate(quarter: u8, x: f32, y: f32) -> (f32, f32) {
    let angle = f32::from(quarter % 4) * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let c = SIZE as f32 / 2.0;
    let (dx, dy) = (x - c, y - c);
    let (u, v) = (dx * cos - dy * sin, dx * sin + dy * cos);
    (
        WIDTH as f32 / 2.0 + (u - v),
        HEIGHT as f32 / 2.0 + f32::midpoint(u, v),
    )
}

impl Minimap {
    pub fn new() -> Self {
        Self {
            bases: [None, None, None, None],
            canvas: Canvas::new(WIDTH, HEIGHT),
        }
    }

    fn base(&mut self, island: &Island, quarter: u8) -> &Canvas {
        self.bases[usize::from(quarter % 4)].get_or_insert_with(|| {
            let mut canvas = Canvas::filled(WIDTH, HEIGHT, Rgba::new(10, 20, 34, 170));
            // Each tile becomes a 2 × 1 block; nearer tiles are drawn later.
            let mut tiles: Vec<(usize, usize)> =
                (0..SIZE * SIZE).map(|i| (i % SIZE, i / SIZE)).collect();
            tiles.sort_by(|a, b| {
                let depth =
                    |t: &(usize, usize)| locate(quarter, t.0 as f32 + 0.5, t.1 as f32 + 0.5).1;
                depth(a).total_cmp(&depth(b))
            });
            for (x, y) in tiles {
                let i = index(x, y);
                let level = island.level[i];
                let color = match island.ground[i] {
                    Ground::Water if crate::iso::is_shallow(island, (x, y)) => {
                        Rgba::rgb(46, 120, 150)
                    }
                    Ground::Water => continue,
                    Ground::Sand => Rgba::rgb(214, 192, 132),
                    Ground::Grass => Rgba::rgb(84, 150, 70),
                    Ground::Meadow => Rgba::rgb(110, 160, 76),
                    Ground::Rock => Rgba::rgb(130, 128, 126),
                };
                let lift = 0.8 + f32::from(level) * 0.07;
                let mut color = crate::iso::scale(color, lift);
                // Tall props (trees, the beacon) darken their tile, like a forest on a map.
                if island.prop[i].is_some_and(Prop::is_tall) {
                    color = crate::iso::scale(color, 0.7);
                }
                let (mx, my) = locate(quarter, x as f32 + 0.5, y as f32 + 0.5);
                canvas.fill_rect(mx as i32 - 1, my as i32, 2, 1, color);
            }
            canvas
        })
    }

    /// Draws the minimap for this frame: markers blink with `time`.
    pub fn draw(&mut self, island: &Island, quarter: u8, time: f32, markers: &Markers) -> &Canvas {
        let base = self.base(island, quarter).clone();
        self.canvas.clone_from(&base);
        let canvas = &mut self.canvas;
        canvas.set_blend_mode(BlendMode::Alpha);
        let blink = (time * 4.0).sin() > 0.0;
        let mark = |canvas: &mut Canvas, (x, y): (f32, f32), color: Rgba| {
            let (mx, my) = locate(quarter, x, y);
            canvas.fill_rect(mx as i32 - 1, my as i32, 2, 1, color);
        };
        for &crystal in &markers.crystals {
            mark(
                canvas,
                crystal,
                if blink {
                    Rgba::rgb(140, 230, 255)
                } else {
                    Rgba::rgb(40, 140, 220)
                },
            );
        }
        for &wisp in &markers.wisps {
            mark(canvas, wisp, Rgba::rgb(170, 255, 210));
        }
        let beacon = (island.beacon.0 as f32 + 0.5, island.beacon.1 as f32 + 0.5);
        mark(
            canvas,
            beacon,
            if markers.beacon_lit {
                Rgba::rgb(255, 230, 120)
            } else {
                Rgba::rgb(220, 70, 60)
            },
        );
        let (px, py) = locate(quarter, markers.player.0, markers.player.1);
        let color = if blink {
            Rgba::WHITE
        } else {
            Rgba::rgb(255, 170, 90)
        };
        canvas.fill_rect(px as i32 - 1, py as i32 - 1, 3, 2, color);
        canvas.rect(0, 0, WIDTH, HEIGHT, Rgba::new(255, 220, 160, 120));
        &self.canvas
    }
}

/// Copies the minimap into the top-right corner of `world`.
pub fn place(world: &mut Canvas, minimap: &Canvas) {
    let x = world.width() as i32 - minimap.width() as i32 - 3;
    world.blit(minimap, x, 3, BlitOptions::default());
}
