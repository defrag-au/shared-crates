//! `ActivityFeed` — a wallet's transactions as day-grouped cards: what it was,
//! who it was with, what moved, and what it cost.
//!
//! The table form ([`crate::flow_ledger`]) is the forensic view — dense, one
//! line per movement, built for reading a whole history at once. This is the
//! *account* view, the one a wallet app shows: fewer, larger units, each one
//! self-contained enough that a person recognises their own transaction in it
//! without cross-referencing a column header.
//!
//! ## What a row has to answer
//!
//! A ledger row that reads `−9.6 ₳ · +2 items · addr1q8pyt…t9gu9y`, repeated
//! five times, answers none of the questions a holder actually has. The three
//! that matter, and where each lands on the card:
//!
//! - **What was this?** — the tag strip, top-left. A venue ("Wayup"), a shape
//!   ("mint", "swap"), a note. Tags are [`crate::Chip`]s, so the palette is
//!   the shared semantic one rather than a per-caller invention.
//! - **What moved?** — the asset pills along the bottom, each with its
//!   thumbnail, its name, and a signed quantity badge. **Assets are named,
//!   never counted**: `+2 items` hides whether a wallet received two junk
//!   airdrops or two of the collection it is being paid in. A count is the
//!   one thing a holder can already see for themselves.
//! - **What did it cost?** — the amount, top-right, signed and coloured, with
//!   an optional second line for a converted value.
//! - **Who was it with?** — the counterparty, on the time line beside the
//!   timestamp. With [`ActivityFeed::walkable`] it is a link, and clicking it
//!   reports [`ActivityFeedResponse::walk`] instead of `clicked`: following
//!   money out of a feed is the move the card exists to enable, and a feed
//!   that can only open its own transactions is a dead end at every hop.
//!
//! Time is deliberately doubled up: a relative age ("18h ago") for the scan,
//! an absolute timestamp beneath it for the record. Day headers break the run
//! so a burst of activity reads as a burst.
//!
//! ## Domain-free
//!
//! Amounts are `i128` in the smallest unit, formatted by a host-supplied
//! closure; asset labels arrive already resolved (hex → name, CIP-67 label
//! stripped) because that is chain-specific work the widget must not guess
//! at. Thumbnails are optional: without one the pill shows a tinted initial,
//! so a feed still reads before any image loader is installed.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{ActivityFeed, ActivityEntry, ActivityAsset, ActivityTag, ChipVariant};
//!
//! let entries = vec![
//!     ActivityEntry::new(1787704389, 2_430_840)
//!         .tag(ActivityTag::new("Wayup", ChipVariant::Warning))
//!         .tx_id("13863a19…")
//!         .asset(ActivityAsset::new("HOSKY Cash Grab #1729", 1)),
//! ];
//! let resp = ActivityFeed::new(&entries, &|v| format!("{:+.2} ₳", v as f64 / 1e6)).show(ui);
//! if let Some(i) = resp.clicked { open_tx(&entries[i]); }
//! ```

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, Vec2};

use crate::chip::{Chip, ChipVariant};
use crate::relative_time::RelativeTime;
use crate::theme;
use crate::timestamp::format_iso8601;

/// Assets shown before the overflow pill takes over.
const MAX_PILLS: usize = 6;
/// Characters of an asset name a pill shows before eliding. [`elide`] takes
/// it out of the MIDDLE, so a long minted name (`HOSKYCashGrab000146545`)
/// keeps both the collection prefix and the serial that distinguishes it.
const PILL_LABEL_CHARS: usize = 22;
/// Characters of a counterparty the time line shows. Shorter than a pill's:
/// it shares that line with a relative age and a full timestamp, and a
/// 103-character address would push both off a phone.
const PARTY_LABEL_CHARS: usize = 18;
/// Hover wording for a walkable counterparty. "Walk" is the verb this toolkit
/// already uses for following money between wallets — see `CustodyWalk` and
/// the stave's lane click.
const WALK_HINT: &str = "walk to this wallet";

