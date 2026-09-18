//! `PricingLadder` — a structural price ladder: what each rung is worth, what
//! actually sold there, and how much evidence stands behind that.
//!
//! Some collections do not price off a curve fitted to sales — they price
//! *structurally*, every rung of one trait sitting at a fixed multiple of the
//! floor rung. The multiple comes from supply (a 15-supply rank is worth
//! `389/15` times a 389-supply one), so the ladder exists in full the moment
//! the collection does, including for rungs nothing has ever sold at.
//!
//! That is exactly why this cannot be a sales chart. A scatter of realized
//! prices shows the rungs that traded and silently omits the rest, which is
//! backwards: the rungs with no sales are the expensive ones, and they are the
//! ones a reader most needs priced. So the ladder is the spine, and sales are
//! drawn against it as evidence.
//!
//! ## Support is shown, never folded into the price
//!
//! Each rung carries how many sales stand behind its realized figure. A rung
//! priced off four sales and one priced off fifty-five are not the same claim,
//! and a bare median hides that. The count sits beside the bar rather than
//! adjusting it, because the structural price does not care how much trading
//! happened — that is the point of a structural model.
//!
//! ## A rung with no sales draws no bar
//!
//! Not an empty bar, and not a zero. [`BulletBar`](crate::bullet_bar) exists
//! partly to make that distinction (see its module docs): a zero value is a
//! claim that nothing was paid, where the truth is that nothing was *observed*.
//! Those rungs say "no sales" in muted text and leave the measure blank.
//!
//! **"Nothing sold here" and "the median wasn't supplied" are separate states**,
//! and conflating them is the easy bug. A caller whose payload carries
//! [`support`](LadderRung::support) but not [`realized`](LadderRung::realized)
//! — which is the shape of a lean public API — would otherwise render every
//! rung as "no sales" while the count column beside it says fifty-five. So
//! `realized: None` only reads as "no sales" when `support` is zero too.
//!
//! And not even then, if the lane disagrees. A demand model that gates its
//! median on a minimum group size reports **support 0 for a rung that did
//! trade** — it just did not trade enough times to be trusted. Measured on
//! Black Flag: Navigator and First Mate each have two real fills and a reported
//! support of zero. So "no sales" additionally requires
//! [`fills`](LadderRung::fills) to be empty; otherwise the marks on the lane
//! would sit beside a claim that nothing sold.
//!
//! ## The pulse lane
//!
//! Each rung's fills as marks on one shared time axis. The aggregate columns
//! say what a rung clears at; this says *when*, and the difference matters
//! because a weighted median hides its own staleness — Quartermaster's figure
//! rests on four fills, the newest of which is a year old, and no amount of
//! looking at the number reveals that. On the lane it is the empty right-hand
//! half.
//!
//! The axis therefore ends at [`now`](PricingLadderConfig::now), never at the
//! newest fill. Scaling to the data would slide every dead rung's last trade up
//! to the right edge and make it look current.
//!
//! ## The bars are per-row, and deliberately not comparable to each other
//!
//! A ladder spans two orders of magnitude, so one shared scale would flatten
//! every cheap rung into an invisible sliver. Each row is scaled to its own
//! rung instead, because the question a reader brings to a row is "did this
//! sell at, above, or below what the structure says it is worth" — a
//! within-row comparison. Cross-rung magnitude is what the price column and
//! the ratio column are for.
//!
//! ```no_run
//! # use egui_widgets::pricing_ladder::{self, Fill, LadderRung, PricingLadderConfig};
//! # fn demo(ui: &mut egui::Ui) {
//! let rungs = vec![LadderRung {
//!     value: "Swab".into(),
//!     supply: 389,
//!     ratio: 1.0,
//!     target: Some(49.0),
//!     realized: Some(23.0),
//!     support: 55,
//!     fills: vec![
//!         Fill { at: 1_788_873_467, price: 24.0 },
//!         Fill { at: 1_785_009_849, price: 19.0 },
//!     ],
//! }];
//! pricing_ladder::show(
//!     ui,
//!     &rungs,
//!     &PricingLadderConfig {
//!         category: "Rank".into(),
//!         highlight: Some("Swab".into()),
//!         now: 1_789_683_535,
//!         ..Default::default()
//!     },
//! );
//! # }
//! ```

