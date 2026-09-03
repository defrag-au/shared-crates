//! `TxCard` — one transaction as a VERDICT: what it was, who it was between,
//! and the one figure that says it — at three densities.
//!
//! ## What this replaces
//!
//! The feed row it supersedes said everything it knew at one type size:
//! `$elchapojr` · `10 ₳ released` · `wayup` · `bought (CO)`, a raw
//! `collection_offer_accepted`, two asset pills, and — as the largest number on
//! the row — `−7.63 ₳`, the wallet's net. Four chips are four co-equal facets
//! where the reader wanted one clause, and the biggest number was the least
//! interesting one: a net is bookkeeping about the WALLET, where `10 ₳` is what
//! actually happened.
//!
//! The social card for the same transaction had already worked this out. It
//! leads with `10 ₳ total` at 104px, names the lot as `2 × MachineHeadz`, says
//! `$boef bought from $elchapojr` as a sentence, and puts the net last, small
//! and grey, for whoever is reconciling against an explorer. This widget is
//! that ranking, in the app.
//!
//! ## Viewpoint is the load-bearing field
//!
//! `flow_route`'s rule — *a link must not pretend to a point of view it does
//! not have* — is a rendering rule, so it lives in the type. The three
//! [`TxViewpoint`] variants differ in what is **sayable**:
//!
//! - [`TxViewpoint::Wallet`] has a subject, so a verb of ownership is available:
//!   "$boef bought from $elchapojr".
//! - [`TxViewpoint::Pair`] has no side. A policy feed watches units move between
//!   two strangers and neither is "us" — so it states the pair and NOTHING ELSE.
//!   There is no buyer from a policy's point of view; there is a party that lost
//!   the unit and a party that gained it.
//! - [`TxViewpoint::Sole`] has one actor and no other side — a mint.
//!
//! A `Pair` cannot be given a verb, because [`TxVerb`] only exists inside
//! `Wallet`. That is the whole reason the viewpoint is an enum with the parties
//! *inside* it rather than a flag beside a fixed pair of party slots.
//!
//! ## Three things this shows that the social card cannot
//!
//! The card is 1200×630 read by a stranger scrolling a chat. This is read by
//! somebody who is about to act on it, so it must not flatten what the card
//! flattens:
//!
//! 1. **How firmly a party is named.** A venue's `seller_stake` is the venue's
//!    word, not an output a walk resolved. [`TxParty::Known`] carries
//!    [`PartyBasis`] positionally so a call site cannot render a party without
//!    stating where the name came from — the same reason `PartyBadge` does.
//! 2. **Which kind of unknown.** [`TxParty::BelowFloor`] resolves by walking
//!    deeper and so PULSES while a pass runs; [`TxParty::Ambiguous`] never
//!    resolves however deep the walk goes, because a batched fill genuinely has
//!    several parties on a side. One blank offering a "deepen" and one blank
//!    refusing to are different promises.
//! 3. **A figure that is provisional.** [`Tone::Caution`] is for a number that
//!    is real but partial, which a card has no state for.
//!
//! ## Density
//!
//! [`TxDensity`] is matched exhaustively everywhere it is consulted — there is
//! no `is_compact()`, because "is it small" is not the question any of these
//! sites are actually asking. Each one asks something specific (how big is the
//! art, does the footnote appear at all) and answers it per variant.
//!
//! - [`TxDensity::Row`] — the feed unit. Art at 44px, unfanned: at that size a
//!   fan is a smear, and a lone print is never tilted anyway.
//! - [`TxDensity::Feature`] — the selected row, or the top of a feed.
//! - [`TxDensity::Poster`] — the transaction's own page, and the share preview:
//!   *this is what people see if you post this link*.
//!
//! ## Domain-free
//!
//! Amounts arrive as preformatted strings and asset names arrive resolved —
//! hex decoded, CIP-67 label stripped — because that is chain-specific work the
//! widget must not guess at. The same convention [`crate::activity_feed`]
//! follows, and the reason both can be driven from a wallet-relative row and a
//! policy-relative one without knowing that either exists.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{TxCard, TxCardData, TxDensity, TxHeadline, Tone};
//!
//! let resp = TxCard::new(&data, TxDensity::Row).show(ui);
//! if resp.clicked { open_tx(&data); }
//! if let Some(party) = resp.walk { follow_the_money(party); }
//! ```

use egui::{
    Align, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Pos2, Rect, Response,
    RichText, Sense, Shape, Stroke, Ui, Vec2, emath::Rot2,
};

use crate::icons::PhosphorIcon;
use crate::party_badge::{PartyBadge, PartyBasis};
use crate::relative_time::RelativeTime;
use crate::theme;

