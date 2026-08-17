//! `PartyBadge` — a counterparty as it should appear everywhere in a forensic
//! trace: resolved name, **how firmly that name is known**, and the cluster it
//! belongs to.
//!
//! ## Why the basis is required, not optional
//!
//! In a wallet investigation the identities do the analytical work — "this is
//! the payout provider", "this holds 32.6M ADA so it is custodial", "this is
//! the operator's personal wallet". Some of those are read off the chain and
//! some are things a person said, and a display that renders them identically
//! lets the second kind quietly become the first.
//!
//! That is not hypothetical. In the analysis this widget came out of, a
//! per-machine cost figure supplied in conversation was reconciled against an
//! on-chain total by solving for the exchange rate that made it fit; the fitted
//! rate was then described as plausible, and the whole thing sat in two
//! write-ups as established fact until a real rate lookup falsified it days
//! later. Nothing in the presentation had ever distinguished the two.
//!
//! So [`PartyBadge::new`] takes the [`PartyBasis`] as a positional argument.
//! There is no default and no builder for it — a call site cannot render a
//! party without stating how well it is known.
//!
//! ## Reading the badge
//!
//! - [`PartyBasis::Observed`] — no marker. The chain says so directly (balance,
//!   transaction count, absence of a staking credential).
//! - [`PartyBasis::Asserted`] — hollow marker + the source on hover. Someone
//!   told us. **Never render this without a source.**
//! - [`PartyBasis::Derived`] — half marker. Follows from a recorded claim.
//!
//! Unlabelled parties render as a truncated key in monospace, so an
//! un-annotated wallet never looks like a named one.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{PartyBadge, PartyBasis};
//!
//! // Read off the chain — stands on its own.
//! PartyBadge::new("reward wallet", PartyBasis::Observed).show(ui);
//!
//! // Someone told us. The source is mandatory in practice; the widget
//! // shows an unsourced assertion as suspect.
//! PartyBadge::new("the artist", PartyBasis::Asserted)
//!     .source("operator, Discord 2026-08-12")
//!     .cluster("contractors", CLUSTER_COLOR)
//!     .show(ui);
//!
//! // No label yet — renders the key, never a guess.
//! PartyBadge::unlabelled("stake1u9yfhm5la35av8te20s8ezprz568ap5d8zzfz2mnqmgrltgq6z7yj")
//!     .stakeless(false)
//!     .show(ui);
//! ```

use egui::{Color32, CornerRadius, Frame, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

/// How firmly a party's identity is known.
///
/// Mirrors `chain_ledger::Basis`, deliberately re-declared rather than imported
/// so the widget catalog stays domain-free (the same convention `PriceTimeline`
/// and `FocusList` follow). Consumers map one to the other at the edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartyBasis {
    /// Read directly off the chain. Stands on its own.
    Observed,
    /// Supplied from outside the chain — a person, a document, a screenshot.
    Asserted,
    /// Follows from another recorded claim.
    Derived,
}

impl PartyBasis {
    /// Short word for the hover line.
    pub fn word(self) -> &'static str {
        match self {
            PartyBasis::Observed => "observed",
            PartyBasis::Asserted => "asserted",
            PartyBasis::Derived => "derived",
        }
    }

    /// Whether the chain alone supports this identity.
    pub fn is_self_supporting(self) -> bool {
        matches!(self, PartyBasis::Observed)
    }
}

/// Middle-elide a long identifier, matching `IdPill`'s treatment so a key looks
/// the same whether or not it has been labelled.
fn elide(key: &str, head: usize, tail: usize) -> String {
    let n = key.chars().count();
    if n <= head + tail + 1 {
        return key.to_string();
    }
    let start: String = key.chars().take(head).collect();
    let end: String = key.chars().skip(n - tail).collect();
    format!("{start}…{end}")
}

