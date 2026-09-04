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
//! [`TxDensity`] decides SIZES per variant, and maps once to a [`TxEdit`] that
//! decides what SURVIVES. There is no `is_compact()`: the first cut of this had
//! three boolean predicates on the density that all split it the same way and
//! was about to grow three more, which is one decision asked six times. Now
//! the split is made in one place and every site matches on the named result.
//!
//! - [`TxDensity::Row`] — the feed unit, on the tight edit: three lines at
//!   ~65px. Art at 30px, and still fanned: at the tuned pile style the buried
//!   prints are corners, and corners survive 30px.
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
    Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Response, RichText, Sense, Ui,
    Vec2,
};

use crate::chip::{Chip, ChipVariant};
use crate::icons::PhosphorIcon;
use crate::image_stack::{ImageStack, StackImage};
use crate::party_badge::PartyBasis;
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
    /// The feed unit. Art at 30px, fanned like the others.
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

    /// Which EDIT of the verdict this density gets. Decided once, here;
    /// matched everywhere else.
    fn edit(self) -> TxEdit {
        match self {
            TxDensity::Row => TxEdit::Tight,
            TxDensity::Feature | TxDensity::Poster => TxEdit::Full,
        }
    }
}

/// The two edits of a verdict — what survives, and where it sits.
///
/// # Why this exists instead of six booleans
///
/// There were `headline_is_columnar()`, `shows_footnote()` and
/// `shows_absolute_time()`, and the row cut was about to add three more. Six
/// predicates on one enum that all split it the same way are one decision
/// asked six times — the `is_compact()` shape — and the first one somebody
/// adds that splits it *differently* is a bug nobody can see from the call
/// sites. So the density maps to a named edit ONCE and every site matches on
/// that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxEdit {
    /// THREE LINES: subject / party + time / facets. For a feed of hundreds.
    ///
    /// - The headline goes to a right-hand column, so a column of rows is a
    ///   column of figures to scan down.
    /// - No kicker: what it says — venue, shape — is on the chips already.
    /// - The relative time hangs off the party clause rather than taking a
    ///   line, and the absolute stamp is dropped.
    /// - The caution moves into the chip row as an amber note. Not a chip: it
    ///   is not a facet anyone filters by, and it keeps the colour it has at
    ///   every other density so it means the same thing everywhere.
    /// - No footnote. A reconciliation line nobody can read is worse than
    ///   none; it is on the Feature card one click away.
    Tight,
    /// The whole stack: kicker, headline, subject, party, time with its
    /// absolute stamp, caution, footnote, chips.
    Full,
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

    /// Where a click goes: the KEY when one was given, else the label. `None`
    /// for either absence — which is what stops a feed offering to walk into
    /// a party it cannot name.
    ///
    /// The key is what makes a handle clickable in a feed: `$elchapojr` is
    /// the label, and the stake behind it is where the walk has to go.
    fn walkable(&self) -> Option<&'a str> {
        match self {
            TxParty::Known { label, key, .. } => Some(key.unwrap_or(label)),
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
    Pair { from: TxParty<'a>, to: TxParty<'a> },
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

// Every proportion of the pile — mount, peek, tilt, shadow — belongs to
// `image_stack::ImageStackStyle`, which has a slider bench behind it. Nothing
// about the treatment is decided here.

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
    /// Filterable facets — venue, shape, annotation.
    ///
    /// # Why these survived the redesign
    ///
    /// The row this replaces was *only* chips: `$elchapojr` · `10 ₳ released` ·
    /// `wayup` · `bought (CO)`, four co-equal tags standing in for a sentence
    /// nobody had written. Deleting them was the wrong correction. A chip is a
    /// FACET — a thing you click to filter a feed down to — and that is a
    /// capability the surface wants; what was wrong was chips doing the work of
    /// the verdict as well.
    ///
    /// So both, with the jobs separated: the kicker, headline and party clause
    /// say what happened, and the chips are what you can slice the feed by.
    /// They sit LAST for that reason — they are navigation, not narration.
    pub tags: Vec<(&'a str, ChipVariant)>,
    /// The thing that changes the reading — "2 items in this transaction",
    /// "figures are partial". Amber at every density, because a caution that
    /// gets edited out is not a caution; a row has no line to spare for it, so
    /// there it leads the chip row instead. Keep it SHORT — on a row it sits
    /// beside the chips, and a sentence there pushes them off the card.
    pub caution: Option<&'a str>,
    /// The reconciliation line — `wallet net −7.6295 ₳`. True, and not the
    /// point: it is here for whoever is checking against an explorer, which is
    /// why it is last, grey, and the first thing a row drops.
    pub footnote: Option<&'a str>,
}

impl<'a> TxCardData<'a> {
    pub fn new(
        kicker: &'a str,
        headline: TxHeadline<'a>,
        view: TxViewpoint<'a>,
        when: i64,
    ) -> Self {
        Self {
            kicker,
            headline,
            subject: None,
            view,
            when,
            art: TxArt::None,
            tags: Vec::new(),
            caution: None,
            footnote: None,
        }
    }

    /// Add a filterable facet — a venue, a shape, an annotation.
    pub fn tag(mut self, label: &'a str, variant: ChipVariant) -> Self {
        self.tags.push((label, variant));
        self
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
    /// A TAG was clicked — the label to filter the feed down to. Reported
    /// separately again, because slicing a feed is a third distinct move from
    /// opening a row or following its money.
    pub filtered: Option<String>,
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
        let mut filtered = None;

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
                // FULL WIDTH, ALWAYS. A `Frame` shrinks to its content, so a
                // feed of these came out ragged — 620px for a long venue
                // breakdown, 300px for a mint — with no shared left or right
                // edge. Rows in a list are read as a column; anything that
                // makes them individually shaped reads as broken layout rather
                // than as varying content. A caller wanting a narrow card puts
                // it in a narrow `Ui`.
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    paint_art(ui, &self.data.art, d);
                    ui.add_space(match d {
                        TxDensity::Row => 7.0,
                        TxDensity::Feature => 10.0,
                        TxDensity::Poster => 18.0,
                    });

                    // The headline column is claimed from the RIGHT first, so
                    // the text column gets whatever is left and wraps into it
                    // rather than pushing the figure off the card. That
                    // ordering is what stops a long handle collapsing the
                    // amount column to zero width.
                    match d.edit() {
                        TxEdit::Tight => {
                            // MEASURE THE FIGURE, THEN DIVIDE — do not let the two
                            // columns race for the width.
                            //
                            // This was a `right_to_left` layout with the headline
                            // claimed first. Inside it the headline's own
                            // `horizontal` expanded to the full row, the text
                            // column was handed what was left, which was nothing,
                            // and every label wrapped ONE GLYPH PER LINE into a
                            // column hundreds of points tall. `ActivityFeed`'s
                            // story keeps a regression case for the identical
                            // failure; it is the characteristic way an egui row
                            // with a right-aligned figure breaks.
                            let gap = 12.0;
                            let head_w = headline_width(ui, &self.data.headline, d);
                            let text_w = (ui.available_width() - head_w - gap).max(96.0);

                            ui.allocate_ui_with_layout(
                                Vec2::new(text_w, 0.0),
                                Layout::top_down(Align::LEFT),
                                |ui| {
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
                                    filtered = filtered.take().or(r.2);
                                },
                            );
                            ui.add_space(gap);
                            ui.allocate_ui_with_layout(
                                Vec2::new(ui.available_width().max(head_w), 0.0),
                                Layout::top_down(Align::RIGHT),
                                |ui| headline(ui, &self.data.headline, d),
                            );
                        }
                        TxEdit::Full => {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = match d {
                                    TxDensity::Row => 1.0,
                                    TxDensity::Feature => 2.0,
                                    TxDensity::Poster => 4.0,
                                };
                                ui.label(
                                    RichText::new(self.data.kicker)
                                        .size(d.kicker_size())
                                        .color(theme::TEXT_MUTED),
                                );
                                headline(ui, &self.data.headline, d);
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
                                filtered = filtered.take().or(r.2);
                            });
                        }
                    }
                });
            });

        let response = inner.response.interact(Sense::click());
        TxCardResponse {
            clicked: response.clicked(),
            walk,
            deepen,
            filtered,
            response,
        }
    }
}

