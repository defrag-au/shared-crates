//! `MintArrivals` — watch a collection land in people's hands, one asset at a
//! time.
//!
//! Every asset is a dot. Every holder is a pile. As the playhead advances, dots
//! arrive and piles grow, and concentration stops being a statistic and becomes
//! a shape: a handful of mounds among hundreds of specks.
//!
//! ## Why not a graph
//!
//! The obvious reach is a node-link diagram of wallets. Five hundred nodes is a
//! hairball that says nothing, and the relationship being shown here is not
//! interesting anyway — every asset comes from the same place. What matters is
//! *how unevenly they land*, which is a question about quantity and position,
//! not connectivity.
//!
//! ## Dots never move once placed
//!
//! Each holder's pile is packed on a phyllotaxis spiral: dot `k` sits at
//! `r = c·√k`, `θ = k·137.5°`. That angle is the golden one, so points fill a
//! disc evenly with no spokes or rings — but the property that matters here is
//! that **dot `k`'s position depends only on `k`**. A pile grows outward as
//! assets arrive; it never reshuffles. Anything that repacked on every frame
//! would shimmer during playback and make the growth impossible to follow.
//!
//! ## Scale is set by the largest pile over the whole series
//!
//! Not by the pile currently on screen. If dot size were fitted to the visible
//! maximum, every arrival would rescale the entire field and nothing could be
//! compared to the frame before it.

use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// One asset arriving with a holder.
#[derive(Clone, Debug)]
pub struct Arrival<'a> {
    pub timestamp: i64,
    pub holder: &'a str,
    /// How many assets landed in this event.
    pub count: u32,
}

impl<'a> Arrival<'a> {
    pub fn new(timestamp: i64, holder: &'a str, count: u32) -> Self {
        Self {
            timestamp,
            holder,
            count,
        }
    }
}

/// The golden angle, in radians — 137.5°.
const GOLDEN_ANGLE: f32 = 2.399_963_2;

/// Where dot `k` sits within a pile, in units of the spacing constant.
///
/// Depends only on `k`, which is the whole point: piles grow, they never
/// rearrange.
pub fn pile_offset(k: u32) -> (f32, f32) {
    let r = (k as f32).sqrt();
    let theta = k as f32 * GOLDEN_ANGLE;
    (r * theta.cos(), r * theta.sin())
}

/// Holders in the order they first appear, with holdings at `at`.
///
/// Arrival order, not size order: sorting by holdings would move a wallet across
/// the screen as it grew, and the growth is the thing being watched.
pub fn piles_at<'a>(arrivals: &[Arrival<'a>], at: i64) -> Vec<(&'a str, u32)> {
    // Index by holder so this stays O(arrivals). The obvious `out.iter()
    // .position(...)` is O(arrivals × holders) — invisible at fixture scale and
    // pathological on a real collection (10k arrivals over ~8k holders is ~40M
    // string compares, EVERY FRAME).
    let mut index: HashMap<&'a str, usize> = HashMap::with_capacity(arrivals.len());
    let mut out: Vec<(&'a str, u32)> = Vec::new();
    for a in arrivals {
        let slot = *index.entry(a.holder).or_insert_with(|| {
            // First appearance fixes the position, even if this arrival is
            // still in the future — otherwise the layout would reflow as
            // the playhead moves.
            out.push((a.holder, 0));
            out.len() - 1
        });
        if a.timestamp <= at {
            out[slot].1 += a.count;
        }
    }
    out
}

/// The largest single pile across the whole series — the scale reference.
pub fn peak_pile(arrivals: &[Arrival<'_>]) -> u32 {
    let mut totals: HashMap<&str, u32> = HashMap::with_capacity(arrivals.len());
    for a in arrivals {
        *totals.entry(a.holder).or_insert(0) += a.count;
    }
    totals.into_values().max().unwrap_or(0)
}

pub struct MintArrivals<'a> {
    arrivals: &'a [Arrival<'a>],
    playhead: i64,
    /// How long, in the series' own time units, a newly-arrived dot spends
    /// flying in. Zero disables the flight entirely.
    flight: i64,
    dot_color: Option<Color32>,
    height: f32,
}

