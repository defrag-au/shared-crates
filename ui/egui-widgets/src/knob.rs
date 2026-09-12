//! `Knob` — a rotary control, as a prototype to riff on.
//!
//! **Status: exploratory.** Four faces and an adjustable feel, so the choice can
//! be made by using it rather than by arguing about it. Nothing depends on this
//! yet; the shape is expected to move.
//!
//! ## What it is competing with
//!
//! `egui::DragValue` — a filled box you drag horizontally. Three things make it
//! feel worse than it should for a parameter you are tuning by ear or by eye:
//!
//! - **The box is the heaviest thing in the row**, so a column of them reads as
//!   a column of boxes rather than as a set of values. Same complaint that
//!   produced [`SliderGroup`](crate::slider_group).
//! - **It shows no range.** `0.4` tells you nothing about how much room is left
//!   above it, so tuning means dragging until it looks right, releasing, and
//!   reading — instead of aiming.
//! - **Horizontal drag fights the row it sits in** and has no natural gain: the
//!   distance-to-change ratio comes from egui's `speed`, which is a per-call
//!   number nobody can pick correctly for every range.
//!
//! A knob answers all three: the sweep IS the range, the pointer IS the
//! position, and vertical travel is the one convention every mixing desk and
//! plugin already shares.
//!
//! ## The feel, which is the part worth riffing on
//!
//! - **Vertical drag, not circular.** Tracking a circle with a mouse is
//!   miserable; every audio tool that tried it went back to up/down.
//! - **[`Knob::travel`] is the whole gain story**: the pixels of drag that span
//!   the full range. One number, in the unit the hand actually works in, rather
//!   than a per-range `speed`.
//! - **Shift is fine mode** at [`FINE`] of the gain.
//! - **Double-click resets** to [`Knob::default_value`], so exploring is
//!   reversible without undo.
//! - **Scroll works while hovered**, for the nudge case.

use std::ops::RangeInclusive;

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::theme::{Density, Ink, Space, SpaceExt, TextSize, ThemeExt, Token};

/// Knob diameter at [`Density::Comfortable`], in px.
const BASE_DIAMETER: f32 = 44.0;

/// Drag pixels that span the full range, before [`Knob::travel`] overrides it.
///
/// 160 is roughly a comfortable forearm movement without re-gripping, and it
/// means the whole range is reachable in one gesture on a laptop trackpad.
const BASE_TRAVEL: f32 = 160.0;

/// Gain multiplier while shift is held.
pub const FINE: f32 = 0.2;

/// Where the sweep starts and ends, measured in turns from straight up.
///
/// `±0.375` is 270° of sweep with a 90° gap at the bottom — the gap is what
/// makes the ends readable at a glance, because an unbroken circle has no
/// visible start.
const SWEEP: f32 = 0.375;

/// How the knob draws itself. All four read the same value; they differ in what
/// they make easy to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobFace {
    /// An open arc with a filled sweep and a pointer. Shows position and
    /// remaining range equally well.
    Arc,
    /// A filled disc with a pointer. Reads as a physical control; the position
    /// is the pointer alone, so it is the least precise to read.
    Dial,
    /// A thick ring filled to the value, no pointer. The most legible at small
    /// sizes, because the lit length survives when a pointer is one pixel.
    Ring,
    /// Discrete ticks lit up to the value. For a parameter with steps, or where
    /// the reader needs to count rather than estimate.
    Ticks,
}

impl KnobFace {
    pub const ALL: [Self; 4] = [Self::Arc, Self::Dial, Self::Ring, Self::Ticks];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Arc => "arc",
            Self::Dial => "dial",
            Self::Ring => "ring",
            Self::Ticks => "ticks",
        }
    }
}

/// How big the knob is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnobSize {
    /// [`BASE_DIAMETER`] scaled by the theme's density.
    FromDensity,
    /// An exact diameter in px, ignoring density.
    Fixed(f32),
}

