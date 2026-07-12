//! `DistributionWaterfall` — how a buyer's payment flows down to what lands
//! in each party's wallet under settle-as-you-mint.
//!
//! The recurring confusion: an artist is told "50/50" but sees less, because
//! 50% is of the **distributable**, not of what the buyer paid — NFT min-ADA,
//! network fee, and the platform fee come off the top first. This widget makes
//! that flow legible: a stacked proportion bar (where every lovelace of the
//! gross goes) over a labelled breakdown (the exact waterfall).
//!
//! One widget, three lifecycle modes ([`WaterfallMode`]) and two audiences:
//! - **Projected** — pre-mint, on a hypothetical sale at the configured price.
//! - **Live** — during the mint, actuals so far.
//! - **Final** — ended, a historical artifact.
//!
//! The manager passes `highlight = None` (all parties neutral); a party
//! dashboard passes `highlight = Some(their_name)` to emphasise their slice.
//!
//! Pure VM in, nothing computed here beyond layout — the caller builds the
//! numbers (from `plan_inline_distribution` for a projection, or the recorded
//! per-tx ledger for actuals).

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme;

/// Where in its lifecycle the figures come from. Drives the badge + framing
/// only; the waterfall shape is identical across modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterfallMode {
    /// Pre-mint projection (a hypothetical sale at the configured price).
    Projected,
    /// Mid-mint actuals so far.
    Live,
    /// Ended — final actuals, a historical artifact.
    Final,
}

impl WaterfallMode {
    fn badge(self) -> (&'static str, Color32) {
        match self {
            WaterfallMode::Projected => ("PROJECTED", theme::ACCENT_BLUE),
            WaterfallMode::Live => ("LIVE", theme::ACCENT_GREEN),
            WaterfallMode::Final => ("FINAL", theme::TEXT_SECONDARY),
        }
    }
}

/// One distribution party (founder / artist / …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterfallParty {
    pub name: String,
    /// Configured basis points of the **distributable** (snapshot).
    pub share_bps: u32,
    /// This party's lovelace in this waterfall.
    pub lovelace: u64,
}

/// The waterfall view-model. Construct with the figures, then [`show`].
///
/// [`show`]: DistributionWaterfall::show
#[derive(Debug, Clone)]
pub struct DistributionWaterfall {
    pub mode: WaterfallMode,
    /// Free-text basis, e.g. `"per 70 ADA sale"` / `"across 142 sales"`.
    pub basis: String,
    /// Buyer payments in scope (the top of the waterfall).
    pub gross_lovelace: u64,
    /// NFT min-ADA delivered to buyers (rides the NFT).
    pub delivery_lovelace: u64,
    /// Inline overpayment/shortfall refunds returned to buyers.
    pub refund_lovelace: u64,
    /// Network (tx) fees.
    pub network_fee_lovelace: u64,
    /// Platform fee.
    pub platform_fee_lovelace: u64,
    /// Optional snapshot label for the platform fee, e.g. `"5%, min 2 ADA"`.
    pub platform_fee_label: Option<String>,
    /// The residual split among the parties.
    pub distributable_lovelace: u64,
    pub parties: Vec<WaterfallParty>,
    /// Party name to emphasise ("your" view); `None` = manager/neutral.
    pub highlight: Option<String>,
}