/// One tag on a card — a venue, a shape, a note.
#[derive(Clone, Copy)]
pub struct ActivityTag<'a> {
    pub label: &'a str,
    pub variant: ChipVariant,
}

impl<'a> ActivityTag<'a> {
    pub fn new(label: &'a str, variant: ChipVariant) -> Self {
        Self { label, variant }
    }
}

/// One asset that moved. `quantity` is signed — positive arrived, negative
/// left — because on a card the direction belongs on the thing that moved,
/// not only on the total.
#[derive(Clone, Copy)]
pub struct ActivityAsset<'a> {
    pub label: &'a str,
    pub quantity: i64,
    /// Thumbnail URI. `None` renders a tinted initial instead.
    pub image_url: Option<&'a str>,
}

impl<'a> ActivityAsset<'a> {
    pub fn new(label: &'a str, quantity: i64) -> Self {
        Self {
            label,
            quantity,
            image_url: None,
        }
    }

    pub fn image(mut self, url: Option<&'a str>) -> Self {
        self.image_url = url;
        self
    }
}

/// One transaction.
pub struct ActivityEntry<'a> {
    pub timestamp: i64,
    /// Net change in the smallest unit — positive arrived, negative left.
    pub amount: i128,
    pub tags: Vec<ActivityTag<'a>>,
    pub assets: Vec<ActivityAsset<'a>>,
    /// Assets the transaction was ABOUT rather than moved — what an offer
    /// names, what a listing lists. Rendered without a quantity badge and
    /// behind their own caption, because a target drawn like a movement
    /// claims a receipt that never happened.
    pub targets: Vec<ActivityAsset<'a>>,
    /// Caption for the target row ("offer on", "listing"). Domain wording is
    /// the caller's; the widget only knows these did not move.
    pub targets_label: &'a str,
    /// Distinct targets before the caller's cap, for the overflow note.
    pub targets_total: usize,
    /// Secondary amount line (a converted value, a fee, a note).
    pub secondary: Option<String>,
    /// Who this was with, shown on the time line beside the timestamp.
    ///
    /// Always rendered when present — "when" and "who" are the two facts a
    /// card is scanned for, and the counterparty used to appear only on an
    /// untagged card, so the moment a transaction was recognised at all the
    /// other side of it vanished.
    pub counterparty: Option<&'a str>,
    /// Preposition for the counterparty — "from", "to", "with". Domain
    /// wording is the caller's; the widget only knows there was another side,
    /// not which way the value went.
    pub counterparty_label: &'a str,
    pub tx_id: Option<&'a str>,
}

impl<'a> ActivityEntry<'a> {
    pub fn new(timestamp: i64, amount: i128) -> Self {
        Self {
            timestamp,
            amount,
            tags: Vec::new(),
            assets: Vec::new(),
            targets: Vec::new(),
            targets_label: "for",
            targets_total: 0,
            secondary: None,
            counterparty: None,
            counterparty_label: "",
            tx_id: None,
        }
    }

    /// An asset this transaction was about but did not move.
    pub fn target(mut self, asset: ActivityAsset<'a>) -> Self {
        self.targets.push(asset);
        self.targets_total = self.targets_total.max(self.targets.len());
        self
    }

    /// Caption + true count for the target row (a cart may exceed the cap).
    pub fn targets_meta(mut self, label: &'a str, total: usize) -> Self {
        self.targets_label = label;
        self.targets_total = total;
        self
    }

    pub fn tag(mut self, tag: ActivityTag<'a>) -> Self {
        self.tags.push(tag);
        self
    }

    pub fn asset(mut self, asset: ActivityAsset<'a>) -> Self {
        self.assets.push(asset);
        self
    }

    pub fn secondary(mut self, text: impl Into<String>) -> Self {
        self.secondary = Some(text.into());
        self
    }

