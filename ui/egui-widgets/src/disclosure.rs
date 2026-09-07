//! `Disclosure` — a detail region that opens beneath the row it explains, eased
//! open and tied to that row, without the feed jumping under the reader.
//!
//! # Why this is a widget and not four lines of `if open { … }`
//!
//! The naive version — draw the body when a flag is set — is the one that
//! feels cheap, and every reason is a detail it skips:
//!
//! - **It appears.** A block of detail materialising mid-list reads as the
//!   layout breaking, not as something opening. Height has to grow from zero,
//!   and it has to grow on a curve.
//! - **It shoves.** Inserting 200pt into a scrolled list moves every row below
//!   it, so the row the reader clicked slides away from their pointer at the
//!   moment they are looking at it. Anchoring is the single biggest difference
//!   between "slick" and "janky", and it is the thing nobody remembers to do
//!   by hand.
//! - **It floats.** Detail drawn as its own card, with its own background and
//!   margins, reads as a sibling of the rows rather than as part of one. A
//!   rule down the left edge, indented under the row, is what makes it read as
//!   *belonging to* the thing above it.
//!
//! # What it does NOT own
//!
//! Which row is open. The caller holds that — usually as an `Option<Id>` it
//! already has for other reasons (a selection, a route parameter, a URL).
//! Owning it here would mean two sources of truth for something that is often
//! in the address bar, and accordion semantics ("only one open at a time")
//! fall out of the caller's `Option` for free rather than needing a mode flag.
//!
//! # Animation is egui's, not ours
//!
//! The open/close is [`egui::collapsing_header::CollapsingState`], which
//! already measures the body at its natural height and clips to an eased
//! fraction of it. Hand-rolling that means either laying the body out twice
//! per frame — impossible with an `FnOnce`, and double the interaction ids if
//! you force `Fn` — or growing the *available* height, which re-wraps the
//! content on every frame so text reflows as it opens. What this adds is the
//! rule, the indent and the anchoring; the machinery underneath is stock.
//!
//! # When to reach for [`crate::detail_split`] instead
//!
//! That puts the detail in a column beside the content. Prefer it when the
//! detail is LONG: a disclosure with 400pt in it pushes the rest of the list
//! off-screen and has to be scrolled past to reach the next row, which is
//! worse than a side panel on every axis. This is for detail measured in a
//! handful of lines — a few fields, a short list, a couple of actions — where
//! the tie to one specific row is worth more than the column.

use egui::collapsing_header::CollapsingState;
use egui::{Id, Rect, Ui, Vec2};

use crate::theme;

/// Width of the rule tying the body to the row above it.
const RULE_W: f32 = 2.0;

/// How far the body sits past the rule.
const INDENT: f32 = 10.0;

/// Space above the body, so the rule does not start hard against the row.
const LEAD: f32 = 4.0;

/// A detail region attached to the row above it.
pub struct Disclosure {
    id: Id,
    open: bool,
    anchor: bool,
    indent: f32,
    rule: Option<egui::Color32>,
}

impl Disclosure {
    /// `id` must be stable for the ROW, not its index — a feed re-pages, and
    /// an index-keyed animation then plays on whichever row inherited the
    /// slot, so opening row 3 animates a different transaction each time.
    pub fn new(id: impl std::hash::Hash, open: bool) -> Self {
        Self {
            id: Id::new(id),
            open,
            anchor: true,
            indent: INDENT,
            rule: Some(theme::ACCENT),
        }
    }

