//! StatStrip — windowed summary "stat cards", laid out as uniform tiles that
//! fill their container and wrap into even rows.
//!
//! A dashboard idiom: the same metric summarised across a few time windows
//! (24h / 7d / 30d), each as a compact card. Every card draws its own frame:
//! the window label sits in a recessed pill in the top-right corner, the
//! headline reads large below it (with an optional trend delta beside it, or
//! beneath it when the pair does not fit), then optional marks stack underneath
//! — an activity sparkline, a price-range bar on a strip-wide shared axis, and
//! a caption pinned to the card's floor. Windows with no marks fall back to a
//! shared empty note (e.g. "no fills") while still showing their zeroed
//! headline, so the strip keeps a stable shape.
//!
//! ## Measure, then lay out
//!
//! **Every card in a strip is the same width and the same height.** That is the
//! widget's job, not the caller's, and it is the whole reason the layout is a
//! two-pass measurement rather than a wrapping row:
//!
//! - A card is an `egui::Frame`, and a frame grows to its content. So the old
//!   `card_width` was a floor, not a width, and the headline row — headline plus
//!   trend delta — sized each card to whatever string it happened to hold. Four
//!   windows came out four different widths, ordered by trend length.
//! - Overflow does not stay local: `Region::expand_to_include_rect` unions an
//!   over-wide child into the parent's `max_rect`, so a long trend string spread
//!   outward into the surrounding layout.
//!
//! So the strip measures the widest headline row, picks a column count that fits
//! at [`StatStrip::min_card_width`], divides the row evenly, and only then
//! measures height — because whether a trend fits beside its headline depends on
//! the width just chosen. Both consumers of this widget had grown their own
//! call-site workaround for the missing behaviour before it lived here.
//!
//! Purely presentational — the caller does the folding/aggregation and hands
//! in already-formatted headlines/captions plus raw series/spreads, so the
//! widget stays currency- and domain-agnostic.

use egui::{Color32, CornerRadius, FontId, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::{SparkHoverStyle, Sparkline, Trend, theme};

/// The sizes and spacings a card paints with.
///
/// Named and shared because both measuring passes — `natural_width` and
/// `stack_height` — have to measure with exactly the same ones. The same rule
/// `MetricCard` states for its own constants: a measurement that drifts from the
/// painting produces a strip that is confidently the wrong size, which is worse
/// than not measuring at all.
const HEADLINE_SIZE: f32 = 24.0;
const DETAIL_SIZE: f32 = 12.0;
const TREND_SIZE: f32 = 11.0;
/// Width `draw_trend` spends before its label: an 8pt gap from the headline
/// plus a 9pt arrow. It zeroes the row's `item_spacing` first, so this is the
/// whole gap — the row's own spacing does not also apply.
const TREND_ARROW: f32 = 17.0;
const SPARK_HEIGHT: f32 = 22.0;
const RANGE_HEIGHT: f32 = 9.0;
const MARGIN_X: f32 = 12.0;
/// Taller than the sides: the top margin reserves room for the window pill,
/// which is painted absolutely into the card's top-right corner.
const MARGIN_TOP: f32 = 26.0;
const MARGIN_BOTTOM: f32 = 12.0;

/// A low / median / high price triple for a window's range bar. Positions are
/// mapped onto a domain shared across the whole strip, so the bands are
/// directly comparable window-to-window.
#[derive(Clone, Copy)]
pub struct StatRange {
    /// Lowest fill in the window.
    pub low: f64,
    /// Median fill — drawn as a tick within the band.
    pub mid: f64,
    /// Highest fill in the window.
    pub high: f64,
}

/// One window in the strip: a short time-window label, a headline metric, and
/// optional enrichments — a trend delta, a per-window activity sparkline, a
/// price-range bar, and a caption line.
///
/// When `detail` is `None` and no enrichments are set, the card renders the
/// strip's empty note in place of the caption — the window still shows its
/// headline (typically a zero count), so an empty window reads as "nothing
/// happened here" rather than vanishing.
pub struct StatWindow {
    /// Short window label at the top of the card (e.g. "24h").
    pub label: String,
    /// Headline metric — the large value (e.g. "4 sales").
    pub headline: String,
    /// Caption line beneath the marks (e.g. "vol 66"). `None` marks an empty
    /// window; the strip's empty note is shown instead.
    pub detail: Option<String>,
    /// Direction + label of the change vs the previous equal-length window
    /// (e.g. `(Trend::Up, "+18%")`), drawn beside the headline.
    pub trend: Option<(Trend, String)>,
    /// Per-window activity series (e.g. fills per day) for the sparkline.
    pub spark: Option<Vec<f64>>,
    /// Realized price spread for the range bar.
    pub range: Option<StatRange>,
}

impl StatWindow {
    /// A window with a label and headline, and no enrichments yet.
    pub fn new(label: impl Into<String>, headline: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            headline: headline.into(),
            detail: None,
            trend: None,
            spark: None,
            range: None,
        }
    }

    /// Attach the caption line shown beneath the marks.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Attach a trend delta (direction + label) shown beside the headline.
    pub fn trend(mut self, dir: Trend, label: impl Into<String>) -> Self {
        self.trend = Some((dir, label.into()));
        self
    }

    /// Attach a per-window activity series for the sparkline.
    pub fn spark(mut self, series: impl Into<Vec<f64>>) -> Self {
        self.spark = Some(series.into());
        self
    }

    /// Attach a low/median/high price spread for the range bar.
    pub fn range(mut self, low: f64, mid: f64, high: f64) -> Self {
        self.range = Some(StatRange { low, mid, high });
        self
    }
}

