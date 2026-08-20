//! `CustodyWalk` — where a specific sum came from, unit by unit.
//!
//! An indented tree over one traced amount: each row is a share of the parent,
//! and the leaves are the points where the money genuinely entered the wallet.
//!
//! ## What makes this different from a flow chart
//!
//! A flow view says value moved between two parties. This says *these units
//! became those units* — which on a UTxO chain is a fact, because every input
//! names the output it consumes. That distinction is the whole point, so the
//! widget states which one it is showing: [`CustodyStrength::Proven`] for a
//! UTxO chain, [`CustodyStrength::Inferred`] for an account chain where the
//! nearest analogue is instruction ordering. Rendering the two identically
//! invites a reader to treat a reconstruction as a trace.
//!
//! ## Change is not a payment
//!
//! When a wallet spends a large UTxO to make a small payment, the remainder
//! returns to it as change. A leaf that stops at that transaction and names its
//! payee has the answer backwards: the payee received the *payment*, and the
//! change is money the wallet already had. [`WalkNodeKind::Change`] rows are
//! therefore interior nodes — they always continue — and are drawn muted so
//! they never read as an origin.
//!
//! ## Bounds are nodes, not omissions
//!
//! [`WalkNodeKind::BeyondDepth`], [`BudgetExhausted`](WalkNodeKind::BudgetExhausted)
//! and [`Unresolved`](WalkNodeKind::Unresolved) are rendered as real leaves
//! carrying real value, and [`CustodyWalk`] reports
//! [`WalkSummary::is_complete`] as false whenever one is present. A walk that
//! quietly stops looks exactly like a walk that finished; this one has to say
//! so, and the header prints `PARTIAL` when it does.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{CustodyStrength, CustodyWalk, WalkNode, WalkNodeKind, PartyBasis};
//!
//! let nodes = vec![
//!     WalkNode::root(20_520_170_000, "payout to 173 holders"),
//!     WalkNode::received(1, 13_537_040_000, "conduit", PartyBasis::Observed),
//!     WalkNode::change(1, 3_624_660_000, "$mekkaops", PartyBasis::Observed),
//!     WalkNode::received(2, 3_624_660_000, "parking wallet", PartyBasis::Observed),
//! ];
//! CustodyWalk::new(&nodes, &|v| format!("{:.2}", v as f64 / 1e6))
//!     .strength(CustodyStrength::Proven)
//!     .show(ui);
//! ```

use egui::{
    Align2, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Shape, Ui, Vec2, epaint::Mesh,
};

use crate::party_badge::{PartyBadge, PartyBasis};
use crate::timestamp::format_iso8601;

/// Whether a custody trace is a fact or a reconstruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyStrength {
    /// UTxO chain: every input names the output it consumes.
    Proven,
    /// Account chain: reconstructed from instruction ordering. Not a trace of
    /// specific units, and must not be presented as one.
    Inferred,
}

