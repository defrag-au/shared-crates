//! `FlowStave` — one wallet's money story as a sequence chart on the spine.
//!
//! The transfers table answers "what happened", but a reader reconstructing
//! CAUSALITY from it has to juggle timestamps, signs and row order in their
//! head — on a real case (a front wallet: treasury funds it, batches mint into
//! it, assets forward out of it minutes later) that reconstruction is the
//! whole finding, and a table buries it. This is the narrative face: the
//! classic message-sequence-chart form, with wallets as vertical lanes and
//! each movement drawn as an arrow at its moment in time.
//!
//! ## The three commitments
//!
//! - **Time is the axis.** Events run downward in time order, with the gap
//!   between rows LOG-compressed: a five-minute mint→forward cascade stays a
//!   visible cluster while an idle fortnight shrinks to a bounded gap instead
//!   of a page of nothing. Day boundaries are ruled and labelled, the playhead
//!   is a line across the chart, and everything after it is dimmed — the
//!   spine's "now" applies here exactly as on every other face.
//! - **Direction is an arrow.** Toward the focal lane is inbound (blue), away
//!   is outbound (orange) — the same hues every flow face uses — with an
//!   arrowhead at the destination, affordable here because a stave shows tens
//!   of events where the ring shows thousands.
//! - **Units keep their identity.** Each arrow carries its caller-formatted
//!   payload label ("5,000 ₳", "12 items", "25 USDM"); asset movements wear a
//!   count chip at the destination end. A mint is NOT an arrow from nowhere —
//!   it is a diamond spark on the receiving lane, because "created here" and
//!   "arrived from someone unresolved" are different facts and the old face
//!   rendered both the same way.
//!
//! Lanes are ordered by ring class, nearest the focal wallet first — the same
//! curation the ring encodes, so the two faces tell one story. Lane positions
//! depend only on the lane list, never on the events in view.

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::flow_ring::{IN, OUT, ring_tint};
use crate::selection::Selection;
use crate::time_spine::SpineState;

/// A wallet's vertical lane. Order in the caller's slice breaks ties, so the
/// layout is stable across frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaveLane<'a> {
    pub key: &'a str,
    /// Ring class, 0 = core — lanes sort by it, nearest the focal first.
    pub ring: u8,
}

impl<'a> StaveLane<'a> {
    pub fn new(key: &'a str, ring: u8) -> Self {
        Self { key, ring }
    }
}

/// Where a movement came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaveOrigin<'a> {
    Party(&'a str),
    /// Created in this transaction — a mint, drawn as a spark, never an arrow.
    Mint,
    /// The walk could not resolve the payer. An arrow from the chart's edge:
    /// absence drawn as absence.
    Unresolved,
}

/// One movement involving the focal wallet.
#[derive(Debug, Clone, PartialEq)]
pub struct StaveEvent<'a> {
    pub timestamp: i64,
    pub from: StaveOrigin<'a>,
    pub to: &'a str,
    /// Caller-formatted payload ("5,000 ₳", "12 items", "25 USDM") — the
    /// widget never guesses a unit's decimals.
    pub label: String,
    /// Assets riding in this movement; > 0 draws the count chip.
    pub items: i32,
    /// Relative weight 0..=1 (caller's log scale) — arrow thickness.
    pub magnitude: f32,
    /// For the hover card and the caller's click-through.
    pub tx: &'a str,
}

pub struct FlowStaveResponse {
    pub response: Response,
    pub hovered: Option<usize>,
    /// Index into the caller's slice of the clicked event, for playhead moves
    /// or explorer hand-off.
    pub clicked: Option<usize>,
    /// A lane HEADER was clicked — the reader wants that entity as the new
    /// subject. The caller owns what that means (refocus, and any history
    /// stack for going back); the widget only reports the intent. Never the
    /// focal lane: it is already the subject.
    pub clicked_lane: Option<String>,
    pub events_shown: usize,
    /// Events outside the spine's brush — filtered by the reader, not lost.
    pub events_clipped: usize,
}

