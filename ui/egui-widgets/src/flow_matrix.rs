//! `FlowMatrix` — who paid whom, across MANY wallets at once.
//!
//! Every other flow widget in this catalog takes one subject: `FlowLedger` is
//! one wallet's movements, `CustodyWalk` is one sum's provenance, `CapitalFlow`
//! is one pool leaving. They all assume you already know where to look. This is
//! the face for when you do not — a project with twenty-odd wallets and a few
//! hundred counterparties, and the question "where should I even start".
//!
//! ## Why a matrix and not a node-link graph
//!
//! The intuitive answer to "show me multi-wallet flows" is a graph, and at this
//! size it is the wrong one: twenty subjects plus a few hundred counterparties
//! is a few hundred nodes, which is the same hairball this catalog already
//! rejected for the asset field. More importantly it buries the finding that
//! actually cracks a multi-wallet case — **two wallets paying the SAME
//! counterparty**. In a node-link that is a pair of edges lost among hundreds;
//! in a matrix it is a column with two marks in it, and your eye finds it for
//! free. Blocks of shared payees are how a set of wallets is shown to be one
//! interest.
//!
//! A matrix also never overlaps, so it degrades into "dense" rather than
//! "illegible" as the data grows.
//!
//! ## One unit at a time, always
//!
//! Cardano quantities are raw integers and decimals are not on-chain, so ADA
//! and a stablecoin's smallest unit are **not comparable numbers**. Summing or
//! co-scaling them would be meaningless. The widget therefore renders exactly
//! one unit and names it; the caller filters.
//!
//! ## Encoding
//!
//! - **Direction is polarity**, so the ramp is diverging: one hue for value
//!   leaving the row's wallet, another for value arriving, a neutral gap at
//!   zero. Never red/green — that reads as good/bad rather than out/in.
//! - **Magnitude is intensity, on a LOG scale**, stated in the legend. Treasury
//!   flows span six orders of magnitude; on a linear ramp everything except the
//!   largest cell is invisible, which is a chart that shows one fact.
//! - **An unresolved payer is its own column**, not a hidden caveat. Offline
//!   walks cannot name the funder of most receipts, and that column being the
//!   biggest one on screen is the honest picture of what is known.

use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::selection::Selection;
use crate::time_spine::SpineState;

/// One directed movement of one unit between two parties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatrixFlow<'a> {
    pub timestamp: i64,
    /// The row: one of the project's own wallets.
    pub party: &'a str,
    /// The column. EMPTY means the payer could not be resolved — rendered as
    /// its own column rather than dropped.
    pub counterparty: &'a str,
    /// Signed from `party`'s view: negative left that wallet.
    pub quantity: i64,
}

pub struct FlowMatrixResponse {
    pub response: Response,
    /// `(party, counterparty)` under the pointer.
    pub hovered: Option<(String, String)>,
    pub rows_shown: usize,
    pub cols_shown: usize,
    /// Rows and columns beyond the cap — stated, never silently dropped.
    pub rows_hidden: usize,
    pub cols_hidden: usize,
    pub flows_in_window: usize,
}

pub struct FlowMatrix<'a> {
    flows: &'a [MatrixFlow<'a>],
    spine: &'a SpineState,
    selection: &'a mut Selection,
    unit_label: &'a str,
    label: Option<&'a dyn Fn(&str) -> String>,
    max_rows: usize,
    max_cols: usize,
    cell: f32,
}

/// Out of the row's wallet, and into it. Two hues from the catalog's
/// categorical order — deliberately not red/green.
const OUT: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x2e);
const IN: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
/// What the empty counterparty is called on screen.
const UNKNOWN: &str = "unresolved payer";

