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
    /// The direction mountains run, as the tectonics pushed them: an
    /// unnormalised tangent vector (zero where nothing has folded the crust).
    pub fold: [f32; 3],
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
        }
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

/// Height continental interiors wear down to, km: old continents ride a few
/// hundred metres up on their thick, buoyant crust (Earth's stable cratons
/// stand ~0.3–0.5 km), not at sea level.
pub const BASE_LEVEL: f32 = 0.3;

/// Advance one sample's crust by `dt` million years: it ages; sea floor
/// follows its age down (keeping any uplift it was given, while volcanoes on
/// it subside); land above the sea wears down in proportion to its height.
pub fn age(crust: &mut Crust, dt: f32) {
    crust.age += dt;
    match crust.kind {
        Kind::Oceanic => {
            crust.volcanic *= (-dt / VOLCANO_DECAY).exp();
            crust.elevation = ocean_elevation(crust.age) + crust.raised + crust.volcanic;
        }
        Kind::Continental => {
            if crust.elevation > BASE_LEVEL {
                let above = crust.elevation - BASE_LEVEL;
                crust.elevation -= above * (1.0 - (-EROSION / PEAK * dt).exp());
            }
        }
    }
}

/// Raise (or lower) crust by `dz` km. Sea floor raised above the sea becomes
/// land: the crust of an island arc, the seed of a continent.
pub fn raise(crust: &mut Crust, dz: f32) {
    match crust.kind {
        Kind::Oceanic => {
            crust.raised += dz;
            crust.elevation = ocean_elevation(crust.age) + crust.raised + crust.volcanic;
            if crust.elevation > 0.0 {
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
        crust.elevation = ocean_elevation(crust.age) + crust.raised + crust.volcanic;
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
