//! `ActivityLanes` — one thin lane per party, showing WHEN it acted, under the
//! shared spine.
//!
//! The Rerun timeline idea, borrowed for wallets: below the time ruler, every
//! entity gets a lane, and a tick appears wherever that entity had data. You
//! do not read the ticks; you read the *shape* of the lanes — which wallets
//! were active when, which went quiet, which fired exactly once. That is the
//! "where should I even look" question answered by layout, and it is a
//! different question from the one the ring answers (who dealt with whom).
//!
//! Twenty-odd lanes is scannable in a way twenty-odd nodes is not, because a
//! lane has an order and an extent and a node has neither.
//!
//! ## What is fixed
//!
//! - **x is the spine's own `TimeScale`**, passed in from the spine's response,
//!   so a tick sits exactly under the ruler date it happened on and the
//!   playhead line runs straight through both.
//! - **Row order is the caller's** and does not change with the data in view.
//!   A wallet keeps its lane for the session — same object-constancy rule as
//!   the ring's seats and the field's piles.
//! - **In above the midline, out below**, in the same two hues the spine's own
//!   marks use, so the two lanes of information read as one system.

use egui::{Align2, Color32, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::selection::Selection;
use crate::time_spine::{MarkKind, SpineState, TimeScale};

/// One party's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane<'a> {
    pub key: &'a str,
    /// Display name — the caller resolves aliases; the widget never guesses.
    pub name: String,
    /// `(timestamp, direction)`, any order.
    pub events: &'a [(i64, MarkKind)],
    /// Drawn faint and skipped by the density hero when off.
    pub active: bool,
}

pub struct ActivityLanesResponse {
    pub response: Response,
    pub hovered: Option<String>,
    /// A lane's on/off toggle was clicked.
    pub toggled: Option<String>,
    pub lanes_shown: usize,
    /// Events inside the brush window, across shown lanes.
    pub events_in_window: usize,
}

pub struct ActivityLanes<'a> {
    lanes: &'a [Lane<'a>],
    scale: &'a TimeScale,
    spine: &'a SpineState,
    selection: &'a mut Selection,
    /// Left gutter — matches the spine's play-button reservation so the lanes
    /// line up under its ruler rather than under its button.
    gutter: f32,
    label_w: f32,
    lane_h: f32,
    max_lanes: usize,
}

const IN: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
const OUT: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x2e);

