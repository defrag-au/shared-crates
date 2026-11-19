//! The simulation: plates drifting in steps, crust carried with them, and the
//! crust resampled onto a fixed grid every few steps.
//!
//! **Representation.** The crust is a fixed set of evenly spread sample
//! points (the reference grid), each owned by a plate and holding a
//! [`Crust`]. Between resamples a plate's samples move with it rigidly: a
//! sample's position is its plate's accumulated rotation applied to its
//! reference point. Every [`Params::resample_every`] steps the crust is
//! resampled back onto the grid: each grid point takes the crust of the
//! moved sample that now covers it.
//!
//! Resampling rarely, not every step, keeps coastlines from jittering: a
//! moved plate keeps its exact shape between resamples, and snapping to the
//! grid adds at most half a spacing of error each time. The paper this
//! follows (Cortial et al. 2019) resamples every 10–60 steps.
//!
//! **Subduction, every step.** Where two plates' moved samples overlap, one
//! plate dives under the other, decided once per pair of plates and kept
//! (real subduction zones persist): oceanic crust dives under continental,
//! and of two oceanic plates the one with the older crust at the meeting
//! dives. The diving plate's oceanic samples under the other plate are
//! consumed then and there, so the trench advances step by step instead of
//! the plates piling up hundreds of kilometres deep between resamples (which
//! shredded the overlap into confetti when each grid point chose alone).
//! Continental crust does not dive: where continents meet they overlap until
//! the next resample sutures them (collision, below).
//!
//! Subduction builds as it consumes (Cortial et al. §4.1):
//! - **Uplift** of the overriding plate within [`SUBDUCTION_REACH`] of the
//!   trench, at `u0 · f(distance) · g(speed) · h(diving crust)`: peaking a
//!   few hundred km inland (where volcanic arcs stand), faster for faster
//!   convergence, more for higher crust going down. Sea floor raised above
//!   the sea becomes the land of an island arc.
//! - **A trench** where the diving plate goes under.
//! - **Slab pull**: the diving plate's spin axis turns toward its trench, so
//!   subduction drags plates along.
//!
//! **What a resample finds.**
//! - A grid point covered by one plate takes that plate's crust.
//! - A grid point covered by none lies in a gap that opened as plates moved
//!   apart: it becomes new sea floor (see [`Sim::resample`]).
//! - Grid points covered by two plates' continents are a collision. Past
//!   [`COLLISION_AREA`] of overlap, the smaller continent's colliding
//!   landmass (a terrane) detaches from its plate and sutures to the other,
//!   and the collision raises mountains around it (Cortial et al. §4.2).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use glam::{Quat, Vec3};

use crate::crust::{self, Crust, Kind, Orogeny};
use crate::impacts::{self, Bombardment, Impact};
use crate::noise::{Rng, fbm, hash};
use crate::sphere::{self, Index};

/// How a planet's tectonics run.
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub seed: u64,
    /// Crust sample points over the whole sphere.
    pub samples: usize,
    /// Plates at the start.
    pub plates: usize,
    /// Share of the surface that starts as the supercontinent.
    pub continent_fraction: f32,
    /// Fastest plate speed, km per million years (= mm per year).
    pub max_speed: f32,
    /// Planet radius, km: turns plate speeds into rotation rates.
    pub radius_km: f32,
    /// Million years per step.
    pub step: f32,
    /// Steps between resamples onto the grid.
    pub resample_every: u32,
    /// Expected rifts per resample for a typical-sized, wholly continental
    /// plate (Cortial et al.'s `λ₀`); 0 turns rifting off.
    pub rift_rate: f32,
    /// Mantle hotspots under a planet made by [`Sim::new`].
    pub hotspots: usize,
    /// What the crust is at the start (see [`Start`]).
    pub start: Start,
    /// The mantle's heat at the start, as a multiple of Earth's today: 1 is
    /// a planet as cool as Earth now, and stays so; above 1 the planet cools
    /// toward 1 over [`Params::heat_decay`]. See [`Sim::heat`].
    pub heat: f32,
    /// e-folding time of the excess heat, My.
    pub heat_decay: f32,
    /// Impacts through the planet's life ([`crate::impacts`]).
    pub bombardment: Bombardment,
}

/// The crust a planet made by [`Sim::new`] starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// One supercontinent, a ~30% cap, on an ocean of mixed age: a planet
    /// in mid-life, like Earth when Pangaea began to break up.
    Supercontinent,
    /// Newly crusted over: young basaltic crust everywhere, many small
    /// plates, no continents. Continents grow from what the tectonics raises
    /// (island arcs, collisions of arcs), as Earth's did.
    Primordial,
}

impl Params {
    /// An Earth-sized planet at about 70 km resolution, ten plates and one
    /// supercontinent covering ~30% of it, stepped every 2 My and resampled
    /// every 20 My. Plate speeds up to 100 mm/y (Cortial et al.'s maximum).
    /// A typical plate that is all continent rifts every ~60 My on average,
    /// a half-continental one every ~120 My: enough to keep breaking
    /// continents apart as collisions weld them (Earth's supercontinents have
    /// assembled and broken up over a few hundred million years; at 0.15
    /// every rift in a 240 My run fell in its first 40). A dozen hotspots.
    pub fn earth_like(seed: u64) -> Self {
        Self {
            seed,
            samples: 100_000,
            plates: 10,
            continent_fraction: 0.3,
            max_speed: 100.0,
            radius_km: 6370.0,
            step: 2.0,
            resample_every: 10,
            rift_rate: 0.35,
            hotspots: 12,
            start: Start::Supercontinent,
            heat: 1.0,
            heat_decay: 1500.0,
            bombardment: Bombardment::NONE,
        }
    }

    /// A young, hot planet just crusted over: no continents, 24 small plates,
    /// a mantle 3.5× as hot as Earth's today cooling toward it with an
    /// e-folding time of 1.5 Gyr (≈1.1× by 4.5 Gyr). Earth's radiogenic heat
    /// production was ~3–4× today's when it formed, and its Archean mantle
    /// perhaps 150–250 °C hotter. Twenty hotspots. Meant to be run for
    /// billions of years.
    ///
    /// Impacts come through its whole life: ~2 a My at first (resolvable
    /// craters, ~100 km across and up), falling with an e-folding time of
    /// 150 My, then one every ~50 My for ever. Earth's early record is gone,
    /// but the Moon's shows the same shape: a heavy bombardment in the first
    /// few hundred My, then a steady trickle.
    pub fn primordial(seed: u64) -> Self {
        Self {
            plates: 24,
            hotspots: 20,
            start: Start::Primordial,
            heat: 3.5,
            bombardment: Bombardment {
                early: 2.0,
                decay: 150.0,
                background: 0.02,
            },
            ..Self::earth_like(seed)
        }
    }
}

/// A plate's rigid motion: rotation about `axis` (unit, through the centre)
/// at `rate` radians per million years.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub axis: Vec3,
    pub rate: f32,
}

/// A live crater, held to the plate it rides.
struct Anchor {
    id: u32,
    plate: u16,
    /// Its centre in the plate's frame at the last resample (the grid's
    /// frame then): the plate's rotation since carries it to where it is.
    at: Vec3,
    radius: f32,
}

struct Plate {
    motion: Motion,
    /// Rotation since the last resample.
    rotation: Quat,
}

/// What a resample found.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Share of grid points with continental crust.
    pub continental: f32,
    /// Grid points in gaps, filled with new sea floor.
    pub opened: usize,
    /// Grid points where plates still overlapped at the resample.
    pub overlapped: usize,
    /// Samples consumed by subduction since the last resample.
    pub subducted: usize,
    /// Continental collisions sutured at this resample.
    pub collisions: usize,
    /// Plates that rifted apart at this resample.
    pub rifts: usize,
    /// Uplift of continental crust since the last resample, summed over
    /// samples, km: by subduction (cordilleras) and by collision. A measure
    /// of what is holding continents up against erosion.
    pub raised_by_subduction: f32,
    pub raised_by_collision: f32,
}

/// The crust on the grid at a moment, with the motion each plate is making.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Million years since the start.
    pub time: f32,
    /// The plate owning each grid point.
    pub owner: Vec<u16>,
    pub crust: Vec<Crust>,
    /// The surface's elevation against the sea, km: the crust's own, with
    /// continental shelves where sea floor meets a continent on its own plate
    /// (see [`Sim::frame`]), and the continents lowered by a hot planet's
    /// flooding ([`Sim::flood`]).
    pub surface: Vec<f32>,
    pub plates: Vec<Motion>,
    /// The mantle's heat, a multiple of Earth's today ([`Sim::heat`]).
    pub heat: f32,
    /// Impacts since the last frame, in the order they landed.
    pub impacts: Vec<Impact>,
    pub stats: Stats,
}

pub struct Sim {
    params: Params,
    points: Vec<Vec3>,
    spacing: f32,
    neighbours: Vec<Vec<u32>>,
    /// The grid's spatial index.
    grid: Index,
    owner: Vec<u16>,
    crust: Vec<Crust>,
    plates: Vec<Plate>,
    /// False for a sample consumed by subduction since the last resample.
    alive: Vec<bool>,
    /// Which plate of each meeting pair (lower id, higher id) dives.
    diver: BTreeMap<(u16, u16), u16>,
    subducted: usize,
    /// Continental uplift by subduction since the last resample, km summed.
    raised_by_subduction: f32,
    /// Plumes fixed in the mantle: the grid does not move, so neither do
    /// they, while plates slide over them.
    hotspots: Vec<Vec3>,
    /// Draws for rifting, seeded from the planet's seed.
    rng: Rng,
    /// Draws for impacts: a stream of their own, so turning them on or off
    /// leaves every other draw as it was.
    impact_rng: Rng,
    /// Impacts since the last frame.
    impacts: Vec<Impact>,
    /// Every impact so far, in order (their ids are 1, 2, …).
    history: Vec<Impact>,
    /// The craters still in the crust, each anchored to its plate.
    live: Vec<Anchor>,
    impacts_made: u32,
    time: f32,
    since_resample: u32,
    stats: Stats,
}

impl Sim {
    /// A planet whose crust begins as one supercontinent on an ocean of
    /// varied age, split among `params.plates` plates drifting in random
    /// directions: the supercontinent spans several plates, so their motion
    /// rifts it apart.
    pub fn new(params: Params) -> Self {
        let mut rng = Rng::new(params.seed);
        let seed = (params.seed ^ (params.seed >> 32)) as u32;
        let centre = rng.direction();
        // The cap that covers the share asked for: area (1 − cos θ) / 2.
        let cap = (1.0 - 2.0 * params.continent_fraction)
            .clamp(-1.0, 1.0)
            .acos();
        let centroids: Vec<Vec3> = (0..params.plates).map(|_| rng.direction()).collect();
        let motions: Vec<Motion> = (0..params.plates)
            .map(|_| Motion {
                axis: rng.direction(),
                rate: rng.range(0.3, 1.0) * params.max_speed / params.radius_km,
            })
            .collect();
        let hotspots: Vec<Vec3> = (0..params.hotspots).map(|_| rng.direction()).collect();
        let start = params.start;
        let sim = Self::with(params, motions, |p| {
            // Plates: Voronoi cells of the centroids, their borders warped.
            let plate = centroids
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    let warp = 0.25 * fbm(p, 2.0, 3, seed.wrapping_add(1000 + k as u32));
                    (k, p.dot(*c).clamp(-1.0, 1.0).acos() + warp)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map_or(0, |(k, _)| k as u16);
            if start == Start::Primordial {
                // Basalt crusted over a magma ocean: young everywhere, 0–30 My.
                return (
                    plate,
                    Crust::oceanic(15.0 + 15.0 * fbm(p, 2.5, 3, seed ^ 0x0CEA)),
                );
            }
            // The supercontinent: a cap with a coastline warped by noise.
            let reach = p.dot(centre).clamp(-1.0, 1.0).acos() + 0.35 * fbm(p, 1.5, 4, seed);
            let crust = if reach < cap {
                // Old, stable continental interior.
                Crust::continental((0.4 + 0.5 * fbm(p, 4.0, 4, seed ^ 0xC0A5)).max(0.05), 500.0)
            } else {
                // Sea floor of mixed age, 25–145 My.
                Crust::oceanic(85.0 + 60.0 * fbm(p, 2.5, 3, seed ^ 0x0CEA))
            };
            (plate, crust)
        });
        sim.with_hotspots(hotspots)
    }