// ============================================================================
// Density
// ============================================================================

/// How much room this card has, and therefore how much of the verdict survives.
///
/// Not a size in pixels: each variant is a different EDIT of the same verdict.
/// A row drops the footnote entirely rather than shrinking it, because a
/// reconciliation line nobody can read is worse than one that is not there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxDensity {
    /// The feed unit. Three lines, art at 44px, no fan.
    Row,
    /// The selected row, or the top of a feed. Art fans.
    Feature,
    /// The transaction's own page — and the share preview.
    Poster,
}

impl TxDensity {
    /// Every density, for a storybook or a density picker.
    pub const ALL: [TxDensity; 3] = [TxDensity::Row, TxDensity::Feature, TxDensity::Poster];

    pub fn label(self) -> &'static str {
        match self {
            TxDensity::Row => "Row",
            TxDensity::Feature => "Feature",
            TxDensity::Poster => "Poster",
        }
    }

    /// The front print's edge length.
    fn art_size(self) -> f32 {
        match self {
            TxDensity::Row => 30.0,
            TxDensity::Feature => 56.0,
            TxDensity::Poster => 130.0,
        }
    }

    /// Does the pile fan, mount and tilt?
    ///
    /// Only where there is room for the fan to READ. At 44px the peek of a
    /// buried print is eight pixels of white, which is noise rather than a
    /// second item — so a row shows the front print alone and lets the subject
    /// line carry the count.
    fn fans(self) -> bool {
        match self {
            TxDensity::Row => false,
            TxDensity::Feature | TxDensity::Poster => true,
        }
    }

    /// The headline, sized against the APP's scale rather than the social
    /// card's.
    ///
    /// The card these borrow their ranking from sets its figure at 104px — but
    /// that is 1200px wide and read at thumbnail size in somebody else's chat,
    /// where the number has to survive being ~8% of the image. In the app it is
    /// read at arm's length beside a 14px body and a 20px heading, and the same
    /// ratio lands as a billboard. What ports is the HIERARCHY — headline above
    /// subject above party above footnote — not the multiplier.
    ///
    /// So even the poster's figure is only twice the app's largest text, and a
    /// row's is barely above body. Rank is carried by weight, colour and order;
    /// size only has to break the tie.
    fn headline_size(self) -> f32 {
        match self {
            TxDensity::Row => 15.0,
            TxDensity::Feature => 22.0,
            TxDensity::Poster => 40.0,
        }
    }

    fn subject_size(self) -> f32 {
        match self {
            TxDensity::Row => 13.0,
            TxDensity::Feature => 15.0,
            TxDensity::Poster => 22.0,
        }
    }

    fn kicker_size(self) -> f32 {
        match self {
            TxDensity::Row => 10.0,
            TxDensity::Feature => 11.0,
            TxDensity::Poster => 14.0,
        }
    }

    fn body_size(self) -> f32 {
        match self {
            TxDensity::Row => 11.0,
            TxDensity::Feature => 12.0,
            TxDensity::Poster => 14.0,
        }
    }

    fn padding(self) -> i8 {
        match self {
            TxDensity::Row => 6,
            TxDensity::Feature => 10,
            TxDensity::Poster => 20,
        }
    }

    /// Where the headline sits.
    ///
    /// A row puts it in a right-hand column so a column of rows has a column of
    /// figures to scan down. The larger densities put it inline under the
    /// kicker, where it is the first thing read rather than the last.
    fn headline_is_columnar(self) -> bool {
        match self {
            TxDensity::Row => true,
            TxDensity::Feature | TxDensity::Poster => false,
        }
    }

    /// Does the reconciliation footnote appear at all?
    fn shows_footnote(self) -> bool {
        match self {
            TxDensity::Row => false,
            TxDensity::Feature | TxDensity::Poster => true,
        }
    }

    /// Does the absolute timestamp appear beside the relative one?
    fn shows_absolute_time(self) -> bool {
        match self {
            TxDensity::Row => false,
            TxDensity::Feature | TxDensity::Poster => true,
        }
    }
}

// ============================================================================
// Tone
// ============================================================================

/// What a figure argues, as a colour.
///
/// NOT a sign. `−7.63 ₳` on a purchase is negative and completely ordinary,
/// where a partial figure is unsigned and wants a warning. The caller decides
/// what the number MEANS; the widget only renders the verdict it is handed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Money in, or a mint. Green.
    Positive,
    /// Money out. Red.
    Negative,
    /// A count, a breakdown, a fact with no direction.
    Neutral,
    /// Real but provisional — a partial window, a capped row. Amber.
    Caution,
}

