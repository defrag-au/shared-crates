//! Hilbert curve → hang slots. The document is still `sw1`; this only
//! *fills* the table. [`World::emit`] does not know about Hilbert.

use scenegraph::dump_id;

use crate::error::Error;
use crate::world::{Hang, Wall, World};

/// Wall order inside a Hilbert cell: n, e, s, w.
pub const HILBERT_WALLS: [Wall; 4] = [Wall::N, Wall::E, Wall::S, Wall::W];

/// Side length `2^order`. Order 8 is a 256×256 cell wing.
pub const MAX_ORDER: u32 = 8;

/// One print to hang. `image` is an opaque `m=` key; `title` is dump-safe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Print {
    pub image: String,
    pub title: String,
}

impl Print {
    pub fn new(image: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            title: title.into(),
        }
    }
}

/// Smallest order whose `2^k × 2^k` cells can hold `count` hangs (4 per cell).
pub fn hilbert_order_for(count: u32) -> u32 {
    let cells = count.div_ceil(4);
    let mut k = 0u32;
    while (1u32 << (2 * k)) < cells {
        k += 1;
        if k >= MAX_ORDER {
            return MAX_ORDER;
        }
    }
    k
}

/// Grid side `2^order`.
pub fn hilbert_side(order: u32) -> Result<u32, Error> {
    if order > MAX_ORDER {
        return Err(Error::HilbertOrder(order));
    }
    Ok(1u32 << order)
}

/// Cell `(x, z)` and wall for hang index `i` on a curve of `order`.
/// +X east, +Z south. Index 0 is `(0,0)` north.
pub fn hilbert_slot(order: u32, i: u32) -> Result<((i32, i32), Wall), Error> {
    let n = hilbert_side(order)?;
    let cells = n.saturating_mul(n);
    let max = cells.saturating_mul(4);
    if i >= max {
        return Err(Error::HilbertFull { order, i });
    }
    let (x, z) = d2xy(n, i / 4);
    let wall = HILBERT_WALLS[(i % 4) as usize];
    Ok(((x as i32, z as i32), wall))
}

/// Cell for Hilbert distance `d` on a curve of `order`.
pub fn hilbert_xy(order: u32, d: u32) -> Result<(i32, i32), Error> {
    let n = hilbert_side(order)?;
    let max = n.saturating_mul(n);
    if d >= max {
        return Err(Error::HilbertFull { order, i: d });
    }
    let (x, z) = d2xy(n, d);
    Ok((x as i32, z as i32))
}

/// Inverse of [`hilbert_slot`] for the cell. `x` and `z` in `0..2^order`.
pub fn hilbert_index(order: u32, x: u32, z: u32) -> Result<u32, Error> {
    let n = hilbert_side(order)?;
    if x >= n || z >= n {
        return Err(Error::Parse(format!("cell {x},{z} outside order {order}")));
    }
    Ok(xy2d(n, x, z))
}

impl World {
    /// Place `items` along a Hilbert curve. Order is the smallest that
    /// fits. Returns that order. Empty input is a no-op (order 0).
    pub fn fill_hilbert(&mut self, items: impl IntoIterator<Item = Print>) -> Result<u32, Error> {
        let items: Vec<Print> = items.into_iter().collect();
        let order = hilbert_order_for(items.len() as u32);
        self.fill_hilbert_order(order, items)?;
        Ok(order)
    }

    /// Place `items` on a fixed-order curve. Too many items is an error
    /// (does not partial-fill). Occupied slots are an error.
    pub fn fill_hilbert_order(
        &mut self,
        order: u32,
        items: impl IntoIterator<Item = Print>,
    ) -> Result<(), Error> {
        let items: Vec<Print> = items.into_iter().collect();
        let n = hilbert_side(order)?;
        let max = n.saturating_mul(n).saturating_mul(4);
        if items.len() as u32 > max {
            return Err(Error::HilbertFull { order, i: max });
        }
        for (i, print) in items.into_iter().enumerate() {
            let ((x, z), wall) = hilbert_slot(order, i as u32)?;
            let name = unique_print_name(self, &print);
            let mut hang = Hang::new((x, z), wall, name, print.image);
            if !print.title.is_empty() {
                hang = hang.title(print.title);
            }
            self.push(hang)?;
        }
        Ok(())
    }
}