    /// Put plumes under the planet at these (unit) directions, fixed in the
    /// mantle.
    pub fn with_hotspots(mut self, hotspots: Vec<Vec3>) -> Self {
        self.hotspots = hotspots;
        self
    }

    /// A planet of `motions.len()` plates whose crust at each grid point
    /// `init` gives as (owning plate, crust).
    pub fn with(params: Params, motions: Vec<Motion>, init: impl Fn(Vec3) -> (u16, Crust)) -> Self {
        let points = sphere::fibonacci(params.samples);
        let spacing = sphere::spacing(params.samples);
        let index = Index::new(&points, spacing * 2.0);
        let neighbours = sphere::neighbours(&points, &index, 8, spacing * 2.5);
        let (owner, crust): (Vec<u16>, Vec<Crust>) = points.iter().map(|&p| init(p)).unzip();
        let plates = motions
            .into_iter()
            .map(|motion| Plate {
                motion,
                rotation: Quat::IDENTITY,
            })
            .collect();
        let alive = vec![true; points.len()];
        let rng = Rng::new(params.seed ^ 0x5249_4654);
        let impact_rng = Rng::new(params.seed ^ 0x494D_5041);
        let mut sim = Self {
            params,
            points,
            spacing,
            neighbours,
            grid: index,
            owner,
            crust,
            plates,
            alive,
            diver: BTreeMap::new(),
            subducted: 0,
            raised_by_subduction: 0.0,
            hotspots: Vec::new(),
            rng,
            impact_rng,
            impacts: Vec::new(),
            history: Vec::new(),
            live: Vec::new(),
            impacts_made: 0,
            time: 0.0,
            since_resample: 0,
            stats: Stats::default(),
        };
        sim.stats.continental = sim.continental_share();
        sim
    }

    /// The mantle's heat now, a multiple of Earth's today:
    /// `1 + (H₀ − 1)·e^(−t/τ)`, for `H₀` = [`Params::heat`] and
    /// `τ` = [`Params::heat_decay`]. A hotter mantle drives faster plates
    /// ([`Sim::speed_scale`]), floods the continents ([`Sim::flood`]),
    /// breaks plates up more ([`HOT_RIFT`]) and feeds stronger plumes.
    pub fn heat(&self) -> f32 {
        1.0 + (self.params.heat - 1.0) * (-self.time / self.params.heat_decay).exp()
    }

    /// The height land stops rising toward, km: [`MOUNTAIN_LIMIT`] over the
    /// heat. Hot crust is weak, and high ranges on it spread and collapse
    /// under their own weight, as Archean terrains are thought to have had
    /// low relief: ~1.7 km at 3.5× today's heat, 6 km at today's. Without
    /// it, a hot planet's fast plates and many trenches kept most of its land
    /// near 2 km while a cool one's settled under 1.
    pub fn mountain_limit(&self) -> f32 {
        MOUNTAIN_LIMIT / self.heat()
    }

    /// How far a hot mantle has drowned the continents, km.
    ///
    /// A hotter mantle holds the ocean floor higher, and the oceans rise with
    /// it: depths below the sea surface stay much as they are, while the
    /// continents stand that much lower against the sea (their "freeboard"
    /// shrinks). It is why the early Earth is thought to have had little dry
    /// land. Elevations in the crust stay measured against a sea as cool as
    /// today's; [`Frame::surface`] takes the flooding off the land.
    pub fn flood(&self) -> f32 {
        HOT_FLOOD * (self.heat() - 1.0)
    }

    /// How much faster than today's the plates move, `H^0.75`: ~2.6× at 3.5
    /// times today's heat. Estimates of Archean plate speeds vary; faster,
    /// but not by the ~12× that boundary-layer scaling (speed ∝ heat flow²)
    /// would give, which nothing in the geological record suggests.
    fn speed_scale(&self) -> f32 {
        self.heat().powf(0.75)
    }

    /// The grid's sample points.
    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    /// The crust on the grid. Exact right after a resample (and at the
    /// start); between resamples the plates have moved on from it.
    ///
    /// Its `surface` adds continental shelves: sea floor beside a continent
    /// on the same plate (a passive margin, where ocean and continent are
    /// welded together) shoals to [`SHELF_DEPTH`] within [`SHELF_WIDTH`]
    /// spacings of the coast, then falls to its own depth by
    /// [`SHELF_REACH`]. Where the sea floor is another plate's (an active
    /// margin) the trench comes right up to the coast, as it does on Earth.
    /// Computed fresh for each frame, so shelves never pile up.
    pub fn frame(&self) -> Frame {
        Frame {
            time: self.time,
            owner: self.owner.clone(),
            crust: self.crust.clone(),
            surface: self.surface(),
            plates: self.plates.iter().map(|p| p.motion).collect(),
            heat: self.heat(),
            impacts: self.impacts.clone(),
            stats: self.stats,
        }
    }

    fn surface(&self) -> Vec<f32> {
        let n = self.points.len();
        let mut surface: Vec<f32> = self.crust.iter().map(|c| c.elevation).collect();
        let reach = SHELF_REACH * self.spacing;
        let mut dist = vec![f32::INFINITY; n];
        let mut heap = BinaryHeap::new();
        for (j, c) in self.crust.iter().enumerate() {
            if c.kind == Kind::Continental {
                heap.push(Reverse((0u32, j as u32)));
            }
        }
        while let Some(Reverse((bits, j))) = heap.pop() {
            let (j, d) = (j as usize, f32::from_bits(bits));
            if d >= dist[j] {
                continue;
            }
            dist[j] = d;
            for &k in &self.neighbours[j] {
                let k = k as usize;
                let c = &self.crust[k];
                // Out over this plate's own sea floor, not into a trench.
                if c.kind == Kind::Oceanic
                    && self.owner[k] == self.owner[j]
                    && c.elevation > TRENCH + 1.0
                {
                    let nd = d + self.points[j].distance(self.points[k]);
                    if nd < reach && nd < dist[k] {
                        heap.push(Reverse((nd.to_bits(), k as u32)));
                    }
                }
            }
        }
        // A hot planet's continents, and the shelves fringing them, stand
        // lower against its risen sea.
        let flood = self.flood();
        let shelf_depth = SHELF_DEPTH - flood;
        for j in 0..n {
            match self.crust[j].kind {
                Kind::Continental => surface[j] -= flood,
                Kind::Oceanic if dist[j].is_finite() => {
                    let fall = smoothstep(
                        SHELF_WIDTH * self.spacing,
                        SHELF_REACH * self.spacing,
                        dist[j],
                    );
                    let shelf = shelf_depth + (surface[j] - shelf_depth) * fall;
                    surface[j] = surface[j].max(shelf);
                }
                Kind::Oceanic => {}
            }
        }
        surface
    }

    /// Advance one step: plates turn, crust ages. Returns the new frame when
    /// the step ends with a resample.
    pub fn step(&mut self) -> Option<Frame> {
        let dt = self.params.step;
        let speed = self.speed_scale();
        for plate in &mut self.plates {
            let turn = Quat::from_axis_angle(plate.motion.axis, plate.motion.rate * speed * dt);
            plate.rotation = (turn * plate.rotation).normalize();
        }
        for c in &mut self.crust {
            crust::age(c, dt);
        }
        self.subduct();
        self.strike(dt);
        self.time += dt;
        self.since_resample += 1;
        if self.since_resample >= self.params.resample_every {
            self.resample();
            let frame = self.frame();
            self.impacts.clear();
            Some(frame)
        } else {
            None
        }
    }

    /// This step's impacts: each cuts its crater into the crust under it,
    /// wherever the plates have carried that crust to. Sea floor takes the
    /// crater in its tectonic relief, so it keeps it until it subducts; land
    /// takes it in its height, and erosion wears it down.
    fn strike(&mut self, dt: f32) {
        let r_km = self.params.radius_km;
        let strikes = impacts::draw(
            &mut self.impact_rng,
            &self.params.bombardment,
            self.time,
            dt,
            IMPACT_MIN * self.spacing,
            r_km,
        );
        for (time, dir, radius, depth) in strikes {
            self.cut(time, dir, radius, depth);
        }
    }

    /// Cut one crater, `radius` radians and `depth` km deep, centred at
    /// `dir` in the sphere's fixed frame, into the crust there now.
    fn cut(&mut self, time: f32, dir: Vec3, radius: f32, depth: f32) {
        let flood = self.flood();
        let reach = (radius * 2.2).cos();
        let mut nearest = (f32::MIN, Kind::Oceanic, 0u16);
        for i in 0..self.points.len() {
            if !self.alive[i] {
                continue;
            }
            let p = self.plates[self.owner[i] as usize].rotation * self.points[i];
            let cos = p.dot(dir);
            if cos < reach {
                continue;
            }
            let t = cos.clamp(-1.0, 1.0).acos() / radius;
            let c = &mut self.crust[i];
            if cos > nearest.0 {
                nearest = (cos, c.kind, self.owner[i]);
            }
            crust::scar(c, impacts::profile(t) * depth, flood);
            if t < SCAR_CORE {
                c.scar = self.impacts_made + 1;
            }
        }
        self.impacts_made += 1;
        if nearest.0 > f32::MIN {
            // Anchored to the plate under its centre, in that plate's frame
            // at the last resample: the plate's rotation since carries it.
            let plate = nearest.2;
            let since = self.plates[plate as usize].rotation;
            self.live.push(Anchor {
                id: self.impacts_made,
                plate,
                at: since.inverse() * dir,
                radius,
            });
        }
        let impact = Impact {
            id: self.impacts_made,
            time,
            dir,
            radius,
            depth,
            ocean: nearest.1 == Kind::Oceanic,
        };
        self.impacts.push(impact);
        self.history.push(impact);
    }

    /// Every impact so far, survived or not, in the order they landed.
    pub fn history(&self) -> &[Impact] {
        &self.history
    }

    /// Carry each live crater's anchor through a resample, after the new
    /// crust is on the grid: its plate's rotation since the last resample
    /// moves it, and the crust now under it tells whether it survived. If
    /// that crust no longer carries the crater's [`Crust::scar`], it was
    /// consumed at a trench or overridden by another plate, and the crater is
    /// gone. A survivor re-tags its whole bowl, so the resample's
    /// nearest-sample copying (which drops and doubles single samples at
    /// random) cannot wear the record away. `rotations` are the plates'
    /// rotations since the last resample.
    fn carry_craters(&mut self, rotations: &[Quat]) {
        let mut live = std::mem::take(&mut self.live);
        live.retain_mut(|anchor| {
            let now = (rotations[anchor.plate as usize] * anchor.at).normalize();
            let Some(centre) = self.grid.nearest(&self.points, now, self.spacing * 2.0) else {
                return false;
            };
            let core = (anchor.radius * SCAR_CORE).max(self.spacing);
            let id = anchor.id;
            // Alive while any crust in its heart still carries it: a single
            // grid point's tag can be lost to resample noise.
            let mut held = Vec::new();
            let mut heart = Vec::new();
            self.grid.near(now, core, |i| {
                if self.points[i].distance(now) <= core {
                    heart.push(i);
                    if self.crust[i].scar == id {
                        held.push(i);
                    }
                }
            });
            if held.is_empty() {
                return false;
            }
            // Its plate is the one holding most of what carries it.
            let plate = {
                let mut counts: BTreeMap<u16, usize> = BTreeMap::new();
                for &i in &held {
                    *counts.entry(self.owner[i]).or_default() += 1;
                }
                counts
                    .into_iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
                    .map_or(self.owner[centre], |(p, _)| p)
            };
            anchor.at = now;
            anchor.plate = plate;
            let tags: Vec<usize> = heart
                .into_iter()
                .filter(|&i| self.owner[i] == plate)
                .collect();
            for i in tags {
                // A later crater keeps its claim on crust it overlaps.
                if self.crust[i].scar < id {
                    self.crust[i].scar = id;
                }
            }
            true
        });
        self.live = live;
    }

