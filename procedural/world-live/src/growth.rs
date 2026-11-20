//! What the season does to the ground at a moment: how much snow lies, and
//! how far the plants are in leaf. Both from the moment's temperature and
//! rain ([`crate::seasons`]), so they follow the year with its lag.
//!
//! A first rule, judged by eye: snow lies (and melts) with the day's
//! temperature, with no memory of the winter before it; leaves need both
//! warmth and the season's rain, except on evergreens.

/// The rules. Every number is a starting value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrowthModel {
    /// °C from which leaves start to come out, and at which they are full:
    /// growing temperatures for most temperate plants.
    pub leaf_warmth: (f32, f32),
    /// The moment's rain as a share of the place's annual mean: below the
    /// first the plants are dry-season brown, above the second fully green.
    pub leaf_water: (f32, f32),
    /// °C at which snow starts to lie, and at which it covers fully.
    pub snow_cold: (f32, f32),
    /// Annual rain (the precipitation layer's relative units) below which
    /// snow lies thin: a cold desert has frost, not a snowfield.
    pub snow_rain: f32,
}

impl Default for GrowthModel {
    fn default() -> Self {
        GrowthModel {
            leaf_warmth: (2.0, 12.0),
            leaf_water: (0.3, 0.8),
            snow_cold: (1.0, -4.0),
            snow_rain: 0.08,
        }
    }
}

impl GrowthModel {
    /// How far into leaf the plants are, 0 (bare or dry-season brown) to 1
    /// (full summer green). `evergreen` is the share of the plants that keep
    /// their leaves all year (conifers, rainforest), 0 to 1.
    pub fn leaf(&self, kelvin: f32, rain_now: f32, rain_annual: f32, evergreen: f32) -> f32 {
        let warm = smoothstep(self.leaf_warmth.0, self.leaf_warmth.1, kelvin - 273.15);
        let share = if rain_annual > 1e-6 {
            rain_now / rain_annual
        } else {
            1.0
        };
        let wet = smoothstep(self.leaf_water.0, self.leaf_water.1, share);
        let evergreen = evergreen.clamp(0.0, 1.0);
        evergreen + (1.0 - evergreen) * warm * wet
    }

    /// How much of the ground snow covers, 0 to 1.
    pub fn snow(&self, kelvin: f32, rain_annual: f32) -> f32 {
        let cold = smoothstep(self.snow_cold.0, self.snow_cold.1, kelvin - 273.15);
        cold * self.snow_fed(rain_annual)
    }

    /// How well fed a place's snow is, 0.25 to 1, from its annual rain: the
    /// part of [`GrowthModel::snow`] that does not change with the moment,
    /// for a renderer that stores it and works out the cold itself.
    pub fn snow_fed(&self, rain_annual: f32) -> f32 {
        (rain_annual / self.snow_rain).clamp(0.25, 1.0)
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: f32 = 273.15;

    #[test]
    fn leaves_need_warmth_and_rain_but_evergreens_keep_theirs() {
        let model = GrowthModel::default();
        assert_eq!(model.leaf(C + 20.0, 1.0, 1.0, 0.0), 1.0);
        assert_eq!(model.leaf(C - 5.0, 1.0, 1.0, 0.0), 0.0);
        // A savanna's dry season: warm, but a tenth of its mean rain.
        assert_eq!(model.leaf(C + 25.0, 0.1, 1.0, 0.0), 0.0);
        assert_eq!(model.leaf(C - 5.0, 0.1, 1.0, 1.0), 1.0);
        let mixed = model.leaf(C - 5.0, 1.0, 1.0, 0.4);
        assert!((mixed - 0.4).abs() < 1e-6);
    }

    #[test]
    fn snow_lies_below_freezing_and_thin_where_little_falls() {
        let model = GrowthModel::default();
        assert_eq!(model.snow(C + 5.0, 0.5), 0.0);
        assert_eq!(model.snow(C - 10.0, 0.5), 1.0);
        let dry = model.snow(C - 10.0, 0.01);
        assert!(dry > 0.0 && dry < 0.5, "a cold desert's frost: {dry}");
    }
}
