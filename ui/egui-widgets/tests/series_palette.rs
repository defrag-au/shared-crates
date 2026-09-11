//! The categorical ramp is an **encoding**, and a theme is allowed to replace
//! it — so every theme has to re-earn the property the original ramp was chosen
//! for: adjacent series stay apart, under normal vision and under both common
//! dichromacies.
//!
//! # Why this test had to exist before the ramp could be themed
//!
//! `channel_bands::CHANNEL_PALETTE` carried its validation in a doc comment —
//! "adjacent CVD separation (worst ΔE 8.4), normal-vision floor (worst ΔE 19.3),
//! validated against surface #1a1a2e" — and a test that only checked the five
//! entries were `!=` each other. That is enough while the ramp is a `const`
//! nobody edits. It is not enough once a *theme* can supply one: `!=` passes for
//! two blues a protanope cannot tell apart, and a stacked band chart with two
//! indistinguishable bands is not a styling regression, it is a wrong chart that
//! still looks fine to the person who shipped it.
//!
//! So the validator moved out of the prose and into here, and now runs over
//! `Theme::PRESETS`. Adding a preset is what enrols it.
//!
//! The maths is ΔE76 over CIELAB with Machado et al. (2009) severity-1.0
//! dichromacy matrices — the same harness `flow_ring`'s own palette test uses,
//! restated here because integration tests cannot reach a `#[cfg(test)]` module
//! (`tests/contrast.rs` restates its luminance maths for the same reason).

use egui::Color32;
use egui_widgets::theme::Theme;

