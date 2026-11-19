//! What the crust is at a point, and how it changes with time on its own.

/// Oceanic crust is thin and dense, made at ridges and lost at trenches;
/// continental crust is thick and light, and does not sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Oceanic,
    Continental,
}

/// What last built mountains in continental crust (Cortial et al.'s
/// orogeny type): subduction beneath it (Andean, like the Andes) or a
/// continent colliding with it (Himalayan).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orogeny {
    None,
    Andean,
    Himalayan,
}

/// The crust at one sample point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crust {
    pub kind: Kind,
    /// Surface elevation, km; 0 is sea level.
    pub elevation: f32,
    /// Oceanic: millions of years since it formed at a ridge. Continental:
    /// millions of years since its last mountain building.
    pub age: f32,
    /// Oceanic: tectonic uplift (or a trench's sinking) on top of the depth
    /// its age gives, km, so ageing does not erase it.
    pub raised: f32,
    /// Oceanic: a volcano's pile on top of that, km. It subsides and wears
    /// away with time (see [`VOLCANO_DECAY`]): a hotspot's islands sink as
    /// their plate carries them off, into seamounts.
    pub volcanic: f32,
    pub orogeny: Orogeny,
    /// The direction the tectonics pushed this crust while building its
    /// mountains (convergence for an Andean margin, away from the suture for a
    /// collision): an unnormalised tangent vector, zero where nothing has
    /// folded the crust. Ranges and their folds run across it.
    pub fold: [f32; 3],
    /// Continental: a crater's relief on this ground, km (the bowl negative,
    /// the rim positive), part of `elevation` but kept apart from it so
    /// erosion does not treat a crater ring like a mountain range: it fades
    /// on its own clock ([`CRATER_DECAY`]) while the ground under it wears.
    pub crater: f32,
    /// The impact whose crater's heart this crust holds ([`crate::Impact`]'s
    /// `id`), or 0. It travels with the crust: where it ends up is where the
    /// crater is now, and when the last of it is subducted the crater is gone.
    pub scar: u32,
}

impl Crust {
    /// New sea floor `age` million years old.
    pub fn oceanic(age: f32) -> Self {
        Self {
            kind: Kind::Oceanic,
            elevation: ocean_elevation(age),
            age,
            raised: 0.0,
            volcanic: 0.0,
            orogeny: Orogeny::None,
            fold: [0.0; 3],
            crater: 0.0,
            scar: 0,
        }
    }

    /// Sea floor's elevation from its parts: the depth its age gives, any
    /// tectonic uplift, and any volcano on it.
    pub fn sea_floor(&self) -> f32 {
        ocean_elevation(self.age) + self.raised + self.volcanic
    }

    /// Continental crust at `elevation` km, `age` million years since it
    /// last built mountains.
    pub fn continental(elevation: f32, age: f32) -> Self {
        Self {
            kind: Kind::Continental,
            elevation,
            age,
            raised: 0.0,
            volcanic: 0.0,
            orogeny: Orogeny::None,
            fold: [0.0; 3],
            crater: 0.0,
            scar: 0,
        }
    }
}

/// e-folding time of a volcanic island's height above the sea floor, My:
/// the Hawaiian islands subside and wear below the waves in a few tens of
/// millions of years, becoming the Emperor seamounts.
pub const VOLCANO_DECAY: f32 = 25.0;

/// Sea floor elevation (km, negative) for crust `age` million years old:
/// the age–depth relation measured on Earth's oceans (Parsons & Sclater
/// 1977). New crust is hot and stands high at the ridge (2.5 km deep); it
/// sinks as it cools, with the square root of age at first, then levelling
/// off toward ~6.4 km on old abyssal plains.
///
/// The two forms cross near 30 My; taking the shallower of them at every age
/// joins them without a step (switching at a fixed age leaves one: at 70 My
/// the sea floor would jump 60 m up).
pub fn ocean_elevation(age: f32) -> f32 {
    let age = age.max(0.0);
    let young = 2.5 + 0.35 * age.sqrt();
    let old = 6.4 - 3.2 * (-age / 62.8).exp();
    -young.min(old)
}

/// Highest continental elevation the model expects, km: erosion is scaled
/// against it, so high ground wears fastest.
pub const PEAK: f32 = 10.0;