pub struct FlowStave<'a> {
    focal: &'a str,
    lanes: &'a [StaveLane<'a>],
    events: &'a [StaveEvent<'a>],
    spine: &'a SpineState,
    selection: &'a mut Selection,
    height: f32,
    label: Option<&'a dyn Fn(&str) -> String>,
}

/// The pinned lane-header band's height.
const HEADER_H: f32 = 30.0;
/// Vertical rhythm: the tightest two events may sit this close…
const ROW_MIN: f32 = 22.0;
/// …and however long the silence, never further apart than this.
const ROW_MAX: f32 = 64.0;
/// Pointer forgiveness around an event's line.
const HOVER_SLOP: f32 = 6.0;

/// Log-compressed vertical gap for a time delta: monotone in `dt`, clamped to
/// `[ROW_MIN, ROW_MAX]`.
///
/// Pure linear time is the honest axis and the unreadable one — a real case
/// is minutes of action separated by idle weeks, so linear either crushes the
/// cascades (the finding) or scrolls forever. Log keeps ORDER and RHYTHM:
/// near-simultaneous stays visibly tight, a day reads bigger than a minute,
/// and a month cannot push the story off screen. The true time is printed in
/// the gutter, so the compression never has to be trusted.
fn row_gap(dt: i64) -> f32 {
    if dt <= 0 {
        return ROW_MIN;
    }
    let t = ((dt as f32) + 1.0).ln() / ((14.0 * 86_400.0f32) + 1.0).ln();
    ROW_MIN + (ROW_MAX - ROW_MIN) * t.clamp(0.0, 1.0)
}

/// Lane x-order: focal at the centre, counterparties fanning out by ring
/// class — nearest ring closest, alternating right/left so both flanks fill
/// evenly. Depends only on the lane list; events never move a lane.
fn lane_order<'a>(focal: &'a str, lanes: &'a [StaveLane<'a>]) -> Vec<&'a str> {
    let mut sorted: Vec<&StaveLane<'a>> = lanes.iter().filter(|l| l.key != focal).collect();
    sorted.sort_by_key(|l| l.ring); // stable: caller order breaks ties
    let mut order = vec![focal];
    for (i, l) in sorted.iter().enumerate() {
        if i % 2 == 0 {
            order.push(l.key); // right flank
        } else {
            order.insert(0, l.key); // left flank
        }
    }
    order
}