pub struct PartyBadge<'a> {
    label: Option<&'a str>,
    key: Option<&'a str>,
    basis: PartyBasis,
    source: Option<&'a str>,
    cluster: Option<(&'a str, Color32)>,
    stakeless: bool,
    /// Extra context appended to the hover, e.g. "32,672,197 ADA · 509 policies".
    detail: Option<&'a str>,
    text_size: f32,
}

impl<'a> PartyBadge<'a> {
    /// A labelled party. `basis` is positional on purpose — see the module docs.
    pub fn new(label: &'a str, basis: PartyBasis) -> Self {
        Self {
            label: Some(label),
            key: None,
            basis,
            source: None,
            cluster: None,
            stakeless: false,
            detail: None,
            text_size: 12.0,
        }
    }

    /// A party with no label yet. Renders the key in monospace, so an
    /// un-annotated wallet is never mistaken for a named one. Basis is
    /// necessarily `Observed` — the key is what the chain returned.
    pub fn unlabelled(key: &'a str) -> Self {
        Self {
            label: None,
            key: Some(key),
            basis: PartyBasis::Observed,
            source: None,
            cluster: None,
            stakeless: false,
            detail: None,
            text_size: 12.0,
        }
    }

    /// The underlying key, shown on hover beneath a label.
    pub fn key(mut self, key: &'a str) -> Self {
        self.key = Some(key);
        self
    }

    /// Where an assertion came from. Required in practice for
    /// [`PartyBasis::Asserted`]; its absence is rendered as a warning.
    pub fn source(mut self, source: &'a str) -> Self {
        self.source = Some(source);
        self
    }

    pub fn cluster(mut self, name: &'a str, color: Color32) -> Self {
        self.cluster = Some((name, color));
        self
    }

    /// Marks an address with no staking credential — the enterprise/off-ramp
    /// *shape*. Never an assertion about where the money went.
    pub fn stakeless(mut self, stakeless: bool) -> Self {
        self.stakeless = stakeless;
        self
    }

    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    pub fn text_size(mut self, size: f32) -> Self {
        self.text_size = size;
        self
    }