impl Tone {
    fn color(self) -> Color32 {
        match self {
            Tone::Positive => theme::SUCCESS,
            Tone::Negative => theme::ERROR,
            Tone::Neutral => theme::TEXT_PRIMARY,
            Tone::Caution => theme::ACCENT_ORANGE,
        }
    }
}

// ============================================================================
// Parties
// ============================================================================

/// Who was on one side — or which kind of nobody.
///
/// The two absences are NOT interchangeable and must not be collapsed into an
/// `Option`. See the module docs.
#[derive(Clone, Debug)]
pub enum TxParty<'a> {
    /// A named party. `basis` is positional so it cannot be forgotten.
    Known {
        label: &'a str,
        /// The raw key, shown on hover so a handle can be checked.
        key: Option<&'a str>,
        basis: PartyBasis,
    },
    /// Not resolved YET — the source sits below the walk floor, and walking
    /// deeper finds it. Renders as a pulsing placeholder.
    BelowFloor,
    /// Never resolves, however deep the walk goes: a batched fill has several
    /// parties on this side. Renders as a statement, never as a placeholder,
    /// and never with a "deepen" offer attached.
    Ambiguous { count: usize },
}

impl<'a> TxParty<'a> {
    /// A party the chain itself resolved.
    pub fn observed(label: &'a str) -> Self {
        TxParty::Known {
            label,
            key: None,
            basis: PartyBasis::Observed,
        }
    }

    /// A party a VENUE named — its word, not an output a walk resolved.
    pub fn derived(label: &'a str) -> Self {
        TxParty::Known {
            label,
            key: None,
            basis: PartyBasis::Derived,
        }
    }

    pub fn key(mut self, k: &'a str) -> Self {
        if let TxParty::Known { key, .. } = &mut self {
            *key = Some(k);
        }
        self
    }

    /// The label, when there is one to follow. `None` for either absence —
    /// which is what stops a feed offering to walk into a party it cannot name.
    fn walkable(&self) -> Option<&'a str> {
        match self {
            TxParty::Known { label, .. } => Some(label),
            TxParty::BelowFloor | TxParty::Ambiguous { .. } => None,
        }
    }
}

/// What this wallet DID, when there is a wallet whose side we are on.
///
/// Only reachable inside [`TxViewpoint::Wallet`], because a verb of ownership
/// needs somebody to own. Derived from which way the ITEMS went, never from
/// which way the money went — a collection offer accepted by the other side has
/// this wallet paying *and* receiving, so the money says "to them" under a pile
/// of NFTs that arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxVerb {
    Bought,
    Sold,
    /// Assets left, with no market verdict — a plain send.
    SentTo,
    /// Assets arrived, with no market verdict.
    ReceivedFrom,
}

impl TxVerb {
    /// The clause between the two parties: `$boef` **bought from** `$elchapojr`.
    fn phrase(self) -> &'static str {
        match self {
            TxVerb::Bought => "bought from",
            TxVerb::Sold => "sold to",
            TxVerb::SentTo => "sent to",
            TxVerb::ReceivedFrom => "received from",
        }
    }
}

/// Whose point of view this card is drawn from — and therefore what it may say.
#[derive(Clone, Debug)]
pub enum TxViewpoint<'a> {
    /// One wallet's own side. A verb is available.
    Wallet {
        who: TxParty<'a>,
        verb: TxVerb,
        other: TxParty<'a>,
    },
    /// No side at all: a policy watching a unit move between two strangers.
    /// States the pair, symmetrically, with no verb of ownership.
    Pair {
        from: TxParty<'a>,
        to: TxParty<'a>,
    },
    /// One actor and no other side — a mint into existence.
    Sole { who: TxParty<'a> },
}

// ============================================================================
// Headline + art
// ============================================================================

/// The one figure that says what this was.
#[derive(Clone, Debug)]
pub struct TxHeadline<'a> {
    /// Preformatted — `10 ₳`, `2 × MachineHeadz`, `6 listed · 2 offers made`.
    /// The host owns formatting because it owns the decimals and the symbol.
    pub value: &'a str,
    /// A word after the figure — `total`. Only ever for a LOT, where `10 ₳`
    /// beside `2 × MachineHeadz` reads as ten each about as readily as ten for
    /// the pair.
    pub qualifier: Option<&'a str>,
    pub tone: Tone,
}

impl<'a> TxHeadline<'a> {
    pub fn new(value: &'a str, tone: Tone) -> Self {
        Self {
            value,
            qualifier: None,
            tone,
        }
    }

    pub fn qualifier(mut self, q: &'a str) -> Self {
        self.qualifier = Some(q);
        self
    }
}