    pub fn counterparty(mut self, who: Option<&'a str>) -> Self {
        self.counterparty = who;
        self
    }

    /// The preposition in front of the counterparty ("from", "to", "with").
    pub fn counterparty_label(mut self, caption: &'a str) -> Self {
        self.counterparty_label = caption;
        self
    }

    pub fn tx_id(mut self, tx_id: &'a str) -> Self {
        self.tx_id = Some(tx_id);
        self
    }
}

#[derive(Default)]
pub struct ActivityFeedResponse {
    /// Index of the card clicked this frame.
    pub clicked: Option<usize>,
    /// Index of the card whose COUNTERPARTY was clicked — a request to go to
    /// that party, not to open the transaction.
    ///
    /// Mutually exclusive with [`Self::clicked`]: one pointer press is one
    /// intent, and the party sits inside the card, so the two must not both
    /// fire and leave the caller opening a panel about the wallet it just
    /// navigated away from.
    pub walk: Option<usize>,
}

pub struct ActivityFeed<'a> {
    entries: &'a [ActivityEntry<'a>],
    format_amount: &'a dyn Fn(i128) -> String,
    show_day_headers: bool,
    max_pills: usize,
    walkable: bool,
    marked: Option<usize>,
    scroll_to: Option<usize>,
}

impl<'a> ActivityFeed<'a> {
    pub fn new(
        entries: &'a [ActivityEntry<'a>],
        format_amount: &'a dyn Fn(i128) -> String,
    ) -> Self {
        Self {
            entries,
            format_amount,
            show_day_headers: true,
            max_pills: MAX_PILLS,
            walkable: false,
            marked: None,
            scroll_to: None,
        }
    }

    /// Draw this card as the one under discussion.
    ///
    /// For a selection made somewhere OTHER than the feed — a detail panel
    /// opened from a link, or from the stave. Without it the panel talks about
    /// a transaction the reader cannot pick out of the list, which is the same
    /// problem as not scrolling to it: the card is present but anonymous.
    ///
    /// Persistent, unlike [`Self::scroll_to`]: it marks for as long as the
    /// selection stands.
    pub fn marked(mut self, index: Option<usize>) -> Self {
        self.marked = index;
        self
    }

    /// Bring this card into view, this frame.
    ///
    /// **One-shot, and the caller owns the "once".** Passing `Some` on every
    /// frame would pin the scroll position and the reader could never move
    /// away from it — so the host is expected to clear this after the frame
    /// that used it. Separated from [`Self::marked`] for exactly that reason:
    /// the two look like one feature and have opposite lifetimes.
    pub fn scroll_to(mut self, index: Option<usize>) -> Self {
        self.scroll_to = index;
        self
    }

    /// Is the counterparty somewhere the reader can GO?
    ///
    /// Opt-in, because that depends entirely on the host: a feed of a single
    /// wallet's own history can follow the other side, an embedded receipt
    /// list has nowhere to send anyone. Off, the party still shows — it is
    /// half of what a card says — it simply is not a link, so the affordance
    /// never promises a destination that does not exist.
    pub fn walkable(mut self, walkable: bool) -> Self {
        self.walkable = walkable;
        self
    }

    /// Asset pills shown inline before the rest collapse into "+N more".
    ///
    /// A caller's decision because it is really a question about WIDTH, which
    /// the widget cannot see far enough to answer: six pills wrap to a
    /// comfortable two lines on a desktop card and to six lines on a phone,
    /// where they push the card past the viewport.
    pub fn max_pills(mut self, max: usize) -> Self {
        self.max_pills = max.max(1);
        self
    }

    /// Suppress the day headers (for a short embedded feed).
    pub fn day_headers(mut self, show: bool) -> Self {
        self.show_day_headers = show;
        self
    }

    pub fn show(self, ui: &mut Ui) -> ActivityFeedResponse {
        let mut resp = ActivityFeedResponse::default();
        let mut last_day: Option<String> = None;

        for (i, entry) in self.entries.iter().enumerate() {
            let stamp = format_iso8601(entry.timestamp, true);
            let day = stamp.get(..10).unwrap_or_default().to_string();
            if self.show_day_headers && last_day.as_deref() != Some(day.as_str()) {
                if last_day.is_some() {
                    ui.add_space(6.0);
                }
                ui.label(
                    RichText::new(friendly_day(&day))
                        .color(theme::TEXT_SECONDARY)
                        .strong(),
                );
                ui.add_space(2.0);
                last_day = Some(day);
            }
            let marked = self.marked == Some(i);
            let (hit, rect) = self.card(ui, entry, &stamp, marked);
            // Scrolled AFTER the card is laid out, so there is a real rect to
            // aim at. `Align::Center` rather than `Min`: a transaction arrived
            // at from a link wants its neighbours visible — what came before
            // and after it is most of why somebody sent the link.
            if self.scroll_to == Some(i) {
                ui.scroll_to_rect(rect, Some(egui::Align::Center));
            }
            match hit {
                Hit::Card => resp.clicked = Some(i),
                Hit::Party => resp.walk = Some(i),
                Hit::Miss => {}
            }
            ui.add_space(4.0);
        }
        resp
    }

    /// One card. Returns what the click landed on, and where the card ended
    /// up — the rect is what [`Self::scroll_to`] aims at.
    fn card(
        &self,
        ui: &mut Ui,
        entry: &ActivityEntry<'_>,
        stamp: &str,
        marked: bool,
    ) -> (Hit, egui::Rect) {
        // Where the counterparty ended up, so the click below can tell "open
        // this transaction" from "go to that wallet". Set inside the frame,
        // read outside it — see the interact block at the end.
        let mut party_rect = None;
        let inner = Frame::new()
            .fill(theme::BG_SECONDARY)
            .stroke(Stroke::new(1.0_f32, theme::BORDER))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                // SPLIT THE HEADER ROW EXPLICITLY. Letting the two columns
                // negotiate their own widths inside a plain `horizontal` was a
                // narrow-screen collapse: the tag row does not wrap, so on a
                // phone it overflowed past the card, the amount column was
                // handed what was left (zero), and the amount then wrapped ONE
                // GLYPH PER LINE — several hundred points of invisible column
                // that made the card look mostly empty. Measuring the amount
                // and reserving it FIRST means neither column can starve the
                // other.
                let amount = (self.format_amount)(entry.amount);
                let avail = ui.available_width();
                // The amount is the headline number and must never wrap; the
                // cap keeps a long `secondary` from eating the tag column.
                let right_w = amount_column_width(ui, &amount, entry.secondary.as_deref())
                    .min((avail * 0.42).max(MIN_AMOUNT_COL));
                let left_w = (avail - right_w - ui.spacing().item_spacing.x).max(MIN_AMOUNT_COL);

                ui.horizontal_top(|ui| {
                    // Left: what it was, and when.
                    ui.allocate_ui_with_layout(
                        Vec2::new(left_w, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_max_width(left_w);
                            // WRAPPED. Four tags is normal — a token, a shape,
                            // a venue and an outcome — and on a phone that is
                            // two lines, not one clipped one.
                            ui.horizontal_wrapped(|ui| {
                                for tag in &entry.tags {
                                    Chip::new(tag.label).variant(tag.variant).show(ui);
                                }
                            });
                            // WHEN and WHO on one line. The counterparty used
                            // to live in the tag row above and only when that
                            // row was otherwise empty, so recognising a
                            // transaction — a venue, a mint, a handle — cost
                            // the card the other side of it, which is the one
                            // thing a reader following money needs next.
                            ui.horizontal_wrapped(|ui| {
                                ui.add(RelativeTime::new(entry.timestamp));
                                ui.label(
                                    RichText::new(format!("· {stamp}"))
                                        .color(theme::TEXT_MUTED)
                                        .small(),
                                );
                                if let Some(cp) = entry.counterparty {
                                    party_rect = Some(self.party(ui, entry.counterparty_label, cp));
                                }
                            });
                        },
                    );

                    // Right: what it cost.
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width().max(right_w), 0.0),
                        egui::Layout::top_down(egui::Align::RIGHT),
                        |ui| {
                            let color = if entry.amount >= 0 {
                                theme::SUCCESS
                            } else {
                                theme::ACCENT_ORANGE
                            };
                            ui.label(RichText::new(amount).color(color).monospace().strong());
                            if let Some(second) = &entry.secondary {
                                ui.label(RichText::new(second).color(theme::TEXT_MUTED).small());
                            }
                        },
                    );
                });

                // Bottom: what moved.
                if !entry.assets.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for asset in entry.assets.iter().take(self.max_pills) {
                            asset_pill(ui, asset, true);
                        }
                        let extra = entry.assets.len().saturating_sub(self.max_pills);
                        if extra > 0 {
                            ui.label(
                                RichText::new(format!("+{extra} more"))
                                    .color(theme::TEXT_MUTED)
                                    .small(),
                            );
                        }
                    });
                }

                // …and what it was about, if that is a different thing.
                if !entry.targets.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(entry.targets_label)
                                .color(theme::TEXT_MUTED)
                                .small(),
                        );
                        for asset in entry.targets.iter().take(self.max_pills) {
                            asset_pill(ui, asset, false);
                        }
                        let shown = entry.targets.len().min(self.max_pills);
                        let extra = entry.targets_total.saturating_sub(shown);
                        if extra > 0 {
                            ui.label(
                                RichText::new(format!("+{extra} more"))
                                    .color(theme::TEXT_MUTED)
                                    .small(),
                            );
                        }
                    });
                }
            });

        // ONE interaction over the whole card, and the party resolved by
        // geometry rather than by nesting a second clickable widget inside it.
        //
        // A nested link would never fire: this `interact` is registered after
        // everything the frame drew, so it sits on top of the card's own
        // contents and wins the hit test for every point inside it. Comparing
        // the pointer against the party's rect is the same decision, made in
        // the one place that actually receives the click.
        let response = ui.interact(
            inner.response.rect,
            ui.id().with(entry.tx_id.unwrap_or(stamp)),
            Sense::click(),
        );
        // `walkable` gates the HIT, not just the styling. Without it a feed
        // with nowhere to send anyone would still swallow every click that
        // landed on a party, and the card it sits in would stop opening.
        let on_party =
            self.walkable && party_rect.is_some_and(|r: egui::Rect| ui.rect_contains_pointer(r));
        // The card's own hover cues are withheld over the party — outlining
        // the whole card would promise it is about to open, which is exactly
        // what clicking there does NOT do.
        if response.hovered() && !on_party {
            ui.painter().rect_stroke(
                inner.response.rect,
                CornerRadius::same(8),
                Stroke::new(1.0_f32, theme::ACCENT_BLUE),
                egui::StrokeKind::Inside,
            );
        }
        match (entry.tx_id, on_party) {
            (_, true) => {
                response.clone().on_hover_text(WALK_HINT);
            }
            (Some(tx), false) => {
                response.clone().on_hover_text(tx);
            }
            (None, false) => {}
        }
        // THE MARK, painted last so it survives the hover stroke above rather
        // than being overdrawn by it. A selection made elsewhere has to stay
        // visible while the pointer moves over other cards — that is the whole
        // situation it exists for.
        if marked {
            ui.painter().rect_stroke(
                inner.response.rect,
                CornerRadius::same(8),
                Stroke::new(2.0_f32, theme::ACCENT_CYAN),
                egui::StrokeKind::Inside,
            );
        }
        let hit = match (response.clicked(), on_party) {
            (false, _) => Hit::Miss,
            (true, true) => Hit::Party,
            (true, false) => Hit::Card,
        };
        (hit, inner.response.rect)
    }

    /// The counterparty on the time line. Returns the rect it claimed, which
    /// is what the card's click is measured against.
    fn party(&self, ui: &mut Ui, caption: &str, who: &str) -> egui::Rect {
        let colour = if self.walkable {
            theme::ACCENT_BLUE
        } else {
            theme::TEXT_SECONDARY
        };
        ui.label(RichText::new("·").color(theme::TEXT_MUTED).small());
        // The caption is inside the hit rect: "from addr1q…" is one phrase,
        // and a link that starts one word into it invites a miss.
        let caption_rect = (!caption.is_empty()).then(|| {
            ui.label(RichText::new(caption).color(theme::TEXT_MUTED).small())
                .rect
        });
        let mut rect = ui
            .label(
                RichText::new(elide(who, PARTY_LABEL_CHARS))
                    .color(colour)
                    .monospace()
                    .small(),
            )
            .rect;
        // …but only when the wrap left them on the SAME line. At phone width
        // the caption stays with the timestamp and the address drops below
        // it, and a bounding box over both would claim the empty stretch of
        // the timestamp line between them — clicks landing on nothing and
        // walking away from the transaction the reader was reading.
        if let Some(cap) = caption_rect {
            if (cap.center().y - rect.center().y).abs() < 1.0 {
                rect = rect.union(cap);
            }
        }
        if self.walkable && ui.rect_contains_pointer(rect) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            // Underlined on hover rather than re-tinted: the tint is already
            // spent saying "this is a link", and the colour cannot be changed
            // after the text is painted anyway.
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 1.0,
                Stroke::new(1.0_f32, theme::ACCENT_BLUE),
            );
        }
        rect
    }
}

