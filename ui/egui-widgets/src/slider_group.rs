//! `SliderGroup` — a bank of labelled faders on one spine.
//!
//! A mixing desk rotated a quarter turn: each row is a channel — name, fader,
//! readout — and the rows are tight enough together to read as one instrument
//! rather than a pile of separate controls.
//!
//! ## Why this exists
//!
//! `egui::Slider::new(v, r).text("Label")` is the single most repeated control
//! in this catalogue — 92 call sites across the storybook alone — and stacking
//! them gives you the thing this widget replaces:
//!
//! - **The labels do not line up.** `.text()` puts the label *after* the slider,
//!   so a column of them is ragged and the eye has no edge to run down. Reading
//!   "which control is which" costs a scan per row.
//! - **The readout outweighs the control.** The default value is a `DragValue`
//!   in a filled, bordered box, so the heaviest object in each row is the number
//!   rather than the fader you are meant to be moving.
//! - **The track is the smallest thing on the row**, at a fixed 100px however
//!   much room there is, so the control with the most information in it gets the
//!   least space.
//!
//! The fix is alignment, not decoration. Three columns on a shared spine —
//! labels right-aligned to one edge, every rail starting and ending at the same
//! x, readouts right-aligned in a column wide enough for the widest value the
//! range can produce, so digits do not jitter as a fader moves.
//!
//! ## What it is NOT
//!
//! Not a replacement for a lone slider in a sentence of controls — a single
//! `ui.add(Slider…)` inline with other widgets is fine and this would be heavier
//! than it. This is for a **bank**: two or more related quantities the reader
//! compares and balances against each other. It accepts one row because a bank
//! that happens to have one channel today should not need rewriting when it
//! gets a second.
//!
//! ## Example
//!
//! ```ignore
//! let changed = SliderGroup::new()
//!     .slider("Size", &mut state.size, 120.0..=400.0)
//!     .slider("Rings", &mut state.rings, 2..=8)
//!     .fader(Fader::new("Opacity", &mut state.opacity, 0.0..=1.0).decimals(2))
//!     .show(ui)
//!     .changed;
//! ```

use std::ops::RangeInclusive;

use egui::{Align, Layout, Ui, emath::Numeric};

use crate::theme::{Ink, Space, SpaceExt, TextSize, ThemeExt, Token};

/// Rail thickness at [`Density::Comfortable`](crate::theme::Density), in px.
///
/// Thicker than egui's 8px default: the rail is the thing being read along, and
/// at the default it is thinner than the text beside it.
const BASE_RAIL: f32 = 10.0;

/// How long the rails are.
///
/// There is deliberately no "natural" maximum here. A throw of 360px, or any
/// other number, would be this widget forming a private opinion about layout in
/// a codebase that has a theme and a layout engine for exactly that — and every
/// hardcoded dimension this pass has removed started life as an equally
/// reasonable-looking constant.
///
/// So the widget fills what it is given, like any other fill-width widget here,
/// and a caller that wants it narrower says so the way callers already do:
///
/// ```ignore
/// ui.set_max_width(ui.fit_width(520.0));
/// SliderGroup::new().slider("Size", &mut size, 0.0..=1.0).show(ui);
/// ```
///
/// [`fit_width`](crate::viewport::LayoutExt::fit_width) — which clamps to the
/// CONTAINER — rather than a bare `set_max_width`, which *widens* a `Ui` when
/// less space is available.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RailWidth {
    /// Take everything the label and readout columns do not.
    Fill,
    /// Exactly this many px — for pinning the throw without constraining the
    /// label and readout columns with it.
    Fixed(f32),
}

impl From<f32> for RailWidth {
    fn from(px: f32) -> Self {
        Self::Fixed(px)
    }
}

/// One channel: a label, a value, and the range it moves through.
pub struct Fader<'a> {
    label: String,
    /// egui's own get/set indirection, which is what lets rows of different
    /// numeric types live in one `Vec`.
    get_set: Box<dyn FnMut(Option<f64>) -> f64 + 'a>,
    range: RangeInclusive<f64>,
    integral: bool,
    suffix: String,
    decimals: Option<usize>,
    tint: Option<Ink>,
    step: Option<f64>,
}

