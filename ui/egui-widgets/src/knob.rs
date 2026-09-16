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
//!
//! ## Under a finger
//!
//! A touch screen is not a mouse with a worse pointer; three of the four points
//! above have no finger equivalent, and the first one actively fights the page.
//! See [`KnobTouch`] for the gesture routing, and note what changes:
//!
//! | | mouse | finger |
//! |---|---|---|
//! | take the drag | on movement | after a [`HOLD`](crate::touch::HOLD) rest, by default |
//! | fine mode | shift | pull sideways, continuously |
//! | throw | [`BASE_TRAVEL`] | × [`TOUCH_TRAVEL`], i.e. **longer** |
//! | wheel nudge | yes | no — that gesture is the page |
//!
//! The throw is the counter-intuitive one. The instinct is that a small screen
//! wants a small gesture; the truth is that the device with the least precision
//! and no modifier key needs *more* pixels per unit, not fewer.
//!
//! Reset stays on double-click/double-tap rather than moving to a long press,
//! because the long press is now how the knob is taken hold of.

use std::ops::RangeInclusive;

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::theme::{Density, Ink, InkExt, Space, SpaceExt, TextSize, ThemeExt, Token};

/// Knob diameter at [`Density::Comfortable`], in px.
const BASE_DIAMETER: f32 = 44.0;

/// Drag pixels that span the full range, before [`Knob::travel`] overrides it.
///
/// 160 is roughly a comfortable forearm movement without re-gripping, and it
/// means the whole range is reachable in one gesture on a laptop trackpad.
const BASE_TRAVEL: f32 = 160.0;

/// Gain multiplier at full fine mode — shift on a mouse, fully pulled away on a
/// touch screen.
pub const FINE: f32 = 0.2;

/// Travel is multiplied by this under touch.
///
/// More pixels per unit, not fewer. A finger is less precise than a mouse and
/// has no modifier key to fall back on, so the device that can least afford a
/// twitchy control is exactly the one the mouse-tuned default makes twitchiest.
const TOUCH_TRAVEL: f32 = 1.6;

/// Horizontal distance from the press at which touch fine mode reaches [`FINE`].
const PULL_FULL: f32 = 120.0;