/// What a click on a card landed on.
enum Hit {
    Miss,
    Card,
    Party,
}

/// Floor for either header column, so a very narrow card degrades to a squeeze
/// rather than to a column of one character per line.
const MIN_AMOUNT_COL: f32 = 72.0;

/// How wide the amount column wants to be, MEASURED before the tag column gets
/// to claim space — the same measure-then-reserve shape as [`pill_width`], and
/// for the same reason: egui hands a child whatever is left over, so whichever
/// column lays out first silently decides the other one's fate.
fn amount_column_width(ui: &Ui, amount: &str, secondary: Option<&str>) -> f32 {
    let width_of = |text: &str, style: egui::TextStyle| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), style.resolve(ui.style()), Color32::WHITE)
            .size()
            .x
    };
    let mut w = width_of(amount, egui::TextStyle::Monospace);
    if let Some(second) = secondary {
        w = w.max(width_of(second, egui::TextStyle::Small));
    }
    w
}

/// A named asset with its thumbnail. `moved` decides whether it carries a
/// signed quantity — a referenced asset gets no badge and a neutral tint, so
/// "the offer names this" cannot be misread as "this arrived".
/// Pill geometry, kept in one place because the width is now MEASURED before
/// the pill draws and the two must agree.
const PILL_ICON: f32 = 18.0;
const PILL_PAD_X: f32 = 6.0;
const PILL_PAD_Y: f32 = 3.0;

