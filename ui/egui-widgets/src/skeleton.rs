//! `Skeleton` — placeholder shapes standing in for content that is not on
//! screen, and a statement of WHY it is not.
//!
//! ## The reason is positional, on purpose
//!
//! There are two reasons content is missing, and they call for opposite
//! behaviour:
//!
//! - [`SkeletonReason::Loading`] — it is coming. The placeholder **pulses**,
//!   because "wait" is exactly the right instruction and motion is how a
//!   surface says it.
//! - [`SkeletonReason::Withheld`] — it is not coming. Entitlement, a window, a
//!   paywall. The placeholder is **static and recedes**, because waiting
//!   produces nothing and a shimmer would be a lie told in animation.
//!
//! Getting that backwards is the failure this widget exists to prevent: a
//! shimmer over a paywall trains a reader to sit and wait for rows no amount
//! of patience will deliver. So the reason is a constructor argument rather
//! than a setter with a default — a call site cannot draw a skeleton without
//! saying which it means. Same rule, and the same reasoning, as
//! [`crate::party_badge::PartyBadge`]'s basis.
//!
//! ## Two shapes
//!
//! - [`Skeleton::rows`] — a list that stops. A list that ends looks identical
//!   to a list that was always empty, and the difference can be enormous: one
//!   wallet has three thousand transactions behind a window, the other has
//!   never done anything.
//! - [`Skeleton::block`] — one rectangle, for a thumbnail or an avatar that
//!   has no image yet or is not ours to show. Reserves the space so the layout
//!   does not jump when it arrives.
//!
//! ## It carries no data
//!
//! Deliberately. The number of rows is a VISUAL quantity chosen by the caller,
//! never the number of hidden items — the same shapes are drawn whether three
//! rows are behind the gate or three thousand. A placeholder that leaked the
//! shape of its content would defeat the gate it illustrates.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{Skeleton, SkeletonReason};
//!
//! // A feed cut short by the reader's tier.
//! Skeleton::rows(3, SkeletonReason::Withheld).show(ui);
//!
//! // A thumbnail still being fetched.
//! Skeleton::block(egui::vec2(64.0, 64.0), SkeletonReason::Loading).show(ui);
//! ```

use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

use crate::theme::{Radius, Space, SpaceExt, ThemeExt};

/// Why the content is not here. See the module docs — this is not decoration,
/// it decides whether the placeholder moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkeletonReason {
    /// On its way. Pulses, because waiting is the right instruction.
    Loading,
    /// Not on its way — held back by entitlement, a window, or a paywall.
    /// Static and receding: no amount of waiting produces it.
    Withheld,
}

impl SkeletonReason {
    /// Does this reason animate?
    fn breathes(self) -> bool {
        matches!(self, SkeletonReason::Loading)
    }
}

/// What the placeholder is shaped like.
enum Shape {
    /// A list of rows, each carrying bars at the given width fractions.
    Rows { count: usize, bars: Vec<f32> },
    /// A single rectangle — a thumbnail, an avatar, a chart that has not
    /// arrived.
    Block { size: Vec2 },
}

pub struct Skeleton {
    shape: Shape,
    reason: SkeletonReason,
    row_height: f32,
}

impl Skeleton {
    /// A list that stops. `reason` is positional — see the module docs.
    pub fn rows(count: usize, reason: SkeletonReason) -> Self {
        Self {
            shape: Shape::Rows {
                count,
                // The shape of a title and a detail line, which is what most
                // rows are.
                bars: vec![0.34, 0.18],
            },
            reason,
            row_height: 54.0,
        }
    }

    /// One rectangle, for an image or a panel. `reason` is positional — see
    /// the module docs.
    pub fn block(size: Vec2, reason: SkeletonReason) -> Self {
        Self {
            shape: Shape::Block { size },
            reason,
            row_height: size.y,
        }
    }

    /// Height of each row. Match the real row it stands in for, or the
    /// handover from content to placeholder reads as a layout jump.
    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    /// Bar widths within each row, as fractions of the available width.
    ///
    /// Give it the rhythm of whatever it replaces: one bar for a plain list,
    /// two for a card with a detail line, three for something denser. Ignored
    /// by [`Skeleton::block`], which has no bars.
    pub fn bars(mut self, widths: impl Into<Vec<f32>>) -> Self {
        if let Shape::Rows { bars, .. } = &mut self.shape {
            *bars = widths.into();
        }
        self
    }

