//! `ServiceBanner` — a persistent strip saying the backend is not currently
//! whole, in the operator's own words.
//!
//! ## Why this is not a toast
//!
//! [`crate::toast`] is for things that HAPPENED: they auto-dismiss, they stack
//! bottom-right, and they are allowed to overlap content because they are
//! transient. A service outage is none of those. It is a CONDITION, it is true
//! for as long as it is true, and a reader who scrolled past the moment it
//! appeared still needs to know why nothing works. A toast that lasted long
//! enough to serve that purpose would be a toast permanently covering the
//! screen — which is the exact failure a toast overlay is tuned to avoid.
//!
//! So this takes layout space at the top of the app instead. It pushes content
//! down rather than covering it, and it stays until the condition lifts.
//!
//! ## Shape
//!
//! One line of message, an optional "since" stamp, and an optional dismiss.
//! Dismissal is deliberately opt-in per host: for a hard outage the honest
//! thing is a banner the reader cannot get rid of, because getting rid of it
//! would not get rid of the outage.
//!
//! ```ignore
//! use egui_widgets::ServiceBanner;
//!
//! if let Some(notice) = &state.maintenance {
//!     ServiceBanner::new(&notice.message)
//!         .since(notice.since_unix)
//!         .show(ui);
//! }
//! ```

use egui::{RichText, Ui};

use crate::icons::{PhosphorIcon, install_phosphor_font};
use crate::relative_time::RelativeTime;
use crate::theme;

/// How the banner reads. Only the palette differs — the shape is one line
/// either way, because a service notice that needs a paragraph is a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BannerTone {
    /// Something is wrong right now: an outage, planned work.
    #[default]
    Warning,
    /// Something is worth knowing but nothing is broken.
    Info,
    /// It came back. Hosts normally show this briefly after a warning lifts.
    Good,
}

impl BannerTone {
    /// `(accent, icon)`. The fill and stroke are derived from the accent, so a
    /// tone is one colour decision rather than three that can disagree.
    fn palette(self) -> (egui::Color32, PhosphorIcon) {
        match self {
            Self::Warning => (theme::WARNING, PhosphorIcon::Warning),
            Self::Info => (theme::ACCENT_BLUE, PhosphorIcon::Eye),
            Self::Good => (theme::SUCCESS, PhosphorIcon::CheckCircle),
        }
    }
}

/// See the [module docs](self).
pub struct ServiceBanner<'a> {
    message: &'a str,
    tone: BannerTone,
    since_unix: Option<i64>,
    dismissible: bool,
}

impl<'a> ServiceBanner<'a> {
    /// A warning banner reading `message`.
    pub fn new(message: &'a str) -> Self {
        Self {
            message,
            tone: BannerTone::default(),
            since_unix: None,
            dismissible: false,
        }
    }

    /// Set the tone. See [`BannerTone`].
    pub fn tone(mut self, tone: BannerTone) -> Self {
        self.tone = tone;
        self
    }

    /// Show how long this has been true, as a relative stamp.
    ///
    /// Worth more than it looks: "under maintenance" reads the same at one
    /// minute and at three hours, and those call for very different patience.
    pub fn since(mut self, unix: i64) -> Self {
        self.since_unix = Some(unix);
        self
    }

    /// Offer a close button. OFF by default — see the module docs.
    pub fn dismissible(mut self, yes: bool) -> Self {
        self.dismissible = yes;
        self
    }

    /// Draw the banner. Returns `true` if the reader dismissed it, which is
    /// always `false` unless [`Self::dismissible`] was set.
    pub fn show(self, ui: &mut Ui) -> bool {
        install_phosphor_font(ui.ctx());
        let (accent, icon) = self.tone.palette();
        let mut dismissed = false;

        egui::Frame::default()
            .fill(accent.gamma_multiply(0.14))
            .stroke(egui::Stroke::new(1.0_f32, accent.gamma_multiply(0.55)))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .corner_radius(6.0)
            .show(ui, |ui| {
                // Claim the row before laying out, so the banner spans the app
                // rather than shrink-wrapping its text — a full-width strip is
                // what makes it read as a condition of the whole surface.
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    icon.show(ui, 15.0, accent);
                    // The message column takes what is left, so a long notice
                    // WRAPS instead of pushing the layout wider than the app.
                    // Measured before the optional trailing controls, which
                    // are fixed-width and known.
                    let trailing = if self.dismissible {
                        ui.spacing().item_spacing.x + 22.0
                    } else {
                        0.0
                    };
                    let text_w = (ui.available_width() - trailing).max(80.0);
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(text_w, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_max_width(text_w);
                            ui.label(RichText::new(self.message).color(theme::TEXT_PRIMARY));
                            if let Some(since) = self.since_unix {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("since").small().color(theme::TEXT_MUTED),
                                    );
                                    ui.add(RelativeTime::new(since));
                                });
                            }
                        },
                    );
                    if self.dismissible {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            if ui
                                .add(
                                    egui::Label::new(
                                        PhosphorIcon::X.rich_text(13.0, theme::TEXT_MUTED),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_text("Dismiss")
                                .clicked()
                            {
                                dismissed = true;
                            }
                        });
                    }
                });
            });
        dismissed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each tone gets its own icon. A banner whose glyph does not match its
    /// colour is worse than no glyph — it is the wrong signal at a glance.
    #[test]
    fn every_tone_has_a_distinct_icon() {
        let icons = [
            BannerTone::Warning.palette().1,
            BannerTone::Info.palette().1,
            BannerTone::Good.palette().1,
        ];
        assert_eq!(icons[0], PhosphorIcon::Warning);
        assert_ne!(icons[0], icons[1]);
        assert_ne!(icons[1], icons[2]);
    }

    /// Warning is the default: a banner constructed without a decision should
    /// read as "something is wrong", which is why a host reached for one.
    #[test]
    fn the_default_tone_is_a_warning() {
        assert_eq!(ServiceBanner::new("down").tone, BannerTone::Warning);
    }

    /// Off by default. An outage a reader can dismiss is an outage they will
    /// forget about while it is still happening.
    #[test]
    fn a_banner_is_not_dismissible_unless_asked() {
        assert!(!ServiceBanner::new("down").dismissible);
        assert!(ServiceBanner::new("down").dismissible(true).dismissible);
    }
}