/// One print in the pile.
#[derive(Clone, Debug)]
pub struct TxPrint<'a> {
    pub image_url: Option<&'a str>,
    /// Resolved display name — used for the fallback initial and the hover.
    pub label: &'a str,
}

impl<'a> TxPrint<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            image_url: None,
            label,
        }
    }

    pub fn image(mut self, url: &'a str) -> Self {
        self.image_url = Some(url);
        self
    }
}

/// What the card leads with, visually.
#[derive(Clone, Debug)]
pub enum TxArt<'a> {
    /// The traded items, front-first, fanned as prints.
    ///
    /// A two-item sale drawn as ONE picture reads as a one-item sale — the
    /// count line says otherwise, but the picture is what a reader takes in
    /// first and it quietly contradicts it.
    Prints(&'a [TxPrint<'a>]),
    /// A venue's mark. For a batch no single item represents: six listings, two
    /// offers and a cancellation in one submission, where picking one item's
    /// artwork captions nine others.
    Mark {
        image_url: Option<&'a str>,
        label: &'a str,
    },
    /// Nothing to show — a plain movement of ADA.
    None,
}

/// Prints drawn before the fan stops being legible.
///
/// Three. Beyond that the pile is a smear and the count carries the meaning
/// anyway — nobody tells nine prints from twelve by looking, but they do read
/// "12 items".
const STACK_MAX: usize = 3;

/// House tilt angles, in degrees, indexed FRONT-FIRST.
///
/// Fixed rather than random: a row is re-laid every frame, so "random" would
/// mean "jitters while you look at it". The front sits almost straight so the
/// subject reads cleanly; the ones behind lean enough to look dropped rather
/// than filed.
const TILT_DEG: [f32; STACK_MAX] = [-1.5, 5.0, -7.0];

/// The white border that makes a thumbnail read as a PRINT rather than as a
/// picture with a line round it.
const MOUNT_RATIO: f32 = 0.07;

/// How much of each buried print stays visible past the one in front.
const PEEK_RATIO: f32 = 0.26;

/// Photographic paper, not pure white — pure white against this background
/// glares and pulls focus off the artwork it is framing.
const MOUNT_FILL: Color32 = Color32::from_rgb(244, 244, 239);

// ============================================================================
// Data
// ============================================================================

/// One transaction, already reduced to a verdict.
///
/// Every field is the ANSWER to a question, not a fact to be ranked later: the
/// ranking happened upstream, which is the entire point of the type. A caller
/// holding a wallet-relative row and a caller holding a policy-relative one
/// both arrive here, and the widget cannot tell which.
#[derive(Clone, Debug)]
pub struct TxCardData<'a> {
    /// What kind of thing this was — `collection offer accepted · wayup`.
    /// Muted and small: it LABELS the headline, so it must not compete with it.
    pub kicker: &'a str,
    pub headline: TxHeadline<'a>,
    /// What was traded — `2 × MachineHeadz`.
    ///
    /// A lot is NEVER named after one of its members: "MachineHeadz527" above a
    /// line counting two says the sale was #527 and something else came along.
    /// Where the members share a stem that stem is the honest thing to say;
    /// where they do not, a bare count is.
    pub subject: Option<&'a str>,
    pub view: TxViewpoint<'a>,
    /// Unix seconds.
    pub when: i64,
    pub art: TxArt<'a>,
    /// The thing that changes the reading — "2 items in this transaction",
    /// "figures are partial". Amber, and it survives at every density above
    /// `Row` because a caution that gets edited out is not a caution.
    pub caution: Option<&'a str>,
    /// The reconciliation line — `wallet net −7.6295 ₳`. True, and not the
    /// point: it is here for whoever is checking against an explorer, which is
    /// why it is last, grey, and the first thing a row drops.
    pub footnote: Option<&'a str>,
}

impl<'a> TxCardData<'a> {
    pub fn new(kicker: &'a str, headline: TxHeadline<'a>, view: TxViewpoint<'a>, when: i64) -> Self {
        Self {
            kicker,
            headline,
            subject: None,
            view,
            when,
            art: TxArt::None,
            caution: None,
            footnote: None,
        }
    }

    pub fn subject(mut self, s: &'a str) -> Self {
        self.subject = Some(s);
        self
    }

    pub fn art(mut self, art: TxArt<'a>) -> Self {
        self.art = art;
        self
    }

    pub fn caution(mut self, c: &'a str) -> Self {
        self.caution = Some(c);
        self
    }

    pub fn footnote(mut self, f: &'a str) -> Self {
        self.footnote = Some(f);
        self
    }
}

// ============================================================================
// Widget
// ============================================================================