/// Result of [`StatStrip::show`] — reports the hovered sparkline bucket so the
/// caller can render a richer tooltip than the built-in crosshair.
pub struct StatStripResponse {
    spark_hover: Option<(usize, usize, egui::Response)>,
}

impl StatStripResponse {
    /// The `(window index, bucket index)` under the pointer, if a sparkline is
    /// hovered.
    pub fn hovered_bucket(&self) -> Option<(usize, usize)> {
        self.spark_hover.as_ref().map(|(w, b, _)| (*w, *b))
    }

    /// Attach a tooltip to the hovered sparkline bucket. The closure receives
    /// the window index and bucket index; nothing renders when no sparkline is
    /// hovered.
    pub fn spark_tooltip(self, add: impl FnOnce(&mut Ui, usize, usize)) {
        if let Some((wi, bi, resp)) = self.spark_hover {
            resp.on_hover_ui(|ui| add(ui, wi, bi));
        }
    }
}

/// Which sparkline bucket the pointer is over, matching the sparkline's own
/// `plot_rect = rect.shrink(4.0)` mapping so it lines up with the crosshair.
fn hovered_bucket(resp: &egui::Response, n: usize) -> Option<usize> {
    if n < 2 {
        return None;
    }
    let pos = resp.hover_pos()?;
    let plot = resp.rect.shrink(4.0);
    let rel = ((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
    Some((rel * (n - 1) as f32).round() as usize)
}

/// A horizontal strip of windowed stat cards.
pub struct StatStrip<'a> {
    windows: &'a [StatWindow],
    empty_note: &'a str,
    card_width: f32,
    value_color: Color32,
    label_bg: Color32,
}

impl<'a> StatStrip<'a> {
    /// A strip over the given windows, with default styling.
    pub fn new(windows: &'a [StatWindow]) -> Self {
        Self {
            windows,
            empty_note: "no data",
            card_width: 190.0,
            value_color: theme::TEXT_PRIMARY,
            // Recessed (darker than the card) so the window pill reads as an
            // inset tag rather than more card surface.
            label_bg: theme::BG_PRIMARY,
        }
    }

    /// Text shown in place of the detail line for empty windows
    /// (`detail == None`). Defaults to "no data".
    pub fn empty_note(mut self, note: &'a str) -> Self {
        self.empty_note = note;
        self
    }

    /// FLOOR for the shared card width, not the width itself. Defaults to 190.
    ///
    /// The strip decides the actual width: it measures every card, takes the
    /// widest, and gives them all that — then fills the row, so a strip always
    /// spans its container rather than trailing off into dead space. This
    /// number only stops a strip of very short values collapsing into a huddle
    /// of tiny cards, and sets how many columns fit before the strip wraps.
    ///
    /// Renamed from a "fixed width" that never was one: the value was a floor
    /// on the frame, and the frame grew to its content regardless. See
    /// [`Self::show`].
    pub fn min_card_width(mut self, width: f32) -> Self {
        self.card_width = width;
        self
    }

    /// Accent color for each headline value. Defaults to the primary text color.
    pub fn value_color(mut self, color: Color32) -> Self {
        self.value_color = color;
        self
    }

    /// Background of the top-right window pill. Defaults to the recessed
    /// primary background.
    pub fn label_bg(mut self, color: Color32) -> Self {
        self.label_bg = color;
        self
    }

    /// Render the strip as a horizontal row of cards. The returned
    /// [`StatStripResponse`] reports which sparkline bucket (if any) is hovered,
    /// so the caller can attach a richer tooltip.
    pub fn show(self, ui: &mut Ui) -> StatStripResponse {
        // Shared price domain so range bars are comparable window-to-window.
        let domain =
            self.windows
                .iter()
                .filter_map(|w| w.range)
                .fold(None, |acc: Option<(f64, f64)>, r| {
                    let (lo, hi) = acc.unwrap_or((r.low, r.high));
                    Some((lo.min(r.low), hi.max(r.high)))
                });

        if self.windows.is_empty() {
            return StatStripResponse { spark_hover: None };
        }

        // MEASURE, THEN LAY OUT — the discipline `MetricRow` already documents,
        // arrived at here the hard way.
        //
        // This used to be `left_to_right(TOP).with_main_wrap(true)` over cards
        // that each asked for `card_width`. Neither half worked:
        //
        // - `card_width` was a FLOOR, not a width. Each card is an `egui::Frame`
        //   and a frame grows to its content, so the headline row — headline
        //   plus trend delta, side by side — pushed the card as wide as its
        //   longest string. A strip of four windows came out four different
        //   widths, ordered by how long each trend text happened to be:
        //   "+0 ₳ / -0 ₳" gave a 320pt card and "+804523 ₳ / -835199 ₳" a 525pt
        //   one. Worse, an overflowing child is unioned into the parent's
        //   `max_rect` by `Region::expand_to_include_rect`, so the overflow
        //   spread outward rather than clipping.
        // - The wrap then broke the row into ragged groups — three cards and a
        //   lonely fourth — because it packs greedily at whatever widths it was
        //   handed.
        //
        // So: measure every card, take the widest and the tallest, choose a
        // column count that fits, and give every card the same box.
        let natural_w = self.natural_width(ui);
        let gap = ui.spacing().item_spacing.x;
        let avail = ui.available_width();

        // How many columns fit at the FLOOR width — then the cards share the
        // row equally, so the strip spans its container instead of leaving a
        // gutter of dead space on the right. Never more columns than windows,
        // never fewer than one.
        let cols = (((avail + gap) / (self.card_width + gap)).floor() as usize)
            .clamp(1, self.windows.len());
        let w = ((avail - gap * (cols - 1) as f32) / cols as f32)
            // 1pt of slack: `available_width` is measured before the row lays
            // out and does not know about a scrollbar that may yet appear.
            - 1.0;
        // A measured card wider than its share is not clipped — it keeps its
        // own width and the row overflows visibly, which is a local failure
        // rather than a silently truncated number.
        let w = w.max(natural_w.min(self.card_width));
        // HEIGHT IS MEASURED SECOND, because it DEPENDS on the width: a trend
        // that does not fit beside its headline takes a line of its own, and
        // whether it fits is not knowable until the column count is settled.
        // Measuring both in one pass is what left the narrow strip with two
        // card heights in the same row.
        let h = self.stack_height(ui, w);

        let mut spark_hover = None;
        // `Align::TOP` is load-bearing: `ui.horizontal` centres by default, so
        // cards of differing height sit at differing tops and the row looks
        // staggered even once the widths agree.
        ui.vertical(|ui| {
            for (row, chunk) in self.windows.chunks(cols).enumerate() {
                if row > 0 {
                    ui.add_space(ui.spacing().item_spacing.y);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    for (i, window) in chunk.iter().enumerate() {
                        let wi = row * cols + i;
                        if let Some((resp, bucket)) = self.card(ui, window, domain, w, h) {
                            spark_hover = Some((wi, bucket, resp));
                        }
                    }
                });
            }
        });
        StatStripResponse { spark_hover }
    }

    /// Width of one line of text at a given size, measured the way the card
    /// paints it. A free-standing helper because both measuring passes and the
    /// card itself need the same answer.
    fn text_w(ui: &Ui, s: &str, size: f32) -> f32 {
        ui.painter()
            .layout_no_wrap(s.to_owned(), FontId::proportional(size), Color32::WHITE)
            .size()
            .x
    }

    /// Does this window's trend fit beside its headline in a card `w` wide?
    ///
    /// The one place the question is asked, so [`Self::stack_height`] and
    /// [`Self::card`] cannot disagree about it — if they did, a card would
    /// reserve room for one layout and paint another.
    fn trend_fits(ui: &Ui, w: &StatWindow, card_w: f32) -> bool {
        let Some((_, label)) = &w.trend else {
            return true;
        };
        let row = Self::text_w(ui, &w.headline, HEADLINE_SIZE)
            + TREND_ARROW
            + Self::text_w(ui, label, TREND_SIZE);
        row <= card_w - MARGIN_X * 2.0
    }

    /// The widest headline row across the windows, floored at the caller's
    /// minimum. This is what decides how many columns fit.
    ///
    /// Measured with exactly the font sizes and spacings [`Self::card`] paints
    /// with — a measurement that drifts from the painting is worse than none,
    /// because it produces a strip that is confidently the wrong size.
    fn natural_width(&self, ui: &Ui) -> f32 {
        let mut width = self.card_width;
        for w in self.windows {
            // Headline and trend sit side by side — see `TREND_ARROW` for why
            // the row's own item spacing is not part of the gap.
            let mut row = Self::text_w(ui, &w.headline, HEADLINE_SIZE);
            if let Some((_, label)) = &w.trend {
                row += TREND_ARROW + Self::text_w(ui, label, TREND_SIZE);
            }
            width = width.max(row + MARGIN_X * 2.0);
        }
        width
    }

    /// The tallest card in the strip, at a settled card width.
    ///
    /// Every card is drawn at this height so the row keeps one top and one
    /// bottom edge — windows carry different marks (a quiet window has no
    /// sparkline), and letting each size itself is what made the row look
    /// staggered even once the widths agreed.
    fn stack_height(&self, ui: &Ui, card_w: f32) -> f32 {
        let mut height: f32 = 0.0;
        for w in self.windows {
            // The vertical stack, in the order `card` draws it.
            let mut h = MARGIN_TOP + HEADLINE_SIZE * 1.2;
            if !Self::trend_fits(ui, w, card_w) {
                h += TREND_SIZE * 1.6;
            }
            if w.spark.as_ref().is_some_and(|s| s.len() >= 2) {
                h += 6.0 + SPARK_HEIGHT;
            }
            if w.range.is_some() {
                h += 6.0 + RANGE_HEIGHT;
            }
            h += 4.0 + DETAIL_SIZE * 1.4 + MARGIN_BOTTOM;
            height = height.max(h);
        }
        height
    }

    /// Render one card. Returns the sparkline's `(response, bucket)` when the
    /// pointer is over its sparkline, so `show` can surface the hover.
    fn card(
        &self,
        ui: &mut Ui,
        w: &StatWindow,
        domain: Option<(f64, f64)>,
        card_w: f32,
        card_h: f32,
    ) -> Option<(egui::Response, usize)> {
        let mut spark_hover = None;
        // Top margin reserves room for the corner pill (painted absolutely
        // below) so the headline always clears it, whatever the card width.
        let frame = egui::Frame::NONE
            .fill(theme::BG_HIGHLIGHT)
            .corner_radius(6.0)
            .inner_margin(Margin {
                left: MARGIN_X as i8,
                right: MARGIN_X as i8,
                top: MARGIN_TOP as i8,
                bottom: MARGIN_BOTTOM as i8,
            })
            .stroke(egui::Stroke::new(1.0_f32, theme::BORDER));

        let inner_w = card_w - MARGIN_X * 2.0;
        ui.allocate_ui(Vec2::new(card_w, card_h), |ui| {
            let card = frame.show(ui, |ui| {
                ui.vertical(|ui| {
                    // BOTH bounds, and a height floor. A minimum alone lets the
                    // content grow the card — which is the bug this widget had
                    // — and a maximum alone lets it shrink. The height floor is
                    // what stops a window with no sparkline sitting shorter
                    // than its neighbours and breaking the row's bottom edge.
                    ui.set_min_width(inner_w);
                    ui.set_max_width(inner_w);
                    ui.set_min_height(card_h - MARGIN_TOP - MARGIN_BOTTOM);

                    // Headline, with the trend delta beside it — or beneath it
                    // when the pair does not fit.
                    //
                    // The decision is made by `trend_fits`, NOT by letting
                    // `horizontal_wrapped` sort it out. egui wraps a wrapped row
                    // at word boundaries INSIDE the label, which broke
                    // "+373 ₳ / -876 ₳" across two lines with a lone "₳"
                    // underneath. The trend is one quantity and moves as one.
                    let fits = Self::trend_fits(ui, w, card_w);
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&w.headline)
                                    .color(self.value_color)
                                    .size(HEADLINE_SIZE)
                                    .strong(),
                            );
                            if let (true, Some((dir, label))) = (fits, &w.trend) {
                                draw_trend(ui, *dir, label);
                            }
                        });
                        if let (false, Some((dir, label))) = (fits, &w.trend) {
                            ui.horizontal(|ui| {
                                draw_trend(ui, *dir, label);
                            });
                        }
                    });

                    // Activity sparkline. Crosshair-only hover — the caller
                    // owns the tooltip via `StatStripResponse`.
                    if let Some(series) = &w.spark
                        && series.len() >= 2
                    {
                        ui.add_space(6.0);
                        let resp = Sparkline::new(series)
                            .height(SPARK_HEIGHT)
                            .line_width(1.5)
                            .line_color(self.value_color)
                            .fill(tint(self.value_color, 30))
                            .show_endpoint(false)
                            .bg_color(theme::BG_HIGHLIGHT)
                            .hover_style(SparkHoverStyle::CrosshairOnly)
                            .show(ui);
                        if let Some(bucket) = hovered_bucket(&resp, series.len()) {
                            spark_hover = Some((resp, bucket));
                        }
                    }

                    // Price-range bar on the strip-wide shared domain.
                    if let (Some(r), Some((lo, hi))) = (w.range, domain) {
                        ui.add_space(6.0);
                        self.draw_range_bar(ui, r, lo, hi);
                    }

                    // Caption ON THE CARD'S FLOOR, not merely after the marks.
                    //
                    // Cards in a strip do not all carry the same marks — a
                    // window with no activity has no sparkline — so a caption
                    // that simply follows the last mark sits at a different
                    // height on every card. The row then has three "N tx" lines
                    // at three heights, which reads as misalignment even though
                    // the cards themselves are flush. Taking up the slack first
                    // gives the strip a shared bottom baseline.
                    let slack = ui.available_height() - DETAIL_SIZE * 1.4;
                    ui.add_space(slack.max(4.0));
                    let (detail, color) = match &w.detail {
                        Some(d) => (d.as_str(), theme::TEXT_SECONDARY),
                        None => (self.empty_note, theme::TEXT_MUTED),
                    };
                    ui.label(RichText::new(detail).color(color).size(DETAIL_SIZE));
                });
            });

            // Window label — recessed pill hugging the top-right corner.
            self.paint_window_pill(ui, card.response.rect, &w.label);
        });
        spark_hover
    }

    /// Draw the window's realized price spread as a floating low→high whisker
    /// with end caps and a median dot, sitting on a faint full-width track that
    /// represents the strip-wide `[dom_lo, dom_hi]` axis. Reads as a range
    /// (where the window's prices sit within the whole strip), not a fill bar.
    fn draw_range_bar(&self, ui: &mut Ui, r: StatRange, dom_lo: f64, dom_hi: f64) {
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 9.0), Sense::hover());

        // Inset the mapping so even the global min/max float off the edges,
        // reinforcing "range" over "progress to the edge".
        let span = (dom_hi - dom_lo).max(f64::EPSILON);
        let pad = 0.05_f32;
        let x_of = |v: f64| {
            let t = ((v - dom_lo) / span) as f32;
            rect.left() + (pad + t * (1.0 - 2.0 * pad)) * rect.width()
        };
        let cy = rect.center().y;
        let (x_lo, x_hi, x_mid) = (x_of(r.low), x_of(r.high), x_of(r.mid));

        let painter = ui.painter();
        // Faint full track — the shared axis extent.
        painter.hline(
            rect.x_range(),
            cy,
            Stroke::new(1.0_f32, tint(theme::TEXT_MUTED, 70)),
        );
        // This window's low→high whisker.
        painter.hline(
            egui::Rangef::new(x_lo, x_hi),
            cy,
            Stroke::new(2.0_f32, tint(self.value_color, 160)),
        );
        // End caps at low and high.
        for x in [x_lo, x_hi] {
            painter.vline(
                x,
                egui::Rangef::new(cy - 3.0, cy + 3.0),
                Stroke::new(1.5_f32, self.value_color),
            );
        }
        // Median dot.
        painter.circle_filled(egui::pos2(x_mid, cy), 2.5, self.value_color);

        resp.on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!(
                    "{:.0}\u{2013}{:.0} \u{00b7} med {:.0}",
                    r.low, r.high, r.mid
                ))
                .size(11.0),
            );
        });
    }

    /// Paint the window label as a small recessed pill inset into the card's
    /// top-right corner, on top of the card frame.
    fn paint_window_pill(&self, ui: &Ui, card_rect: Rect, label: &str) {
        let inset = 5.0;
        let pad = Vec2::new(6.0, 2.0);
        let painter = ui.painter();
        let galley = painter.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(11.0),
            theme::TEXT_MUTED,
        );
        let pill_size = galley.size() + pad * 2.0;
        let pill_rect = Rect::from_min_size(
            egui::pos2(
                card_rect.max.x - inset - pill_size.x,
                card_rect.min.y + inset,
            ),
            pill_size,
        );
        painter.rect_filled(pill_rect, CornerRadius::same(4), self.label_bg);
        painter.galley(pill_rect.min + pad, galley, theme::TEXT_SECONDARY);
    }
}