/// How wide this pill will be, computed before anything is drawn.
///
/// Exists because of a real layout trap: `Frame::show` inside a
/// `horizontal_wrapped` row never triggers the row's wrap. A frame takes the
/// cursor's remaining rect, lays its children out, and only THEN calls
/// `allocate_rect` — and `allocate_rect` does not consult the wrap logic, it
/// simply advances the cursor. So a pill that does not fit is drawn
/// overhanging and clipped at the column edge, and the first thing off the
/// end is the QUANTITY, which sits last. Six long asset names ran clean off
/// the card that way.
///
/// Measuring first and reserving with `allocate_exact_size` puts the size in
/// front of the allocation, which is the only form the wrapped layout can act
/// on.
fn pill_width(ui: &Ui, asset: &ActivityAsset<'_>, moved: bool) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    let font = egui::TextStyle::Small.resolve(ui.style());
    let text_w = |s: String| {
        ui.painter()
            .layout_no_wrap(s, font.clone(), egui::Color32::WHITE)
            .size()
            .x
    };
    let mut w = PILL_PAD_X * 2.0 + PILL_ICON + gap + text_w(elide(asset.label, PILL_LABEL_CHARS));
    if moved {
        w += gap + text_w(signed_quantity(asset.quantity));
    }
    w
}