/// The stacked text: subject, party clause, time, caution, footnote.
///
/// Returns `(walked party, asked to deepen, tag clicked)`.
fn text_column(
    ui: &mut Ui,
    data: &TxCardData<'_>,
    d: TxDensity,
    walkable: bool,
    walking: bool,
    now: Option<i64>,
) -> (Option<String>, bool, Option<String>) {
    let mut filtered = None;
    // TIGHTER THAN THE APP DEFAULT. These lines are one statement broken over
    // four rows, not four separate controls, so they lead like a paragraph —
    // egui's stock 6px gap between labels reads as four unrelated things and is
    // most of what made the first pass feel oversized.
    ui.spacing_mut().item_spacing.y = match d {
        TxDensity::Row => 1.0,
        TxDensity::Feature => 2.0,
        TxDensity::Poster => 4.0,
    };

    // No kicker in the tight edit — the Full edit draws it above the headline
    // in `show()`, and a row has chips saying the same thing.

    if let Some(subject) = data.subject {
        ui.label(
            RichText::new(subject)
                .size(d.subject_size())
                .color(theme::TEXT_PRIMARY)
                .strong(),
        );
    }

    let edit = d.edit();

    // Time. WORDS FIRST — "1h ago" is what a reader scanning a feed actually
    // uses; the absolute stamp is for the record and only earns its space once
    // there is space. In the tight edit the words hang off the party clause
    // and the stamp is gone; in the full edit they get a row of their own.
    let inline_time = match edit {
        TxEdit::Tight => Some((data.when, now)),
        TxEdit::Full => None,
    };
    let (walk, deepen) = party_clause(ui, &data.view, d, walkable, walking, inline_time);

    match edit {
        TxEdit::Tight => {}
        TxEdit::Full => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                relative_time(ui, data.when, now, d.body_size());
                ui.label(
                    RichText::new(format!("· {}", iso_utc(data.when)))
                        .size(d.body_size())
                        .color(theme::TEXT_MUTED),
                );
            });
        }
    }

    // The caution: its own line where there is one, else it rides in the chip
    // row below. Either way it is amber, because it means the same thing.
    let caution_in_chip_row = match (edit, data.caution) {
        (TxEdit::Full, Some(caution)) => {
            ui.label(
                RichText::new(caution)
                    .size(d.body_size())
                    .color(theme::ACCENT_ORANGE),
            );
            None
        }
        (TxEdit::Tight, caution) => caution,
        (TxEdit::Full, None) => None,
    };

    match (edit, data.footnote) {
        (TxEdit::Full, Some(footnote)) => {
            ui.label(
                RichText::new(footnote)
                    .size(d.body_size())
                    .color(theme::TEXT_MUTED),
            );
        }
        (TxEdit::Tight, _) | (TxEdit::Full, None) => {}
    }

    // FACETS LAST. They are how a reader slices the feed, not how they read the
    // row — putting them first is what made the old layout a tag soup with the
    // verdict hidden inside it.
    if !data.tags.is_empty() || caution_in_chip_row.is_some() {
        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // The caution LEADS the row when it is here: it changes the
            // reading, and the chips only refine it.
            if let Some(caution) = caution_in_chip_row {
                ui.label(
                    RichText::new(caution)
                        .size(d.body_size())
                        .color(theme::ACCENT_ORANGE),
                );
                if !data.tags.is_empty() {
                    ui.add_space(4.0);
                }
            }
            for (label, variant) in &data.tags {
                if Chip::new(label)
                    .variant(*variant)
                    .clickable(true)
                    .show(ui)
                    .clicked
                {
                    filtered = Some((*label).to_string());
                }
            }
        });
    }

    (walk, deepen, filtered)
}