/// What the reader did with the card.
pub struct TxCardResponse {
    /// The card body was clicked — open the transaction.
    pub clicked: bool,
    /// A PARTY was clicked. Reported separately from `clicked` because
    /// following money out is a different move from opening the transaction,
    /// and a feed that can only open its own rows is a dead end at every hop.
    pub walk: Option<String>,
    /// The reader asked to walk deeper, from a [`TxParty::BelowFloor`] slot.
    /// Never offered for [`TxParty::Ambiguous`], which deepening cannot fix.
    pub deepen: bool,
    pub response: Response,
}

pub struct TxCard<'a> {
    data: &'a TxCardData<'a>,
    density: TxDensity,
    selected: bool,
    walkable: bool,
    /// Is a deepening pass in flight? Decides whether a `BelowFloor` slot reads
    /// "wait" (pulsing) or "reach further back" (an offer).
    walking: bool,
    /// Pinned "now", for a story that must render identically every frame.
    /// Live clock when absent — see [`RelativeTime`].
    now: Option<i64>,
}

impl<'a> TxCard<'a> {
    pub fn new(data: &'a TxCardData<'a>, density: TxDensity) -> Self {
        Self {
            data,
            density,
            selected: false,
            walkable: true,
            walking: false,
            now: None,
        }
    }

    /// Pin "now" so a story renders the same every frame.
    pub fn now(mut self, now_secs: i64) -> Self {
        self.now = Some(now_secs);
        self
    }

    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }

    /// Are parties links? Off where there is nowhere to walk TO.
    pub fn walkable(mut self, w: bool) -> Self {
        self.walkable = w;
        self
    }

    pub fn walking(mut self, w: bool) -> Self {
        self.walking = w;
        self
    }

    pub fn show(self, ui: &mut Ui) -> TxCardResponse {
        let d = self.density;
        let mut walk = None;
        let mut deepen = false;

        let stroke = match self.selected {
            true => theme::stroke(1.0, theme::ACCENT),
            false => theme::hairline(theme::BORDER),
        };

        let inner = Frame::new()
            .fill(theme::BG_SECONDARY)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(d.padding()))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    paint_art(ui, &self.data.art, d);
                    ui.add_space(match d {
                        TxDensity::Row => 8.0,
                        TxDensity::Feature => 14.0,
                        TxDensity::Poster => 30.0,
                    });

                    // The headline column is claimed from the RIGHT first, so
                    // the text column gets whatever is left and wraps into it
                    // rather than pushing the figure off the card. That
                    // ordering is what stops a long handle collapsing the
                    // amount column to zero width.
                    let columnar = d.headline_is_columnar();
                    if columnar {
                        let w = ui.available_width();
                        ui.allocate_ui_with_layout(
                            Vec2::new(w, 0.0),
                            Layout::right_to_left(Align::TOP),
                            |ui| {
                                ui.vertical(|ui| {
                                    ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
                                        headline(ui, &self.data.headline, d);
                                    });
                                });
                                ui.vertical(|ui| {
                                    let r = text_column(
                                        ui,
                                        self.data,
                                        d,
                                        self.walkable,
                                        self.walking,
                                        self.now,
                                    );
                                    walk = walk.take().or(r.0);
                                    deepen |= r.1;
                                });
                            },
                        );
                    } else {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(self.data.kicker)
                                    .size(d.kicker_size())
                                    .color(theme::TEXT_MUTED),
                            );
                            headline(ui, &self.data.headline, d);
                            let r =
                                text_column(ui, self.data, d, self.walkable, self.walking, self.now);
                            walk = walk.take().or(r.0);
                            deepen |= r.1;
                        });
                    }
                });
            });

        let response = inner.response.interact(Sense::click());
        TxCardResponse {
            clicked: response.clicked(),
            walk,
            deepen,
            response,
        }
    }
}