/// Continental erosion, km per million years per [`PEAK`] of height above
/// [`BASE_LEVEL`]: land wears toward its base level with an e-folding time
/// of `PEAK / EROSION` = 40 My.
///
/// Cortial et al. use 0.03 (3 × 10⁻² mm/y at their 10 km peak, ~330 My to
/// e-fold), but on its own that left continents almost wholly 2–5 km
/// highland after a few hundred My of mountain building. Active ranges wear
/// at ~0.1–1 km/My on Earth; at 0.25, a range under the strongest
/// subduction uplift (~0.14 km/My) settles near 5–6 km, and one no longer
/// rising falls to ~1 km within ~70 My.
pub const EROSION: f32 = 0.25;

/// e-folding time, My, of sediment filling a continental basin below the sea
/// (a crater's bowl): slower than a range wears down, since a basin hundreds
/// of km across fills from what the land around it sheds. At the erosion
/// rate (40 My) a 2 km crater was a flat plain in ~150 My.
pub const INFILL: f32 = 150.0;

/// Height continental interiors wear down to, km: old continents ride a few
/// hundred metres up on their thick, buoyant crust (Earth's stable cratons
/// stand ~0.3–0.5 km), not at sea level.
pub const BASE_LEVEL: f32 = 0.3;

/// Advance one sample's crust by `dt` million years: it ages; sea floor
/// follows its age down (keeping any uplift it was given, while volcanoes on
/// it subside); land above the sea wears down in proportion to its height;
/// continental ground below the sea (a crater's bowl) fills back up toward
/// it with sediment, at the same rate.
pub fn age(crust: &mut Crust, dt: f32) {
    crust.age += dt;
    match crust.kind {
        Kind::Oceanic => {
            crust.volcanic *= (-dt / VOLCANO_DECAY).exp();
            crust.elevation = crust.sea_floor();
        }
        Kind::Continental => {
            // The ground under any crater wears (or fills); the crater's own
            // relief fades on its own clock.
            let mut ground = crust.elevation - crust.crater;
            if ground > BASE_LEVEL {
                let wear = 1.0 - (-EROSION / PEAK * dt).exp();
                ground -= (ground - BASE_LEVEL) * wear;
            } else if ground < 0.0 {
                let fill = 1.0 - (-dt / INFILL).exp();
                ground -= ground * fill;
            }
            crust.crater *= (-dt / CRATER_DECAY).exp();
            crust.elevation = ground + crust.crater;
        }
    }
}

/// e-folding time of a land crater's relief, My: a 200 My old crater keeps
/// about half its depth, a billion-year-old one is all but gone, as on Earth
/// (Manicouagan, 214 My, is still a clear ring; craters of a few Gyr survive
/// only as eroded roots, like Vredefort).
pub const CRATER_DECAY: f32 = 250.0;

/// Cut `dz` km of crater relief into crust: into continental ground as its
/// own relief ([`Crust::crater`]), into sea floor as tectonic relief that it
/// keeps until it subducts. Sea floor raised past `threshold` (a rim on a
/// shallow sea) becomes land, as with [`raise_past`].
pub fn scar(crust: &mut Crust, dz: f32, threshold: f32) {
    match crust.kind {
        Kind::Continental => {
            crust.crater += dz;
            crust.elevation += dz;
        }
        Kind::Oceanic => raise_past(crust, dz, threshold),
    }
}

/// Raise (or lower) crust by `dz` km. Sea floor raised above the sea becomes
/// land: the crust of an island arc, the seed of a continent.
pub fn raise(crust: &mut Crust, dz: f32) {
    raise_past(crust, dz, 0.0);
}

/// [`raise`], with sea floor becoming continent only once it stands
/// `threshold` km above today's sea level: on a hot planet whose sea has
/// risen ([`crate::Sim::flood`]), an arc stays a chain of volcanic islands
/// until it has built past the risen sea.
pub fn raise_past(crust: &mut Crust, dz: f32, threshold: f32) {
    match crust.kind {
        Kind::Oceanic => {
            crust.raised += dz;
            crust.elevation = crust.sea_floor();
            if crust.elevation > threshold {
                crust.kind = Kind::Continental;
                crust.age = 0.0;
                crust.raised = 0.0;
                crust.volcanic = 0.0;
            }
        }
        Kind::Continental => crust.elevation += dz,
    }
}

