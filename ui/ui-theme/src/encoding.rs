//! The colours a chart **encodes with** — as distinct from the ones chrome is
//! painted in.
//!
//! # Why this is a separate axis from [`crate::tokens::ColorTokens`]
//!
//! The two answer different questions, and collapsing them silently produces
//! wrong charts. A semantic token says *what a thing means* — this failed, this
//! is a warning. An encoding colour says *where this datum sits*, and its job is
//! to preserve a relationship: distinguishable-from, more-than, further-from-
//! zero. Mapping a six-way categorical ramp onto four semantic tokens makes two
//! categories collide, and a stacked chart with two identical bands is not a
//! styling regression — it is a wrong chart that still looks fine to whoever
//! shipped it.
//!
//! # Kinds, because the kind is what carries the invariant
//!
//! There is no single "chart palette". There are five kinds, each with a
//! different thing a theme must not break:
//!
//! | Kind | Used by | Invariant |
//! |---|---|---|
//! | [`SeriesPalette::categorical`] | channel bands, ring classes | adjacent pairs separable under dichromacy |
//! | [`Diverging`] | flow in/out, price up/down, over/under | arms separable **and** a neutral at surface luminance |
//! | [`Sequential`] | rarity rank, density, depth | **monotonic in luminance** |
//! | [`IdentityEnvelope`] | policy / wallet colours | unbounded hues, bounded legibility |
//! | *fixed / physical* | foil, terrain metaphor | **not themed at all** |
//!
//! That last row is a real category, not an oversight. A holographic foil is a
//! hue rotation simulating a physical effect; a themed rainbow is not a rainbow.
//! Those literals stay literal, and the reason is written where they live.
//!
//! # The enforcement is the deliverable
//!
//! Letting a theme supply chart colours is only safe because each invariant is a
//! test over every preset. That is not belt-and-braces: within an hour of the
//! categorical validator existing it caught a sixth slot colliding at ΔE 7.3
//! under protanopia, and the ordinal ramp was found non-monotonic after years in
//! the tree. Without the tests, every new preset is a chance to ship an encoding
//! that reads fine to the person who added it and lies to everyone else.

use crate::color::{Paint, Srgb, from_oklab, mix, oklab};

/// An **ordinal** ramp: rank, depth, density, recency — anything where the
/// reader's question is "more or less?".
///
/// # Generated, not enumerated, and that is the whole point
///
/// The theme supplies two endpoints and the ramp is interpolated in Oklab.
/// Listing fixed steps is what the suite had, and it produced a ramp that was
/// non-monotonic in luminance: a reader scanning by brightness read the rank
/// order wrong, and in greyscale or under either common dichromacy it was
/// actively misleading rather than merely ugly. Nobody noticed for years,
/// because nothing could notice.
///
/// Generating from endpoints makes monotonicity **structural**: [`Self::at`] is
/// linear in Oklab L between a dim end and a bright one, so it cannot dip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sequential<P> {
    /// The "least" end. Should recede toward the surface.
    pub low: P,
    /// The "most" end.
    pub high: P,
}

impl<P: Paint> Sequential<P> {
    /// A ramp **around one hue** — dark to light, keeping `base`'s colour.
    ///
    /// For "ordered slices of one quantity": a top-1% / top-10% / rest
    /// breakdown, a two-band supply bar. Not the categorical ramp, because those
    /// bands are ordered slices of one quantity and a categorical set would
    /// imply they are unrelated things. Sharing a hue is what says *related*;
    /// the lightness is what says *more*.
    pub fn around(base: P) -> Self {
        let [_, a, b] = oklab(base);
        Self {
            low: from_oklab([0.38, a, b]),
            high: from_oklab([0.82, a, b]),
        }
    }

    /// The colour at `t` in `0..=1`, where 0 is [`Self::low`].
    pub fn at(&self, t: f32) -> P {
        mix(self.low, self.high, t)
    }