/// The stacked text: subject, party clause, time, caution, footnote.
///
/// Returns `(walked party, asked to deepen)`.
fn text_column(
    ui: &mut Ui,
    data: &TxCardData<'_>,
    d: TxDensity,
    walkable: bool,
    walking: bool,
    now: Option<i64>,
) -> (Option<String>, bool) {
    // A row's kicker lives here rather than above the headline: the headline is
    // in its own right-hand column, so a kicker over it would label a figure
    // sitting somewhere else on the card.
    if d.headline_is_columnar() {
        ui.label(
            RichText::new(data.kicker)
                .size(d.kicker_size())
                .color(theme::TEXT_MUTED),
        );
    }

    if let Some(subject) = data.subject {
        ui.label(
            RichText::new(subject)
                .size(d.subject_size())
                .color(theme::TEXT_PRIMARY)
                .strong(),
        );
    }

    let (walk, deepen) = party_clause(ui, &data.view, d, walkable, walking);

    // Time. WORDS FIRST — "1h ago" is what a reader scanning a feed actually
    // uses; the absolute stamp is for the record and only earns its space once
    // there is space.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let mut rel = RelativeTime::new(data.when)
            .size(d.body_size())
            .color(theme::TEXT_MUTED);
        if let Some(now) = now {
            rel = rel.now(now);
        }
        ui.add(rel);
        if d.shows_absolute_time() {
            ui.label(
                RichText::new(format!("· {}", iso_utc(data.when)))
                    .size(d.body_size())
                    .color(theme::TEXT_MUTED),
            );
        }
    });

    if let Some(caution) = data.caution {
        ui.label(
            RichText::new(caution)
                .size(d.body_size())
                .color(theme::ACCENT_ORANGE),
        );
    }

    if d.shows_footnote() {
        if let Some(footnote) = data.footnote {
            ui.label(
                RichText::new(footnote)
                    .size(d.body_size())
                    .color(theme::TEXT_MUTED),
            );
        }
    }

    (walk, deepen)
}

/// `$boef bought from $elchapojr`, or `$a → $b`, or one lone party.
fn party_clause(
    ui: &mut Ui,
    view: &TxViewpoint<'_>,
    d: TxDensity,
    walkable: bool,
    walking: bool,
) -> (Option<String>, bool) {
    let mut walk = None;
    let mut deepen = false;
    let size = d.body_size();

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        match view {
            TxViewpoint::Wallet { who, verb, other } => {
                let (w, dp) = party(ui, who, size, walkable, walking);
                walk = walk.take().or(w);
                deepen |= dp;
                ui.label(
                    RichText::new(verb.phrase())
                        .size(size)
                        .color(theme::TEXT_MUTED),
                );
                let (w, dp) = party(ui, other, size, walkable, walking);
                walk = walk.take().or(w);
                deepen |= dp;
            }
            TxViewpoint::Pair { from, to } => {
                let (w, dp) = party(ui, from, size, walkable, walking);
                walk = walk.take().or(w);
                deepen |= dp;
                // WORDS OR AN ICON, never a raw `→`: the arrow codepoint is not
                // in the default face and renders as tofu.
                ui.label(PhosphorIcon::ArrowRight.rich_text(size, theme::TEXT_MUTED));
                let (w, dp) = party(ui, to, size, walkable, walking);
                walk = walk.take().or(w);
                deepen |= dp;
            }
            TxViewpoint::Sole { who } => {
                let (w, dp) = party(ui, who, size, walkable, walking);
                walk = walk.take().or(w);
                deepen |= dp;
            }
        }
    });

    (walk, deepen)
}

/// One party slot — a badge, a pulsing placeholder, or a statement.
fn party(
    ui: &mut Ui,
    p: &TxParty<'_>,
    size: f32,
    walkable: bool,
    walking: bool,
) -> (Option<String>, bool) {
    match p {
        TxParty::Known { label, key, basis } => {
            let mut badge = PartyBadge::new(label, *basis).text_size(size);
            if let Some(k) = key {
                badge = badge.key(k);
            }
            let resp = badge.show(ui);
            // `walkable()` is the ONE definition of "is there anywhere to go
            // from here" — consulted rather than re-derived, so an absence
            // cannot become clickable by way of a branch added above it.
            let target = match walkable && resp.clicked() {
                true => p.walkable().map(str::to_string),
                false => None,
            };
            (target, false)
        }
        TxParty::BelowFloor => {
            // PULSES while a pass is running, because waiting is the right
            // instruction; offers a deepen when nothing is, because reaching
            // further back is. Two different sentences, one state.
            match walking {
                true => {
                    ui.ctx().request_repaint();
                    let t = ui.input(|i| i.time) as f32;
                    let a = 110.0 + 90.0 * (t * 2.2).sin();
                    ui.label(
                        RichText::new("source below floor")
                            .size(size)
                            .italics()
                            .color(theme::TEXT_MUTED.gamma_multiply(a / 200.0)),
                    );
                    (None, false)
                }
                false => {
                    let r = ui.link(
                        RichText::new("source below floor — reach further back")
                            .size(size)
                            .italics()
                            .color(theme::ACCENT),
                    );
                    (None, r.clicked())
                }
            }
        }
        TxParty::Ambiguous { count } => {
            // A STATEMENT, never a placeholder, and never with a deepen offer:
            // this one does not resolve however deep the walk goes.
            ui.label(
                RichText::new(format!("{count} parties on this side"))
                    .size(size)
                    .italics()
                    .color(theme::TEXT_MUTED),
            )
            .on_hover_text(
                "A batched fill has several parties on a side. Walking deeper does not \
                 resolve this — there is no single counterparty to find.",
            );
            (None, false)
        }
    }
}