impl CustodyStrength {
    pub fn badge(self) -> &'static str {
        match self {
            CustodyStrength::Proven => "PROVEN",
            CustodyStrength::Inferred => "INFERRED",
        }
    }

    fn explanation(self) -> &'static str {
        match self {
            CustodyStrength::Proven => {
                "Each input names the exact output it consumes, so this traces these \
                 specific units. A fact about the chain, not a reconstruction."
            }
            CustodyStrength::Inferred => {
                "This chain has no input->output link. The path is reconstructed from \
                 instruction ordering and is INFERENCE — do not describe it as a trace \
                 of specific funds."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkNodeKind {
    /// The amount being traced.
    Root,
    /// A genuine receipt — where money entered. A terminal, and the only kind
    /// that counts as resolved.
    Received,
    /// The wallet's own change returning. Always an interior node.
    Change,
    /// Hit the depth ceiling.
    BeyondDepth { hops: usize },
    /// Hit the transaction budget.
    BudgetExhausted,
    /// The parent transaction could not be fetched.
    Unresolved,
}

impl WalkNodeKind {
    /// Whether this leaf answers the question.
    pub fn is_resolved(self) -> bool {
        matches!(self, WalkNodeKind::Received | WalkNodeKind::Root)
    }

    /// Whether this is a bound the walk ran into rather than an answer.
    pub fn is_bound(self) -> bool {
        matches!(
            self,
            WalkNodeKind::BeyondDepth { .. }
                | WalkNodeKind::BudgetExhausted
                | WalkNodeKind::Unresolved
        )
    }
}

/// One row. `depth` 0 is the root; a node's parent is the nearest preceding row
/// with a smaller depth.
pub struct WalkNode<'a> {
    pub depth: usize,
    pub value: i128,
    pub kind: WalkNodeKind,
    pub label: Option<&'a str>,
    pub party_key: Option<&'a str>,
    pub basis: PartyBasis,
    pub stakeless: bool,
    pub timestamp: Option<i64>,
    pub tx_id: Option<&'a str>,
}

impl<'a> WalkNode<'a> {
    fn bare(depth: usize, value: i128, kind: WalkNodeKind) -> Self {
        Self {
            depth,
            value,
            kind,
            label: None,
            party_key: None,
            basis: PartyBasis::Observed,
            stakeless: false,
            timestamp: None,
            tx_id: None,
        }
    }

    pub fn root(value: i128, label: &'a str) -> Self {
        Self {
            label: Some(label),
            ..Self::bare(0, value, WalkNodeKind::Root)
        }
    }

    pub fn received(depth: usize, value: i128, from: &'a str, basis: PartyBasis) -> Self {
        Self {
            label: Some(from),
            basis,
            ..Self::bare(depth, value, WalkNodeKind::Received)
        }
    }

    /// A change output — the wallet paying `payee` and getting the rest back.
    /// The payee is shown only as context; it is never the origin.
    pub fn change(depth: usize, value: i128, payee: &'a str, basis: PartyBasis) -> Self {
        Self {
            label: Some(payee),
            basis,
            ..Self::bare(depth, value, WalkNodeKind::Change)
        }
    }

    pub fn beyond_depth(depth: usize, value: i128, hops: usize) -> Self {
        Self::bare(depth, value, WalkNodeKind::BeyondDepth { hops })
    }

    pub fn budget_exhausted(depth: usize, value: i128) -> Self {
        Self::bare(depth, value, WalkNodeKind::BudgetExhausted)
    }

    pub fn unresolved(depth: usize, value: i128) -> Self {
        Self::bare(depth, value, WalkNodeKind::Unresolved)
    }

    pub fn at(mut self, timestamp: i64) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    pub fn tx_id(mut self, tx_id: &'a str) -> Self {
        self.tx_id = Some(tx_id);
        self
    }

    pub fn party_key(mut self, key: &'a str) -> Self {
        self.party_key = Some(key);
        self
    }

    pub fn stakeless(mut self, stakeless: bool) -> Self {
        self.stakeless = stakeless;
        self
    }

    /// Leaves are the rows that carry the answer; interior nodes are
    /// subdivided by their children and must not be counted alongside them.
    fn is_leaf(&self, next: Option<&WalkNode<'_>>) -> bool {
        next.map(|n| n.depth <= self.depth).unwrap_or(true)
    }
}

/// What a walk adds up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkSummary {
    /// The traced amount (the root).
    pub traced: i128,
    /// Leaf value that reached a genuine receipt.
    pub resolved: i128,
    /// Leaf value stopped by a depth/budget/fetch bound.
    pub bounded: i128,
    pub leaf_count: usize,
}

impl WalkSummary {
    /// True only when every leaf is a receipt.
    pub fn is_complete(&self) -> bool {
        self.bounded == 0
    }

    /// Share of the traced amount that reached an origin, 0.0–1.0.
    pub fn resolved_fraction(&self) -> f64 {
        if self.traced == 0 {
            return 0.0;
        }
        self.resolved as f64 / self.traced as f64
    }
}

