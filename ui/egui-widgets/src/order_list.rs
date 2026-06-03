//! `OrderList` — the mint-orders dashboard. VM rows in, action stream out, no
//! widget-owned state beyond ephemeral filter/search (kept in egui memory).
//!
//! Replaces the portal's hand-rolled order list (a flat 500-row dump with no
//! dates and a noisy `refund: not_required` on every line). What it adds:
//!
//! - **Clear dates** — every row carries a relative "time ago" ([`RelativeTime`])
//!   with the absolute UTC timestamp on hover (created + last-updated).
//! - **Summary + filter** — per-status counts as clickable filter chips, so the
//!   headline ("500 failed") is also the filter, plus a search box over order id
//!   / recipient. Both are client-side (egui memory), no re-fetch.
//! - **Quieter rows** — the refund chip only shows when it actually means
//!   something (`pending` / `refunded` / `required`), not on every line.
//! - **Real history** — the expandable drawer renders the full `order_events`
//!   timeline (event · time · tx · detail), which the old view dropped.
//!
//! The host owns IO: `Refresh` / `SetShowSubmitted` re-fetch server-side;
//! `ToggleHistory` asks the host to load that order's events into the row.

use egui::{
    Align, Color32, CornerRadius, Frame, Label, Layout, Margin, RichText, ScrollArea, Sense,
    Stroke, Ui,
};

use crate::chip::{Chip, ChipVariant};
use crate::relative_time::{relative_label, RelativeTime};

// ─────────────────────────────────────────────────────────────────────
// View-model
// ─────────────────────────────────────────────────────────────────────

/// One entry in an order's history timeline (`order_events` row).
#[derive(Clone, Debug)]
pub struct OrderEventRow {
    /// Lifecycle event: `created` / `reserved` / `submitted` / `confirmed` /
    /// `failed` / `refund_required` / `refunded` / `delivered`.
    pub event: String,
    /// Failure reason / refund note, if any.
    pub detail: Option<String>,
    /// Full tx hash (mint on `submitted`, refund on `refunded`). Widget
    /// truncates for display and copies the full value on click.
    pub tx_hash: Option<String>,
    /// Event time (unix seconds).
    pub at: i64,
}

/// Order lifecycle status as a typed enum. The wire status set isn't frozen, so
/// `Custom(String)` keeps it open — a legacy (`reserved` / `minting`) or future
/// state still round-trips through [`OrderStatus::as_str`] and renders (as a
/// muted chip). Build one from the wire string via `OrderStatus::from(s)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OrderStatus {
    Pending,
    Fulfilling,
    Submitted,
    Confirmed,
    Delivered,
    /// Benign terminal — the collection sold out before this order filled.
    Unfulfilled,
    Failed,
    /// Any other lifecycle string (unknown / legacy / future).
    Custom(String),
}

impl OrderStatus {
    /// The canonical lifecycle statuses, attention-first — the order the filter
    /// strip lays its chips out in (matches the engine's `list_orders` intent).
    pub const KNOWN: [OrderStatus; 7] = [
        OrderStatus::Failed,
        OrderStatus::Pending,
        OrderStatus::Fulfilling,
        OrderStatus::Submitted,
        OrderStatus::Confirmed,
        OrderStatus::Delivered,
        OrderStatus::Unfulfilled,
    ];

    /// The wire string — round-trips with [`OrderStatus::from`].
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::Fulfilling => "fulfilling",
            Self::Submitted => "submitted",
            Self::Confirmed => "confirmed",
            Self::Delivered => "delivered",
            Self::Unfulfilled => "unfulfilled",
            Self::Failed => "failed",
            Self::Custom(s) => s,
        }
    }

    /// Semantic badge variant: red failures, amber in-flight, blue submitted,
    /// green done, grey for the benign `unfulfilled` terminal + unknowns.
    pub fn chip_variant(&self) -> ChipVariant {
        match self {
            Self::Delivered | Self::Confirmed => ChipVariant::Success,
            Self::Submitted => ChipVariant::Info,
            Self::Pending | Self::Fulfilling => ChipVariant::Warning,
            Self::Failed => ChipVariant::Danger,
            Self::Unfulfilled | Self::Custom(_) => ChipVariant::Muted,
        }
    }

    /// A distinct accent colour for the filter-chip dot + selected tint.
    pub fn accent(&self) -> Color32 {
        match self {
            Self::Failed => Color32::from_rgb(200, 90, 90),
            Self::Pending | Self::Fulfilling => Color32::from_rgb(210, 180, 110),
            Self::Submitted => Color32::from_rgb(120, 170, 210),
            Self::Confirmed | Self::Delivered => Color32::from_rgb(140, 200, 140),
            Self::Unfulfilled => Color32::from_rgb(120, 140, 170),
            Self::Custom(_) => Color32::from_gray(150),
        }
    }
}

