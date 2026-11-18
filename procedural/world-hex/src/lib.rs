//! Hex navigation on a world sphere, built on [H3](https://h3geo.org) via
//! [`h3o`].
//!
//! A world (for example a `world-bake` bake) speaks in unit directions; H3
//! speaks in latitude and longitude. A [`Frame`] joins them: it rotates world
//! directions into H3's sphere, so cells can be looked up from a direction and
//! their centres and outlines turned back into directions. The rotation is
//! free to choose, which matters because H3 always has twelve pentagons per
//! resolution: [`Frame::search`] picks a rotation that keeps them off land.
//!
//! On top of that:
//!
//! - [`neighbours`] and [`disk`] — H3's rings, sorted so results are the same
//!   on every build (`h3o`'s slow path collects through a hash set whose order
//!   is seeded at compile time).
//! - [`path`] — A* over cells with a caller's step costs (the `pathfinding`
//!   crate), with H3 grid distance as the heuristic.
//! - [`Patch`] — flat `i, j` hex coordinates around an anchor cell, so flat
//!   hex tooling (wave function collapse, local layouts) can run on a region.
//! - [`samples`] — points spread over a cell, for summarising a world layer
//!   into one value per cell.
//!
//! Directions use `world-bake`'s convention: the pole is +y, latitude is
//! `asin(y)`, longitude runs from +x towards +z. With [`Frame::identity`] a
//! world longitude and latitude are the H3 ones.

use h3o::{CoordIJ, LatLng, LocalIJ};
use pathfinding::directed::astar::astar;

pub use h3o::{CellIndex, Resolution};

/// A unit direction on the world sphere.
pub type Dir = [f32; 3];

/// The rotation from world directions into H3's sphere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Row-major rotation matrix, world → H3 frame.
    rotation: [[f64; 3]; 3],
}

impl Default for Frame {
    fn default() -> Self {
        Self::identity()
    }
}

