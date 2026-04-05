use crate::Vec2;

/// Simple grid-based spatial index for fast proximity queries.
pub struct SpatialGrid {
    cell_size: f32,
    cols: usize,
    rows: usize,
    offset_x: f32,
    offset_y: f32,
    cells: Vec<Vec<usize>>,
}

impl SpatialGrid {
    /// Create a spatial grid covering a given bounds.
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32, cell_size: f32) -> Self {
        let cols = ((max_x - min_x) / cell_size).ceil() as usize + 1;
        let rows = ((max_y - min_y) / cell_size).ceil() as usize + 1;
        Self {
            cell_size,
            cols,
            rows,
            offset_x: min_x,
            offset_y: min_y,
            cells: vec![Vec::new(); cols * rows],
        }
    }

    /// Insert a point with the given index.
    pub fn insert(&mut self, pos: Vec2, index: usize) {
        let (col, row) = self.to_cell(pos);
        if col < self.cols && row < self.rows {
            self.cells[row * self.cols + col].push(index);
        }
    }

    /// Insert a line segment, adding its index to all cells it crosses.
    #[allow(dead_code)]
    pub fn insert_segment(&mut self, a: Vec2, b: Vec2, index: usize) {
        let (c0, r0) = self.to_cell(a);
        let (c1, r1) = self.to_cell(b);

        let min_c = c0.min(c1);
        let max_c = (c0.max(c1)).min(self.cols - 1);
        let min_r = r0.min(r1);
        let max_r = (r0.max(r1)).min(self.rows - 1);

        for r in min_r..=max_r {
            for c in min_c..=max_c {
                self.cells[r * self.cols + c].push(index);
            }
        }
    }

    /// Find all indices in cells within `radius` of `pos`.
    /// Returns candidate indices (caller should do precise distance check if needed).
    pub fn query_radius(&self, pos: Vec2, radius: f32) -> Vec<usize> {
        let mut result = Vec::new();

        let (cc, cr) = self.to_cell(pos);
        let span = (radius / self.cell_size).ceil() as usize + 1;

        let min_c = cc.saturating_sub(span);
        let max_c = (cc + span).min(self.cols - 1);
        let min_r = cr.saturating_sub(span);
        let max_r = (cr + span).min(self.rows - 1);

        for r in min_r..=max_r {
            for c in min_c..=max_c {
                for &idx in &self.cells[r * self.cols + c] {
                    if !result.contains(&idx) {
                        result.push(idx);
                    }
                }
            }
        }

        result
    }

    /// Check if any stored point is within `radius` of `pos`.
    pub fn has_nearby(&self, pos: Vec2, radius: f32, points: &[Vec2]) -> bool {
        let radius_sq = radius * radius;
        let (cc, cr) = self.to_cell(pos);
        let span = (radius / self.cell_size).ceil() as usize + 1;

        let min_c = cc.saturating_sub(span);
        let max_c = (cc + span).min(self.cols - 1);
        let min_r = cr.saturating_sub(span);
        let max_r = (cr + span).min(self.rows - 1);

        for r in min_r..=max_r {
            for c in min_c..=max_c {
                for &idx in &self.cells[r * self.cols + c] {
                    if idx < points.len() {
                        let d = points[idx] - pos;
                        if d.x * d.x + d.y * d.y < radius_sq {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    fn to_cell(&self, pos: Vec2) -> (usize, usize) {
        let col = ((pos.x - self.offset_x) / self.cell_size).floor().max(0.0) as usize;
        let row = ((pos.y - self.offset_y) / self.cell_size).floor().max(0.0) as usize;
        (col.min(self.cols - 1), row.min(self.rows - 1))
    }
}