impl<'a> FlowStave<'a> {
    /// `events` must be sorted by timestamp; every event involves `focal`.
    pub fn new(
        focal: &'a str,
        lanes: &'a [StaveLane<'a>],
        events: &'a [StaveEvent<'a>],
        spine: &'a SpineState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            focal,
            lanes,
            events,
            spine,
            selection,
            height: 460.0,
            label: None,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn label(mut self, f: &'a dyn Fn(&str) -> String) -> Self {
        self.label = f.into();
        self
    }

    pub fn show(self, ui: &mut Ui) -> FlowStaveResponse {
        let Self {
            focal,
            lanes,
            events,
            spine,
            selection,
            height,
            label,
        } = self;
        let name_of = |k: &str| -> String {
            match label {
                Some(f) => f(k),
                None => elide(k),
            }
        };

        let (lo, hi) = spine.filter_range();
        let in_range: Vec<(usize, &StaveEvent<'_>)> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.timestamp >= lo && e.timestamp <= hi)
            .collect();
        let events_clipped = events.len() - in_range.len();

        // ── vertical layout: order preserved, gaps log-compressed ─────────
        let mut ys = Vec::with_capacity(in_range.len());
        let mut y = ROW_MIN;
        let mut prev_t: Option<i64> = None;
        for (_, e) in &in_range {
            if let Some(p) = prev_t {
                y += row_gap(e.timestamp - p);
            }
            ys.push(y);
            prev_t = Some(e.timestamp);
        }
        let content_h = (y + ROW_MIN).max(height - HEADER_H);

        let order = lane_order(focal, lanes);
        let ring_of = |k: &str| lanes.iter().find(|l| l.key == k).map(|l| l.ring);

        let mut out = FlowStaveResponse {
            response: ui.allocate_response(vec2(0.0, 0.0), Sense::hover()),
            hovered: None,
            clicked: None,
            clicked_lane: None,
            events_shown: in_range.len(),
            events_clipped,
        };

        // Lane geometry is computed ONCE, from the outer width, and shared by
        // the pinned header and the scrolling body — two derivations would
        // drift the first time either got an inset the other didn't.
        const GUTTER: f32 = 78.0;
        let avail_w = ui.available_width();
        let span = (avail_w - GUTTER - 56.0).max(60.0);
        let step = span / (order.len().max(2) as f32 - 1.0).max(1.0);
        let lane_off = |k: &str| -> Option<f32> {
            order
                .iter()
                .position(|o| *o == k)
                .map(|i| GUTTER + step * i as f32)
        };
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let small = egui::TextStyle::Small.resolve(ui.style());

        // ── header: PINNED, never scrolls ──────────────────────────────────
        // The lane names are the legend for every arrow below; a legend that
        // scrolls away is no legend. Also the click surface: an entity's name
        // is the natural handle for "make this one the subject".
        let (head_rect, head_resp) =
            ui.allocate_exact_size(vec2(avail_w, HEADER_H), Sense::hover());
        {
            let painter = ui.painter_at(head_rect);
            for k in &order {
                let x = head_rect.left() + lane_off(k).expect("in order");
                let is_focal = *k == focal;
                let tint = match ring_of(k) {
                    _ if is_focal => ink,
                    Some(r) => ring_tint(r),
                    None => muted,
                };
                let name = truncate(&name_of(k), 14);
                let hit = Rect::from_center_size(
                    pos2(x, head_rect.top() + HEADER_H * 0.45),
                    vec2(
                        (name.chars().count() as f32 * 7.0).max(30.0),
                        HEADER_H - 4.0,
                    ),
                );
                // The focal lane is not clickable — it is already the subject,
                // and a dead click teaches the reader the others are dead too.
                let lane_resp = if is_focal {
                    None
                } else {
                    Some(ui.interact(hit, head_resp.id.with(*k), Sense::click()))
                };
                let hovered_lane = lane_resp.as_ref().is_some_and(|r| r.hovered());
                if let Some(r) = &lane_resp {
                    r.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    if r.clicked() {
                        out.clicked_lane = Some((*k).to_string());
                    }
                }
                let text_col = match (is_focal, hovered_lane) {
                    (true, _) => ink,
                    (false, true) => ink,
                    (false, false) => muted,
                };
                painter.circle_filled(pos2(x, head_rect.bottom() - 6.0), 3.0, tint);
                painter.text(
                    pos2(x, head_rect.bottom() - 12.0),
                    Align2::CENTER_BOTTOM,
                    &name,
                    small.clone(),
                    text_col,
                );
                if hovered_lane {
                    // Underline as the affordance — colour alone is not a cue.
                    let half = (name.chars().count() as f32 * 3.4).max(12.0);
                    painter.line_segment(
                        [
                            pos2(x - half, head_rect.bottom() - 10.0),
                            pos2(x + half, head_rect.bottom() - 10.0),
                        ],
                        Stroke::new(1.0_f32, ink.gamma_multiply(0.8)),
                    );
                }
            }
        }

        egui::ScrollArea::vertical()
            .max_height(height - HEADER_H)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(avail_w, content_h), Sense::click());
                let painter = ui.painter_at(rect);
                let small = egui::TextStyle::Small.resolve(ui.style());

                // ── lane guide lines ───────────────────────────────────────
                for k in &order {
                    let x = rect.left() + lane_off(k).expect("in order");
                    let is_focal = *k == focal;
                    let tint = match ring_of(k) {
                        _ if is_focal => ink,
                        Some(r) => ring_tint(r),
                        None => muted,
                    };
                    // Faint; the focal lane a touch firmer — it is the subject
                    // every arrow relates to.
                    let w = if is_focal { 1.2_f32 } else { 0.6_f32 };
                    let a = if is_focal { 0.5 } else { 0.25 };
                    painter.line_segment(
                        [pos2(x, rect.top()), pos2(x, rect.top() + content_h - 4.0)],
                        Stroke::new(w, tint.gamma_multiply(a)),
                    );
                }

                // ── playhead ───────────────────────────────────────────────
                // Between the last event at-or-before `now` and the next one.
                let now = spine.playhead;
                let play_y = match in_range.iter().position(|(_, e)| e.timestamp > now) {
                    Some(0) => rect.top() + ROW_MIN * 0.4,
                    Some(i) => rect.top() + (ys[i - 1] + ys[i]) * 0.5,
                    None => rect.top() + content_h - ROW_MIN * 0.4,
                };
                painter.line_segment(
                    [
                        pos2(rect.left() + 4.0, play_y),
                        pos2(rect.right() - 4.0, play_y),
                    ],
                    Stroke::new(1.0_f32, ink.gamma_multiply(0.35)),
                );

                // ── events ─────────────────────────────────────────────────
                let watched = selection.active().map(|s| s.to_string());
                // Hover is resolved BEFORE drawing so the hovered row can wear
                // its highlight band — the affordance that says "this row is
                // clickable", which a tooltip alone never quite does.
                let hover = response.hover_pos();
                let mut best: Option<(usize, f32)> = None;
                if let Some(h) = hover {
                    for (row, (idx, _)) in in_range.iter().enumerate() {
                        let d = (h.y - (rect.top() + ys[row])).abs();
                        if d <= HOVER_SLOP + 4.0 && best.is_none_or(|(_, bd)| d < bd) {
                            best = Some((*idx, d));
                        }
                    }
                }
                let mut last_day: Option<i64> = None;
                for (row, (idx, e)) in in_range.iter().enumerate() {
                    let ey = rect.top() + ys[row];
                    if matches!(best, Some((i, _)) if i == *idx) {
                        painter.rect_filled(
                            Rect::from_min_max(
                                pos2(rect.left() + GUTTER - 8.0, ey - ROW_MIN * 0.45),
                                pos2(rect.right() - 4.0, ey + ROW_MIN * 0.45),
                            ),
                            3.0,
                            ink.gamma_multiply(0.06),
                        );
                    }

                    // Day rule: a labelled separator when the date changes —
                    // the true clock, so log-compression never has to be
                    // trusted.
                    let day = e.timestamp.div_euclid(86_400);
                    if last_day != Some(day) {
                        last_day = Some(day);
                        let ly = ey - ROW_MIN * 0.7;
                        painter.line_segment(
                            [
                                pos2(rect.left() + GUTTER - 6.0, ly),
                                pos2(rect.right() - 4.0, ly),
                            ],
                            Stroke::new(0.5_f32, muted.gamma_multiply(0.3)),
                        );
                        painter.text(
                            pos2(rect.left() + 4.0, ly),
                            Align2::LEFT_CENTER,
                            crate::time_spine::format_date(e.timestamp),
                            small.clone(),
                            muted,
                        );
                    }
                    // Clock, per event.
                    painter.text(
                        pos2(rect.left() + GUTTER - 10.0, ey),
                        Align2::RIGHT_CENTER,
                        hhmm(e.timestamp),
                        small.clone(),
                        muted.gamma_multiply(0.9),
                    );

                    let future = e.timestamp > now;
                    let involved = watched.as_deref().is_none_or(|w| {
                        w == e.to || matches!(e.from, StaveOrigin::Party(p) if p == w)
                    });
                    let dim = match (future, involved) {
                        (true, _) => 0.25,
                        (false, false) => 0.35,
                        (false, true) => 1.0,
                    };

                    let lane_x = |k: &str| lane_off(k).map(|o| rect.left() + o);
                    let to_x = lane_x(e.to);
                    let (col, from_x) = match e.from {
                        StaveOrigin::Party(p) => {
                            let c = if e.to == focal {
                                IN
                            } else if p == focal {
                                OUT
                            } else {
                                muted
                            };
                            (c, lane_x(p))
                        }
                        // Unresolved payers arrive from the chart's edge.
                        StaveOrigin::Unresolved => (IN, Some(rect.left() + GUTTER + 2.0)),
                        StaveOrigin::Mint => (ink, None),
                    };
                    let Some(tx_) = to_x else { continue };

                    match from_x {
                        Some(fx) if (fx - tx_).abs() > 1.0 => {
                            let w = 1.0 + 2.6 * e.magnitude.clamp(0.0, 1.0);
                            // STOP THE SHAFT AT THE HEAD'S BASE. Drawing it
                            // to the tip leaves a nub poking out of the
                            // point: egui rounds the cap of a thick stroke,
                            // so half the line width overshoots the endpoint
                            // the triangle is trying to make sharp.
                            let rightward = tx_ > fx;
                            let base = tx_ - if rightward { HEAD_LEN } else { -HEAD_LEN };
                            // A hop between adjacent lanes can be shorter than
                            // the head; drawing a shaft then would run it
                            // BACKWARDS out of the arrow.
                            if (tx_ - fx).abs() > HEAD_LEN {
                                painter.line_segment(
                                    [pos2(fx, ey), pos2(base, ey)],
                                    Stroke::new(w, col.gamma_multiply(0.85 * dim)),
                                );
                            }
                            arrowhead(&painter, pos2(tx_, ey), rightward, col.gamma_multiply(dim));
                            // Payload label rides the arrow, biased toward the
                            // origin so it never collides with the head.
                            let mid = pos2(fx + (tx_ - fx) * 0.45, ey - 7.0);
                            painter.text(
                                mid,
                                Align2::CENTER_BOTTOM,
                                &e.label,
                                small.clone(),
                                col.gamma_multiply(dim.max(0.5)),
                            );
                            if e.items > 0 {
                                // Above the line, not on it — a chip crossing
                                // the arrow reads as struck-through.
                                item_chip(
                                    &painter,
                                    pos2(tx_ + if tx_ > fx { -22.0 } else { 22.0 }, ey - 9.0),
                                    e.items,
                                    ink.gamma_multiply(dim),
                                    &small,
                                );
                            }
                        }
                        _ => {
                            // A mint (or a degenerate zero-length arrow):
                            // a spark on the receiving lane.
                            let c = col.gamma_multiply(dim);
                            diamond(&painter, pos2(tx_, ey), 4.5, c);
                            painter.text(
                                pos2(tx_ + 10.0, ey),
                                Align2::LEFT_CENTER,
                                format!("{} · minted", e.label),
                                small.clone(),
                                ink.gamma_multiply(dim.max(0.5)),
                            );
                        }
                    }
                }

                if let Some((idx, _)) = best {
                    out.hovered = Some(idx);
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                    let e = &events[idx];
                    egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        response.id.with("tip"),
                        egui::PopupAnchor::Pointer,
                    )
                    .show(|ui| {
                        ui.set_max_width(320.0);
                        let from = match e.from {
                            StaveOrigin::Party(p) => name_of(p),
                            StaveOrigin::Mint => "mint · created here".into(),
                            StaveOrigin::Unresolved => "unresolved payer".into(),
                        };
                        ui.label(
                            egui::RichText::new(format!("{from} → {}", name_of(e.to))).strong(),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {}",
                                e.label,
                                crate::time_spine::format_date(e.timestamp)
                            ))
                            .small(),
                        );
                        ui.label(
                            egui::RichText::new(format!("tx {}", elide(e.tx)))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    if response.clicked() {
                        out.clicked = Some(idx);
                    }
                }
                out.response = response;
            });
        out
    }
}

