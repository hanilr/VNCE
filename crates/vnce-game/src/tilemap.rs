//! Grid worlds: [`TileMap`] layers drawn with a [`Tileset`].

use vnce_raster::{BlitOptions, Canvas};

use crate::collision::Aabb;
use crate::sprite::SpriteSheet;

/// A tile: an index into the frames of the [`Tileset`]'s sprite sheet.
pub type TileId = u16;

/// The images and properties of tiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tileset {
    sheet: SpriteSheet,
    solid: Vec<bool>,
}

impl Tileset {
    /// A tileset whose tile `n` is frame `n` of `sheet`. No tile is solid yet.
    #[must_use]
    pub fn new(sheet: SpriteSheet) -> Self {
        let count = sheet.frame_count();
        Self {
            sheet,
            solid: vec![false; count],
        }
    }

    /// Marks tiles as solid (builder style).
    #[must_use]
    pub fn with_solid(mut self, tiles: impl IntoIterator<Item = TileId>) -> Self {
        for tile in tiles {
            self.set_solid(tile, true);
        }
        self
    }

    /// Sets whether `tile` blocks movement.
    pub fn set_solid(&mut self, tile: TileId, solid: bool) {
        let index = usize::from(tile);
        if index >= self.solid.len() {
            self.solid.resize(index + 1, false);
        }
        self.solid[index] = solid;
    }

    /// Returns `true` if `tile` blocks movement.
    #[must_use]
    pub fn is_solid(&self, tile: TileId) -> bool {
        self.solid.get(usize::from(tile)).copied().unwrap_or(false)
    }

    /// The sprite sheet with the tile images.
    #[must_use]
    pub fn sheet(&self) -> &SpriteSheet {
        &self.sheet
    }
}

/// A grid of square tiles in one or more layers (layer 0 is drawn first).
///
/// ```
/// use vnce_game::TileMap;
///
/// let level = "\
/// #....#
/// #.o..#
/// ######";
/// let map = TileMap::from_text(level, 8, |c| (c == '#').then_some(0));
/// assert_eq!((map.width(), map.height()), (6, 3));
/// assert_eq!(map.get(0, 0, 2), Some(0));
/// assert_eq!(map.get(0, 1, 1), None);
/// assert_eq!(TileMap::find_in_text(level, 'o'), [(2, 1)]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileMap {
    width: u32,
    height: u32,
    tile_size: u32,
    layers: Vec<Vec<Option<TileId>>>,
}

impl TileMap {
    /// An empty map of `width` × `height` tiles of `tile_size` pixels with one layer.
    #[must_use]
    pub fn new(width: u32, height: u32, tile_size: u32) -> Self {
        Self {
            width,
            height,
            tile_size: tile_size.max(1),
            layers: vec![vec![None; (width as usize) * (height as usize)]],
        }
    }

    /// A one-layer map from text: one row per line, one tile per character, mapped with
    /// `legend` (`None` for an empty cell). The map is as wide as the longest line.
    pub fn from_text(text: &str, tile_size: u32, legend: impl Fn(char) -> Option<TileId>) -> Self {
        let lines: Vec<&str> = text.lines().collect();
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let mut map = Self::new(
            u32::try_from(width).unwrap_or(u32::MAX),
            u32::try_from(lines.len()).unwrap_or(u32::MAX),
            tile_size,
        );
        for (row, line) in lines.iter().enumerate() {
            for (column, c) in line.chars().enumerate() {
                map.layers[0][row * width + column] = legend(c);
            }
        }
        map
    }

    /// The `(column, row)` positions of `c` in a text level, row by row (for placing
    /// objects such as the player or coins).
    #[must_use]
    pub fn find_in_text(text: &str, c: char) -> Vec<(u32, u32)> {
        text.lines()
            .enumerate()
            .flat_map(|(row, line)| {
                line.chars()
                    .enumerate()
                    .filter(move |&(_, ch)| ch == c)
                    .map(move |(column, _)| (column, row))
            })
            .filter_map(|(column, row)| {
                Some((u32::try_from(column).ok()?, u32::try_from(row).ok()?))
            })
            .collect()
    }

    /// Adds an empty layer on top and returns its index.
    pub fn add_layer(&mut self) -> usize {
        self.layers
            .push(vec![None; (self.width as usize) * (self.height as usize)]);
        self.layers.len() - 1
    }

    /// Number of layers.
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Width in tiles.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in tiles.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Size of one tile in pixels.
    #[must_use]
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// The whole map in world pixels.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "map sizes in pixels fit in f32")]
    pub fn bounds(&self) -> Aabb {
        Aabb::new(
            0.0,
            0.0,
            (self.width * self.tile_size) as f32,
            (self.height * self.tile_size) as f32,
        )
    }

    fn index(&self, column: i64, row: i64) -> Option<usize> {
        let inside = (0..i64::from(self.width)).contains(&column)
            && (0..i64::from(self.height)).contains(&row);
        inside.then(|| usize::try_from(row * i64::from(self.width) + column).unwrap_or(0))
    }

    /// The tile at `(column, row)` of `layer` (`None` for an empty cell or outside).
    #[must_use]
    pub fn get(&self, layer: usize, column: i64, row: i64) -> Option<TileId> {
        let index = self.index(column, row)?;
        self.layers.get(layer)?[index]
    }