impl<'a> FlowMatrix<'a> {
    /// `flows` must already be filtered to ONE unit.
    pub fn new(
        flows: &'a [MatrixFlow<'a>],
        unit_label: &'a str,
        spine: &'a SpineState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            flows,
            spine,
            selection,
            unit_label,
            label: None,
            max_rows: 14,
            max_cols: 18,
            cell: 26.0,
        }
    }

    pub fn label(mut self, f: &'a dyn Fn(&str) -> String) -> Self {
        self.label = Some(f);
        self
    }

    pub fn max_rows(mut self, n: usize) -> Self {
        self.max_rows = n.max(1);
        self
    }

    pub fn max_cols(mut self, n: usize) -> Self {
        self.max_cols = n.max(1);
        self
    }

    pub fn cell_size(mut self, px: f32) -> Self {
        self.cell = px.clamp(10.0, 64.0);
        self
    }

    pub fn show(self, ui: &mut Ui) -> FlowMatrixResponse {
        let Self {
            flows,
            spine,
            selection,
            unit_label,
            label,
            max_rows,
            max_cols,
            cell,
        } = self;

        let name = |k: &str| -> String {
            if k.is_empty() {
                return UNKNOWN.to_string();
            }
            match label {
                Some(f) => f(k),
                None => elide(k),
            }
        };

        // ── aggregate, inside the spine's window ──────────────────────────
        // The playhead REVEALS and the brush FILTERS, exactly as every other
        // face on this spine.
        let (lo, hi) = spine.filter_range();
        let mut agg: std::collections::HashMap<(&str, &str), Cell> =
            std::collections::HashMap::new();
        let mut row_mag: std::collections::HashMap<&str, u128> = std::collections::HashMap::new();
        let mut col_mag: std::collections::HashMap<&str, u128> = std::collections::HashMap::new();
        let mut in_window = 0usize;
        for f in flows {
            if f.timestamp > spine.playhead || f.timestamp < lo || f.timestamp > hi {
                continue;
            }
            in_window += 1;
            let mag = f.quantity.unsigned_abs() as u128;
            let e = agg.entry((f.party, f.counterparty)).or_default();
            e.net += f.quantity as i128;
            e.gross += mag;
            e.count += 1;
            e.first = if e.first == 0 {
                f.timestamp
            } else {
                e.first.min(f.timestamp)
            };
            e.last = e.last.max(f.timestamp);
            *row_mag.entry(f.party).or_default() += mag;
            *col_mag.entry(f.counterparty).or_default() += mag;
        }

        // Rank by gross movement so the busiest wallets and payees lead.
        let mut rows: Vec<&str> = row_mag.keys().copied().collect();
        rows.sort_by_key(|k| (std::cmp::Reverse(row_mag[k]), *k));
        let rows_hidden = rows.len().saturating_sub(max_rows);
        rows.truncate(max_rows);
        let mut cols: Vec<&str> = col_mag.keys().copied().collect();
        cols.sort_by_key(|k| (std::cmp::Reverse(col_mag[k]), *k));
        let cols_hidden = cols.len().saturating_sub(max_cols);
        cols.truncate(max_cols);

        // ── layout ────────────────────────────────────────────────────────
        let row_w = 150.0;
        let head_h = 96.0;
        let grid_w = cols.len() as f32 * cell;
        let grid_h = rows.len() as f32 * cell;
        let total = Vec2::new(
            row_w + grid_w.max(60.0) + 8.0,
            head_h + grid_h.max(30.0) + 26.0,
        );
        let (rect, response) = ui.allocate_exact_size(total, Sense::click());
        let painter = ui.painter_at(rect.expand(2.0));
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let small = egui::TextStyle::Small.resolve(ui.style());

        if rows.is_empty() {
            painter.text(
                rect.left_top() + vec2(0.0, 4.0),
                Align2::LEFT_TOP,
                format!("no {unit_label} moved in this window"),
                small,
                muted,
            );
            return FlowMatrixResponse {
                response,
                hovered: None,
                rows_shown: 0,
                cols_shown: 0,
                rows_hidden,
                cols_hidden,
                flows_in_window: in_window,
            };
        }

        let grid_origin = pos2(rect.left() + row_w, rect.top() + head_h);
        // Log scale, normalised across the OCCUPIED range rather than from
        // zero. Treasury flows span orders of magnitude but rarely start near
        // 1, so `ln(v)/ln(max)` bunches every cell into the top of the ramp and
        // the chart stops encoding magnitude at all — which the first
        // screenshot showed plainly. Spanning [min, max] restores the contrast
        // the whole face depends on.
        let peak = agg.values().map(|c| c.gross).max().unwrap_or(1).max(1);
        let floor = agg.values().map(|c| c.gross).min().unwrap_or(1).max(1);
        let lo_log = (floor as f64).ln();
        let span_log = ((peak as f64).ln() - lo_log).max(1e-6);

        // ── column headers (rotated — horizontal text will not fit) ───────
        for (ci, c) in cols.iter().enumerate() {
            let galley = painter.layout_no_wrap(
                truncate(&name(c), 18),
                small.clone(),
                if c.is_empty() { muted } else { ink },
            );
            let anchor = pos2(
                grid_origin.x + ci as f32 * cell + cell * 0.5,
                grid_origin.y - 6.0,
            );
            let shape = egui::epaint::TextShape::new(anchor, galley, muted)
                .with_angle_and_anchor(-std::f32::consts::FRAC_PI_2, Align2::LEFT_CENTER);
            painter.add(shape);
        }

        // ── cells ─────────────────────────────────────────────────────────
        let mut hovered: Option<(String, String)> = None;
        for (ri, r) in rows.iter().enumerate() {
            for (ci, c) in cols.iter().enumerate() {
                let cr = Rect::from_min_size(
                    pos2(
                        grid_origin.x + ci as f32 * cell,
                        grid_origin.y + ri as f32 * cell,
                    ),
                    Vec2::splat(cell - 1.5),
                );
                let Some(v) = agg.get(&(*r, *c)) else {
                    // An empty cell is information too — keep the grid legible.
                    painter.rect_filled(
                        cr,
                        CornerRadius::same(2),
                        ui.visuals().faint_bg_color.linear_multiply(0.4),
                    );
                    continue;
                };
                // Floor at 0.25 so the smallest cell still reads as present —
                // "there was a payment here" is itself the finding.
                let t = (0.25
                    + 0.75 * (((v.gross.max(1) as f64).ln() - lo_log) / span_log).clamp(0.0, 1.0))
                    as f32;
                let base = if v.net < 0 { OUT } else { IN };
                painter.rect_filled(cr, CornerRadius::same(2), base.gamma_multiply(t));
                if response.hover_pos().is_some_and(|p| cr.contains(p)) {
                    hovered = Some((r.to_string(), c.to_string()));
                    painter.rect_stroke(
                        cr,
                        CornerRadius::same(2),
                        Stroke::new(1.5_f32, ink),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }

        // ── row labels, with the selection's emphasis ─────────────────────
        for (ri, r) in rows.iter().enumerate() {
            let e = selection.emphasis(r);
            let y = grid_origin.y + ri as f32 * cell + cell * 0.5;
            painter.text(
                pos2(rect.left() + row_w - 8.0, y),
                Align2::RIGHT_CENTER,
                truncate(&name(r), 22),
                small.clone(),
                ink.gamma_multiply(e),
            );
        }

        // ── legend + what is NOT shown ────────────────────────────────────
        let mut note = format!("{unit_label} · log scale · out / in");
        if rows_hidden > 0 || cols_hidden > 0 {
            note.push_str(&format!(
                "  ·  {rows_hidden} more wallets, {cols_hidden} more counterparties NOT shown"
            ));
        }
        painter.text(
            pos2(rect.left(), rect.bottom() - 12.0),
            Align2::LEFT_TOP,
            note,
            small.clone(),
            muted,
        );

        // ── tooltip + click-to-pin ────────────────────────────────────────
        if let Some((r, c)) = &hovered
            && let Some(v) = agg.get(&(r.as_str(), c.as_str()))
        {
            let (rn, cn) = (name(r), name(c));
            let (net, gross, count) = (v.net, v.gross, v.count);
            let (first, last) = (v.first, v.last);
            egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    ui.layer_id(),
                    ui.id().with("fm-tip"),
                    egui::PopupAnchor::Pointer,
                )
                .show(|ui| {
                    ui.set_max_width(320.0);
                    ui.label(egui::RichText::new(format!("{rn}  ->  {cn}")).strong());
                    ui.label(
                        egui::RichText::new(format!(
                            "net {net} · gross {gross} {unit_label} over {count} flows"
                        ))
                        .small(),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} .. {}",
                            crate::time_spine::format_date(first),
                            crate::time_spine::format_date(last)
                        ))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                    );
                    if c.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "the payer could not be resolved — the receipt is exact, its source is not",
                            )
                            .small()
                            .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
        }
        if response.clicked() {
            match &hovered {
                // Clicking a cell watches the COUNTERPARTY — the row is already
                // a known project wallet; the column is the lead worth pulling.
                Some((_, c)) if !c.is_empty() => selection.toggle_pin(c.clone()),
                Some((r, _)) => selection.toggle_pin(r.clone()),
                None => selection.clear_pin(),
            }
        }

        FlowMatrixResponse {
            response,
            hovered,
            rows_shown: rows.len(),
            cols_shown: cols.len(),
            rows_hidden,
            cols_hidden,
            flows_in_window: in_window,
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Cell {
    net: i128,
    gross: u128,
    count: usize,
    first: i64,
    last: i64,
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let head: String = s.chars().take(n.saturating_sub(1)).collect();
    format!("{head}…")
}

fn elide(key: &str) -> String {
    if key.len() <= 18 {
        return key.to_string();
    }
    format!("{}…{}", &key[..10], &key[key.len() - 5..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Id, Pos2, Rect, vec2};

    fn run(
        flows: &[MatrixFlow<'_>],
        spine: &SpineState,
        sel: &mut Selection,
    ) -> FlowMatrixResponse {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let mut out = None;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
            ..Default::default()
        };
        ctx.begin_pass(raw);
        egui::Area::new(Id::new("fm")).show(&ctx, |ui| {
            ui.set_min_size(vec2(900.0, 600.0));
            out = Some(FlowMatrix::new(flows, "ADA", spine, sel).show(ui));
        });
        let _ = ctx.end_pass();
        out.unwrap()
    }

    fn f<'a>(t: i64, p: &'a str, c: &'a str, q: i64) -> MatrixFlow<'a> {
        MatrixFlow {
            timestamp: t,
            party: p,
            counterparty: c,
            quantity: q,
        }
    }

    /// The finding the form exists for: two wallets paying the SAME
    /// counterparty is one column with two filled cells.
    #[test]
    fn a_shared_counterparty_is_one_column() {
        let flows = [
            f(10, "walletA", "payee", -100),
            f(11, "walletB", "payee", -200),
            f(12, "walletA", "other", -5),
        ];
        let mut sel = Selection::default();
        let r = run(&flows, &SpineState::new((0, 100)), &mut sel);
        assert_eq!(r.rows_shown, 2, "two paying wallets");
        assert_eq!(r.cols_shown, 2, "and two distinct payees");
        assert_eq!(r.flows_in_window, 3);
    }

    /// The playhead reveals and the brush filters — same verbs as every other
    /// face on the spine.
    #[test]
    fn the_spine_reveals_and_filters() {
        let flows = [
            f(10, "walletA", "early", -100),
            f(90, "walletA", "late", -100),
        ];
        let mut sel = Selection::default();
        let mut spine = SpineState::new((0, 100));

        spine.set_playhead(50);
        let r = run(&flows, &spine, &mut sel);
        assert_eq!(r.flows_in_window, 1, "the late flow has not happened yet");

        spine.set_playhead(100);
        assert_eq!(run(&flows, &spine, &mut sel).flows_in_window, 2);

        spine.set_brush(Some((80, 100)));
        let r = run(&flows, &spine, &mut sel);
        assert_eq!(r.flows_in_window, 1, "brushed to the late window");
        assert_eq!(r.cols_shown, 1);
    }

    /// An unresolvable payer is a COLUMN, not a dropped row. Most receipts in
    /// an offline walk have one, and hiding them would misreport coverage.
    #[test]
    fn the_unresolved_payer_gets_its_own_column() {
        let flows = [f(10, "walletA", "", 500), f(11, "walletA", "known", 10)];
        let mut sel = Selection::default();
        let r = run(&flows, &SpineState::new((0, 100)), &mut sel);
        assert_eq!(r.cols_shown, 2, "the empty counterparty is a column");
        assert_eq!(r.flows_in_window, 2);
    }

    /// Caps are STATED. A matrix silently truncated to its top rows reads as
    /// the whole picture.
    #[test]
    fn hidden_rows_and_columns_are_reported() {
        let names: Vec<String> = (0..30).map(|i| format!("wallet{i:02}")).collect();
        let cps: Vec<String> = (0..40).map(|i| format!("payee{i:02}")).collect();
        let flows: Vec<MatrixFlow<'_>> = (0..30)
            .map(|i| f(10, names[i].as_str(), cps[i].as_str(), -(i as i64 + 1)))
            .collect();
        let mut sel = Selection::default();
        let r = run(&flows, &SpineState::new((0, 100)), &mut sel);
        assert_eq!(r.rows_shown, 14);
        assert_eq!(r.rows_hidden, 16, "30 wallets, 14 shown");
        assert_eq!(r.cols_shown, 18);
        assert_eq!(r.cols_hidden, 12);
    }
}
