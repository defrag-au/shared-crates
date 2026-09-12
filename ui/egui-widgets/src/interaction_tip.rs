//! `InteractionTip` — a hint about a gesture, shown where the gesture is not.
//!
//! ## Why a tooltip cannot do this job
//!
//! egui raises a hover tooltip on a **long touch**. Several controls in this
//! crate use a long touch as the gesture that takes hold of them (see
//! [`crate::touch`]). So on a phone the two fire together, and the tooltip's
//! `Area` opens under the finger and swallows the drag it was trying to explain.
//!
//! That is not a bug to tune away. A tip anchored to the control it describes
//! cannot work when the gesture that summons it *is* the gesture being
//! described — the explanation and the thing explained are competing for the
//! same pixels and the same finger. The only way out is to move the tip
//! somewhere the hand is not, which on a phone means the top of the screen.
//!
//! ## Shape
//!
//! A strip that slides down from the top edge, holds, and slides back. It is
//! host-driven and keyed, exactly like [`crate::toast::ToastQueue`] and for the
//! same reason: a control reporting "I was grabbed" every frame would otherwise
//! stack hundreds of entries. Showing the same key again refreshes its dwell
//! rather than adding a second strip.
//!
//! It is deliberately NOT a toast. A toast reports that something happened and
//! the reader may have missed it; this reports how to work the thing currently
//! under their thumb, so it is quieter, shorter-lived, and it goes to the top
//! where a toast stack goes to the bottom.
//!
//! ```ignore
//! // once per paint, near the top of the central panel
//! interaction_tip::show(ctx, &mut app.tips);
//!
//! // wherever a gesture wants explaining
//! if resp.engaged {
//!     app.tips.hint("knob-drag", "Drag up and down · pull aside for fine");
//! }
//! ```

use egui::{Align2, Area, Color32, Context, Frame, Id, Order, RichText};

use crate::motion::Easing;
use crate::theme::{Radius, Space, TextSize, Theme, ThemeExt};

/// Seconds the strip stays at rest before retracting, once nothing is
/// refreshing it.
const DWELL: f64 = 1.6;

/// Seconds the slide takes in each direction.
const SLIDE: f64 = 0.18;

/// One hint the host wants shown.
#[derive(Clone, Debug)]
struct Tip {
    key: String,
    text: String,
    /// When it was last asked for. Refreshed on every repeat, so a hint holds
    /// while the gesture it describes is still happening.
    last_seen: f64,
}

/// Host-owned queue of interaction hints. Put one on your app state.
#[derive(Clone, Debug, Default)]
pub struct TipQueue {
    tips: Vec<Tip>,
}

impl TipQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask for a hint. Safe to call every frame — repeats refresh the dwell on
    /// the existing strip rather than stacking a new one.
    ///
    /// `key` identifies the hint, not the control: two knobs explaining the same
    /// gesture should pass the same key, or moving between them flickers.
    pub fn hint(&mut self, key: impl Into<String>, text: impl Into<String>) {
        let key = key.into();
        let text = text.into();
        match self.tips.iter_mut().find(|t| t.key == key) {
            Some(existing) => {
                existing.text = text;
                // `last_seen` is stamped by `show`, which is the only place that
                // knows the clock. Clearing it here would need a `Context`.
                existing.last_seen = f64::NAN;
            }
            None => self.tips.push(Tip {
                key,
                text,
                last_seen: f64::NAN,
            }),
        }
    }

    /// Drop a hint immediately, without waiting out its dwell.
    pub fn dismiss(&mut self, key: &str) {
        self.tips.retain(|t| t.key != key);
    }

    /// Whether anything is currently asking to be shown.
    pub fn is_empty(&self) -> bool {
        self.tips.is_empty()
    }
}

/// How far through its slide a strip is, given how long since it was last asked
/// for.
///
/// Pure, because the whole of "does this feel right" is this curve and it should
/// be arguable without a screen. Returns `0.0` fully retracted, `1.0` fully out.
fn extension(age: f64, appeared: f64) -> f32 {
    // Sliding IN is measured from when the strip first appeared; sliding OUT
    // from when it was last refreshed. Keeping them separate is what lets a
    // hint that keeps being asked for stay put instead of pumping.
    let in_t = (appeared / SLIDE).clamp(0.0, 1.0) as f32;
    let out_t = ((age - DWELL) / SLIDE).clamp(0.0, 1.0) as f32;
    (in_t - out_t).clamp(0.0, 1.0)
}

