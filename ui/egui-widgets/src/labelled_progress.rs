//! `LabelledProgress` — a busy mark the size of the words beside it.
//!
//! ## Why it's a widget
//!
//! Every surface that waits for something writes `Spinner::new()` next to a
//! label, and every one of them comes out the wrong size. That is not
//! carelessness at the call sites — it is `egui::Spinner`'s own arithmetic,
//! and it is wrong in BOTH directions:
//!
//! - **Unsized**, it takes `spacing.interact_size.y`. Under touch sizing that
//!   is floored at the minimum tap target — 44pt — so a spinner beside an
//!   11pt caption comes out four times its height.
//! - **Sized**, it draws `radius = height / 2 - 2`. Ask for 12 and you get a
//!   circle of 8, visibly smaller than the 12pt text it sits against.
//!
//! Compensating that `- 2` at a call site hardcodes an egui internal, and the
//! call sites did not know to. So the mark is painted here instead, filling
//! exactly the box it is given: `radius * 2 + stroke == size`. The box is the
//! label's own line height, so the mark and the words are the same height by
//! construction rather than by a number someone tuned once.
//!
//! ## What it is not
//!
//! Not [`progress_bar`](crate::progress_bar), which is for a KNOWN fraction —
//! a percentage, a countdown. This is for work whose remaining time is not
//! knowable, which is most chain work: a submitted transaction is waiting for
//! a block, and nothing can say how far through that it is.
//!
//! Not [`CachedSpinner`](crate::image_loader::CachedSpinner) either. That
//! exists to stamp one precomputed arc at fifty positions in an image grid
//! without redoing the trig per cell; it takes a radius and has no label. If
//! you are drawing many marks at once and none of them has words, use that.
//!
//! ```no_run
//! # use egui_widgets::labelled_progress::{LabelledProgress, ProgressState};
//! # use egui_widgets::theme::TextSize;
//! # fn demo(ui: &mut egui::Ui) {
//! LabelledProgress::new("Building transactions").show(ui);
//!
//! LabelledProgress::new("Waiting for a block")
//!     .size(TextSize::Sm)
//!     .show(ui);
//!
//! LabelledProgress::new("Submitted")
//!     .state(ProgressState::Done)
//!     .show(ui);
//! # }
//! ```

use egui::{Color32, RichText, Ui, Vec2};

use crate::icons::PhosphorIcon;
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt};

/// Seconds per revolution.
const SPIN_SECONDS: f64 = 1.0;
/// How much of the circle the arc covers, in turns.
///
/// A gap is what makes the rotation legible — a full ring spinning looks like
/// a ring standing still.
const SPIN_ARC: f32 = 0.7;
/// Segments in the arc. Enough that the curve does not read as faceted.
const SPIN_SEGMENTS: usize = 24;

/// What the mark is saying.
///
/// An enum and not a `bool` plus an `Option<bool>`: the three states are the
/// three things a step can be, and a caller that has to encode "done" as
/// "not busy and not failed" gets it wrong the first time a fourth state is
/// added.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ProgressState {
    /// Working, with no way to say how far along. A sweeping arc.
    #[default]
    Busy,
    /// Finished, and it worked.
    Done,
    /// Finished, and it did not.
    Failed,
    /// Its turn has not come — an earlier step has to finish first. A hollow
    /// ring: present, clearly not moving, and not mistaken for done.
    Waiting,
}

/// A label with a state mark sized to match it.
pub struct LabelledProgress<'a> {
    label: &'a str,
    size: TextSize,
    state: ProgressState,
    colour: Option<Color32>,
}

impl<'a> LabelledProgress<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            size: TextSize::Base,
            state: ProgressState::Busy,
            colour: None,
        }
    }

    /// The type step for the label. The mark is sized from it.
    pub fn size(mut self, size: TextSize) -> Self {
        self.size = size;
        self
    }

    pub fn state(mut self, state: ProgressState) -> Self {
        self.state = state;
        self
    }

    /// Override the colour both the mark and the label take.
    ///
    /// By default the state chooses: muted while busy or waiting, green when
    /// done, the error colour when failed. Override when the surface has its
    /// own meaning for the row — not to signal the state a second time.
    pub fn colour(mut self, colour: Color32) -> Self {
        self.colour = Some(colour);
        self
    }

    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let tokens = ui.tokens();
        let point_size = ui.text_size(self.size);
        // The mark's box is the LABEL's line height, so the two match by
        // construction. A fixed point size would drift the moment a theme
        // changed its ramp or a caller picked a different step.
        let extent = crate::theme::line_height(ui, point_size);
        let ink = self.colour.unwrap_or(match self.state {
            ProgressState::Busy | ProgressState::Waiting => tokens.color.text_muted,
            ProgressState::Done => tokens.color.accent_green,
            ProgressState::Failed => tokens.color.error,
        });

        ui.horizontal(|ui| {
            ui.set_item_gap_x(Space::Base);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(extent), egui::Sense::hover());
            match self.state {
                ProgressState::Busy => paint_busy(ui, rect, ink),
                ProgressState::Waiting => {
                    let (stroke, radius) = busy_geometry(extent);
                    ui.painter().circle_stroke(
                        rect.center(),
                        radius,
                        egui::Stroke::new(stroke, ink),
                    );
                }
                // Glyphs rather than painted marks: a tick and a cross are
                // what the icon set is for, and at this size they read better
                // than anything two arcs could say.
                ProgressState::Done => {
                    paint_glyph(ui, rect, PhosphorIcon::Check, point_size, ink);
                }
                ProgressState::Failed => {
                    paint_glyph(ui, rect, PhosphorIcon::X, point_size, ink);
                }
            }
            ui.label(RichText::new(self.label).size(point_size).color(ink));
        })
        .response
    }
}

