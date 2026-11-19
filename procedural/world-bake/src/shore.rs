//! What kind of shore a stretch of coast is.
//!
//! A bake carries each shore's *suitability* for a beach and for marsh
//! ([`crate::LayerName::SHORE`]): continuous, so the layers sample and blend
//! cleanly. A reader classifies a point into a [`Shore`] with
//! [`Shore::classify`], under the params the bake was made with
//! ([`crate::WorldManifest::shore`]), and draws or simulates by kind.

use serde::{Deserialize, Serialize};

/// A smooth step from 0 at `from` to 1 at `to` (or down, if `to < from`):
/// a threshold with a soft edge, for params that read "below this, fully;
/// above that, not at all".
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ramp {
    pub from: f32,
    pub to: f32,
}

impl Ramp {
    pub const fn new(from: f32, to: f32) -> Self {
        Self { from, to }
    }

    /// 0 at `from`, 1 at `to`, smoothstepped between.
    pub fn at(self, x: f32) -> f32 {
        let t = ((x - self.from) / (self.to - self.from)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// 1 at `from`, 0 at `to`: [`Ramp::at`] falling.
    pub fn below(self, x: f32) -> f32 {
        1.0 - self.at(x)
    }
}

/// The kind of a stretch of shore.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shore {
    /// Steep ground meeting the sea: cliffs, rock platforms.
    Rocky,
    /// Sand or shingle, built by waves from sediment on gentle ground.
    Beach,
    /// Very gentle, sheltered ground the sea floods and drains.
    Marsh(Marsh),
}

/// Marsh, by climate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marsh {
    /// Mudflat and salt marsh.
    Salt,
    /// Mangrove, in the warm.
    Mangrove,
}

impl Shore {
    /// The shore at a point from its baked suitabilities (each 0..1) and
    /// its mean temperature, Kelvin, with how strongly it is that kind
    /// (0..1; 1 for a fully suited shore). A stretch suited to neither,
    /// enough to pass [`ShoreParams::suited`], is rocky.
    pub fn classify(
        beach: f32,
        marsh: f32,
        temperature_k: f32,
        params: &ShoreParams,
    ) -> (Shore, f32) {
        let (kind, suitability) = if beach >= marsh {
            (Shore::Beach, beach)
        } else if temperature_k >= params.mangrove_k {
            (Shore::Marsh(Marsh::Mangrove), marsh)
        } else {
            (Shore::Marsh(Marsh::Salt), marsh)
        };
        let strength = params.suited.at(suitability);
        if strength > 0.0 {
            (kind, strength)
        } else {
            (Shore::Rocky, 1.0 - beach.max(marsh))
        }
    }
}

/// How a bake judges its shores (`world_baker::bake_shore`). Recorded in
/// the manifest with the layers it made.
///
/// The defaults were calibrated on one tectonic world (seed 1, `tectonic-1g`)
/// toward Earth's ~31% sandy ice-free coast (Luijendijk et al. 2018): they
/// give it 37% beach, 60% rocky, ~3% marsh. The marsh share has no sourced
/// target, only "a few percent, on the flattest, calmest shores". Other
/// worlds will land elsewhere; the `bake_swell` example reports the split.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShoreParams {
    /// Texels either side of the coast the layers cover.
    pub band: u32,
    /// Texels inland whose height decides how steep a shore is: the coastal
    /// strip, ~20 km at 1 on an Earth-sized 1024 bake. Measured over the
    /// whole band, a range behind a coastal plain counted as a steep shore,
    /// and 3% of a tectonic world's coast came out beach against Earth's
    /// ~31% sandy (Luijendijk et al. 2018, "The State of the World's
    /// Beaches").
    pub strip: u32,
    /// Rise across the coastal strip, as a share of the land's relief, from
    /// a coastal plain (gentle) to ground meeting the sea steeply.
    pub steep: Ramp,
    /// Rise from ground flat enough for marsh to too steep for it.
    pub hilly: Ramp,
    /// Exposure to the swell (`SurfExposure`) from calm enough for marsh to
    /// too rough for it.
    pub rough: Ramp,
    /// A beach's suitability on a fully sheltered shore, as a share of an
    /// exposed one's: waves build beaches, but sheltered ones exist.
    pub sheltered_beach: f32,
    /// Sediment everywhere, as a share of a river mouth's: eroding coasts
    /// and longshore drift supply beaches without a river.
    pub base_sediment: f32,
    /// Quantiles of the land's water flow from no river to a river mouth's
    /// full sediment.
    pub river: Ramp,
    /// Smoothing passes over the band, each a mean with the band's
    /// 4-neighbours.
    pub smoothing: u32,
    /// Suitability from too little to be that kind of shore (rocky) to
    /// fully that kind ([`Shore::classify`]).
    pub suited: Ramp,
    /// Mean temperature, Kelvin, from which marsh is mangrove: mangroves
    /// are a tropical and subtropical fringe, roughly where the annual mean
    /// is above ~20 °C.
    pub mangrove_k: f32,
}

impl Default for ShoreParams {
    fn default() -> Self {
        Self {
            band: 3,
            strip: 1,
            steep: Ramp::new(0.025, 0.1),
            hilly: Ramp::new(0.02, 0.06),
            rough: Ramp::new(0.2, 0.4),
            sheltered_beach: 0.5,
            base_sediment: 0.8,
            river: Ramp::new(0.9, 0.99),
            smoothing: 3,
            suited: Ramp::new(0.2, 0.4),
            mangrove_k: 293.15,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ramp_rises_and_falls_between_its_ends() {
        let r = Ramp::new(0.2, 0.4);
        assert_eq!(r.at(0.1), 0.0);
        assert_eq!(r.at(0.5), 1.0);
        assert!((r.at(0.3) - 0.5).abs() < 1e-6);
        assert_eq!(r.below(0.1), 1.0);
    }

    #[test]
    fn shores_classify_by_their_strongest_suitability() {
        let p = ShoreParams::default();
        assert_eq!(Shore::classify(0.9, 0.1, 280.0, &p).0, Shore::Beach);
        assert_eq!(
            Shore::classify(0.1, 0.9, 280.0, &p).0,
            Shore::Marsh(Marsh::Salt)
        );
        assert_eq!(
            Shore::classify(0.1, 0.9, 300.0, &p).0,
            Shore::Marsh(Marsh::Mangrove)
        );
        assert_eq!(Shore::classify(0.05, 0.05, 300.0, &p).0, Shore::Rocky);
    }
}
