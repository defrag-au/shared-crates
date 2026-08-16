//! `CapitalFlow` — a project raised some money; watch where it went.
//!
//! The view an investigation of a project actually opens on. Not "here is a
//! wallet and its counterparties" — that asks the reader to already know what
//! they are looking for — but **"they raised X, and here is it leaving, over
//! time, by destination."**
//!
//! ## Why time, and why a playhead
//!
//! Some findings only exist temporally. A funding channel that paid every month
//! for eight months and then paid nothing is invisible in a total and obvious as
//! a band that stops. A category that receives money it was never budgeted for
//! shows up the moment it appears. A playhead turns those into things that
//! *happen* on screen rather than claims made in a table, and it stops on any
//! frame, so the moment a channel dies is a screenshot rather than a sentence.
//!
//! ## The raise line is a reference, not a ceiling
//!
//! Cumulative deployment routinely exceeds the raise, because royalties and
//! other income flow through the same wallets — in the case this came from, a
//! treasury that banked 445,417 deployed 577,950. Clamping to the raise would
//! hide exactly that. So [`CapitalFlow::raised`] draws a labelled reference line
//! and the stack is free to cross it; the crossing is a finding, not an error.
//!
//! ## Labelling pays off here
//!
//! Destinations are whatever the caller groups them into. Un-annotated, that is
//! a hundred anonymous addresses and the chart is noise; annotated into five
//! clusters it is an argument. That is the whole loop the annotation layer
//! exists to serve, and this is where a reader sees the return on it.

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};

use crate::channel_bands::{CHANNEL_PALETTE, OTHER_COLOR};

/// One movement of capital out to a destination.
#[derive(Clone, Debug)]
pub struct FlowEvent<'a> {
    pub timestamp: i64,
    /// Destination group — a cluster name, not a raw address. The grouping is
    /// the caller's (and therefore the annotator's) decision.
    pub destination: &'a str,
    /// Always positive: this is value leaving.
    pub amount: i128,
}

impl<'a> FlowEvent<'a> {
    pub fn new(timestamp: i64, destination: &'a str, amount: i128) -> Self {
        Self {
            timestamp,
            destination,
            amount: amount.abs(),
        }
    }
}

/// A destination band: its total, and where it sits in the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band {
    pub name: String,
    pub total: i128,
    pub color: Color32,
}

/// Bands in a stable order, largest first, with palette colours assigned by
/// rank over the **whole** series.
///
/// Computed once over everything rather than up to the playhead: if bands
/// reordered or changed colour as time advanced, the eye would lose the thing it
/// was following, and the animation would be actively misleading.
pub fn bands(events: &[FlowEvent<'_>]) -> Vec<Band> {
    let mut totals: Vec<(String, i128)> = Vec::new();
    for e in events {
        match totals.iter_mut().find(|(n, _)| n == e.destination) {
            Some((_, t)) => *t += e.amount,
            None => totals.push((e.destination.to_string(), e.amount)),
        }
    }
    // Ties broken by name so the order is fully determined by the data.
    totals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    totals
        .into_iter()
        .enumerate()
        .map(|(i, (name, total))| Band {
            name,
            total,
            color: CHANNEL_PALETTE.get(i).copied().unwrap_or(OTHER_COLOR),
        })
        .collect()
}

/// Cumulative total per band at `at`, in `bands` order.
pub fn cumulative_at(events: &[FlowEvent<'_>], bands: &[Band], at: i64) -> Vec<i128> {
    let mut out = vec![0i128; bands.len()];
    for e in events.iter().filter(|e| e.timestamp <= at) {
        if let Some(i) = bands.iter().position(|b| b.name == e.destination) {
            out[i] += e.amount;
        }
    }
    out
}

/// What the reader is being told at the playhead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapitalState {
    pub deployed: i128,
    /// Raise not yet deployed. Zero once deployment passes the raise.
    pub undeployed: i128,
    /// Deployment beyond the raise — income from elsewhere flowing through the
    /// same wallets. Zero until the stack crosses the line.
    pub beyond_raise: i128,
}

pub fn state_at(events: &[FlowEvent<'_>], raised: i128, at: i64) -> CapitalState {
    let deployed: i128 = events
        .iter()
        .filter(|e| e.timestamp <= at)
        .map(|e| e.amount)
        .sum();
    CapitalState {
        deployed,
        undeployed: (raised - deployed).max(0),
        beyond_raise: (deployed - raised).max(0),
    }
}

pub struct CapitalFlowResponse {
    /// Set when the reader dragged or clicked the timeline.
    pub scrubbed_to: Option<i64>,
    pub state: CapitalState,
}

pub struct CapitalFlow<'a> {
    events: &'a [FlowEvent<'a>],
    bands: &'a [Band],
    raised: i128,
    playhead: i64,
    format_value: &'a dyn Fn(i128) -> String,
    format_time: &'a dyn Fn(i64) -> String,
    height: f32,
}