impl<'a> ActivityLanes<'a> {
    pub fn new(
        lanes: &'a [Lane<'a>],
        scale: &'a TimeScale,
        spine: &'a SpineState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            lanes,
            scale,
            spine,
            selection,
            gutter: 30.0,
            label_w: 150.0,
            lane_h: 14.0,
            max_lanes: 40,
        }
    }

    /// Match the spine: `30.0` when it shows a play button, `0.0` otherwise.
    pub fn gutter(mut self, px: f32) -> Self {
        self.gutter = px;
        self
    }

    /// Width of the label column. **Pass the same value to
    /// `TimeSpine::left_inset`** or the ruler starts left of the labels and
    /// they are clipped away by the painter with no warning.
    pub fn label_width(mut self, px: f32) -> Self {
        self.label_w = px;
        self
    }

    pub fn lane_height(mut self, px: f32) -> Self {
        self.lane_h = px.clamp(8.0, 40.0);
        self
    }

    pub fn max_lanes(mut self, n: usize) -> Self {
        self.max_lanes = n.max(1);
        self
    }

    pub fn show(self, ui: &mut Ui) -> ActivityLanesResponse {
        let Self {
            lanes,
            scale,
            spine,
            selection,
            gutter,
            label_w,
            lane_h,
            max_lanes,
        } = self;
        let shown = lanes.len().min(max_lanes);
        let hidden = lanes.len() - shown;
        let extra = if hidden > 0 { 14.0 } else { 0.0 };
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width(), shown as f32 * lane_h + extra),
            Sense::click(),
        );
        let painter = ui.painter_at(rect);
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let small = egui::TextStyle::Small.resolve(ui.style());

        // Where the ruler is: the SPINE'S scale, not one of our own, so a tick
        // sits exactly under the ruler date it happened on.
        //
        // The label column lives in the space the spine reserved via
        // `left_inset`. If the caller forgot to reserve it, labels would be
        // right-aligned to a ruler that starts ~30px in and get clipped away
        // silently by `painter_at` — so fall back to indenting them INTO the
        // plot, which is ugly but visible. A missing label is worse than a
        // cramped one.
        let x_left = scale.x_range().min;
        let x_right = scale.x_range().max;
        let label_right = label_right_for(rect.left(), x_left, rect.width(), label_w);
        let (lo, hi) = spine.filter_range();
        let brushed = spine.brush.is_some();

        let mut hovered: Option<String> = None;
        let mut toggled: Option<String> = None;
        let mut events_in_window = 0usize;
        let ptr = response.hover_pos();

        for (i, lane) in lanes.iter().take(shown).enumerate() {
            let top = rect.top() + i as f32 * lane_h;
            let row = Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), top + lane_h));
            let mid = top + lane_h * 0.5;
            let emph = selection.emphasis(lane.key);
            let is_hover = ptr.is_some_and(|p| row.contains(p));
            if is_hover {
                hovered = Some(lane.key.to_string());
                painter.rect_filled(row, 0.0, ui.visuals().faint_bg_color);
            }

            // Toggle dot in the gutter, label right-aligned to the ruler edge.
            let dot = pos2(rect.left() + gutter * 0.5, mid);
            let on_col = if lane.active {
                ink.gamma_multiply(emph)
            } else {
                muted.gamma_multiply(0.35)
            };
            painter.circle_filled(dot, 3.0, on_col);
            if response.clicked() && ptr.is_some_and(|p| (p - dot).length() <= 8.0) {
                toggled = Some(lane.key.to_string());
            }
            painter.text(
                pos2(label_right, mid),
                Align2::RIGHT_CENTER,
                truncate(&lane.name, 20),
                small.clone(),
                if lane.active {
                    ink.gamma_multiply(emph)
                } else {
                    muted.gamma_multiply(0.5)
                },
            );

            // Baseline: faint, so the ticks read as marks ON something.
            painter.line_segment(
                [pos2(x_left, mid), pos2(x_right, mid)],
                Stroke::new(1.0_f32, ui.visuals().faint_bg_color.gamma_multiply(1.6)),
            );

            if !lane.active {
                continue;
            }
            for &(t, kind) in lane.events {
                if t > spine.playhead {
                    continue;
                }
                let in_window = t >= lo && t <= hi;
                if in_window {
                    events_in_window += 1;
                }
                let Some(x) = scale.x_from_time_f32(t as f64) else {
                    continue;
                };
                if x < x_left || x > x_right {
                    continue;
                }
                // Outside the brush the tick stays but recedes: the brush
                // FILTERS, it does not erase, and a still needs the context.
                let a = if brushed && !in_window { 0.18 } else { 0.85 } * emph.max(0.25);
                let (y0, y1, col) = match kind {
                    MarkKind::In => (mid, top + 2.0, IN),
                    MarkKind::Out => (mid, top + lane_h - 2.0, OUT),
                };
                painter.line_segment(
                    [pos2(x, y0), pos2(x, y1)],
                    Stroke::new(1.0_f32, col.gamma_multiply(a)),
                );
            }
        }

        // The playhead, through every lane, so the ruler and the lanes read as
        // one instrument.
        if let Some(px) = scale.x_from_time_f32(spine.playhead as f64)
            && (x_left..=x_right).contains(&px)
        {
            painter.line_segment(
                [
                    pos2(px, rect.top()),
                    pos2(px, rect.top() + shown as f32 * lane_h),
                ],
                Stroke::new(1.0_f32, ink.gamma_multiply(0.55)),
            );
        }

        if hidden > 0 {
            painter.text(
                pos2(x_left, rect.bottom() - 2.0),
                Align2::LEFT_BOTTOM,
                format!("{hidden} more lanes not shown"),
                small,
                muted,
            );
        }

        // Selection: hover emphasises everywhere; click pins.
        match &hovered {
            Some(h) => selection.hover(h.clone()),
            None => {
                if let Some(prev) = ui.data(|d| d.get_temp::<String>(response.id.with("hov"))) {
                    selection.clear_hover_if(&prev);
                }
            }
        }
        ui.data_mut(|d| match &hovered {
            Some(h) => {
                d.insert_temp(response.id.with("hov"), h.clone());
            }
            None => d.remove::<String>(response.id.with("hov")),
        });
        if response.clicked() && toggled.is_none() {
            match &hovered {
                Some(h) => selection.toggle_pin(h.clone()),
                None => selection.clear_pin(),
            }
        }

        ActivityLanesResponse {
            response,
            hovered,
            toggled,
            lanes_shown: shown,
            events_in_window,
        }
    }
}

