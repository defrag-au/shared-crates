//! `HolderFormation` — people arriving, and how evenly the collection lands.
//!
//! The other face of a mint. [`CapitalFlow`](crate::CapitalFlow) shows the money
//! leaving; this shows assets arriving and a holder base forming, on the same
//! time axis and driven by the same playhead, so the two can be read together.
//!
//! ## One axis: assets distributed
//!
//! Height is cumulative assets in holders' hands. Holder *count* is carried in
//! the hero line rather than a second y-scale — two measures of different scale
//! on one chart is the dual-axis mistake, and the count is a headline anyway,
//! not something anyone reads off a gridline.
//!
//! ## Why bands are ranked here, when everywhere else that is forbidden
//!
//! The rest of this catalog insists colour follows an **entity**, never its
//! rank, so a filter cannot repaint the survivors. This widget deliberately does
//! the opposite: its bands are `top 1`, `top 10`, `everyone else`, recomputed at
//! each step from the ranking *at that moment*.
//!
//! That is because concentration **is** the subject. The question is not "what
//! did this wallet do" — that is what
//! [`PartyBadge`](crate::PartyBadge)-carrying views answer — but "how evenly did
//! this land, and did that change as it went". A band that swells means the top
//! of the distribution is pulling away, whoever happens to be in it. Pinning
//! bands to identities would answer a different question and hide this one.

use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// One acquisition: a wallet receiving assets at a moment.
#[derive(Clone, Debug)]
pub struct Acquisition<'a> {
    pub timestamp: i64,
    pub holder: &'a str,
    /// Assets gained. Negative for a disposal, so a holder base can shrink.
    pub count: i64,
}

impl<'a> Acquisition<'a> {
    pub fn new(timestamp: i64, holder: &'a str, count: i64) -> Self {
        Self {
            timestamp,
            holder,
            count,
        }
    }
}

/// The distribution at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Distribution {
    pub holders: usize,
    pub distributed: i64,
    /// Held by the single largest holder.
    pub top1: i64,
    /// Held by the ten largest, inclusive of `top1`.
    pub top10: i64,
}

impl Distribution {
    /// Share of everything distributed held by the ten largest, 0.0–1.0.
    ///
    /// The number a reader should take away: one wallet in ten holding most of
    /// a collection is a different project from one where it does not.
    pub fn top10_share(&self) -> f64 {
        if self.distributed <= 0 {
            return 0.0;
        }
        self.top10 as f64 / self.distributed as f64
    }
}