/// The headline figure, with the digits in a tabular face.
///
/// # Why the value is split
///
/// Numbers in a feed are read against each other, so the digits want tabular
/// figures — which is what the monospace family is for. But `₳` (U+20B3) is not
/// in every mono face, and a headline that falls back to tofu is the worst
/// possible place for a missing glyph. So the digits go to mono and everything
/// after the last space goes to the proportional face, which is the one that
/// demonstrably has the symbol.
///
/// A value with no space in it renders whole, proportionally — the safe
/// direction to be wrong in.
fn headline(ui: &mut Ui, h: &TxHeadline<'_>, d: TxDensity) {
    let size = d.headline_size();
    let colour = h.tone.color();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = size * 0.12;
        match h.value.rsplit_once(' ') {
            Some((digits, symbol)) if digits.chars().any(|c| c.is_ascii_digit()) => {
                ui.label(
                    RichText::new(digits)
                        .font(FontId::new(size, FontFamily::Monospace))
                        .color(colour),
                );
                ui.label(RichText::new(symbol).size(size).color(colour));
            }
            _ => {
                ui.label(RichText::new(h.value).size(size).color(colour));
            }
        }
        if let Some(q) = h.qualifier {
            ui.label(
                RichText::new(q)
                    .size(size * 0.34)
                    .color(theme::TEXT_MUTED),
            );
        }
    });
}

// ============================================================================
// Art
// ============================================================================

fn paint_art(ui: &mut Ui, art: &TxArt<'_>, d: TxDensity) {
    match art {
        TxArt::Prints(prints) => paint_prints(ui, prints, d),
        TxArt::Mark { image_url, label } => paint_mark(ui, *image_url, label, d),
        TxArt::None => {}
    }
}

/// The traded items as a fanned deck of prints.
///
/// # How it overlaps
///
/// Painter-level, back to front, because the mount has to rotate WITH its
/// image — a rotated picture inside an upright white border reads as a mistake.
/// Each print is a rotated quad in paper-white with a rotated shadow beneath
/// and the image rotated about the same centre.
///
/// A LONE PRINT IS NEVER TILTED. The tilt says "there are more of these behind";
/// with nothing behind it, it is just a crooked picture, and the artwork is the
/// thing the reader is trying to look at.
fn paint_prints(ui: &mut Ui, prints: &[TxPrint<'_>], d: TxDensity) {
    let shown: Vec<&TxPrint<'_>> = match d.fans() {
        true => prints.iter().take(STACK_MAX).collect(),
        // A row shows the front print alone: at 44px the peek is a few pixels
        // of white, which is noise rather than a second item.
        false => prints.iter().take(1).collect(),
    };
    if shown.is_empty() {
        return;
    }

    let size = d.art_size();
    let mount = (size * MOUNT_RATIO).max(2.0);
    let peek = size * PEEK_RATIO;
    let single = shown.len() == 1;
    let lift_step = size * 0.05;

    let n = shown.len() as f32;
    let width = size + 2.0 * mount + (n - 1.0) * peek;
    // Slack for the tilt and the lift: a rotated quad's corners reach past the
    // box it would otherwise occupy, and clipping the pile is the one thing
    // that makes the whole treatment look broken.
    let slack = match single {
        true => 0.0,
        false => size * 0.18,
    };
    let height = size + 2.0 * mount + slack;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();

    // BACK TO FRONT. Later shapes paint over earlier ones, so the list is
    // walked in reverse: prints[0] is the one a reader looks at, and it should
    // not be the one buried.
    for (i, print) in shown.iter().enumerate().rev() {
        let from_back = shown.len() - 1 - i;
        let tilt = match single {
            true => 0.0,
            false => TILT_DEG.get(i).copied().unwrap_or(0.0),
        };
        let angle = tilt.to_radians();
        let rot = Rot2::from_angle(angle);

        let cx = rect.left() + mount + size / 2.0 + from_back as f32 * peek;
        let cy = rect.top() + mount + size / 2.0 + i as f32 * lift_step;
        let center = Pos2::new(cx, cy);

        // The mount: a rotated quad in paper-white. It is the separating edge
        // between overlapping prints as well as the frame around each one.
        let half = size / 2.0 + mount;
        let quad = |c: Pos2, h: f32| -> Vec<Pos2> {
            [(-h, -h), (h, -h), (h, h), (-h, h)]
                .iter()
                .map(|(x, y)| c + rot * Vec2::new(*x, *y))
                .collect()
        };

        if !single {
            painter.add(Shape::convex_polygon(
                quad(center + Vec2::new(0.0, size * 0.035), half),
                Color32::from_black_alpha(120),
                Stroke::NONE,
            ));
        }
        painter.add(Shape::convex_polygon(
            quad(center, half),
            MOUNT_FILL,
            Stroke::NONE,
        ));

        let img_rect = Rect::from_center_size(center, Vec2::splat(size));
        match print.image_url {
            Some(url) => {
                egui::Image::new(url)
                    .rotate(angle, Vec2::splat(0.5))
                    .corner_radius(CornerRadius::same(2))
                    .paint_at(ui, img_rect);
            }
            // No image loader installed, or no artwork: a tinted initial, so
            // the pile still reads as a pile of somethings.
            None => {
                painter.add(Shape::convex_polygon(
                    quad(center, size / 2.0),
                    theme::BG_HIGHLIGHT,
                    Stroke::NONE,
                ));
                if let Some(ch) = print.label.chars().next() {
                    painter.text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        ch.to_uppercase().to_string(),
                        FontId::proportional(size * 0.4),
                        theme::TEXT_MUTED,
                    );
                }
            }
        }
    }
}