impl KnobSize {
    fn resolve(self, ui: &Ui) -> f32 {
        match self {
            Self::FromDensity => {
                let d = crate::theme::ThemeExt::tokens(ui).density;
                (BASE_DIAMETER * density_multiplier(d)).clamp(20.0, 160.0)
            }
            Self::Fixed(px) => px.clamp(20.0, 160.0),
        }
    }
}

fn density_multiplier(d: Density) -> f32 {
    d.multiplier()
}

/// Where the number goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readout {
    /// Under the knob. Always legible, costs a row of height.
    Below,
    /// Inside the knob. Only works on [`KnobFace::Ring`] and [`KnobFace::Arc`],
    /// which have a hole; on the others it fights the pointer.
    Centre,
    /// No number — for a bank where a shared readout sits elsewhere.
    None,
}

#[derive(Debug, Clone, Default)]
pub struct KnobResponse {
    /// The value moved this frame.
    pub changed: bool,
    /// The value after this frame's interaction.
    pub value: f32,
    /// The knob is being dragged right now — for a host that wants to show a
    /// bigger readout, or suppress an expensive live preview.
    pub dragging: bool,
}

pub struct Knob<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    face: KnobFace,
    size: KnobSize,
    readout: Readout,
    label: Option<&'a str>,
    suffix: &'a str,
    decimals: Option<usize>,
    travel: Option<f32>,
    default_value: Option<f32>,
    tint: Ink,
    ticks: usize,
}

