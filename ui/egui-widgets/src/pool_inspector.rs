//! `PoolInspector` — a liquidity pool's ACTUAL state, including the parts that are easy to read wrongly.
//!
//! A development and diagnosis surface, not a trading one. Where
//! [`crate::pool_liquidity_indicator`] answers "which pool should get what
//! share of a split", this answers "what is in this pool right now, and does
//! it add up".
//!
//! It exists because both pool types we spend directly hold value that is NOT
//! on the curve, and mistaking one for the other is silent:
//!
//! - A **Splash royalty pool** accrues treasury and royalty INSIDE the pool
//!   UTxO. The constant product trades against the balance MINUS those, so a
//!   surface that shows the raw balance is showing a number no swap uses.
//! - A **LumpPad pool** holds its two unclaimed fee buckets in the same UTxO
//!   as the curve reserve, plus a virtual reserve that exists only in the
//!   datum. Its invariant — quote asset in the UTxO equals reserve plus both
//!   buckets — is the one assertion that catches a mis-decoded datum, so the
//!   widget checks it and says so rather than rendering a plausible lie.
//!
//! Both views draw the same composition bar, so "how much of this is really
//! tradable" reads the same way whichever venue you are looking at.

use egui::{Color32, RichText, Ui};

use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};

// ============================================================================
// Types
// ============================================================================

/// A LumpPad pool: a LUMP-quoted constant product with a virtual reserve and
/// two in-pool fee buckets.
#[derive(Clone, Debug)]
pub struct LumpPadPoolView {
    /// `tx_hash#index`, so a reader can go and look at it.
    pub utxo_ref: String,
    /// The token this pool trades.
    pub ticker: String,
    /// `R` — quote asset on the curve.
    pub reserve: u64,
    /// `T` — tokens still in the pool.
    pub tokens: u64,
    /// `A` — unclaimed platform fees.
    pub platform_bucket: u64,
    /// `B` — unclaimed creator fees.
    pub creator_bucket: u64,
    /// Added to `reserve` for pricing, but held by nobody.
    pub virtual_reserve: u64,
    /// What the UTxO actually holds of the quote asset. Compared against
    /// `reserve + platform_bucket + creator_bucket`.
    pub quote_in_utxo: u64,
    /// Ticker of the quote asset — LUMP, for every pool so far.
    pub quote_ticker: String,
    /// Spot price as a rational, so no precision is lost before display.
    pub spot_price_num: u64,
    pub spot_price_den: u64,
    /// Proportion of supply still inside the pool, in basis points — the
    /// overhang a quote-asset collapse would make cheap.
    pub in_pool_bps: u32,
    /// Whether the datum's fee schedule matches the registry's record. False
    /// means this is a launch we have not seen, and quoting it against our
    /// constants would be wrong silently.
    pub schedule_matches_registry: bool,
}

impl LumpPadPoolView {
    /// What the invariant says the UTxO should hold.
    pub fn expected_quote(&self) -> u64 {
        self.reserve
            .saturating_add(self.platform_bucket)
            .saturating_add(self.creator_bucket)
    }

    /// `LUMP in UTxO == R + A + B`. False means a mis-decoded datum or a pool
    /// that is not what we think it is — either way, do not trade it.
    pub fn invariant_holds(&self) -> bool {
        self.quote_in_utxo == self.expected_quote()
    }

    /// Spot price as a float, for display only.
    pub fn spot_price(&self) -> f64 {
        if self.spot_price_den == 0 {
            return 0.0;
        }
        self.spot_price_num as f64 / self.spot_price_den as f64
    }
}

/// One side of a Splash royalty pool.
#[derive(Clone, Debug)]
pub struct RoyaltySide {
    pub ticker: String,
    /// How many of this asset's units make one of what `ticker` names — 6 for
    /// ADA, 0 for LUMP and for every LumpPad token.
    ///
    /// Not optional, and not defaulted to zero. Every quantity on chain is an
    /// integer of the smallest unit, so a side that does not carry its own
    /// scale renders 29,549,431,005 lovelace as "29,549,431,005 ADA" — a pool
    /// a thousand times the size of Cardano's supply, stated with total
    /// confidence.
    pub decimals: u8,
    /// What the UTxO holds.
    pub balance: u64,
    /// Accrued treasury, owed out of `balance`.
    pub treasury: u64,
    /// Accrued royalty, owed out of `balance`.
    pub royalty: u64,
}

