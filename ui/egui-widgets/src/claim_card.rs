//! `ClaimCard` — an assertion, what would refute it, and whether anyone has
//! tried.
//!
//! ## The gate is on citing a claim, not on writing one down
//!
//! A finding is only as good as the test it survived. In the investigation this
//! came out of, the two strongest moves were both attempts to kill a conclusion
//! rather than support it — *"if this wallet were the mining pool, it would have
//! paid during the seven months the fleet was actually running"* (it paid
//! nothing) and *"if the long-running funding channel were the same exchange,
//! the conclusion collapses"* (it wasn't).
//!
//! An earlier cut of this widget made the falsifier a required constructor
//! argument. That was the wrong place for the friction: nobody writes down what
//! would refute an idea at the moment they are still deciding whether they
//! believe it, so a mandatory field gets filled with "TBD" and the discipline
//! evaporates while still *looking* present — which is worse than not having the
//! field.
//!
//! So capture is free: [`ClaimCard::new`] takes only the statement, and a claim
//! starts [`FalsifierStatus::Untested`]. What the falsifier gates is
//! **promotion** — [`FalsifierStatus::is_load_bearing`] is true only once a test
//! has been recorded and survived. The cost lands where it belongs, at the point
//! of citing something, and never at the point of jotting it.
//!
//! ## Refuted claims are kept, never deleted
//!
//! [`FalsifierStatus::Refuted`] is a first-class state that keeps rendering.
//! "We tested this and it failed" is a finding, and it is the record that stops
//! a dead idea being re-derived from the same evidence three weeks later. In the
//! source investigation a per-machine cost figure was reconciled against an
//! on-chain total, believed for days, then falsified by a price check —
//! discarding that would leave the same trap open for the next reader.
//!
//! ## Marks first, prose on demand
//!
//! A claim list is read by scanning, and egui is poor at blocks of text — twenty
//! stacked cards of paragraphs is a wall nobody reads. So everything a reader
//! needs at a glance is carried by marks:
//!
//! - a **three-pip state track** (stated → falsifiable → tested) says how far
//!   the claim got and what it still needs, with no sentence to read;
//! - **support pips** shaped exactly as [`PartyBadge`](crate::PartyBadge) shapes
//!   basis — filled observed, half derived, hollow asserted, warn-coloured when
//!   unattributed — answer "what does this rest on" before any row is opened;
//! - the **statement** is one elided line, full text on hover.
//!
//! The falsifier, the outcome and the support list live behind
//! [`ClaimCard::expanded`]. That is not the falsifier being hidden: its absence
//! is visible in the state track whether or not anyone expands the card, which
//! is the point — let the mark carry the discipline, not a paragraph.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{ClaimCard, ClaimSupport, FalsifierStatus, PartyBasis};
//!
//! let support = vec![
//!     ClaimSupport::new("conduit → reward wallet, 3 txs, 30,531.41 ADA", PartyBasis::Observed),
//!     ClaimSupport::new("27 machines × $187 deposit", PartyBasis::Asserted),
//! ];
//!
//! ClaimCard::new(
//!     "The June payout was funded by a custodial withdrawal, not mining revenue.",
//!     "If this wallet were the mining pool, it would have paid during the seven \
//!      months the fleet was producing.",
//!     FalsifierStatus::Survived,
//! )
//! .support(&support)
//! .outcome("Ran 2026-08-16: the wallet has existed since 2024-09 and paid nothing until 2026-05.")
//! .show(ui);
//! ```

use egui::{
    Color32, CornerRadius, Frame, Margin, Pos2, Response, RichText, Sense, Stroke, Ui, Vec2,
};

use crate::party_badge::{PartyBadge, PartyBasis};

/// How far a claim has got towards being something you could cite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FalsifierStatus {
    /// Just written down. Nobody has said what would refute it. The default,
    /// and a perfectly respectable state for an idea in progress.
    #[default]
    Untested,
    /// A falsifier is recorded but has not been run.
    Pending,
    /// Run, and the claim held. The only state that carries weight.
    Survived,
    /// Run, and it killed the claim. Kept on the board deliberately.
    Refuted,
}