/// `$boef bought from $elchapojr`, or `$a → $b`, or one lone party.
fn party_clause(
    ui: &mut Ui,
    view: &TxViewpoint<'_>,
    d: TxDensity,
    walkable: bool,
    walking: bool,
    inline_time: Option<(i64, Option<i64>)>,
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
            // BOTH SIDES AMBIGUOUS IS ONE FACT, not two absences with an arrow
            // between them. A batched fill or a bulk transfer has several
            // parties on each side; the live feed rendered that as "0 parties
            // on this side → 0 parties on this side", which reads as a broken
            // row rather than as the honest statement that there is no
            // directed pair to draw.
            TxViewpoint::Pair {
                from: TxParty::Ambiguous { count: a },
                to: TxParty::Ambiguous { count: b },
            } => {
                let text = match a + b {
                    0 => "several parties — no single sender or recipient".to_string(),
                    n => format!("several parties — {n} in this movement"),
                };
                ui.label(
                    RichText::new(text)
                        .size(size)
                        .italics()
                        .color(theme::TEXT_MUTED),
                )
                .on_hover_text(
                    "A batched fill or a bulk transfer: more than one party on a side, so \
                     there is no single sender and no single recipient. Walking deeper \
                     does not resolve this.",
                );
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
        // `$boef bought from $elchapojr · 1h ago` — the tight edit's time.
        if let Some((when, now)) = inline_time {
            ui.label(RichText::new("·").size(size).color(theme::TEXT_MUTED));
            relative_time(ui, when, now, size);
        }
    });

    (walk, deepen)
}