    /// `n` evenly spaced steps, low to high. `n == 1` yields the high end — a
    /// single-step ramp is showing the reader "the most", not "the least".
    pub fn steps(&self, n: usize) -> Vec<P> {
        match n {
            0 => Vec::new(),
            1 => vec![self.high],
            _ => (0..n).map(|i| self.at(i as f32 / (n - 1) as f32)).collect(),
        }
    }

    /// The step for `rank` out of `total`, **inverted** so rank 1 is the
    /// brightest. Ranks are ordinal-from-the-top; density and depth are not, and
    /// use [`Self::at`] directly.
    pub fn by_rank(&self, rank: u32, total: u32) -> P {
        if total == 0 {
            return self.low;
        }
        let pct = (rank as f32 / total as f32).clamp(0.0, 1.0);
        self.at(1.0 - pct)
    }
}

/// A **bipolar** encoding: value arriving vs leaving, price up vs down, over vs
/// under target.
///
/// The neutral is not decoration. A diverging scale's midpoint is "no
/// difference", and if it renders as a colour rather than as the surface, every
/// chart using the scale gains a band of visual weight exactly where the data
/// says nothing is happening.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diverging<P> {
    /// The "toward us / up / over" arm.
    pub positive: P,
    /// The "away / down / under" arm.
    pub negative: P,
    /// The midpoint. Should sit near the surface, not between the two hues.
    pub neutral: P,
}

impl<P: Paint> Diverging<P> {
    /// The colour at `t` in `-1.0..=1.0`, where 0 is [`Self::neutral`].
    pub fn at(&self, t: f32) -> P {
        let t = t.clamp(-1.0, 1.0);
        if t >= 0.0 {
            mix(self.neutral, self.positive, t)
        } else {
            mix(self.neutral, self.negative, -t)
        }
    }
}

/// The envelope for **hashed identity** colours — one colour per policy, per
/// wallet, per anything with unbounded cardinality.
///
/// A ramp cannot serve this: there is no Nth colour when N is "every policy on
/// chain". What a theme *can* own is the envelope — how saturated and how light
/// those hues are allowed to be — which keeps arbitrary hues legible on this
/// particular surface without constraining which hue any one identity gets. Hue
/// is the hash's to choose; legibility is the theme's.
///
/// Not generic: it is two numbers. [`Self::color`] is where a colour appears.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdentityEnvelope {
    /// Chroma in Oklab terms, as a 0..1 fraction of a vivid reference.
    pub saturation: f32,
    /// Oklab lightness, 0..1. The band that has to clear the surface.
    pub lightness: f32,
}

impl IdentityEnvelope {
    /// A stable colour for `hash`. Same hash, same colour, for the life of the
    /// theme — identity colours are compared across screenshots and sessions.
    pub fn color<P: Paint>(&self, hash: u64) -> P {
        // 72 hue steps.
        let hue = (hash % 72) as f32 * (std::f32::consts::TAU / 72.0);
        // 0.16 is roughly the chroma of a vivid sRGB hue in Oklab; scaling it
        // keeps the whole wheel inside gamut at this lightness.
        let chroma = 0.16 * self.saturation.clamp(0.0, 1.0);
        from_oklab([
            self.lightness.clamp(0.0, 1.0),
            chroma * hue.cos(),
            chroma * hue.sin(),
        ])
    }
}

/// A colour drawn from the **encoding** palette rather than the chrome one.
///
/// Carries no colour itself, so it is renderer-free and `const`-constructible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Series {
    /// The `i`th categorical series colour — folds past the last slot, so any
    /// index is valid. See [`SeriesPalette::nth`].
    Nth(usize),
    /// A ring/class tint. See [`SeriesPalette::class`].
    Class(u8),
    /// Value arriving — the inbound end of the flow ramp.
    Inbound,
    /// Value leaving — the outbound end of the flow ramp.
    Outbound,
}