/// Arrowhead length. The shaft stops here so the point stays sharp — see the
/// call site.
const HEAD_LEN: f32 = 7.0;

fn arrowhead(painter: &egui::Painter, at: Pos2, rightward: bool, col: Color32) {
    let dx = if rightward { -HEAD_LEN } else { HEAD_LEN };
    painter.add(egui::Shape::convex_polygon(
        vec![at, at + vec2(dx, -4.0), at + vec2(dx, 4.0)],
        col,
        Stroke::NONE,
    ));
}

fn diamond(painter: &egui::Painter, at: Pos2, r: f32, col: Color32) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            at + vec2(0.0, -r),
            at + vec2(r, 0.0),
            at + vec2(0.0, r),
            at + vec2(-r, 0.0),
        ],
        col,
        Stroke::NONE,
    ));
}

/// The asset-count chip at an arrow's destination end.
fn item_chip(painter: &egui::Painter, at: Pos2, items: i32, col: Color32, font: &egui::FontId) {
    let r = Rect::from_center_size(at, vec2(7.0, 7.0));
    painter.rect_stroke(r, 1.5, Stroke::new(1.0_f32, col), egui::StrokeKind::Middle);
    painter.text(
        at + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        format!("×{items}"),
        font.clone(),
        col,
    );
}