/// Right edge of the label column, kept inside the clip rect.
///
/// When the spine reserved a gutter (`TimeSpine::left_inset`), labels sit just
/// left of the ruler. When it did not, right-aligning there would place them
/// off the widget's left edge, where `painter_at` discards them silently — so
/// they indent into the plot instead. Cramped beats invisible.
fn label_right_for(rect_left: f32, x_left: f32, rect_w: f32, label_w: f32) -> f32 {
    if x_left - rect_left >= 40.0 {
        x_left - 8.0
    } else {
        rect_left + label_w.min(rect_w * 0.3)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_spine::TimeView;
    use egui::{Id, Pos2, Rangef, vec2};

    fn run(
        lanes: &[Lane<'_>],
        spine: &SpineState,
        sel: &mut Selection,
        click: Option<Pos2>,
    ) -> ActivityLanesResponse {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let scale = TimeScale::continuous(
            Rangef::new(200.0, 800.0),
            TimeView::covering(spine.domain.0, spine.domain.1),
            spine.domain.0,
            spine.domain.1,
        );
        let mut out = None;
        let mut events = Vec::new();
        if let Some(p) = click {
            events.push(egui::Event::PointerMoved(p));
        }
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 400.0))),
            events,
            ..Default::default()
        };
        ctx.begin_pass(raw);
        egui::Area::new(Id::new("al")).show(&ctx, |ui| {
            ui.set_min_size(vec2(900.0, 400.0));
            out = Some(ActivityLanes::new(lanes, &scale, spine, sel).show(ui));
        });
        let _ = ctx.end_pass();
        out.unwrap()
    }

    /// The playhead reveals; the brush filters but does not erase.
    #[test]
    fn events_follow_the_playhead_and_the_brush() {
        let ev_a = [
            (100i64, MarkKind::In),
            (500, MarkKind::Out),
            (900, MarkKind::In),
        ];
        let lanes = [Lane {
            key: "a",
            name: "wallet a".into(),
            events: &ev_a,
            active: true,
        }];
        let mut sel = Selection::default();
        let mut spine = SpineState::new((0, 1000));

        spine.set_playhead(600);
        let r = run(&lanes, &spine, &mut sel, None);
        assert_eq!(r.events_in_window, 2, "the 900 event has not happened yet");

        spine.set_playhead(1000);
        spine.set_brush(Some((400, 600)));
        let r = run(&lanes, &spine, &mut sel, None);
        assert_eq!(r.events_in_window, 1, "brushed to the middle event only");
    }

    /// An inactive lane keeps its row (order is stable) but contributes nothing.
    #[test]
    fn an_inactive_lane_keeps_its_row_but_counts_nothing() {
        let ev = [(100i64, MarkKind::In)];
        let lanes = [
            Lane {
                key: "a",
                name: "a".into(),
                events: &ev,
                active: false,
            },
            Lane {
                key: "b",
                name: "b".into(),
                events: &ev,
                active: true,
            },
        ];
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));
        let r = run(&lanes, &spine, &mut sel, None);
        assert_eq!(r.lanes_shown, 2, "the row is still there");
        assert_eq!(r.events_in_window, 1, "only the active lane counts");
    }

    /// Labels must land INSIDE the widget's rect. Drawing outside a
    /// `painter_at` clip vanishes with no warning and no panic, which is how
    /// a whole label column can go missing and still pass every other test.
    #[test]
    fn labels_stay_inside_the_clip_rect() {
        // A spine that reserved a proper gutter: labels sit left of the ruler.
        let with_gutter = super::label_right_for(0.0, 160.0, 900.0, 150.0);
        assert!(with_gutter > 0.0 && with_gutter < 160.0);

        // A spine that reserved NOTHING (ruler starts at the play button):
        // labels must fall back INTO the plot rather than off the left edge.
        let no_gutter = super::label_right_for(0.0, 30.0, 900.0, 150.0);
        assert!(
            no_gutter > 0.0,
            "a label right-aligned at 30-8=22 would be clipped away; got {no_gutter}"
        );
        assert!(no_gutter >= 30.0, "must be indented into the plot");
    }

    /// Caps are stated.
    #[test]
    fn overflow_is_reported_not_swallowed() {
        let ev = [(100i64, MarkKind::In)];
        let names: Vec<String> = (0..50).map(|i| format!("w{i}")).collect();
        let lanes: Vec<Lane<'_>> = names
            .iter()
            .map(|n| Lane {
                key: n,
                name: n.clone(),
                events: &ev,
                active: true,
            })
            .collect();
        let mut sel = Selection::default();
        let spine = SpineState::new((0, 1000));
        let r = run(&lanes, &spine, &mut sel, None);
        assert_eq!(r.lanes_shown, 40);
    }
}