impl<'a> Knob<'a> {
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            face: KnobFace::Arc,
            size: KnobSize::FromDensity,
            readout: Readout::Below,
            label: None,
            suffix: "",
            decimals: None,
            travel: None,
            default_value: None,
            tint: Ink::Token(Token::Accent),
            ticks: 11,
        }
    }

    pub fn face(mut self, face: KnobFace) -> Self {
        self.face = face;
        self
    }

    pub fn size(mut self, size: KnobSize) -> Self {
        self.size = size;
        self
    }

    pub fn readout(mut self, readout: Readout) -> Self {
        self.readout = readout;
        self
    }

    /// A caption under the knob. A bank of unlabelled knobs is a puzzle.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    pub fn decimals(mut self, n: usize) -> Self {
        self.decimals = Some(n);
        self
    }

    /// Drag pixels that span the full range. Lower = twitchier.
    pub fn travel(mut self, px: f32) -> Self {
        self.travel = Some(px);
        self
    }

    /// The value a double-click returns to.
    pub fn default_value(mut self, v: f32) -> Self {
        self.default_value = Some(v);
        self
    }

    pub fn tint(mut self, tint: Ink) -> Self {
        self.tint = tint;
        self
    }

    /// How many ticks [`KnobFace::Ticks`] draws.
    pub fn ticks(mut self, n: usize) -> Self {
        self.ticks = n.max(2);
        self
    }

    /// Decimals derived from the range when not set — the same rule
    /// [`SliderGroup`](crate::slider_group) uses, so a knob and a fader showing
    /// the same parameter print the same number.
    fn auto_decimals(&self) -> usize {
        let span = (*self.range.end() - *self.range.start()).abs();
        match span {
            s if s >= 100.0 => 0,
            s if s >= 10.0 => 1,
            _ => 2,
        }
    }

    pub fn show(self, ui: &mut Ui) -> KnobResponse {
        // Before the destructure, or the borrow is gone and the rule ends up
        // written out a second time here — which is how the widget and its own
        // test drift apart.
        let auto = self.auto_decimals();
        let Self {
            value,
            range,
            face,
            size,
            readout,
            label,
            suffix,
            decimals,
            travel,
            default_value,
            tint,
            ticks,
        } = self;

        let decimals = decimals.unwrap_or(auto);
        let diameter = size.resolve(ui);
        let travel = travel.unwrap_or(BASE_TRAVEL);
        let gap = ui.space(Space::Xs);
        let line_h = ui.text_size(TextSize::Sm) + 2.0;

        let caption_rows =
            usize::from(label.is_some()) + usize::from(matches!(readout, Readout::Below));
        let total = vec2(diameter, diameter + caption_rows as f32 * (line_h + gap));

        let (rect, resp) = ui.allocate_exact_size(total, Sense::click_and_drag());
        let dial = egui::Rect::from_min_size(rect.min, Vec2::splat(diameter));

        let span = *range.end() - *range.start();
        let mut out = KnobResponse {
            changed: false,
            value: *value,
            dragging: resp.dragged(),
        };

        // ── Interaction ─────────────────────────────────────────────────────
        if resp.dragged() {
            // UP increases. Screen y grows downward, hence the negation — the
            // one place this has to be said, so it is said once.
            let dy = -resp.drag_delta().y;
            let fine = ui.input(|i| i.modifiers.shift_only());
            let next = apply_drag(*value, dy, travel, span, fine);
            if next != *value {
                *value = next.clamp(*range.start(), *range.end());
                out.changed = true;
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let fine = ui.input(|i| i.modifiers.shift_only());
                let next = apply_drag(*value, scroll, travel, span, fine)
                    .clamp(*range.start(), *range.end());
                if next != *value {
                    *value = next;
                    out.changed = true;
                }
            }
        }
        if resp.double_clicked()
            && let Some(d) = default_value
        {
            let d = d.clamp(*range.start(), *range.end());
            if d != *value {
                *value = d;
                out.changed = true;
            }
        }
        out.value = *value;

        // ── Paint ───────────────────────────────────────────────────────────
        if ui.is_rect_visible(rect) {
            let colors = ui.tokens().color;
            let accent = tint.of(ui);
            let track = colors.border;
            let t = match span.abs() > f32::EPSILON {
                true => ((*value - *range.start()) / span).clamp(0.0, 1.0),
                // A zero-width range has no position to show. Parking the
                // pointer at the start is a claim; parking it centred is not.
                false => 0.5,
            };
            paint_face(ui, dial, face, t, accent, track, ticks, colors.bg_highlight);

            let text = format!("{:.decimals$}{suffix}", *value);
            let mut y = dial.bottom() + gap;
            if matches!(readout, Readout::Below) {
                text_centred(ui, pos2(dial.center().x, y), &text, colors.text_primary);
                y += line_h + gap;
            }
            if matches!(readout, Readout::Centre) {
                text_centred_on(ui, dial.center(), &text, colors.text_primary);
            }
            if let Some(l) = label {
                text_centred(ui, pos2(dial.center().x, y), l, colors.text_muted);
            }
        }

        if let Some(l) = label {
            resp.on_hover_text(format!(
                "{l} — drag up/down, shift for fine, double-click to reset"
            ));
        }
        out
    }
}

/// The value after `dy` pixels of vertical drag.
///
/// Pure, and public to the tests, because the feel IS this function — if the
/// gain is wrong, no amount of painting fixes it.
fn apply_drag(value: f32, dy: f32, travel: f32, span: f32, fine: bool) -> f32 {
    let gain = match fine {
        true => FINE,
        false => 1.0,
    };
    // A zero travel would divide by zero and send the knob to an end on the
    // first pixel; it is a caller error, so it is clamped rather than panicking.
    value + dy / travel.max(1.0) * span * gain
}

/// Turns-from-straight-up for a normalised value.
fn angle_for(t: f32) -> f32 {
    -SWEEP + t.clamp(0.0, 1.0) * SWEEP * 2.0
}

/// A point on the dial at `turns` from straight up.
fn point_on(center: Pos2, radius: f32, turns: f32) -> Pos2 {
    let a = turns * std::f32::consts::TAU;
    pos2(center.x + radius * a.sin(), center.y - radius * a.cos())
}