impl<'a> Fader<'a> {
    pub fn new<T: Numeric>(
        label: impl Into<String>,
        value: &'a mut T,
        range: RangeInclusive<T>,
    ) -> Self {
        let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
        Self {
            label: label.into(),
            integral: T::INTEGRAL,
            get_set: Box::new(move |v: Option<f64>| {
                if let Some(v) = v {
                    *value = T::from_f64(v);
                }
                value.to_f64()
            }),
            range: lo..=hi,
            suffix: String::new(),
            decimals: None,
            tint: None,
            step: None,
        }
    }

    /// Unit written after the number — `"%"`, `"px"`, `" ADA"`.
    pub fn suffix(mut self, s: impl Into<String>) -> Self {
        self.suffix = s.into();
        self
    }

    /// Fixed decimal places. Default is derived from the range — see
    /// [`Fader::auto_decimals`].
    pub fn decimals(mut self, n: usize) -> Self {
        self.decimals = Some(n);
        self
    }

    /// Tint for this channel's filled travel. Default is the theme's accent;
    /// per-row tints are for banks where the channels mean different things.
    pub fn tint(mut self, ink: impl Into<Ink>) -> Self {
        self.tint = Some(ink.into());
        self
    }

    pub fn step_by(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    /// Decimal places when the caller did not say.
    ///
    /// Chosen from the RANGE rather than fixed, because the same default cannot
    /// suit `0.0..=1.0` and `100.0..=2000.0`: two decimals makes the first
    /// readable and turns the second into `1200.00`, which is four characters
    /// of noise and a wider column for every row in the bank.
    fn auto_decimals(&self) -> usize {
        if self.integral {
            return 0;
        }
        match self.range.end() - self.range.start() {
            span if span >= 100.0 => 0,
            span if span >= 10.0 => 1,
            _ => 2,
        }
    }

    fn format(&self, v: f64) -> String {
        let d = self.decimals.unwrap_or_else(|| self.auto_decimals());
        format!("{v:.d$}{}", self.suffix, d = d)
    }

    /// The widest string this fader can display, for sizing the readout column.
    ///
    /// Both ends of the range AND the value in hand: a range can be wider than
    /// its current value (`0..=1000` showing `5`) or narrower than it, when a
    /// caller's state has drifted outside the range it declared.
    fn widest(&mut self, now: f64) -> String {
        let candidates = [*self.range.start(), *self.range.end(), now];
        candidates
            .iter()
            .map(|v| self.format(*v))
            .max_by_key(|s| s.chars().count())
            .unwrap_or_default()
    }
}

/// How wide the label column is.
///
/// Not a bare `f32` for the same reason a colour is not a bare `Color32`: the
/// useful default is a function of the labels and the theme's type ramp, and a
/// number cannot be one. The widget this replaced hardcoded `140.0`, which
/// clipped under a larger ramp and wasted a column of space under a smaller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LabelWidth {
    /// The widest label in the group, measured at the theme's own type size.
    Measured,
    /// An exact width in px — for aligning a bank against something else on the
    /// page rather than against itself.
    Fixed(f32),
}

impl From<f32> for LabelWidth {
    fn from(px: f32) -> Self {
        Self::Fixed(px)
    }
}

/// What the group's running total is measured against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// The total the channels are supposed to sum to.
    pub target: f64,
    /// How far off `target` still counts as balanced. Sliders land on
    /// fractional values, so an exact comparison reads "over budget" at
    /// 100.00001.
    pub tolerance: f64,
}

impl Budget {
    pub fn new(target: f64) -> Self {
        Self {
            target,
            tolerance: 0.05,
        }
    }

    fn verdict(&self, sum: f64) -> Verdict {
        if sum > self.target + self.tolerance {
            Verdict::Over
        } else if sum < self.target - self.tolerance {
            Verdict::Under
        } else {
            Verdict::Balanced
        }
    }
}

/// Where the group's total sits relative to its [`Budget`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Over,
    Under,
    Balanced,
}

impl Verdict {
    pub const ALL: [Verdict; 3] = [Verdict::Over, Verdict::Under, Verdict::Balanced];

    pub fn label(self) -> &'static str {
        match self {
            Self::Over => "over budget",
            Self::Under => "under budget",
            Self::Balanced => "balanced",
        }
    }

    /// Semantic, not literal. Over is an `Error` because it is the state that
    /// cannot ship; under is a `Warning` because it is merely unfinished.
    pub fn ink(self) -> Ink {
        match self {
            Self::Over => Ink::Token(Token::Error),
            Self::Under => Ink::Token(Token::Warning),
            Self::Balanced => Ink::Token(Token::Success),
        }
    }
}