use egui::{RichText, Stroke, Ui};

use crate::bullet_bar::BulletBar;
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt, Token};

/// One observed fill: when it happened and what it cleared at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fill {
    /// Unix seconds.
    pub at: i64,
    /// Display units (ADA), matching [`LadderRung::target`].
    pub price: f64,
}

/// How a rung's fills are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaneStyle {
    /// Time only — every fill the same mark. Compact, and all that can be drawn
    /// when rungs have no ladder price to measure against.
    Ticks,
    /// Time **and** price: x is when, y is the fill's price as a multiple of
    /// its own rung's ladder price, so the lane's rule stops being decoration
    /// and becomes the datum — marks above it cleared over the ladder, below
    /// it under.
    ///
    /// This is what lets one instrument answer both questions. A separate
    /// measure beside a separate lane makes the reader join "what it clears
    /// at" to "when it cleared" by eye; here a drift upward over time *is* a
    /// rung appreciating, with no claim asserted on top of the marks.
    #[default]
    PriceScatter,
}

impl LaneStyle {
    /// Whether to draw the [`BulletBar`] measure beside the lane.
    ///
    /// **Only under [`Ticks`](Self::Ticks).** A price scatter already places
    /// every fill against the ladder price, so a bar restating where the median
    /// landed is the same question answered twice — and being the loudest mark
    /// in the row, it wins the reader's eye while carrying strictly less than
    /// the lane beside it. Under `Ticks` the lane carries no price at all, so
    /// the bar is the only thing answering "at what", and it stays.
    fn shows_measure(self) -> bool {
        matches!(self, LaneStyle::Ticks)
    }
}

/// One rung, in **display units** (ADA, not lovelace).
///
/// The caller converts: this widget has no business knowing a chain's
/// denomination, and a `u64` of lovelace would force it to decide on decimals
/// and thousands separators that the caller has already resolved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LadderRung {
    /// The trait value this rung is — `"Carpenter"`, `"Captain"`.
    pub value: String,
    /// How many assets carry it. Drives the ratio, shown so the reader can
    /// check the arithmetic rather than take it on trust.
    pub supply: u64,
    /// Multiple of the sentinel rung's price.
    pub ratio: f64,
    /// What the structure says the rung is worth. `None` when no anchor has
    /// resolved yet (an empty book), which is distinct from zero.
    pub target: Option<f64>,
    /// What actually sold here — a median. `None` when nothing has.
    pub realized: Option<f64>,
    /// How many sales the model's `realized` median rests on — which is **not**
    /// necessarily how many sales happened. A model that gates its median on a
    /// minimum group size reports zero here for a rung that did trade.
    /// [`fills`](Self::fills) is the observed truth; this is the model's
    /// confidence in its own number.
    pub support: usize,
    /// Every observed fill on this rung, any order.
    ///
    /// Raw observations, not a statistic — so unlike [`realized`](Self::realized)
    /// a caller may derive these locally without inheriting a weighting
    /// question. These drive the lane *and* the displayed count, because a rung
    /// that traded twice should say two however little the model trusts it.
    pub fills: Vec<Fill>,
}

/// Framing the rows cannot carry themselves.
#[derive(Clone, Debug)]
pub struct PricingLadderConfig {
    /// The trait the ladder is built on — `"Rank"`.
    pub category: String,
    /// The rung everything else is a multiple of. Marked in the list.
    pub sentinel: Option<String>,
    /// Appended to every price. `"ADA"` by default.
    pub unit: String,
    /// A rung to pick out — typically the one the asset on screen sits on.
    pub highlight: Option<String>,
    /// Disambiguates the grid when several ladders share a surface.
    pub id_salt: String,
    /// Now, in unix seconds. **The pulse lane's right edge, always.**
    ///
    /// Anchoring it to the newest fill instead would rescale every dead rung
    /// until its last trade touched the right edge — a rung that last sold a
    /// year ago would read as current. The whole value of the lane is the
    /// empty space on the right.
    ///
    /// Zero disables the lane entirely (the column is not drawn).
    pub now: i64,
    /// Whether the lane carries price as well as time.
    pub lane: LaneStyle,
}