/// Centre a glyph in the same box the arc would fill.
///
/// Through `PhosphorIcon::paint`, which knows the icon font family and
/// installs it. Painting `icon.as_str()` with `FontId::proportional` compiles,
/// renders a tofu box, and is invisible to `tests/no_broken_glyphs.rs` —
/// which reads source strings, not what the painter was handed.
fn paint_glyph(ui: &Ui, at: egui::Rect, icon: PhosphorIcon, size: f32, colour: Color32) {
    icon.paint(
        ui.painter(),
        at.center(),
        egui::Align2::CENTER_CENTER,
        size,
        colour,
    );
}

/// A busy mark: an arc sweeping once per [`SPIN_SECONDS`], filling `at`.
///
/// `pub(crate)` so a surface whose rows already carry their own names — the
/// wallet roster, say — can draw the mark without a second label.
pub(crate) fn paint_busy(ui: &Ui, at: egui::Rect, colour: Color32) {
    let (stroke, radius) = busy_geometry(at.width());
    let weight = egui::Stroke::new(stroke, colour);

    if !ui.ctx().travel_allowed() {
        // A spinner is the one thing on screen that cannot hold still. Same
        // place, same size, no travel.
        ui.painter().circle_stroke(at.center(), radius, weight);
        return;
    }

    let phase = (ui.input(|i| i.time) / SPIN_SECONDS).fract() as f32;
    let points: Vec<egui::Pos2> = (0..=SPIN_SEGMENTS)
        .map(|i| {
            let t = phase + SPIN_ARC * (i as f32 / SPIN_SEGMENTS as f32);
            let a = t * std::f32::consts::TAU;
            egui::pos2(
                at.center().x + radius * a.sin(),
                at.center().y - radius * a.cos(),
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(points, weight));
    // Nothing else on the row is necessarily animating, so the next pass has
    // to be asked for.
    ui.ctx().request_repaint();
}

/// `(stroke, radius)` for a busy mark filling a box of `width`.
///
/// The stroke sits INSIDE the box — `radius * 2 + stroke == width` — so the
/// mark's drawn extent is exactly the box it was given, and therefore exactly
/// the line height of the label beside it. That equality is the whole
/// invariant, and it is the one `egui::Spinner` breaks.
pub(crate) fn busy_geometry(width: f32) -> (f32, f32) {
    let stroke = (width * 0.12).max(1.5);
    (stroke, (width - stroke) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mark fills its box exactly, at every size a caller might ask for.
    ///
    /// This is the invariant the widget exists to hold. It was broken twice in
    /// `egui::Spinner`, in opposite directions, both times by its own
    /// arithmetic rather than by the size asked for.
    #[test]
    fn the_mark_fills_the_box_exactly() {
        for width in [10.0_f32, 12.0, 16.0, 24.0, 44.0] {
            let (stroke, radius) = busy_geometry(width);
            let drawn = radius * 2.0 + stroke;
            assert!(
                (drawn - width).abs() < 1e-4,
                "width {width} drew {drawn} — the mark must fill its box exactly"
            );
            assert!(stroke >= 1.5, "width {width} gave a hairline of {stroke}");
            assert!(radius > 0.0, "width {width} gave radius {radius}");
        }
    }

    /// A full ring spinning looks like a ring standing still.
    #[test]
    fn the_arc_leaves_a_gap_so_the_rotation_reads() {
        // `const {}` because these ARE constants, and clippy is right that a
        // runtime assertion on one is theatre — this way it fails to compile.
        // Closed, and the rotation cannot be seen; too short, and it reads as
        // a dash rather than a ring.
        const { assert!(SPIN_ARC > 0.4 && SPIN_ARC < 1.0) };
        const { assert!(SPIN_SECONDS > 0.0) };
        const { assert!(SPIN_SEGMENTS >= 12) };
    }

    /// Busy is the default: the state a caller reaches for this widget in.
    #[test]
    fn the_default_state_is_busy() {
        assert_eq!(ProgressState::default(), ProgressState::Busy);
    }
}