    /// Paint a block skeleton into a rect the caller already owns, allocating
    /// nothing.
    ///
    /// For a surface that has decided its geometry and is only filling it —
    /// [`crate::smart_image::SmartImage`] standing in for a thumbnail that has
    /// not arrived. [`Self::show`] is the one to reach for otherwise; it
    /// allocates, which is what a skeleton standing in for a *list* has to do.
    ///
    /// Only the block shape: rows lay themselves out down a container, which
    /// is a thing you cannot do inside someone else's rect.
    pub fn paint_block_at(ui: &Ui, rect: Rect, reason: SkeletonReason) {
        request_pulse_frames(ui, reason);
        let breath = breath_at(ui, reason, rect);
        paint_block(ui, rect, ui.visuals().text_color(), breath);
    }

    pub fn show(self, ui: &mut Ui) {
        request_pulse_frames(ui, self.reason);

        // The THEME's text colour, not a literal grey: these sit against
        // whatever surface the host is using, and a hardcoded tone that suited
        // one background is how a placeholder ends up invisible on another.
        let base = ui.visuals().text_color();

        match self.shape {
            Shape::Block { size } => {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                paint_block(ui, rect, base, breath_at(ui, self.reason, rect));
            }
            Shape::Rows { count, bars } => {
                for i in 0..count {
                    // WITHHELD rows recede down the list, which says the
                    // content continues past what is readable. Loading rows
                    // do not: they are all equally on their way, and fading
                    // them would imply the later ones are less likely.
                    let depth = match self.reason {
                        SkeletonReason::Withheld => 1.0 - (i as f32 / count.max(2) as f32) * 0.6,
                        SkeletonReason::Loading => 1.0,
                    };

                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), self.row_height),
                        Sense::hover(),
                    );
                    // After the rect, because the pulse is phased by WHERE
                    // this row is — see `phase_of`. Down a list that reads as
                    // the wave running down the page.
                    let fade = depth * breath_at(ui, self.reason, rect);
                    ui.painter().rect_filled(
                        rect,
                        ui.tokens().corner(Radius::Md),
                        tint(base, 0.06 * fade),
                    );

                    let inner = rect.shrink2(Vec2::new(14.0, 12.0));
                    let bar_height = 10.0;
                    let gap = 8.0;
                    for (n, width) in bars.iter().enumerate() {
                        let y = n as f32 * (bar_height + gap);
                        if y + bar_height > inner.height() {
                            break;
                        }
                        let bar = Rect::from_min_size(
                            inner.min + Vec2::new(0.0, y),
                            Vec2::new(inner.width() * width.clamp(0.02, 1.0), bar_height),
                        );
                        ui.painter().rect_filled(
                            bar,
                            ui.tokens().corner(Radius::Sm),
                            tint(base, 0.12 * fade),
                        );
                    }
                    ui.gap(Space::Base);
                }
            }
        }
    }
}

/// The pulse, and the repaint that drives it.
///
/// Motion only where it means something: a repaint is requested for the
/// animated reason ONLY, so a withheld placeholder costs an idle surface
/// nothing — which matters, because a paywalled list can sit on screen
/// indefinitely. The same applies to a grid of loading thumbnails: the frames
/// stop the moment the last one arrives.
fn request_pulse_frames(ui: &Ui, reason: SkeletonReason) {
    if reason.breathes() {
        ui.ctx().request_repaint();
    }
}

/// Radians of phase lag per point of distance across the surface.
///
/// Tuned so neighbouring cards are visibly offset but a row still reads as one
/// movement: at ~150pt between cards that is ~0.6 rad of lag, so a six-wide
/// row spans most of a cycle and the eye follows a wave across it rather than
/// seeing six independent blinks.
const PHASE_PER_POINT: f32 = 0.004;

/// How far through the pulse a placeholder at this position is.
///
/// Derived from POSITION, which is what stops a grid strobing. One shared
/// clock meant forty thumbnails flashing in perfect unison, and synchronised
/// flashing reads as an alarm rather than as content arriving. Offset by
/// `x + y` it becomes a wave travelling down and across — the same
/// information, stated as movement with a direction.
///
/// Position rather than an index because a skeleton is not told its index,
/// and because a wave anchored to the screen stays coherent when a grid
/// reflows or is scrolled.
fn phase_of(at: Pos2) -> f32 {
    (at.x + at.y) * PHASE_PER_POINT
}

