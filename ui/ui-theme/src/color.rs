//! One sRGB type, one bridge per renderer, and the maths that validates a
//! palette.
//!
//! Everything here was already written somewhere in `egui-widgets` — relative
//! luminance **four times**, in two precisions, across three crates, because an
//! integration test cannot reach a `#[cfg(test)]` module. Two of those copies
//! had drifted apart on the sRGB threshold itself: `theme.rs` used `0.039_28`
//! (the older WCAG figure) while both test suites used `0.040_45` (the sRGB
//! spec), so the contrast suite was not quite measuring what the shipped
//! `ColorTokens::on` computed.
//!
//! ⚠️ **This crate uses `0.040_45` throughout.** It is the value the validators
//! already agreed on, and converging is the point of moving the maths here. The
//! difference only matters within a hair of a contrast tie.

/// Straight — **not premultiplied** — 8-bit sRGB with alpha.
///
/// The distinction is not pedantry. `egui::Color32` stores channels *already
/// multiplied* by alpha, and `Color32::from_rgba_premultiplied` reads like the
/// right constructor for "this colour, faded": pass a palette colour straight
/// in and you get an invalid colour that blends additively and comes out far
/// lighter than intended. That shipped once, in a selection wash. Keeping the
/// shared type straight means the premultiply happens in exactly one place —
/// the [`Paint`] impl — and no caller can choose wrongly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Srgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Srgb {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// From `0xRRGGBB`. For transcribing a palette that was written as hex.
    pub const fn hex(v: u32) -> Self {
        Self::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }
}

/// A renderer's colour type, as this crate needs to see it.
///
/// Two methods, because everything else — luminance, contrast, mixing,
/// dichromacy — derives from the channels. Implementations live in THIS crate
/// behind feature gates: the orphan rule forbids `egui-widgets` from writing
/// `impl Paint for Color32`, since both halves are foreign to it.
pub trait Paint: Copy {
    fn srgb(self) -> Srgb;
    fn from_srgb(c: Srgb) -> Self;
}

impl Paint for Srgb {
    fn srgb(self) -> Srgb {
        self
    }
    fn from_srgb(c: Srgb) -> Self {
        c
    }
}

#[cfg(feature = "egui")]
impl Paint for egui::Color32 {
    fn srgb(self) -> Srgb {
        let [r, g, b, a] = self.to_srgba_unmultiplied();
        Srgb { r, g, b, a }
    }
    fn from_srgb(c: Srgb) -> Self {
        // UNmultiplied: see [`Srgb`] on why the other constructor is a trap.
        egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
    }
}

#[cfg(feature = "macroquad")]
impl Paint for macroquad::color::Color {
    fn srgb(self) -> Srgb {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Srgb {
            r: q(self.r),
            g: q(self.g),
            b: q(self.b),
            a: q(self.a),
        }
    }
    fn from_srgb(c: Srgb) -> Self {
        macroquad::color::Color::new(
            c.r as f32 / 255.0,
            c.g as f32 / 255.0,
            c.b as f32 / 255.0,
            c.a as f32 / 255.0,
        )
    }
}

// ---------------------------------------------------------------------------
// sRGB transfer functions
// ---------------------------------------------------------------------------