impl Default for PricingLadderConfig {
    fn default() -> Self {
        Self {
            category: String::new(),
            sentinel: None,
            unit: "ADA".to_string(),
            highlight: None,
            id_salt: "pricing_ladder".to_string(),
            now: 0,
            lane: LaneStyle::default(),
        }
    }
}

/// Width of the evidence measure. Fixed rather than `available_width` so the
/// bars line up into a column instead of each taking whatever its row had left.
const BAR_WIDTH: f32 = 110.0;

/// Width of the pulse lane. Wider than the measure — it carries years, and the
/// question it answers is where the marks *clump*, which needs room.
const LANE_WIDTH: f32 = 150.0;

/// The dearest fill at an instant. Several assets on one rung routinely settle
/// in one transaction and so share a timestamp exactly; the highest is the
/// honest one to emphasise, since it is the mark a reader would otherwise
/// think the lane had omitted.
fn price_at(fills: &[Fill], at: i64) -> f64 {
    fills
        .iter()
        .filter(|f| f.at == at)
        .map(|f| f.price)
        .fold(f64::MIN, f64::max)
}

/// Vertical span of a price-scatter lane. Taller than a tick lane because it
/// now carries a second dimension; below about this the octaves collapse and
/// the datum stops separating from the marks.
const SCATTER_HEIGHT: f32 = 22.0;

/// The y-domain, in octaves around the ladder price. Shared across every rung
/// so rows compare, and clamped rather than fitted: Black Flag has fills at
/// 0.004× (nominal min-ADA transfers dressed as sales), and letting one of
/// those set the floor would squash every real fill into the top sliver.
const OCTAVES_BELOW: f32 = 4.0;
const OCTAVES_ABOVE: f32 = 1.5;