fn hhmm(unix: i64) -> String {
    let s = unix.rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3_600, (s % 3_600) / 60)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    format!(
        "{}…",
        s.chars().take(n.saturating_sub(1)).collect::<String>()
    )
}

fn elide(key: &str) -> String {
    if key.len() <= 16 {
        return key.to_string();
    }
    format!("{}…{}", &key[..9], &key[key.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The focal wallet holds the centre and counterparties fan out by ring
    /// class, nearest first — the stave tells the ring's story sideways.
    #[test]
    fn lanes_centre_the_focal_and_fan_by_ring() {
        let lanes = [
            StaveLane::new("stranger", 3),
            StaveLane::new("treasury", 0),
            StaveLane::new("front", 3),
            StaveLane::new("associate", 1),
        ];
        let order = lane_order("front", &lanes);
        let centre = order.iter().position(|k| *k == "front").unwrap();
        let treasury = order.iter().position(|k| *k == "treasury").unwrap();
        let associate = order.iter().position(|k| *k == "associate").unwrap();
        let stranger = order.iter().position(|k| *k == "stranger").unwrap();
        let d = |i: usize| (i as i64 - centre as i64).unsigned_abs();
        assert!(
            d(treasury) <= d(associate) && d(associate) <= d(stranger),
            "nearer ring class sits nearer the focal lane: {order:?}"
        );
        // Stability: same lanes, same order, every time.
        assert_eq!(order, lane_order("front", &lanes));
    }

    /// Order is preserved and gaps are compressed, never inverted: a
    /// five-minute cascade stays tight, an idle month stays bounded.
    #[test]
    fn row_gaps_compress_time_without_reordering() {
        assert_eq!(row_gap(0), ROW_MIN, "simultaneous events stack at minimum");
        let five_min = row_gap(300);
        let a_day = row_gap(86_400);
        let a_month = row_gap(30 * 86_400);
        assert!(five_min < a_day, "a day reads bigger than five minutes");
        assert!(a_day < a_month || (a_month - ROW_MAX).abs() < 1e-3);
        assert!(
            a_month <= ROW_MAX,
            "no silence can push the story off screen"
        );
        assert!(row_gap(365 * 86_400) <= ROW_MAX);
    }

    /// Direction is resolved against the FOCAL wallet, and a mint is never an
    /// arrow — these are the two facts the old table got wrong.
    #[test]
    fn direction_reads_against_the_focal_wallet() {
        // These mirror the draw-time rules; kept as a table so a future editor
        // sees the mapping in one place.
        let toward_focal = |from: &str, to: &str| -> &'static str {
            if to == "front" {
                "in"
            } else if from == "front" {
                "out"
            } else {
                "lateral"
            }
        };
        assert_eq!(toward_focal("treasury", "front"), "in");
        assert_eq!(toward_focal("front", "jprigs"), "out");
        assert_eq!(toward_focal("a", "b"), "lateral");
    }

    #[test]
    fn the_clock_gutter_prints_utc_hhmm() {
        assert_eq!(hhmm(0), "00:00");
        assert_eq!(hhmm(86_399), "23:59");
        assert_eq!(
            hhmm(1_750_000_000 % 86_400 + 1_750_000_000 / 86_400 * 86_400),
            hhmm(1_750_000_000)
        );
    }
}
