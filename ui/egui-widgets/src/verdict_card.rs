//! `VerdictCard` — a `tx_verdict::TxVerdict` drawn as a [`TxCard`] (feature
//! `verdict`).
//!
//! ## Why this is one place
//!
//! [`TxCard`] is deliberately domain-free: it takes preformatted strings and
//! resolved labels, so the widget catalogue never depends on what a
//! transaction is. The verdict is the domain. Something has to map one to the
//! other, and if every app writes that mapping by hand they will disagree —
//! about which tag kind gets which chip, about what an unnamed print is
//! called, about whether a `Derived` party renders as observed. This is the
//! mapping, once, feature-gated like `gateway` so a consumer that never sees
//! a verdict never pays for the dependency.
//!
//! ## What it owns
//!
//! `TxCardData` borrows every string it shows. Two of them do not exist on the
//! verdict — a print's thumbnail URL and its display label are computed from
//! an [`Item`] — so this holds them, built once per card in [`Self::new`], and
//! lends them to the card in [`Self::show`]. A feed builds one of these per
//! visible row per frame, which is the same cost the feed already paid for its
//! old entry list.
//!
//! ## What a click returns
//!
//! [`TxCardResponse::walk`] carries the party's KEY when the verdict set one,
//! else its label — see `tx_verdict::Party`. A producer that keys its parties
//! gets a navigable address straight back; one that does not gets the label,
//! which is what it had.

use egui::Ui;
use tx_verdict::{Art, Basis, Item, Party, TagKind, Tone, TxVerdict, Verb, Viewpoint, asset_name};

use crate::chip::ChipVariant;
use crate::image_loader::{AssetImageSize, iiif_asset_url};
use crate::party_badge::PartyBasis;
use crate::tx_card::{
    self, TxArt, TxCard, TxCardData, TxCardResponse, TxDensity, TxHeadline, TxParty, TxPrint,
    TxVerb, TxViewpoint,
};

pub struct VerdictCard<'v> {
    verdict: &'v TxVerdict,
    /// `(label, thumbnail url)` per print, front-first.
    prints: Vec<(String, String)>,
}

impl<'v> VerdictCard<'v> {
    pub fn new(verdict: &'v TxVerdict) -> Self {
        let prints = match &verdict.art {
            Art::Prints { items } => items
                .iter()
                .map(|item| {
                    (
                        print_label(item),
                        iiif_asset_url(&item.policy, &item.name_hex, AssetImageSize::Thumbnail),
                    )
                })
                .collect(),
            Art::Mark { .. } | Art::None => Vec::new(),
        };
        Self { verdict, prints }
    }

    pub fn verdict(&self) -> &'v TxVerdict {
        self.verdict
    }

    /// Draw it. `walking` is whether a deepening pass is in flight, which
    /// decides what a below-floor party says — see [`TxParty::BelowFloor`].
    pub fn show(
        &self,
        ui: &mut Ui,
        density: TxDensity,
        selected: bool,
        walking: bool,
    ) -> TxCardResponse {
        let v = self.verdict;
        let prints: Vec<TxPrint<'_>> = self
            .prints
            .iter()
            .map(|(label, url)| TxPrint::new(label).image(url))
            .collect();
        let art = match &v.art {
            Art::Prints { .. } => TxArt::Prints(&prints),
            // No venue logo source on the widget side; the mark falls back to
            // the venue's initial, which is what the story shows too.
            Art::Mark { venue } => TxArt::Mark {
                image_url: None,
                label: venue,
            },
            Art::None => TxArt::None,
        };

        let mut headline = TxHeadline::new(&v.headline.value, tone(v.headline.tone));
        if let Some(q) = &v.headline.qualifier {
            headline = headline.qualifier(q);
        }

        let mut data =
            TxCardData::new(&v.kicker, headline, viewpoint(&v.view), v.when_unix).art(art);
        if let Some(s) = &v.subject {
            data = data.subject(s);
        }
        if let Some(c) = &v.caution {
            data = data.caution(c);
        }
        if let Some(f) = &v.footnote {
            data = data.footnote(f);
        }
        for tag in &v.tags {
            data = data.tag(&tag.label, chip(tag.kind));
        }

        TxCard::new(&data, density)
            .selected(selected)
            .walking(walking)
            .show(ui)
    }
}

/// What to call a print: its on-chain name, else the policy's stem. Never raw
/// hex — a reference token and its user token share a name under different
/// labels, and hex would make them look like two assets.
fn print_label(item: &Item) -> String {
    asset_name(&item.name_hex)
        .unwrap_or_else(|| format!("{}…", &item.policy[..8.min(item.policy.len())]))
}

fn party(p: &Party) -> TxParty<'_> {
    match p {
        Party::Known { label, key, basis } => TxParty::Known {
            label,
            key: key.as_deref(),
            basis: party_basis(*basis),
        },
        Party::BelowFloor => TxParty::BelowFloor,
        Party::Ambiguous { count } => TxParty::Ambiguous { count: *count },
    }
}

fn viewpoint(view: &Viewpoint) -> TxViewpoint<'_> {
    match view {
        Viewpoint::Wallet { who, verb, other } => TxViewpoint::Wallet {
            who: TxParty::observed(who),
            verb: tx_verb(*verb),
            other: party(other),
        },
        Viewpoint::Pair { from, to } => TxViewpoint::Pair {
            from: party(from),
            to: party(to),
        },
        Viewpoint::Sole { who } => TxViewpoint::Sole {
            who: TxParty::observed(who),
        },
    }
}

fn party_basis(b: Basis) -> PartyBasis {
    match b {
        Basis::Observed => PartyBasis::Observed,
        Basis::Asserted => PartyBasis::Asserted,
        Basis::Derived => PartyBasis::Derived,
    }
}

fn tx_verb(v: Verb) -> TxVerb {
    match v {
        Verb::Bought => TxVerb::Bought,
        Verb::Sold => TxVerb::Sold,
        Verb::SentTo => TxVerb::SentTo,
        Verb::ReceivedFrom => TxVerb::ReceivedFrom,
    }
}

fn tone(t: Tone) -> tx_card::Tone {
    match t {
        Tone::Positive => tx_card::Tone::Positive,
        Tone::Negative => tx_card::Tone::Negative,
        Tone::Neutral => tx_card::Tone::Neutral,
        Tone::Caution => tx_card::Tone::Caution,
    }
}

/// Which chip a facet gets. Venues yellow — what the app already uses for
/// them — shapes teal, verbs green.
fn chip(kind: TagKind) -> ChipVariant {
    match kind {
        TagKind::Venue => ChipVariant::Warning,
        TagKind::Shape => ChipVariant::Info,
        TagKind::Verb => ChipVariant::Success,
    }
}