    /// Sets the tile at `(column, row)` of `layer`; ignored outside the map.
    pub fn set(&mut self, layer: usize, column: i64, row: i64, tile: Option<TileId>) {
        if let Some(index) = self.index(column, row) {
            if let Some(cells) = self.layers.get_mut(layer) {
                cells[index] = tile;
            }
        }
    }

    /// The tile of `layer` under the world position `(x, y)`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "world positions and tile sizes are far inside the float ranges"
    )]
    pub fn tile_at(&self, layer: usize, x: f32, y: f32) -> Option<TileId> {
        let size = self.tile_size as f32;
        self.get(layer, (x / size).floor() as i64, (y / size).floor() as i64)
    }

    /// Returns `true` if any layer has a solid tile at `(column, row)`, or if the
    /// position is outside the map (so objects stay inside).
    #[must_use]
    pub fn is_solid(&self, tileset: &Tileset, column: i32, row: i32) -> bool {
        let (column, row) = (i64::from(column), i64::from(row));
        if self.index(column, row).is_none() {
            return true;
        }
        (0..self.layers.len())
            .filter_map(|layer| self.get(layer, column, row))
            .any(|tile| tileset.is_solid(tile))
    }

    /// Draws every layer onto `target`, with the world position `offset` at the top-left
    /// corner of the target (usually [`Camera::offset`](crate::Camera::offset)). Only the
    /// visible tiles are drawn.
    pub fn draw(&self, tileset: &Tileset, target: &mut Canvas, offset: (i32, i32)) {
        let size = i64::from(self.tile_size);
        let (ox, oy) = (i64::from(offset.0), i64::from(offset.1));
        let first_column = ox.div_euclid(size).max(0);
        let first_row = oy.div_euclid(size).max(0);
        let last_column = (ox + i64::from(target.width()))
            .div_euclid(size)
            .min(i64::from(self.width) - 1);
        let last_row = (oy + i64::from(target.height()))
            .div_euclid(size)
            .min(i64::from(self.height) - 1);
        for layer in 0..self.layers.len() {
            for row in first_row..=last_row {
                for column in first_column..=last_column {
                    if let Some(tile) = self.get(layer, column, row) {
                        let x = i32::try_from(column * size - ox).unwrap_or(i32::MAX);
                        let y = i32::try_from(row * size - oy).unwrap_or(i32::MAX);
                        tileset.sheet().draw(
                            target,
                            usize::from(tile),
                            x,
                            y,
                            BlitOptions::default(),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use vnce_raster::Rgba;

    use super::*;

    fn tileset() -> Tileset {
        let wall = Canvas::filled(2, 2, Rgba::rgb(200, 200, 200));
        let grass = Canvas::filled(2, 2, Rgba::rgb(0, 200, 0));
        Tileset::new(SpriteSheet::from_frames(&[wall, grass])).with_solid([0])
    }

    #[test]
    fn text_levels_and_queries() {
        let map = TileMap::from_text("#.\n.g#", 2, |c| match c {
            '#' => Some(0),
            'g' => Some(1),
            _ => None,
        });
        assert_eq!((map.width(), map.height(), map.tile_size()), (3, 2, 2));
        assert_eq!(map.get(0, 1, 1), Some(1));
        assert_eq!(map.get(0, 2, 0), None, "short lines are padded");
        assert_eq!(map.get(0, -1, 0), None);
        assert_eq!(map.tile_at(0, 5.9, 3.0), Some(0));
        assert_eq!(map.bounds(), Aabb::new(0.0, 0.0, 6.0, 4.0));
        let tiles = tileset();
        assert!(map.is_solid(&tiles, 0, 0));
        assert!(!map.is_solid(&tiles, 1, 1), "grass is not solid");
        assert!(map.is_solid(&tiles, 9, 0), "outside is solid");
        assert!(!tiles.is_solid(500));
    }

    #[test]
    fn layers_and_editing() {
        let mut map = TileMap::new(2, 2, 4);
        let top = map.add_layer();
        assert_eq!((top, map.layer_count()), (1, 2));
        map.set(top, 1, 1, Some(0));
        map.set(top, 5, 5, Some(0));
        map.set(7, 0, 0, Some(0));
        assert_eq!(map.get(top, 1, 1), Some(0));
        assert!(map.is_solid(&tileset(), 1, 1));
        assert!(!map.is_solid(&tileset(), 0, 0));
    }

    #[test]
    fn drawing_with_an_offset() {
        let map = TileMap::from_text("#g", 2, |c| match c {
            '#' => Some(0),
            'g' => Some(1),
            _ => None,
        });
        let mut target = Canvas::new(3, 2);
        // Scrolled one pixel right: half of the wall, then the grass.
        map.draw(&tileset(), &mut target, (1, 0));
        let row: Vec<Rgba> = target.pixels()[..3].to_vec();
        assert_eq!(
            row,
            [
                Rgba::rgb(200, 200, 200),
                Rgba::rgb(0, 200, 0),
                Rgba::rgb(0, 200, 0)
            ]
        );
        // Far away: nothing is drawn.
        let mut empty = Canvas::new(3, 2);
        map.draw(&tileset(), &mut empty, (-100, -100));
        assert!(empty.pixels().iter().all(|p| p.is_transparent()));
    }

    #[test]
    fn finding_objects_in_text() {
        assert_eq!(
            TileMap::find_in_text("o.o\n..o", 'o'),
            [(0, 0), (2, 0), (2, 1)]
        );
        assert_eq!(TileMap::find_in_text("", 'o'), Vec::<(u32, u32)>::new());
    }
}
