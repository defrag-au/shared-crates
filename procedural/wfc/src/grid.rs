use crate::TileId;

/// The output grid from a solved WFC run.
///
/// Each cell contains the resolved TileId.
#[derive(Debug, Clone)]
pub struct WfcGrid {
    pub width: usize,
    pub height: usize,
    cells: Vec<TileId>,
}

impl WfcGrid {
    pub(crate) fn new(width: usize, height: usize, cells: Vec<TileId>) -> Self {
        Self { width, height, cells }
    }

    /// Get the tile at (col, row).
    pub fn get(&self, col: usize, row: usize) -> TileId {
        self.cells[row * self.width + col]
    }

    /// Iterate over all cells as (col, row, tile_id).
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, TileId)> + '_ {
        (0..self.height).flat_map(move |row| {
            (0..self.width).map(move |col| (col, row, self.cells[row * self.width + col]))
        })
    }

    /// Get the underlying flat slice.
    pub fn as_slice(&self) -> &[TileId] {
        &self.cells
    }
}