    /// An unsourced assertion — the state the widget is meant to make visible.
    fn is_unsourced_assertion(&self) -> bool {
        self.basis == PartyBasis::Asserted && self.source.is_none()
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let muted = ui.visuals().weak_text_color();
        let normal = ui.visuals().text_color();
        let warn = ui.visuals().warn_fg_color;

        let frame = Frame::NONE
            .inner_margin(Margin::symmetric(5, 2))
            .corner_radius(CornerRadius::same(3));

        let resp = frame
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.horizontal(|ui| {
                    // Cluster colour sits left, as a thin bar rather than a
                    // fill — a filled background would compete with Chip and
                    // read as a status.
                    if let Some((_, color)) = self.cluster {
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(2.0, self.text_size + 2.0),
                            Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 1.0, color);
                    }

                    self.paint_basis_marker(ui, muted);

                    match self.label {
                        Some(label) => {
                            let color = if self.is_unsourced_assertion() {
                                warn
                            } else {
                                normal
                            };
                            ui.label(RichText::new(label).size(self.text_size).color(color));
                        }
                        None => {
                            let key = self.key.unwrap_or("");
                            ui.label(
                                RichText::new(elide(key, 10, 6))
                                    .size(self.text_size - 1.0)
                                    .monospace()
                                    .color(muted),
                            );
                        }
                    }

                    if self.stakeless {
                        // Shape, not origin — the tooltip says so explicitly.
                        ui.label(
                            RichText::new("no-stake")
                                .size(self.text_size - 3.0)
                                .color(muted),
                        )
                        .on_hover_text(
                            "No staking credential — enterprise-address shape, \
                             common for exchange and OTC legs. A shape, not a \
                             claim about where the funds came from.",
                        );
                    }
                });
            })
            .response;

        resp.on_hover_ui(|ui| self.hover(ui))
    }

    /// A small glyph carrying the basis: filled = observed, half = derived,
    /// hollow = asserted. Deliberately shape-coded rather than colour-coded so
    /// it survives both themes and colour-vision differences.
    fn paint_basis_marker(&self, ui: &mut Ui, muted: Color32) {
        let d = self.text_size * 0.5;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(d, d), Sense::hover());
        let c = rect.center();
        let r = d * 0.42;
        let p = ui.painter();
        match self.basis {
            PartyBasis::Observed => {
                p.circle_filled(c, r, muted);
            }
            PartyBasis::Derived => {
                p.circle_stroke(c, r, Stroke::new(1.0_f32, muted));
                p.circle_filled(c, r * 0.45, muted);
            }
            PartyBasis::Asserted => {
                let color = if self.source.is_none() {
                    ui.visuals().warn_fg_color
                } else {
                    muted
                };
                p.circle_stroke(c, r, Stroke::new(1.0_f32, color));
            }
        }
    }

    fn hover(&self, ui: &mut Ui) {
        let muted = ui.visuals().weak_text_color();

        if let Some(label) = self.label {
            ui.label(RichText::new(label).strong());
        }
        if let Some(key) = self.key {
            ui.label(RichText::new(key).monospace().size(11.0).color(muted));
        }

        ui.label(
            RichText::new(format!("basis: {}", self.basis.word()))
                .size(11.0)
                .color(muted),
        );

        match (self.basis, self.source) {
            (PartyBasis::Asserted, None) => {
                ui.label(
                    RichText::new("ASSERTED WITH NO SOURCE — treat as unverified")
                        .size(11.0)
                        .color(ui.visuals().warn_fg_color),
                );
            }
            (_, Some(src)) => {
                ui.label(
                    RichText::new(format!("source: {src}"))
                        .size(11.0)
                        .color(muted),
                );
            }
            _ => {}
        }

        if let Some((name, _)) = self.cluster {
            ui.label(
                RichText::new(format!("cluster: {name}"))
                    .size(11.0)
                    .color(muted),
            );
        }
        if let Some(detail) = self.detail {
            ui.label(RichText::new(detail).size(11.0).color(muted));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_observed_stands_on_its_own() {
        assert!(PartyBasis::Observed.is_self_supporting());
        assert!(!PartyBasis::Asserted.is_self_supporting());
        assert!(!PartyBasis::Derived.is_self_supporting());
    }

    /// The state the widget exists to surface.
    #[test]
    fn asserted_without_source_is_flagged() {
        let bare = PartyBadge::new("the artist", PartyBasis::Asserted);
        assert!(bare.is_unsourced_assertion());

        let sourced = PartyBadge::new("the artist", PartyBasis::Asserted).source("operator, 08-12");
        assert!(!sourced.is_unsourced_assertion());
    }

    /// An observation needs no source, so it must never be flagged.
    #[test]
    fn observed_is_never_flagged_for_a_missing_source() {
        assert!(
            !PartyBadge::new("32.6M ADA wallet", PartyBasis::Observed).is_unsourced_assertion()
        );
        assert!(!PartyBadge::new("x", PartyBasis::Derived).is_unsourced_assertion());
    }

    /// An un-annotated party must render its key, never an invented label.
    #[test]
    fn unlabelled_keeps_the_key_and_claims_nothing() {
        let b = PartyBadge::unlabelled("stake1u9yfhm5la35av8te20s8ezprz568ap5d8zzf");
        assert!(b.label.is_none());
        assert_eq!(b.basis, PartyBasis::Observed);
        assert!(!b.is_unsourced_assertion());
    }

    #[test]
    fn elide_keeps_both_ends_and_shortens() {
        let key = "stake1u9yfhm5la35av8te20s8ezprz568ap5d8zzfz2mnqmgrltgq6z7yj";
        let e = elide(key, 10, 6);
        assert!(e.starts_with("stake1u9yf"));
        assert!(e.ends_with("gq6z7yj".get(1..).unwrap()));
        assert!(e.chars().count() < key.chars().count());
    }

    #[test]
    fn elide_leaves_short_keys_alone() {
        assert_eq!(elide("abc", 10, 6), "abc");
    }
}