/// sRGB channel to linear light.
pub fn to_linear(v: u8) -> f32 {
    let v = v as f32 / 255.0;
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light back to an sRGB channel.
pub fn to_srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

// ---------------------------------------------------------------------------
// WCAG
// ---------------------------------------------------------------------------

/// WCAG relative luminance of an opaque colour.
pub fn relative_luminance<P: Paint>(c: P) -> f32 {
    let c = c.srgb();
    0.2126 * to_linear(c.r) + 0.7152 * to_linear(c.g) + 0.0722 * to_linear(c.b)
}

/// WCAG contrast ratio between two opaque colours, in `1.0..=21.0`.
///
/// Public, and the single implementation, because every palette decision —
/// a legible foreground over a fill, the contrast suite, a consumer picking a
/// label colour over a chart series — must measure the same thing.
pub fn contrast_ratio<P: Paint>(a: P, b: P) -> f32 {
    let (x, y) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// `color` at `alpha` (0–255) — a scrim, a wash, a translucent band.
///
/// Takes the colour and the alpha separately, and does any premultiplication
/// inside the [`Paint`] impl, so the caller cannot reach for the wrong
/// constructor. See [`Srgb`].
pub fn with_alpha<P: Paint>(color: P, alpha: u8) -> P {
    let c = color.srgb();
    P::from_srgb(Srgb { a: alpha, ..c })
}

/// Whichever of `light` or `dark` reads more legibly **on** `fill`.
///
/// Computed, never hand-picked: a palette cannot quietly ship unreadable text
/// if the foreground is derived from the contrast ratio.
pub fn legible_on<P: Paint>(fill: P, light: P, dark: P) -> P {
    if contrast_ratio(dark, fill) >= contrast_ratio(light, fill) {
        dark
    } else {
        light
    }
}

// ---------------------------------------------------------------------------
// Oklab — perceptual mixing
// ---------------------------------------------------------------------------

/// sRGB to Oklab `[L, a, b]`.
pub fn oklab<P: Paint>(c: P) -> [f32; 3] {
    let c = c.srgb();
    let (r, g, b) = (to_linear(c.r), to_linear(c.g), to_linear(c.b));
    let l = (0.412_221_46 * r + 0.536_332_54 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_84 * g + 0.629_978_5 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Oklab `[L, a, b]` back to sRGB.
pub fn from_oklab<P: Paint>(lab: [f32; 3]) -> P {
    let [ll, aa, bb] = lab;
    let l = (ll + 0.396_337_78 * aa + 0.215_803_76 * bb).powi(3);
    let m = (ll - 0.105_561_346 * aa - 0.063_854_17 * bb).powi(3);
    let s = (ll - 0.089_484_18 * aa - 1.291_485_5 * bb).powi(3);
    P::from_srgb(Srgb::rgb(
        to_srgb(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
        to_srgb(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
        to_srgb(-0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s),
    ))
}

/// Interpolate between two colours in Oklab. `t` is clamped to `0..=1`.
///
/// Perceptual, not channel-wise: a linear sRGB blend passes through muddy
/// mid-tones, which is what made an ordinal ramp non-monotonic in luminance.
pub fn mix<P: Paint>(a: P, b: P, t: f32) -> P {
    let t = t.clamp(0.0, 1.0);
    let (x, y) = (oklab(a), oklab(b));
    from_oklab([
        x[0] + (y[0] - x[0]) * t,
        x[1] + (y[1] - x[1]) * t,
        x[2] + (y[2] - x[2]) * t,
    ])
}

// ---------------------------------------------------------------------------
// CIELAB and colour-vision deficiency
// ---------------------------------------------------------------------------

/// CIELAB of an sRGB colour — enough for ΔE76, which is what the separation
/// floors are stated in. `f64` because a floor of 8.0 is decided by tenths.
pub fn lab<P: Paint>(c: P) -> [f64; 3] {
    let c = c.srgb();
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
    // D65 white point.
    let (x, y, z) = (
        (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047,
        0.2126 * r + 0.7152 * g + 0.0722 * b,
        (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883,
    );
    let f = |t: f64| {
        if t > 0.008_856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// ΔE76 — Euclidean distance in CIELAB.
pub fn delta_e<P: Paint>(a: P, b: P) -> f64 {
    lab(a)
        .iter()
        .zip(lab(b).iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The two common dichromacies.
///
/// An enum rather than a raw matrix at the call site: a 3×3 of floats says
/// nothing about which vision it models, and these are the numbers a reviewer
/// cannot check by eye.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dichromacy {
    /// Red-blind.
    Protan,
    /// Green-blind.
    Deutan,
}

impl Dichromacy {
    /// Machado et al. (2009), severity 1.0.
    pub const fn matrix(self) -> [[f64; 3]; 3] {
        match self {
            Dichromacy::Protan => [
                [0.152_286, 1.052_583, -0.204_868],
                [0.114_503, 0.786_281, 0.099_216],
                [-0.003_882, -0.048_116, 1.051_998],
            ],
            Dichromacy::Deutan => [
                [0.367_322, 0.860_646, -0.227_968],
                [0.280_085, 0.672_501, 0.047_413],
                [-0.011_820, 0.042_940, 0.968_881],
            ],
        }
    }
}

/// How `c` appears under `vision`.
///
/// Used to prove adjacent entries of an encoding ramp stay apart: two series a
/// protanope cannot separate is not a styling regression, it is a wrong chart
/// that still looks right to whoever shipped it.
pub fn simulate<P: Paint>(c: P, vision: Dichromacy) -> P {
    let m = vision.matrix();
    let c = c.srgb();
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let enc = |v: f64| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
    P::from_srgb(Srgb::rgba(
        enc(m[0][0] * r + m[0][1] * g + m[0][2] * b),
        enc(m[1][0] * r + m[1][1] * g + m[1][2] * b),
        enc(m[2][0] * r + m[2][1] * g + m[2][2] * b),
        c.a,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Srgb = Srgb::rgb(0, 0, 0);
    const WHITE: Srgb = Srgb::rgb(255, 255, 255);
    /// Tokyo Night's primary background and text — the pair the ramp is judged
    /// against.
    const BG: Srgb = Srgb::rgb(26, 27, 38);
    const TEXT: Srgb = Srgb::rgb(192, 202, 245);

    #[test]
    fn hex_matches_the_channel_constructor() {
        assert_eq!(Srgb::hex(0x1a_1b_26), BG);
        assert_eq!(Srgb::hex(0xff_ff_ff), WHITE);
    }

    /// The WCAG range is 1..=21, and its endpoints are the check that the
    /// transfer function is right — a wrong threshold shifts this off 21.
    #[test]
    fn black_on_white_is_the_full_range() {
        assert!((contrast_ratio(BLACK, WHITE) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(WHITE, WHITE) - 1.0).abs() < 0.001);
    }

    #[test]
    fn contrast_does_not_care_which_way_round_it_is_asked() {
        assert_eq!(contrast_ratio(BG, TEXT), contrast_ratio(TEXT, BG));
    }

    /// The shipped text ramp must clear WCAG AA for small text on its own
    /// background. This is the floor the whole palette is built to.
    #[test]
    fn the_shipped_text_clears_aa_on_its_background() {
        assert!(
            contrast_ratio(TEXT, BG) >= 4.5,
            "got {}",
            contrast_ratio(TEXT, BG)
        );
    }

    #[test]
    fn a_wash_keeps_its_colour_and_takes_the_alpha() {
        let washed = with_alpha(TEXT, 40);
        assert_eq!(washed.a, 40);
        assert_eq!((washed.r, washed.g, washed.b), (TEXT.r, TEXT.g, TEXT.b));
    }

    /// Legibility is DERIVED. On a near-black fill the dark option must lose,
    /// however much a designer likes it.
    #[test]
    fn legible_on_picks_by_measurement_not_preference() {
        assert_eq!(legible_on(BG, TEXT, BLACK), TEXT);
        assert_eq!(legible_on(WHITE, TEXT, BLACK), BLACK);
    }

    #[test]
    fn a_mix_reaches_both_of_its_ends() {
        assert_eq!(mix(BG, TEXT, 0.0), BG);
        assert_eq!(mix(BG, TEXT, 1.0), TEXT);
        // And is clamped, so a caller's stray `t` cannot leave the ramp.
        assert_eq!(mix(BG, TEXT, -1.0), BG);
        assert_eq!(mix(BG, TEXT, 9.0), TEXT);
    }

    /// Oklab mixing is monotonic in luminance — the property a channel-wise
    /// blend loses, and the reason an ordinal ramp could read "darker" for a
    /// higher rank.
    #[test]
    fn an_ordinal_ramp_never_goes_backwards() {
        let steps: Vec<f32> = (0..=10)
            .map(|i| relative_luminance(mix(BG, TEXT, i as f32 / 10.0)))
            .collect();
        for pair in steps.windows(2) {
            assert!(pair[1] >= pair[0], "{steps:?} dips");
        }
    }

    #[test]
    fn a_colour_is_zero_distance_from_itself() {
        assert!(delta_e(TEXT, TEXT) < 1e-9);
    }

    /// Grey has no red/green information to lose, so both dichromacies leave it
    /// where it is. If this drifts, the matrices were transcribed wrongly.
    #[test]
    fn grey_survives_both_dichromacies() {
        let grey = Srgb::rgb(128, 128, 128);
        for vision in [Dichromacy::Protan, Dichromacy::Deutan] {
            assert!(
                delta_e(simulate(grey, vision), grey) < 2.0,
                "{vision:?} moved grey"
            );
        }
    }

    /// The finding that sized the categorical ramp at five: red and green
    /// collapse toward each other under protanopia. The ramp's own floor is
    /// ΔE 8, so a pair this close is exactly what the validator must catch.
    #[test]
    fn red_and_green_collapse_under_protanopia() {
        let red = Srgb::rgb(0xd9, 0x59, 0x26);
        let green = Srgb::rgb(0x19, 0x9e, 0x70);
        let apart = delta_e(red, green);
        let collapsed = delta_e(
            simulate(red, Dichromacy::Protan),
            simulate(green, Dichromacy::Protan),
        );
        assert!(apart > collapsed, "normal {apart}, protan {collapsed}");
    }

    /// Round-tripping must not drift, or a ramp regenerated each frame would
    /// shimmer. Ported from `egui-widgets`' encoding tests when the Oklab
    /// helpers moved here.
    #[test]
    fn oklab_round_trips_within_a_bit() {
        for c in [
            Srgb::hex(0x39_87_e5),
            Srgb::rgb(255, 215, 0),
            Srgb::rgb(12, 13, 16),
            WHITE,
        ] {
            let back: Srgb = from_oklab(oklab(c));
            for (a, b) in [(c.r, back.r), (c.g, back.g), (c.b, back.b)] {
                assert!(a.abs_diff(b) <= 1, "{c:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn srgb_round_trips_through_linear() {
        for v in [0u8, 1, 27, 128, 200, 255] {
            assert_eq!(to_srgb(to_linear(v)), v);
        }
    }
}