impl From<&str> for OrderStatus {
    fn from(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "fulfilling" => Self::Fulfilling,
            "submitted" => Self::Submitted,
            "confirmed" => Self::Confirmed,
            "delivered" => Self::Delivered,
            "unfulfilled" => Self::Unfulfilled,
            "failed" => Self::Failed,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl From<String> for OrderStatus {
    fn from(s: String) -> Self {
        // Reuse the &str match, but move the string into `Custom` (no re-alloc)
        // when it isn't a known variant.
        match Self::from(s.as_str()) {
            Self::Custom(_) => Self::Custom(s),
            known => known,
        }
    }
}

/// View-model for one order row. Pre-resolved by the caller — `status` is a typed
/// [`OrderStatus`]; `refund_status` stays a raw string (only known ones chip).
#[derive(Clone, Debug)]
pub struct OrderRow {
    /// Order id (ULID or `test:…` harness id). Full value; the widget truncates
    /// for display and copies the full string on click.
    pub order_id: String,
    /// Delivery address (full bech32). Truncated for display, copied in full.
    pub recipient: String,
    /// Units ordered.
    pub quantity: u32,
    /// Lifecycle status (typed; `Custom` for anything off the canonical list).
    pub status: OrderStatus,
    /// Refund axis: `none` / `not_required` / `pending` / `refunded` /
    /// `required` / `failed`. Only the meaningful ones render a chip.
    pub refund_status: String,
    /// Lovelace captured (refundable), if a payment was seen.
    pub paid_lovelace: Option<u64>,
    /// Created (unix seconds) — the primary row timestamp.
    pub created_at: i64,
    /// Last state change (unix seconds) — shown on the time hover.
    pub updated_at: i64,
    /// `true` when the host has the history drawer open for this order.
    pub detail_open: bool,
    /// `true` while the host is fetching this order's events.
    pub detail_loading: bool,
    /// Loaded history (`None` until fetched). `Some(vec![])` = loaded, empty.
    pub events: Option<Vec<OrderEventRow>>,
    /// Failure / refund note from the order detail, shown in the drawer.
    pub note: Option<String>,
}

impl OrderRow {
    /// Minimal constructor; set the optional fields with the field-setters.
    pub fn new(
        order_id: impl Into<String>,
        recipient: impl Into<String>,
        quantity: u32,
        status: impl Into<OrderStatus>,
        created_at: i64,
        updated_at: i64,
    ) -> Self {
        Self {
            order_id: order_id.into(),
            recipient: recipient.into(),
            quantity,
            status: status.into(),
            refund_status: "none".to_string(),
            paid_lovelace: None,
            created_at,
            updated_at,
            detail_open: false,
            detail_loading: false,
            events: None,
            note: None,
        }
    }
}

/// What the host should act on. Filters/search are handled internally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderListAction {
    /// History button toggled — host loads/clears this order's events.
    ToggleHistory { order_id: String },
    /// Re-fetch the current order set.
    Refresh,
    /// "show submitted" toggled — host re-fetches with the new server scope
    /// (`true` includes the happy-path submitted/confirmed/delivered orders).
    SetShowSubmitted(bool),
}

/// Drained by the host after [`OrderList::show`].
#[derive(Default)]
pub struct OrderListResponse {
    pub actions: Vec<OrderListAction>,
}

// ─────────────────────────────────────────────────────────────────────
// Builder
// ─────────────────────────────────────────────────────────────────────

/// Builder — `OrderList::new(&rows).now(now).show(ui)`.
pub struct OrderList<'a> {
    rows: &'a [OrderRow],
    now: i64,
    fetched_at: Option<i64>,
    show_submitted: bool,
    loading: bool,
    error: Option<&'a str>,
}