fn asset_pill(ui: &mut Ui, asset: &ActivityAsset<'_>, moved: bool) {
    let arrived = asset.quantity >= 0;
    let tint = if !moved {
        theme::TEXT_MUTED
    } else if arrived {
        theme::SUCCESS
    } else {
        theme::ACCENT_ORANGE
    };
    // Reserve the whole pill up front so the wrapped row can break BEFORE it,
    // then draw into the rect we were given. See [`pill_width`].
    let size = egui::vec2(pill_width(ui, asset, moved), PILL_ICON + PILL_PAD_Y * 2.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), theme::BG_HIGHLIGHT);
    let ui = &mut ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(PILL_PAD_X, PILL_PAD_Y)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    match asset.image_url {
        // Reserve the space FIRST, then only request the image if that space
        // is actually on screen.
        //
        // `ui.add(Image::new(url))` starts a fetch+decode the moment the
        // widget is built, and a feed is a long scrolling list: every card
        // below the fold was pulling its thumbnails on first paint. A few
        // hundred requests against a six-connections-per-host browser limit is
        // a stall before the first card is readable.
        //
        // The size is fixed, so gating the paint cannot move layout — a
        // scrolled-past pill occupies exactly the same box whether or not its
        // image has been asked for.
        Some(url) => {
            let (icon, _) = ui.allocate_exact_size(Vec2::splat(PILL_ICON), egui::Sense::hover());
            if ui.is_rect_visible(icon) {
                egui::Image::new(url)
                    .fit_to_exact_size(Vec2::splat(PILL_ICON))
                    .corner_radius(CornerRadius::same(4))
                    .paint_at(ui, icon);
            }
        }
        None => initial_disc(ui, asset.label, tint),
    }
    ui.label(
        RichText::new(elide(asset.label, PILL_LABEL_CHARS))
            .color(theme::TEXT_PRIMARY)
            .small(),
    );
    // Quantity is a signed badge, not a bare number: on a card the direction
    // belongs to the thing that moved.
    if moved {
        ui.label(
            RichText::new(signed_quantity(asset.quantity))
                .color(tint)
                .small()
                .strong(),
        );
    }
}