    /// Re-read each live anchor's plate after plates are renumbered or split.
    fn rehome_craters(&mut self) {
        for anchor in &mut self.live {
            if let Some(centre) = self
                .grid
                .nearest(&self.points, anchor.at, self.spacing * 2.0)
            {
                anchor.plate = self.owner[centre];
            }
        }
    }

    /// The craters still in the crust: each impact that has not been carried
    /// down a trench or overridden, where its plate has taken it, and how
    /// deep its bowl still stands below its rim. Read right after a resample
    /// (the end of [`Sim::run`]), when the grid holds the crust exactly.
    pub fn craters(&self) -> Vec<impacts::Crater> {
        let mut hearts: Vec<Vec<usize>> = vec![Vec::new(); self.history.len()];
        for (i, c) in self.crust.iter().enumerate() {
            if c.scar > 0 {
                hearts[c.scar as usize - 1].push(i);
            }
        }
        let mut out = Vec::new();
        for anchor in &self.live {
            let impact = &self.history[anchor.id as usize - 1];
            let heart = &hearts[anchor.id as usize - 1];
            if heart.is_empty() {
                continue;
            }
            let dir = anchor.at;
            // Floor: the mean of its bowl's inner half (the whole heart, if
            // that holds no grid point; walls would make it read shallow).
            // Rim: the crust a radius out.
            let inner = (impact.radius * 0.5).max(self.spacing * 0.75).cos();
            let middle: Vec<f32> = heart
                .iter()
                .filter(|&&i| self.points[i].dot(dir) >= inner)
                .map(|&i| self.crust[i].elevation)
                .collect();
            let floor = if middle.is_empty() {
                heart
                    .iter()
                    .map(|&i| self.crust[i].elevation)
                    .fold(f32::MAX, f32::min)
            } else {
                middle.iter().sum::<f32>() / middle.len() as f32
            };
            let (lo, hi) = ((impact.radius * 1.25).cos(), (impact.radius * 0.85).cos());
            let rim: Vec<f32> = self
                .points
                .iter()
                .zip(&self.crust)
                .filter(|(p, _)| (lo..=hi).contains(&p.dot(dir)))
                .map(|(_, c)| c.elevation)
                .collect();
            let rim = rim.iter().sum::<f32>() / rim.len().max(1) as f32;
            let ocean = heart
                .iter()
                .filter(|&&i| self.crust[i].kind == Kind::Oceanic)
                .count()
                * 2
                > heart.len();
            // On land the crater's relief is its own field: the deepest of
            // what it keeps is its depth, whatever terrain it sits in (a
            // basin 1000 km across has coast and sea in its rim ring). Sea
            // floor holds it mixed into its tectonic relief, so there the
            // rim-to-floor measure stands, never deeper than it was made.
            let kept = heart
                .iter()
                .map(|&i| self.crust[i].crater)
                .fold(0.0f32, f32::min);
            let depth = if ocean {
                (rim - floor).clamp(0.0, impact.depth)
            } else {
                (-kept).clamp(0.0, impact.depth)
            };
            out.push(impacts::Crater {
                impact: *impact,
                dir,
                depth,
                ocean,
            });
        }
        out
    }

    /// Run until `until` million years, handing each resampled frame (and
    /// the starting one) to `on_frame`.
    pub fn run(&mut self, until: f32, mut on_frame: impl FnMut(&Frame)) {
        on_frame(&self.frame());
        while self.time + self.params.step * 0.5 < until {
            if let Some(frame) = self.step() {
                on_frame(&frame);
            }
        }
    }

    /// A plate's surface velocity at `p`, km per million years.
    fn velocity(&self, plate: u16, p: Vec3) -> Vec3 {
        let m = self.plates[plate as usize].motion;
        m.axis.cross(p) * m.rate * self.speed_scale() * self.params.radius_km
    }

    /// Where plates' moved samples now overlap, consume the diving plate's
    /// oceanic crust under the other, raise the overriding plate, deepen the
    /// trench, and let the slab pull its plate.
    fn subduct(&mut self) {
        let n = self.points.len();
        let dt = self.params.step;
        let live: Vec<usize> = (0..n).filter(|&i| self.alive[i]).collect();
        let moved: Vec<Vec3> = live
            .iter()
            .map(|&i| self.plates[self.owner[i] as usize].rotation * self.points[i])
            .collect();
        let index = Index::new(&moved, self.spacing * 2.0);
        let cover = COVER * self.spacing;
        let reach = TRENCH_REACH * self.spacing;

        // Every pair of samples on different plates within `reach`, as
        // indices into `live`, in a fixed order; those within `cover`
        // overlap, the rest are only close (the trench's edge).
        let mut close: Vec<(usize, usize, bool)> = Vec::new();
        for (a, &pa) in moved.iter().enumerate() {
            let plate_a = self.owner[live[a]];
            index.near(pa, reach, |b| {
                if b > a && self.owner[live[b]] != plate_a {
                    let d = moved[b].distance_squared(pa);
                    if d <= reach * reach {
                        close.push((a, b, d <= cover * cover));
                    }
                }
            });
        }
        close.sort_unstable();
        let overlaps: Vec<(usize, usize)> =
            close.iter().filter(|c| c.2).map(|c| (c.0, c.1)).collect();

        // Decide which plate dives for each pair meeting for the first time:
        // a vote over this step's overlapping samples.
        let mut votes: BTreeMap<(u16, u16), (i64, i64)> = BTreeMap::new();
        for &(a, b) in &overlaps {
            let (ia, ib) = (live[a], live[b]);
            let key = pair(self.owner[ia], self.owner[ib]);
            if self.diver.contains_key(&key) {
                continue;
            }
            let (ca, cb) = (&self.crust[ia], &self.crust[ib]);
            // +1 for the lower-id plate diving here, −1 for the higher.
            let lower_dives = match dives(ca, cb) {
                Some(true) => self.owner[ia] == key.0,
                Some(false) => self.owner[ib] == key.0,
                None => continue,
            };
            let v = votes.entry(key).or_default();
            if lower_dives {
                v.0 += 1;
            } else {
                v.1 += 1;
            }
        }
        for (key, (lower, higher)) in votes {
            // Ties go to the higher id, so the choice is always made.
            self.diver
                .insert(key, if lower > higher { key.0 } else { key.1 });
        }

        // Consume the diving plate's oceanic samples where they overlap. Each
        // consumed sample makes the overriding sample above it a source of
        // uplift, and marks the diving plate's front for slab pull.
        // Source: (overriding sample, height factor, closing speed, fold push).
        let mut sources: BTreeMap<usize, (f32, f32, Vec3)> = BTreeMap::new();
        let mut fronts: BTreeMap<u16, Vec<Vec3>> = BTreeMap::new();
        let recycle = (HOT_RECYCLE * (self.heat() - 1.0)).clamp(0.0, 0.9);
        // One roll per front per resample interval: a front recycles or
        // collides as a whole. Rolling per sample left neighbours choosing
        // opposite fates, and shredded the coasts into confetti.
        let interval = (self.time / (dt * self.params.resample_every as f32)).floor() as i32;
        let seed = (self.params.seed ^ (self.params.seed >> 32)) as u32;
        for &(a, b) in &overlaps {
            let (ia, ib) = (live[a], live[b]);
            let Some(&diver) = self.diver.get(&pair(self.owner[ia], self.owner[ib])) else {
                continue;
            };
            let (under, over, at) = if self.owner[ia] == diver {
                (ia, ib, moved[b])
            } else {
                (ib, ia, moved[a])
            };
            if !self.alive[under] {
                continue;
            }
            if self.crust[under].kind == Kind::Continental {
                // Forced subduction (Cortial et al. §4.1): on a hot planet,
                // young continental crust is dragged down too, with a chance
                // rising with the heat; stabilised crust resists.
                let c = &self.crust[under];
                let young = c.age < CRATON_AGE;
                let (pa, pb) = pair(self.owner[ia], self.owner[ib]);
                let roll =
                    hash(pa as i32, pb as i32, interval, seed ^ 0x5EC) as f32 / u32::MAX as f32;
                if !(young && roll < recycle) {
                    continue;
                }
            }
            self.alive[under] = false;
            self.subducted += 1;
            // h: the paper's z̃², the diving crust's height between the
            // deepest trench and the highest peak, squared. Its own height,
            // from its age: not the trench this subduction cut into it,
            // which would let the trench weaken the uplift it comes with.
            let own = crust::ocean_elevation(self.crust[under].age);
            let z = (own - TRENCH_FLOOR) / (crust::PEAK - TRENCH_FLOOR);
            let push = self.velocity(diver, at) - self.velocity(self.owner[over], at);
            let source = (z * z, push.length(), push);
            let e = sources.entry(over).or_insert(source);
            if source.0 * source.1 > e.0 * e.1 {
                *e = source;
            }
            fronts.entry(diver).or_default().push(at);
        }

        // The trench: the diving plate's sea floor beside the overriding
        // plate sinks to trench depth before it goes under.
        for &(a, b, _) in &close {
            let (ia, ib) = (live[a], live[b]);
            let Some(&diver) = self.diver.get(&pair(self.owner[ia], self.owner[ib])) else {
                continue;
            };
            let under = if self.owner[ia] == diver { ia } else { ib };
            let c = &mut self.crust[under];
            if self.alive[under] && c.kind == Kind::Oceanic {
                let sink = (TRENCH - c.elevation).min(0.0);
                crust::raise(c, sink);
            }
        }

        self.raise_over_trenches(&sources, dt);
        self.pull_slabs(&live, &moved, &fronts, dt);
        self.erupt_hotspots(&live, &moved, &index, dt);
    }

    /// Hotspots: sea floor passing over a plume builds a volcano, highest
    /// over the plume's centre and capped at [`VOLCANO_CAP`] above the sea
    /// floor. As the plate carries it off it subsides ([`crust::age`]), so a
    /// plate drifting over a plume leaves a chain of islands sinking into
    /// seamounts, youngest over the plume.
    fn erupt_hotspots(&mut self, live: &[usize], moved: &[Vec3], index: &Index, dt: f32) {
        let plume = PLUME_RADIUS * self.spacing;
        let hotspots = self.hotspots.clone();
        // A hotter mantle feeds its plumes more melt.
        let heat = self.heat();
        for &h in &hotspots {
            index.near(h, plume, |k| {
                let d = moved[k].distance(h);
                if d <= plume {
                    let x = d / (plume * 0.5);
                    let dz = HOTSPOT_RATE * heat * (-x * x).exp() * dt;
                    crust::erupt(&mut self.crust[live[k]], dz, VOLCANO_CAP);
                }
            });
        }
    }