/// Render the queue. Call once per paint.
pub fn show(ctx: &Context, queue: &mut TipQueue) {
    if queue.tips.is_empty() {
        return;
    }
    let now = ctx.input(|i| i.time);
    let theme = ThemeExt::tokens(ctx);

    // Stamp anything the host asked for this frame.
    let mut first_seen: Vec<(String, f64)> = Vec::new();
    for tip in &mut queue.tips {
        if tip.last_seen.is_nan() {
            tip.last_seen = now;
        }
        first_seen.push((tip.key.clone(), tip.last_seen));
    }

    // Only the newest is shown. Two strips stacked at the top edge is a banner,
    // and a banner is the thing this is trying not to be.
    let Some(tip) = queue
        .tips
        .iter()
        .max_by(|a, b| a.last_seen.total_cmp(&b.last_seen))
        .cloned()
    else {
        return;
    };

    let appeared = ctx
        .data(|d| d.get_temp::<f64>(Id::new(("interaction-tip-appeared", &tip.key))))
        .unwrap_or(now);
    ctx.data_mut(|d| d.insert_temp(Id::new(("interaction-tip-appeared", &tip.key)), appeared));

    let age = now - tip.last_seen;
    let ext = extension(age, now - appeared);
    if ext <= 0.0 {
        queue.tips.retain(|t| t.key != tip.key);
        ctx.data_mut(|d| d.remove::<f64>(Id::new(("interaction-tip-appeared", &tip.key))));
        return;
    }
    // An eased slide rather than a linear one, and degraded through the theme's
    // motion tokens so a reduced-motion reader gets the strip without the travel.
    let eased = ctx.easing(Easing::OutCubic).apply(ext);
    let travel = match ctx.travel_allowed() {
        true => eased,
        false => 1.0,
    };

    let gap = theme.space(Space::Md);
    let drop = -(1.0 - travel) * 64.0;

    Area::new(Id::new("interaction-tip"))
        .order(Order::Foreground)
        // NO SENSE. The entire point is that this must not take a gesture — it
        // exists because something anchored to the control did exactly that.
        .sense(egui::Sense::empty())
        .anchor(Align2::CENTER_TOP, [0.0, gap + drop])
        .show(ctx, |ui| {
            ui.set_opacity(travel);
            Frame::new()
                .fill(theme.color.bg_highlight)
                .corner_radius(theme.corner(Radius::Lg))
                .stroke(crate::theme::hairline(theme.color.border))
                .inner_margin(theme.margin_xy(Space::Lg, Space::Base))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&tip.text)
                            .color(theme.color.text_primary)
                            .size(theme.text_size(TextSize::Sm)),
                    );
                });
        });

    // The strip has to retract on its own, and nothing else is moving while a
    // finger rests on a control.
    ctx.request_repaint();
}

/// The tint a host can use to match something to the strip.
pub fn tip_fill(theme: &Theme) -> Color32 {
    theme.color.bg_highlight
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeat_refreshes_rather_than_stacking() {
        // A control reporting "I am grabbed" every frame is the normal case, so
        // the queue has to absorb it. Stacking would build one strip per frame.
        let mut q = TipQueue::new();
        for _ in 0..120 {
            q.hint("knob-drag", "Drag up and down");
        }
        assert_eq!(q.tips.len(), 1);
    }

    #[test]
    fn different_keys_coexist_in_the_queue() {
        let mut q = TipQueue::new();
        q.hint("a", "one");
        q.hint("b", "two");
        assert_eq!(q.tips.len(), 2);
        q.dismiss("a");
        assert_eq!(q.tips.len(), 1);
    }

    #[test]
    fn a_hint_still_being_asked_for_stays_fully_out() {
        // The failure this guards: measuring the slide-out from the same clock
        // as the slide-in makes a hint that keeps being refreshed pump in and
        // out while the finger is still on the control.
        // Measured from SLIDE onward — before that it is still arriving, which
        // `it_arrives_and_leaves_rather_than_appearing` covers.
        for held in [SLIDE, 1.0, 10.0, 600.0] {
            assert_eq!(
                extension(0.0, held),
                1.0,
                "age 0 means still being asked for, at t={held}"
            );
        }
    }

    #[test]
    fn it_arrives_and_leaves_rather_than_appearing() {
        // Fully retracted at birth, fully out after the slide, gone after the
        // dwell plus the slide back.
        assert_eq!(extension(0.0, 0.0), 0.0, "starts retracted");
        assert!(extension(0.0, SLIDE * 0.5) > 0.0, "mid-slide is partway");
        assert_eq!(extension(0.0, SLIDE), 1.0, "fully out after the slide");
        assert_eq!(
            extension(DWELL + SLIDE, 10.0),
            0.0,
            "retracted after dwell + slide"
        );
    }

    #[test]
    fn the_dwell_outlasts_the_slide() {
        // Otherwise the strip begins leaving before it has arrived, and reads as
        // a flicker rather than as a message.
        const { assert!(DWELL > SLIDE * 2.0) };
    }
}