impl<'a> OrderList<'a> {
    pub fn new(rows: &'a [OrderRow]) -> Self {
        Self {
            rows,
            now: 0,
            fetched_at: None,
            show_submitted: false,
            loading: false,
            error: None,
        }
    }

    /// Pin "now" (unix seconds) — one clock read shared by every relative time
    /// + the "fetched … ago" label, so the whole window agrees.
    pub fn now(mut self, now: i64) -> Self {
        self.now = now;
        self
    }

    /// When the snapshot was fetched (unix seconds) — drives "· fetched 9s ago".
    pub fn fetched_at(mut self, t: Option<i64>) -> Self {
        self.fetched_at = t;
        self
    }

    /// Current server scope — reflected by the "show submitted" checkbox.
    pub fn show_submitted(mut self, b: bool) -> Self {
        self.show_submitted = b;
        self
    }

    /// A fetch is in flight — swaps Refresh for a spinner.
    pub fn loading(mut self, b: bool) -> Self {
        self.loading = b;
        self
    }

    /// A fetch error to surface above the list.
    pub fn error(mut self, e: Option<&'a str>) -> Self {
        self.error = e;
        self
    }

    pub fn show(self, ui: &mut Ui) -> OrderListResponse {
        let mut resp = OrderListResponse::default();

        // ── Controls: summary + age (left), refresh + scope (right) ──────
        let scope = if self.show_submitted {
            "orders"
        } else {
            "needing attention"
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} {scope}", self.rows.len()))
                    .small()
                    .color(Color32::from_gray(185)),
            );
            if let Some(f) = self.fetched_at {
                ui.label(
                    RichText::new(format!("· fetched {}", relative_label(self.now - f)))
                        .small()
                        .color(Color32::from_gray(140)),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.loading {
                    ui.spinner();
                } else if ui.small_button("Refresh").clicked() {
                    resp.actions.push(OrderListAction::Refresh);
                }
                let mut show = self.show_submitted;
                if ui
                    .checkbox(&mut show, "show submitted")
                    .on_hover_text(
                        "Include the happy-path submitted / confirmed / delivered orders",
                    )
                    .changed()
                {
                    resp.actions.push(OrderListAction::SetShowSubmitted(show));
                }
            });
        });

        if let Some(e) = self.error {
            ui.add_space(2.0);
            ui.colored_label(Color32::from_rgb(220, 120, 120), format!("error: {e}"));
        }

        // ── Summary + filter strip: per-status count badges, MULTI-select ─
        let filter_id = ui.id().with("order_list_status_filter");
        let search_id = ui.id().with("order_list_search");
        // Active filter is a SET of status strings — select several at once.
        // Empty = show all.
        let mut active: Vec<String> = ui
            .data_mut(|d| d.get_temp::<Vec<String>>(filter_id))
            .unwrap_or_default();
        let mut search: String = ui
            .data_mut(|d| d.get_temp::<String>(search_id))
            .unwrap_or_default();

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            // Search keeps a stable left anchor as the chips reflow.
            let changed = ui
                .add(
                    egui::TextEdit::singleline(&mut search)
                        .hint_text("search id / address")
                        .desired_width(150.0),
                )
                .changed();
            if changed {
                ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
            }

            // "all" clears the selection.
            if filter_chip(
                ui,
                "all",
                self.rows.len(),
                Color32::from_gray(150),
                active.is_empty(),
            ) {
                active.clear();
            }
            // Known statuses present, in canonical attention-first order.
            for st in OrderStatus::KNOWN {
                let n = self.rows.iter().filter(|r| r.status == st).count();
                if n == 0 {
                    continue;
                }
                let selected = active.iter().any(|s| s == st.as_str());
                if filter_chip(ui, st.as_str(), n, st.accent(), selected) {
                    toggle_status(&mut active, st.as_str());
                }
            }
            // Any `Custom` statuses present (defensive — unknown wire values).
            let mut extras: Vec<&str> = self
                .rows
                .iter()
                .filter_map(|r| match &r.status {
                    OrderStatus::Custom(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect();
            extras.sort_unstable();
            extras.dedup();
            for key in extras {
                let n = self
                    .rows
                    .iter()
                    .filter(|r| r.status.as_str() == key)
                    .count();
                let selected = active.iter().any(|s| s == key);
                if filter_chip(ui, key, n, Color32::from_gray(150), selected) {
                    toggle_status(&mut active, key);
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(filter_id, active.clone()));

        // ── Filter the rows for display ──────────────────────────────────
        let needle = search.trim().to_ascii_lowercase();
        let filtered: Vec<&OrderRow> = self
            .rows
            .iter()
            .filter(|r| active.is_empty() || active.iter().any(|s| s == r.status.as_str()))
            .filter(|r| {
                needle.is_empty()
                    || r.order_id.to_ascii_lowercase().contains(&needle)
                    || r.recipient.to_ascii_lowercase().contains(&needle)
            })
            .collect();

        ui.add_space(4.0);
        ui.separator();

        if self.rows.is_empty() {
            if !self.loading {
                ui.add_space(6.0);
                ui.colored_label(Color32::from_gray(150), "No orders yet.");
            }
            return resp;
        }
        if filtered.is_empty() {
            ui.add_space(6.0);
            ui.colored_label(Color32::from_gray(150), "No orders match the filter.");
            return resp;
        }

        // ── List ─────────────────────────────────────────────────────────
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for o in &filtered {
                    render_row(ui, o, self.now, &mut resp);
                }
            });

        resp
    }
}