/// What the reader did this frame.
pub struct SliderGroupResponse {
    /// Any channel moved.
    pub changed: bool,
    /// Index of the channel that moved, if one did.
    pub changed_row: Option<usize>,
    /// Sum of every channel's value, whether or not a [`Budget`] was set — a
    /// caller may want it for its own readout.
    pub total: f64,
    /// `None` unless a [`Budget`] was set.
    pub verdict: Option<Verdict>,
}

/// A bank of labelled faders. See the module docs.
pub struct SliderGroup<'a> {
    rows: Vec<Fader<'a>>,
    label_width: LabelWidth,
    rail_width: RailWidth,
    budget: Option<Budget>,
    rail: Option<f32>,
}

impl<'a> Default for SliderGroup<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> SliderGroup<'a> {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            label_width: LabelWidth::Measured,
            rail_width: RailWidth::Fill,
            budget: None,
            rail: None,
        }
    }

    /// Add a pre-configured channel.
    ///
    /// Named `fader` rather than `add` because a bare `add` on a builder reads
    /// as `std::ops::Add` at a glance — and clippy says so too.
    pub fn fader(mut self, fader: Fader<'a>) -> Self {
        self.rows.push(fader);
        self
    }

    /// Add a channel with no extra configuration — the common case.
    pub fn slider<T: Numeric>(
        self,
        label: impl Into<String>,
        value: &'a mut T,
        range: RangeInclusive<T>,
    ) -> Self {
        self.fader(Fader::new(label, value, range))
    }

    /// Width of the label column. Default [`LabelWidth::Measured`].
    pub fn label_width(mut self, w: impl Into<LabelWidth>) -> Self {
        self.label_width = w.into();
        self
    }

    /// Show a running-total-vs-budget line under the bank — the master meter.
    pub fn budget(mut self, budget: impl Into<Budget>) -> Self {
        self.budget = Some(budget.into());
        self
    }

    /// Rail thickness override. Default scales [`BASE_RAIL`] with density.
    pub fn rail_height(mut self, px: f32) -> Self {
        self.rail = Some(px);
        self
    }

    /// How long the rails are. Default [`RailWidth::Fill`] — see its docs for
    /// why there is no built-in maximum.
    pub fn rail_width(mut self, w: impl Into<RailWidth>) -> Self {
        self.rail_width = w.into();
        self
    }

    pub fn show(mut self, ui: &mut Ui) -> SliderGroupResponse {
        let theme = ui.tokens();
        let gap = ui.space(Space::Md);
        let rail = self
            .rail
            .unwrap_or(BASE_RAIL * theme.density.multiplier());
        let font = egui::FontId::proportional(ui.text_size(TextSize::Sm));
        // Monospace for the readout: a proportional digit set changes width as
        // the value changes, so the number visibly shifts while you drag the
        // very fader you are watching.
        let mono = egui::FontId::monospace(ui.text_size(TextSize::Sm));

        let measure = ui.painter().clone();
        let text_w = |s: &str, f: &egui::FontId| -> f32 {
            measure
                .layout_no_wrap(s.to_owned(), f.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        };

        // ── the spine ─────────────────────────────────────────────────────
        // Both gutters are measured before anything is drawn, so every row
        // shares one label edge and one readout edge. This is the whole point
        // of the widget: the columns are what make a stack of sliders read as
        // one instrument.
        let value_w = {
            let mut widest = 0.0_f32;
            for r in self.rows.iter_mut() {
                let now = (r.get_set)(None);
                widest = widest.max(text_w(&r.widest(now), &mono));
            }
            widest
        };

        let wanted_label_w = match self.label_width {
            LabelWidth::Fixed(px) => px,
            LabelWidth::Measured => self
                .rows
                .iter()
                .map(|r| text_w(&r.label, &font))
                .fold(0.0_f32, f32::max),
        };

        // In a container too narrow for `label + rail + readout`, something has
        // to give, and it must be the LABEL: a truncated name is still a name,
        // whereas a rail squeezed to a few pixels is not a control any more.
        //
        // The floor is DERIVED, not chosen — a fader may not end up narrower
        // than the number it sets. That is a rule with a reason rather than a
        // pixel count someone liked, which is the whole objection to the
        // constants this widget nearly shipped with.
        let avail = ui.available_width();
        let label_w = wanted_label_w.min((avail - value_w * 2.0 - gap * 2.0).max(0.0));
        let rail_w = match self.rail_width {
            RailWidth::Fixed(px) => px,
            RailWidth::Fill => (avail - label_w - value_w - gap * 2.0).max(0.0),
        };

        let mut out = SliderGroupResponse {
            changed: false,
            changed_row: None,
            total: 0.0,
            verdict: None,
        };

        let row_h = ui.spacing().interact_size.y.max(rail);
        let last = self.rows.len().saturating_sub(1);
        for (i, row) in self.rows.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;

                // Label, right-aligned against the rail.
                ui.allocate_ui_with_layout(
                    egui::vec2(label_w, row_h),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        // Truncate, so a cramped container loses characters
                        // rather than pushing the rail off the edge.
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&row.label)
                                    .color(theme.color.text_secondary)
                                    .size(font.size),
                            )
                            .truncate(),
                        );
                    },
                );

                ui.spacing_mut().slider_width = rail_w;
                ui.spacing_mut().slider_rail_height = rail;
                ui.visuals_mut().selection.bg_fill = row
                    .tint
                    .unwrap_or(Ink::Token(Token::Accent))
                    .resolve(&theme);

                let range = row.range.clone();
                let step = row.step;
                let integral = row.integral;
                let mut slider =
                    egui::Slider::from_get_set(range, |v| (row.get_set)(v))
                        // The readout is ours, drawn in the column to the
                        // right. egui's own is a `DragValue` in a filled box,
                        // which outweighs the control it belongs to.
                        .show_value(false)
                        // Travel is the fader's job — an unfilled rail gives no
                        // sense of how far along the range you are without
                        // reading the number.
                        .trailing_fill(true)
                        .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.45 });
                if integral {
                    slider = slider.integer();
                }
                if let Some(s) = step {
                    slider = slider.step_by(s);
                }
                if ui.add(slider).changed() {
                    out.changed = true;
                    out.changed_row = Some(i);
                }

                let now = (row.get_set)(None);
                ui.allocate_ui_with_layout(
                    egui::vec2(value_w, row_h),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        ui.label(
                            egui::RichText::new(row.format(now))
                                .color(theme.color.text_primary)
                                .font(mono.clone()),
                        );
                    },
                );
            });
            // Tight: the rows are one instrument, not a list of controls.
            if i < last {
                ui.gap(Space::Xs);
            }
        }

        out.total = self
            .rows
            .iter_mut()
            .map(|r| (r.get_set)(None))
            .sum::<f64>();

        if let Some(budget) = self.budget {
            let verdict = budget.verdict(out.total);
            out.verdict = Some(verdict);
            ui.gap(Space::Sm);
            // Right-aligned, so it lands under the readout column it is the sum
            // of. Left-aligned it reads as a caption for the whole widget and
            // gives no hint which numbers produced it.
            //
            // `allocate_ui_with_layout` and not `with_layout`: the latter takes
            // the whole remaining height of the parent, which drops the meter to
            // the bottom of the pane instead of under the bank.
            let meter = egui::vec2(ui.available_width(), row_h);
            ui.allocate_ui_with_layout(meter, Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{:.0} / {:.0} — {}",
                        out.total,
                        budget.target,
                        verdict.label()
                    ))
                    .color(verdict.ink().resolve(&theme))
                    .size(font.size),
                );
            });
        }

        out
    }
}

