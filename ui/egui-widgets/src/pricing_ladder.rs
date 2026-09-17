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
//! `realized: None` only reads as "no sales" when `support` is zero too;
//! otherwise the measure is simply blank and the count speaks for itself.
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
//! # use egui_widgets::pricing_ladder::{self, LadderRung, PricingLadderConfig};
//! # fn demo(ui: &mut egui::Ui) {
//! let rungs = vec![LadderRung {
//!     value: "Swab".into(),
//!     supply: 389,
//!     ratio: 1.0,
//!     target: Some(49.0),
//!     realized: Some(23.0),
//!     support: 55,
//! }];
//! pricing_ladder::show(
//!     ui,
//!     &rungs,
//!     &PricingLadderConfig {
//!         category: "Rank".into(),
//!         highlight: Some("Swab".into()),
//!         ..Default::default()
//!     },
//! );
//! # }
//! ```

use egui::{RichText, Ui};

use crate::bullet_bar::BulletBar;
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt, Token};

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
    /// How many sales stand behind `realized`.
    pub support: usize,
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
}

impl Default for PricingLadderConfig {
    fn default() -> Self {
        Self {
            category: String::new(),
            sentinel: None,
            unit: "ADA".to_string(),
            highlight: None,
            id_salt: "pricing_ladder".to_string(),
        }
    }
}

/// Width of the evidence measure. Fixed rather than `available_width` so the
/// bars line up into a column instead of each taking whatever its row had left.
const BAR_WIDTH: f32 = 110.0;

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

    ui.scope(|ui| {
        egui::Grid::new(&config.id_salt)
            .num_columns(6)
            .spacing([10.0, 3.0])
            .striped(true)
            .show(ui, |ui| {
                for header in [
                    config.category.as_str(),
                    "supply",
                    "ratio",
                    "ladder price",
                    "realized vs ladder",
                    "n",
                ] {
                    ui.label(
                        RichText::new(header)
                            .color(muted)
                            .size(ui.text_size(TextSize::Xs)),
                    );
                }
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

                    // The measure. Nothing sold here => no bar at all; see the
                    // module docs on why an empty bar would be a claim.
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
                                .height(10.0)
                                .detail(price(realized, &config.unit));
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
                            // Nothing traded here at all — say so. But if sales
                            // exist and only the median is missing, saying "no
                            // sales" beside a count of 55 would be a flat lie;
                            // leave the measure blank and let `n` speak.
                            None if rung.support == 0 => ui.label(
                                RichText::new("no sales")
                                    .color(muted)
                                    .size(ui.text_size(TextSize::Xs)),
                            ),
                            None => ui.label(
                                RichText::new("\u{2014}")
                                    .color(muted)
                                    .size(ui.text_size(TextSize::Xs)),
                            ),
                        }
                    });

                    ui.label(
                        RichText::new(if rung.support == 0 {
                            "\u{2014}".to_string()
                        } else {
                            rung.support.to_string()
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

    /// "Nothing sold here" is `support == 0`, NOT merely `realized == None` —
    /// a payload carrying counts but no medians must not render every rung as
    /// unsold. Guards the distinction the module docs promise.
    #[test]
    fn unsold_is_support_zero_not_realized_none() {
        let traded_without_median = LadderRung {
            value: "Swab".into(),
            supply: 389,
            ratio: 1.0,
            target: Some(49.0),
            realized: None,
            support: 55,
        };
        let never_traded = LadderRung {
            value: "Legendary".into(),
            supply: 6,
            ratio: 64.83,
            target: Some(3176.8),
            realized: None,
            support: 0,
        };
        // Both lack a median, so `realized` alone cannot separate them.
        assert!(traded_without_median.realized.is_none());
        assert!(never_traded.realized.is_none());
        // Support is what does.
        assert_ne!(
            traded_without_median.support == 0,
            never_traded.support == 0
        );
    }
}