// ─────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────

/// A clickable status-filter pill: a status-colour dot, the label, and the count
/// in a badge. Selected → translucent-accent fill + accent border. Returns true
/// on click. The count badge replaces the old trailing-number text so the qty
/// reads as a distinct chip, and the whole strip is multi-select.
fn filter_chip(ui: &mut Ui, label: &str, count: usize, accent: Color32, selected: bool) -> bool {
    let (fill, stroke, text) = if selected {
        (
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 55),
            Stroke::new(1.0, accent),
            Color32::from_gray(235),
        )
    } else {
        (
            Color32::from_gray(34),
            Stroke::NONE,
            Color32::from_gray(200),
        )
    };
    let resp = Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(7))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            // Status-colour dot.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 3.5, accent);
            ui.label(RichText::new(label).small().color(text));
            // Count badge.
            Frame::new()
                .fill(Color32::from_black_alpha(70))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(5, 0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(count.to_string())
                            .small()
                            .strong()
                            .color(text),
                    );
                });
        })
        .response
        .interact(Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// Toggle a status string in/out of the multi-select filter set.
fn toggle_status(active: &mut Vec<String>, key: &str) {
    if let Some(i) = active.iter().position(|s| s == key) {
        active.remove(i);
    } else {
        active.push(key.to_string());
    }
}

/// Refund chips only render for these — `none` / `not_required` are silent.
fn refund_variant(refund_status: &str) -> Option<ChipVariant> {
    match refund_status {
        "refunded" => Some(ChipVariant::Success),
        "pending" => Some(ChipVariant::Warning),
        "required" | "failed" => Some(ChipVariant::Danger),
        _ => None,
    }
}

fn render_row(ui: &mut Ui, o: &OrderRow, now: i64, resp: &mut OrderListResponse) {
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        Chip::new(o.status.as_str())
            .variant(o.status.chip_variant())
            .upper_case(true)
            .show(ui);

        copy_label(
            ui,
            &o.order_id,
            truncate_middle(&o.order_id, 10, 6),
            "order id",
            Color32::from_gray(150),
        );
        ui.label(RichText::new("->").small().color(Color32::from_gray(110)));
        copy_label(
            ui,
            &o.recipient,
            truncate_middle(&o.recipient, 10, 6),
            "recipient",
            Color32::from_gray(190),
        );
        ui.label(
            RichText::new(format!("×{}", o.quantity))
                .small()
                .color(Color32::from_gray(150)),
        );
        if let Some(p) = o.paid_lovelace {
            ui.label(
                RichText::new(format!("{} ADA", fmt_ada(p)))
                    .small()
                    .color(Color32::from_gray(170)),
            );
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // History toggle.
            let label = if o.detail_open {
                "- history"
            } else {
                "history"
            };
            if ui.small_button(label).clicked() {
                resp.actions.push(OrderListAction::ToggleHistory {
                    order_id: o.order_id.clone(),
                });
            }
            // Relative time (created), absolute created + updated on hover.
            ui.add(RelativeTime::new(o.created_at).now(now).size(11.0))
                .on_hover_text(format!(
                    "created {}\nupdated {}",
                    fmt_utc(o.created_at),
                    fmt_utc(o.updated_at)
                ));
            // Refund chip, only when it means something.
            if let Some(v) = refund_variant(&o.refund_status) {
                Chip::new(&format!("refund {}", o.refund_status))
                    .variant(v)
                    .show(ui);
            }
        });
    });

    if o.detail_open {
        render_history(ui, o, now);
    }
    ui.add_space(3.0);
    ui.separator();
}