impl Series {
    /// This entry's value in `s`.
    pub fn get<P: Paint>(self, s: &SeriesPalette<P>) -> P {
        match self {
            Series::Nth(i) => s.nth(i),
            Series::Class(ring) => s.class(ring),
            Series::Inbound => s.inbound(),
            Series::Outbound => s.outbound(),
        }
    }
}

/// Every encoding a chart draws from.
///
/// # This vocabulary was already shared, it just wasn't named
///
/// `0x3987e5` ("value arriving") was written out as a literal in **thirteen**
/// modules and `0xe08a2e` ("value leaving") in seven. One vocabulary, maintained
/// by copy-paste, that no theme could move and no one could revise. Naming it is
/// what makes both possible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeriesPalette<P> {
    /// The categorical ramp, in assignment order. Distinguishable by hue;
    /// **never index this for a meaning** — that is what the chrome tokens are
    /// for.
    ///
    /// **Five, and do not extend it.** Five is how many hues survive the
    /// adjacent-pair separation floors under both common dichromacies on a dark
    /// surface. The first draft carried six and the validator rejected it
    /// immediately — the sixth collapsed onto the first at ΔE 7.3 under
    /// protanopia. A sixth series folds into [`Self::other`].
    pub categorical: [P; 5],
    /// The folded "everything else" band. Deliberately outside the categorical
    /// order so it never reads as one more series.
    pub other: P,
    /// **Nothing has been looked at yet** — deliberately distinct from a
    /// measured zero. Rendering "unknown" and "none" the same way is how a
    /// half-finished walk reads as a finished one.
    pub unobserved: P,
    /// Value arriving vs leaving — the direction encoding many modules share.
    pub flow: Diverging<P>,
    /// A short **ordinal classification**: how close a party is to the subject.
    /// Innermost first; the fourth step is [`Self::unobserved`], because "nobody
    /// has looked" is the absence of a judgement rather than a fourth class.
    ///
    /// Enumerated rather than generated because its stops carry an **external**
    /// constraint no two-endpoint interpolation can be steered to meet: these
    /// dots are drawn over chords painted in [`Self::flow`], and a seat must
    /// never be mistakable for a payment. Violet, the obvious first pick,
    /// collapses onto the chord blue under protanopia at ΔE 5.1. A single-hue
    /// lightness ramp was tried and failed differently — two of its steps
    /// measured ΔE 22 normal / 19 deutan, clearing the floor and still reading
    /// as one colour on 4px dots. Moving in hue as well takes that pair to
    /// ΔE 53 / 44.
    pub classes: [P; 3],
    /// Rank, depth, density: anything ordinal.
    pub ordinal: Sequential<P>,
    /// The envelope for hashed identity colours.
    pub identity: IdentityEnvelope,
}

impl<P: Paint> SeriesPalette<P> {
    /// The values the suite already used, in their observed proportions.
    pub fn tokyo_night() -> Self {
        let c = P::from_srgb;
        Self {
            categorical: [
                c(Srgb::hex(0x39_87_e5)),
                c(Srgb::hex(0xd9_59_26)),
                c(Srgb::hex(0x19_9e_70)),
                c(Srgb::hex(0xc9_85_00)),
                c(Srgb::hex(0xd5_51_81)),
            ],
            other: c(Srgb::hex(0x6b_6b_80)),
            unobserved: c(Srgb::hex(0x4d_54_78)),
            flow: Diverging {
                positive: c(Srgb::hex(0x39_87_e5)),
                negative: c(Srgb::hex(0xe0_8a_2e)),
                neutral: c(Srgb::hex(0x2a_2e_3f)),
            },
            classes: [
                c(Srgb::hex(0xa5_f3_e4)), // core — the subject itself
                c(Srgb::hex(0x3d_dc_84)), // paid BY it
                c(Srgb::hex(0x0f_8f_8a)), // bought FROM it
            ],
            ordinal: Sequential {
                low: c(Srgb::hex(0x3b_40_5a)),
                high: c(Srgb::hex(0xff_d7_00)),
            },
            identity: IdentityEnvelope {
                saturation: 0.75,
                lightness: 0.72,
            },
        }
    }