    /// Keep the plate list to the plates that exist. A plate whittled down to
    /// a sliver (under [`SLIVER`] of a typical plate) joins the neighbour it
    /// shares most border with; a plate with nothing left (consumed by
    /// subduction, or sutured whole into another) is retired, and the rest
    /// are renumbered in order. Without this, rifting adds plates for ever:
    /// a hot planet run for 4 Gyr went from 24 to ~1300.
    fn tidy_plates(&mut self) {
        let n = self.points.len();
        let typical = n as f32 / self.params.plates.max(1) as f32;
        let sliver = (typical * SLIVER).ceil() as usize;
        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); self.plates.len()];
        for j in 0..n {
            groups[self.owner[j] as usize].push(j);
        }
        for plate in 0..self.plates.len() {
            let size = groups[plate].len();
            if size == 0 || size >= sliver {
                continue;
            }
            let mut border: BTreeMap<u16, usize> = BTreeMap::new();
            for &j in &groups[plate] {
                for &k in &self.neighbours[j] {
                    let other = self.owner[k as usize];
                    if other as usize != plate {
                        *border.entry(other).or_default() += 1;
                    }
                }
            }
            // Most shared border; ties to the lower id.
            let Some(into) = border
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(&p, _)| p)
            else {
                continue;
            };
            let members = std::mem::take(&mut groups[plate]);
            for &j in &members {
                self.owner[j] = into;
            }
            groups[into as usize].extend(members);
        }

        let mut renumber = vec![u16::MAX; self.plates.len()];
        let mut kept = 0u16;
        for (plate, group) in groups.iter().enumerate() {
            if !group.is_empty() {
                renumber[plate] = kept;
                kept += 1;
            }
        }
        if kept as usize == self.plates.len() {
            return;
        }
        let mut plates = std::mem::take(&mut self.plates);
        let mut index = 0;
        plates.retain(|_| {
            let keep = renumber[index] != u16::MAX;
            index += 1;
            keep
        });
        self.plates = plates;
        for o in &mut self.owner {
            *o = renumber[*o as usize];
        }
        // Renumbering keeps the order, so each pair's lower id stays lower.
        self.diver = std::mem::take(&mut self.diver)
            .into_iter()
            .filter_map(|((a, b), d)| {
                let (a, b, d) = (
                    renumber[a as usize],
                    renumber[b as usize],
                    renumber[d as usize],
                );
                (a != u16::MAX && b != u16::MAX && d != u16::MAX).then_some(((a, b), d))
            })
            .collect();
    }

    /// Rifting (Cortial et al. §4.4): each plate breaks apart with chance
    /// `1 − e^−λ` per resample, `λ = λ₀ · (continental share) · A/A₀`, so
    /// large continental plates rift most and sea-floor plates seldom. It
    /// breaks into 2–4 fragments: Voronoi cells of random points inside it,
    /// their borders warped into irregular rifts, each fragment given a
    /// motion carrying it away from the plate's centre. The gaps they open
    /// fill with new sea floor at the next resample: a rift becomes an ocean.
    /// Returns the plates that rifted.
    fn rift(&mut self) -> usize {
        if self.params.rift_rate <= 0.0 {
            return 0;
        }
        let n = self.points.len();
        let typical = n as f32 / self.params.plates.max(1) as f32;
        let r_km = self.params.radius_km;
        let heat = self.heat();
        // Each plate's grid points, in one pass: over billions of years rifts
        // multiply plates, and a scan of the grid per plate would not keep up.
        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); self.plates.len()];
        for j in 0..n {
            groups[self.owner[j] as usize].push(j);
        }
        let mut rifted = 0;
        for plate in 0..self.plates.len() as u16 {
            let members = std::mem::take(&mut groups[plate as usize]);
            let area = members.len() as f32;
            let land = members
                .iter()
                .filter(|&&j| self.crust[j].kind == Kind::Continental)
                .count() as f32;
            // Every plate draws the same numbers whether or not it rifts, so
            // one plate's fate never shifts another's.
            let roll = self.rng.range(0.0, 1.0);
            let pieces = 2 + (self.rng.next_u64() % 3) as usize;
            let warp_seed = self.rng.next_u64() as u32;
            let picks: Vec<u64> = (0..4).map(|_| self.rng.next_u64()).collect();
            let speeds: Vec<f32> = (0..4).map(|_| self.rng.range(20.0, 50.0)).collect();
            if area < typical * MIN_RIFT_AREA {
                continue;
            }
            // A hot planet breaks up even its sea-floor plates.
            let lambda =
                self.params.rift_rate * (land / area + HOT_RIFT * (heat - 1.0)) * (area / typical);
            if roll >= 1.0 - (-lambda).exp() {
                continue;
            }
            let centres: Vec<Vec3> = (0..pieces)
                .map(|f| self.points[members[(picks[f] % members.len() as u64) as usize]])
                .collect();
            let fragment = |p: Vec3| {
                centres
                    .iter()
                    .enumerate()
                    .map(|(f, c)| {
                        let warp = 0.12 * fbm(p, 3.0, 3, warp_seed.wrapping_add(f as u32));
                        (f, p.dot(*c).clamp(-1.0, 1.0).acos() + warp)
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map_or(0, |(f, _)| f)
            };
            let assigned: Vec<usize> = members.iter().map(|&j| fragment(self.points[j])).collect();
            if (0..pieces).any(|f| !assigned.contains(&f)) {
                continue; // a fragment came out empty: no clean break
            }
            let centre = members
                .iter()
                .map(|&j| self.points[j])
                .sum::<Vec3>()
                .normalize();
            let parent = self.plates[plate as usize].motion;
            let max_rate = self.params.max_speed / r_km;
            let mut ids = vec![plate];
            for _ in 1..pieces {
                ids.push(self.plates.len() as u16);
                self.plates.push(Plate {
                    motion: parent,
                    rotation: Quat::IDENTITY,
                });
            }
            for f in 0..pieces {
                let piece: Vec3 = members
                    .iter()
                    .zip(&assigned)
                    .filter(|(_, a)| **a == f)
                    .map(|(&j, _)| self.points[j])
                    .sum::<Vec3>()
                    .normalize();
                // Spin about c × f carries f directly away from c.
                let away = centre.cross(piece).normalize_or_zero();
                let spin = parent.axis * parent.rate + away * speeds[f] / r_km;
                let rate = spin.length();
                if rate > 1e-9 {
                    self.plates[ids[f] as usize].motion = Motion {
                        axis: spin / rate,
                        rate: rate.min(max_rate),
                    };
                }
            }
            for (&j, &f) in members.iter().zip(&assigned) {
                self.owner[j] = ids[f];
            }
            rifted += 1;
        }
        rifted
    }

    /// Raise the overriding plates over their trenches: every live sample
    /// within [`SUBDUCTION_REACH`] of an uplift source on its own plate rises
    /// by the paper's `u0 · f(d) · g(v) · h`, measured from its nearest
    /// source. Distances run over the plate's own samples, which move
    /// rigidly together, so the reference grid's spacings are theirs.
    fn raise_over_trenches(&mut self, sources: &BTreeMap<usize, (f32, f32, Vec3)>, dt: f32) {
        if sources.is_empty() {
            return;
        }
        let n = self.points.len();
        let reach = SUBDUCTION_REACH / self.params.radius_km;
        let mut best: Vec<Option<(f32, usize)>> = vec![None; n];
        let mut heap = BinaryHeap::new();
        for &s in sources.keys() {
            heap.push(Reverse((0u32, s as u32, s as u32)));
        }
        while let Some(Reverse((bits, i, src))) = heap.pop() {
            let (i, d) = (i as usize, f32::from_bits(bits));
            if best[i].is_some() {
                continue;
            }
            best[i] = Some((d, src as usize));
            for &k in &self.neighbours[i] {
                let k = k as usize;
                if best[k].is_none() && self.alive[k] && self.owner[k] == self.owner[i] {
                    let nd = d + self.points[i].distance(self.points[k]);
                    if nd <= reach {
                        heap.push(Reverse((nd.to_bits(), k as u32, src)));
                    }
                }
            }
        }
        let flood = self.flood();
        let limit = self.mountain_limit();
        let era_speed = self.params.max_speed * self.speed_scale();
        for (i, b) in best.iter().enumerate() {
            let Some((d, src)) = *b else { continue };
            let (h, v, push) = sources[&src];
            // The push is in today's frame; the crust is held in the grid's
            // frame of the last resample, where its plate's rotation since
            // has carried it from.
            let push = self.plates[self.owner[i] as usize].rotation.inverse() * push;
            let km = d * self.params.radius_km;
            let c = &mut self.crust[i];
            let profile = match c.kind {
                Kind::Continental => cordillera(km),
                Kind::Oceanic => island_arc(km),
            };
            // Closing speed against the era's own top speed: on a hot planet
            // every plate is faster, and against today's top speed ordinary
            // convergence pinned the term at its cap.
            let rate = UPLIFT * profile * (v / era_speed).min(1.5) * (HEIGHT_FLOOR + h);
            let saturate = if c.kind == Kind::Continental {
                (1.0 - c.elevation.max(0.0) / limit).max(0.0)
            } else {
                1.0
            };
            let dz = rate * saturate * dt;
            if c.kind == Kind::Continental {
                self.raised_by_subduction += dz;
            }
            // Arc crust becomes continent once it has built past the sea.
            let c = &mut self.crust[i];
            crust::raise_past(c, dz, flood);
            if c.kind == Kind::Continental && dz > OROGENY_UPLIFT {
                c.orogeny = Orogeny::Andean;
                c.age = 0.0;
                for (f, p) in c.fold.iter_mut().zip(push.to_array()) {
                    *f += FOLD_RESPONSE * p * dt;
                }
            }
        }
    }

    /// Slab pull (Cortial et al. §4.1): a diving plate's spin axis turns by
    /// `ε Σ (c × q) / |c × q|` over its front samples `q` (`c` its centroid),
    /// drawing it toward the trench; long fronts pull hardest. Speeds stay
    /// within [`Params::max_speed`].
    fn pull_slabs(
        &mut self,
        live: &[usize],
        moved: &[Vec3],
        fronts: &BTreeMap<u16, Vec<Vec3>>,
        dt: f32,
    ) {
        if fronts.is_empty() {
            return;
        }
        let mut centroid = vec![Vec3::ZERO; self.plates.len()];
        for (k, &i) in live.iter().enumerate() {
            centroid[self.owner[i] as usize] += moved[k];
        }
        let max_rate = self.params.max_speed / self.params.radius_km;
        for (&plate, front) in fronts {
            let c = centroid[plate as usize].normalize_or_zero();
            let pull: Vec3 = front.iter().map(|&q| c.cross(q).normalize_or_zero()).sum();
            let m = &mut self.plates[plate as usize].motion;
            let spin = m.axis * m.rate + pull * SLAB_PULL * dt;
            let rate = spin.length();
            if rate > 1e-9 {
                m.axis = spin / rate;
                m.rate = rate.min(max_rate);
            }
        }
    }

    /// Suture colliding continents (Cortial et al. §4.2). `zone[j]` is the
    /// pair of plates whose continents both covered grid point `j`. For each
    /// pair overlapping by at least [`COLLISION_AREA`], the plate with less
    /// continent gives up the colliding landmass (its continental points
    /// connected to the overlap) to the other. The joined continent is raised
    /// by `Δz(p) = Δ · (1 − (d/r)²)²`, `d` the distance to the suture (the
    /// paper measures from the whole terrane; see below), over a radius
    /// `r = r_c · √(v/v₀ · A/A₀)` that grows with the terrane's area `A` and
    /// the closing speed `v`. Returns the collisions sutured, and the
    /// continental uplift they raised (km, summed over samples).
    fn collide(
        &self,
        zone: &[Option<(u16, u16)>],
        owner: &mut [u16],
        crust: &mut [Crust],
    ) -> (usize, f32) {
        let mut raised = 0.0;
        let n = self.points.len();
        let r_km = self.params.radius_km;
        let cell_area = 4.0 * std::f32::consts::PI * r_km * r_km / n as f32;
        let mut counts: BTreeMap<(u16, u16), usize> = BTreeMap::new();
        for z in zone.iter().flatten() {
            *counts.entry(*z).or_default() += 1;
        }
        let mut sutured = 0;
        let limit = self.mountain_limit();
        for (&(a, b), &count) in &counts {
            if (count as f32) * cell_area < COLLISION_AREA {
                continue;
            }
            let land = |plate: u16, owner: &[u16], crust: &[Crust]| {
                (0..n)
                    .filter(|&j| owner[j] == plate && crust[j].kind == Kind::Continental)
                    .count()
            };
            let (la, lb) = (land(a, owner, crust), land(b, owner, crust));
            let (donor, receiver) = if la < lb { (a, b) } else { (b, a) };

            // The terrane: the overlap and the donor's continent joined to it.
            let mut terrane = vec![false; n];
            let mut stack: Vec<usize> = (0..n).filter(|&j| zone[j] == Some((a, b))).collect();
            for &j in &stack {
                terrane[j] = true;
            }
            while let Some(j) = stack.pop() {
                for &k in &self.neighbours[j] {
                    let k = k as usize;
                    if !terrane[k] && owner[k] == donor && crust[k].kind == Kind::Continental {
                        terrane[k] = true;
                        stack.push(k);
                    }
                }
            }
            let members: Vec<usize> = (0..n).filter(|&j| terrane[j]).collect();
            for &j in &members {
                owner[j] = receiver;
            }

            // How far the collision raises ground, and how high.
            let centre: Vec3 = members
                .iter()
                .map(|&j| self.points[j])
                .sum::<Vec3>()
                .normalize();
            let area = members.len() as f32 * cell_area;
            let typical = 4.0 * std::f32::consts::PI * r_km * r_km / self.params.plates as f32;
            let speed = (self.velocity(donor, centre) - self.velocity(receiver, centre)).length();
            let radius_km = (COLLISION_REACH
                * ((speed / self.params.max_speed) * (area / typical)).sqrt())
            .clamp(COLLISION_REACH_MIN, COLLISION_REACH);
            let surge =
                COLLISION_MAX * (1.0 - (-COLLISION_COEFFICIENT * area / COLLISION_MAX).exp());

            // Distances from the suture (the overlap), over the joined
            // continent on both sides of it. Measuring from the whole
            // terrane, as the paper's d(p, R) does, raises everything inside
            // a colliding continent by the full surge: a continent-sized
            // terrane became one plateau.
            let radius = radius_km / r_km;
            let mut dist = vec![f32::INFINITY; n];
            let mut heap = BinaryHeap::new();
            for j in (0..n).filter(|&j| zone[j] == Some((a, b))) {
                heap.push(Reverse((0u32, j as u32)));
            }
            while let Some(Reverse((bits, j))) = heap.pop() {
                let (j, d) = (j as usize, f32::from_bits(bits));
                if d >= dist[j] {
                    continue;
                }
                dist[j] = d;
                for &k in &self.neighbours[j] {
                    let k = k as usize;
                    if owner[k] == receiver && crust[k].kind == Kind::Continental {
                        let nd = d + self.points[j].distance(self.points[k]);
                        if nd < radius && nd < dist[k] {
                            heap.push(Reverse((nd.to_bits(), k as u32)));
                        }
                    }
                }
            }
            for j in 0..n {
                if !dist[j].is_finite() {
                    continue;
                }
                let x = dist[j] / radius;
                let falloff = (1.0 - x * x).max(0.0).powi(2);
                let c = &mut crust[j];
                let saturate = (1.0 - c.elevation.max(0.0) / limit).max(0.0);
                raised += surge * falloff * saturate;
                crust::raise(c, surge * falloff * saturate);
                if falloff > 0.1 {
                    c.orogeny = Orogeny::Himalayan;
                    c.age = 0.0;
                    // Folds run across the push: (n × (p − q)/|p − q|) × n.
                    let p = self.points[j];
                    let away = (p - centre).normalize_or_zero();
                    c.fold = p.cross(away).cross(p).to_array();
                }
            }
            sutured += 1;
        }
        (sutured, raised)
    }

    fn continental_share(&self) -> f32 {
        let land = self
            .crust
            .iter()
            .filter(|c| c.kind == Kind::Continental)
            .count();
        land as f32 / self.crust.len() as f32
    }

    /// Put the moved crust back on the grid.
    ///
    /// Each grid point looks for the moved samples within [`COVER`]
    /// spacings of it, keeping the nearest from each plate.
    ///
    /// A point no plate covers lies in a gap: new sea floor. Its plate is
    /// the nearest across the gap, and its age comes from where it sits
    /// between the two plates the gap opened between: crust at the ridge
    /// midway is new, crust at a plate's edge is as old as the gap (the time
    /// since the last resample), as at a real spreading ridge. Distances are
    /// measured through the gap along the grid (two nearest plates per point,
    /// by Dijkstra).
    fn resample(&mut self) {
        let n = self.points.len();
        let elapsed = self.since_resample as f32 * self.params.step;
        let moved: Vec<Vec3> = (0..n)
            .map(|i| self.plates[self.owner[i] as usize].rotation * self.points[i])
            .collect();
        // Only crust still at the surface: subducted samples have gone under.
        let live: Vec<Vec3> = (0..n)
            .map(|i| {
                if self.alive[i] {
                    moved[i]
                } else {
                    Vec3::splat(9.0)
                }
            })
            .collect();
        let index = Index::new(&live, self.spacing * 2.0);
        let cover = COVER * self.spacing;

        let mut owner = vec![u16::MAX; n];
        let mut crust = vec![Crust::oceanic(0.0); n];
        let mut gap = vec![false; n];
        let mut zone: Vec<Option<(u16, u16)>> = vec![None; n];
        let mut overlapped = 0;
        // (plate, squared distance, sample) of the nearest sample per plate.
        let mut nearest: Vec<(u16, f32, usize)> = Vec::with_capacity(4);
        for j in 0..n {
            let p = self.points[j];
            nearest.clear();
            index.near(p, cover, |i| {
                let d = live[i].distance_squared(p);
                if d > cover * cover {
                    return;
                }
                let plate = self.owner[i];
                match nearest.iter_mut().find(|e| e.0 == plate) {
                    Some(e) if d < e.1 => *e = (plate, d, i),
                    Some(_) => {}
                    None => nearest.push((plate, d, i)),
                }
            });
            // Deterministic whatever order the index yields samples in.
            nearest.sort_by_key(|a| a.0);
            let winner = match nearest.len() {
                0 => {
                    gap[j] = true;
                    continue;
                }
                1 => nearest[0],
                _ => {
                    overlapped += 1;
                    // Two plates' continents here: a collision zone.
                    let mut lands = nearest
                        .iter()
                        .filter(|e| self.crust[e.2].kind == Kind::Continental)
                        .map(|e| e.0);
                    if let (Some(x), Some(y)) = (lands.next(), lands.next()) {
                        zone[j] = Some(pair(x, y));
                    }
                    *nearest
                        .iter()
                        .min_by(|a, b| on_top(&self.crust[a.2], a.1, &self.crust[b.2], b.1))
                        .expect("two or more")
                }
            };
            owner[j] = winner.0;
            crust[j] = self.crust[winner.2];
            // The fold turns with its plate, and lies flat on the sphere
            // where the crust has landed.
            let c = &mut crust[j];
            if c.fold != [0.0; 3] {
                let turned = self.plates[winner.0 as usize].rotation * Vec3::from(c.fold);
                c.fold = (turned - p * turned.dot(p)).to_array();
            }
        }

        let opened = gap.iter().filter(|&&g| g).count();
        if opened > 0 {
            self.fill_gaps(&gap, &mut owner, &mut crust, elapsed);
        }
        let (collisions, raised_by_collision) = self.collide(&zone, &mut owner, &mut crust);
        self.owner = owner;
        self.crust = crust;
        let rotations: Vec<Quat> = self.plates.iter().map(|p| p.rotation).collect();
        self.carry_craters(&rotations);
        for plate in &mut self.plates {
            plate.rotation = Quat::IDENTITY;
        }
        self.alive.fill(true);
        self.since_resample = 0;
        // On the fresh grid, where a change of owner moves nothing: tidy the
        // plates, then rift.
        self.tidy_plates();
        let rifts = self.rift();
        self.rehome_craters();
        self.stats = Stats {
            continental: self.continental_share(),
            opened,
            overlapped,
            subducted: std::mem::take(&mut self.subducted),
            collisions,
            rifts,
            raised_by_subduction: std::mem::take(&mut self.raised_by_subduction),
            raised_by_collision,
        };
    }

    fn fill_gaps(&self, gap: &[bool], owner: &mut [u16], crust: &mut [Crust], elapsed: f32) {
        let n = self.points.len();
        // Up to two (plate, distance) labels per gap point: its two nearest
        // plates across the gap.
        let mut labels: Vec<Vec<(u16, f32)>> = vec![Vec::new(); n];
        let mut heap = BinaryHeap::new();
        for j in 0..n {
            if !gap[j] && self.neighbours[j].iter().any(|&k| gap[k as usize]) {
                heap.push(Reverse((0u32, j as u32, owner[j])));
            }
        }
        while let Some(Reverse((bits, j, plate))) = heap.pop() {
            let (j, d) = (j as usize, f32::from_bits(bits));
            if gap[j] {
                let l = &mut labels[j];
                if l.len() >= 2 || l.iter().any(|e| e.0 == plate) {
                    continue;
                }
                l.push((plate, d));
            }
            for &k in &self.neighbours[j] {
                let k = k as usize;
                let l = &labels[k];
                if gap[k] && l.len() < 2 && !l.iter().any(|e| e.0 == plate) {
                    let nd = d + self.points[j].distance(self.points[k]);
                    // Non-negative floats order the same as their bits.
                    heap.push(Reverse((nd.to_bits(), k as u32, plate)));
                }
            }
        }
        for j in (0..n).filter(|&j| gap[j]) {
            let age = match labels[j][..] {
                [(a, da), (_, db)] => {
                    owner[j] = a;
                    elapsed * (da - db).abs() / (da + db).max(1e-9)
                }
                [(a, _)] => {
                    owner[j] = a;
                    elapsed
                }
                // Cut off from every plate: keep its old owner, brand new.
                _ => {
                    owner[j] = self.owner[j];
                    0.0
                }
            };
            crust[j] = Crust::oceanic(age);
        }
    }
}