fn render_history(ui: &mut Ui, o: &OrderRow, now: i64) {
    ui.add_space(2.0);
    ui.indent(("order_history", &o.order_id), |ui| {
        let Some(events) = &o.events else {
            if o.detail_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("loading history…")
                            .small()
                            .color(Color32::from_gray(150)),
                    );
                });
            }
            return;
        };
        if let Some(note) = &o.note {
            ui.label(
                RichText::new(format!("note: {note}"))
                    .small()
                    .color(Color32::from_rgb(210, 170, 120)),
            );
        }
        if events.is_empty() {
            ui.label(
                RichText::new("no events recorded")
                    .small()
                    .color(Color32::from_gray(140)),
            );
            return;
        }
        for e in events {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&e.event).small().strong());
                ui.add(RelativeTime::new(e.at).now(now).size(11.0))
                    .on_hover_text(fmt_utc(e.at));
                if let Some(tx) = &e.tx_hash {
                    copy_label(
                        ui,
                        tx,
                        format!("tx {}", truncate_middle(tx, 8, 6)),
                        "tx hash",
                        Color32::from_gray(160),
                    );
                }
                if let Some(d) = &e.detail {
                    ui.label(RichText::new(d).small().color(Color32::from_gray(150)));
                }
            });
        }
    });
}

/// A monospace, click-to-copy label. Copies `full`, displays `display`.
fn copy_label(ui: &mut Ui, full: &str, display: String, what: &str, color: Color32) {
    let r = ui.add(
        Label::new(RichText::new(display).monospace().small().color(color)).sense(Sense::click()),
    );
    if r.on_hover_text(format!("{what} — click to copy\n{full}"))
        .clicked()
    {
        ui.ctx().copy_text(full.to_string());
    }
}

/// Middle-elide `s` to `head…tail` when it's longer than the kept slice.
fn truncate_middle(s: &str, head: usize, tail: usize) -> String {
    let n = s.chars().count();
    if n <= head + tail + 1 {
        return s.to_string();
    }
    let prefix: String = s.chars().take(head).collect();
    let suffix: String = s.chars().skip(n - tail).collect();
    format!("{prefix}…{suffix}")
}

/// Lovelace → ADA with up to 6 dp, trailing zeros trimmed (`2.5`, not `2.500000`).
fn fmt_ada(lovelace: u64) -> String {
    let s = format!("{:.6}", lovelace as f64 / 1_000_000.0);
    let s = s.trim_end_matches('0');
    s.trim_end_matches('.').to_string()
}

/// Unix seconds → `YYYY-MM-DD HH:MM UTC` (no deps — the wasm/native widget can't
/// pull `chrono`). Civil-from-days is Howard Hinnant's algorithm.
fn fmt_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, m) = (tod / 3600, (tod % 3600) / 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

/// Days since 1970-01-01 → (year, month, day). Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formats_known_epochs() {
        assert_eq!(fmt_utc(0), "1970-01-01 00:00 UTC");
        // 2026-06-04 12:00 UTC = 1780well-known; compute via the algorithm itself
        // is circular, so pin a couple of hand-checked points.
        assert_eq!(fmt_utc(1_000_000_000), "2001-09-09 01:46 UTC");
        assert_eq!(fmt_utc(1_700_000_000), "2023-11-14 22:13 UTC");
    }

    #[test]
    fn ada_trims_trailing_zeros() {
        assert_eq!(fmt_ada(2_500_000), "2.5");
        assert_eq!(fmt_ada(1_000_000), "1");
        assert_eq!(fmt_ada(1_234_560), "1.23456");
    }

    #[test]
    fn truncate_keeps_short_strings() {
        assert_eq!(truncate_middle("abc", 10, 6), "abc");
        assert_eq!(
            truncate_middle("abcdefghijklmnopqrstuvwxyz", 4, 4),
            "abcd…wxyz"
        );
    }
}
