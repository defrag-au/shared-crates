//! Dense packing of square tiles into a fixed number of columns.
//!
//! Each tile asks for a side in cells (1 for an ordinary tile, 2 or 3 for one
//! with visual priority). Tiles are placed in the order given — the host's
//! order is its ranking — each at the first position, reading row by row,
//! where its whole square is free. A later small tile may fill a hole an
//! earlier big one left, so the wall stays tight: the CSS grid's "dense" flow.
//!
//! Pure: cells in, cells out, so every arrangement is tested without a window.

/// The largest side a tile may ask for. A wider ask is clamped to the wall.
pub const MAX_SPAN: u8 = 4;

/// Where a tile sits, in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    pub col: u16,
    pub row: u16,
    /// Its side, in cells: the ask, clamped to the wall's width.
    pub span: u8,
}

impl Cell {
    /// The row below its bottom edge.
    pub fn bottom(&self) -> u16 {
        self.row + u16::from(self.span)
    }

    pub fn overlaps(&self, other: &Cell) -> bool {
        let (a, b) = (self, other);
        let (aw, bw) = (u16::from(a.span), u16::from(b.span));
        a.col < b.col + bw && b.col < a.col + aw && a.row < b.row + bw && b.row < a.row + aw
    }
}

/// The side a tile asking for `span` gets on a wall `cols` wide.
pub fn clamp_span(span: u8, cols: u16) -> u8 {
    let widest = cols.clamp(1, u16::from(MAX_SPAN)) as u8;
    span.clamp(1, widest)
}

/// Place tiles asking for `spans`, in order, on a wall `cols` wide. One cell
/// per ask, in the same order.
pub fn pack(cols: u16, spans: &[u8]) -> Vec<Cell> {
    let cols = cols.max(1);
    let width = usize::from(cols);
    // Rows of occupancy, grown as tiles go lower.
    let mut taken: Vec<Vec<bool>> = Vec::new();
    // The first row with a free cell: nothing above it can take a tile.
    let mut floor = 0usize;
    let mut out = Vec::with_capacity(spans.len());

    for &ask in spans {
        let span = clamp_span(ask, cols);
        let s = usize::from(span);
        let (row, col) = 'search: {
            let mut row = floor;
            loop {
                for col in 0..=(width - s) {
                    if fits(&taken, row, col, s) {
                        break 'search (row, col);
                    }
                }
                row += 1;
            }
        };
        while taken.len() < row + s {
            taken.push(vec![false; width]);
        }
        for line in taken.iter_mut().skip(row).take(s) {
            for c in line.iter_mut().skip(col).take(s) {
                *c = true;
            }
        }
        while floor < taken.len() && taken[floor].iter().all(|c| *c) {
            floor += 1;
        }
        out.push(Cell {
            col: col as u16,
            row: row as u16,
            span,
        });
    }
    out
}

fn fits(taken: &[Vec<bool>], row: usize, col: usize, s: usize) -> bool {
    (row..row + s).all(|r| match taken.get(r) {
        Some(line) => line[col..col + s].iter().all(|c| !*c),
        None => true,
    })
}

/// Rows the packed wall uses.
pub fn rows(cells: &[Cell]) -> u16 {
    cells.iter().map(Cell::bottom).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holes(cols: u16, cells: &[Cell]) -> Vec<(u16, u16)> {
        let mut out = Vec::new();
        let last = rows(cells);
        for row in 0..last {
            for col in 0..cols {
                let probe = Cell { col, row, span: 1 };
                if !cells.iter().any(|c| c.overlaps(&probe)) {
                    out.push((col, row));
                }
            }
        }
        out
    }

    #[test]
    fn ordinary_tiles_read_row_by_row_in_order() {
        let cells = pack(3, &[1; 7]);
        let at: Vec<(u16, u16)> = cells.iter().map(|c| (c.col, c.row)).collect();
        assert_eq!(at, [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 2)]);
    }

    #[test]
    fn a_big_tile_takes_its_square_and_small_ones_pack_round_it() {
        // A 2x2 first on a 3-wide wall: the column beside it fills first.
        let cells = pack(3, &[2, 1, 1, 1, 1, 1]);
        assert_eq!(
            cells[0],
            Cell {
                col: 0,
                row: 0,
                span: 2
            }
        );
        assert_eq!((cells[1].col, cells[1].row), (2, 0));
        assert_eq!((cells[2].col, cells[2].row), (2, 1));
        assert_eq!((cells[3].col, cells[3].row), (0, 2));
        assert!(holes(3, &cells).is_empty());
    }

    #[test]
    fn a_later_small_tile_fills_a_hole_a_big_one_left() {
        // 1, then a 2x2 that cannot fit beside it on a 2-wide... on 3 wide it
        // goes to (1,0); the next 1x1 fills (0,1) beside it.
        let cells = pack(3, &[1, 2, 1]);
        assert_eq!((cells[1].col, cells[1].row), (1, 0));
        assert_eq!((cells[2].col, cells[2].row), (0, 1));
    }

    #[test]
    fn nothing_overlaps_and_everything_is_on_the_wall() {
        // A ranking like a real collection: a legend, a few boosted, a crowd.
        let mut spans = vec![3, 2, 2, 2, 2];
        spans.extend([1; 60]);
        spans.insert(20, 2);
        spans.insert(41, 3);
        for cols in [3u16, 4, 7, 10] {
            let cells = pack(cols, &spans);
            for (i, a) in cells.iter().enumerate() {
                assert!(a.col + u16::from(a.span) <= cols, "{cols}: {a:?}");
                for b in &cells[i + 1..] {
                    assert!(!a.overlaps(b), "{cols}: {a:?} overlaps {b:?}");
                }
            }
            // Holes only where nothing small was left to fill them: none in
            // any row but the last two.
            let last = rows(&cells);
            let early: Vec<_> = holes(cols, &cells)
                .into_iter()
                .filter(|(_, row)| row + 2 < last)
                .collect();
            assert!(early.is_empty(), "{cols} wide: holes at {early:?}");
        }
    }

    #[test]
    fn an_ask_wider_than_the_wall_is_clamped() {
        assert_eq!(pack(2, &[3])[0].span, 2);
        assert_eq!(
            pack(1, &[2, 1])[1],
            Cell {
                col: 0,
                row: 1,
                span: 1
            }
        );
        assert_eq!(clamp_span(9, 10), MAX_SPAN);
        assert_eq!(clamp_span(0, 3), 1);
    }

    #[test]
    fn an_empty_wall_has_no_rows() {
        assert!(pack(3, &[]).is_empty());
        assert_eq!(rows(&[]), 0);
    }
}