    /// Keep the region in view as it opens. On by default; turn it off when
    /// the caller is already scrolling deliberately, so the two do not fight
    /// over the same frame.
    pub fn anchor(mut self, anchor: bool) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }

    /// Tint the tie-rule, or `None` to draw none.
    pub fn rule(mut self, rule: Option<egui::Color32>) -> Self {
        self.rule = rule;
        self
    }

    /// Draw `body` if this is open or still animating.
    ///
    /// Returns whether anything was drawn, so a caller can skip its trailing
    /// spacing rather than leaving a gap under a closed row.
    pub fn show(self, ui: &mut Ui, body: impl FnOnce(&mut Ui)) -> bool {
        // `set_open` is what drives the animation: the state remembers where
        // it was and eases toward where the caller says it should be. Loading
        // with the caller's value as the default matters for the first frame —
        // a row that arrives already open (a deep link with `?tx=`) should be
        // open, not animate open from nothing.
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), self.id, self.open);
        state.set_open(self.open);

        let openness = state.openness(ui.ctx());
        if openness <= f32::EPSILON {
            return false;
        }

        ui.add_space(LEAD);
        let left = ui.cursor().min.x;
        let indent = RULE_W + self.indent;

        // The body is indented past the rule by hand rather than with
        // `ui.indent`, which adds its own bookkeeping and a default width this
        // has an opinion about.
        let inner = state.show_body_unindented(ui, |ui| {
            let rect = ui.available_rect_before_wrap();
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_size(
                        rect.min + Vec2::new(indent, 0.0),
                        Vec2::new((rect.width() - indent).max(0.0), rect.height()),
                    ))
                    .layout(*ui.layout()),
            );
            body(&mut child);
            let used = child.min_rect();
            ui.advance_cursor_after_rect(used);
            used
        });

        let Some(inner) = inner else {
            return false;
        };

        // THE RULE, drawn after the body so it spans exactly what was shown —
        // `show_body_unindented` clips to the eased height, and the response
        // rect is that clipped height rather than the natural one.
        if let Some(colour) = self.rule {
            let span = inner.response.rect;
            ui.painter().rect_filled(
                Rect::from_min_size(
                    egui::pos2(left, span.min.y),
                    Vec2::new(RULE_W, span.height()),
                ),
                1.0,
                colour,
            );
        }

        // ANCHOR ONLY WHILE OPENING. On a settled panel this would fight a
        // reader who has since scrolled away from it — the scroll would snap
        // back every frame and the list would feel stuck.
        if self.anchor && self.open && openness < 1.0 {
            ui.scroll_to_rect(inner.response.rect, None);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A CLOSED disclosure takes no space and runs no body.
    ///
    /// The second half is what matters: a body that runs while closed is a
    /// body whose fetches, `Image::new` calls and interaction ids all happen
    /// for every row in the feed at once.
    #[test]
    fn a_closed_disclosure_draws_nothing() {
        let ctx = egui::Context::default();
        let mut ran = false;
        let mut drew = true;
        let _ = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                let before = ui.cursor().min.y;
                drew = Disclosure::new("row", false).show(ui, |ui| {
                    ran = true;
                    ui.label("detail");
                });
                assert_eq!(ui.cursor().min.y, before, "no space taken");
            });
        });
        assert!(!drew);
        assert!(!ran, "a closed body must not run");
    }

    /// An OPEN one runs its body and reports that it drew.
    #[test]
    fn an_open_disclosure_draws_its_body() {
        let ctx = egui::Context::default();
        let mut ran = false;
        let mut drew = false;
        let _ = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                drew = Disclosure::new("row", true).show(ui, |ui| {
                    ran = true;
                    ui.label("detail");
                });
            });
        });
        assert!(drew);
        assert!(ran);
    }

    /// A row that arrives ALREADY open — a deep link with `?tx=` — is open on
    /// its first frame rather than animating open from nothing, which would
    /// read as the page still loading.
    #[test]
    fn a_deep_linked_row_opens_without_animating() {
        let ctx = egui::Context::default();
        let mut openness = 0.0;
        let _ = ctx.run_ui(Default::default(), |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                Disclosure::new("row", true).show(ui, |ui| {
                    ui.label("detail");
                });
                openness = CollapsingState::load_with_default_open(ui.ctx(), Id::new("row"), false)
                    .openness(ui.ctx());
            });
        });
        assert_eq!(openness, 1.0, "fully open on the first frame");
    }
}