impl FalsifierStatus {
    pub fn badge(self) -> &'static str {
        match self {
            FalsifierStatus::Untested => "UNTESTED",
            FalsifierStatus::Pending => "TEST PENDING",
            FalsifierStatus::Survived => "SURVIVED",
            FalsifierStatus::Refuted => "REFUTED",
        }
    }

    /// Whether this claim may be leaned on. Only a claim whose falsifier was
    /// run and survived carries weight — an untested one is a guess with
    /// citations, and a refuted one is a record of a dead end.
    ///
    /// This is the gate. A claim can sit in any other state indefinitely
    /// without complaint; it just cannot be cited as a finding.
    pub fn is_load_bearing(self) -> bool {
        matches!(self, FalsifierStatus::Survived)
    }

    /// Whether the card should read as provisional.
    pub fn is_provisional(self) -> bool {
        matches!(self, FalsifierStatus::Untested | FalsifierStatus::Pending)
    }
}

/// One piece of evidence under a claim.
pub struct ClaimSupport<'a> {
    pub summary: &'a str,
    pub basis: PartyBasis,
    pub source: Option<&'a str>,
    pub reference: Option<&'a str>,
}

impl<'a> ClaimSupport<'a> {
    pub fn new(summary: &'a str, basis: PartyBasis) -> Self {
        Self {
            summary,
            basis,
            source: None,
            reference: None,
        }
    }

    pub fn source(mut self, source: &'a str) -> Self {
        self.source = Some(source);
        self
    }

    /// A transaction hash, document id, or anything else a reader can go check.
    pub fn reference(mut self, reference: &'a str) -> Self {
        self.reference = Some(reference);
        self
    }

    fn is_unsourced_assertion(&self) -> bool {
        self.basis == PartyBasis::Asserted && self.source.is_none()
    }
}

/// How many pieces of support are assertions nobody attributed.
///
/// Surfaced on the card rather than left to a reader to notice: the whole
/// failure mode is an assertion inheriting the authority of the chain data it
/// sits next to.
pub fn unsourced_assertions(support: &[ClaimSupport<'_>]) -> usize {
    support
        .iter()
        .filter(|s| s.is_unsourced_assertion())
        .count()
}

/// The weakest basis anything under this claim rests on — a claim is no
/// stronger than its softest support.
pub fn weakest_basis(support: &[ClaimSupport<'_>]) -> Option<PartyBasis> {
    support.iter().map(|s| s.basis).max_by_key(|b| match b {
        PartyBasis::Observed => 0,
        PartyBasis::Derived => 1,
        PartyBasis::Asserted => 2,
    })
}

pub struct ClaimCardResponse {
    pub response: Response,
    /// The card was clicked — the host flips its own expanded flag.
    pub toggled: bool,
}

pub struct ClaimCard<'a> {
    statement: &'a str,
    falsifier: Option<&'a str>,
    status: FalsifierStatus,
    support: &'a [ClaimSupport<'a>],
    outcome: Option<&'a str>,
    id: Option<&'a str>,
    expanded: bool,
}

impl<'a> ClaimCard<'a> {
    /// A claim, at its cheapest: just the statement. Starts
    /// [`FalsifierStatus::Untested`].
    pub fn new(statement: &'a str) -> Self {
        Self {
            statement,
            falsifier: None,
            status: FalsifierStatus::default(),
            support: &[],
            outcome: None,
            id: None,
            expanded: false,
        }
    }

    /// What would refute this. Optional at capture time; required in practice
    /// before the claim can reach [`FalsifierStatus::Survived`].
    pub fn falsifier(mut self, falsifier: &'a str) -> Self {
        self.falsifier = Some(falsifier);
        self
    }

    pub fn status(mut self, status: FalsifierStatus) -> Self {
        self.status = status;
        self
    }