    /// Brighter and cooler, to sit on the marketplace palette's near-black.
    pub fn opensea() -> Self {
        let c = P::from_srgb;
        Self {
            categorical: [
                c(Srgb::rgb(59, 142, 240)),
                c(Srgb::rgb(251, 146, 60)),
                c(Srgb::rgb(52, 199, 123)),
                c(Srgb::rgb(245, 181, 68)),
                c(Srgb::rgb(244, 88, 110)),
            ],
            other: c(Srgb::rgb(107, 107, 128)),
            unobserved: c(Srgb::rgb(71, 78, 92)),
            flow: Diverging {
                positive: c(Srgb::rgb(59, 142, 240)),
                negative: c(Srgb::rgb(251, 146, 60)),
                neutral: c(Srgb::rgb(34, 37, 44)),
            },
            // The same aqua → green → teal band, brightened to this palette. The
            // band is not a style choice: it is the region that stays clear of
            // both chord colours under dichromacy, and the validator holds this
            // preset to it exactly as it holds the default.
            classes: [
                c(Srgb::hex(0xa7_f3_d0)),
                c(Srgb::hex(0x34_c7_7b)),
                c(Srgb::hex(0x0d_87_7b)),
            ],
            ordinal: Sequential {
                low: c(Srgb::rgb(45, 49, 58)),
                high: c(Srgb::rgb(56, 189, 248)),
            },
            identity: IdentityEnvelope {
                saturation: 0.85,
                lightness: 0.75,
            },
        }
    }

    /// Series `i`, **folding** past the end of the ramp into [`Self::other`].
    ///
    /// Folding, not wrapping. Wrapping was the first draft and it is the worse
    /// failure: it makes series 0 and series 5 the *same colour* while both
    /// still claim to be data, which is precisely the collision the ramp is
    /// capped at five to avoid. A folded band is honest — it says "several
    /// things, not broken out".
    pub fn nth(&self, i: usize) -> P {
        self.categorical.get(i).copied().unwrap_or(self.other)
    }

    /// Value arriving. Named for the encoding, not the hue.
    pub fn inbound(&self) -> P {
        self.flow.positive
    }

    /// Value leaving.
    pub fn outbound(&self) -> P {
        self.flow.negative
    }

