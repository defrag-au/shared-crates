//! `InstallPanel` — what installing a bot asks for, and why: the link to hand
//! over, the permissions it requests, and the settings the link cannot carry.
//!
//! An OAuth install link is a request to a server's administrators to hand a
//! bot a set of permissions. The permission bitfield is opaque, so the one
//! thing a reader needs — *what am I agreeing to* — is invisible in the link
//! itself, and asking the reader to decode an integer in Discord's developer
//! portal is how a bot ends up with more than anyone intended.
//!
//! So the panel shows three things, in this order:
//!
//! 1. **The link**, with a copy affordance. It is what gets sent.
//! 2. **The permissions**, each with the reason it is there. A permission with
//!    no reason is one nobody can decide on, so the caller must supply one —
//!    [`InstallFact::detail`] is not optional.
//! 3. **What is not a permission** — the privileged intents, the scope, the
//!    developer-portal toggles an install link cannot set. Separating these
//!    matters because a reader who does not find them here will assume the link
//!    covered them.
//!
//! Data-only: the caller owns the URL and the two lists (typically from the
//! same table it built the URL from, so the two cannot disagree).

use egui::RichText;
use egui::Ui;

use crate::icons::PhosphorIcon;
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt};

/// One line in either list: a name, and the reason it is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallFact<'a> {
    pub name: &'a str,
    /// One clause. Not a paragraph — this is read beside the permission, and a
    /// wall of text here is a wall of text nobody reads before clicking grant.
    pub detail: &'a str,
}

impl<'a> InstallFact<'a> {
    pub fn new(name: &'a str, detail: &'a str) -> Self {
        Self { name, detail }
    }
}

/// What the panel draws. See the module docs.
pub struct InstallPanel<'a> {
    url: &'a str,
    permissions: &'a [InstallFact<'a>],
    requirements: &'a [InstallFact<'a>],
    blurb: &'a str,
    note: Option<&'a str>,
    empty_note: Option<&'a str>,
}

/// What the reader did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InstallPanelResponse {
    /// The copy button was clicked; the link is on the clipboard already.
    pub copied: bool,
}

impl<'a> InstallPanel<'a> {
    /// `url` is the invite link; `permissions` is what it requests.
    pub fn new(url: &'a str, permissions: &'a [InstallFact<'a>]) -> Self {
        Self {
            url,
            permissions,
            requirements: &[],
            blurb: "",
            note: None,
            empty_note: None,
        }
    }

    /// Settings the link cannot carry. See the module docs for why these are a
    /// separate list rather than more permissions.
    pub fn requirements(mut self, requirements: &'a [InstallFact<'a>]) -> Self {
        self.requirements = requirements;
        self
    }

    /// One or two lines above the link: who this is for, and what to do with it.
    pub fn blurb(mut self, blurb: &'a str) -> Self {
        self.blurb = blurb;
        self
    }

    /// A closing line — a caveat, or what happens on a re-install.
    pub fn note(mut self, note: &'a str) -> Self {
        self.note = Some(note);
        self
    }

    /// The caller's words for an unresolved link. Without it the panel says what
    /// it can — that there is no link — which is true but not the reason.
    pub fn empty_note(mut self, note: &'a str) -> Self {
        self.empty_note = Some(note);
        self
    }

    pub fn show(self, ui: &mut Ui) -> InstallPanelResponse {
        crate::icons::ensure_fonts(ui);
        let mut response = InstallPanelResponse::default();

        if !self.blurb.is_empty() {
            ui.colored_label(ui.tokens().color.text_muted, self.blurb);
            ui.gap(Space::Md);
        }

        if self.url.is_empty() {
            // The empty state is a real state: the caller has not resolved the
            // link yet. Saying so beats a copy button that copies nothing.
            ui.colored_label(
                ui.tokens().color.text_muted,
                self.empty_note.unwrap_or("No install link is available yet."),
            );
            return response;
        }

        ui.horizontal_wrapped(|ui| {
            let copy = ui.small_button(
                PhosphorIcon::Copy.rich_text(13.0, ui.tokens().color.accent_blue),
            );
            if copy.on_hover_text("Copy the install link").clicked() {
                ui.ctx().copy_text(self.url.to_string());
                response.copied = true;
            }
            ui.add(
                egui::Label::new(
                    RichText::new(self.url)
                        .monospace()
                        .size(ui.text_size(TextSize::Sm)),
                )
                .truncate(),
            )
            .on_hover_text(self.url);
        });
        ui.gap(Space::Lg);

        if !self.permissions.is_empty() {
            ui.strong("Permissions requested");
            ui.gap(Space::Xs);
            for fact in self.permissions {
                fact_row(ui, PhosphorIcon::Check, ui.tokens().color.success, fact);
            }
            ui.gap(Space::Md);
        }

        if !self.requirements.is_empty() {
            ui.strong("Not permissions");
            ui.gap(Space::Xs);
            for fact in self.requirements {
                fact_row(
                    ui,
                    PhosphorIcon::Warning,
                    ui.tokens().color.accent_yellow,
                    fact,
                );
            }
            ui.gap(Space::Md);
        }

        if let Some(note) = self.note {
            ui.colored_label(
                ui.tokens().color.text_muted,
                RichText::new(note).size(ui.text_size(TextSize::Sm)),
            );
        }

        response
    }
}

/// One name-and-reason line, marked.
fn fact_row(ui: &mut Ui, icon: PhosphorIcon, color: egui::Color32, fact: &InstallFact<'_>) {
    ui.horizontal_wrapped(|ui| {
        icon.show(ui, 12.0, color);
        ui.label(RichText::new(fact.name).size(ui.text_size(TextSize::Base)));
        ui.colored_label(
            ui.tokens().color.text_muted,
            RichText::new(fact.detail).size(ui.text_size(TextSize::Sm)),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant the panel is built around: every permission carries its
    /// reason, so a caller cannot render a list of names and call it explained.
    /// The type enforces it; this pins that the constructor takes both.
    #[test]
    fn a_fact_is_a_name_and_a_reason() {
        let fact = InstallFact::new("View Audit Log", "the snapshot reads role changes");
        assert_eq!(fact.name, "View Audit Log");
        assert!(!fact.detail.is_empty());
    }

    /// Nothing is invented for a panel built with nothing. Every field is the
    /// caller's, so a default here would be a default arriving on an operator's
    /// screen beside a link that does not exist.
    #[test]
    fn nothing_is_invented_for_an_unresolved_link() {
        let panel = InstallPanel::new("", &[]);
        assert!(panel.url.is_empty());
        assert!(panel.permissions.is_empty());
        assert!(panel.requirements.is_empty());
        assert!(panel.blurb.is_empty());
        assert!(panel.note.is_none());
        assert!(panel.empty_note.is_none());
    }

    /// A caller that knows why the link is missing gets to say so; the panel's
    /// own line is the fallback, not an override.
    #[test]
    fn the_empty_note_is_the_callers() {
        let panel =
            InstallPanel::new("", &[]).empty_note("The worker builds this on the first sync.");
        assert_eq!(
            panel.empty_note,
            Some("The worker builds this on the first sync.")
        );
    }
}