/// Points along the sweep from `t0` to `t1`, dense enough not to look faceted.
fn arc_points(center: Pos2, radius: f32, t0: f32, t1: f32) -> Vec<Pos2> {
    let steps = 48;
    (0..=steps)
        .map(|i| {
            let t = t0 + (t1 - t0) * (i as f32 / steps as f32);
            point_on(center, radius, angle_for(t))
        })
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "a paint routine's parameters are its inputs; bundling them into a struct would be one more indirection to read through"
)]
fn paint_face(
    ui: &Ui,
    dial: egui::Rect,
    face: KnobFace,
    t: f32,
    accent: Color32,
    track: Color32,
    ticks: usize,
    fill: Color32,
) {
    let p = ui.painter();
    let c = dial.center();
    let r = dial.width() * 0.5;

    match face {
        KnobFace::Arc => {
            let ar = r * 0.82;
            let w = (r * 0.16).max(2.0);
            p.add(egui::Shape::line(
                arc_points(c, ar, 0.0, 1.0),
                Stroke::new(w, track),
            ));
            if t > 0.0 {
                p.add(egui::Shape::line(
                    arc_points(c, ar, 0.0, t),
                    Stroke::new(w, accent),
                ));
            }
            let a = angle_for(t);
            p.line_segment(
                [point_on(c, r * 0.30, a), point_on(c, r * 0.62, a)],
                Stroke::new((r * 0.12).max(1.5), accent),
            );
        }
        KnobFace::Dial => {
            p.circle_filled(c, r * 0.80, fill);
            p.circle_stroke(c, r * 0.80, Stroke::new(1.0_f32, track));
            let a = angle_for(t);
            p.line_segment(
                [point_on(c, r * 0.24, a), point_on(c, r * 0.70, a)],
                Stroke::new((r * 0.14).max(2.0), accent),
            );
        }
        KnobFace::Ring => {
            let rr = r * 0.78;
            let w = (r * 0.30).max(3.0);
            p.add(egui::Shape::line(
                arc_points(c, rr, 0.0, 1.0),
                Stroke::new(w, track),
            ));
            if t > 0.0 {
                p.add(egui::Shape::line(
                    arc_points(c, rr, 0.0, t),
                    Stroke::new(w, accent),
                ));
            }
        }
        KnobFace::Ticks => {
            let lit = (t * (ticks - 1) as f32).round() as usize;
            for i in 0..ticks {
                let tt = i as f32 / (ticks - 1) as f32;
                let a = angle_for(tt);
                let color = match i <= lit {
                    true => accent,
                    false => track,
                };
                p.line_segment(
                    [point_on(c, r * 0.62, a), point_on(c, r * 0.90, a)],
                    Stroke::new((r * 0.10).max(1.5), color),
                );
            }
            let a = angle_for(t);
            p.line_segment(
                [point_on(c, r * 0.18, a), point_on(c, r * 0.50, a)],
                Stroke::new((r * 0.12).max(1.5), accent),
            );
        }
    }
}

/// Text centred horizontally, with its top at `top`.
fn text_centred(ui: &Ui, top: Pos2, text: &str, color: Color32) {
    let size = ui.text_size(TextSize::Sm);
    ui.painter().text(
        top,
        egui::Align2::CENTER_TOP,
        text,
        egui::FontId::proportional(size),
        color,
    );
}