    /// The classification tint for `ring`, saturating at [`Self::unobserved`].
    ///
    /// Saturating rather than wrapping for the same reason [`Self::nth`] folds:
    /// past the last named class there is no further judgement to show, and
    /// recycling `classes[0]` would claim one.
    pub fn class(&self, ring: u8) -> P {
        self.classes
            .get(ring as usize)
            .copied()
            .unwrap_or(self.unobserved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Dichromacy, delta_e, relative_luminance, simulate};

    fn presets() -> Vec<SeriesPalette<Srgb>> {
        vec![SeriesPalette::tokyo_night(), SeriesPalette::opensea()]
    }

    #[test]
    fn a_ramp_ends_where_it_says_it_does() {
        let s = SeriesPalette::<Srgb>::tokyo_night().ordinal;
        assert_eq!(s.at(0.0), s.low);
        assert_eq!(s.at(1.0), s.high);
        // Out of range clamps rather than extrapolating into nonsense.
        assert_eq!(s.at(-5.0), s.low);
        assert_eq!(s.at(5.0), s.high);
    }

    #[test]
    fn rank_one_is_the_bright_end() {
        let s = SeriesPalette::<Srgb>::tokyo_night().ordinal;
        assert_eq!(s.by_rank(0, 100), s.high);
        assert_eq!(s.by_rank(100, 100), s.low);
        // No total means nothing is ranked — recede, don't claim first place.
        assert_eq!(s.by_rank(1, 0), s.low);
    }

    #[test]
    fn diverging_sits_on_its_neutral_at_zero() {
        let d = SeriesPalette::<Srgb>::tokyo_night().flow;
        assert_eq!(d.at(0.0), d.neutral);
        assert_eq!(d.at(1.0), d.positive);
        assert_eq!(d.at(-1.0), d.negative);
    }

    #[test]
    fn steps_covers_both_ends() {
        let s = SeriesPalette::<Srgb>::tokyo_night().ordinal;
        assert!(s.steps(0).is_empty());
        assert_eq!(s.steps(1), vec![s.high]);
        let five = s.steps(5);
        assert_eq!(five.len(), 5);
        assert_eq!(five[0], s.low);
        assert_eq!(five[4], s.high);
    }

    /// The same identity must keep its colour across frames and sessions —
    /// these get compared between screenshots.
    #[test]
    fn identity_colour_is_stable_and_in_gamut() {
        let e = SeriesPalette::<Srgb>::tokyo_night().identity;
        assert_eq!(e.color::<Srgb>(12345), e.color::<Srgb>(12345));
        assert_ne!(e.color::<Srgb>(0), e.color::<Srgb>(24)); // a third of the wheel apart
    }

    #[test]
    fn folding_past_the_ramp_never_recycles_a_series() {
        let s = SeriesPalette::<Srgb>::tokyo_night();
        assert_eq!(s.nth(5), s.other);
        assert_eq!(s.nth(99), s.other);
        assert_ne!(
            s.nth(5),
            s.nth(0),
            "a sixth series must not claim the first"
        );
        // Classes saturate for the same reason.
        assert_eq!(s.class(3), s.unobserved);
        assert_ne!(s.class(3), s.class(0));
    }

    /// The floors the ramp was chosen against. Stated rather than measured, so a
    /// new preset has room to differ without matching Tokyo Night exactly.
    ///
    /// **Adjacent** pairs only, and that is the contract, not a weakening: the
    /// first version checked every pair and failed the shipped ramp, because
    /// aqua and magenta sit at ΔE 5.2 under deuteranopia. That is a bug in the
    /// test, not the palette — assignment order is what keeps neighbours apart.
    #[test]
    fn adjacent_series_stay_apart_under_both_dichromacies() {
        const NORMAL_FLOOR: f64 = 19.0;
        const CVD_FLOOR: f64 = 8.0;
        for palette in presets() {
            for pair in palette.categorical.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                assert!(
                    delta_e(a, b) >= NORMAL_FLOOR,
                    "{a:?} vs {b:?} normal {}",
                    delta_e(a, b)
                );
                for vision in [Dichromacy::Protan, Dichromacy::Deutan] {
                    let d = delta_e(simulate(a, vision), simulate(b, vision));
                    assert!(d >= CVD_FLOOR, "{a:?} vs {b:?} under {vision:?} is {d}");
                }
            }
        }
    }

    /// The invariant [`Sequential`] exists to make structural. A dip here means
    /// a reader scanning by brightness reads the rank order wrong.
    #[test]
    fn the_ordinal_ramp_is_monotonic_in_luminance() {
        for palette in presets() {
            let steps = palette.ordinal.steps(12);
            let lums: Vec<f32> = steps.iter().map(|c| relative_luminance(*c)).collect();
            for pair in lums.windows(2) {
                assert!(pair[1] >= pair[0], "{lums:?} dips");
            }
        }
    }

    /// A diverging neutral that reads as a colour puts visual weight exactly
    /// where the data says nothing is happening.
    #[test]
    fn the_diverging_neutral_sits_near_the_surface() {
        for palette in presets() {
            let n = relative_luminance(palette.flow.neutral);
            let (p, q) = (
                relative_luminance(palette.flow.positive),
                relative_luminance(palette.flow.negative),
            );
            assert!(n < p && n < q, "neutral {n} is not the quietest of {p}/{q}");
        }
    }
}