impl<'a> CapitalFlow<'a> {
    /// `events` must be sorted by timestamp; `bands` comes from [`bands`].
    pub fn new(
        events: &'a [FlowEvent<'a>],
        bands: &'a [Band],
        raised: i128,
        playhead: i64,
        format_value: &'a dyn Fn(i128) -> String,
        format_time: &'a dyn Fn(i64) -> String,
    ) -> Self {
        Self {
            events,
            bands,
            raised,
            playhead,
            format_value,
            format_time,
            height: 260.0,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    fn span(&self) -> (i64, i64) {
        let first = self.events.first().map(|e| e.timestamp).unwrap_or(0);
        let last = self.events.last().map(|e| e.timestamp).unwrap_or(first + 1);
        (first, last.max(first + 1))
    }

    /// Y scale: the raise, or peak deployment if that is higher. Scaling to the
    /// raise alone would clip the very case worth seeing.
    fn scale_max(&self) -> i128 {
        let peak: i128 = self.events.iter().map(|e| e.amount).sum();
        self.raised.max(peak).max(1)
    }

    pub fn show(self, ui: &mut Ui) -> CapitalFlowResponse {
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let (t0, t1) = self.span();
        let scale = self.scale_max() as f64;

        let width = ui.available_width();
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(width, self.height), Sense::click_and_drag());

        // Reserve a strip for the hero readout above and the axis below.
        let hero_h = 34.0;
        let axis_h = 16.0;
        let plot = Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + hero_h),
            Pos2::new(rect.right(), rect.bottom() - axis_h),
        );
        let painter = ui.painter_at(rect);

        let x_of =
            |t: i64| plot.left() + ((t - t0) as f64 / (t1 - t0) as f64) as f32 * plot.width();
        let y_of = |v: i128| plot.bottom() - (v as f64 / scale) as f32 * plot.height();

        let scrubbed_to = (resp.dragged() || resp.clicked())
            .then(|| resp.interact_pointer_pos())
            .flatten()
            .map(|p| {
                let f = ((p.x - plot.left()) / plot.width()).clamp(0.0, 1.0) as f64;
                t0 + (f * (t1 - t0) as f64) as i64
            });

        let state = state_at(self.events, self.raised, self.playhead);

        // ── the stack, as a staircase ────────────────────────────────────
        //
        // Capital moves at discrete instants, so the cumulative area is a
        // staircase and not a smooth curve. Interpolating between events would
        // draw money moving on days nothing happened.
        let mut steps: Vec<i64> = self.events.iter().map(|e| e.timestamp).collect();
        steps.dedup();
        let mut running = vec![0i128; self.bands.len()];

        let mut prev_x = x_of(t0);
        let mut prev_tops: Vec<f32> = vec![plot.bottom(); self.bands.len()];
        for t in steps.iter().copied().chain(std::iter::once(t1)) {
            let x = x_of(t);
            // Draw the segment leading up to this instant at the running totals.
            let future = t > self.playhead;
            let mut base = plot.bottom();
            for (i, band) in self.bands.iter().enumerate() {
                let h = (running[i] as f64 / scale) as f32 * plot.height();
                let top = base - h;
                if h > 0.0 && x > prev_x {
                    let seg = Rect::from_min_max(Pos2::new(prev_x, top), Pos2::new(x, base));
                    // The future is ghosted rather than hidden: you can see
                    // where it is going without mistaking it for what has
                    // already happened.
                    painter.rect_filled(
                        seg,
                        0.0,
                        band.color.gamma_multiply(if future { 0.16 } else { 1.0 }),
                    );
                }
                prev_tops[i] = top;
                base = top;
            }
            // Then apply this instant's movements.
            for e in self.events.iter().filter(|e| e.timestamp == t) {
                if let Some(i) = self.bands.iter().position(|b| b.name == e.destination) {
                    running[i] += e.amount;
                }
            }
            prev_x = x;
        }

        // ── the raise line ───────────────────────────────────────────────
        let raise_y = y_of(self.raised);
        painter.line_segment(
            [
                Pos2::new(plot.left(), raise_y),
                Pos2::new(plot.right(), raise_y),
            ],
            Stroke::new(1.0_f32, muted.gamma_multiply(0.8)),
        );
        painter.text(
            Pos2::new(plot.right(), raise_y - 2.0),
            Align2::RIGHT_BOTTOM,
            format!("raised {}", (self.format_value)(self.raised)),
            FontId::monospace(9.0),
            muted,
        );

        // ── playhead ─────────────────────────────────────────────────────
        let px = x_of(self.playhead);
        painter.line_segment(
            [Pos2::new(px, plot.top()), Pos2::new(px, plot.bottom())],
            Stroke::new(1.0_f32, ink),
        );
        // Keep the label inside the plot: centred on the playhead it is clipped
        // away at either extreme, which is exactly where a scrubber spends most
        // of its time.
        let label = (self.format_time)(self.playhead);
        let half = label.len() as f32 * 3.0;
        painter.text(
            Pos2::new(
                px.clamp(plot.left() + half, plot.right() - half),
                plot.bottom() + 2.0,
            ),
            Align2::CENTER_TOP,
            label,
            FontId::monospace(9.0),
            ink,
        );

        // ── hero readout ─────────────────────────────────────────────────
        //
        // The one line a reader should get without interacting: how much has
        // left, as a share of what came in.
        let pct = if self.raised > 0 {
            state.deployed as f64 * 100.0 / self.raised as f64
        } else {
            0.0
        };
        painter.text(
            Pos2::new(rect.left(), rect.top() + 2.0),
            Align2::LEFT_TOP,
            format!("{} deployed", (self.format_value)(state.deployed)),
            FontId::monospace(15.0),
            ink,
        );
        let tail = if state.beyond_raise > 0 {
            format!(
                "{pct:.0}% of the raise · {} BEYOND it",
                (self.format_value)(state.beyond_raise)
            )
        } else {
            format!(
                "{pct:.0}% of the raise · {} still held",
                (self.format_value)(state.undeployed)
            )
        };
        painter.text(
            Pos2::new(rect.left(), rect.top() + 20.0),
            Align2::LEFT_TOP,
            tail,
            FontId::proportional(10.0),
            if state.beyond_raise > 0 {
                ui.visuals().warn_fg_color
            } else {
                muted
            },
        );

        CapitalFlowResponse { scrubbed_to, state }
    }
}