impl<'a> MintArrivals<'a> {
    pub fn new(arrivals: &'a [Arrival<'a>], playhead: i64) -> Self {
        Self {
            arrivals,
            playhead,
            flight: 0,
            dot_color: None,
            height: 320.0,
        }
    }

    /// Animate arrivals in over this much series-time.
    ///
    /// The flight exists only to draw the eye to *where* something landed. It is
    /// deliberately short, and a paused or scrubbed frame settles immediately —
    /// a still has to be readable, because a still is what gets screenshotted
    /// into a write-up.
    pub fn flight(mut self, flight: i64) -> Self {
        self.flight = flight;
        self
    }

    pub fn dot_color(mut self, c: Color32) -> Self {
        self.dot_color = Some(c);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let muted = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let dot = self
            .dot_color
            .unwrap_or(Color32::from_rgb(0x39, 0x87, 0xe5));

        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), self.height), Sense::hover());
        let hero_h = 30.0;
        let field = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + hero_h), rect.max);
        let painter = ui.painter_at(rect);

        let piles = piles_at(self.arrivals, self.playhead);
        if piles.is_empty() {
            return resp;
        }

        // Grid laid out to fill the field with roughly square cells.
        let n = piles.len();
        let aspect = (field.width() / field.height().max(1.0)).max(0.1);
        let cols = ((n as f32 * aspect).sqrt().ceil() as usize).max(1);
        let rows = n.div_ceil(cols);
        let cell_w = field.width() / cols as f32;
        let cell_h = field.height() / rows as f32;
        let cell_r = (cell_w.min(cell_h) * 0.5) * 0.92;

        // Spacing so the biggest pile in the WHOLE series exactly fills a cell.
        // Fitting to the visible maximum would rescale the field on every
        // arrival and make consecutive frames incomparable.
        let peak = peak_pile(self.arrivals).max(1);
        let spacing = cell_r / (peak as f32).sqrt().max(1.0);
        // Dot SIZE is decoupled from pile spacing. Sizing dots to fit the
        // largest pile makes a three-asset holder sub-pixel dust when the peak
        // is in the hundreds — and those small holders are most of the picture.
        // A legible floor means a whale's dots overlap into a solid disc, which
        // reads correctly as "this one is off the scale".
        let dot_r = (spacing * 0.42).clamp(1.15, 3.5);

        // Dots are emitted per ARRIVAL, not per holder, so each one knows when
        // it landed and can be flown in. Running counts give every dot its
        // stable index within its pile.
        let centre_of = |i: usize| {
            Pos2::new(
                field.left() + (i % cols) as f32 * cell_w + cell_w * 0.5,
                field.top() + (i / cols) as f32 * cell_h + cell_h * 0.5,
            )
        };
        let source = Pos2::new(field.center().x, field.top());

        let mut running: Vec<u32> = vec![0; piles.len()];
        let mut drawn = 0u32;
        for arrival in self.arrivals {
            if arrival.timestamp > self.playhead {
                continue;
            }
            let Some(i) = piles.iter().position(|(h, _)| *h == arrival.holder) else {
                continue;
            };
            let centre = centre_of(i);
            // 0 = just landed, 1 = settled. With `flight` unset everything is
            // settled, so a still frame is always the truth.
            let t = if self.flight > 0 {
                (((self.playhead - arrival.timestamp) as f32) / self.flight as f32).clamp(0.0, 1.0)
            } else {
                1.0
            };
            // Ease-out: fast away from the mint, settling gently into the pile.
            let eased = 1.0 - (1.0 - t) * (1.0 - t);

            for k in running[i]..running[i] + arrival.count {
                let (ox, oy) = pile_offset(k);
                let settled = Pos2::new(centre.x + ox * spacing, centre.y + oy * spacing);
                let p = if eased >= 1.0 {
                    settled
                } else {
                    source + (settled - source) * eased
                };
                painter.circle_filled(
                    p,
                    dot_r,
                    // In flight it is brighter, so the eye is pulled to what is
                    // happening now rather than to the accumulated mass.
                    if eased >= 1.0 {
                        dot
                    } else {
                        dot.lerp_to_gamma(Color32::WHITE, 0.5)
                    },
                );
                drawn += 1;
            }
            running[i] += arrival.count;
        }

        // Rings last, over the dots, on piles big enough to read as one object.
        for (i, (_h, count)) in piles.iter().enumerate() {
            if *count > peak / 4 && *count > 3 {
                painter.circle_stroke(
                    centre_of(i),
                    (*count as f32).sqrt() * spacing + dot_r + 1.0,
                    Stroke::new(1.0_f32, dot.gamma_multiply(0.5)),
                );
            }
        }

        let holders = piles.iter().filter(|(_, c)| *c > 0).count();
        let biggest = piles.iter().map(|(_, c)| *c).max().unwrap_or(0);
        painter.text(
            Pos2::new(rect.left(), rect.top() + 2.0),
            Align2::LEFT_TOP,
            format!("{drawn} assets · {holders} holders"),
            FontId::monospace(14.0),
            ink,
        );
        painter.text(
            Pos2::new(rect.left(), rect.top() + 18.0),
            Align2::LEFT_TOP,
            format!("largest single holder: {biggest}"),
            FontId::proportional(10.0),
            muted,
        );

        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(t: i64, h: &'static str, n: u32) -> Arrival<'static> {
        Arrival::new(t, h, n)
    }

    /// The property the whole layout rests on: a dot's position depends only on
    /// its index, so a pile grows outward instead of reshuffling.
    #[test]
    fn a_dots_position_depends_only_on_its_index() {
        let small = pile_offset(3);
        let large = pile_offset(3);
        assert_eq!(small, large);
        // And distinct indices are distinct places.
        assert_ne!(pile_offset(3), pile_offset(4));
    }

    /// Piles pack outward: later dots sit further from the centre.
    #[test]
    fn later_dots_sit_further_out() {
        let r = |k: u32| {
            let (x, y) = pile_offset(k);
            (x * x + y * y).sqrt()
        };
        assert!(r(0) < r(1));
        assert!(r(10) < r(50));
        assert!(r(50) < r(200));
    }

    #[test]
    fn holdings_accumulate_up_to_the_playhead() {
        let v = vec![a(10, "alice", 2), a(20, "bob", 1), a(30, "alice", 3)];
        let at10 = piles_at(&v, 10);
        assert_eq!(at10.iter().find(|(h, _)| *h == "alice").unwrap().1, 2);
        let at99 = piles_at(&v, 99);
        assert_eq!(at99.iter().find(|(h, _)| *h == "alice").unwrap().1, 5);
        assert_eq!(at99.iter().find(|(h, _)| *h == "bob").unwrap().1, 1);
    }

    /// Layout is fixed by first appearance across the WHOLE series, so cells do
    /// not reflow as the playhead advances.
    #[test]
    fn positions_are_stable_as_the_playhead_moves() {
        let v = vec![a(10, "first", 1), a(20, "second", 1), a(30, "third", 1)];
        let order = |at: i64| {
            piles_at(&v, at)
                .into_iter()
                .map(|(h, _)| h)
                .collect::<Vec<_>>()
        };
        assert_eq!(order(0), ["first", "second", "third"]);
        assert_eq!(order(15), ["first", "second", "third"]);
        assert_eq!(order(99), ["first", "second", "third"]);
    }

    /// Scale comes from the whole series, not what is currently visible —
    /// otherwise every arrival rescales the field.
    #[test]
    fn peak_is_taken_over_the_entire_series() {
        let v = vec![a(1, "small", 5), a(100, "whale", 400)];
        assert_eq!(peak_pile(&v), 400, "even though the whale arrives last");
    }

    #[test]
    fn peak_sums_repeat_arrivals_for_one_holder() {
        let v = vec![a(1, "w", 100), a(2, "w", 150), a(3, "other", 200)];
        assert_eq!(peak_pile(&v), 250);
    }

    #[test]
    fn an_empty_series_is_well_defined() {
        assert!(piles_at(&[], 0).is_empty());
        assert_eq!(peak_pile(&[]), 0);
    }

    /// Concentration is what the picture is for: one pile far larger than the
    /// rest has to be far larger on screen too.
    #[test]
    fn a_whale_pile_dwarfs_the_others_in_radius() {
        let radius = |count: u32| (count as f32).sqrt();
        assert!(radius(400) > radius(4) * 9.0, "100x assets is 10x radius");
    }
    /// Real-collection scale. Under the previous O(arrivals × holders) scans
    /// this was ~40M string compares per call and the widget ran them EVERY
    /// FRAME; a 10k-asset mint pegged a core. Keep it linear.
    #[test]
    fn scales_to_a_real_collection() {
        let holders: Vec<String> = (0..8_000).map(|i| format!("stake1holder{i:05}")).collect();
        let arrivals: Vec<Arrival<'_>> = (0..10_001)
            .map(|i| Arrival::new(i as i64, holders[i % holders.len()].as_str(), 1))
            .collect();

        let piles = piles_at(&arrivals, i64::MAX);
        assert_eq!(piles.len(), holders.len(), "one pile per distinct holder");
        // First appearance fixes position — order must be arrival order.
        assert_eq!(piles[0].0, holders[0]);
        assert_eq!(
            piles.iter().map(|(_, n)| *n as usize).sum::<usize>(),
            10_001
        );
        // 10_001 over 8_000 holders: the first 2_001 get two, the rest one.
        assert_eq!(peak_pile(&arrivals), 2);
    }
}