/// Spacings within which a moved sample covers a grid point. A rotated copy
/// of the grid has a sample within ~0.6 spacings of every point; past that,
/// the point is in a gap.
const COVER: f32 = 0.85;

/// A continental shelf's depth, km (Earth's average shelf break is ~130 m
/// down; a little deeper reads at this resolution).
const SHELF_DEPTH: f32 = -0.15;
/// Spacings of shelf off a coast before the slope (~100 km at 70 km
/// resolution; Earth's average shelf is ~80 km wide, some over 1000).
const SHELF_WIDTH: f32 = 1.5;
/// Spacings off a coast by which the slope reaches the sea floor's own depth.
const SHELF_REACH: f32 = 3.5;
/// A hotspot plume's radius, in spacings.
const PLUME_RADIUS: f32 = 1.5;
/// Volcanic growth over a plume's centre, km per My: a sample passes over a
/// plume in a few My at typical plate speeds, and builds an island.
const HOTSPOT_RATE: f32 = 3.0;
/// Tallest a hotspot volcano grows above the sea floor, km: Hawaii's stand
/// ~9–10 km above theirs.
const VOLCANO_CAP: f32 = 9.0;
/// A plate smaller than this share of a typical plate does not rift.
const MIN_RIFT_AREA: f32 = 0.3;
/// Chance, per unit of heat above today's, that young continental crust
/// arriving at a trench is dragged down rather than colliding: 0.875 at
/// 3.5×, ~0.06 by 4 Gyr. Much of the early Earth's crust is thought to have
/// been recycled into the mantle before it could stabilise; without this,
/// continental crust (immortal once made) reached its long-run share in a
/// quarter of a billion years.
const HOT_RECYCLE: f32 = 0.35;
/// Continental crust this many My past its last mountain building counts as
/// stabilised (a craton's core), and resists being dragged down.
const CRATON_AGE: f32 = 300.0;
/// A plate smaller than this share of a typical plate is a sliver, merged
/// into its neighbour: ~200 000 km² for an Earth-sized planet of ten plates,
/// about the size of Earth's smallest named microplates.
const SLIVER: f32 = 0.15;
/// Smallest crater the crust records, in grid spacings of radius: smaller
/// ones fall between the samples and would leave a single dented point.
const IMPACT_MIN: f32 = 1.5;
/// Out to this share of its radius, the crust struck is a crater's heart
/// and carries its [`Crust::scar`]: the whole bowl. Each resample takes the
/// nearest moved sample for each grid point, which drops or doubles single
/// samples at random; a heart of half the radius (one or two samples for the
/// smallest craters) was lost within a few hundred My though its bowl stayed.
const SCAR_CORE: f32 = 1.0;
/// How far a hot mantle floods the continents, km per unit of heat above
/// today's: 1.25 km at 3.5× ([`Sim::flood`]). Hotter, more buoyant mantle
/// held young oceans shallower; the figure is an estimate, not a measured
/// relation.
const HOT_FLOOD: f32 = 0.5;
/// Rift chance a wholly oceanic plate gains per unit of heat above today's,
/// as a share of a continental plate's: hot young planets are thought to
/// have had smaller, shorter-lived plates.
const HOT_RIFT: f32 = 0.3;