/// Legend + per-band totals at the playhead. Separate from the plot so a host
/// can place it wherever it has room.
pub fn legend(
    ui: &mut Ui,
    bands: &[Band],
    at: &[i128],
    format_value: &dyn Fn(i128) -> String,
) -> Response {
    let muted = ui.visuals().weak_text_color();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for (i, b) in bands.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let (r, _) = ui.allocate_exact_size(Vec2::new(9.0, 9.0), Sense::hover());
                ui.painter().rect_filled(r, 1.0, b.color);
                ui.label(RichText::new(&b.name).size(10.0).color(muted));
                ui.label(
                    RichText::new(format_value(at.get(i).copied().unwrap_or(0)))
                        .size(10.0)
                        .monospace(),
                );
            });
        }
    })
    .response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t: i64, dest: &'static str, amt: i128) -> FlowEvent<'static> {
        FlowEvent::new(t, dest, amt)
    }

    /// Band order and colour are fixed over the whole series. If they shifted as
    /// the playhead moved, the eye would lose what it was following.
    #[test]
    fn bands_are_ranked_over_the_whole_series_not_the_visible_part() {
        let events = vec![
            ev(1, "small-early", 100),
            ev(2, "big-late", 10),
            ev(9, "big-late", 5_000),
        ];
        let b = bands(&events);
        assert_eq!(b[0].name, "big-late", "ranked on its full total, not t=2");
        assert_eq!(b[0].color, CHANNEL_PALETTE[0]);
        assert_eq!(b[1].name, "small-early");
        assert_eq!(b[1].color, CHANNEL_PALETTE[1]);
    }

    #[test]
    fn ties_break_by_name_so_order_is_deterministic() {
        let a = bands(&[ev(1, "zebra", 100), ev(1, "alpha", 100)]);
        let b = bands(&[ev(1, "alpha", 100), ev(1, "zebra", 100)]);
        assert_eq!(a[0].name, "alpha");
        assert_eq!(
            a.iter().map(|x| &x.name).collect::<Vec<_>>(),
            b.iter().map(|x| &x.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cumulative_counts_only_up_to_the_playhead() {
        let events = vec![ev(10, "a", 100), ev(20, "a", 50), ev(30, "b", 7)];
        let b = bands(&events);
        let ai = b.iter().position(|x| x.name == "a").unwrap();
        assert_eq!(cumulative_at(&events, &b, 5)[ai], 0);
        assert_eq!(
            cumulative_at(&events, &b, 10)[ai],
            100,
            "inclusive of the instant"
        );
        assert_eq!(cumulative_at(&events, &b, 25)[ai], 150);
        assert_eq!(cumulative_at(&events, &b, 99)[ai], 150);
    }

    /// The case worth seeing: deployment exceeding the raise, because other
    /// income flowed through the same wallets. Clamping would hide it.
    #[test]
    fn deployment_beyond_the_raise_is_reported_not_clamped() {
        let events = vec![ev(1, "off-ramp", 400), ev(2, "ops", 300)];
        let s = state_at(&events, 500, 2);
        assert_eq!(s.deployed, 700);
        assert_eq!(s.undeployed, 0);
        assert_eq!(s.beyond_raise, 200, "the excess is the finding");
    }

    #[test]
    fn undeployed_is_what_is_left_of_the_raise() {
        let events = vec![ev(1, "off-ramp", 400)];
        let s = state_at(&events, 1_000, 1);
        assert_eq!(s.deployed, 400);
        assert_eq!(s.undeployed, 600);
        assert_eq!(s.beyond_raise, 0);
    }

    #[test]
    fn nothing_deployed_before_the_first_event() {
        let events = vec![ev(100, "a", 50)];
        let s = state_at(&events, 1_000, 99);
        assert_eq!(s.deployed, 0);
        assert_eq!(s.undeployed, 1_000);
    }

    /// Outflow is always positive, whichever sign the caller had it in.
    #[test]
    fn an_event_normalises_to_a_positive_amount() {
        assert_eq!(FlowEvent::new(1, "d", -500).amount, 500);
        assert_eq!(FlowEvent::new(1, "d", 500).amount, 500);
    }

    /// Past the validated palette, bands take the neutral rather than an
    /// invented hue.
    #[test]
    fn bands_beyond_the_palette_take_the_neutral() {
        let events: Vec<FlowEvent> = ["a", "b", "c", "d", "e", "f", "g"]
            .iter()
            .enumerate()
            .map(|(i, d)| ev(i as i64, d, 100 - i as i128))
            .collect();
        let b = bands(&events);
        assert_eq!(b[4].color, CHANNEL_PALETTE[4]);
        assert_eq!(b[5].color, OTHER_COLOR);
        assert_eq!(b[6].color, OTHER_COLOR);
    }

    #[test]
    fn an_empty_series_is_well_defined() {
        assert!(bands(&[]).is_empty());
        let s = state_at(&[], 1_000, 0);
        assert_eq!(s.deployed, 0);
        assert_eq!(s.undeployed, 1_000);
    }
}