fn unique_print_name(world: &World, print: &Print) -> String {
    let mut base = dump_id(&print.title);
    if base.is_empty() {
        base = dump_id(&print.image);
    }
    if base.is_empty() {
        base = "hang".to_string();
    }
    let stem = format!("p-{base}");
    if world.hangs().iter().all(|h| h.name != stem) {
        return stem;
    }
    let mut n = 2u32;
    loop {
        let cand = format!("p-{base}-{n}");
        if world.hangs().iter().all(|h| h.name != cand) {
            return cand;
        }
        n += 1;
    }
}

/// Distance along a `n×n` Hilbert curve (`n` is a power of two).
/// Internals are signed: `rot` may flip past zero (the usual C sketch).
fn d2xy(n: u32, mut d: u32) -> (u32, u32) {
    let n = n as i32;
    let mut x = 0i32;
    let mut y = 0i32;
    let mut s = 1i32;
    while s < n {
        let rx = 1 & (d as i32 / 2);
        let ry = 1 & (d as i32 ^ rx);
        rot(s, &mut x, &mut y, rx, ry);
        x += s * rx;
        y += s * ry;
        d /= 4;
        s *= 2;
    }
    (x as u32, y as u32)
}

fn xy2d(n: u32, x: u32, y: u32) -> u32 {
    let n = n as i32;
    let mut x = x as i32;
    let mut y = y as i32;
    let mut d = 0i32;
    let mut s = n / 2;
    while s > 0 {
        let rx = i32::from((x & s) != 0);
        let ry = i32::from((y & s) != 0);
        d += s * s * ((3 * rx) ^ ry);
        rot(s, &mut x, &mut y, rx, ry);
        s /= 2;
    }
    d as u32
}

fn rot(n: i32, x: &mut i32, y: &mut i32, rx: i32, ry: i32) {
    if ry == 0 {
        if rx == 1 {
            *x = n - 1 - *x;
            *y = n - 1 - *y;
        }
        std::mem::swap(x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_two_is_the_classic_u() {
        let got: Vec<_> = (0..4).map(|d| d2xy(2, d)).collect();
        assert_eq!(got, [(0, 0), (0, 1), (1, 1), (1, 0)]);
    }

    #[test]
    fn d2xy_roundtrips() {
        for order in 0..=4 {
            let n = hilbert_side(order).unwrap();
            for d in 0..n * n {
                let (x, y) = d2xy(n, d);
                assert_eq!(xy2d(n, x, y), d, "order {order} d={d} -> {x},{y}");
            }
        }
    }

    #[test]
    fn consecutive_cells_are_cardinal_neighbours() {
        for order in 1..=4 {
            let n = hilbert_side(order).unwrap();
            for d in 0..n * n - 1 {
                let a = d2xy(n, d);
                let b = d2xy(n, d + 1);
                let manh = a.0.abs_diff(b.0) + a.1.abs_diff(b.1);
                assert_eq!(manh, 1, "order {order} {a:?} → {b:?}");
            }
        }
    }

    #[test]
    fn slot_zero_is_origin_north() {
        assert_eq!(hilbert_slot(0, 0).unwrap(), ((0, 0), Wall::N));
        assert_eq!(hilbert_slot(0, 3).unwrap(), ((0, 0), Wall::W));
        match hilbert_slot(0, 4) {
            Err(Error::HilbertFull { order, i }) => {
                assert_eq!(order, 0);
                assert_eq!(i, 4);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(hilbert_slot(1, 4).unwrap(), ((0, 1), Wall::N));
    }

    #[test]
    fn order_for_count() {
        assert_eq!(hilbert_order_for(0), 0);
        assert_eq!(hilbert_order_for(4), 0);
        assert_eq!(hilbert_order_for(5), 1);
        assert_eq!(hilbert_order_for(16), 1);
        assert_eq!(hilbert_order_for(17), 2);
        assert_eq!(hilbert_order_for(64), 2);
    }

    #[test]
    fn order_cap_errors() {
        match hilbert_side(MAX_ORDER + 1) {
            Err(Error::HilbertOrder(k)) => assert_eq!(k, MAX_ORDER + 1),
            other => panic!("{other:?}"),
        }
    }
}