impl DistributionWaterfall {
    /// Render the waterfall into `ui`.
    pub fn show(&self, ui: &mut Ui) {
        // ── Mode badge + basis caption ────────────────────────────
        ui.horizontal(|ui| {
            let (txt, col) = self.mode.badge();
            badge(ui, txt, col);
            if !self.basis.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&self.basis).small().color(theme::TEXT_MUTED));
            }
        });
        ui.add_space(6.0);

        // ── Stacked proportion bar (full width = gross) ───────────
        self.render_bar(ui);
        ui.add_space(8.0);

        // ── Labelled breakdown ────────────────────────────────────
        self.render_breakdown(ui);
    }

    /// The colour a deduction / party segment paints in. Parties cycle a
    /// palette; the highlighted party stays vivid while the rest dim.
    fn party_color(&self, idx: usize, name: &str) -> Color32 {
        const PALETTE: [Color32; 5] = [
            theme::ACCENT_BLUE,
            theme::ACCENT_MAGENTA,
            theme::ACCENT_CYAN,
            theme::ACCENT_GREEN,
            theme::ACCENT_ORANGE,
        ];
        let c = PALETTE[idx % PALETTE.len()];
        match &self.highlight {
            Some(h) if h != name => dim(c),
            _ => c,
        }
    }

    fn render_bar(&self, ui: &mut Ui) {
        let gross = self.gross_lovelace.max(1);
        let h = 18.0;
        let width = ui.available_width().max(40.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, theme::BG_HIGHLIGHT);

        // Segments, in waterfall order. Deductions are muted; the platform
        // fee amber; parties take the palette.
        let mut segs: Vec<(u64, Color32)> = Vec::new();
        if self.delivery_lovelace > 0 {
            segs.push((self.delivery_lovelace, theme::TEXT_MUTED));
        }
        if self.refund_lovelace > 0 {
            segs.push((self.refund_lovelace, theme::TEXT_SECONDARY));
        }
        if self.network_fee_lovelace > 0 {
            segs.push((self.network_fee_lovelace, theme::TEXT_MUTED));
        }
        if self.platform_fee_lovelace > 0 {
            segs.push((self.platform_fee_lovelace, theme::ACCENT_YELLOW));
        }
        for (i, p) in self.parties.iter().enumerate() {
            segs.push((p.lovelace, self.party_color(i, &p.name)));
        }

        let mut x = rect.left();
        for (lov, color) in segs {
            let w = (lov as f32 / gross as f32) * width;
            if w <= 0.0 {
                continue;
            }
            let seg = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, h));
            painter.rect_filled(seg, 0.0, color);
            x += w;
        }
        // Thin outline so a near-empty bar still reads as a bar.
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(0.5_f32, theme::BG_HIGHLIGHT),
            StrokeKind::Inside,
        );
    }

    fn render_breakdown(&self, ui: &mut Ui) {
        // Top line — total received (works across all modes: a single sale
        // projected, or the aggregate so far / final).
        line(
            ui,
            "Inbound",
            self.gross_lovelace,
            theme::TEXT_PRIMARY,
            true,
            0.0,
        );

        // Deductions off the top (skip zero rows).
        deduction(
            ui,
            "NFT min-ADA",
            "to buyer, rides the NFT",
            self.delivery_lovelace,
        );
        deduction(ui, "Refunds", "back to buyer", self.refund_lovelace);
        deduction(ui, "Network fee", "", self.network_fee_lovelace);
        if self.platform_fee_lovelace > 0 {
            let note = self.platform_fee_label.as_deref().unwrap_or("");
            deduction(ui, "Platform fee", note, self.platform_fee_lovelace);
        }

        ui.add_space(2.0);
        ui.separator();
        line(
            ui,
            "Distributable",
            self.distributable_lovelace,
            theme::TEXT_PRIMARY,
            true,
            0.0,
        );

        // Party split — each indented, with their share %, highlighted one
        // emphasised + a "you" marker.
        for (i, p) in self.parties.iter().enumerate() {
            let is_you = self.highlight.as_deref() == Some(p.name.as_str());
            let color = self.party_color(i, &p.name);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let mut name = RichText::new(format!("{}  {}%", p.name, p.share_bps / 100))
                    .small()
                    .color(color);
                if is_you {
                    name = name.strong();
                }
                ui.label(name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_you {
                        ui.label(RichText::new("you").small().strong().color(theme::ACCENT));
                        ui.add_space(6.0);
                    }
                    let mut amt = RichText::new(format!("{} ADA", ada(p.lovelace)))
                        .monospace()
                        .small()
                        .color(if is_you { theme::TEXT_PRIMARY } else { color });
                    if is_you {
                        amt = amt.strong();
                    }
                    ui.label(amt);
                });
            });
        }
    }
}

// ── Render helpers ──────────────────────────────────────────────────

/// A small filled mode badge.
fn badge(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            36,
        ))
        .stroke(Stroke::new(1.0_f32, color))
        .corner_radius(egui::CornerRadius::same(3))
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        });
}

/// A `label … amount ADA` row (indented `indent` px, optionally strong).
fn line(ui: &mut Ui, label: &str, lovelace: u64, color: Color32, strong: bool, indent: f32) {
    ui.horizontal(|ui| {
        if indent > 0.0 {
            ui.add_space(indent);
        }
        let mut l = RichText::new(label).small().color(color);
        if strong {
            l = l.strong();
        }
        ui.label(l);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut a = RichText::new(format!("{} ADA", ada(lovelace)))
                .monospace()
                .small()
                .color(color);
            if strong {
                a = a.strong();
            }
            ui.label(a);
        });
    });
}

/// An indented `− label (note) … amount` deduction row; skipped when zero.
fn deduction(ui: &mut Ui, label: &str, note: &str, lovelace: u64) {
    if lovelace == 0 {
        return;
    }
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let text = if note.is_empty() {
            format!("- {label}")
        } else {
            format!("- {label}  ({note})")
        };
        ui.label(RichText::new(text).small().color(theme::TEXT_SECONDARY));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{} ADA", ada(lovelace)))
                    .monospace()
                    .small()
                    .color(theme::TEXT_SECONDARY),
            );
        });
    });
}

/// Lovelace → ADA, 3 decimals (millilovelace precision — plenty for a
/// distribution report, and tidier than 6 dp on aggregate totals).
fn ada(lovelace: u64) -> String {
    format!("{:.3}", lovelace as f64 / 1_000_000.0)
}

/// Dim a colour for the non-highlighted parties.
fn dim(c: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 90)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ada_formats_three_dp() {
        assert_eq!(ada(32_300_000), "32.300");
        assert_eq!(ada(0), "0.000");
    }
}