/// Holdings per wallet at `at`, largest first. Wallets at zero or below are
/// dropped — someone who sold everything is not a holder.
pub fn holdings_at(events: &[Acquisition<'_>], at: i64) -> Vec<(String, i64)> {
    // Keyed, not scanned. The linear `by.iter_mut().find(...)` this replaces was
    // O(events × holders) and allocated a String per new holder — fine for a
    // fixture, ruinous for a real collection (25k events over ~8k holders).
    let mut acc: HashMap<&str, i64> = HashMap::with_capacity(events.len().min(4096));
    for e in events.iter().filter(|e| e.timestamp <= at) {
        *acc.entry(e.holder).or_insert(0) += e.count;
    }
    let mut by: Vec<(String, i64)> = acc.into_iter().map(|(h, n)| (h.to_string(), n)).collect();
    by.retain(|(_, n)| *n > 0);
    // Ties broken by name so the ordering is determined by the data alone.
    by.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    by
}

/// The whole staircase in ONE pass over `events`.
///
/// `steps` must be ascending. Returns one [`Distribution`] per step.
///
/// Why this exists: the chart needs a Distribution at every sampled instant,
/// and calling [`distribution_at`] per step re-scans every event and re-sorts
/// every holder each time — 900 steps over 25k events cost ~850ms PER FRAME on
/// a real collection. None of those values depend on the playhead (only the
/// shading does), so the staircase is computed once, streaming.
pub fn distribution_series(events: &[Acquisition<'_>], steps: &[i64]) -> Vec<Distribution> {
    let mut acc: HashMap<&str, i64> = HashMap::with_capacity(events.len().min(8192));
    let mut out = Vec::with_capacity(steps.len());
    let mut i = 0usize;
    let mut top: Vec<i64> = Vec::new();
    for &t in steps {
        while i < events.len() && events[i].timestamp <= t {
            *acc.entry(events[i].holder).or_insert(0) += events[i].count;
            i += 1;
        }
        // Top-k by partial selection over holders — no full sort per step.
        top.clear();
        top.extend(acc.values().copied().filter(|n| *n > 0));
        let holders = top.len();
        let distributed: i64 = top.iter().sum();
        let k = top.len().min(10);
        if k > 0 {
            top.select_nth_unstable_by(k - 1, |a, b| b.cmp(a));
        }
        let top10: i64 = top[..k].iter().sum();
        let top1 = top[..k].iter().copied().max().unwrap_or(0);
        out.push(Distribution {
            holders,
            distributed,
            top1,
            top10,
        });
    }
    out
}

pub fn distribution_at(events: &[Acquisition<'_>], at: i64) -> Distribution {
    let by = holdings_at(events, at);
    Distribution {
        holders: by.len(),
        distributed: by.iter().map(|(_, n)| *n).sum(),
        top1: by.first().map(|(_, n)| *n).unwrap_or(0),
        top10: by.iter().take(10).map(|(_, n)| *n).sum(),
    }
}

/// Cohort colours. Deliberately a single-hue ramp, light→dark, not the
/// categorical palette: these bands are ordered slices of one quantity, and a
/// categorical set would imply they are unrelated things.
const TOP1: Color32 = Color32::from_rgb(0x1b, 0x5e, 0x9e);
const TOP10: Color32 = Color32::from_rgb(0x39, 0x87, 0xe5);
const REST: Color32 = Color32::from_rgb(0x8f, 0xbe, 0xf2);

pub struct HolderFormation<'a> {
    events: &'a [Acquisition<'a>],
    playhead: i64,
    /// Assets the collection will ultimately distribute, if known — the target
    /// the mint is filling toward.
    supply: Option<i64>,
    format_time: &'a dyn Fn(i64) -> String,
    height: f32,
}

impl<'a> HolderFormation<'a> {
    pub fn new(
        events: &'a [Acquisition<'a>],
        playhead: i64,
        format_time: &'a dyn Fn(i64) -> String,
    ) -> Self {
        Self {
            events,
            playhead,
            supply: None,
            format_time,
            height: 200.0,
        }
    }

    pub fn supply(mut self, supply: i64) -> Self {
        self.supply = Some(supply);
        self
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

    pub fn show(self, ui: &mut Ui) -> Distribution {
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let (t0, t1) = self.span();

        let total: i64 = self.events.iter().map(|e| e.count.max(0)).sum();
        let scale = self.supply.unwrap_or(total).max(total).max(1) as f64;

        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), self.height), Sense::hover());
        let hero_h = 32.0;
        let axis_h = 14.0;
        let plot = Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + hero_h),
            Pos2::new(rect.right(), rect.bottom() - axis_h),
        );
        let painter = ui.painter_at(rect);

        let x_of =
            |t: i64| plot.left() + ((t - t0) as f64 / (t1 - t0) as f64) as f32 * plot.width();

        // Sample at each distinct instant — acquisitions are discrete, so the
        // area is a staircase. Interpolating would draw assets moving on days
        // nothing happened.
        let mut steps: Vec<i64> = self.events.iter().map(|e| e.timestamp).collect();
        steps.dedup();
        // Sample at most one step per horizontal pixel. Each step costs a full
        // `distribution_at`, so sampling every distinct instant meant ~20,000
        // recomputes per frame on a real collection — more steps than the plot
        // has pixels to draw them in, so the extra work bought nothing at all.
        // First and last are always kept, so the span is unchanged.
        // One sample per ~3px. This is a STAIRCASE: at 3px the risers are still
        // wider than the stroke, so nothing visible is lost, and the sample
        // count drives the whole cost (see `distribution_series`).
        let max_steps = ((plot.width() / 3.0).ceil() as usize).max(2);
        if steps.len() > max_steps {
            let n = steps.len();
            let stride = (n as f32 / max_steps as f32).ceil() as usize;
            let last = steps[n - 1];
            steps = steps.into_iter().step_by(stride.max(1)).collect();
            if steps.last() != Some(&last) {
                steps.push(last);
            }
        }

        // One pass for the whole staircase — see `distribution_series`.
        let series = distribution_series(self.events, &steps);
        let mut prev_x = x_of(t0);
        let mut prev: Option<Distribution> = None;
        for (si, t) in steps.iter().copied().enumerate() {
            let d = series[si].clone();
            let x = x_of(t);
            if let Some(p) = &prev {
                let future = t > self.playhead;
                let alpha = if future { 0.18 } else { 1.0 };
                let seg = |v: i64| (v as f64 / scale) as f32 * plot.height();
                // Bottom-up: top1, then the rest of the top ten, then everyone
                // else — so the concentrated part is anchored to the baseline
                // and its growth is read against a fixed edge.
                let mut base = plot.bottom();
                for (value, color) in [
                    (p.top1, TOP1),
                    (p.top10 - p.top1, TOP10),
                    (p.distributed - p.top10, REST),
                ] {
                    if value <= 0 {
                        continue;
                    }
                    let h = seg(value);
                    let r = Rect::from_min_max(Pos2::new(prev_x, base - h), Pos2::new(x, base));
                    painter.rect_filled(r, 0.0, color.gamma_multiply(alpha));
                    base -= h;
                }
            }
            prev = Some(d);
            prev_x = x;
        }

        // Supply line — the target the mint is filling toward.
        if let Some(supply) = self.supply {
            let y = plot.bottom() - (supply as f64 / scale) as f32 * plot.height();
            painter.line_segment(
                [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
                Stroke::new(1.0_f32, muted.gamma_multiply(0.7)),
            );
            painter.text(
                Pos2::new(plot.right(), y - 2.0),
                Align2::RIGHT_BOTTOM,
                format!("supply {supply}"),
                FontId::monospace(9.0),
                muted,
            );
        }

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

        let now = distribution_at(self.events, self.playhead);
        painter.text(
            Pos2::new(rect.left(), rect.top() + 2.0),
            Align2::LEFT_TOP,
            format!("{} holders", now.holders),
            FontId::monospace(15.0),
            ink,
        );
        painter.text(
            Pos2::new(rect.left(), rect.top() + 19.0),
            Align2::LEFT_TOP,
            format!(
                "{} assets distributed · top 10 hold {:.0}%",
                now.distributed,
                now.top10_share() * 100.0
            ),
            FontId::proportional(10.0),
            muted,
        );

        now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(t: i64, h: &'static str, n: i64) -> Acquisition<'static> {
        Acquisition::new(t, h, n)
    }

    #[test]
    fn holders_accumulate_up_to_the_playhead() {
        let e = vec![a(10, "alice", 3), a(20, "bob", 1), a(30, "carol", 5)];
        assert_eq!(distribution_at(&e, 5).holders, 0);
        assert_eq!(distribution_at(&e, 10).holders, 1);
        assert_eq!(distribution_at(&e, 25).holders, 2);
        assert_eq!(distribution_at(&e, 99).holders, 3);
        assert_eq!(distribution_at(&e, 99).distributed, 9);
    }

    /// Someone who sold everything is not a holder.
    #[test]
    fn a_wallet_that_sells_out_stops_counting() {
        let e = vec![a(10, "alice", 5), a(20, "alice", -5)];
        assert_eq!(distribution_at(&e, 10).holders, 1);
        let after = distribution_at(&e, 20);
        assert_eq!(after.holders, 0, "sold out is not a holder");
        assert_eq!(after.distributed, 0);
        assert_eq!(after.top10_share(), 0.0, "no division by zero");
    }

    /// Concentration is the subject: the top band reflects whoever leads *now*,
    /// which is why these bands are ranked where the rest of the catalog
    /// forbids it.
    #[test]
    fn the_top_band_follows_the_rank_not_a_fixed_wallet() {
        let e = vec![a(10, "early", 100), a(20, "late", 500)];
        assert_eq!(distribution_at(&e, 10).top1, 100, "early leads at t=10");
        assert_eq!(distribution_at(&e, 20).top1, 500, "late leads at t=20");
    }

    #[test]
    fn top10_includes_top1_and_never_exceeds_the_total() {
        let e: Vec<Acquisition> = (0..25)
            .map(|i| Acquisition::new(i, ["w0", "w1", "w2"][i as usize % 3], 10))
            .collect();
        let d = distribution_at(&e, 99);
        assert!(d.top10 >= d.top1);
        assert_eq!(
            d.top10, d.distributed,
            "fewer than ten holders: top10 is all"
        );
        assert!(d.top10_share() <= 1.0);
    }

    #[test]
    fn concentration_is_visible_in_the_share() {
        // One whale, nine minnows.
        let mut e = vec![a(1, "whale", 900)];
        for i in 0..9 {
            e.push(Acquisition::new(
                2,
                ["a", "b", "c", "d", "e", "f", "g", "h", "i"][i],
                10,
            ));
        }
        let d = distribution_at(&e, 99);
        assert_eq!(d.top1, 900);
        assert_eq!(d.distributed, 990);
        assert!(d.top10_share() > 0.99, "ten holders hold everything here");
        assert!((d.top1 as f64 / d.distributed as f64) > 0.9);
    }

    #[test]
    fn holdings_come_back_largest_first_with_stable_ties() {
        let e = vec![a(1, "zebra", 5), a(1, "alpha", 5), a(1, "big", 9)];
        let h = holdings_at(&e, 99);
        assert_eq!(h[0].0, "big");
        assert_eq!(h[1].0, "alpha", "ties break by name");
        assert_eq!(h[2].0, "zebra");
    }

    #[test]
    fn an_empty_series_is_well_defined() {
        let d = distribution_at(&[], 0);
        assert_eq!(d.holders, 0);
        assert_eq!(d.distributed, 0);
        assert_eq!(d.top10_share(), 0.0);
    }
    /// Real-collection scale — see the `mint_arrivals` twin. `holdings_at` was
    /// O(events × holders) with a String allocation per new holder, and the
    /// chart called it once per distinct timestamp.
    #[test]
    fn scales_to_a_real_collection() {
        let holders: Vec<String> = (0..8_000).map(|i| format!("stake1holder{i:05}")).collect();
        let events: Vec<Acquisition<'_>> = (0..25_728)
            .map(|i| Acquisition::new(i as i64, holders[i % holders.len()].as_str(), 1))
            .collect();

        let d = distribution_at(&events, i64::MAX);
        assert_eq!(d.holders, 8_000);
        assert_eq!(d.distributed, 25_728);
        // Sorted desc by holdings: the busiest holder leads.
        assert!(d.top1 >= 1);
        assert!(d.top10 >= d.top1);
        // Half-way through the series, only the elapsed events count.
        let half = distribution_at(&events, 12_863);
        assert_eq!(half.distributed, 12_864);
    }
}