impl RoyaltySide {
    /// What the constant product actually trades against.
    pub fn effective(&self) -> u64 {
        self.balance
            .saturating_sub(self.treasury)
            .saturating_sub(self.royalty)
    }

    /// `quantity` at this side's scale, grouped, without the ticker — the
    /// rows name the asset in their label already.
    fn figure(&self, quantity: u64, max_decimals: u8) -> String {
        crate::route_quote::Amount::new(quantity, self.decimals, self.ticker.clone())
            .figure_capped(max_decimals)
    }
}

/// A Splash royalty pool.
#[derive(Clone, Debug)]
pub struct SplashPoolView {
    pub utxo_ref: String,
    /// `poolX` and `poolY`, in the datum's own order — direction is stated
    /// against these, never against a ticker.
    pub x: RoyaltySide,
    pub y: RoyaltySide,
    pub fee_num: u64,
    pub treasury_fee: u64,
    pub royalty_fee: u64,
    /// The denominator the validator's fee arithmetic uses.
    pub fee_den: u64,
}

impl SplashPoolView {
    /// `feeNum − treasuryFee − royaltyFee` — the factor the curve uses.
    pub fn swap_fee_num(&self) -> u64 {
        self.fee_num
            .saturating_sub(self.treasury_fee)
            .saturating_sub(self.royalty_fee)
    }

    /// One of the datum's fee numerators as a percentage of an input.
    pub fn fee_pct(&self, numerator: u64) -> f64 {
        if self.fee_den == 0 {
            return 0.0;
        }
        numerator as f64 * 100.0 / self.fee_den as f64
    }

    /// The liquidity providers' share of an input, in basis points.
    pub fn lp_fee_bps(&self) -> f64 {
        if self.fee_den == 0 {
            return 0.0;
        }
        (self.fee_den.saturating_sub(self.fee_num)) as f64 * 10_000.0 / self.fee_den as f64
    }
}

/// Which venue's pool is on screen.
///
/// An enum rather than two widgets: "inspect a pool" is one job, and a
/// development surface wants to put them side by side.
#[derive(Clone, Debug)]
pub enum PoolView {
    LumpPad(Box<LumpPadPoolView>),
    SplashRoyalty(Box<SplashPoolView>),
}

/// Display options.
///
/// Sizes are [`TextSize`] steps, not point sizes — the ramp belongs to the
/// theme, and a literal here is a size no theme switch and no density setting
/// can reach. They resolve at render time, the first moment a widget has a
/// `Ui` to ask.
#[derive(Clone, Debug)]
pub struct PoolInspectorConfig {
    /// Figures and their labels.
    pub body: TextSize,
    /// Qualifying notes and the legend.
    pub detail: TextSize,
    /// The venue / pair heading.
    pub title: TextSize,
    /// Draw the composition bar. Off gives figures only.
    pub show_composition: bool,
    /// Most decimal places any figure here shows.
    ///
    /// This is an at-a-glance view of a pool's shape, not a receipt: two
    /// places carry the magnitude and the rest is noise to scan past. A
    /// zero-decimal asset is unaffected — the cap only ever removes places
    /// the asset actually has.
    pub max_decimals: u8,
}

impl Default for PoolInspectorConfig {
    fn default() -> Self {
        Self {
            body: TextSize::Md,
            detail: TextSize::Base,
            title: TextSize::Lg,
            show_composition: true,
            max_decimals: 2,
        }
    }
}

/// The config's steps resolved against the live theme, once per frame.
struct Sizes {
    body: f32,
    detail: f32,
    title: f32,
}