/// The pulse itself: slow and shallow. Fast or deep reads as an error state.
///
/// Pure, so its shape can be asserted on without a `Ui`.
fn pulse(time: f32, phase: f32) -> f32 {
    0.75 + 0.25 * (time * 2.2 - phase).sin()
}

/// The pulse for a placeholder occupying `rect`, on this frame's clock.
fn breath_at(ui: &Ui, reason: SkeletonReason, rect: Rect) -> f32 {
    if !reason.breathes() {
        return 1.0;
    }
    pulse(ui.input(|i| i.time) as f32, phase_of(rect.min))
}

/// One placeholder rectangle. Shared by the allocating and the paint-at-a-rect
/// entry points so they cannot drift apart.
fn paint_block(ui: &Ui, rect: Rect, base: Color32, breath: f32) {
    ui.painter().rect_filled(
        rect,
        ui.tokens().corner(Radius::Md),
        tint(base, 0.10 * breath),
    );
}

/// The text colour at a low alpha — so a placeholder sits against whatever
/// surface the host is drawing on rather than against an assumed one.
fn tint(base: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        base.r(),
        base.g(),
        base.b(),
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::{PHASE_PER_POINT, phase_of, pulse};
    use egui::pos2;

    /// The point of phasing by position: two placeholders side by side must
    /// NOT be at the same point in the pulse. A grid that flashes in unison
    /// reads as an alarm rather than as content arriving.
    #[test]
    fn neighbouring_placeholders_are_out_of_step() {
        // Sampled across a cycle, not at one instant: two phases always
        // coincide momentarily as they pass a turning point, so a single
        // reading proves nothing either way. What matters is that they are
        // visibly apart for most of the cycle.
        let (left, right) = (phase_of(pos2(0.0, 0.0)), phase_of(pos2(150.0, 0.0)));
        let widest = (0..128)
            .map(|step| {
                let time = step as f32 * 0.05;
                (pulse(time, left) - pulse(time, right)).abs()
            })
            .fold(0.0_f32, f32::max);
        assert!(
            widest > 0.1,
            "a card 150pt away never gets further than {widest} out of step"
        );
    }

    /// …and the wave has to run diagonally, or a grid resolves into columns
    /// blinking together instead of one movement.
    #[test]
    fn the_wave_travels_across_and_down() {
        assert!(phase_of(pos2(100.0, 0.0)) > phase_of(pos2(0.0, 0.0)));
        assert!(phase_of(pos2(0.0, 100.0)) > phase_of(pos2(0.0, 0.0)));
        // Equal lag from either axis, which is what makes it diagonal.
        assert_eq!(phase_of(pos2(100.0, 0.0)), phase_of(pos2(0.0, 100.0)));
    }

    /// Stable for a given position: the phase must not wander between frames,
    /// or the "wave" is just noise.
    #[test]
    fn a_position_always_has_the_same_phase() {
        assert_eq!(phase_of(pos2(42.0, 17.0)), phase_of(pos2(42.0, 17.0)));
    }

    /// Shallow on purpose. The alpha this multiplies is already low, and a
    /// pulse that reaches zero reads as content disappearing rather than
    /// arriving.
    #[test]
    fn the_pulse_stays_shallow_whatever_the_phase() {
        for step in 0..64 {
            let time = step as f32 * 0.25;
            let value = pulse(time, phase_of(pos2(37.0, 91.0)));
            assert!(
                (0.5..=1.0).contains(&value),
                "pulse left its band at t={time}: {value}"
            );
        }
    }

    /// A lag big enough to see, small enough that a row still reads as one
    /// movement — roughly half a cycle across a six-wide grid of 150pt cards.
    #[test]
    fn the_lag_across_a_row_is_most_of_a_cycle_not_a_blur() {
        let across_a_row = phase_of(pos2(6.0 * 150.0, 0.0));
        assert!(
            (1.5..std::f32::consts::TAU).contains(&across_a_row),
            "six cards span {across_a_row} rad"
        );
        // Compile-time, so a future edit that zeroes the lag — collapsing the
        // wave back into the unison this exists to break — fails to build.
        const { assert!(PHASE_PER_POINT > 0.0) };
    }
}