/// The relative age — `1h ago` — with a pinned "now" for stories.
fn relative_time(ui: &mut Ui, when: i64, now: Option<i64>, size: f32) {
    let mut rel = RelativeTime::new(when).size(size).color(theme::TEXT_MUTED);
    if let Some(now) = now {
        rel = rel.now(now);
    }
    ui.add(rel);
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
            // PLAIN TEXT, NOT A `PartyBadge`.
            //
            // The badge prefixes every party with a shape-coded basis dot, and
            // in a forensic trace — where a reader is weighing whether to trust
            // a name — that glyph is the point. In a feed it is a 5px bullet in
            // front of every handle on every row, which reads as list markup
            // and says nothing. The basis is kept as INFORMATION, on the hover,
            // where it costs nothing and is there when somebody asks.
            let resp = ui
                .link(RichText::new(*label).size(size).color(theme::ACCENT))
                .on_hover_text(match basis {
                    PartyBasis::Observed => "Resolved from the chain.".to_string(),
                    PartyBasis::Derived => {
                        "The venue's word — decoded from a market event, not an output the walk \
                         resolved."
                            .to_string()
                    }
                    PartyBasis::Asserted => {
                        "Asserted from outside the chain.".to_string()
                    }
                } + &match key {
                    Some(k) => format!("\n{k}"),
                    None => String::new(),
                });
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
            // A count of zero is "the derivation declined and recorded nobody",
            // not "nobody was there" — say several, not 0.
            let text = match count {
                0 => "several parties on this side".to_string(),
                n => format!("{n} parties on this side"),
            };
            ui.label(
                RichText::new(text)
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

/// The headline figure — in the surface's OWN face, always.
///
/// # Why there is no monospace here
///
/// There was. The social card sets its digits in JetBrains Mono because a
/// number is read against another number and tabular figures line up, and that
/// reasoning was carried over unexamined. It does not survive the move, for two
/// reasons:
///
/// 1. **This headline is not always a number.** A venue batch leads with
///    `6 listed · 2 offers made · 1 delisted`, and the split-on-last-space rule
///    that isolates `10` from `₳` saw a digit in that sentence and set the whole
///    breakdown in a terminal face. Prose in mono at 22px does not read as
///    precision, it reads as a mistake.
/// 2. **Tabular alignment buys nothing in a column that mixes shapes.** The
///    figures only line up if every row has a figure; interleave a count and a
///    breakdown and the argument for the face is gone.
///
/// So the headline is the app's own proportional face, distinguished by weight,
/// colour and size — which is how everything else on the surface is ranked.
/// This also drops the `₳`-not-in-the-mono-face hazard the split existed to
/// dodge in the first place.
/// How wide the headline wants to be, measured before anything is drawn.
///
/// The row layout has to divide a fixed width between the story and the figure,
/// and the only safe way to do that is to ask the font how much the figure
/// needs rather than letting two flex columns negotiate — see the call site for
/// what happens when they negotiate.
///
/// `.strong()` renders very slightly wider than the plain face it is measured
/// against, so the result carries a little slack. Over-reserving pushes the
/// text column in by a few points; under-reserving wraps the figure, which is
/// far worse.
fn headline_width(ui: &Ui, h: &TxHeadline<'_>, d: TxDensity) -> f32 {
    let measure = |text: &str, size: f32| {
        ui.painter()
            .layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE)
            .size()
            .x
    };
    let value = measure(h.value, d.headline_size());
    let qualifier = h.qualifier.map_or(0.0, |q| measure(q, d.body_size()) + 6.0);
    (value + qualifier) * 1.06 + 4.0
}

fn headline(ui: &mut Ui, h: &TxHeadline<'_>, d: TxDensity) {
    let size = d.headline_size();
    let colour = h.tone.color();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(h.value).size(size).color(colour).strong());
        if let Some(q) = h.qualifier {
            // Sized against the BODY, not against the headline: a qualifier
            // scaled off a 40px figure is itself a headline, and it is meant to
            // be the quiet word that stops `10 ₳` being read as ten each.
            ui.label(
                RichText::new(q)
                    .size(d.body_size())
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
/// Delegated to [`crate::image_stack`], which owns every proportion of the
/// treatment and has a slider bench behind it. This was hand-painted here
/// first and looked notably weaker than the server-rendered card without it
/// being obvious why from the code — which is exactly the case for the geometry
/// living somewhere it can be tuned by eye rather than by constant.
fn paint_prints(ui: &mut Ui, prints: &[TxPrint<'_>], d: TxDensity) {
    let images: Vec<StackImage<'_>> = prints
        .iter()
        .map(|p| match p.image_url {
            Some(url) => StackImage::new(p.label).image(url),
            None => StackImage::new(p.label),
        })
        .collect();
    // FANNED AT EVERY DENSITY. There was a `fans()` on the density that turned
    // the pile off for a row, on the theory that a 30px fan is a smear. That
    // was true at the first-guess style and false at the tuned one — with
    // almost no horizontal step the buried prints are corners poking out, and
    // those survive 30px. The bench's "fanned vs single" row is the evidence;
    // a predicate that had come to answer the same thing at every density was
    // deleted rather than kept.
    ImageStack::new(&images).size(d.art_size()).show(ui);
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