/// Spacings around a trench within which the diving plate's sea floor sinks
/// to [`TRENCH`].
const TRENCH_REACH: f32 = 2.0;
/// Trench elevation, km: Earth's run 7–11 km deep.
const TRENCH: f32 = -8.0;
/// Deepest trench, km: the bottom of the height scale the diving crust is
/// measured on (Cortial et al.'s `z_t`).
const TRENCH_FLOOR: f32 = -10.0;
/// How far inland of a trench subduction raises the overriding plate, km.
/// Cortial et al.'s `r_s` is 1800 km; Earth's subduction ranges are belts a
/// few hundred km wide (the Andes ~300–700 km), and at 1800 a small
/// continent ringed by trenches was raised everywhere, all the time: on a
/// hot planet ~90% of all land had been lifted within the last 50 My.
const SUBDUCTION_REACH: f32 = 900.0;
/// Where subduction raises a continent most, km behind the trench (the
/// Andes' crest stands ~200–400 km inland).
const CORDILLERA_PEAK: f32 = 300.0;
/// How quickly a cordillera falls away inland of its crest, km: to a quarter
/// by ~550 km behind the trench (Earth's cordilleras are a few hundred km
/// wide).
const CORDILLERA_FALL: f32 = 210.0;
/// Where an island arc stands behind its trench, km, and its half-width:
/// Earth's arcs are volcanic chains ~100–200 km wide, ~150–300 km behind
/// the trench.
const ARC_PEAK: f32 = 230.0;
const ARC_WIDTH: f32 = 90.0;
/// Base subduction uplift, km per million years (Cortial et al.'s `u₀`,
/// 0.6 mm/y).
const UPLIFT: f32 = 0.6;
/// Added to the paper's height term `h = z̃²`. Taken alone it is ~0.04 for
/// ordinary sea floor going down: an island arc would take ~170 My to
/// surface where the paper's own example surfaces one in ~10 My, and
/// Earth's arcs emerge within 10–20 My of subduction starting. With the
/// floor, moderate convergence (~45 km/My) raises a crest ~0.14 km/My (the
/// Andes have risen at 0.2–0.5), an arc surfaces in a few tens of My, and
/// higher crust going down still raises more. Raising it further lifts
/// whole oceans: at 1.0 the land doubled in 240 My.
const HEIGHT_FLOOR: f32 = 0.5;
/// Land stops rising toward this height, km: uplift tapers to nothing as
/// mountains approach it (a stand-in for isostasy), so ranges do not grow
/// without bound. On a hot planet the limit falls with the heat
/// ([`Sim::mountain_limit`]).
const MOUNTAIN_LIMIT: f32 = 6.0;
/// Uplift in one step, km, that counts as mountain building (resets the
/// orogeny age).
const OROGENY_UPLIFT: f32 = 0.01;
/// How strongly the convergence direction bends the fold direction
/// (Cortial et al.'s `β`).
const FOLD_RESPONSE: f32 = 0.05;
/// Slab pull strength, radians per My² per front sample (Cortial et al.'s
/// `ε`, "much less than 1"): a front of a hundred samples turns a plate's
/// spin by about 1% of the fastest rate per step.
const SLAB_PULL: f32 = 1e-6;
/// Overlap between two continents, km², that sutures them: roughly the
/// paper's 300 km of interpenetration along a 1000 km front.
const COLLISION_AREA: f32 = 3.0e5;
/// Largest distance a collision raises ground over, km, from the suture.
/// Cortial et al.'s `r_c` is 4200 km; the Himalaya and the Tibetan plateau
/// reach ~1000–1500 km from theirs, and at 4200 a collision raised most of a
/// continent. Only the largest, fastest collisions reach this far: the
/// radius grows with the terrane's size and speed.
const COLLISION_REACH: f32 = 1500.0;
/// Smallest, km: even a small terrane raises the ground it meets. An island
/// arc docking raises a belt along the suture, not its new continent.
const COLLISION_REACH_MIN: f32 = 250.0;
/// The paper's discrete collision coefficient, per km: `Δ = Δ_c · A`.
const COLLISION_COEFFICIENT: f32 = 1.3e-5;
/// The surge saturates toward this, km: `Δ_c · A` taken literally would
/// raise a continent-sized terrane by over 100 km.
const COLLISION_MAX: f32 = 4.5;

/// Subduction uplift of a continent by distance inland of the trench (the
/// paper's `f`): rising from a quarter at the trench to full at
/// [`CORDILLERA_PEAK`], then falling away over [`CORDILLERA_FALL`], gone by
/// [`SUBDUCTION_REACH`].
fn cordillera(km: f32) -> f32 {
    if km < CORDILLERA_PEAK {
        0.25 + 0.75 * smoothstep(0.0, CORDILLERA_PEAK, km)
    } else {
        let x = (km - CORDILLERA_PEAK) / CORDILLERA_FALL;
        (-x * x).exp() * (1.0 - smoothstep(CORDILLERA_PEAK, SUBDUCTION_REACH, km))
    }
}

