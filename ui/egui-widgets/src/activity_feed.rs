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
    /// Secondary amount line (a converted value, a fee, a note).
    pub secondary: Option<String>,
    /// Counterparty, shown muted when there are no tags to carry the identity.
    pub counterparty: Option<&'a str>,
    pub tx_id: Option<&'a str>,
}

impl<'a> ActivityEntry<'a> {
    pub fn new(timestamp: i64, amount: i128) -> Self {
        Self {
            timestamp,
            amount,
            tags: Vec::new(),
            assets: Vec::new(),
            secondary: None,
            counterparty: None,
            tx_id: None,
        }
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

    pub fn tx_id(mut self, tx_id: &'a str) -> Self {
        self.tx_id = Some(tx_id);
        self
    }
}

#[derive(Default)]
pub struct ActivityFeedResponse {
    /// Index of the card clicked this frame.
    pub clicked: Option<usize>,
}

pub struct ActivityFeed<'a> {
    entries: &'a [ActivityEntry<'a>],
    format_amount: &'a dyn Fn(i128) -> String,
    show_day_headers: bool,
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
        }
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
            if self.card(ui, entry, &stamp) {
                resp.clicked = Some(i);
            }
            ui.add_space(4.0);
        }
        resp
    }

    /// One card. Returns whether it was clicked.
    fn card(&self, ui: &mut Ui, entry: &ActivityEntry<'_>, stamp: &str) -> bool {
        let inner = Frame::new()
            .fill(theme::BG_SECONDARY)
            .stroke(Stroke::new(1.0_f32, theme::BORDER))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    // Left: what it was, and when.
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            for tag in &entry.tags {
                                Chip::new(tag.label).variant(tag.variant).show(ui);
                            }
                            // With no tag to carry identity, the counterparty
                            // is the only "what was this" the card has.
                            let bare = entry.counterparty.filter(|_| entry.tags.is_empty());
                            if let Some(cp) = bare {
                                ui.label(
                                    RichText::new(elide(cp, 28))
                                        .color(theme::TEXT_SECONDARY)
                                        .monospace(),
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add(RelativeTime::new(entry.timestamp));
                            ui.label(
                                RichText::new(format!("· {stamp}"))
                                    .color(theme::TEXT_MUTED)
                                    .small(),
                            );
                        });
                    });

                    // Right: what it cost.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        ui.vertical(|ui| {
                            ui.with_layout(
                                egui::Layout::top_down(egui::Align::RIGHT),
                                |ui| {
                                    let color = if entry.amount >= 0 {
                                        theme::SUCCESS
                                    } else {
                                        theme::ACCENT_ORANGE
                                    };
                                    ui.label(
                                        RichText::new((self.format_amount)(entry.amount))
                                            .color(color)
                                            .monospace()
                                            .strong(),
                                    );
                                    if let Some(second) = &entry.secondary {
                                        ui.label(
                                            RichText::new(second)
                                                .color(theme::TEXT_MUTED)
                                                .small(),
                                        );
                                    }
                                },
                            );
                        });
                    });
                });

                // Bottom: what moved.
                if !entry.assets.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for asset in entry.assets.iter().take(MAX_PILLS) {
                            asset_pill(ui, asset);
                        }
                        let extra = entry.assets.len().saturating_sub(MAX_PILLS);
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

        let response = ui.interact(
            inner.response.rect,
            ui.id().with(entry.tx_id.unwrap_or(stamp)),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter().rect_stroke(
                inner.response.rect,
                CornerRadius::same(8),
                Stroke::new(1.0_f32, theme::ACCENT_BLUE),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(tx) = entry.tx_id {
            response.clone().on_hover_text(tx);
        }
        response.clicked()
    }
}

/// A named asset with its thumbnail and signed quantity.
fn asset_pill(ui: &mut Ui, asset: &ActivityAsset<'_>) {
    let arrived = asset.quantity >= 0;
    let tint = if arrived {
        theme::SUCCESS
    } else {
        theme::ACCENT_ORANGE
    };
    Frame::new()
        .fill(theme::BG_HIGHLIGHT)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                match asset.image_url {
                    Some(url) => {
                        ui.add(
                            egui::Image::new(url)
                                .fit_to_exact_size(Vec2::splat(18.0))
                                .corner_radius(CornerRadius::same(4)),
                        );
                    }
                    None => initial_disc(ui, asset.label, tint),
                }
                ui.label(
                    RichText::new(elide(asset.label, 26))
                        .color(theme::TEXT_PRIMARY)
                        .small(),
                );
                // Quantity is a signed badge, not a bare number: on a card the
                // direction belongs to the thing that moved.
                let qty = if asset.quantity.abs() == 1 && arrived {
                    "+1".to_string()
                } else {
                    format!("{:+}", asset.quantity)
                };
                ui.label(RichText::new(qty).color(tint).small().strong());
            });
        });
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

    #[test]
    fn day_header_reads_as_a_date() {
        assert_eq!(friendly_day("2026-08-25"), "August 25, 2026");
        assert_eq!(friendly_day("2026-01-01"), "January 1, 2026");
        // Malformed input degrades to the raw string rather than panicking.
        assert_eq!(friendly_day("nonsense"), "nonsense");
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