/// Signed quantity with digit grouping. A fungible move is routinely six or
/// more digits, and `-420000` is a number the reader has to count on screen;
/// `-420,000` is one they can read.
fn signed_quantity(quantity: i64) -> String {
    let sign = if quantity < 0 { '-' } else { '+' };
    let digits = quantity.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    grouped.push(sign);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    grouped
}

/// Thumbnail stand-in: a tinted disc carrying the label's first character, so
/// a pill still reads with no image loader and no metadata.
fn initial_disc(ui: &mut Ui, label: &str, tint: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
    let ch = label
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('?')
        .to_ascii_uppercase();
    ui.painter()
        .circle_filled(rect.center(), 9.0, tint.gamma_multiply(0.35));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ch,
        egui::FontId::proportional(11.0),
        tint,
    );
}

/// `2026-08-25` → `August 25, 2026`.
fn friendly_day(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut parts = iso.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return iso.to_string();
    };
    let month = m
        .parse::<usize>()
        .ok()
        .and_then(|m| MONTHS.get(m.saturating_sub(1)).copied())
        .unwrap_or(m);
    let day = d.trim_start_matches('0');
    format!("{month} {day}, {y}")
}

fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max / 2).collect();
    let tail: String = s
        .chars()
        .skip(s.chars().count().saturating_sub(max / 2))
        .collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records every URI egui asks for, so a test can assert about *fetches*
    /// rather than about pixels.
    #[derive(Default)]
    struct CountingLoader {
        asked: std::sync::Mutex<Vec<String>>,
    }

    impl egui::load::ImageLoader for CountingLoader {
        fn id(&self) -> &str {
            "counting"
        }
        fn load(
            &self,
            _ctx: &egui::Context,
            uri: &str,
            _hint: egui::load::SizeHint,
        ) -> egui::load::ImageLoadResult {
            self.asked.lock().expect("asked").push(uri.to_string());
            Ok(egui::load::ImagePoll::Pending { size: None })
        }
        fn forget(&self, _uri: &str) {}
        fn forget_all(&self) {}
        fn byte_size(&self) -> usize {
            0
        }
    }

    /// The lag report: a long feed pulled every thumbnail on first paint,
    /// including cards far below the fold. Hundreds of concurrent fetches
    /// against a six-per-host browser limit stalls the view before the first
    /// card is readable.
    ///
    /// Asserts on the loader, not on appearance: what matters is that work is
    /// not STARTED for rows nobody can see.
    #[test]
    fn offscreen_thumbnails_are_not_requested() {
        let loader = std::sync::Arc::new(CountingLoader::default());
        let ctx = egui::Context::default();
        ctx.add_image_loader(loader.clone());

        // Enough entries that most of them cannot fit in the viewport.
        let labels: Vec<String> = (0..200).map(|i| format!("asset {i}")).collect();
        let urls: Vec<String> = (0..200).map(|i| format!("https://img/{i}.png")).collect();
        let entries: Vec<ActivityEntry<'_>> = (0..200)
            .map(|i| {
                ActivityEntry::new(1_756_000_000 + i as i64 * 3600, 1_000_000)
                    .asset(ActivityAsset::new(&labels[i], 1).image(Some(&urls[i])))
            })
            .collect();

        let input = egui::RawInput {
            // A short viewport: only the first few cards can be on screen.
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 300.0),
            )),
            ..Default::default()
        };
        let fmt = |a: i128| format!("{a}");
        // `run_ui` hands back a `&mut Ui` rather than a `&Context`, so the
        // panel is shown INSIDE it — `Context::run` and `Panel::show` are the
        // deprecated pair of that older shape and only make sense together.
        let _ = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ActivityFeed::new(&entries, &fmt).show(ui);
                });
            });
        });

        let asked = loader.asked.lock().expect("asked").len();
        assert!(
            asked > 0,
            "nothing was requested at all — the visible cards still need their thumbnails"
        );
        assert!(
            asked < entries.len() / 2,
            "{asked} of {} thumbnails requested — off-screen cards are still \
             fetching, which is the first-paint stall",
            entries.len()
        );
    }

    #[test]
    fn day_header_reads_as_a_date() {
        assert_eq!(friendly_day("2026-08-25"), "August 25, 2026");
        assert_eq!(friendly_day("2026-01-01"), "January 1, 2026");
        // Malformed input degrades to the raw string rather than panicking.
        assert_eq!(friendly_day("nonsense"), "nonsense");
    }

    #[test]
    fn quantities_group_and_keep_their_sign() {
        assert_eq!(signed_quantity(1), "+1");
        assert_eq!(signed_quantity(-420_000), "-420,000");
        assert_eq!(signed_quantity(1_250), "+1,250");
        assert_eq!(signed_quantity(999), "+999");
        assert_eq!(signed_quantity(1_000_000), "+1,000,000");
    }

    #[test]
    fn long_asset_names_elide_in_the_middle() {
        let long = "HOSKY C(ash Grab)NFT 1729 Special Edition";
        let out = elide(long, 26);
        assert!(out.contains('…'));
        assert!(out.chars().count() <= 27);
        assert_eq!(elide("short", 26), "short");
    }
}