impl Frame {
    /// World longitude and latitude are H3's.
    pub const fn identity() -> Self {
        Self {
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    /// A rotation by `angle` radians about `axis` (need not be unit).
    pub fn axis_angle(axis: [f64; 3], angle: f64) -> Self {
        let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        let [x, y, z] = if len > 0.0 {
            [axis[0] / len, axis[1] / len, axis[2] / len]
        } else {
            [0.0, 1.0, 0.0]
        };
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        Self {
            rotation: [
                [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
                [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
                [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
            ],
        }
    }

    /// The row-major rotation matrix, world → H3 frame (for recording a
    /// chosen frame beside a world).
    pub const fn matrix(&self) -> [[f64; 3]; 3] {
        self.rotation
    }

    /// A frame from a matrix recorded by [`Frame::matrix`].
    pub const fn from_matrix(rotation: [[f64; 3]; 3]) -> Self {
        Self { rotation }
    }

    /// A world direction as H3 latitude and longitude.
    fn h3_of(&self, dir: Dir) -> LatLng {
        let d = [dir[0] as f64, dir[1] as f64, dir[2] as f64];
        let r = &self.rotation;
        let v = [
            r[0][0] * d[0] + r[0][1] * d[1] + r[0][2] * d[2],
            r[1][0] * d[0] + r[1][1] * d[1] + r[1][2] * d[2],
            r[2][0] * d[0] + r[2][1] * d[1] + r[2][2] * d[2],
        ];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2])
            .sqrt()
            .max(f64::MIN_POSITIVE);
        let lat = (v[1] / len).clamp(-1.0, 1.0).asin();
        let lng = v[2].atan2(v[0]);
        LatLng::from_radians(lat, lng).expect("finite latitude and longitude")
    }

    /// H3 latitude and longitude as a world direction.
    fn world_of(&self, ll: LatLng) -> Dir {
        let (lat, lng) = (ll.lat_radians(), ll.lng_radians());
        let v = [lat.cos() * lng.cos(), lat.sin(), lat.cos() * lng.sin()];
        // The inverse of a rotation is its transpose.
        let r = &self.rotation;
        [
            (r[0][0] * v[0] + r[1][0] * v[1] + r[2][0] * v[2]) as f32,
            (r[0][1] * v[0] + r[1][1] * v[1] + r[2][1] * v[2]) as f32,
            (r[0][2] * v[0] + r[1][2] * v[1] + r[2][2] * v[2]) as f32,
        ]
    }

    /// The cell a world direction falls in.
    pub fn cell_at(&self, dir: Dir, resolution: Resolution) -> CellIndex {
        self.h3_of(dir).to_cell(resolution)
    }

    /// A cell's centre, as a world direction.
    pub fn centre(&self, cell: CellIndex) -> Dir {
        self.world_of(LatLng::from(cell))
    }

    /// A cell's outline, as world directions in order (six corners for a
    /// hexagon, five for a pentagon; more where a cell crosses an
    /// icosahedron edge, as H3 adds distortion vertices there).
    pub fn boundary(&self, cell: CellIndex) -> Vec<Dir> {
        cell.boundary()
            .iter()
            .map(|&ll| self.world_of(ll))
            .collect()
    }

    /// The twelve pentagons at `resolution`, with their centres as world
    /// directions.
    pub fn pentagons(&self, resolution: Resolution) -> Vec<(CellIndex, Dir)> {
        resolution
            .pentagons()
            .map(|cell| (cell, self.centre(cell)))
            .collect()
    }

    /// How many pentagons at `resolution` touch ground where `blocked` is
    /// true: a pentagon counts if its centre or any neighbour's centre does,
    /// so pentagons are kept off coasts as well as inland.
    pub fn pentagons_on(&self, resolution: Resolution, blocked: impl Fn(Dir) -> bool) -> usize {
        resolution
            .pentagons()
            .filter(|&p| {
                std::iter::once(p)
                    .chain(neighbours(p))
                    .any(|cell| blocked(self.centre(cell)))
            })
            .count()
    }

    /// The frame, from a fixed deterministic set of `candidates` rotations
    /// (the identity first), with the fewest pentagons on `blocked` ground at
    /// `resolution`. Ties keep the earlier candidate, so the answer depends
    /// only on the inputs.
    pub fn search(
        resolution: Resolution,
        candidates: usize,
        blocked: impl Fn(Dir) -> bool,
    ) -> (Self, usize) {
        let mut best = (
            Self::identity(),
            Self::identity().pentagons_on(resolution, &blocked),
        );
        for k in 1..candidates {
            if best.1 == 0 {
                break;
            }
            let frame = candidate(k, candidates);
            let count = frame.pentagons_on(resolution, &blocked);
            if count < best.1 {
                best = (frame, count);
            }
        }
        best
    }
}

/// The `k`th of `n` candidate rotations: axes spread over the sphere on a
/// Fibonacci spiral, angles stepping by the golden angle.
fn candidate(k: usize, n: usize) -> Frame {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let y = 1.0 - 2.0 * (k as f64 + 0.5) / n as f64;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let theta = golden * k as f64;
    let angle = (golden * 0.5 * k as f64).rem_euclid(std::f64::consts::TAU);
    Frame::axis_angle([r * theta.cos(), y, r * theta.sin()], angle)
}

/// A cell's neighbours, sorted by index: six for a hexagon, five for a
/// pentagon.
pub fn neighbours(cell: CellIndex) -> Vec<CellIndex> {
    let mut ring: Vec<CellIndex> = cell.grid_disk::<Vec<_>>(1);
    ring.retain(|&c| c != cell);
    ring.sort_unstable();
    ring
}

/// Every cell within `k` steps of `cell` (itself included), sorted by index.
pub fn disk(cell: CellIndex, k: u32) -> Vec<CellIndex> {
    let mut cells: Vec<CellIndex> = cell.grid_disk::<Vec<_>>(k);
    cells.sort_unstable();
    cells
}

/// The cheapest route from `start` to `goal`, A* over cells.
///
/// `step_cost(from, to)` is the cost of moving between neighbours, or `None`
/// where the move is not allowed (water for a land unit, a cliff). `min_step`
/// must not exceed any allowed step's cost: the heuristic is a lower bound on
/// the steps left ([`steps_at_least`]) times `min_step`, which keeps it
/// admissible.
///
/// Returns the cells in order, `start` and `goal` included, and the total
/// cost.
pub fn path(
    start: CellIndex,
    goal: CellIndex,
    mut step_cost: impl FnMut(CellIndex, CellIndex) -> Option<u32>,
    min_step: u32,
) -> Option<(Vec<CellIndex>, u32)> {
    astar(
        &start,
        |&from| {
            neighbours(from)
                .into_iter()
                .filter_map(|to| step_cost(from, to).map(|cost| (to, cost)))
                .collect::<Vec<_>>()
        },
        |&cell| steps_at_least(cell, goal) * min_step,
        |&cell| cell == goal,
    )
}

/// The furthest apart two neighbouring cells' centres can be, as a multiple
/// of the resolution's average edge length. A regular hexagon's neighbours
/// are √3 edges apart; H3's cells vary in size across the sphere, so the
/// bound carries a factor of two over that, checked across every cell of a
/// resolution by `neighbours_are_never_further_than_the_hop_bound`.
const MAX_HOP_EDGES: f64 = 2.0 * 1.732_050_807_568_877_2;

/// A lower bound on the steps from `a` to `b`. H3's grid distance where it
/// can measure one; where it cannot — across a pentagon, or between cells
/// too far apart to unfold onto one plane — the great-circle angle between
/// the centres divided by the longest possible single step.
pub fn steps_at_least(a: CellIndex, b: CellIndex) -> u32 {
    match a.grid_distance(b) {
        Ok(d) => d.max(0) as u32,
        Err(_) => {
            let angle = LatLng::from(a).distance_rads(LatLng::from(b));
            let hop = MAX_HOP_EDGES * a.resolution().edge_length_rads();
            (angle / hop).floor() as u32
        }
    }
}

/// Flat hex coordinates around an anchor cell.
///
/// H3's local IJ coordinates are exact on a hex grid until they reach across
/// a pentagon, where the grid has a missing wedge: [`Patch::local`] and
/// [`Patch::cell`] return `None` there. Keep patches away from pentagons
/// (see [`Frame::search`]) and they behave like a plain axial hex map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Patch {
    pub anchor: CellIndex,
}

impl Patch {
    pub const fn new(anchor: CellIndex) -> Self {
        Self { anchor }
    }

    /// `(i, j)` of a cell, relative to the patch's anchoring frame.
    pub fn local(&self, cell: CellIndex) -> Option<(i32, i32)> {
        cell.to_local_ij(self.anchor)
            .ok()
            .map(|ij| (ij.coord.i, ij.coord.j))
    }

    /// The cell at `(i, j)`.
    pub fn cell(&self, i: i32, j: i32) -> Option<CellIndex> {
        CellIndex::try_from(LocalIJ::new(self.anchor, CoordIJ::new(i, j))).ok()
    }
}

/// Points spread over a cell, as world directions, for summarising a world
/// layer into one value per cell: the centre, then for each corner `rings`
/// points stepping from the centre towards it (stopping short of the corner,
/// which belongs to three cells).
pub fn samples(frame: &Frame, cell: CellIndex, rings: usize) -> Vec<Dir> {
    let centre = frame.centre(cell);
    let mut points = vec![centre];
    for corner in frame.boundary(cell) {
        for r in 1..=rings {
            let t = r as f32 / (rings as f32 + 1.0);
            let p = [
                centre[0] + (corner[0] - centre[0]) * t,
                centre[1] + (corner[1] - centre[1]) * t,
                centre[2] + (corner[2] - centre[2]) * t,
            ];
            let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            points.push([p[0] / len, p[1] / len, p[2] / len]);
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    const RES: Resolution = Resolution::Three;

    fn dot(a: Dir, b: Dir) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn lon_lat(lon: f32, lat: f32) -> Dir {
        let (lon, lat) = (lon.to_radians(), lat.to_radians());
        [lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()]
    }

    #[test]
    fn identity_frame_uses_world_longitude_and_latitude() {
        let dir = lon_lat(-71.8, -18.9);
        let cell = Frame::identity().cell_at(dir, RES);
        let ll = LatLng::new(-18.9, -71.8).unwrap();
        assert_eq!(cell, ll.to_cell(RES));
    }

    #[test]
    fn a_cell_contains_its_own_centre_in_any_frame() {
        let frames = [
            Frame::identity(),
            Frame::axis_angle([0.3, 1.0, -0.2], 1.1),
            candidate(17, 64),
        ];
        for frame in frames {
            for dir in [
                lon_lat(0.0, 0.0),
                lon_lat(123.0, 45.0),
                lon_lat(-60.0, -80.0),
            ] {
                let cell = frame.cell_at(dir, RES);
                let centre = frame.centre(cell);
                assert_eq!(frame.cell_at(centre, RES), cell);
                assert!(dot(centre, dir) > 0.99, "centre is near the direction");
            }
        }
    }

    #[test]
    fn hexagons_have_six_neighbours_and_pentagons_five() {
        let hex = Frame::identity().cell_at(lon_lat(10.0, 20.0), RES);
        assert!(!hex.is_pentagon());
        assert_eq!(neighbours(hex).len(), 6);
        for pentagon in RES.pentagons() {
            assert_eq!(neighbours(pentagon).len(), 5);
        }
        // Neighbourhood is symmetric.
        for n in neighbours(hex) {
            assert!(neighbours(n).contains(&hex));
        }
    }

    #[test]
    fn rings_are_sorted_so_every_build_agrees() {
        for cell in RES.pentagons().take(3) {
            let ring = disk(cell, 3);
            assert!(ring.windows(2).all(|w| w[0] < w[1]));
        }
    }

    #[test]
    fn uniform_paths_take_the_grid_distance() {
        let frame = Frame::identity();
        let a = frame.cell_at(lon_lat(0.0, 10.0), RES);
        let b = frame.cell_at(lon_lat(8.0, 14.0), RES);
        let (cells, cost) = path(a, b, |_, _| Some(1), 1).expect("reachable");
        let steps = a.grid_distance(b).unwrap() as u32;
        assert_eq!(cost, steps);
        assert_eq!(cells.len() as u32, steps + 1);
        assert_eq!((cells[0], *cells.last().unwrap()), (a, b));
        for pair in cells.windows(2) {
            assert!(neighbours(pair[0]).contains(&pair[1]));
        }
    }

    #[test]
    fn paths_go_around_what_cannot_be_crossed() {
        let frame = Frame::identity();
        let a = frame.cell_at(lon_lat(0.0, 10.0), RES);
        let b = frame.cell_at(lon_lat(8.0, 10.0), RES);
        let direct = path(a, b, |_, _| Some(1), 1).unwrap();
        // Wall off the direct route's middle cells.
        let wall: Vec<CellIndex> = direct.0[1..direct.0.len() - 1].to_vec();
        let around =
            path(a, b, |_, to| (!wall.contains(&to)).then_some(1), 1).expect("a way round");
        assert!(around.1 > direct.1);
        assert!(around.0.iter().all(|c| !wall.contains(c)));
        // A cell no one may enter cannot be reached.
        assert!(path(a, b, |_, to| (to != b).then_some(1), 1).is_none());
    }

    #[test]
    fn neighbours_are_never_further_than_the_hop_bound() {
        // Every cell at resolution 2 and each of its neighbours: the bound the
        // fallback heuristic divides by must be at least the true longest hop.
        let res = Resolution::Two;
        let hop = MAX_HOP_EDGES * res.edge_length_rads();
        let mut longest: f64 = 0.0;
        for base in CellIndex::base_cells() {
            for cell in base.children(res) {
                let c = LatLng::from(cell);
                for n in neighbours(cell) {
                    longest = longest.max(c.distance_rads(LatLng::from(n)));
                }
            }
        }
        assert!(
            longest <= hop,
            "longest hop {longest} exceeds the bound {hop}"
        );
    }

    #[test]
    fn far_apart_cells_still_get_a_useful_heuristic() {
        let frame = Frame::identity();
        let a = frame.cell_at(lon_lat(-72.0, -19.0), RES);
        let b = frame.cell_at(lon_lat(108.0, 19.0), RES); // the antipode
        assert!(a.grid_distance(b).is_err(), "H3 cannot unfold this far");
        let bound = steps_at_least(a, b);
        assert!(bound > 10, "a real lower bound, not zero: {bound}");
        // And it is a lower bound: a uniform route takes at least that many
        // steps (its cost, at one per step).
        let (_, steps) = path(a, b, |_, _| Some(1), 1).expect("reachable");
        assert!(
            steps >= bound,
            "route of {steps} steps under the bound {bound}"
        );
    }

    #[test]
    fn patches_give_flat_coordinates_that_round_trip() {
        let anchor = Frame::identity().cell_at(lon_lat(30.0, 30.0), RES);
        let patch = Patch::new(anchor);
        for cell in disk(anchor, 4) {
            let (i, j) = patch
                .local(cell)
                .expect("near the anchor, away from pentagons");
            assert_eq!(patch.cell(i, j), Some(cell));
        }
    }

    #[test]
    fn samples_stay_inside_their_cell() {
        let frame = Frame::axis_angle([1.0, 0.2, 0.0], 0.4);
        let cell = frame.cell_at(lon_lat(-40.0, 5.0), RES);
        let points = samples(&frame, cell, 2);
        assert_eq!(points.len(), 1 + 6 * 2);
        for p in points {
            assert_eq!(frame.cell_at(p, RES), cell);
        }
    }

    #[test]
    fn search_moves_pentagons_off_land_deterministically() {
        // A continent: a 30° cap centred on one of the identity frame's
        // pentagons. (No rotation can clear a cap wider than ~37°: the
        // pentagons sit at an icosahedron's corners, and that is the radius
        // of the largest cap between them.)
        let centre = Frame::identity().pentagons(RES)[0].1;
        let land = move |d: Dir| dot(d, centre) > 30f32.to_radians().cos();
        let start = Frame::identity().pentagons_on(RES, land);
        let (frame, count) = Frame::search(RES, 64, land);
        assert!(start > 0, "the identity frame has pentagons on this land");
        assert_eq!(count, 0, "a rotation clears them");
        assert_eq!(frame.pentagons_on(RES, land), 0);
        assert_eq!(Frame::search(RES, 64, land), (frame, count));
        // The recorded matrix rebuilds the same frame.
        assert_eq!(Frame::from_matrix(frame.matrix()), frame);
    }
}
