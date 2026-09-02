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

use egui::{Color32, CornerRadius, Rect, Sense, Ui, Vec2};

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

    pub fn show(self, ui: &mut Ui) {
        // Motion only where it means something. A repaint is requested only
        // for the animated reason, so a withheld placeholder costs an idle
        // surface nothing — which matters, because a paywalled list can sit
        // on screen indefinitely.
        let breath = match self.reason.breathes() {
            true => {
                ui.ctx().request_repaint();
                let t = ui.input(|i| i.time) as f32;
                // A slow, shallow pulse. Fast or deep reads as an error state.
                0.75 + 0.25 * (t * 2.2).sin()
            }
            false => 1.0,
        };

        // The THEME's text colour, not a literal grey: these sit against
        // whatever surface the host is using, and a hardcoded tone that suited
        // one background is how a placeholder ends up invisible on another.
        let base = ui.visuals().text_color();

        match self.shape {
            Shape::Block { size } => {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(6), tint(base, 0.10 * breath));
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
                    let fade = depth * breath;

                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), self.row_height),
                        Sense::hover(),
                    );
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(6), tint(base, 0.06 * fade));

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
                            CornerRadius::same(3),
                            tint(base, 0.12 * fade),
                        );
                    }
                    ui.add_space(6.0);
                }
            }
        }
    }
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