/// Subduction uplift of sea floor by distance behind the trench: a narrow
/// band, the volcanic arc, centred [`ARC_PEAK`] back.
fn island_arc(km: f32) -> f32 {
    let x = (km - ARC_PEAK) / ARC_WIDTH;
    (-x * x).exp()
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A pair of plates, lower id first.
fn pair(a: u16, b: u16) -> (u16, u16) {
    (a.min(b), a.max(b))
}

/// Whether crust `a` dives under crust `b` (`Some(true)`), `b` under `a`
/// (`Some(false)`), or neither (two continents: they collide).
fn dives(a: &Crust, b: &Crust) -> Option<bool> {
    match (a.kind, b.kind) {
        (Kind::Oceanic, Kind::Continental) => Some(true),
        (Kind::Continental, Kind::Oceanic) => Some(false),
        // The older, colder, denser sea floor dives.
        (Kind::Oceanic, Kind::Oceanic) => Some(a.age > b.age),
        (Kind::Continental, Kind::Continental) => None,
    }
}

/// Which of two overlapping crusts stays on top (`Less` = `a` does):
/// continental over oceanic; of two oceanic, the younger (less dense); of
/// two continental, the nearer sample (collision comes in a later phase).
fn on_top(a: &Crust, da: f32, b: &Crust, db: f32) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.kind, b.kind) {
        (Kind::Continental, Kind::Oceanic) => Ordering::Less,
        (Kind::Oceanic, Kind::Continental) => Ordering::Greater,
        (Kind::Oceanic, Kind::Oceanic) => a.age.total_cmp(&b.age),
        (Kind::Continental, Kind::Continental) => da.total_cmp(&db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small(seed: u64) -> Params {
        Params {
            samples: 6000,
            ..Params::earth_like(seed)
        }
    }

    #[test]
    fn a_run_is_deterministic_from_its_seed() {
        let mut a = Sim::new(small(7));
        let mut b = Sim::new(small(7));
        let (mut fa, mut fb) = (Vec::new(), Vec::new());
        a.run(60.0, |f| fa.push(f.clone()));
        b.run(60.0, |f| fb.push(f.clone()));
        assert_eq!(fa, fb);
        assert_eq!(fa.len(), 4, "the start and three resamples");
        let mut c = Sim::new(small(8));
        let mut fc = Vec::new();
        c.run(60.0, |f| fc.push(f.clone()));
        assert_ne!(fa, fc, "another seed is another planet");
    }

    #[test]
    fn the_start_is_one_supercontinent_of_the_share_asked_for() {
        let sim = Sim::new(small(3));
        let share = sim.frame().stats.continental;
        assert!((0.2..0.4).contains(&share), "{share}");
        // It spans more than one plate, so drift can tear it.
        let plates: std::collections::HashSet<u16> = sim
            .frame()
            .crust
            .iter()
            .zip(&sim.frame().owner)
            .filter(|(c, _)| c.kind == Kind::Continental)
            .map(|(_, &o)| o)
            .collect();
        assert!(plates.len() >= 2, "on {} plates", plates.len());
    }

    /// Two plates: the northern hemisphere and the southern, turning about
    /// the x axis in opposite senses. They pull apart along the equator on
    /// the +z side (a ridge opens) and push together on the −z side.
    fn hemispheres(rate: f32) -> Sim {
        let params = Params {
            samples: 8000,
            plates: 2,
            ..Params::earth_like(1)
        };
        let motions = vec![
            Motion {
                axis: Vec3::X,
                rate: -rate,
            },
            Motion {
                axis: Vec3::X,
                rate,
            },
        ];
        Sim::with(params, motions, |p| {
            let plate = if p.y >= 0.0 { 0 } else { 1 };
            // Continents round the two poles, clear of the equator.
            let crust = if p.y.abs() > 0.8 {
                Crust::continental(0.5, 500.0)
            } else {
                Crust::oceanic(100.0)
            };
            (plate, crust)
        })
    }

    #[test]
    fn plates_pulling_apart_open_new_sea_floor_youngest_at_the_ridge() {
        // ~4° of opening on each side over one resample interval.
        let rate = 0.0035;
        let mut sim = hemispheres(rate);
        let land_before = sim.frame().stats.continental;
        let mut frame = None;
        while frame.is_none() {
            frame = sim.step();
        }
        let frame = frame.unwrap();
        assert!(frame.stats.opened > 0, "a gap opened");
        assert!(frame.owner.iter().all(|&o| o < 2), "every point owned");

        // Along the +z meridian: the new crust is youngest at the equator
        // and as old as the interval at the gap's edges.
        let elapsed = 20.0;
        let points = sim.points();
        let mut by_latitude: Vec<(f32, f32)> = points
            .iter()
            .zip(&frame.crust)
            .filter(|(p, c)| p.z > 0.95 && c.age < elapsed + 0.01)
            .map(|(p, c)| (p.y.asin(), c.age))
            .collect();
        by_latitude.sort_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
        assert!(by_latitude.len() > 3, "new crust along the ridge");
        let (ridge, edge) = (by_latitude[0], by_latitude[by_latitude.len() - 1]);
        assert!(
            ridge.1 < edge.1,
            "ridge {ridge:?} younger than edge {edge:?}"
        );
        assert!(ridge.1 < elapsed * 0.35, "ridge crust is new: {ridge:?}");
        // New crust stands high; old sea floor away from the ridge is deep.
        let young = Crust::oceanic(ridge.1).elevation;
        assert!(young > crust::ocean_elevation(100.0));

        // The polar continents rode along untouched.
        let land_after = frame.stats.continental;
        assert!(
            (land_after - land_before).abs() < 0.01,
            "land {land_before} → {land_after}"
        );
    }

    #[test]
    fn where_oceans_meet_the_older_dives_and_the_trench_stays_clean() {
        // The hemispheres again, now oceanic both: young in the north, old in
        // the south. On the −z side they converge; the south must dive.
        let rate = 0.0035;
        let params = Params {
            samples: 8000,
            plates: 2,
            ..Params::earth_like(1)
        };
        let motions = vec![
            Motion {
                axis: Vec3::X,
                rate: -rate,
            },
            Motion {
                axis: Vec3::X,
                rate,
            },
        ];
        let mut sim = Sim::with(params, motions, |p| {
            if p.y >= 0.0 {
                (0, Crust::oceanic(30.0))
            } else {
                (1, Crust::oceanic(120.0))
            }
        });
        let mut frame = None;
        while frame.is_none() {
            frame = sim.step();
        }
        let frame = frame.unwrap();
        assert!(frame.stats.subducted > 0, "crust went under");
        // Nothing piled up: at most the one row of grid points on the trench
        // line itself, which sees a sample of each plate on either side. The
        // front is half the equator: π / spacing points.
        let front = (std::f32::consts::PI / sim.spacing) as usize;
        assert!(
            frame.stats.overlapped <= front,
            "{} overlapping points along a {front}-point front",
            frame.stats.overlapped
        );
        // The north (overriding) plate advanced south over the trench on the
        // −z side: points a little south of the equator there are its own.
        let points = sim.points();
        let advanced = points
            .iter()
            .zip(&frame.owner)
            .filter(|(p, _)| p.z < -0.95 && (-0.05..-0.01).contains(&p.y))
            .collect::<Vec<_>>();
        assert!(!advanced.is_empty());
        assert!(
            advanced.iter().all(|(_, o)| **o == 0),
            "the overriding plate holds the ground it moved over"
        );
        // No stranded islands of either plate across the line.
        let strays = points
            .iter()
            .zip(&frame.owner)
            .filter(|(p, o)| p.z < -0.5 && ((**o == 0 && p.y < -0.12) || (**o == 1 && p.y > 0.02)))
            .count();
        assert_eq!(strays, 0, "the trench is a clean line");
    }

    /// The hemispheres with crust of the caller's choosing.
    fn hemispheres_of(
        rate: f32,
        north: impl Fn(Vec3) -> Crust,
        south: impl Fn(Vec3) -> Crust,
    ) -> Sim {
        let params = Params {
            samples: 8000,
            plates: 2,
            ..Params::earth_like(1)
        };
        let motions = vec![
            Motion {
                axis: Vec3::X,
                rate: -rate,
            },
            Motion {
                axis: Vec3::X,
                rate,
            },
        ];
        Sim::with(params, motions, |p| {
            if p.y >= 0.0 {
                (0, north(p))
            } else {
                (1, south(p))
            }
        })
    }

    fn run_for(sim: &mut Sim, my: f32) -> Frame {
        let until = sim.time() + my;
        let mut last = sim.frame();
        sim.run(until, |f| last = f.clone());
        last
    }

    #[test]
    fn an_ocean_diving_under_a_continent_raises_mountains_and_cuts_a_trench() {
        let mut sim = hemispheres_of(
            0.0035,
            |_| Crust::continental(0.5, 500.0),
            |_| Crust::oceanic(120.0),
        );
        let start = sim.frame().plates[1];
        let frame = run_for(&mut sim, 20.0);
        let points = sim.points().to_vec();
        let on_converging_side = |p: &Vec3| p.z < -0.9;
        // Inland of the trench (north of the equator, −z side) the land rose
        // and was marked as Andean; far inland it did not.
        let near: Vec<&Crust> = points
            .iter()
            .zip(&frame.crust)
            .filter(|(p, c)| on_converging_side(p) && c.kind == Kind::Continental)
            .map(|(_, c)| c)
            .collect();
        let highest = near.iter().map(|c| c.elevation).fold(f32::MIN, f32::max);
        assert!(highest > 1.0, "a coastal range: highest {highest} km");
        assert!(near.iter().any(|c| c.orogeny == Orogeny::Andean));
        let pole = points
            .iter()
            .zip(&frame.crust)
            .filter(|(p, _)| p.y > 0.97)
            .map(|(_, c)| c.elevation)
            .fold(f32::MIN, f32::max);
        assert!(pole < 0.55, "the far interior did not rise: {pole} km");
        // A trench on the diving side.
        let deepest = points
            .iter()
            .zip(&frame.crust)
            .filter(|(p, c)| on_converging_side(p) && c.kind == Kind::Oceanic)
            .map(|(_, c)| c.elevation)
            .fold(f32::MAX, f32::min);
        assert!(deepest <= -7.5, "a trench: deepest {deepest} km");
        // Slab pull turned the diving plate, within the speed limit.
        let now = frame.plates[1];
        assert_ne!(now, start, "the slab pulled");
        let max_rate = sim.params.max_speed / sim.params.radius_km;
        assert!(now.rate <= max_rate + 1e-9);
    }

    #[test]
    fn where_oceans_converge_an_island_arc_rises() {
        let mut sim = hemispheres_of(0.0035, |_| Crust::oceanic(30.0), |_| Crust::oceanic(120.0));
        let frame = run_for(&mut sim, 100.0);
        let arc = sim
            .points()
            .iter()
            .zip(&frame.crust)
            .filter(|(p, c)| p.z < -0.5 && c.kind == Kind::Continental)
            .count();
        assert!(arc > 0, "new land behind the trench");
        // Only there: the diverging side made none.
        let elsewhere = sim
            .points()
            .iter()
            .zip(&frame.crust)
            .filter(|(p, c)| p.z > 0.5 && c.kind == Kind::Continental)
            .count();
        assert_eq!(elsewhere, 0);
    }

    #[test]
    fn colliding_continents_suture_into_one_plate_and_raise_mountains() {
        // Continents astride the equator on the converging (−z) side, one on
        // each plate, already touching: they collide at once.
        let continent = |p: Vec3| p.z < -0.7 && p.y.abs() < 0.4;
        let mut sim = hemispheres_of(
            0.0035,
            |p| {
                if continent(p) {
                    Crust::continental(0.5, 500.0)
                } else {
                    Crust::oceanic(60.0)
                }
            },
            |p| {
                if continent(p) {
                    Crust::continental(0.5, 500.0)
                } else {
                    Crust::oceanic(60.0)
                }
            },
        );
        let land_before = sim.frame().stats.continental;
        let frame = run_for(&mut sim, 20.0);
        assert!(frame.stats.collisions >= 1, "{:?}", frame.stats);
        // One plate holds the whole joined continent: no confetti.
        let owners: std::collections::BTreeSet<u16> = sim
            .points()
            .iter()
            .zip(&frame.crust)
            .zip(&frame.owner)
            .filter(|((p, c), _)| p.z < -0.5 && c.kind == Kind::Continental)
            .map(|(_, &o)| o)
            .collect();
        assert_eq!(owners.len(), 1, "sutured into one plate: {owners:?}");
        // Mountains along the suture, marked as collisional.
        let land: Vec<&Crust> = frame
            .crust
            .iter()
            .filter(|c| c.kind == Kind::Continental)
            .collect();
        let highest = land.iter().map(|c| c.elevation).fold(f32::MIN, f32::max);
        assert!(highest > 1.5, "a collision range: {highest} km");
        assert!(land.iter().any(|c| c.orogeny == Orogeny::Himalayan));
        // The overlap shortened the crust, but most of it survives.
        let land_after = frame.stats.continental;
        assert!(
            land_after > land_before * 0.7,
            "{land_before} → {land_after}"
        );
    }

    /// One plate over the whole sphere, with the crust and motion given.
    fn one_plate(rate: f32, rift_rate: f32, crust: impl Fn(Vec3) -> Crust) -> Sim {
        let params = Params {
            samples: 8000,
            plates: 1,
            rift_rate,
            ..Params::earth_like(4)
        };
        Sim::with(
            params,
            vec![Motion {
                axis: Vec3::Y,
                rate,
            }],
            |p| (0, crust(p)),
        )
    }

    #[test]
    fn a_rifting_continent_splits_its_plate_and_opens_an_ocean() {
        // A plate nearly all continent, made certain to rift.
        let mut sim = one_plate(0.002, 50.0, |p| {
            if p.y > -0.6 {
                Crust::continental(0.5, 500.0)
            } else {
                Crust::oceanic(80.0)
            }
        });
        let first = run_for(&mut sim, 20.0);
        assert!(first.stats.rifts >= 1, "{:?}", first.stats);
        assert!(first.plates.len() >= 2, "fragments became plates");
        // The fragments drift apart: the next resample finds a rift opened
        // and filled with brand-new sea floor where there was continent.
        let next = run_for(&mut sim, 20.0);
        assert!(next.stats.opened > 0, "{:?}", next.stats);
        let new_sea_in_old_land = sim
            .points()
            .iter()
            .zip(&next.crust)
            .filter(|(p, c)| p.y > -0.4 && c.kind == Kind::Oceanic && c.age <= 20.0)
            .count();
        assert!(
            new_sea_in_old_land > 0,
            "a young ocean in the old continent"
        );
    }

    #[test]
    fn a_plate_over_a_hotspot_leaves_a_chain_of_sinking_islands() {
        // Sea floor turning about y over a plume fixed at +x on the equator:
        // the plate moves from +x toward −z.
        let rate = 0.005;
        let mut sim = one_plate(rate, 0.0, |_| Crust::oceanic(60.0)).with_hotspots(vec![Vec3::X]);
        let frame = run_for(&mut sim, 40.0);
        let points = sim.points().to_vec();
        // Volcanic pile along the equator, by angle downstream of the plume.
        let along = |angle: f32| {
            let d = Vec3::new(angle.cos(), 0.0, -angle.sin());
            let nearest = (0..points.len())
                .min_by(|&a, &b| points[a].distance(d).total_cmp(&points[b].distance(d)))
                .unwrap();
            frame.crust[nearest].volcanic
        };
        let (fresh, older, oldest) = (along(0.0), along(rate * 15.0), along(rate * 35.0));
        assert!(fresh > 2.0, "a volcano over the plume: {fresh} km");
        assert!(
            older > 0.0 && older < fresh,
            "older down the chain: {older}"
        );
        assert!(oldest < older, "and older still: {oldest}");
        // The fresh one breaks the surface; it is an island, not a continent.
        let islands = frame
            .surface
            .iter()
            .zip(&frame.crust)
            .filter(|(z, c)| **z > 0.0 && c.kind == Kind::Oceanic)
            .count();
        assert!(islands > 0, "an island stands above the sea");
        assert_eq!(frame.stats.continental, 0.0);
    }

    #[test]
    fn shelves_fringe_passive_margins_but_not_trenches() {
        // Passive: a continent and its sea floor on one still plate.
        let sim = one_plate(0.0, 0.0, |p| {
            if p.y > 0.7 {
                Crust::continental(0.5, 500.0)
            } else {
                Crust::oceanic(100.0)
            }
        });
        let frame = sim.frame();
        let points = sim.points();
        let spacing = sim.spacing;
        let coast = (0.7f32).asin();
        let at = |lat: f32| {
            (0..points.len())
                .filter(|&j| frame.crust[j].kind == Kind::Oceanic)
                .min_by(|&a, &b| {
                    let la = (points[a].y.asin() - lat).abs();
                    let lb = (points[b].y.asin() - lat).abs();
                    la.total_cmp(&lb)
                })
                .unwrap()
        };
        let shelf = at(coast - spacing * 0.8);
        assert!(
            frame.surface[shelf] > -0.5,
            "shallow off the coast: {}",
            frame.surface[shelf]
        );
        let abyss = at(0.0);
        assert_eq!(
            frame.surface[abyss], frame.crust[abyss].elevation,
            "open ocean untouched"
        );

        // Active: an ocean diving under a continent keeps its trench.
        let mut active = hemispheres_of(
            0.0035,
            |_| Crust::continental(0.5, 500.0),
            |_| Crust::oceanic(120.0),
        );
        let frame = run_for(&mut active, 20.0);
        let deepest_by_coast = active
            .points()
            .iter()
            .zip(&frame.surface)
            .zip(&frame.crust)
            .filter(|((p, _), c)| p.z < -0.9 && c.kind == Kind::Oceanic)
            .map(|((_, &z), _)| z)
            .fold(f32::MAX, f32::min);
        assert!(
            deepest_by_coast < -7.0,
            "the trench, not a shelf: {deepest_by_coast}"
        );
    }

    fn young(seed: u64) -> Params {
        Params {
            samples: 6000,
            ..Params::primordial(seed)
        }
    }

    #[test]
    fn a_primordial_planet_starts_hot_without_continents_and_cools() {
        let mut sim = Sim::new(young(2));
        let start = sim.frame();
        assert_eq!(start.stats.continental, 0.0, "no continents yet");
        assert!((start.heat - 3.5).abs() < 1e-5);
        // The hot mantle floods the continents by 0.5 km per unit of heat.
        assert!((sim.flood() - 1.25).abs() < 1e-4);
        let mut heats = Vec::new();
        let mut last = start.clone();
        sim.run(400.0, |f| {
            heats.push(f.heat);
            last = f.clone();
        });
        assert!(heats.windows(2).all(|w| w[1] < w[0]), "always cooling");
        let now = *heats.last().unwrap();
        assert!(now > 1.0 && now < 3.5, "{now}");
        // Land stands that much lower against the sea; sea floor does not.
        let flood = sim.flood();
        assert!(flood > 0.0);
        for (c, &z) in last.crust.iter().zip(&last.surface) {
            match c.kind {
                Kind::Continental => assert!((z - (c.elevation - flood)).abs() < 1e-5),
                Kind::Oceanic => assert!(z >= c.elevation),
            }
        }
        let cool = Sim::new(small(2));
        assert_eq!(cool.frame().heat, 1.0, "today's Earth stays as it is");
        assert_eq!(cool.flood(), 0.0);
    }

    /// The grid point nearest `d` and its crust.
    fn crust_at(sim: &Sim, frame: &Frame, d: Vec3) -> Crust {
        let i = (0..sim.points().len())
            .min_by(|&a, &b| {
                sim.points()[a]
                    .distance(d)
                    .total_cmp(&sim.points()[b].distance(d))
            })
            .unwrap();
        frame.crust[i]
    }

    #[test]
    fn an_ocean_crater_rides_its_plate_into_the_trench_and_is_gone() {
        // Ocean diving under a continent (the hemispheres, converging on the
        // −z side). A crater on the southern sea floor, ahead of the trench.
        let rate = 0.0035;
        let mut sim = hemispheres_of(
            rate,
            |_| Crust::continental(0.5, 500.0),
            |_| Crust::oceanic(120.0),
        );
        // South plate turns +rate about x: −z moves toward +y (the trench).
        let at = Vec3::new(0.0, -0.25, -1.0).normalize();
        sim.cut(0.0, at, 0.08, 2.0);
        let frame = run_for(&mut sim, 20.0);
        assert_eq!(frame.impacts.len(), 1, "the event is reported");
        assert!(frame.impacts[0].ocean);
        // It moved with the plate: deepest now where the plate carried it.
        let carried = Quat::from_axis_angle(Vec3::X, rate * 20.0) * at;
        let floor = crust_at(&sim, &frame, carried);
        assert!(floor.raised < -1.0, "the bowl rode along: {floor:?}");
        // Next frame's list starts empty.
        let later = run_for(&mut sim, 20.0);
        assert!(later.impacts.is_empty());
        // In time the floor carrying it goes down the trench, and with it
        // every trace: the planet is then the same as a twin never struck.
        let gone = run_for(&mut sim, 160.0);
        let mut twin = hemispheres_of(
            rate,
            |_| Crust::continental(0.5, 500.0),
            |_| Crust::oceanic(120.0),
        );
        let unstruck = run_for(&mut twin, 200.0);
        assert_eq!(gone.owner, unstruck.owner);
        assert_eq!(gone.crust, unstruck.crust, "the crater was subducted");
    }

    #[test]
    fn the_crater_record_follows_the_crust_and_forgets_what_subducts() {
        // A land crater on a plate turning about y: found where the plate
        // carried it, shallower with age.
        let rate = 0.004;
        let mut sim = one_plate(rate, 0.0, |_| Crust::continental(0.5, 500.0));
        sim.cut(0.0, Vec3::X, 0.1, 2.0);
        let young = run_for(&mut sim, 20.0);
        let craters = sim.craters();
        assert_eq!(craters.len(), 1);
        let carried = Quat::from_axis_angle(Vec3::Y, rate * 20.0) * Vec3::X;
        assert!(
            craters[0].dir.angle_between(carried) < sim.spacing * 1.5,
            "found where its plate took it: {:?} vs {carried:?}",
            craters[0].dir
        );
        assert!(!craters[0].ocean);
        let fresh_depth = craters[0].depth;
        assert!(fresh_depth > 1.0, "{fresh_depth}");
        assert!(young.crust.iter().any(|c| c.scar == craters[0].impact.id));
        run_for(&mut sim, 200.0);
        let aged = sim.craters()[0].depth;
        assert!(aged < fresh_depth * 0.5, "softened: {fresh_depth} → {aged}");

        // An ocean crater carried into a trench drops off the record.
        let mut ocean = hemispheres_of(
            0.0035,
            |_| Crust::continental(0.5, 500.0),
            |_| Crust::oceanic(120.0),
        );
        ocean.cut(0.0, Vec3::new(0.0, -0.25, -1.0).normalize(), 0.08, 2.0);
        run_for(&mut ocean, 20.0);
        assert_eq!(ocean.craters().len(), 1, "still on its sea floor");
        assert!(ocean.craters()[0].ocean);
        run_for(&mut ocean, 180.0);
        assert!(ocean.craters().is_empty(), "subducted, and forgotten");
    }

    #[test]
    fn a_fold_turns_with_its_plate() {
        // Folds pushed east everywhere (y × p), on a plate turning 90° about
        // y. Each sample's crust came from a quarter-turn back, where east
        // pointed another way: carried rigidly, its fold points east here.
        let rate = 0.004;
        let mut sim = one_plate(rate, 0.0, |p| Crust {
            fold: Vec3::Y.cross(p).to_array(),
            ..Crust::continental(0.5, 500.0)
        });
        let frame = run_for(&mut sim, std::f32::consts::FRAC_PI_2 / rate);
        let mut checked = 0;
        for (p, c) in sim.points().iter().zip(&frame.crust) {
            let east = Vec3::Y.cross(*p);
            // Near the equator. Each resample hands crust to the nearest grid
            // point, a spacing or so from where it was carried; east turns
            // across that hop by its length × sin(latitude), so at higher
            // latitudes twenty resamples' hops add up to tens of degrees
            // that are the reference's, not the fold's.
            if p.y.abs() > 0.3 {
                continue;
            }
            let fold = Vec3::from(c.fold);
            assert!(
                fold.dot(*p).abs() < 1e-4,
                "flat on the sphere: {fold:?} at {p:?}"
            );
            assert!(
                fold.angle_between(east) < 0.1,
                "turned with the plate: {fold:?} vs east {east:?} at {p:?}"
            );
            checked += 1;
        }
        assert!(checked > sim.points().len() / 4);
    }

    #[test]
    fn a_small_crater_is_not_worn_away_by_resampling() {
        // Sea floor on one drifting plate: no trench to take it, only twenty
        // resamples' worth of nearest-sample copying. A crater a couple of
        // grid spacings across (a handful of samples) must come through
        // with its record and its place.
        let rate = 0.004;
        let mut sim = one_plate(rate, 0.0, |_| Crust::oceanic(60.0));
        let at = Vec3::new(1.0, 0.3, 0.2).normalize();
        let radius = sim.spacing * 1.6;
        sim.cut(0.0, at, radius, 2.0);
        run_for(&mut sim, 400.0);
        let craters = sim.craters();
        assert_eq!(craters.len(), 1, "kept through 20 resamples");
        let carried = Quat::from_axis_angle(Vec3::Y, rate * 400.0) * at;
        assert!(
            craters[0].dir.angle_between(carried) < sim.spacing,
            "where its plate took it: {:?} vs {carried:?}",
            craters[0].dir
        );
        assert!(craters[0].ocean);
    }

    #[test]
    fn a_land_crater_stays_and_wears_down() {
        let mut sim = one_plate(0.0, 0.0, |_| Crust::continental(0.5, 500.0));
        sim.cut(0.0, Vec3::X, 0.08, 2.0);
        let fresh = crust_at(&sim, &sim.frame(), Vec3::X).elevation;
        assert!(fresh < -1.0, "a bowl below the plain: {fresh}");
        let frame = run_for(&mut sim, 100.0);
        let old = crust_at(&sim, &frame, Vec3::X).elevation;
        assert!(
            old < 0.5 && old > fresh,
            "still there, softened: {fresh} → {old}"
        );
    }

    #[test]
    fn a_primordial_planet_is_bombarded_heavily_then_steadily() {
        let mut sim = Sim::new(young(8));
        let mut per_frame = Vec::new();
        sim.run(800.0, |f| per_frame.push(f.impacts.len()));
        let early: usize = per_frame[1..4].iter().sum();
        let late: usize = per_frame[per_frame.len() - 3..].iter().sum();
        assert!(early > late * 3, "heavy early: {per_frame:?}");
        assert!(late > 0, "and it never stops: {per_frame:?}");
        // The Earth-like preset has none.
        let mut cool = Sim::new(small(8));
        let mut any = 0;
        cool.run(100.0, |f| any += f.impacts.len());
        assert_eq!(any, 0);
    }

    #[test]
    fn hot_crust_holds_lower_mountains() {
        let hot = Sim::new(young(3));
        assert!((hot.mountain_limit() - MOUNTAIN_LIMIT / 3.5).abs() < 1e-5);
        let cool = Sim::new(small(3));
        assert_eq!(
            cool.mountain_limit(),
            MOUNTAIN_LIMIT,
            "today's planet: unchanged"
        );
        // The limit rises as the planet cools.
        let mut sim = Sim::new(young(3));
        let before = sim.mountain_limit();
        run_for(&mut sim, 200.0);
        assert!(sim.mountain_limit() > before);
    }

    #[test]
    fn plates_are_retired_and_slivers_merged() {
        let mut sim = Sim::new(young(5));
        let mut counts = Vec::new();
        sim.run(1000.0, |f| {
            let mut sizes = vec![0usize; f.plates.len()];
            for &o in &f.owner {
                sizes[o as usize] += 1;
            }
            assert!(sizes.iter().all(|&s| s > 0), "no empty plate kept");
            counts.push(f.plates.len());
        });
        // Rifting goes on, but the count stays bounded rather than climbing
        // for ever.
        let most = *counts.iter().max().unwrap();
        assert!(most < 24 * 4, "plate counts {counts:?}");
    }

    #[test]
    fn hot_plates_move_faster() {
        let moved = |heat: f32| {
            let params = Params {
                samples: 8000,
                plates: 1,
                rift_rate: 0.0,
                heat,
                ..Params::earth_like(1)
            };
            let mut sim = Sim::with(
                params,
                // Far enough in 20 My (~0.2 rad cool) that the grid's
                // spacing (~0.04 rad) does not swamp the comparison.
                vec![Motion {
                    axis: Vec3::Y,
                    rate: 0.01,
                }],
                |p| {
                    if p.distance(Vec3::X) < 0.4 {
                        (0, Crust::continental(0.5, 500.0))
                    } else {
                        (0, Crust::oceanic(100.0))
                    }
                },
            );
            let centre = |f: &Frame, pts: &[Vec3]| -> Vec3 {
                pts.iter()
                    .zip(&f.crust)
                    .filter(|(_, c)| c.kind == Kind::Continental)
                    .map(|(p, _)| *p)
                    .sum::<Vec3>()
                    .normalize()
            };
            let before = centre(&sim.frame(), sim.points());
            let frame = run_for(&mut sim, 20.0);
            centre(&frame, sim.points()).angle_between(before)
        };
        let (cool, hot) = (moved(1.0), moved(3.5));
        // ~3.5^0.75 ≈ 2.6× (cooling a little over the 20 My).
        assert!(hot > cool * 2.2 && hot < cool * 2.7, "{cool} vs {hot}");
    }

    #[test]
    fn a_hot_planet_breaks_up_its_sea_floor_plates_and_grows_continents() {
        let mut sim = Sim::new(young(5));
        let mut rifts = 0;
        let mut land = Vec::new();
        sim.run(600.0, |f| {
            rifts += f.stats.rifts;
            land.push(f.stats.continental);
        });
        assert!(
            rifts > 0,
            "hot oceanic plates rift though no continent does"
        );
        // Land grows out of nothing: island arcs, then their collisions.
        let last = *land.last().unwrap();
        assert!(last > 0.01, "continents emerged: {:.1}%", last * 100.0);
    }

    #[test]
    fn a_plate_carries_its_crust_round_rigidly() {
        // One plate, the whole sphere, turning about y.
        let params = Params {
            samples: 8000,
            plates: 1,
            ..Params::earth_like(1)
        };
        let rate = 0.004;
        let mut sim = Sim::with(
            params,
            vec![Motion {
                axis: Vec3::Y,
                rate,
            }],
            |p| {
                let crust = if p.distance(Vec3::X) < 0.4 {
                    Crust::continental(0.5, 500.0)
                } else {
                    Crust::oceanic(100.0)
                };
                (0, crust)
            },
        );
        let land = |f: &Frame, points: &[Vec3]| -> Vec3 {
            let sum: Vec3 = points
                .iter()
                .zip(&f.crust)
                .filter(|(_, c)| c.kind == Kind::Continental)
                .map(|(p, _)| *p)
                .sum();
            sum.normalize()
        };
        let before = land(&sim.frame(), sim.points());
        let mut frame = None;
        while frame.is_none() {
            frame = sim.step();
        }
        let frame = frame.unwrap();
        let after = land(&frame, sim.points());
        let expected = Quat::from_axis_angle(Vec3::Y, rate * 20.0) * before;
        assert!(
            after.angle_between(expected) < sim.spacing,
            "moved to {after:?}, expected {expected:?}"
        );
        assert_eq!(frame.stats.opened, 0, "one plate opens no gaps");
        assert!((frame.stats.continental - sim.frame().stats.continental).abs() < 1e-6);
    }
}