impl Sizes {
    fn resolve(ui: &Ui, config: &PoolInspectorConfig) -> Self {
        Self {
            body: ui.text_size(config.body),
            detail: ui.text_size(config.detail),
            title: ui.text_size(config.title),
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render one pool.
pub fn show(ui: &mut Ui, pool: &PoolView, config: &PoolInspectorConfig) {
    egui::Frame::new()
        .fill(ui.tokens().color.bg_secondary)
        .corner_radius(ui.tokens().corner(Radius::Md))
        .inner_margin(ui.tokens().margin(Space::Xl))
        .stroke(ui.tokens().geometry.border(ui.tokens().color.border))
        .show(ui, |ui| {
            dense(ui);
            let sizes = Sizes::resolve(ui, config);
            match pool {
                PoolView::LumpPad(view) => lumppad(ui, view, &sizes, config),
                PoolView::SplashRoyalty(view) => splash(ui, view, &sizes, config),
            }
        });
}

/// Tighten a region of read-only rows.
///
/// `interact_size.y` is a FLOOR on allocated height and sets the row height of
/// every `horizontal` — so at a compact breakpoint, where it rises to a 44px
/// touch target, a stack of figures nobody can click inherits it and the whole
/// panel becomes a ladder of air. It has to be set on the REGION: by the time
/// a row is being laid out its height is already decided.
fn dense(ui: &mut Ui) {
    ui.spacing_mut().interact_size = egui::Vec2::ZERO;
    ui.spacing_mut().item_spacing.y = ui.tokens().space(Space::Sm);
}

fn lumppad(ui: &mut Ui, view: &LumpPadPoolView, sizes: &Sizes, config: &PoolInspectorConfig) {
    let show_composition = config.show_composition;
    title(
        ui,
        &format!("LumpPad · {}", view.ticker),
        &view.utxo_ref,
        sizes,
    );

    // The invariant first. If it does not hold, nothing below can be trusted,
    // so say that before showing any of it.
    if !view.invariant_holds() {
        warning(
            ui,
            &format!(
                "INVARIANT BROKEN — UTxO holds {} {} but the datum says {} (R {} + A {} + B {})",
                group(view.quote_in_utxo),
                view.quote_ticker,
                group(view.expected_quote()),
                group(view.reserve),
                group(view.platform_bucket),
                group(view.creator_bucket),
            ),
            sizes,
        );
    }
    if !view.schedule_matches_registry {
        warning(
            ui,
            "Fee schedule differs from the registry's record — this is not a launch we have seen",
            sizes,
        );
    }

    if show_composition {
        ui.gap(Space::Sm);
        // Where the pool's quote asset actually sits. The virtual reserve is
        // shown alongside because it prices every trade and is held by
        // nobody — the single most counter-intuitive thing about this venue.
        composition_bar(
            ui,
            &[
                Band {
                    label: "curve reserve".into(),
                    value: view.reserve,
                    colour: ui.tokens().color.accent_green,
                },
                Band {
                    label: "platform".into(),
                    value: view.platform_bucket,
                    colour: ui.tokens().color.accent_yellow,
                },
                Band {
                    label: "creator".into(),
                    value: view.creator_bucket,
                    colour: ui.tokens().color.accent_orange,
                },
            ],
            sizes,
        );
    }

    ui.gap(Space::Md);
    row(ui, "Reserve (R)", &group(view.reserve), sizes);
    row(ui, "Virtual reserve", &group(view.virtual_reserve), sizes);
    note(ui, "prices every trade; held by nobody", sizes);
    row(ui, "Tokens in pool (T)", &group(view.tokens), sizes);
    row(ui, "Spot", &format!("{:.8}", view.spot_price()), sizes);
    note(
        ui,
        &format!("{} per {}", view.quote_ticker, view.ticker),
        sizes,
    );
    row(
        ui,
        "Supply in pool",
        &format!("{:.2}%", view.in_pool_bps as f64 / 100.0),
        sizes,
    );
    ui.gap(Space::Sm);
    row(
        ui,
        "Claimable now",
        &format!(
            "{} {}",
            group(view.platform_bucket + view.creator_bucket),
            view.quote_ticker
        ),
        sizes,
    );
    note(
        ui,
        &format!(
            "platform {} + creator {}",
            group(view.platform_bucket),
            group(view.creator_bucket)
        ),
        sizes,
    );
}

fn splash(ui: &mut Ui, view: &SplashPoolView, sizes: &Sizes, config: &PoolInspectorConfig) {
    let show_composition = config.show_composition;
    let dp = config.max_decimals;
    title(
        ui,
        &format!("Splash royalty · {}/{}", view.x.ticker, view.y.ticker),
        &view.utxo_ref,
        sizes,
    );

    if show_composition {
        for side in [&view.x, &view.y] {
            ui.gap(Space::Sm);
            ui.label(
                RichText::new(&side.ticker)
                    .color(ui.tokens().color.text_secondary)
                    .size(sizes.body),
            );
            // Effective reserve against what is merely SITTING there. The gap
            // is the whole point of this view.
            composition_bar(
                ui,
                &[
                    Band {
                        label: "effective".into(),
                        value: side.effective(),
                        colour: ui.tokens().color.accent_green,
                    },
                    Band {
                        label: "treasury".into(),
                        value: side.treasury,
                        colour: ui.tokens().color.accent_yellow,
                    },
                    Band {
                        label: "royalty".into(),
                        value: side.royalty,
                        colour: ui.tokens().color.accent_magenta,
                    },
                ],
                sizes,
            );
        }
    }

    ui.gap(Space::Md);
    for side in [&view.x, &view.y] {
        row(
            ui,
            &format!("{} balance", side.ticker),
            &side.figure(side.balance, dp),
            sizes,
        );
        row(
            ui,
            &format!("{} effective", side.ticker),
            &side.figure(side.effective(), dp),
            sizes,
        );
        note(
            ui,
            &format!(
                // ASCII hyphen, not U+2212: no minus glyph in the app's fonts.
                "-{} treasury, -{} royalty",
                side.figure(side.treasury, dp),
                side.figure(side.royalty, dp)
            ),
            sizes,
        );
    }
    ui.gap(Space::Sm);
    row(
        ui,
        "Curve factor",
        &format!("{} / {}", group(view.swap_fee_num()), group(view.fee_den)),
        sizes,
    );
    // All three as percentages of the SAME denominator. Printing the LP share
    // as a percentage beside two bare numerators invites reading 50 as 50 of
    // something; it is 50/100,000, three orders of magnitude smaller than the
    // 0.90% it sits next to.
    note(
        ui,
        &format!(
            "LP {:.2}%, treasury {:.3}%, royalty {:.3}%",
            view.lp_fee_bps() / 100.0,
            view.fee_pct(view.treasury_fee),
            view.fee_pct(view.royalty_fee)
        ),
        sizes,
    );
}

// ============================================================================
// Parts
// ============================================================================

struct Band {
    label: String,
    value: u64,
    colour: Color32,
}

/// A legend swatch's radius, as a fraction of its label's point size, so the
/// legend scales with the type ramp.
const SWATCH_RADIUS: f32 = 0.25;

/// A stacked proportional bar with a legend beneath.
///
/// Magnitude as bar width rather than a number the reader has to compare by
/// eye — the accrued bands are often a rounding error beside the reserve, and
/// that IS the finding.
fn composition_bar(ui: &mut Ui, bands: &[Band], sizes: &Sizes) {
    let total: u64 = bands.iter().map(|b| b.value).sum();
    // The bar's weight tracks the TYPE RAMP, not a pixel count: it sits in a
    // column of figures, and at a larger theme a fixed 10px bar would read as
    // a hairline beside them.
    let height = sizes.detail;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let radius = ui.tokens().corner(Radius::Xs);
        ui.painter()
            .rect_filled(rect, radius, ui.tokens().color.bg_primary);
        if total > 0 {
            let mut x = rect.min.x;
            for band in bands {
                let w = width * (band.value as f32 / total as f32);
                if w <= 0.0 {
                    continue;
                }
                let segment =
                    egui::Rect::from_min_size(egui::pos2(x, rect.min.y), egui::vec2(w, height));
                ui.painter().rect_filled(segment, radius, band.colour);
                x += w;
            }
        }
    }

    ui.gap(Space::Xs);
    // Every item in the legend is allocated at the LABEL's line height, so the
    // swatch centres against its text instead of riding up against the
    // ascenders — see the note in `tx_watch::stage_marks`.
    let row = crate::theme::line_height(ui, sizes.detail);
    ui.horizontal_wrapped(|ui| {
        for band in bands {
            let share = if total > 0 {
                band.value as f64 * 100.0 / total as f64
            } else {
                0.0
            };
            let (dot, _) =
                ui.allocate_exact_size(egui::vec2(sizes.detail, row), egui::Sense::hover());
            if ui.is_rect_visible(dot) {
                ui.painter()
                    .circle_filled(dot.center(), sizes.detail * SWATCH_RADIUS, band.colour);
            }
            ui.label(
                RichText::new(format!("{} {:.2}%", band.label, share))
                    .color(ui.tokens().color.text_muted)
                    .size(sizes.detail),
            );
        }
    });
}

fn title(ui: &mut Ui, name: &str, utxo_ref: &str, sizes: &Sizes) {
    ui.label(
        RichText::new(name)
            .color(ui.tokens().color.text_primary)
            .strong()
            .size(sizes.title),
    );
    ui.label(
        RichText::new(elide(utxo_ref))
            .color(ui.tokens().color.text_muted)
            .monospace()
            .size(sizes.detail),
    );
}

fn warning(ui: &mut Ui, message: &str, sizes: &Sizes) {
    ui.gap(Space::Sm);
    egui::Frame::new()
        .fill(ui.tokens().color.bg_highlight)
        .corner_radius(ui.tokens().corner(Radius::Sm))
        .inner_margin(ui.tokens().margin(Space::Md))
        .show(ui, |ui| {
            ui.label(
                RichText::new(message)
                    .color(ui.tokens().color.error)
                    .size(sizes.body),
            );
        });
}

/// A label and ONE figure. The figure is the point, so the label truncates
/// under pressure rather than the two drawing over each other — which is what
/// a `horizontal` does when its contents do not fit.
fn row(ui: &mut Ui, label: &str, value: &str, sizes: &Sizes) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(label)
                    .color(ui.tokens().color.text_muted)
                    .size(sizes.body),
            )
            .truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .color(ui.tokens().color.text_secondary)
                    .size(sizes.body),
            );
        });
    });
}