/// Text centred on a point, both axes.
fn text_centred_on(ui: &Ui, at: Pos2, text: &str, color: Color32) {
    let size = ui.text_size(TextSize::Sm);
    ui.painter().text(
        at,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(size),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_increases_and_down_decreases() {
        // The one convention a knob cannot get wrong. Screen y grows downward,
        // so this is exactly the sign that is easy to invert by accident.
        let up = apply_drag(0.5, 10.0, 100.0, 1.0, false);
        let down = apply_drag(0.5, -10.0, 100.0, 1.0, false);
        assert!(up > 0.5, "dragging up must increase: {up}");
        assert!(down < 0.5, "dragging down must decrease: {down}");
    }

    #[test]
    fn travel_is_the_pixels_that_span_the_whole_range() {
        // The promise `travel` makes, in the unit the hand works in: drag that
        // many pixels and you have covered the range, whatever the range is.
        for span in [1.0_f32, 20.0, 1000.0] {
            let moved = apply_drag(0.0, 160.0, 160.0, span, false);
            assert!(
                (moved - span).abs() < 1e-3,
                "span {span} should be covered by 160px, got {moved}"
            );
        }
    }

    #[test]
    fn fine_mode_scales_the_gain_and_nothing_else() {
        let coarse = apply_drag(0.0, 50.0, 160.0, 1.0, false);
        let fine = apply_drag(0.0, 50.0, 160.0, 1.0, true);
        assert!((fine - coarse * FINE).abs() < 1e-6, "{fine} vs {coarse}");
    }

    #[test]
    fn a_zero_travel_does_not_divide_by_zero() {
        // A caller passing 0 is wrong, but a knob that returns NaN corrupts the
        // host's value and every readout downstream of it.
        let v = apply_drag(0.5, 10.0, 0.0, 1.0, false);
        assert!(v.is_finite(), "got {v}");
    }

    #[test]
    fn the_sweep_is_270_degrees_with_the_gap_at_the_bottom() {
        // An unbroken circle has no visible start, so the ends are unreadable.
        let start = angle_for(0.0);
        let end = angle_for(1.0);
        assert!((start - -0.375).abs() < 1e-6);
        assert!((end - 0.375).abs() < 1e-6);
        assert!((end - start - 0.75).abs() < 1e-6, "270° of sweep");
        // Straight up is the midpoint, which is what makes a centred default
        // (pan, balance, trim) read as centred.
        assert!(angle_for(0.5).abs() < 1e-6);
    }

    #[test]
    fn the_sweep_runs_clockwise_from_lower_left() {
        let c = pos2(0.0, 0.0);
        let lo = point_on(c, 10.0, angle_for(0.0));
        let hi = point_on(c, 10.0, angle_for(1.0));
        let top = point_on(c, 10.0, angle_for(0.5));
        assert!(lo.x < 0.0 && lo.y > 0.0, "minimum sits lower-left: {lo:?}");
        assert!(hi.x > 0.0 && hi.y > 0.0, "maximum sits lower-right: {hi:?}");
        assert!(top.y < 0.0 && top.x.abs() < 1e-5, "midpoint is up: {top:?}");
    }

    #[test]
    fn angles_are_clamped_so_an_out_of_range_value_cannot_draw_past_the_ends() {
        // A host can hand us a value outside the range; the pointer must not
        // swing into the gap and read as a smaller value than it is.
        assert!((angle_for(-3.0) - angle_for(0.0)).abs() < 1e-6);
        assert!((angle_for(7.0) - angle_for(1.0)).abs() < 1e-6);
    }

    #[test]
    fn decimals_come_from_the_range_like_the_faders_do() {
        // A knob and a `SliderGroup` fader showing the same parameter must print
        // the same number, or the two controls disagree on screen.
        let mut v = 0.0_f32;
        assert_eq!(Knob::new(&mut v, 0.0..=1.0).auto_decimals(), 2);
        assert_eq!(Knob::new(&mut v, 0.0..=20.0).auto_decimals(), 1);
        assert_eq!(Knob::new(&mut v, 0.0..=2000.0).auto_decimals(), 0);
    }

    #[test]
    fn every_face_is_reachable_and_named() {
        assert_eq!(KnobFace::ALL.len(), 4);
        let mut seen: Vec<&str> = KnobFace::ALL.iter().map(|f| f.label()).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 4, "two faces share a label");
    }
}
