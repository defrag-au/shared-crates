//! `AboutModal` — what a product is, what state it is in, and what a reader
//! should expect from it while it is in that state.
//!
//! The companion to [`crate::tier_ladder`]: the ladder answers "what do I get",
//! this answers "what am I looking at". Both hang off a small mark in the app
//! bar — a tier chip, a BETA badge — because a mark that only carries a
//! tooltip is a mark most readers never read.
//!
//! ## What it is for
//!
//! A pre-release product makes promises it cannot yet keep, and the honest
//! move is to say which ones. A `BETA` badge on its own does not do that: the
//! word is decoration a reader has learnt to skip, so the caveats it stands for
//! — this is slow, these numbers will move — never actually land. This modal is
//! where they land, in the reader's words rather than the changelog's.
//!
//! ## Shape
//!
//! Title, an optional status chip beside it, one line of lede, then a short
//! list of POINTS: an icon, a headline, and one line of detail each. That is
//! deliberately not a prose block — egui has no real text shaping, so a
//! paragraph of caveats reads as a wall and gets skipped exactly like the badge
//! did. Three or four points is the working ceiling; past that, the reader is
//! being handed release notes.
//!
//! Each point should be a thing the reader will otherwise NOTICE and
//! misattribute: "this is slow" reads as broken, "the threshold moved" reads as
//! a bait-and-switch. Naming them first is what turns a defect into a known
//! limitation.
//!
//! ```ignore
//! use egui_widgets::{AboutModal, AboutPoint, PhosphorIcon};
//!
//! AboutModal::new("Flow Explorer")
//!     .status("BETA")
//!     .intro("Early access — here is what that means in practice.")
//!     .point(AboutPoint::new(
//!         PhosphorIcon::Hourglass,
//!         "Not optimised yet",
//!         "Large wallets take a while to load. Speed is the next thing we work on.",
//!     ))
//!     .show(ui, &mut open);
//! ```

use egui::{RichText, Ui};

use crate::icons::{PhosphorIcon, install_phosphor_font};
use crate::theme;

/// One thing a reader should expect: an icon, a headline, and a line saying
/// what it means for them.
pub struct AboutPoint<'a> {
    icon: PhosphorIcon,
    headline: &'a str,
    detail: &'a str,
}

impl<'a> AboutPoint<'a> {
    /// `headline` is the claim; `detail` is the consequence. Keep the detail to
    /// one line — it wraps, but a point that needs three is really two points.
    pub fn new(icon: PhosphorIcon, headline: &'a str, detail: &'a str) -> Self {
        Self {
            icon,
            headline,
            detail,
        }
    }
}

/// See the [module docs](self).
pub struct AboutModal<'a> {
    title: &'a str,
    status: Option<&'a str>,
    intro: Option<&'a str>,
    points: Vec<AboutPoint<'a>>,
}

impl<'a> AboutModal<'a> {
    /// A modal titled `title`, with nothing in it yet.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            status: None,
            intro: None,
            points: Vec::new(),
        }
    }

    /// A short status word shown as a chip beside the title — `BETA`,
    /// `PREVIEW`. Rendered in the warning colour, matching the badge this
    /// modal is normally opened from, so the two read as the same statement.
    pub fn status(mut self, status: &'a str) -> Self {
        self.status = Some(status);
        self
    }

    /// One line under the title, framing the points below it.
    pub fn intro(mut self, intro: &'a str) -> Self {
        self.intro = Some(intro);
        self
    }

    /// Add a point. Order matters — put the one a reader hits first at the top.
    pub fn point(mut self, point: AboutPoint<'a>) -> Self {
        self.points.push(point);
        self
    }

    /// Render while `open` is true, clearing it on dismissal — so the caller
    /// stores one bool and nothing else, as [`crate::tier_ladder`] does.
    pub fn show(self, ui: &mut Ui, open: &mut bool) {
        if !*open {
            return;
        }
        install_phosphor_font(ui.ctx());

        let mut dismissed = false;
        let response = egui::Modal::new(egui::Id::new("about_modal")).show(ui.ctx(), |ui| {
            // CLAMPED to the viewport rather than asserted: a flat minimum
            // wider than a phone makes the modal itself the thing that
            // overflows, on the one screen whose job is to explain the product.
            let room = (ui.ctx().content_rect().width() - 32.0).max(240.0);
            ui.set_min_width(380.0_f32.min(room));
            ui.set_max_width(500.0_f32.min(room));

            ui.horizontal(|ui| {
                ui.label(RichText::new(self.title).size(16.0).strong());
                if let Some(status) = self.status {
                    status_chip(ui, status);
                }
            });
            if let Some(intro) = self.intro {
                ui.label(RichText::new(intro).small().color(theme::TEXT_MUTED));
            }

            for point in &self.points {
                ui.add_space(10.0);
                point_row(ui, point);
            }

            ui.add_space(12.0);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    dismissed = true;
                }
            });
        });

        // A click outside is a dismissal too — the standard escape hatch, which
        // egui reports separately from anything we drew.
        if response.should_close() {
            dismissed = true;
        }
        if dismissed {
            *open = false;
        }
    }
}

/// The status word as a chip. Deliberately NOT [`crate::chip::Chip`]: this one
/// has to match the app-bar badge that opens the modal, and that badge is drawn
/// by the host from the same two theme colours.
fn status_chip(ui: &mut Ui, status: &str) {
    egui::Frame::default()
        .fill(theme::WARNING.gamma_multiply(0.18))
        .stroke(egui::Stroke::new(1.0_f32, theme::WARNING))
        .inner_margin(egui::Margin::symmetric(5, 1))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.label(RichText::new(status).color(theme::WARNING).small().strong());
        });
}

/// Icon in its own fixed column, headline and detail in the next — so a run of
/// points shares a spine instead of each one starting wherever its icon ended.
fn point_row(ui: &mut Ui, point: &AboutPoint<'_>) {
    const ICON_COL: f32 = 22.0;
    ui.horizontal_top(|ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ICON_COL, ICON_COL), egui::Sense::hover());
        ui.painter().text(
            rect.center_top() + egui::vec2(0.0, 2.0),
            egui::Align2::CENTER_TOP,
            point.icon.codepoint(),
            egui::FontId::new(15.0, crate::icons::phosphor_family()),
            theme::WARNING,
        );
        // The text column takes what is left, so the detail wraps against the
        // modal's edge rather than pushing the modal wider.
        ui.vertical(|ui| {
            ui.label(RichText::new(point.headline).strong());
            ui.label(
                RichText::new(point.detail)
                    .small()
                    .color(theme::TEXT_SECONDARY),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_keep_the_order_they_were_added() {
        // The reader hits them top to bottom, so "put the first thing first"
        // has to be something the caller can actually rely on.
        let modal = AboutModal::new("Flow Explorer")
            .point(AboutPoint::new(PhosphorIcon::Hourglass, "speed", "a"))
            .point(AboutPoint::new(PhosphorIcon::Coins, "tiers", "b"));
        let heads: Vec<&str> = modal.points.iter().map(|p| p.headline).collect();
        assert_eq!(heads, ["speed", "tiers"]);
    }

    #[test]
    fn a_modal_with_no_status_or_intro_is_still_valid() {
        // Both are optional on purpose: an "about" for a shipped product has
        // no status word to show.
        let modal = AboutModal::new("Flow Explorer");
        assert!(modal.status.is_none());
        assert!(modal.intro.is_none());
    }
}