    pub fn support(mut self, support: &'a [ClaimSupport<'a>]) -> Self {
        self.support = support;
        self
    }

    /// What actually happened when the falsifier was run. Required in practice
    /// for anything other than a provisional status; its absence is
    /// called out, because "survived" with no account of the test is just an
    /// assertion wearing a badge.
    pub fn outcome(mut self, outcome: &'a str) -> Self {
        self.outcome = Some(outcome);
        self
    }

    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Show the falsifier, outcome and support list. Host-owned, as the rest of
    /// the catalog does it — the card reports [`ClaimCardResponse::toggled`] and
    /// the host flips its own flag.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    fn accent(&self, ui: &Ui) -> Color32 {
        match self.status {
            FalsifierStatus::Untested | FalsifierStatus::Pending => ui.visuals().weak_text_color(),
            FalsifierStatus::Survived => Color32::from_rgb(0x19, 0x9e, 0x70),
            FalsifierStatus::Refuted => ui.visuals().warn_fg_color,
        }
    }

    /// A verdict with no account of the test behind it. Survived/Refuted mean
    /// somebody ran something; without a record of what, the badge is decoration.
    fn is_unevidenced_verdict(&self) -> bool {
        !self.status.is_provisional() && self.outcome.is_none()
    }

    /// Claimed to have been tested with no test written down at all.
    fn claims_a_test_it_never_states(&self) -> bool {
        self.status != FalsifierStatus::Untested && self.falsifier.is_none()
    }
    /// How far along the three steps this claim is: stated → falsifiable →
    /// tested. Returned as filled-pip count so the track can be read without
    /// reading any words.
    fn progress(&self) -> usize {
        let mut n = 1; // stating it is step one
        if self.falsifier.is_some() {
            n += 1;
        }
        if !self.status.is_provisional() {
            n += 1;
        }
        n
    }

    /// One row per claim: stripe, badge, state track, support pips, statement.
    ///
    /// Everything longer than a line lives behind [`ClaimCard::expanded`]. egui
    /// renders paragraphs poorly and twenty stacked cards of prose is a wall
    /// nobody scans — so the state a reader needs at a glance (how far this got,
    /// what it rests on, whether anything under it is an assertion) is carried
    /// by marks, and the words are available on demand.
    pub fn show(self, ui: &mut Ui) -> ClaimCardResponse {
        let accent = self.accent(ui);
        let muted = ui.visuals().weak_text_color();
        let warn = ui.visuals().warn_fg_color;
        let status = self.status;
        let mut toggled = false;

        let inner = Frame::NONE
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(Margin::symmetric(10, 7))
            .corner_radius(CornerRadius::same(3))
            // Dashed below for anything provisional; a solid border would read
            // as ordinary card chrome.
            .stroke(Stroke::NONE)
            .show(ui, |ui| {
                ui.set_width(ui.available_width().min(560.0));

                let top = ui.cursor().top();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    self.state_track(ui, accent, muted);
                    ui.label(
                        RichText::new(status.badge())
                            .size(9.0)
                            .strong()
                            .color(accent),
                    );
                    if let Some(id) = self.id {
                        ui.label(RichText::new(id).size(9.0).monospace().color(muted));
                    }
                    // Support composition, right-aligned: one mark per piece of
                    // evidence, shaped by how firmly it is known.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.support_pips(ui, muted, warn);
                    });
                });

                ui.add_space(3.0);

                // The statement is the one thing that must be legible without
                // interaction — kept to a single elided line.
                let statement = match status {
                    FalsifierStatus::Refuted => RichText::new(self.statement)
                        .size(12.0)
                        .strikethrough()
                        .color(muted),
                    FalsifierStatus::Survived => RichText::new(self.statement).size(12.0).strong(),
                    _ => RichText::new(self.statement).size(12.0).color(muted),
                };
                let line = ui.add(
                    egui::Label::new(statement)
                        .truncate()
                        .sense(egui::Sense::click()),
                );
                if line.clicked() {
                    toggled = true;
                }
                line.on_hover_text(self.statement);

                if self.expanded {
                    self.detail(ui, accent, muted, warn);
                }

                let bottom = ui.cursor().top();
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::Pos2::new(ui.min_rect().left() - 6.0, top),
                        Vec2::new(2.0, (bottom - top).max(1.0)),
                    ),
                    1.0,
                    accent,
                );
            });

        let resp = inner.response;
        if status.is_provisional() {
            let r = resp.rect.shrink(0.5);
            let stroke = Stroke::new(1.0_f32, muted.gamma_multiply(0.55));
            let corners = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
                r.left_top(),
            ];
            for seg in corners.windows(2) {
                ui.painter()
                    .add(egui::Shape::dashed_line(seg, stroke, 4.0, 3.0));
            }
        }

        ClaimCardResponse {
            response: resp,
            toggled,
        }
    }

    /// Three pips: stated · falsifiable · tested. Position in the track says
    /// what the claim still needs, with no sentence required.
    fn state_track(&self, ui: &mut Ui, accent: Color32, muted: Color32) {
        let filled = self.progress();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(30.0, 10.0), Sense::hover());
        let p = ui.painter();
        for i in 0..3 {
            let c = Pos2::new(rect.left() + 4.0 + i as f32 * 11.0, rect.center().y);
            if i > 0 {
                p.line_segment(
                    [Pos2::new(c.x - 7.0, c.y), Pos2::new(c.x - 3.5, c.y)],
                    Stroke::new(
                        1.0_f32,
                        if i < filled {
                            accent.gamma_multiply(0.7)
                        } else {
                            muted.gamma_multiply(0.4)
                        },
                    ),
                );
            }
            if i < filled {
                p.circle_filled(c, 3.0, accent);
            } else {
                p.circle_stroke(c, 3.0, Stroke::new(1.0_f32, muted.gamma_multiply(0.6)));
            }
        }
    }

    /// One mark per piece of support, shaped by basis exactly as
    /// [`PartyBadge`](crate::PartyBadge) shapes it — filled observed, half
    /// derived, hollow asserted — so "what does this rest on" is answered
    /// before any row is read.
    fn support_pips(&self, ui: &mut Ui, muted: Color32, warn: Color32) {
        if self.support.is_empty() {
            return;
        }
        let n = self.support.len();
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(n as f32 * 9.0 + 2.0, 10.0), Sense::hover());
        let p = ui.painter();
        for (i, s) in self.support.iter().enumerate() {
            let c = Pos2::new(rect.right() - 5.0 - i as f32 * 9.0, rect.center().y);
            let col = if s.is_unsourced_assertion() {
                warn
            } else {
                muted
            };
            match s.basis {
                PartyBasis::Observed => {
                    p.circle_filled(c, 3.0, col);
                }
                PartyBasis::Derived => {
                    p.circle_stroke(c, 3.0, Stroke::new(1.0_f32, col));
                    p.circle_filled(c, 1.4, col);
                }
                PartyBasis::Asserted => {
                    p.circle_stroke(c, 3.0, Stroke::new(1.0_f32, col));
                }
            }
        }
        let unsourced = unsourced_assertions(self.support);
        resp.on_hover_text(if unsourced > 0 {
            format!(
                "{n} supporting facts · {unsourced} unsourced assertion{}",
                if unsourced == 1 { "" } else { "s" }
            )
        } else {
            format!("{n} supporting facts, all attributed")
        });
    }

    /// The prose, only once asked for.
    fn detail(&self, ui: &mut Ui, accent: Color32, muted: Color32, warn: Color32) {
        ui.add_space(6.0);
        let cap = |ui: &mut Ui, t: &str| {
            ui.label(RichText::new(t).size(9.0).strong().color(muted));
        };

        match self.falsifier {
            Some(f) => {
                cap(ui, "WOULD REFUTE THIS");
                ui.label(RichText::new(f).size(11.0).italics());
            }
            None => {
                ui.label(
                    RichText::new(if self.claims_a_test_it_never_states() {
                        "Marked tested, but no falsifier was ever written down."
                    } else {
                        "No falsifier yet."
                    })
                    .size(10.0)
                    .italics()
                    .color(if self.claims_a_test_it_never_states() {
                        warn
                    } else {
                        muted
                    }),
                );
            }
        }

        if let Some(outcome) = self.outcome {
            ui.add_space(5.0);
            cap(
                ui,
                match self.status {
                    FalsifierStatus::Refuted => "WHAT KILLED IT",
                    _ => "WHEN RUN",
                },
            );
            ui.label(RichText::new(outcome).size(11.0).color(accent));
        } else if self.is_unevidenced_verdict() {
            ui.add_space(5.0);
            ui.label(
                RichText::new("Marked as tested with no account of the test recorded.")
                    .size(11.0)
                    .color(warn),
            );
        }

        if !self.support.is_empty() {
            ui.add_space(7.0);
            cap(ui, "RESTS ON");
            for s in self.support {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let badge = PartyBadge::new(s.summary, s.basis).text_size(11.0);
                    let badge = match s.source {
                        Some(src) => badge.source(src),
                        None => badge,
                    };
                    badge.show(ui);
                    if let Some(r) = s.reference {
                        ui.label(
                            RichText::new(r)
                                .size(10.0)
                                .monospace()
                                .color(muted.gamma_multiply(0.8)),
                        );
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(summary: &'static str, basis: PartyBasis) -> ClaimSupport<'static> {
        ClaimSupport::new(summary, basis)
    }

    /// Only a survived claim carries weight — and that is the ONLY thing the
    /// falsifier gates. Everything else is free to sit in the case file.
    #[test]
    fn only_a_survived_claim_is_load_bearing() {
        assert!(FalsifierStatus::Survived.is_load_bearing());
        assert!(!FalsifierStatus::Untested.is_load_bearing());
        assert!(!FalsifierStatus::Pending.is_load_bearing());
        assert!(!FalsifierStatus::Refuted.is_load_bearing());
    }

    /// Capture must cost nothing: a bare statement is a valid claim, and it
    /// starts untested rather than being rejected or half-built.
    #[test]
    fn a_bare_statement_is_a_valid_claim() {
        let c = ClaimCard::new("the conduit looks like an exchange withdrawal leg");
        assert_eq!(c.status, FalsifierStatus::Untested);
        assert!(c.falsifier.is_none());
        assert!(!c.status.is_load_bearing());
        // Not an error state — nothing to warn about yet.
        assert!(!c.is_unevidenced_verdict());
        assert!(!c.claims_a_test_it_never_states());
    }

    #[test]
    fn provisional_covers_everything_not_yet_citable() {
        assert!(FalsifierStatus::Untested.is_provisional());
        assert!(FalsifierStatus::Pending.is_provisional());
        assert!(!FalsifierStatus::Survived.is_provisional());
        assert!(!FalsifierStatus::Refuted.is_provisional());
    }

    #[test]
    fn unsourced_assertions_are_counted() {
        let support = vec![
            s("chain: 3 txs, 30,531.41 ADA", PartyBasis::Observed),
            s("27 machines × $187", PartyBasis::Asserted),
            s("operator said so", PartyBasis::Asserted).source("Discord 2026-08-12"),
        ];
        assert_eq!(
            unsourced_assertions(&support),
            1,
            "only the unattributed one"
        );
    }

    /// A claim is no stronger than its softest support.
    #[test]
    fn weakest_basis_finds_the_softest_support() {
        assert_eq!(
            weakest_basis(&[
                s("a", PartyBasis::Observed),
                s("b", PartyBasis::Derived),
                s("c", PartyBasis::Asserted),
            ]),
            Some(PartyBasis::Asserted)
        );
        assert_eq!(
            weakest_basis(&[s("a", PartyBasis::Observed), s("b", PartyBasis::Derived)]),
            Some(PartyBasis::Derived)
        );
        assert_eq!(weakest_basis(&[]), None);
    }

    /// "Survived" with no account of the test is a badge with nothing behind it.
    #[test]
    fn a_verdict_without_an_account_of_the_test_is_flagged() {
        let bare = ClaimCard::new("x")
            .falsifier("y")
            .status(FalsifierStatus::Survived);
        assert!(bare.is_unevidenced_verdict());

        let evidenced = ClaimCard::new("x")
            .falsifier("y")
            .status(FalsifierStatus::Survived)
            .outcome("ran, held");
        assert!(!evidenced.is_unevidenced_verdict());

        // A provisional claim is not expected to have one.
        assert!(!ClaimCard::new("x").is_unevidenced_verdict());
        assert!(
            !ClaimCard::new("x")
                .falsifier("y")
                .status(FalsifierStatus::Pending)
                .is_unevidenced_verdict()
        );
    }

    /// Claiming a verdict without ever writing the test down is the loophole
    /// the softened gate could otherwise open.
    #[test]
    fn a_verdict_with_no_falsifier_at_all_is_flagged() {
        let sneaky = ClaimCard::new("x")
            .status(FalsifierStatus::Survived)
            .outcome("trust me");
        assert!(sneaky.claims_a_test_it_never_states());

        let honest = ClaimCard::new("x")
            .falsifier("check the spot price")
            .status(FalsifierStatus::Survived)
            .outcome("checked");
        assert!(!honest.claims_a_test_it_never_states());

        assert!(!ClaimCard::new("x").claims_a_test_it_never_states());
    }

    /// A refuted claim is retained, and must never read as support.
    #[test]
    fn refuted_claims_are_kept_but_carry_no_weight() {
        let refuted = ClaimCard::new("27 × $187 deposits explain the 30,531 ADA")
            .falsifier("Check ADA spot on the actual transfer dates.")
            .status(FalsifierStatus::Refuted)
            .outcome("Spot was $0.235–0.252, not the $0.1654 the match required.");
        assert!(!refuted.status.is_load_bearing());
        assert!(!refuted.is_unevidenced_verdict());
        assert_eq!(FalsifierStatus::Refuted.badge(), "REFUTED");
    }

    #[test]
    fn badges_are_distinct() {
        let all = [
            FalsifierStatus::Untested,
            FalsifierStatus::Pending,
            FalsifierStatus::Survived,
            FalsifierStatus::Refuted,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a.badge(), b.badge());
            }
        }
    }
}