/// A translucent tint of `color` at the given alpha (0–255) — for band/fill
/// washes that sit over the card surface.
fn tint(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Draw a small trend triangle + label inline on the current row (arrow then
/// label, left to right).
fn draw_trend(ui: &mut Ui, dir: Trend, label: &str) {
    let color = match dir {
        Trend::Up => theme::SUCCESS,
        Trend::Down => theme::ERROR,
        Trend::Flat => theme::TEXT_MUTED,
    };
    // Own the spacing so the arrow hugs its label (default item spacing would
    // wedge them apart); the gap from the headline is added explicitly.
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.add_space(8.0);
    let size = 9.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let c = rect.center();
    let half = size / 2.0;
    {
        let painter = ui.painter();
        match dir {
            Trend::Up => painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x, c.y - half),
                    egui::pos2(c.x + half, c.y + half),
                    egui::pos2(c.x - half, c.y + half),
                ],
                color,
                Stroke::NONE,
            )),
            Trend::Down => painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - half, c.y - half),
                    egui::pos2(c.x + half, c.y - half),
                    egui::pos2(c.x, c.y + half),
                ],
                color,
                Stroke::NONE,
            )),
            Trend::Flat => painter.add(egui::Shape::line_segment(
                [egui::pos2(c.x - half, c.y), egui::pos2(c.x + half, c.y)],
                Stroke::new(1.5_f32, color),
            )),
        };
    }
    ui.add_space(3.0);
    ui.label(RichText::new(label).color(color).size(13.0));
}