/// How a knob behaves under a finger. See [`crate::touch::Grab`], which the
/// fader bank shares — the question is the same for both, so the vocabulary is.
pub use crate::touch::Grab as KnobTouch;

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
    /// A touch gesture has taken hold of this knob. Distinct from `dragging`:
    /// the finger is engaged but may not have moved yet.
    pub engaged: bool,
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
    touch: KnobTouch,
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
            touch: KnobTouch::default(),
        }
    }

    /// How this knob behaves under a finger. See [`KnobTouch`].
    pub fn touch(mut self, touch: KnobTouch) -> Self {
        self.touch = touch;
        self
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
            touch,
        } = self;

        let decimals = decimals.unwrap_or(auto);
        let diameter = size.resolve(ui);
        // A finger gets a longer throw than a mouse, unless the caller has
        // named an exact travel — in which case they meant it.
        // Sticky, and it has to be — see `touch::is_touch`. The knob picks its
        // SENSE from this, and a sense chosen one frame late is a sense that was
        // wrong for the gesture that needed it.
        let on_touch = crate::touch::is_touch(ui);
        let travel = travel.unwrap_or(match on_touch {
            true => BASE_TRAVEL * TOUCH_TRAVEL,
            false => BASE_TRAVEL,
        });
        let gap = ui.space(Space::Xs);
        let line_h = ui.text_size(TextSize::Sm) + 2.0;

        let caption_rows =
            usize::from(label.is_some()) + usize::from(matches!(readout, Readout::Below));
        let total = vec2(diameter, diameter + caption_rows as f32 * (line_h + gap));

        // The policy only binds a finger. A mouse scrolls with the wheel, so
        // nothing is contesting its drag.
        let gate = match on_touch {
            true => touch,
            false => KnobTouch::Direct,
        };
        // `next_auto_id` is the id `allocate_exact_size` is about to assign, and
        // reading it does not advance the counter — which is what lets the sense
        // be chosen from state stored under that same id.
        let id = ui.next_auto_id().with("knob-engaged");
        let engaged = match gate {
            KnobTouch::Direct => true,
            KnobTouch::HoldToEngage => crate::touch::engaged(ui, id),
        };
        // THE SENSE IS THE WHOLE MECHANISM. Sensing drag is what takes the
        // gesture off the `ScrollArea`; sensing only click leaves it there. So
        // an unengaged knob deliberately cannot be dragged.
        let sense = match engaged {
            true => Sense::click_and_drag(),
            false => Sense::click(),
        };

        let (rect, resp) = ui.allocate_exact_size(total, sense);
        let dial = egui::Rect::from_min_size(rect.min, Vec2::splat(diameter));

        let span = *range.end() - *range.start();
        let mut out = KnobResponse {
            changed: false,
            value: *value,
            dragging: resp.dragged(),
            engaged: engaged && gate == KnobTouch::HoldToEngage,
        };

        // ── Engaging ────────────────────────────────────────────────────────
        let hold = match gate {
            KnobTouch::HoldToEngage => {
                crate::touch::advance(ui, id, resp.is_pointer_button_down_on(), engaged)
            }
            KnobTouch::Direct => crate::touch::Hold::default(),
        };
        if hold.just_engaged {
            // Sensing drag from the next pass is NOT enough — egui fixed the
            // drag candidate at press time, when this knob was deliberately not
            // sensing drag, so the scrolling container behind it owns the
            // gesture until it is taken back explicitly.
            crate::touch::take_the_drag(ui.ctx(), resp.id);
        }
        let hold_progress = hold.progress;

        // ── Interaction ─────────────────────────────────────────────────────
        // Fine adjustment, from whichever the device offers. Shift has no finger
        // equivalent, so touch uses pull-away: the further the finger strays
        // sideways from where it landed, the finer the control gets.
        let gain = match on_touch {
            true => ui.input(|i| {
                let pull = i
                    .pointer
                    .press_origin()
                    .zip(i.pointer.latest_pos())
                    .map_or(0.0, |(o, p)| (p.x - o.x).abs());
                gain_for_pull(pull)
            }),
            false => gain_for_key(ui.input(|i| i.modifiers.shift_only())),
        };

        if resp.dragged() {
            // UP increases. Screen y grows downward, hence the negation — the
            // one place this has to be said, so it is said once.
            let dy = -resp.drag_delta().y;
            let next = apply_drag(*value, dy, travel, span, gain);
            if next != *value {
                *value = next.clamp(*range.start(), *range.end());
                out.changed = true;
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        // Wheel only. A touch "scroll" IS the page scrolling, and nudging the
        // knob as the page moves under it would be the same theft the hold gate
        // exists to prevent.
        if resp.hovered() && !on_touch {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let next = apply_drag(*value, scroll, travel, span, gain)
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

            // The hold, made visible. A gesture gate nobody can see is a broken
            // control; a ring that fills under the finger teaches itself in one
            // use, and shows the exact moment the knob becomes draggable.
            if hold_progress > 0.0 {
                ui.painter().add(egui::Shape::line(
                    arc_points(dial.center(), dial.width() * 0.5 - 1.0, 0.0, hold_progress),
                    Stroke::new(2.0_f32, accent.gamma_multiply(0.7)),
                ));
            }
            // Engaged, so the reader knows the page will not move under them.
            if out.engaged {
                ui.painter().circle_stroke(
                    dial.center(),
                    dial.width() * 0.5 - 1.0,
                    Stroke::new(2.0_f32, accent.gamma_multiply(0.45)),
                );
            }

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

        // NO TOOLTIP ON TOUCH. egui raises a hover tooltip on a long touch, and
        // the long touch is this widget's engage gesture — so the two fire
        // together and the tooltip's `Area` lands under the finger and eats the
        // drag it was supposed to explain. A tip anchored to the control it
        // describes cannot work when the gesture that summons it is the gesture
        // being described; a touch hint has to live somewhere the finger is not,
        // which is what [`crate::interaction_tip`] is for. Hosts that want one
        // read `KnobResponse::engaged` and push a hint themselves — the widget
        // does not reach for a queue it was not given.
        if let Some(l) = label
            && !on_touch
        {
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
fn apply_drag(value: f32, dy: f32, travel: f32, span: f32, gain: f32) -> f32 {
    // A zero travel would divide by zero and send the knob to an end on the
    // first pixel; it is a caller error, so it is clamped rather than panicking.
    value + dy / travel.max(1.0) * span * gain
}

/// Fine mode from a modifier key: on or off.
fn gain_for_key(fine: bool) -> f32 {
    match fine {
        true => FINE,
        false => 1.0,
    }
}

/// Fine mode from a finger: continuous, by how far it has strayed sideways.
///
/// The pull-away idiom, because a finger has no shift key. It is also better
/// than a key for this job — the precision is a dial rather than a switch, so
/// the reader chooses how fine without letting go and re-gripping.
fn gain_for_pull(pull_px: f32) -> f32 {
    let t = (pull_px.abs() / PULL_FULL).clamp(0.0, 1.0);
    1.0 + (FINE - 1.0) * t
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
        let up = apply_drag(0.5, 10.0, 100.0, 1.0, 1.0);
        let down = apply_drag(0.5, -10.0, 100.0, 1.0, 1.0);
        assert!(up > 0.5, "dragging up must increase: {up}");
        assert!(down < 0.5, "dragging down must decrease: {down}");
    }

    #[test]
    fn travel_is_the_pixels_that_span_the_whole_range() {
        // The promise `travel` makes, in the unit the hand works in: drag that
        // many pixels and you have covered the range, whatever the range is.
        for span in [1.0_f32, 20.0, 1000.0] {
            let moved = apply_drag(0.0, 160.0, 160.0, span, 1.0);
            assert!(
                (moved - span).abs() < 1e-3,
                "span {span} should be covered by 160px, got {moved}"
            );
        }
    }

    #[test]
    fn fine_mode_scales_the_gain_and_nothing_else() {
        let coarse = apply_drag(0.0, 50.0, 160.0, 1.0, gain_for_key(false));
        let fine = apply_drag(0.0, 50.0, 160.0, 1.0, gain_for_key(true));
        assert!((fine - coarse * FINE).abs() < 1e-6, "{fine} vs {coarse}");
    }

    #[test]
    fn a_zero_travel_does_not_divide_by_zero() {
        // A caller passing 0 is wrong, but a knob that returns NaN corrupts the
        // host's value and every readout downstream of it.
        let v = apply_drag(0.5, 10.0, 0.0, 1.0, 1.0);
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
    fn pull_away_reaches_exactly_the_same_fine_as_the_shift_key() {
        // A finger has no modifier, so precision comes from straying sideways.
        // Both devices must bottom out at the same gain, or the same parameter
        // is finer on one than the other and the two disagree about what a
        // careful adjustment is.
        assert!(
            (gain_for_pull(0.0) - 1.0).abs() < 1e-6,
            "no pull, no change"
        );
        assert!((gain_for_pull(PULL_FULL) - FINE).abs() < 1e-6);
        assert!((gain_for_pull(PULL_FULL) - gain_for_key(true)).abs() < 1e-6);
        // And it is a dial, not a switch: halfway out is halfway fine.
        let half = gain_for_pull(PULL_FULL * 0.5);
        assert!(half < 1.0 && half > FINE, "got {half}");
    }

    #[test]
    fn pull_away_is_symmetric_and_cannot_invert_the_gain() {
        // Straying LEFT is as valid as straying right, and no distance may push
        // the gain below `FINE` — a negative or runaway gain would send the knob
        // the wrong way or to an end.
        assert!((gain_for_pull(-PULL_FULL) - gain_for_pull(PULL_FULL)).abs() < 1e-6);
        for px in [PULL_FULL * 2.0, 10_000.0, f32::MAX] {
            let g = gain_for_pull(px);
            assert!((FINE - 1e-6..=1.0).contains(&g), "pull {px} gave gain {g}");
        }
    }

    #[test]
    fn a_finger_gets_a_longer_throw_than_a_mouse() {
        // Not shorter. The device with less precision and no modifier key needs
        // MORE pixels per unit, which is the opposite of the intuition that a
        // small screen wants a small gesture.
        let mouse = apply_drag(0.0, 50.0, BASE_TRAVEL, 1.0, 1.0);
        let finger = apply_drag(0.0, 50.0, BASE_TRAVEL * TOUCH_TRAVEL, 1.0, 1.0);
        assert!(finger < mouse, "same gesture must move a finger less");
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