/// A venue's mark, for a batch no single item represents.
fn paint_mark(ui: &mut Ui, image_url: Option<&str>, label: &str, d: TxDensity) {
    let size = d.art_size();
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), theme::BG_HIGHLIGHT);
    match image_url {
        Some(url) => {
            egui::Image::new(url)
                .corner_radius(CornerRadius::same(6))
                .paint_at(ui, rect);
        }
        None => {
            if let Some(ch) = label.chars().next() {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    ch.to_uppercase().to_string(),
                    FontId::proportional(size * 0.45),
                    theme::TEXT_MUTED,
                );
            }
        }
    }
}

// ============================================================================
// Time
// ============================================================================

/// `2026-09-03 04:07:29` — the absolute stamp, UTC, no zone suffix at this
/// size because every timestamp on the surface is UTC and repeating it on each
/// row is noise.
fn iso_utc(unix: i64) -> String {
    // Civil-from-days (Howard Hinnant's algorithm) — no chrono, because this
    // crate is compiled for wasm and a date formatter is not worth a
    // dependency that drags in a time zone database.
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dd = doy - (153 * mp + 2) / 5 + 1;
    let mm = if mp < 10 { mp + 3 } else { mp - 9 };
    let yy = if mm <= 2 { y + 1 } else { y };
    format!(
        "{yy:04}-{mm:02}-{dd:02} {:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_utc_matches_known_stamps() {
        // The MachineHeadz collection-offer fill this widget was designed
        // against: slot-derived unix 1788408449.
        assert_eq!(iso_utc(1_788_408_449), "2026-09-03 04:07:29");
        assert_eq!(iso_utc(0), "1970-01-01 00:00:00");
        // A leap day, which the civil-from-days branch gets wrong if the
        // March-based year shift is dropped.
        assert_eq!(iso_utc(1_709_164_800), "2024-02-29 00:00:00");
    }

    /// A `Pair` has no verb slot to fill. This is a compile-time property, not
    /// a runtime one — the test exists so that a future refactor flattening the
    /// viewpoint into `(from, to, Option<verb>)` fails here rather than
    /// silently allowing "a policy bought something".
    #[test]
    fn pair_viewpoint_cannot_carry_a_verb() {
        let v = TxViewpoint::Pair {
            from: TxParty::observed("$a"),
            to: TxParty::derived("$b"),
        };
        match v {
            TxViewpoint::Pair { .. } => {}
            TxViewpoint::Wallet { .. } | TxViewpoint::Sole { .. } => unreachable!(),
        }
    }

    #[test]
    fn absences_are_never_walkable() {
        assert!(TxParty::BelowFloor.walkable().is_none());
        assert!(TxParty::Ambiguous { count: 4 }.walkable().is_none());
        assert_eq!(TxParty::observed("$boef").walkable(), Some("$boef"));
    }

    /// Every density has to answer every question. A new variant that forgets
    /// one is a compile error at the `match`, and this asserts the set is
    /// actually walked rather than trusting `ALL` to be complete.
    #[test]
    fn all_densities_are_distinct() {
        let sizes: Vec<f32> = TxDensity::ALL.iter().map(|d| d.art_size()).collect();
        assert_eq!(sizes.len(), 3);
        assert!(sizes[0] < sizes[1] && sizes[1] < sizes[2]);
    }
}
