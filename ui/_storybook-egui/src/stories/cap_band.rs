//! `CapBand` story — the gap is the story, and so is where the gap closes.
//!
//! Fixtures are shaped from a real Cardano token's first six months: a
//! launchpad mint, a bonding curve that graduated into two AMM pools, and a
//! long decline. Values are in ADA.
//!
//! **The thing to look at:** the yellow line is what everybody quotes and the
//! teal is what the float would actually fetch. They are never close. At the
//! peak the notional reads ~150k while the realisable band sits near 20k —
//! roughly 13% — because the supply is far larger than the pools that would
//! have to absorb it. A chart showing only the yellow line makes an argument
//! it cannot support.
//!
//! Second read: for the first third the band is a **line**, not a band. That is
//! not a rendering shortcut — it means the low and high realisable figures are
//! equal, because at that point every holder was identified and there was no
//! uncertainty to draw. The band only opens once supply lands in contracts
//! nobody has named. Widening uncertainty is a finding, not noise.
//!
//! Third read (the reason the guard exists): that flat stretch is four
//! collinear points per quad. Drawn as a polygon the tessellator cannot derive
//! a normal and throws long diagonal rays across the panel — it shipped that
//! way once and was only caught by looking at it.

use egui_widgets::{cap_band::honesty_ratio, CapBand, CapSample, SpineState, TimeSpine};

const DAY: i64 = 86_400;
const T0: i64 = 1_771_128_940; // the real mint time, so the ruler reads sensibly
const DAYS: i64 = 190;

pub struct CapBandState {
    spine: Option<SpineState>,
    samples: Vec<CapSample>,
    /// Toggles the uncertain band off, so the degenerate-quad path is
    /// reviewable on demand rather than only in the first third.
    collapse_band: bool,
    /// On by default, because the fixture is launch-shaped and linear makes it
    /// unreadable — which is the comparison worth having in front of you.
    log_y: bool,
}

impl Default for CapBandState {
    fn default() -> Self {
        Self {
            spine: None,
            samples: fixture(false),
            collapse_band: false,
            log_y: true,
        }
    }
}

/// Peak of the fixture, in ADA. The series is normalised to hit exactly this
/// so the prose above cannot drift from what renders — the first cut claimed a
/// 150k peak and drew 479k, which is the sort of thing a story exists to catch.
const PEAK: f64 = 150_000.0;

/// A launch: a violent first month, graduation into pools, then decay.
fn fixture(collapsed: bool) -> Vec<CapSample> {
    let shape: Vec<f64> = (0..DAYS)
        .map(|d| {
            let t = d as f64;
            let spike = if t < 30.0 {
                1.0 + 3.2 * (t * 0.9).sin().abs() * (1.0 - t / 40.0).max(0.0)
            } else {
                1.0
            };
            spike / (1.0 + t * 0.12)
        })
        .collect();
    let scale = PEAK / shape.iter().cloned().fold(0.0f64, f64::max);

    (0..DAYS)
        .map(|d| {
            let t = d as f64;
            let notional = shape[d as usize] * scale;
            // Liquidity is a small fraction of the headline and gets relatively
            // better as the pools deepen.
            let low = notional * (0.11 + 0.10 * (t / DAYS as f64));
            // Unidentified supply appears only once the curve graduates.
            let spread = if collapsed || d < 60 {
                0.0
            } else {
                low * 0.06 * ((d - 60) as f64 / 130.0)
            };
            CapSample {
                at: T0 + d * DAY,
                notional,
                low,
                high: low + spread,
            }
        })
        .collect()
}

pub fn show(ui: &mut egui::Ui, state: &mut CapBandState) {
    let spine = state
        .spine
        .get_or_insert_with(|| SpineState::new((T0, T0 + (DAYS - 1) * DAY)));

    ui.horizontal(|ui| {
        if ui
            .checkbox(
                &mut state.collapse_band,
                "collapse the uncertainty (exercise the zero-height guard everywhere)",
            )
            .changed()
        {
            state.samples = fixture(state.collapse_band);
        }
        ui.separator();
        ui.checkbox(&mut state.log_y, "log y");
    });
    ui.add_space(6.0);

    let sr = TimeSpine::new(spine).height(34.0).show_play(true).show(ui);
    ui.add_space(4.0);

    let resp = CapBand::new(&state.samples, &sr.scale)
        .playhead(spine.playhead)
        .height(180.0)
        .labels("notional cap", "realisable — what the float would fetch")
        .format_value(&|v| format!("{v:.0} ADA"))
        .log_y(state.log_y)
        .show(ui);

    ui.add_space(8.0);
    // Read out whatever is under the pointer, else the playhead — the number
    // the band exists to communicate, not left for the eye to estimate.
    let at = resp
        .hovered
        .or_else(|| nearest_to(&state.samples, spine.playhead));
    match at {
        Some(s) => {
            let (lo, hi) = honesty_ratio(&s);
            ui.horizontal(|ui| {
                ui.label(format!("notional {:.0} ADA", s.notional));
                ui.separator();
                ui.label(format!("realisable {:.0} .. {:.0} ADA", s.low, s.high));
                ui.separator();
                ui.colored_label(
                    crate::TEXT_MUTED,
                    format!("honesty ratio {lo:.1}% .. {hi:.1}%"),
                );
            });
        }
        None => {
            ui.colored_label(crate::TEXT_MUTED, "no sample at the playhead");
        }
    }
}

fn nearest_to(samples: &[CapSample], at: i64) -> Option<CapSample> {
    samples.iter().min_by_key(|s| (s.at - at).abs()).copied()
}