/// CIELAB of an sRGB colour — enough for ΔE76, which is what the floors are
/// stated in.
fn lab(c: Color32) -> [f64; 3] {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
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

fn delta_e(a: Color32, b: Color32) -> f64 {
    lab(a)
        .iter()
        .zip(lab(b).iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Machado et al. (2009) severity-1.0 simulation of dichromatic vision.
fn simulate(c: Color32, m: [[f64; 3]; 3]) -> Color32 {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.04045 {
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
    let (r, g, b) = (lin(c.r()), lin(c.g()), lin(c.b()));
    Color32::from_rgb(
        enc(m[0][0] * r + m[0][1] * g + m[0][2] * b),
        enc(m[1][0] * r + m[1][1] * g + m[1][2] * b),
        enc(m[2][0] * r + m[2][1] * g + m[2][2] * b),
    )
}

const PROTAN: [[f64; 3]; 3] = [
    [0.152_286, 1.052_583, -0.204_868],
    [0.114_503, 0.786_281, 0.099_216],
    [-0.003_882, -0.048_116, 1.051_998],
];
const DEUTAN: [[f64; 3]; 3] = [
    [0.367_322, 0.860_646, -0.227_968],
    [0.280_085, 0.672_501, 0.047_413],
    [-0.011_820, 0.042_940, 0.968_881],
];

/// The floors the original ramp was chosen against — see
/// `channel_bands::CHANNEL_PALETTE`'s header. Kept as the *stated* floors rather
/// than the measured ones so a new preset has room to be different without
/// having to match Tokyo Night's exact separations.
const NORMAL_FLOOR: f64 = 19.0;
const CVD_FLOOR: f64 = 8.0;

fn presets() -> Vec<Theme> {
    Theme::PRESETS.iter().map(|p| p()).collect()
}

/// # The check is on ADJACENT pairs, and that is the contract — not a weakening
///
/// The first version of this test checked every pair and failed the *shipped*
/// ramp: aqua vs magenta sit at ΔE 5.2 under deuteranopia. That is not a bug in
/// the palette, it is a bug in the test. `CHANNEL_PALETTE`'s header states the
/// contract precisely — "the checks are on *adjacent* pairs, so the order is
/// part of what passed" — because the thing a reader has to separate is two
/// bands that **touch**. Two hues three slots apart in a stacked chart never
/// share an edge, and demanding they survive dichromacy anyway is a constraint
/// no five-hue ramp on a dark surface can meet.
///
/// So: adjacency is the real requirement, and the order is load-bearing. A
/// preset that reorders its ramp has changed which pairs are adjacent and has to
/// re-earn this.
#[test]
fn every_preset_keeps_adjacent_series_distinguishable() {
    for t in presets() {
        let ramp = t.series.categorical;
        for (i, pair) in ramp.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let d = delta_e(a, b);
            assert!(
                d >= NORMAL_FLOOR,
                "`{}` series {i} vs {}: ΔE {d:.1} < {NORMAL_FLOOR} (normal vision)",
                t.name,
                i + 1
            );
            for (mode, m) in [("protan", PROTAN), ("deutan", DEUTAN)] {
                let d = delta_e(simulate(a, m), simulate(b, m));
                assert!(
                    d >= CVD_FLOOR,
                    "`{}` series {i} vs {}: ΔE {d:.1} < {CVD_FLOOR} ({mode})",
                    t.name,
                    i + 1
                );
            }
        }
    }
}

/// `unobserved` means "nobody has looked yet", and it has to be readable as
/// *not one of the series* — otherwise an unwalked region of a chart quietly
/// joins the data.
#[test]
fn unobserved_never_reads_as_a_series() {
    for t in presets() {
        for (i, s) in t.series.categorical.iter().enumerate() {
            let d = delta_e(*s, t.series.unobserved);
            assert!(
                d >= CVD_FLOOR,
                "`{}` unobserved vs series {i}: ΔE {d:.1} < {CVD_FLOOR}",
                t.name
            );
        }
    }
}

/// Direction is the encoding the most modules share (thirteen write the
/// inbound colour out by hand today). If in and out collapse, every flow view
/// in the suite says nothing.
#[test]
fn inbound_and_outbound_stay_apart_under_dichromacy() {
    for t in presets() {
        let (i, o) = (t.series.inbound(), t.series.outbound());
        assert!(
            delta_e(i, o) >= NORMAL_FLOOR,
            "`{}` in vs out: ΔE {:.1} (normal)",
            t.name,
            delta_e(i, o)
        );
        for (mode, m) in [("protan", PROTAN), ("deutan", DEUTAN)] {
            let d = delta_e(simulate(i, m), simulate(o, m));
            assert!(
                d >= CVD_FLOOR,
                "`{}` in vs out: ΔE {d:.1} < {CVD_FLOOR} ({mode})",
                t.name
            );
        }
    }
}

/// WCAG relative luminance — the thing an ordinal ramp has to be monotonic in,
/// because brightness is what a reader ranks by and what survives greyscale,
/// dichromacy and a bad projector.
fn luminance(c: Color32) -> f64 {
    let ch = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
}

/// # The invariant that `rarity_rank_color` failed for years
///
/// An ordinal ramp encodes *more or less*. If its luminance is not monotonic,
/// the encoding lies: the old rank ramp ran 0.628 → **0.475** → 0.562 → 0.525 →
/// 0.120, so the top-5% tier rendered darker than the top-10% tier below it.
/// Each of those five hues was defensible alone; as a ramp they were wrong, and
/// nothing in the tree could notice.
///
/// Generating from endpoints is what makes this structural rather than lucky —
/// but "structural" is a claim, so it gets checked, across every preset and at
/// enough sample points to catch a bow in the middle.
#[test]
fn every_preset_ordinal_ramp_is_monotonic_in_luminance() {
    for t in presets() {
        let ramp = t.series.ordinal;
        let mut prev = luminance(ramp.at(0.0));
        for i in 1..=50 {
            let cur = luminance(ramp.at(i as f32 / 50.0));
            assert!(
                cur >= prev - 1e-4,
                "`{}` ordinal ramp dips at t={:.2}: {prev:.4} -> {cur:.4}",
                t.name,
                i as f32 / 50.0
            );
            prev = cur;
        }
        // And it has to actually travel, or every rank looks the same.
        let span = luminance(ramp.high) - luminance(ramp.low);
        assert!(
            span > 0.25,
            "`{}` ordinal ramp spans only {span:.3} of luminance — ranks will not separate",
            t.name
        );
    }
}

/// A diverging scale's midpoint means "no difference". If it carries visual
/// weight, every chart using the scale gains a band of emphasis exactly where
/// the data says nothing is happening — so the neutral has to sit near the
/// surface, not halfway between the two hues.
#[test]
fn every_preset_diverging_neutral_recedes() {
    for t in presets() {
        let flow = t.series.flow;
        let surface = luminance(t.color.bg_secondary);
        let neutral = luminance(flow.neutral);
        assert!(
            (neutral - surface).abs() < 0.08,
            "`{}` flow neutral is {neutral:.3} against a {surface:.3} surface — it will read as data",
            t.name
        );
        // Both arms must stand off that neutral, or the scale has no direction.
        for (name, arm) in [("positive", flow.positive), ("negative", flow.negative)] {
            assert!(
                delta_e(arm, flow.neutral) >= NORMAL_FLOOR,
                "`{}` {name} arm is ΔE {:.1} from neutral",
                t.name,
                delta_e(arm, flow.neutral)
            );
        }
    }
}

/// Identity colours are hashed, so the theme cannot vet the hues — only the
/// envelope. Every hue on the wheel has to clear the surface, or some policies
/// are invisible depending on their name.
#[test]
fn every_preset_identity_envelope_clears_its_surface() {
    for t in presets() {
        let surface = t.color.bg_secondary;
        for h in 0..72u64 {
            let c = t.series.identity.color(h);
            let ratio = egui_widgets::theme::contrast_ratio(c, surface);
            assert!(
                ratio >= 3.0,
                "`{}` identity hue {h} is {ratio:.2}:1 on the card surface",
                t.name
            );
        }
    }
}

/// THE RING-RAMP VALIDATOR — was `flow_ring`'s own test, now run per preset.
///
/// It could not stay where it was: `ring_tint` reads the theme now, so a test
/// against module `const`s would be checking values nothing renders. Moving it
/// here is also the point — the constraint it encodes is precisely what a theme
/// is able to break, so it has to run over every preset rather than over one
/// hardcoded ramp.
///
/// The floors are unchanged, including the stricter pair. Verbatim from the
/// original: "a seat that collapses onto a chord makes a wallet look like a
/// payment, and two ring steps that collapse onto each other erase the
/// classification the whole chart encodes … the associate/customer pair carries
/// a much higher one because clearing the floor was exactly what it did while
/// still reading as one colour on 4px dots."
///
/// All pairs, not adjacent-only — unlike the categorical ramp these are dots
/// scattered on one chart, so any two can end up side by side.
#[test]
fn every_preset_keeps_classes_separable_from_each_other_and_from_the_chords() {
    const RING_NORMAL_FLOOR: f64 = 15.0;
    for t in presets() {
        let s = t.series;
        let all: Vec<(&str, Color32)> = vec![
            ("core", s.class(0)),
            ("associate", s.class(1)),
            ("customer", s.class(2)),
            ("unexamined", s.class(3)),
            ("chord-out", s.outbound()),
            ("chord-in", s.inbound()),
        ];
        for (i, (na, a)) in all.iter().enumerate() {
            for (nb, b) in all.iter().skip(i + 1) {
                let d = delta_e(*a, *b);
                assert!(
                    d >= RING_NORMAL_FLOOR,
                    "`{}` {na} vs {nb}: ΔE {d:.1} < {RING_NORMAL_FLOOR}",
                    t.name
                );
                for (kind, m) in [("protan", PROTAN), ("deutan", DEUTAN)] {
                    let d = delta_e(simulate(*a, m), simulate(*b, m));
                    assert!(
                        d >= CVD_FLOOR,
                        "`{}` {na} vs {nb} under {kind}: ΔE {d:.1} < {CVD_FLOOR}",
                        t.name
                    );
                }
            }
        }
        // The pair that prompted the original change: one teal hue in lightness
        // steps measured 22 normal / 19 deutan and still read as one colour.
        let (assoc, cust) = (s.class(1), s.class(2));
        assert!(
            delta_e(assoc, cust) >= 40.0,
            "`{}` associate vs customer must be obvious, not merely legal: ΔE {:.1}",
            t.name,
            delta_e(assoc, cust)
        );
        assert!(
            delta_e(simulate(assoc, DEUTAN), simulate(cust, DEUTAN)) >= 30.0,
            "`{}` associate vs customer under deutan: ΔE {:.1}",
            t.name,
            delta_e(simulate(assoc, DEUTAN), simulate(cust, DEUTAN))
        );
    }
}

/// `nth` folds past the end rather than wrapping. Wrapping would hand series 5
/// the same colour as series 0 while both still claim to be data — the exact
/// collision the ramp is capped at five slots to avoid.
#[test]
fn nth_folds_past_the_end_of_the_ramp() {
    let s = Theme::tokyo_night().series;
    assert_eq!(s.nth(0), s.categorical[0]);
    assert_eq!(s.nth(4), s.categorical[4]);
    assert_eq!(s.nth(5), s.other);
    assert_eq!(s.nth(99), s.other);
}

/// The folded band must not read as a series either — same reason as
/// `unobserved`, different cause.
#[test]
fn other_never_reads_as_a_series() {
    for t in presets() {
        for (i, s) in t.series.categorical.iter().enumerate() {
            let d = delta_e(*s, t.series.other);
            assert!(
                d >= CVD_FLOOR,
                "`{}` other vs series {i}: ΔE {d:.1} < {CVD_FLOOR}",
                t.name
            );
        }
    }
}