/// Summarise a node list. Only leaves are counted — summing every row would
/// double-count each interior node against its own children.
pub fn summarize(nodes: &[WalkNode<'_>]) -> WalkSummary {
    let traced = nodes.first().map(|n| n.value).unwrap_or(0);
    let (mut resolved, mut bounded, mut leaf_count) = (0i128, 0i128, 0usize);
    for (i, n) in nodes.iter().enumerate() {
        if n.depth == 0 || !n.is_leaf(nodes.get(i + 1)) {
            continue;
        }
        leaf_count += 1;
        if n.kind.is_bound() {
            bounded += n.value;
        } else if n.kind.is_resolved() {
            resolved += n.value;
        }
    }
    WalkSummary {
        traced,
        resolved,
        bounded,
        leaf_count,
    }
}

pub struct CustodyWalkResponse {
    pub summary: WalkSummary,
    pub clicked_node: Option<usize>,
}

pub struct CustodyWalk<'a> {
    nodes: &'a [WalkNode<'a>],
    format_value: &'a dyn Fn(i128) -> String,
    strength: CustodyStrength,
    /// Horizontal space between one hop's bars and the next — the room the
    /// ribbons and the labels share.
    column_width: f32,
    height: f32,
    show_header: bool,
}

impl<'a> CustodyWalk<'a> {
    pub fn new(nodes: &'a [WalkNode<'a>], format_value: &'a dyn Fn(i128) -> String) -> Self {
        Self {
            nodes,
            format_value,
            strength: CustodyStrength::Proven,
            column_width: 250.0,
            height: 180.0,
            show_header: true,
        }
    }

    pub fn strength(mut self, strength: CustodyStrength) -> Self {
        self.strength = strength;
        self
    }

    /// Space between hop columns. Wider gives labels more room; the ribbons
    /// stretch to fill it.
    pub fn column_width(mut self, w: f32) -> Self {
        self.column_width = w;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn show_header(mut self, show: bool) -> Self {
        self.show_header = show;
        self
    }
}