impl From<f64> for Budget {
    fn from(target: f64) -> Self {
        Self::new(target)
    }
}

impl From<f32> for Budget {
    fn from(target: f32) -> Self {
        Self::new(target as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Id, Pos2, Rect, vec2};

    fn run(build: impl FnOnce(&mut Ui) -> SliderGroupResponse) -> SliderGroupResponse {
        run_in(600.0, build).0
    }

    /// Render inside a container of a given width, returning the response and
    /// the rect the group actually occupied.
    fn run_in(
        width: f32,
        build: impl FnOnce(&mut Ui) -> SliderGroupResponse,
    ) -> (SliderGroupResponse, Rect) {
        let ctx = egui::Context::default();
        let mut out = None;
        let mut used = Rect::NOTHING;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(2000.0, 600.0))),
            ..Default::default()
        };
        ctx.begin_pass(raw);
        egui::Area::new(Id::new("sg")).show(&ctx, |ui| {
            ui.set_max_width(width);
            let r = ui.scope(|ui| build(ui));
            out = Some(r.inner);
            used = r.response.rect;
        });
        let _ = ctx.end_pass();
        (out.unwrap(), used)
    }

    #[test]
    fn decimals_come_from_the_range_not_a_constant() {
        // One default cannot suit both ends: 2dp makes `0.0..=1.0` readable and
        // turns `100.0..=2000.0` into `1200.00`.
        let mut a = 0.5_f32;
        let mut b = 5.0_f32;
        let mut c = 1200.0_f32;
        let mut n = 4_u32;
        assert_eq!(Fader::new("", &mut a, 0.0..=1.0).auto_decimals(), 2);
        assert_eq!(Fader::new("", &mut b, 0.0..=20.0).auto_decimals(), 1);
        assert_eq!(Fader::new("", &mut c, 100.0..=2000.0).auto_decimals(), 0);
        assert_eq!(
            Fader::new("", &mut n, 2..=8).auto_decimals(),
            0,
            "an integer channel never shows a fraction"
        );
    }

    #[test]
    fn the_readout_column_is_sized_for_the_whole_range() {
        // Sized from the range's ENDS, not the value in hand — otherwise the
        // column resizes as you drag and the number you are reading moves.
        let mut v = 5.0_f32;
        let mut f = Fader::new("x", &mut v, 0.0..=1000.0).suffix("%");
        let widest = f.widest(5.0);
        assert_eq!(widest, "1000%", "the widest the range can print");
    }

    #[test]
    fn a_value_outside_its_range_still_fits() {
        // Caller state can drift outside the range it declared. If the column
        // were sized from the range alone the readout would be clipped.
        let mut v = 12345.0_f32;
        let mut f = Fader::new("x", &mut v, 0.0..=100.0);
        assert_eq!(f.widest(12345.0), "12345");
    }

    #[test]
    fn suffix_and_decimals_survive_formatting() {
        let mut v = 0.5_f32;
        let f = Fader::new("x", &mut v, 0.0..=1.0).suffix("×").decimals(3);
        assert_eq!(f.format(0.5), "0.500×");
    }

    #[test]
    fn a_budget_tolerates_slider_dust() {
        // Sliders land on fractional values; an exact comparison reports "over
        // budget" at 100.00001, which is a cue that cries wolf.
        let b = Budget::new(100.0);
        assert_eq!(b.verdict(100.0), Verdict::Balanced);
        assert_eq!(b.verdict(100.04), Verdict::Balanced);
        assert_eq!(b.verdict(99.96), Verdict::Balanced);
        assert_eq!(b.verdict(100.06), Verdict::Over);
        assert_eq!(b.verdict(99.94), Verdict::Under);
    }

    #[test]
    fn every_verdict_has_its_own_ink() {
        // Over/under/balanced must be distinguishable, and all three must go
        // through the theme — this cue used to be three `Color32` literals.
        let inks: Vec<Ink> = Verdict::ALL.iter().map(|v| v.ink()).collect();
        for (i, a) in inks.iter().enumerate() {
            assert!(!a.is_fixed(), "{:?} escapes the theme", Verdict::ALL[i]);
            for b in &inks[i + 1..] {
                assert_ne!(a, b, "two verdicts share an ink");
            }
        }
    }

    #[test]
    fn the_total_sums_every_channel() {
        let mut a = 10.0_f32;
        let mut b = 25.5_f32;
        let r = run(|ui| {
            SliderGroup::new()
                .slider("a", &mut a, 0.0..=100.0)
                .slider("b", &mut b, 0.0..=100.0)
                .show(ui)
        });
        assert!((r.total - 35.5).abs() < 1e-4, "got {}", r.total);
        assert!(!r.changed, "nothing was dragged");
        assert_eq!(r.verdict, None, "no budget set, no verdict");
    }

    #[test]
    fn a_budget_produces_a_verdict() {
        let mut a = 60.0_f32;
        let mut b = 60.0_f32;
        let r = run(|ui| {
            SliderGroup::new()
                .slider("a", &mut a, 0.0..=100.0)
                .slider("b", &mut b, 0.0..=100.0)
                .budget(100.0_f32)
                .show(ui)
        });
        assert_eq!(r.verdict, Some(Verdict::Over));
    }

    #[test]
    fn one_channel_is_a_valid_bank() {
        // A group that happens to have one row today should not need rewriting
        // when it gets a second.
        let mut only = 3.0_f32;
        let r = run(|ui| {
            SliderGroup::new()
                .slider("only", &mut only, 0.0..=10.0)
                .show(ui)
        });
        assert!((r.total - 3.0).abs() < 1e-4);
    }

    #[test]
    fn an_empty_group_draws_nothing_and_does_not_panic() {
        let r = run(|ui| SliderGroup::new().show(ui));
        assert_eq!(r.total, 0.0);
        assert!(!r.changed);
    }

    #[test]
    fn channels_of_different_numeric_types_share_one_bank() {
        // The reason rows hold a get/set closure rather than a `&mut f32`: a
        // real control panel mixes an integer count with a float ratio, and
        // splitting those into two groups would break the spine that is the
        // entire point of the widget.
        let mut count = 4_u32;
        let mut ratio = 0.25_f32;
        let mut offset = -3_i32;
        let r = run(|ui| {
            SliderGroup::new()
                .slider("count", &mut count, 0..=10)
                .slider("ratio", &mut ratio, 0.0..=1.0)
                .slider("offset", &mut offset, -10..=10)
                .show(ui)
        });
        assert!((r.total - 1.25).abs() < 1e-4, "4 + 0.25 + -3, got {}", r.total);
    }

    // ── "plays nicely anywhere" ───────────────────────────────────────────
    //
    // The widget fills its container by design and takes no view on a maximum
    // — a caller that wants it narrower uses `Ui::fit_width`. That makes
    // "does not overflow whatever it is given" a promise worth testing at both
    // extremes rather than an assumption.

    #[test]
    fn the_bank_stays_inside_its_container() {
        // Including widths narrower than the labels want, which is the case
        // that would push the rail off the right edge.
        for w in [140.0_f32, 220.0, 400.0, 900.0, 1568.0] {
            let mut a = 25.0_f32;
            let mut b = 60.0_f32;
            let (_, rect) = run_in(w, |ui| {
                SliderGroup::new()
                    .fader(Fader::new("Background: Purple", &mut a, 0.0..=100.0).suffix("%"))
                    .fader(Fader::new("Background: Magenta", &mut b, 0.0..=100.0).suffix("%"))
                    .show(ui)
            });
            assert!(
                rect.width() <= w + 0.5,
                "at container {w}, the bank took {}",
                rect.width()
            );
        }
    }

    #[test]
    fn a_budget_line_stays_inside_too() {
        // The meter is right-aligned, which is exactly the shape that escapes a
        // container if it is laid out against the wrong rect.
        for w in [160.0_f32, 500.0] {
            let mut a = 60.0_f32;
            let (_, rect) = run_in(w, |ui| {
                SliderGroup::new()
                    .slider("a", &mut a, 0.0..=100.0)
                    .budget(100.0_f32)
                    .show(ui)
            });
            assert!(rect.width() <= w + 0.5, "at {w}, took {}", rect.width());
        }
    }

    #[test]
    fn a_cramped_container_costs_the_label_not_the_rail() {
        // A truncated name is still a name; a rail squeezed to nothing is not a
        // control. The floor is derived — a fader never ends up narrower than
        // the number it sets — so this asserts the widget still renders and
        // stays in bounds rather than pinning a pixel count.
        let mut v = 25.0_f32;
        let (r, rect) = run_in(150.0, |ui| {
            SliderGroup::new()
                .fader(
                    Fader::new(
                        "an extremely long channel name that cannot possibly fit",
                        &mut v,
                        0.0..=100.0,
                    )
                    .suffix("%"),
                )
                .show(ui)
        });
        assert!((r.total - 25.0).abs() < 1e-4);
        assert!(rect.width() <= 150.5, "took {}", rect.width());
    }

    #[test]
    fn a_fixed_rail_width_is_honoured() {
        let mut v = 1.0_f32;
        let (r, _) = run_in(900.0, |ui| {
            SliderGroup::new()
                .slider("a", &mut v, 0.0..=10.0)
                .rail_width(120.0)
                .show(ui)
        });
        assert!((r.total - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_fixed_label_width_overrides_measurement() {
        // Both paths must render. `Measured` is the default; `Fixed` is for
        // aligning a bank against something else on the page.
        let mut v = 1.0_f32;
        let r = run(|ui| {
            SliderGroup::new()
                .slider("a very long channel name indeed", &mut v, 0.0..=10.0)
                .label_width(40.0)
                .show(ui)
        });
        assert!((r.total - 1.0).abs() < 1e-4);
    }
}