/// Pile a volcano `dz` km higher on sea floor, up to `cap` km above it. A
/// hotspot's volcano stays oceanic crust even when it breaks the surface: an
/// island that will subside, not new continent.
pub fn erupt(crust: &mut Crust, dz: f32, cap: f32) {
    if crust.kind == Kind::Oceanic {
        crust.volcanic = (crust.volcanic + dz).min(cap);
        crust.elevation = crust.sea_floor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sea_floor_starts_high_and_sinks_with_age() {
        assert!((ocean_elevation(0.0) + 2.5).abs() < 1e-6, "ridge at 2.5 km");
        let mut last = ocean_elevation(0.0);
        for a in 1..200 {
            let z = ocean_elevation(a as f32);
            assert!(z < last + 1e-4, "never rises: {a} My");
            last = z;
        }
        assert!(ocean_elevation(200.0) > -6.5 && ocean_elevation(200.0) < -6.0);
        // The two branches meet near 70 My.
        assert!((ocean_elevation(69.99) - ocean_elevation(70.0)).abs() < 0.1);
    }

    #[test]
    fn land_wears_down_and_sea_floor_ages() {
        // Mountains wear noticeably in 2 My (~0.23 km at 5 km), not away.
        let mut land = Crust::continental(5.0, 0.0);
        age(&mut land, 2.0);
        assert!(
            land.elevation < 4.85 && land.elevation > 4.6,
            "{}",
            land.elevation
        );
        // Over 40 My the height above base level falls to 1/e.
        let mut range = Crust::continental(BASE_LEVEL + 4.0, 0.0);
        for _ in 0..20 {
            age(&mut range, 2.0);
        }
        let left = range.elevation - BASE_LEVEL;
        assert!((left - 4.0 / std::f32::consts::E).abs() < 0.01, "{left}");
        // Old lowland rests at base level, not sea level.
        let mut plain = Crust::continental(0.6, 0.0);
        for _ in 0..500 {
            age(&mut plain, 2.0);
        }
        assert!((plain.elevation - BASE_LEVEL).abs() < 0.01);
        let mut sea = Crust::oceanic(0.0);
        age(&mut sea, 10.0);
        assert_eq!(sea.age, 10.0);
        assert_eq!(sea.elevation, ocean_elevation(10.0));
    }

    #[test]
    fn uplift_on_sea_floor_survives_ageing_and_can_make_land() {
        let mut sea = Crust::oceanic(20.0);
        raise(&mut sea, 1.0);
        age(&mut sea, 10.0);
        assert!((sea.elevation - (ocean_elevation(30.0) + 1.0)).abs() < 1e-5);
        // Raised past the surface: an island arc's new land.
        raise(&mut sea, 4.0);
        assert_eq!(sea.kind, Kind::Continental);
        assert!(sea.elevation > 0.0);
        assert_eq!(sea.age, 0.0);
    }

    #[test]
    fn a_crater_on_eroding_highland_keeps_its_relief_on_its_own_clock() {
        // Highland 3 km up; a 1.5 km bowl cut into it, and the same ground
        // beside it with no crater.
        let mut bowl = Crust::continental(3.0, 0.0);
        let mut beside = Crust::continental(3.0, 0.0);
        scar(&mut bowl, -1.5, 0.0);
        for _ in 0..50 {
            age(&mut bowl, 2.0);
            age(&mut beside, 2.0);
        }
        // 100 My on: the ground has worn most of the way to base level, but
        // the bowl still sits below its surroundings by e^(−100/250) of its
        // depth.
        assert!(beside.elevation < 1.0, "{}", beside.elevation);
        let relief = beside.elevation - bowl.elevation;
        let expected = 1.5 * (-100.0f32 / CRATER_DECAY).exp();
        assert!((relief - expected).abs() < 1e-3, "{relief} vs {expected}");
        // Sea floor takes a crater as tectonic relief instead.
        let mut floor = Crust::oceanic(50.0);
        scar(&mut floor, -1.0, 0.0);
        assert_eq!(floor.raised, -1.0);
        assert_eq!(floor.crater, 0.0);
    }

    #[test]
    fn a_hotspot_island_stays_oceanic_and_subsides() {
        let mut sea = Crust::oceanic(40.0);
        erupt(&mut sea, 20.0, 8.0);
        assert_eq!(sea.volcanic, 8.0, "capped");
        assert!(sea.elevation > 0.0, "it broke the surface");
        assert_eq!(sea.kind, Kind::Oceanic, "an island, not a continent");
        let fresh = sea.elevation;
        age(&mut sea, VOLCANO_DECAY);
        assert!(
            sea.elevation < fresh - 2.5,
            "it sank: {fresh} → {}",
            sea.elevation
        );
        assert!((sea.volcanic - 8.0 / std::f32::consts::E).abs() < 1e-4);
    }
}