/// A node's children — the rows directly beneath it in the flat list.
fn children_of(nodes: &[WalkNode<'_>], idx: usize) -> Vec<usize> {
    let depth = nodes[idx].depth;
    let mut out = Vec::new();
    for (j, n) in nodes.iter().enumerate().skip(idx + 1) {
        if n.depth <= depth {
            break;
        }
        if n.depth == depth + 1 {
            out.push(j);
        }
    }
    out
}

/// Where each node sits: a bar whose HEIGHT is its share of the traced total,
/// in a column set by its distance from the root.
///
/// Bands are allocated by subdivision — a node's children divide its own
/// vertical extent in proportion to their values — so a child is always
/// vertically inside its parent and no ribbon ever needs to cross another.
/// Height carries value; that is the encoding an indented text tree lacks, and
/// the reason a 73.5% leg has to *look* like most of the bar.
fn layout(nodes: &[WalkNode<'_>], plot: Rect, bar_w: f32, col_gap: f32, min_h: f32) -> Vec<Rect> {
    let mut rects = vec![Rect::NOTHING; nodes.len()];
    if nodes.is_empty() {
        return rects;
    }

    /// The geometry that does not change as the recursion descends.
    struct Geom {
        left: f32,
        bar_w: f32,
        col_gap: f32,
        min_h: f32,
    }

    fn place(
        nodes: &[WalkNode<'_>],
        idx: usize,
        top: f32,
        height: f32,
        rects: &mut [Rect],
        g: &Geom,
    ) {
        let (bar_w, min_h) = (g.bar_w, g.min_h);
        let x = g.left + nodes[idx].depth as f32 * (bar_w + g.col_gap);
        rects[idx] = Rect::from_min_size(Pos2::new(x, top), Vec2::new(bar_w, height.max(min_h)));

        let kids = children_of(nodes, idx);
        if kids.is_empty() {
            return;
        }
        let total: i128 = kids.iter().map(|k| nodes[*k].value).sum();
        if total <= 0 {
            return;
        }
        // 1px between siblings so two adjacent bands never read as one.
        let gaps = (kids.len().saturating_sub(1)) as f32;
        let usable = (height - gaps).max(0.0);
        let mut y = top;
        for k in kids {
            let h = usable * (nodes[k].value as f64 / total as f64) as f32;
            place(nodes, k, y, h, rects, g);
            y += h + 1.0;
        }
    }

    place(
        nodes,
        0,
        plot.top(),
        plot.height(),
        &mut rects,
        &Geom {
            left: plot.left(),
            bar_w,
            col_gap,
            min_h,
        },
    );
    rects
}

/// A tapered ribbon from a parent bar's right edge to a child bar's left edge.
///
/// Built as a triangle strip rather than a path: epaint fills convex shapes
/// reliably, and a ribbon between two curves is not one.
fn ribbon(from: Rect, to: Rect, color: Color32) -> Shape {
    const STEPS: usize = 24;
    let (x0, x1) = (from.right(), to.left());
    let mx = (x0 + x1) * 0.5;

    let bez = |p0: f32, p1: f32, t: f32| {
        // Cubic with horizontal control handles at the midpoint — the standard
        // Sankey link, so the flow leaves and arrives horizontally.
        let u = 1.0 - t;
        u * u * u * p0 + 3.0 * u * u * t * p0 + 3.0 * u * t * t * p1 + t * t * t * p1
    };
    let x_at = |t: f32| {
        let u = 1.0 - t;
        u * u * u * x0 + 3.0 * u * u * t * mx + 3.0 * u * t * t * mx + t * t * t * x1
    };

    let mut mesh = Mesh::default();
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32;
        let x = x_at(t);
        let yt = bez(from.top(), to.top(), t);
        let yb = bez(from.bottom(), to.bottom(), t);
        mesh.colored_vertex(Pos2::new(x, yt), color);
        mesh.colored_vertex(Pos2::new(x, yb), color);
        if i > 0 {
            let n = (i as u32) * 2;
            mesh.add_triangle(n - 2, n - 1, n);
            mesh.add_triangle(n - 1, n, n + 1);
        }
    }
    Shape::Mesh(mesh.into())
}

impl<'a> CustodyWalk<'a> {
    /// Colour for a node's bar — kind first, because what a row *is* matters
    /// more here than which party it names.
    fn node_color(&self, kind: WalkNodeKind, ui: &Ui) -> Color32 {
        match kind {
            WalkNodeKind::Root => Color32::from_rgb(0x39, 0x87, 0xe5),
            WalkNodeKind::Received => Color32::from_rgb(0x19, 0x9e, 0x70),
            // Change is a pass-through, not an origin — neutral, never a hue
            // that would let it read as a source.
            WalkNodeKind::Change => ui.visuals().weak_text_color(),
            _ => ui.visuals().warn_fg_color,
        }
    }

    pub fn show(self, ui: &mut Ui) -> CustodyWalkResponse {
        let summary = summarize(self.nodes);
        let muted = ui.visuals().weak_text_color();
        let warn = ui.visuals().warn_fg_color;

        if self.show_header {
            self.header(ui, &summary, muted, warn);
            ui.add_space(8.0);
        }

        if self.nodes.is_empty() {
            return CustodyWalkResponse {
                summary,
                clicked_node: None,
            };
        }

        let depth_max = self.nodes.iter().map(|n| n.depth).max().unwrap_or(0);
        let bar_w = 11.0;
        let col_gap = self.column_width;
        let width = (depth_max as f32 + 1.0) * (bar_w + col_gap);
        let height = self.height;

        // The root's label would otherwise sit exactly where its own outgoing
        // ribbons are widest. Give it a caption band above the plot instead —
        // it names the whole diagram, so it belongs at the top, not inline.
        const CAPTION_H: f32 = 18.0;
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(width.min(ui.available_width()), height + CAPTION_H),
            Sense::click(),
        );
        let plot = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + CAPTION_H), rect.max);
        let rects = layout(self.nodes, plot, bar_w, col_gap, 2.0);
        let painter = ui.painter_at(rect.expand(2.0));

        if let Some(r) = self.nodes.first() {
            let caption = match r.label {
                Some(l) => format!("{}  {l}", (self.format_value)(r.value)),
                None => (self.format_value)(r.value),
            };
            painter.text(
                Pos2::new(rect.left(), rect.top() + CAPTION_H * 0.5),
                Align2::LEFT_CENTER,
                caption,
                FontId::monospace(11.0),
                ui.visuals().text_color(),
            );
        }

        let hovered = resp.hover_pos().and_then(|p| {
            self.nodes
                .iter()
                .enumerate()
                .find(|(i, _)| rects[*i].expand2(Vec2::new(0.0, 1.0)).contains(p))
                .map(|(i, _)| i)
        });

        // Ribbons first, under everything.
        for i in 0..self.nodes.len() {
            for k in children_of(self.nodes, i) {
                let dim = hovered.is_some_and(|h| h != k && h != i);
                let c = self
                    .node_color(self.nodes[k].kind, ui)
                    .gamma_multiply(if dim { 0.12 } else { 0.3 });
                painter.add(ribbon(rects[i], rects[k], c));
            }
        }

        // Bars, then labels. The root is captioned above, so it is skipped.
        for (i, n) in self.nodes.iter().enumerate() {
            let dim = hovered.is_some_and(|h| h != i);
            let c = self
                .node_color(n.kind, ui)
                .gamma_multiply(if dim { 0.45 } else { 1.0 });
            painter.rect_filled(rects[i], 2.0, c);
            if i > 0 {
                self.label(ui, &painter, n, rects[i], col_gap, muted, warn, dim);
            }
        }

        if let Some(i) = hovered {
            self.tooltip(&resp, i, summary.traced);
        }

        CustodyWalkResponse {
            summary,
            clicked_node: resp.clicked().then_some(hovered).flatten(),
        }
    }

    /// Label to the right of a bar. Short bars get no label — a 0.002% leg
    /// cannot carry text without colliding with its neighbours, and the value
    /// is still reachable on hover.
    #[allow(clippy::too_many_arguments)]
    fn label(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        n: &WalkNode<'_>,
        r: Rect,
        // `lane` is the horizontal room before the next column; labels are
        // bounded by it so they never run into the following bar.
        lane: f32,
        muted: Color32,
        warn: Color32,
        dim: bool,
    ) {
        // A bound is ALWAYS labelled, however thin its bar. An untraced leg
        // that renders as a silent sliver defeats the point of drawing bounds
        // at all — it is the one label that must never be dropped for space.
        if r.height() < 11.0 && !n.kind.is_bound() {
            return;
        }
        let alpha = if dim { 0.5 } else { 1.0 };
        let ink = ui.visuals().text_color().gamma_multiply(alpha);
        let soft = muted.gamma_multiply(alpha);

        let x = r.right() + 6.0;
        let y = r.center().y;

        let value = (self.format_value)(n.value);
        let value_w = painter
            .layout_no_wrap(value.clone(), FontId::monospace(11.0), ink)
            .size()
            .x;

        let (text, color) = match n.kind {
            WalkNodeKind::Root => (n.label.unwrap_or("traced amount").to_string(), ink),
            WalkNodeKind::Received => (n.label.unwrap_or("(unlabelled)").to_string(), ink),
            WalkNodeKind::Change => (
                format!("change from paying {}", n.label.unwrap_or("—")),
                soft,
            ),
            WalkNodeKind::BeyondDepth { hops } => (
                format!("beyond {hops} {}", if hops == 1 { "hop" } else { "hops" }),
                warn.gamma_multiply(alpha),
            ),
            WalkNodeKind::BudgetExhausted => {
                ("walk budget exhausted".into(), warn.gamma_multiply(alpha))
            }
            WalkNodeKind::Unresolved => ("unresolved".into(), warn.gamma_multiply(alpha)),
        };
        // A backing plate: labels sit over the ribbons, and unbacked text on a
        // translucent flow is the classic Sankey legibility failure.
        let text_w = painter
            .layout_no_wrap(text.clone(), FontId::proportional(11.0), color)
            .size()
            .x;
        // The plate covers the second line too when there is one — a date
        // hanging below the backing and over a ribbon is the same legibility
        // failure the plate exists to fix.
        let two_line = r.height() >= 26.0 && (n.stakeless || n.timestamp.is_some());
        let plate_w = (value_w + 8.0 + text_w).min(lane - 10.0);
        let plate = Rect::from_min_size(
            Pos2::new(x - 3.0, y - 8.0),
            Vec2::new(plate_w + 6.0, if two_line { 27.0 } else { 16.0 }),
        );
        painter.rect_filled(
            plate,
            2.0,
            ui.visuals()
                .panel_fill
                .gamma_multiply(if dim { 0.6 } else { 0.88 }),
        );
        painter.text(
            Pos2::new(x, y),
            Align2::LEFT_CENTER,
            &value,
            FontId::monospace(11.0),
            ink,
        );
        painter.text(
            Pos2::new(x + value_w + 8.0, y),
            Align2::LEFT_CENTER,
            &text,
            FontId::proportional(11.0),
            color,
        );

        // The address shape and the date are secondary — a second line, only
        // where the bar is tall enough to carry one without crowding.
        if two_line {
            let mut sub = String::new();
            if n.stakeless {
                sub.push_str("no-stake");
            }
            if let Some(ts) = n.timestamp {
                if !sub.is_empty() {
                    sub.push_str(" · ");
                }
                sub.push_str(&format_iso8601(ts, false));
            }
            if !sub.is_empty() {
                painter.text(
                    Pos2::new(x, y + 11.0),
                    Align2::LEFT_CENTER,
                    sub,
                    FontId::monospace(9.0),
                    soft,
                );
            }
        }
    }

    fn tooltip(&self, resp: &Response, i: usize, traced: i128) {
        let n = &self.nodes[i];
        resp.clone().on_hover_ui_at_pointer(|ui| {
            ui.label(
                RichText::new((self.format_value)(n.value))
                    .monospace()
                    .strong(),
            );
            if traced > 0 {
                ui.label(
                    RichText::new(format!(
                        "{:.1}% of the traced amount",
                        n.value as f64 * 100.0 / traced as f64
                    ))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
                );
            }
            match n.kind {
                WalkNodeKind::Received => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("received from").size(11.0));
                        let badge = match n.label {
                            Some(l) => PartyBadge::new(l, n.basis),
                            None => PartyBadge::unlabelled(n.party_key.unwrap_or("")),
                        };
                        badge.stakeless(n.stakeless).text_size(11.0).show(ui);
                    });
                }
                WalkNodeKind::Change => {
                    ui.label(
                        RichText::new(
                            "The wallet's own money returning as change, not a payment \
                             received. The named party got the payment; the walk continues \
                             past it.",
                        )
                        .size(11.0),
                    );
                }
                WalkNodeKind::BeyondDepth { .. }
                | WalkNodeKind::BudgetExhausted
                | WalkNodeKind::Unresolved => {
                    ui.label(
                        RichText::new(
                            "Stopped at a bound, not an origin. This share is NOT \
                             attributed to anything — report it as untraced.",
                        )
                        .size(11.0)
                        .color(ui.visuals().warn_fg_color),
                    );
                }
                WalkNodeKind::Root => {}
            }
            if let Some(tx) = n.tx_id {
                ui.label(
                    RichText::new(tx)
                        .monospace()
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });
    }

    fn header(&self, ui: &mut Ui, summary: &WalkSummary, muted: Color32, warn: Color32) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;

            let (bg, fg) = match self.strength {
                CustodyStrength::Proven => (
                    Color32::from_rgb(0x19, 0x9e, 0x70),
                    ui.visuals().extreme_bg_color,
                ),
                CustodyStrength::Inferred => (warn, ui.visuals().extreme_bg_color),
            };
            let text = RichText::new(self.strength.badge())
                .size(9.0)
                .strong()
                .color(fg);
            let galley = ui.painter().layout_no_wrap(
                self.strength.badge().into(),
                egui::FontId::proportional(9.0),
                fg,
            );
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(galley.size().x + 10.0, galley.size().y + 4.0),
                Sense::hover(),
            );
            ui.painter().rect_filled(rect, 2.0, bg);
            ui.painter().galley(
                rect.center() - galley.size() / 2.0,
                ui.painter().layout_no_wrap(
                    self.strength.badge().into(),
                    egui::FontId::proportional(9.0),
                    fg,
                ),
                fg,
            );
            let _ = text;
            ui.label(
                RichText::new(format!("{} leaves", summary.leaf_count))
                    .size(10.0)
                    .color(muted),
            );

            if summary.is_complete() {
                ui.label(RichText::new("every leaf resolved").size(10.0).color(muted));
            } else {
                ui.label(
                    RichText::new(format!(
                        "PARTIAL — {} untraced ({:.0}% resolved)",
                        (self.format_value)(summary.bounded),
                        summary.resolved_fraction() * 100.0
                    ))
                    .size(10.0)
                    .strong()
                    .color(warn),
                )
                .on_hover_text(
                    "Some legs stopped at a bound rather than an origin. Report the \
                     resolved share, not the whole amount, as traced.",
                );
            }
        })
        .response
        .on_hover_text(self.strength.explanation());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(v: i128) -> WalkNode<'static> {
        WalkNode::root(v, "payout")
    }

    /// The Mekka shape: two direct receipts and a change leg that resolves one
    /// level deeper. Only leaves count, or the interior change row would be
    /// added to its own child.
    #[test]
    fn only_leaves_count_toward_the_total() {
        let nodes = vec![
            root(1_000),
            WalkNode::received(1, 600, "funder", PartyBasis::Observed),
            WalkNode::change(1, 400, "vendor", PartyBasis::Observed),
            WalkNode::received(2, 400, "funder", PartyBasis::Observed),
        ];
        let s = summarize(&nodes);
        assert_eq!(s.traced, 1_000);
        assert_eq!(s.leaf_count, 2, "the change row is interior, not a leaf");
        assert_eq!(s.resolved, 1_000);
        assert!(s.is_complete());
    }

    /// A bound is real value that reached no origin, and must break
    /// completeness rather than being dropped.
    #[test]
    fn a_bound_leaf_makes_the_walk_incomplete() {
        let nodes = vec![
            root(1_000),
            WalkNode::received(1, 700, "funder", PartyBasis::Observed),
            WalkNode::beyond_depth(1, 300, 4),
        ];
        let s = summarize(&nodes);
        assert!(!s.is_complete());
        assert_eq!(s.resolved, 700);
        assert_eq!(s.bounded, 300);
        assert_eq!(s.resolved + s.bounded, s.traced, "no value is lost");
        assert!((s.resolved_fraction() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn budget_and_unresolved_are_bounds_too() {
        for node in [
            WalkNode::budget_exhausted(1, 100),
            WalkNode::unresolved(1, 100),
        ] {
            let nodes = vec![root(100), node];
            assert!(!summarize(&nodes).is_complete());
            assert_eq!(summarize(&nodes).bounded, 100);
        }
    }

    /// Change is never an origin — it must not be counted as resolved.
    #[test]
    fn a_trailing_change_leaf_resolves_nothing() {
        // A malformed walk: change with no child. It answers nothing, so it
        // must not be booked as resolved value.
        let nodes = vec![
            root(500),
            WalkNode::change(1, 500, "vendor", PartyBasis::Observed),
        ];
        let s = summarize(&nodes);
        assert_eq!(s.leaf_count, 1);
        assert_eq!(s.resolved, 0, "change is not an origin");
        assert_eq!(s.bounded, 0);
    }

    #[test]
    fn strength_badges_are_distinct() {
        assert_eq!(CustodyStrength::Proven.badge(), "PROVEN");
        assert_eq!(CustodyStrength::Inferred.badge(), "INFERRED");
        assert_ne!(
            CustodyStrength::Proven.explanation(),
            CustodyStrength::Inferred.explanation()
        );
    }

    #[test]
    fn children_are_direct_descendants_only() {
        let nodes = vec![
            root(1_000),
            WalkNode::received(1, 400, "a", PartyBasis::Observed),
            WalkNode::change(1, 600, "vendor", PartyBasis::Observed),
            WalkNode::received(2, 600, "deep", PartyBasis::Observed),
            WalkNode::received(3, 600, "deeper", PartyBasis::Observed),
        ];
        assert_eq!(children_of(&nodes, 0), vec![1, 2], "grandchildren excluded");
        assert_eq!(children_of(&nodes, 2), vec![3]);
        assert_eq!(children_of(&nodes, 3), vec![4]);
        assert!(children_of(&nodes, 1).is_empty());
    }

    /// Bar HEIGHT is the encoding: a leg worth 60% of the traced sum must
    /// occupy ~60% of its parent's band. This is exactly what the old indented
    /// text tree could not express.
    #[test]
    fn band_height_is_proportional_to_value() {
        let nodes = vec![
            root(1_000),
            WalkNode::received(1, 600, "big", PartyBasis::Observed),
            WalkNode::received(1, 400, "small", PartyBasis::Observed),
        ];
        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 101.0));
        let r = layout(&nodes, plot, 10.0, 100.0, 1.0);

        assert_eq!(r[0].height(), 101.0, "root spans the whole plot");
        // 1px sibling gap comes off the usable height first.
        assert!((r[1].height() - 60.0).abs() < 0.5, "got {}", r[1].height());
        assert!((r[2].height() - 40.0).abs() < 0.5, "got {}", r[2].height());
    }

    /// Children are laid out inside their parent's band, so a ribbon never has
    /// to cross another and the reader can follow a branch by eye.
    #[test]
    fn children_nest_inside_the_parent_band() {
        let nodes = vec![
            root(1_000),
            WalkNode::change(1, 1_000, "vendor", PartyBasis::Observed),
            WalkNode::received(2, 700, "a", PartyBasis::Observed),
            WalkNode::received(2, 300, "b", PartyBasis::Observed),
        ];
        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 200.0));
        let r = layout(&nodes, plot, 10.0, 100.0, 1.0);
        for child in [2usize, 3] {
            assert!(r[child].top() >= r[1].top() - 0.01, "child escapes above");
            assert!(
                r[child].bottom() <= r[1].bottom() + 1.01,
                "child escapes below"
            );
        }
        assert!(r[2].bottom() <= r[3].top(), "siblings must not overlap");
    }

    /// Deeper hops sit further right, so hop count is readable as position.
    #[test]
    fn depth_sets_the_column() {
        let nodes = vec![
            root(100),
            WalkNode::change(1, 100, "v", PartyBasis::Observed),
            WalkNode::received(2, 100, "a", PartyBasis::Observed),
        ];
        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 50.0));
        let r = layout(&nodes, plot, 10.0, 100.0, 1.0);
        assert!(r[0].left() < r[1].left());
        assert!(r[1].left() < r[2].left());
    }

    /// A sub-pixel leg still gets a visible bar rather than vanishing.
    #[test]
    fn a_tiny_leg_keeps_a_minimum_bar() {
        let nodes = vec![
            root(1_000_000),
            WalkNode::received(1, 999_999, "big", PartyBasis::Observed),
            WalkNode::received(1, 1, "dust", PartyBasis::Observed),
        ];
        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 100.0));
        let r = layout(&nodes, plot, 10.0, 100.0, 2.0);
        assert!(r[2].height() >= 2.0, "dust must stay visible");
    }

    #[test]
    fn empty_walk_is_well_defined() {
        let s = summarize(&[]);
        assert_eq!(s.traced, 0);
        assert_eq!(s.resolved_fraction(), 0.0);
        assert!(s.is_complete());
    }
}