/// The qualification a figure needs, on its own line.
///
/// Explanations do NOT belong in the value column: a parenthetical there
/// widens the row until it collides with its own label, and egui gives no
/// warning — it just draws one over the other.
fn note(ui: &mut Ui, text: &str, sizes: &Sizes) {
    ui.label(
        RichText::new(text)
            .color(ui.tokens().color.text_muted)
            .size(sizes.detail),
    );
}

/// `0aca3489…a7b3ae6f#0` — enough to recognise, short enough to sit under a
/// title. Full identifiers belong in [`crate::id_pill`], which has the copy
/// affordance.
fn elide(utxo_ref: &str) -> String {
    let (hash, index) = match utxo_ref.split_once('#') {
        Some(pair) => pair,
        None => (utxo_ref, ""),
    };
    if hash.len() <= 20 {
        return utxo_ref.to_string();
    }
    format!("{}…{}#{index}", &hash[..8], &hash[hash.len() - 8..])
}

fn group(amount: u64) -> String {
    crate::route_quote::Amount::new(amount, 0, "").figure()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swole() -> LumpPadPoolView {
        LumpPadPoolView {
            utxo_ref: "0aca3489a43d669b1aef62717933f141b38a940b9aa4923e180512c7a7b3ae6f#0".into(),
            ticker: "SWOLE".into(),
            reserve: 3_243_770,
            tokens: 756_359_192,
            platform_bucket: 167_862,
            creator_bucket: 75_732,
            virtual_reserve: 10_000_000,
            quote_in_utxo: 3_487_364,
            quote_ticker: "LUMP".into(),
            spot_price_num: 13_243_770,
            spot_price_den: 756_359_192,
            in_pool_bps: 7_563,
            schedule_matches_registry: true,
        }
    }

    /// The SWOLE pool's real mainnet state satisfies `LUMP == R + A + B`.
    #[test]
    fn the_invariant_holds_for_a_real_pool() {
        let pool = swole();
        assert_eq!(pool.expected_quote(), 3_487_364);
        assert!(pool.invariant_holds());
    }

    /// A datum that decoded wrongly shows up as a broken invariant, which is
    /// the whole reason the widget checks it.
    #[test]
    fn a_mis_decoded_datum_breaks_the_invariant() {
        let mut pool = swole();
        pool.reserve += 1;
        assert!(!pool.invariant_holds());
    }

    /// The curve trades against the balance MINUS what is already owed.
    #[test]
    fn effective_reserves_exclude_accrued_treasury_and_royalty() {
        let side = RoyaltySide {
            ticker: "ADA".into(),
            decimals: 6,
            balance: 29_549_431_005,
            treasury: 148_513_382,
            royalty: 39_027_415,
        };
        assert_eq!(side.effective(), 29_361_890_208);
        assert!(side.effective() < side.balance, "the gap is the point");
    }

    /// Lovelace is not ADA.
    ///
    /// The live LUMP/ADA pool holds ₳29,549.43. Rendered without its scale it
    /// reads 29,549,431,005 ADA — more than Cardano's entire supply, and the
    /// kind of wrong that looks authoritative because it is grouped neatly.
    #[test]
    fn a_side_renders_at_its_own_scale() {
        let ada = RoyaltySide {
            ticker: "ADA".into(),
            decimals: 6,
            balance: 29_549_431_005,
            treasury: 93_770_398,
            royalty: 93_770_399,
        };
        // Uncapped, the full scale.
        assert_eq!(ada.figure(ada.balance, 6), "29,549.431005");
        assert_eq!(ada.figure(ada.treasury, 6), "93.770398");
        // As the panel actually draws it — two places carry the magnitude.
        assert_eq!(ada.figure(ada.balance, 2), "29,549.43");
        assert_eq!(ada.figure(ada.treasury, 2), "93.77");

        // LUMP has 0 decimals in the token registry, so its raw count IS the
        // figure — the same code path must not scale it.
        let lump = RoyaltySide {
            ticker: "LUMP".into(),
            decimals: 0,
            balance: 196_800_921,
            treasury: 698_241,
            royalty: 698_242,
        };
        // The cap only ever removes places the asset HAS, so a zero-decimal
        // asset reads identically at any cap.
        assert_eq!(lump.figure(lump.balance, 2), "196,800,921");
        assert_eq!(lump.figure(lump.balance, 6), "196,800,921");
    }

    /// Every fee in the note is a share of the SAME denominator.
    #[test]
    fn fee_shares_are_all_percentages() {
        let pool = SplashPoolView {
            utxo_ref: "d92955b6…#1".into(),
            x: side("ADA", 6),
            y: side("LUMP", 0),
            fee_num: 99_100,
            treasury_fee: 50,
            royalty_fee: 50,
            fee_den: 100_000,
        };
        assert_eq!(pool.lp_fee_bps() / 100.0, 0.9);
        assert_eq!(pool.fee_pct(pool.treasury_fee), 0.05);
        assert_eq!(pool.fee_pct(pool.royalty_fee), 0.05);
    }

    fn side(ticker: &str, decimals: u8) -> RoyaltySide {
        RoyaltySide {
            ticker: ticker.into(),
            decimals,
            balance: 1,
            treasury: 0,
            royalty: 0,
        }
    }

    /// The LUMP/ADA pool's own fee schedule: 0.9% LP, 0.05% each to treasury
    /// and royalty, leaving 99,000/100,000 on the curve.
    #[test]
    fn the_curve_factor_is_fee_num_less_both_counters() {
        let pool = SplashPoolView {
            utxo_ref: "d92955b6…#1".into(),
            x: side("ADA", 6),
            y: side("LUMP", 0),
            fee_num: 99_100,
            treasury_fee: 50,
            royalty_fee: 50,
            fee_den: 100_000,
        };
        assert_eq!(pool.swap_fee_num(), 99_000);
        assert!((pool.lp_fee_bps() - 90.0).abs() < 1e-9, "0.9% is 90 bps");
    }

    #[test]
    fn a_utxo_ref_elides_to_something_recognisable() {
        assert_eq!(
            elide("0aca3489a43d669b1aef62717933f141b38a940b9aa4923e180512c7a7b3ae6f#0"),
            "0aca3489…a7b3ae6f#0"
        );
        // Short enough to show whole.
        assert_eq!(elide("abc#1"), "abc#1");
    }
}