/// Draw one rung's fills on a lane whose x-domain is `(from, now)`, shared by
/// every row so the columns are comparable.
///
/// In [`LaneStyle::PriceScatter`] the horizontal rule is the rung's own ladder
/// price and y is `log2(price / ladder)`, so height is a *multiple* — the
/// natural scale for prices, where a 2× move looks the same whether the rung
/// is 49 ADA or 3,176.
///
/// The most recent fill is drawn solid and larger: "when did this last trade"
/// is the question a stale rung has to answer, and it should not be left to
/// the eye to find the right-most smudge.
fn draw_lane(
    ui: &mut Ui,
    fills: &[Fill],
    target: Option<f64>,
    from: i64,
    now: i64,
    style: LaneStyle,
) -> egui::Response {
    let tokens = ui.tokens();
    // Price needs a datum to be a multiple of; without one, fall back to time.
    let scatter = matches!(style, LaneStyle::PriceScatter) && target.is_some_and(|t| t > 0.0);
    let height = if scatter { SCATTER_HEIGHT } else { 11.0 };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(LANE_WIDTH, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Where the ladder price sits. In scatter mode this is the datum every
    // mark is read against, so it is drawn firmer than a tick lane's rule.
    let datum_y = if scatter {
        rect.top() + rect.height() * (OCTAVES_ABOVE / (OCTAVES_ABOVE + OCTAVES_BELOW))
    } else {
        rect.center().y
    };
    // A tick lane's rule is its baseline and has to be there. A scatter's does
    // NOT get one: the datum sits at the same height on every row by
    // construction (the octave window is fixed so rows compare), so drawing it
    // per row repeats one line thirteen times while telling the reader nothing
    // that varies. The reference lives in the marks instead — see `side`.
    if !scatter {
        painter.line_segment(
            [
                egui::pos2(rect.left(), datum_y),
                egui::pos2(rect.right(), datum_y),
            ],
            Stroke::new(0.5, tokens.color.border),
        );
    }

    let span = (now - from).max(1) as f32;
    let x_of = |t: i64| rect.left() + (t - from) as f32 / span * rect.width();
    let y_of = |price: f64| {
        let Some(t) = target.filter(|t| *t > 0.0) else {
            return rect.center().y;
        };
        let oct = ((price / t).max(1e-6)).log2() as f32;
        let frac = (OCTAVES_ABOVE - oct.clamp(-OCTAVES_BELOW, OCTAVES_ABOVE))
            / (OCTAVES_ABOVE + OCTAVES_BELOW);
        rect.top() + frac * rect.height()
    };

    // Which side of the ladder price a fill cleared on, as colour — so the
    // datum is locatable (it is where the colour flips) without spending a
    // rule on every row. On a collection that almost never clears above
    // structure the lane is nearly monochrome, which is itself the finding;
    // the one mark that goes the other way then genuinely stands out.
    let side = |price: f64| match target {
        Some(t) if t > 0.0 && price > t => tokens.series.inbound(),
        _ if scatter => tokens.series.outbound(),
        _ => tokens.series.other,
    };
    let newest = fills.iter().map(|f| f.at).max();

    for f in fills {
        if Some(f.at) == newest {
            continue;
        }
        let x = x_of(f.at).clamp(rect.left(), rect.right());
        if scatter {
            painter.circle_filled(
                egui::pos2(x, y_of(f.price)),
                1.7,
                side(f.price).gamma_multiply(0.8),
            );
        } else {
            painter.line_segment(
                [egui::pos2(x, datum_y - 3.0), egui::pos2(x, datum_y + 3.0)],
                Stroke::new(1.5, side(f.price).gamma_multiply(0.45)),
            );
        }
    }
    // The newest fill, emphasised. In scatter mode it keeps its price — moving
    // it to the top would misreport what the latest trade actually cleared at.
    if let Some(at) = newest {
        let x = x_of(at).clamp(rect.left(), rect.right());
        if scatter {
            let price = price_at(fills, at);
            painter.circle_filled(egui::pos2(x, y_of(price)), 3.2, side(price));
        } else {
            painter.line_segment(
                [
                    egui::pos2(x, rect.top() + 1.0),
                    egui::pos2(x, rect.bottom() - 1.0),
                ],
                Stroke::new(1.5, side(price_at(fills, at))),
            );
        }
    }
    response
}

/// `YYYY-MM` for the lane's left edge, via the civil-date conversion the time
/// widgets already share, so the ladder and the spine name a date the same way.
fn month_label(unix: i64) -> String {
    let (y, m, _) = crate::time_spine::civil_from_unix(unix);
    format!("{y}-{m:02}")
}

/// Price formatting: thousands get a `k` so a 3,176 ADA rung does not widen the
/// column past everything else in it.
fn price(v: f64, unit: &str) -> String {
    if v >= 1000.0 {
        format!("{:.1}k {unit}", v / 1000.0)
    } else {
        format!("{v:.0} {unit}")
    }
}

pub fn show(ui: &mut Ui, rungs: &[LadderRung], config: &PricingLadderConfig) -> egui::Response {
    let tokens = ui.tokens();
    let muted = tokens.color.text_muted;
    let secondary = tokens.color.text_secondary;
    let primary = tokens.color.text_primary;
    let accent = tokens.color.accent_yellow;

    // One domain for every lane, so the columns mean the same thing. The left
    // edge is the oldest fill anywhere on the ladder; the right edge is always
    // `now`.
    let oldest = rungs
        .iter()
        .flat_map(|r| r.fills.iter())
        .map(|f| f.at)
        .min();
    let lane_domain = match (oldest, config.now) {
        (Some(from), now) if now > from => Some((from, now)),
        _ => None,
    };
    // Says what the axes ARE. Without it the lane is a pretty smear: a reader
    // cannot guess that height is a multiple of the rung's own ladder price,
    // and guessing wrong is worse than not reading it.
    let pulse_header = lane_domain
        .map(|(from, _)| match config.lane {
            LaneStyle::PriceScatter => format!(
                "pulse  {} \u{2192} now  \u{00b7}  height = \u{00d7} ladder price",
                month_label(from)
            ),
            LaneStyle::Ticks => format!("pulse  {} \u{2192} now", month_label(from)),
        })
        .unwrap_or_default();

    ui.scope(|ui| {
        egui::Grid::new(&config.id_salt)
            .num_columns(
                6 + usize::from(lane_domain.is_some()) + usize::from(config.lane.shows_measure()),
            )
            .spacing([10.0, 3.0])
            .striped(true)
            .show(ui, |ui| {
                for header in [
                    config.category.as_str(),
                    "supply",
                    "ratio",
                    "ladder price",
                    "realized",
                ] {
                    ui.label(
                        RichText::new(header)
                            .color(muted)
                            .size(ui.text_size(TextSize::Xs)),
                    );
                }
                if config.lane.shows_measure() {
                    ui.label(
                        RichText::new("vs ladder")
                            .color(muted)
                            .size(ui.text_size(TextSize::Xs)),
                    );
                }
                if lane_domain.is_some() {
                    ui.label(
                        RichText::new(&pulse_header)
                            .color(muted)
                            .size(ui.text_size(TextSize::Xs)),
                    );
                }
                ui.label(
                    RichText::new("n")
                        .color(muted)
                        .size(ui.text_size(TextSize::Xs)),
                );
                ui.end_row();

                for rung in rungs {
                    let is_highlight = config.highlight.as_deref() == Some(rung.value.as_str());
                    let is_sentinel = config.sentinel.as_deref() == Some(rung.value.as_str());

                    let name = if is_sentinel {
                        format!("{} \u{00b7} anchor", rung.value)
                    } else {
                        rung.value.clone()
                    };
                    let mut label = RichText::new(name)
                        .color(if is_highlight { accent } else { primary })
                        .size(ui.text_size(TextSize::Sm));
                    if is_highlight {
                        label = label.strong();
                    }
                    ui.label(label);

                    ui.label(
                        RichText::new(rung.supply.to_string())
                            .color(muted)
                            .size(ui.text_size(TextSize::Xs)),
                    );
                    ui.label(
                        RichText::new(format!("\u{00d7}{:.2}", rung.ratio))
                            .color(secondary)
                            .size(ui.text_size(TextSize::Xs)),
                    );
                    ui.label(
                        RichText::new(match rung.target {
                            Some(t) => price(t, &config.unit),
                            None => "\u{2014}".to_string(),
                        })
                        .color(if is_highlight { accent } else { secondary })
                        .size(ui.text_size(TextSize::Sm)),
                    );

                    // The realized figure as a number, in its own column. It is
                    // NOT the bar's `detail` — that stacks the text above the
                    // bar, which turns every row of a 13-rung table into two
                    // lines and knocks the numbers out of a scannable column.
                    ui.label(
                        RichText::new(match (rung.realized, rung.support, rung.fills.is_empty()) {
                            (Some(r), _, _) => price(r, &config.unit),
                            // Nothing traded here at all — say so, but only
                            // when the lane agrees. A model that gates its
                            // median on a minimum group size reports support 0
                            // for a rung that DID trade, just not enough times
                            // to be trusted; "no sales" beside two marks on the
                            // lane is a visible contradiction.
                            (None, 0, true) => "no sales".to_string(),
                            (None, _, _) => "\u{2014}".to_string(),
                        })
                        .color(if rung.realized.is_some() {
                            secondary
                        } else {
                            muted
                        })
                        .size(ui.text_size(TextSize::Xs)),
                    );

                    // The measure. Nothing sold here => no bar at all; see the
                    // module docs on why an empty bar would be a claim.
                    if config.lane.shows_measure() {
                        ui.scope(|ui| {
                            ui.set_width(BAR_WIDTH);
                            match rung.realized {
                                Some(realized) => {
                                    // Scale to whichever of the pair is larger so a
                                    // rung trading well above its ladder price still
                                    // shows the marker rather than pinning it right.
                                    let hi = realized.max(rung.target.unwrap_or(realized)) * 1.15;
                                    let bar = BulletBar::with_target(
                                        realized as f32,
                                        rung.target.map(|t| t as f32),
                                    )
                                    .max(hi.max(f64::EPSILON) as f32)
                                    .height(9.0);
                                    // "At the ladder price" within 10% — the band
                                    // where the structure is being confirmed rather
                                    // than contradicted.
                                    match rung.target {
                                        Some(t) => bar
                                            .good_within(Token::Success, (t * 0.1) as f32)
                                            .show(ui),
                                        None => bar.show(ui),
                                    }
                                }
                                None => ui.label(""),
                            }
                        });
                    }

                    if let Some((from, now)) = lane_domain {
                        let resp = draw_lane(ui, &rung.fills, rung.target, from, now, config.lane);
                        if let Some(last) = rung.fills.iter().map(|f| f.at).max() {
                            let days = (now - last).max(0) / 86_400;
                            let mut tip = format!(
                                "{} fill(s) \u{00b7} last traded {}",
                                rung.fills.len(),
                                if days == 0 {
                                    "today".to_string()
                                } else {
                                    format!("{days} days ago")
                                }
                            );
                            // Say so when the model is standing on less than
                            // what actually traded — otherwise the gap between
                            // this count and the model's is invisible.
                            if rung.support != rung.fills.len() {
                                tip.push_str(&format!(
                                    "\n{} of them back the median",
                                    rung.support
                                ));
                            }
                            resp.on_hover_text(tip);
                        }
                    }

                    // The OBSERVED count, not the model's support. A rung that
                    // traded twice says two, however little a minimum-group-size
                    // gate trusts it — reporting zero there reads as "never
                    // traded" and is the confusion this column exists to avoid.
                    let observed = if rung.fills.is_empty() {
                        rung.support
                    } else {
                        rung.fills.len()
                    };
                    ui.label(
                        RichText::new(if observed == 0 {
                            "\u{2014}".to_string()
                        } else {
                            observed.to_string()
                        })
                        .color(muted)
                        .size(ui.text_size(TextSize::Xs)),
                    );
                    ui.end_row();
                }
            });
        ui.gap(Space::Xs);
    })
    .response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_compacts_thousands() {
        assert_eq!(price(49.0, "ADA"), "49 ADA");
        assert_eq!(price(105.9, "ADA"), "106 ADA");
        assert_eq!(price(3176.8, "ADA"), "3.2k ADA");
    }

    /// The three ways a rung can lack a median, and the one that is genuinely
    /// "nothing sold here". Guards the distinction the module docs promise —
    /// all three cases are real, measured on Black Flag.
    #[test]
    fn unsold_needs_both_zero_support_and_an_empty_lane() {
        let rung = |support, ats: &[i64]| LadderRung {
            value: "x".into(),
            supply: 1,
            ratio: 1.0,
            target: Some(1.0),
            realized: None,
            support,
            fills: ats.iter().map(|&at| Fill { at, price: 1.0 }).collect(),
        };
        // The predicate the realized column renders "no sales" for.
        let unsold = |r: &LadderRung| r.realized.is_none() && r.support == 0 && r.fills.is_empty();

        // Counts but no medians — a lean public payload (Swab: n=55).
        assert!(!unsold(&rung(55, &[])));
        // Traded, but under the model's trust threshold, so it reports
        // support 0 while the lane carries two real marks (Navigator).
        assert!(!unsold(&rung(0, &[1_754_682_598, 1_764_163_517])));
        // Genuinely never traded (Legendary).
        assert!(unsold(&rung(0, &[])));
    }

    /// The lane's right edge is `now`, never the newest fill — otherwise every
    /// dead rung rescales until its last trade touches the edge and reads as
    /// current. Checks the mapping puts a year-old fill in the left half.
    #[test]
    fn lane_anchors_its_right_edge_at_now() {
        let (from, now) = (1_734_723_500_i64, NOW_FIXTURE);
        let stale = 1_757_645_837_i64; // Quartermaster's newest, ~1yr before now
        let frac = (stale - from) as f64 / (now - from) as f64;
        assert!(
            frac < 0.5,
            "a year-old fill should sit left of centre, got {frac}"
        );
    }

    const NOW_FIXTURE: i64 = 1_789_683_535;

    /// The displayed count is the OBSERVED one. Navigator really did trade
    /// twice while the model reported support 0 — showing the model's number
    /// there is the confusion this fixes.
    #[test]
    fn displayed_count_is_observed_not_model_support() {
        let observed = |support: usize, n_fills: usize| {
            if n_fills == 0 { support } else { n_fills }
        };
        assert_eq!(observed(0, 2), 2, "a gated rung must report its real fills");
        assert_eq!(observed(55, 55), 55);
        // No lane data supplied at all — fall back to whatever the model said.
        assert_eq!(observed(55, 0), 55);
        assert_eq!(observed(0, 0), 0);
    }
}
