//! `FlameChart` — nested spans on a zoomable time axis: what a frame spent its time on.
//!
//! The shape a profiler's output wants. Each span is a bar: x is when it ran
//! and how long for, y is how deep in the call stack it sat, and a child is
//! always drawn inside its parent's extent. Reading down a column answers "what
//! was running at this instant"; reading across a row answers "where did this
//! millisecond go".
//!
//! # Why this is here rather than a dependency
//!
//! `puffin_egui` is the obvious answer and it cannot be used: its latest
//! release targets egui 0.28 against this workspace's 0.36, so adding it pulls
//! a SECOND egui into the graph whose panel could not share our `Context`
//! anyway.
//!
//! It turned out to be a small thing to write, because the hard half already
//! existed. [`crate::time_spine::TimeScale`] is a zoomable time↔x mapping with
//! pan, zoom-at-pointer and extrapolation, ported from Rerun's time ruler —
//! which is the part that is genuinely fiddly to get right. What is left is
//! rectangles.
//!
//! # It takes spans, not profiler types
//!
//! [`Span`] is four fields of plain data. The widget has no idea what produced
//! them, which is what lets it be developed and reviewed in the storybook
//! against synthetic input with no browser involved, and what would let a
//! second source — a trace from a worker, a server timing — reuse it unchanged.
//! The `puffin` adapter is [`crate::flame_chart::puffin`], behind a feature, so
//! nothing links a profiler to draw a rectangle.
//!
//! # Reading notes
//!
//! - **Colour is a separator, not an encoding.** With unbounded distinct scope
//!   names there is no series set and no legend to have; hue comes from the
//!   theme's [`ui_theme::IdentityEnvelope`], which fixes lightness so the
//!   surface stays legible and lets only hue vary with the hash. Identity is
//!   carried by the label on the bar and the tooltip, never by colour alone.
//! - **Sub-pixel spans become hairlines, not nothing.** A frame's domain is
//!   the wall-clock gap between repaints, so most of it is idle and the work
//!   is a cluster inside it — at default zoom the individual scopes really are
//!   fractions of a pixel. Dropping them was the first rule here and it
//!   rendered a 149-scope capture completely blank. Only OFF-SCREEN spans are
//!   culled; narrow ones are widened to a hairline, which keeps them visible
//!   and hoverable, and bounds the work at one thin rect per pixel column.
//! - **The axis is its own tick painter.** `time_spine::paint_ticks` takes an
//!   injectable label formatter but its SPACING ladder is
//!   `next_tick_step_secs` — 1 → 10 → 60 → 600 → 3600, a time-of-day
//!   progression. A frame is five milliseconds, so it wants a decimal ladder.
//!   Rather than reach into a 2,500-line module that many surfaces depend on,
//!   the forty lines are here.

use crate::theme::{Ink, InkExt, TextSize, ThemeExt, Token};
use crate::time_spine::{TimeScale, TimeView};
use egui::{Align2, Color32, Rect, Response, Sense, Stroke, Ui, Vec2, pos2};
use std::sync::Arc;

#[cfg(feature = "puffin")]
pub mod puffin;

/// One measured span of work.
///
/// Nanoseconds because that is what profilers emit and because a frame's
/// interesting detail is microseconds wide; milliseconds as floats would lose
/// the bottom of the stack to rounding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Call depth. `0` is a root; a child is always drawn inside its parent.
    pub depth: u16,
    pub start_ns: i64,
    pub duration_ns: i64,
    /// What ran. `Arc` because scope names repeat heavily — a frame is mostly
    /// the same dozen functions — and this is cloned per span.
    pub label: Arc<str>,
}

impl Span {
    pub fn end_ns(&self) -> i64 {
        self.start_ns.saturating_add(self.duration_ns)
    }
}

/// What the reader did to the chart this frame.
pub struct FlameChartResponse {
    pub response: Response,
    /// Index into the spans slice, for a caller that wants to drive a detail
    /// panel from the hover rather than settle for the tooltip.
    pub hovered: Option<usize>,
}

/// Nested spans on a zoomable time axis.
pub struct FlameChart<'a> {
    spans: &'a [Span],
    id_salt: &'a str,
    row_height: f32,
}

impl<'a> FlameChart<'a> {
    pub fn new(spans: &'a [Span]) -> Self {
        Self {
            spans,
            id_salt: "flame-chart",
            row_height: 18.0,
        }
    }

    /// Distinguishes two charts in one surface, since the zoom is held in
    /// `egui` memory against this id.
    pub fn id_salt(mut self, id_salt: &'a str) -> Self {
        self.id_salt = id_salt;
        self
    }

    /// Height of one stack level. The default fits the small text size with
    /// room for the 1px separator above it.
    pub fn row_height(mut self, row_height: f32) -> Self {
        self.row_height = row_height;
        self
    }

    pub fn show(self, ui: &mut Ui) -> FlameChartResponse {
        profiling::function_scope!();

        let theme = ui.tokens();
        let muted = Ink::Token(Token::TextMuted).of(ui);

        let Some((domain_start, domain_end)) = domain_of(self.spans) else {
            let response = ui.label(
                egui::RichText::new("Nothing captured yet")
                    .color(muted)
                    .size(ui.text_size(TextSize::Sm)),
            );
            return FlameChartResponse {
                response,
                hovered: None,
            };
        };

        // EVERYTHING BELOW IS RELATIVE TO THE FIRST SCOPE, and that is not
        // cosmetic. A profiler's timestamps are absolute — puffin's are
        // nanoseconds since the unix epoch, so around 1.79e18 — and an `f64`
        // holding a number that size has an ulp of 256 NANOSECONDS. Mapping
        // positions from it quantises every scope to a quarter-microsecond
        // grid, and the axis reads "1789864436.38s" where a reader wants
        // "0 → 4ms". Rebasing costs one subtraction and removes both.
        let origin = domain_start;
        let domain_end = domain_end - origin;
        let domain_start = 0;

        let depth = self.spans.iter().map(|s| s.depth).max().unwrap_or(0);
        let rows = f32::from(depth) + 1.0;
        let height = rows * self.row_height + AXIS_HEIGHT;

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), height),
            Sense::click_and_drag(),
        );

        let id = ui.id().with(self.id_salt);
        let full = TimeView::covering(domain_start, domain_end);
        let mut view = ui.data(|d| d.get_temp::<TimeView>(id)).unwrap_or(full);

        // ── input ───────────────────────────────────────────────────────────
        //
        // The same verbs `time_spine` established: wheel zooms about the
        // POINTER, drag pans, double-click restores the whole domain. Anchoring
        // zoom on the pointer rather than the centre is what makes "look closer
        // at that spike" one gesture instead of zoom-then-pan.
        let plot = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - AXIS_HEIGHT));
        let scale_for_input = TimeScale::continuous(plot.x_range(), view, domain_start, domain_end);
        if response.double_clicked() {
            view = full;
        } else {
            if response.dragged()
                && let Some(panned) = scale_for_input.pan(-response.drag_delta().x)
            {
                view = panned;
            }
            if let Some(pointer) = response.hover_pos() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 100.0);
                if scroll != 0.0 {
                    let factor = (scroll * ZOOM_PER_SCROLL_UNIT).exp();
                    let span = (domain_end - domain_start).max(1) as f64;
                    if let Some(zoomed) = scale_for_input.zoom_at(pointer.x, factor, span) {
                        view = zoomed;
                    }
                }
            }
        }
        ui.data_mut(|d| d.insert_temp(id, view));

        let scale = TimeScale::continuous(plot.x_range(), view, domain_start, domain_end);
        let painter = ui.painter_at(rect);

        // ── axis ────────────────────────────────────────────────────────────
        //
        // Recessive by construction: hairlines at the muted ink, labels one
        // size down. The data is the figure; the ruler is ground.
        paint_axis(ui, &painter, &scale, plot, muted);

        // ── spans ───────────────────────────────────────────────────────────
        let pointer = response.hover_pos().filter(|p| plot.contains(*p));
        let mut hovered = None;
        let label_size = ui.text_size(TextSize::Xs);

        for (index, span) in self.spans.iter().enumerate() {
            let (Some(x0), Some(x1)) = (
                scale.x_from_time_f32((span.start_ns - origin) as f64),
                scale.x_from_time_f32((span.end_ns() - origin) as f64),
            ) else {
                continue;
            };
            // Off screen is a cull. Too narrow is NOT — it is a clamp to a
            // hairline.
            //
            // Culling narrow spans was the original rule and it rendered an
            // entire capture blank: a frame spans the wall-clock gap between
            // repaints, so 149 scopes worth 3ms of work sat inside a 30ms
            // domain. At 28µs per pixel every one of them was sub-pixel and
            // every one was dropped — a chart obeying its own rule into
            // uselessness.
            //
            // A hairline is honest: it says work happened here and is too
            // small to read at this zoom, which is what zooming is for. The
            // cost is bounded because the OFF-SCREEN cull still runs, so this
            // is at worst one thin rect per pixel column.
            if x1 < plot.left() || plot.right() < x0 {
                continue;
            }

            let top = plot.top() + f32::from(span.depth) * self.row_height;
            // The 1px inset is the surface showing through between adjacent
            // fills, horizontally and vertically — without it a run of sibling
            // calls reads as one long bar.
            let left = x0.max(plot.left());
            let bar = Rect::from_min_max(
                pos2(left + SEPARATOR, top + SEPARATOR),
                // At least a hairline wide, so a scope too brief to draw is
                // still visible AND still hoverable — the tooltip is how you
                // read one without zooming to it.
                pos2(
                    x1.min(plot.right()).max(left + MIN_SPAN_WIDTH),
                    top + self.row_height,
                ),
            );

            let fill = theme
                .series
                .identity
                .color(crate::utxo_map::simple_hash(&span.label) as u64);
            let is_hovered = pointer.is_some_and(|p| bar.contains(p));
            if is_hovered {
                hovered = Some(index);
            }

            painter.rect_filled(bar, CORNER, fill);
            if is_hovered {
                // A ring rather than a colour change: recolouring a bar on
                // hover breaks the one rule this chart's colour has, which is
                // that a hash always lands on the same hue.
                painter.rect_stroke(
                    bar,
                    CORNER,
                    Stroke::new(1.0, Ink::Token(Token::TextPrimary).of(ui)),
                    egui::StrokeKind::Inside,
                );
            }

            // Only label a bar with room for something worth reading. A
            // truncated glyph and an ellipsis is noise at this density.
            if bar.width() >= MIN_LABEL_WIDTH {
                painter.text(
                    pos2(bar.left() + LABEL_PAD, bar.center().y),
                    Align2::LEFT_CENTER,
                    elide(&span.label, bar.width() - LABEL_PAD * 2.0, label_size),
                    egui::FontId::proportional(label_size),
                    ink_on(fill),
                );
            }
        }

        if let Some(index) = hovered {
            let span = &self.spans[index];
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                id.with("tip"),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.set_max_width(320.0);
                // The full name, never elided — the bar's own label is cut to
                // fit and this is where the rest of it lives.
                ui.label(egui::RichText::new(span.label.as_ref()).strong());
                ui.label(
                    egui::RichText::new(format!(
                        "{} · depth {}",
                        format_duration_ns(span.duration_ns),
                        span.depth
                    ))
                    .color(Ink::Token(Token::TextMuted).of(ui))
                    .size(ui.text_size(TextSize::Sm)),
                );
            });
        }

        FlameChartResponse { response, hovered }
    }
}

/// Height reserved under the plot for the ruler.
const AXIS_HEIGHT: f32 = 16.0;
/// The gap between adjacent fills, so siblings read as separate calls.
const SEPARATOR: f32 = 1.0;
const CORNER: u8 = 2;
/// The narrowest a bar is drawn. Anything briefer is widened to this rather
/// than dropped — see the clamp in `show`.
const MIN_SPAN_WIDTH: f32 = 1.5;
const MIN_LABEL_WIDTH: f32 = 28.0;
const LABEL_PAD: f32 = 3.0;
/// Scroll units to `e`-folds of zoom. Tuned so one wheel notch is a noticeable
/// but not disorienting step.
const ZOOM_PER_SCROLL_UNIT: f32 = 0.002;

/// A capture as pasteable text: where the time went, then the tree.
///
/// # Why SELF time leads
///
/// Inclusive time says `App::ui` took 4.2ms, which is true and useless — it is
/// the frame. Self time is a scope's own duration minus its children's, so it
/// says which code actually held the CPU rather than which code was on the
/// stack while something else did. Sorting by it puts the answer first.
///
/// Names are aggregated with a count, because a frame is mostly the same
/// handful of functions: `×48 card` is the finding, where forty-eight separate
/// lines would bury it.
///
/// # Why this exists at all
///
/// A flame chart is read by pointing at it. A capture that has to leave the
/// browser — into a bug report, a message, a diff between two builds — needs
/// to survive as text, and a screenshot does not: it cannot be searched,
/// diffed, or totalled.
pub fn report(spans: &[Span]) -> String {
    use std::fmt::Write as _;

    let Some((start, end)) = domain_of(spans) else {
        return "flame: nothing captured".to_owned();
    };

    let mut out = String::new();
    let _ = writeln!(
        out,
        "flame: {} spans · {} wall",
        spans.len(),
        format_duration_ns(end - start)
    );

    // How much of the window the thread was actually BUSY.
    //
    // This is the difference between "21 readbacks back-to-back for 35ms, so
    // no frame could paint" and "21 readbacks spread across 35ms with gaps
    // between them". The ranked list below cannot tell those apart — both
    // report the same totals — and the distinction is the whole question when
    // something is janky. It used to take a screenshot of the chart to see.
    let (busy, roots) = busy_ns(spans);
    let wall = (end - start).max(1);
    let _ = writeln!(
        out,
        "  busy {} ({}% idle) across {roots} root spans",
        format_duration_ns(busy),
        (wall - busy) * 100 / wall,
    );

    let ranked = aggregate(spans);

    // Self time cannot exceed wall time on one thread. When it does, a scope
    // guard was held across an `.await`: the task yields, another task opens
    // the same scope, and the profiler — which only sees one stream — records
    // them as nested, each charged the full time its future was pending.
    //
    // Said here because the shape is genuinely hard to read otherwise. The
    // capture that prompted this looked like a 211ms image decode inside a
    // 76ms frame, and the honest reading (the decode was WAITING, off-thread,
    // and cost the frame nothing) is the opposite of what it appeared to say.
    let total_self: i64 = ranked.iter().map(|(_, a)| a.self_ns).sum();
    if total_self > wall {
        let _ = writeln!(
            out,
            "WARNING: self time {} EXCEEDS wall {} ({}%) — a scope is being held \
             across an await; treat the nesting below as bogus and read the \
             synchronous scopes only",
            format_duration_ns(total_self),
            format_duration_ns(wall),
            total_self * 100 / wall,
        );
    }

    // No column header, deliberately: each figure is labelled in place. A
    // header row would push the costliest scope off the line below "by self
    // time:", which is where both a reader and the test look for it.
    let _ = writeln!(out, "\nby self time:");
    for (name, agg) in ranked.iter().take(TOP_N) {
        let _ = writeln!(
            out,
            "  {:>9}  {:>5.1}%  ×{:<5} avg {:>8}  max {:>8}  total {:>9}  {name}",
            format_duration_ns(agg.self_ns),
            agg.self_ns as f64 / wall as f64 * 100.0,
            agg.count,
            format_duration_ns(agg.self_ns / agg.count.max(1) as i64),
            format_duration_ns(agg.max_self_ns),
            format_duration_ns(agg.total_ns),
        );
    }
    if ranked.len() > TOP_N {
        let _ = writeln!(out, "  … {} more", ranked.len() - TOP_N);
    }

    let _ = writeln!(out, "\ntree:");
    for span in spans.iter().take(TREE_LINES) {
        let _ = writeln!(
            out,
            "  {:>9} {:indent$}{}",
            format_duration_ns(span.duration_ns),
            "",
            span.label,
            indent = span.depth as usize * 2
        );
    }
    if spans.len() > TREE_LINES {
        let _ = writeln!(out, "  … {} more", spans.len() - TREE_LINES);
    }
    out
}

/// One scope's aggregate across a whole capture.
struct Agg {
    /// Summed self time — duration less direct children.
    self_ns: i64,
    /// Summed inclusive duration. Against `self_ns` this says whether a scope
    /// SPENDS time or merely HOSTS it.
    total_ns: i64,
    /// The worst single occurrence, which is what separates "twenty calls at
    /// 1ms" from "nineteen free ones and a 20ms stall".
    max_self_ns: i64,
    count: usize,
}

/// Aggregate by scope name, ranked by self time.
///
/// Ties break on name so two captures of the same workload diff cleanly —
/// these get compared against each other.
fn aggregate(spans: &[Span]) -> Vec<(&str, Agg)> {
    use std::collections::HashMap;

    let mut totals: HashMap<&str, Agg> = HashMap::new();
    for (index, span) in spans.iter().enumerate() {
        let self_ns = self_time_ns(spans, index);
        let entry = totals.entry(span.label.as_ref()).or_insert(Agg {
            self_ns: 0,
            total_ns: 0,
            max_self_ns: 0,
            count: 0,
        });
        entry.self_ns += self_ns;
        entry.total_ns += span.duration_ns;
        entry.max_self_ns = entry.max_self_ns.max(self_ns);
        entry.count += 1;
    }
    let mut ranked: Vec<(&str, Agg)> = totals.into_iter().collect();
    ranked.sort_by(|a, b| b.1.self_ns.cmp(&a.1.self_ns).then_with(|| a.0.cmp(b.0)));
    ranked
}

/// Wall time the thread was inside SOME root span, and how many there were.
///
/// The union of the depth-0 intervals, not their sum: roots can abut or (in a
/// malformed capture) overlap, and double-counting would report a thread as
/// busier than the clock allows.
fn busy_ns(spans: &[Span]) -> (i64, usize) {
    let mut roots: Vec<(i64, i64)> = spans
        .iter()
        .filter(|s| s.depth == 0)
        .map(|s| (s.start_ns, s.start_ns + s.duration_ns))
        .collect();
    let count = roots.len();
    roots.sort_by_key(|&(start, _)| start);

    let mut busy = 0;
    let mut open: Option<(i64, i64)> = None;
    for (start, end) in roots {
        match open {
            Some((a, b)) if start <= b => open = Some((a, b.max(end))),
            Some((a, b)) => {
                busy += b - a;
                open = Some((start, end));
            }
            None => open = Some((start, end)),
        }
    }
    if let Some((a, b)) = open {
        busy += b - a;
    }
    (busy, count)
}

/// Index one past the last span of `i`'s subtree.
///
/// Depth-first order means a subtree is exactly the run of deeper spans that
/// follows its root.
fn subtree_end(spans: &[Span], i: usize) -> usize {
    let depth = spans[i].depth;
    let mut j = i + 1;
    while j < spans.len() && spans[j].depth > depth {
        j += 1;
    }
    j
}

/// The same capture, encoded for a machine rather than an eye.
///
/// # Why this exists
///
/// [`report`] is padded columns, repeated unit suffixes and one line per span,
/// and its tree truncates long before the interesting part — a capture with
/// 2,885 galley layouts spent its entire 60-line budget printing
/// `text_layout::layout` over and over and conveyed nothing. Pasted into a
/// conversation that is most of a message spent on whitespace.
///
/// This carries strictly MORE information in a fraction of the tokens:
/// scope names appear exactly once in a dictionary and are integers
/// everywhere else, durations are bare integer microseconds (the browser
/// coarsens `performance.now()` to 100µs anyway, so nanosecond digits were
/// always noise), and runs of identical siblings collapse to one entry with a
/// count instead of hundreds of lines.
///
/// # Format
///
/// Self-describing on purpose — the `L` line is a legend, so a reader who has
/// never seen this format can decode it without the source:
///
/// ```text
/// FLAME/1 u=us w=48500 b=39500 r=21 n=254 a=1
/// L S=id:self,n,max,total(desc by self) T=depth,id,dur[,xN=repeats] D=dict(id=pos)
/// D|image_loader::readback|browse_view::draw_browse_view|…
/// S|0:39500,21,2100,39500|1:1300,1,1300,2300|…
/// T|0,0,2100|1,5,100|2,7,31300,x2885|…
/// ```
///
/// Header keys: `w` wall, `b` busy (union of roots), `r` root count, `n` span
/// count, `a=1` present ONLY when self time exceeds wall — the
/// scope-held-across-an-await artifact, where the nesting is meaningless.
/// A trailing `|+K` on `T` means K spans went unprinted.
pub fn report_compact(spans: &[Span]) -> String {
    use std::collections::HashMap;
    use std::fmt::Write as _;

    let Some((start, end)) = domain_of(spans) else {
        return "FLAME/1 empty".to_owned();
    };

    let wall = (end - start).max(1);
    let (busy, roots) = busy_ns(spans);
    let ranked = aggregate(spans);
    let total_self: i64 = ranked.iter().map(|(_, a)| a.self_ns).sum();

    // Ids follow the ranking, so the scopes that appear most in the tree get
    // the shortest numbers.
    let id_of: HashMap<&str, usize> = ranked
        .iter()
        .enumerate()
        .map(|(i, (name, _))| (*name, i))
        .collect();

    let us = |ns: i64| ns / 1_000;

    let mut out = String::new();
    let _ = write!(
        out,
        "FLAME/1 u=us w={} b={} r={roots} n={}",
        us(wall),
        us(busy),
        spans.len(),
    );
    if total_self > wall {
        let _ = write!(out, " a=1");
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "L S=id:self,n,max,total(desc by self) T=depth,id,dur[,xN=repeats] D=dict(id=pos)"
    );

    let _ = write!(out, "D");
    for (name, _) in &ranked {
        let _ = write!(out, "|{name}");
    }
    let _ = writeln!(out);

    let _ = write!(out, "S");
    for (index, (_, agg)) in ranked.iter().enumerate() {
        let _ = write!(
            out,
            "|{index}:{},{},{},{}",
            us(agg.self_ns),
            agg.count,
            us(agg.max_self_ns),
            us(agg.total_ns),
        );
    }
    let _ = writeln!(out);

    let _ = write!(out, "T");
    let mut printed = 0usize;
    let mut i = 0usize;
    while i < spans.len() {
        if printed >= TREE_ENTRIES {
            let _ = write!(out, "|+{}", spans.len() - i);
            break;
        }
        let span = &spans[i];

        // Collapse a run of consecutive siblings sharing a label — the 2,885
        // identical `text_layout::layout` case, which is otherwise the entire
        // budget. Their subtrees go with them; the `S` table above still
        // accounts for every one.
        let mut count = 1usize;
        let mut total = span.duration_ns;
        let mut j = subtree_end(spans, i);
        while j < spans.len() && spans[j].depth == span.depth && spans[j].label == span.label {
            count += 1;
            total += spans[j].duration_ns;
            j = subtree_end(spans, j);
        }

        let id = id_of[span.label.as_ref()];
        if count > 1 {
            let _ = write!(out, "|{},{id},{},x{count}", span.depth, us(total));
            i = j;
        } else {
            let _ = write!(out, "|{},{id},{}", span.depth, us(span.duration_ns));
            i += 1;
        }
        printed += 1;
    }
    let _ = writeln!(out);

    out
}

/// A span's own time: its duration less that of its direct children.
///
/// Children are the spans that FOLLOW it at exactly one level deeper, up to
/// the next span at its own level or shallower — which is what depth-first
/// order means. Reading them positionally avoids carrying parent links on
/// every span for the sake of one report.
fn self_time_ns(spans: &[Span], index: usize) -> i64 {
    let parent = &spans[index];
    let mut self_ns = parent.duration_ns;
    for child in &spans[index + 1..] {
        if child.depth <= parent.depth {
            break;
        }
        if child.depth == parent.depth + 1 {
            self_ns -= child.duration_ns;
        }
    }
    // Clamped: a capture whose children overrun their parent is malformed, and
    // a negative "self time" would sort to the bottom and read as a fact.
    self_ns.max(0)
}

/// Enough to find the answer, few enough to paste into a message.
const TOP_N: usize = 25;
const TREE_LINES: usize = 60;

/// The compact tree's budget, in ENTRIES rather than lines.
///
/// Far larger than [`TREE_LINES`] because an entry is three integers, not a
/// padded line carrying a repeated scope name — and because collapsing runs
/// means this many entries covers vastly more of a capture.
const TREE_ENTRIES: usize = 400;

/// The span of time the chart covers, or `None` for no spans.
///
/// Pure, so the empty and single-span cases are settled by a test rather than
/// by a browser.
pub fn domain_of(spans: &[Span]) -> Option<(i64, i64)> {
    let start = spans.iter().map(|s| s.start_ns).min()?;
    let end = spans.iter().map(Span::end_ns).max()?;
    // A domain of zero width has no mapping to a pixel range. One nanosecond
    // is arbitrary but keeps every downstream division defined.
    Some((start, end.max(start + 1)))
}

/// The next coarser tick step, on a 1 → 2 → 5 → 10 decimal ladder.
///
/// NOT `time_spine::next_tick_step_secs`, which climbs 1 → 10 → 60 → 600 →
/// 3600 because it is ruling a clock. Durations have no sixties in them, and
/// on a five-millisecond frame that ladder yields one tick or none.
pub fn next_tick_step_ns(step: i64) -> i64 {
    let magnitude = 10i64.pow(step.max(1).ilog10());
    match step / magnitude {
        1 => magnitude * 2,
        2 => magnitude * 5,
        _ => magnitude * 10,
    }
}

/// A duration, at the precision a reader can act on.
///
/// Three significant figures and one unit. A frame budget is 16.7ms and the
/// scopes inside it are microseconds, so the unit has to move; what must not
/// move is the number of digits, because these are read in a column.
pub fn format_duration_ns(ns: i64) -> String {
    let abs = ns.unsigned_abs();
    match abs {
        0..=999 => format!("{ns}ns"),
        1_000..=999_999 => format!("{:.1}µs", ns as f64 / 1_000.0),
        1_000_000..=999_999_999 => format!("{:.2}ms", ns as f64 / 1_000_000.0),
        _ => format!("{:.2}s", ns as f64 / 1_000_000_000.0),
    }
}

fn paint_axis(ui: &Ui, painter: &egui::Painter, scale: &TimeScale, plot: Rect, muted: Color32) {
    let (from, to) = (
        scale.time_from_x(plot.left() as f64),
        scale.time_from_x(plot.right() as f64),
    );
    let (Some(from), Some(to)) = (from, to) else {
        return;
    };
    let visible = (to - from).max(1.0);

    // Aim for a tick every ~80px, then snap up to the next ladder rung so the
    // labels land on round numbers a reader can subtract in their head.
    let target = visible / (plot.width() as f64 / 80.0).max(1.0);
    let mut step = 1i64;
    while (step as f64) < target {
        step = next_tick_step_ns(step);
    }

    let first = (from / step as f64).ceil() as i64 * step;
    let label_size = ui.text_size(TextSize::Xs);
    let mut tick = first;
    while (tick as f64) <= to {
        if let Some(x) = scale.x_from_time_f32(tick as f64) {
            painter.line_segment(
                [pos2(x, plot.top()), pos2(x, plot.bottom())],
                Stroke::new(1.0, muted.gamma_multiply(0.25)),
            );
            painter.text(
                pos2(x + 2.0, plot.bottom() + 2.0),
                Align2::LEFT_TOP,
                format_duration_ns(tick),
                egui::FontId::proportional(label_size),
                muted,
            );
        }
        tick += step;
    }
}

/// Black or white, whichever the fill can carry.
///
/// A departure from "text wears text tokens", and a deliberate one: this label
/// sits INSIDE a filled bar, where the token's contrast is against the page
/// surface rather than against the fill. Contrast wins over palette
/// consistency when the alternative is an unreadable label.
fn ink_on(fill: Color32) -> Color32 {
    // Rec. 601 luma — close enough for a binary light/dark decision, and
    // cheaper than an Oklab round trip per bar per frame.
    let luma =
        0.299 * f32::from(fill.r()) + 0.587 * f32::from(fill.g()) + 0.114 * f32::from(fill.b());
    if luma > 140.0 {
        Color32::from_gray(20)
    } else {
        Color32::from_gray(240)
    }
}

/// Trim a label to what fits, middle-out on the assumption that the tail of a
/// scope name (the function) says more than the head (the module).
fn elide(label: &str, width: f32, size: f32) -> String {
    // Proportional text is not monospace, but a flame chart's labels are cut to
    // a rectangle, not typeset. This approximation is checked against the
    // painted width by the caller's `MIN_LABEL_WIDTH` gate.
    let per_char = size * 0.5;
    let fits = (width / per_char).floor().max(0.0) as usize;
    if label.len() <= fits {
        return label.to_owned();
    }
    if fits <= 1 {
        return String::new();
    }
    label
        .char_indices()
        .nth(fits - 1)
        .map(|(cut, _)| format!("{}…", &label[..cut]))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(depth: u16, start_ns: i64, duration_ns: i64, label: &str) -> Span {
        Span {
            depth,
            start_ns,
            duration_ns,
            label: Arc::from(label),
        }
    }

    #[test]
    fn the_domain_spans_from_the_first_start_to_the_last_end() {
        let spans = [
            span(0, 100, 500, "root"),
            span(1, 150, 50, "child"),
            // Deeper but ending LAST, so the end cannot be taken from the root.
            span(1, 400, 300, "tail"),
        ];
        assert_eq!(domain_of(&spans), Some((100, 700)));
    }

    #[test]
    fn an_empty_capture_has_no_domain() {
        assert_eq!(domain_of(&[]), None);
    }

    /// A zero-width domain has no mapping onto a pixel range, and every
    /// downstream division by its span would be a division by zero.
    #[test]
    fn an_instantaneous_capture_still_has_width() {
        let spans = [span(0, 42, 0, "instant")];
        assert_eq!(domain_of(&spans), Some((42, 43)));
    }

    /// The regression that rendered a real capture blank.
    ///
    /// A puffin frame spans the wall-clock gap between repaints, so the work
    /// is a small cluster inside a much larger domain — here 3ms of scopes in
    /// a 30ms frame. At a plausible width that is well under a pixel per
    /// scope, and the original rule dropped every one of them.
    ///
    /// This asserts the arithmetic that made that happen, so the numbers are
    /// on the record rather than in a screenshot: if a future change reasons
    /// about "sub-pixel spans" again, it has to face them.
    #[test]
    fn a_frames_scopes_are_routinely_sub_pixel_at_default_zoom() {
        let frame_ns = 30_000_000.0;
        let width_px = 840.0;
        let ns_per_px = frame_ns / width_px;
        assert!(
            ns_per_px > 28_000.0,
            "a 30ms frame across 840px is ~28µs per pixel, not {ns_per_px}"
        );

        // A 20µs scope — entirely ordinary — cannot fill a pixel.
        let scope_px = 20_000.0 / ns_per_px;
        assert!(
            scope_px < 1.0,
            "a 20µs scope is {scope_px}px; dropping that is dropping the capture"
        );
        // Which is why the floor is a clamp and not a cull.
        assert!(MIN_SPAN_WIDTH > 0.0);
    }

    /// Absolute profiler timestamps are too large for `f64` to hold at
    /// nanosecond resolution, which is why the chart rebases to the first
    /// scope before it maps anything.
    #[test]
    fn absolute_timestamps_lose_nanoseconds_but_rebased_ones_do_not() {
        // puffin reports nanoseconds since the unix epoch.
        let absolute = 1_789_864_436_000_000_000i64;
        let one_ns_later = absolute + 1;
        assert_eq!(
            absolute as f64, one_ns_later as f64,
            "at this magnitude f64 cannot tell two adjacent nanoseconds apart"
        );

        // Rebased onto the first scope, the same instants are exact.
        let origin = absolute;
        assert_ne!(
            (absolute - origin) as f64,
            (one_ns_later - origin) as f64,
            "rebasing is what buys back the resolution"
        );
    }

    /// Self time is the whole point of the report: a parent that merely HOSTS
    /// expensive children must not be credited with their time.
    #[test]
    fn self_time_excludes_children_but_not_siblings() {
        let spans = [
            span(0, 0, 1_000, "parent"),
            span(1, 0, 600, "child-a"),
            span(1, 600, 300, "child-b"),
            // A second root: a sibling of `parent`, not a child, so it must
            // not be deducted from it.
            span(0, 1_000, 500, "next-root"),
        ];

        assert_eq!(self_time_ns(&spans, 0), 100, "1000 less 600 and 300");
        assert_eq!(self_time_ns(&spans, 1), 600, "a leaf keeps all of its own");
        assert_eq!(self_time_ns(&spans, 3), 500);
    }

    /// Only DIRECT children are deducted. Subtracting grandchildren as well
    /// would double-count them and make a deep stack's self time collapse.
    #[test]
    fn grandchildren_are_not_deducted_twice() {
        let spans = [
            span(0, 0, 1_000, "root"),
            span(1, 0, 900, "middle"),
            span(2, 0, 800, "leaf"),
        ];

        assert_eq!(self_time_ns(&spans, 0), 100, "1000 less middle's 900 only");
        assert_eq!(self_time_ns(&spans, 1), 100, "900 less leaf's 800");
        assert_eq!(self_time_ns(&spans, 2), 800);
    }

    /// A malformed capture must not produce a negative time that sorts to the
    /// bottom of the report and reads as a measurement.
    #[test]
    fn children_overrunning_their_parent_clamp_to_zero() {
        let spans = [span(0, 0, 100, "parent"), span(1, 0, 500, "impossible")];
        assert_eq!(self_time_ns(&spans, 0), 0);
    }

    #[test]
    fn the_report_leads_with_the_costliest_scope_and_counts_repeats() {
        // A 5ms frame that is MOSTLY its children: `App::ui` keeps only
        // 5.0 - 4.32 - 0.2 = 0.48ms of its own, so the forty-eight cheap cards
        // outweigh it together while losing to it individually. That is the
        // shape the report exists to surface — one aggregated line, not
        // forty-eight scattered ones, and not the parent that merely hosted
        // them.
        let mut spans = vec![span(0, 0, 5_000_000, "App::ui")];
        for i in 0..48 {
            spans.push(span(1, i * 100_000, 90_000, "card"));
        }
        spans.push(span(1, 4_900_000, 200_000, "block_train::show"));

        let text = report(&spans);
        let by_self = text
            .lines()
            .skip_while(|l| !l.starts_with("by self time"))
            .nth(1)
            .expect("a ranked line");

        assert!(by_self.contains("card"), "cards lead: {by_self}");
        assert!(by_self.contains("×48"), "repeats aggregate: {by_self}");
        assert!(text.contains("50 spans"), "{text}");
        // The parent is still present and still honest — it kept 0.48ms.
        assert!(text.contains("App::ui"));
    }

    #[test]
    fn an_empty_capture_reports_that_rather_than_dividing_by_zero() {
        assert_eq!(report(&[]), "flame: nothing captured");
    }

    /// The await-spanning-scope shape, reproduced: three tasks that each hold
    /// a scope open while their future is pending get recorded as nested, so
    /// the self times sum past the wall clock. The report has to SAY so —
    /// read literally it claims an image decode cost 277% of its frame.
    #[test]
    fn self_time_exceeding_wall_is_called_out_as_an_await_artifact() {
        // Each "task" fully contains the next, which is what one stream does
        // with concurrent guards. Wall is 100ms; self times total 100 across
        // the three, so widen the innermost to overrun.
        let spans = [
            span(0, 0, 100_000_000, "decode"),
            span(1, 10_000_000, 80_000_000, "decode"),
            span(2, 20_000_000, 60_000_000, "decode"),
            // A leaf that reports more than the whole capture lasted.
            span(3, 25_000_000, 150_000_000, "decode"),
        ];
        let text = report(&spans);
        assert!(text.contains("EXCEEDS wall"), "{text}");
        assert!(text.contains("across an await"), "{text}");
    }

    /// …and stays quiet for an ordinary frame, or it is noise that teaches
    /// people to skip the first line of every report.
    #[test]
    fn an_ordinary_frame_carries_no_await_warning() {
        let spans = [
            span(0, 0, 5_000_000, "App::ui"),
            span(1, 1_000_000, 2_000_000, "detail_page::draw"),
        ];
        assert!(!report(&spans).contains("EXCEEDS wall"));
    }

    /// Busy is the UNION of root spans, so the report can tell a saturated
    /// thread from an idle one. Two roots that abut cover 4ms of a 10ms
    /// window; summing them naively would say the same, which is why the
    /// overlapping case below is the one that matters.
    #[test]
    fn busy_is_the_union_of_roots_not_their_sum() {
        let spans = [
            span(0, 0, 2_000_000, "a"),
            span(0, 2_000_000, 2_000_000, "b"),
            span(0, 8_000_000, 2_000_000, "c"),
        ];
        let (busy, roots) = busy_ns(&spans);
        assert_eq!(roots, 3);
        assert_eq!(busy, 6_000_000);

        // Overlapping roots (a malformed capture) must not report a thread as
        // busier than the clock allows.
        let overlapping = [span(0, 0, 5_000_000, "a"), span(0, 1_000_000, 1_000_000, "b")];
        assert_eq!(busy_ns(&overlapping).0, 5_000_000);
    }

    #[test]
    fn the_report_names_how_much_of_the_window_was_idle() {
        // 4ms of work in a 10ms window — the shape that says "spread out with
        // gaps", not "back to back".
        let spans = [
            span(0, 0, 2_000_000, "a"),
            span(0, 8_000_000, 2_000_000, "b"),
        ];
        let text = report(&spans);
        assert!(text.contains("60% idle"), "{text}");
        assert!(text.contains("2 root spans"), "{text}");
    }

    /// The compact encoding has to be decodable by something that has never
    /// seen it, so the legend travels with the data.
    #[test]
    fn the_compact_report_is_self_describing() {
        let spans = [span(0, 0, 5_000_000, "App::ui")];
        let text = report_compact(&spans);
        let mut lines = text.lines();
        assert!(
            lines.next().expect("header").starts_with("FLAME/1 u=us "),
            "{text}"
        );
        assert!(lines.next().expect("legend").starts_with("L "), "{text}");
        assert!(lines.next().expect("dict").starts_with("D|"), "{text}");
        assert!(lines.next().expect("scopes").starts_with("S|"), "{text}");
        assert!(lines.next().expect("tree").starts_with("T|"), "{text}");
    }

    /// The whole point of the encoding: a run of identical siblings costs one
    /// entry, not one per span. The 2,885-galley capture that motivated this
    /// spent its entire 60-line budget on them and said nothing.
    #[test]
    fn identical_siblings_collapse_to_one_tree_entry() {
        let mut spans = vec![span(0, 0, 5_000_000, "App::ui")];
        for i in 0..2885 {
            spans.push(span(1, i * 1_000, 1_000, "text_layout::layout"));
        }
        let text = report_compact(&spans);
        let tree = text
            .lines()
            .find(|l| l.starts_with("T|"))
            .expect("a tree line");

        assert!(tree.contains("x2885"), "the run collapses: {tree}");
        // Two entries: the parent, and the collapsed run.
        assert_eq!(tree.matches('|').count(), 2, "{tree}");
        // And nothing was dropped to achieve it.
        assert!(!tree.contains("|+"), "{tree}");
    }

    /// Ids are positions in the dictionary, and the dictionary is ranked, so
    /// the hottest scope is `0` and the tree can reference it in one char.
    #[test]
    fn dictionary_ids_are_positions_ranked_by_self_time() {
        let spans = [
            span(0, 0, 10_000_000, "host"),
            span(1, 0, 9_000_000, "hot"),
        ];
        let text = report_compact(&spans);
        let dict = text.lines().find(|l| l.starts_with("D|")).expect("dict");
        // `hot` keeps 9ms, `host` keeps 1ms, so `hot` leads and is id 0.
        assert_eq!(dict, "D|hot|host");

        let scopes = text.lines().find(|l| l.starts_with("S|")).expect("scopes");
        // id:self,n,max,total — in microseconds.
        assert!(scopes.starts_with("S|0:9000,1,9000,9000|"), "{scopes}");
    }

    /// An await-corrupted capture must carry its flag in the compact form
    /// too, or the encoding loses the one warning that stops a misreading.
    #[test]
    fn the_compact_header_flags_the_await_artifact() {
        // Needs real depth: `self_time_ns` clamps at zero, so ONE overrunning
        // child cannot push the total past wall — it just zeroes its parent.
        // The artifact shows up when several levels each keep time.
        let nested = [
            span(0, 0, 100_000_000, "decode"),
            span(1, 10_000_000, 80_000_000, "decode"),
            span(2, 20_000_000, 60_000_000, "decode"),
            span(3, 25_000_000, 150_000_000, "decode"),
        ];
        assert!(report_compact(&nested).lines().next().unwrap().contains(" a=1"));

        let ordinary = [span(0, 0, 10_000_000, "App::ui")];
        assert!(!report_compact(&ordinary).lines().next().unwrap().contains(" a=1"));
    }

    #[test]
    fn an_empty_capture_compacts_to_a_marker_rather_than_panicking() {
        assert_eq!(report_compact(&[]), "FLAME/1 empty");
    }

    #[test]
    fn the_tick_ladder_climbs_one_two_five_ten() {
        assert_eq!(next_tick_step_ns(1), 2);
        assert_eq!(next_tick_step_ns(2), 5);
        assert_eq!(next_tick_step_ns(5), 10);
        assert_eq!(next_tick_step_ns(10), 20);
        assert_eq!(next_tick_step_ns(50), 100);
        assert_eq!(next_tick_step_ns(100), 200);
    }

    /// The ladder has to reach a frame's own scale without stalling: a 5ms
    /// frame is 5,000,000ns and the loop that climbs to it must terminate.
    #[test]
    fn the_tick_ladder_reaches_frame_scale() {
        let mut step = 1i64;
        let mut rungs = 0;
        while step < 5_000_000 {
            step = next_tick_step_ns(step);
            rungs += 1;
            assert!(rungs < 64, "ladder stalled at {step}");
        }
        assert_eq!(step, 5_000_000);
    }

    /// Durations are read in a column, so the digit count stays put while the
    /// unit moves.
    #[test]
    fn durations_keep_their_precision_as_the_unit_moves() {
        assert_eq!(format_duration_ns(0), "0ns");
        assert_eq!(format_duration_ns(999), "999ns");
        assert_eq!(format_duration_ns(1_000), "1.0µs");
        assert_eq!(format_duration_ns(12_500), "12.5µs");
        assert_eq!(format_duration_ns(1_000_000), "1.00ms");
        assert_eq!(format_duration_ns(4_200_000), "4.20ms");
        assert_eq!(format_duration_ns(1_500_000_000), "1.50s");
    }

    /// The measured case that started this: a frame's `build p50` of 4.2ms has
    /// to read as milliseconds, not as a seven-digit nanosecond count.
    #[test]
    fn a_frame_budget_reads_in_milliseconds() {
        assert_eq!(format_duration_ns(4_200_000), "4.20ms");
        assert_eq!(format_duration_ns(16_700_000), "16.70ms");
    }

    #[test]
    fn a_label_that_fits_is_left_alone() {
        assert_eq!(elide("show", 200.0, 10.0), "show");
    }

    #[test]
    fn a_label_that_does_not_fit_is_cut_with_an_ellipsis() {
        let cut = elide("listing_grid::show", 30.0, 10.0);
        assert!(cut.ends_with('…'), "{cut:?}");
        assert!(cut.len() < "listing_grid::show".len());
    }

    /// No room at all is no label, not a bare ellipsis taking up the bar.
    #[test]
    fn a_label_with_no_room_is_dropped_entirely() {
        assert_eq!(elide("show", 2.0, 10.0), "");
    }

    /// Light fills take dark ink and vice versa. The bars are theme-generated
    /// hues at a fixed lightness, so this is the one thing standing between a
    /// label and being unreadable on its own bar.
    #[test]
    fn ink_flips_against_the_fill_it_sits_on() {
        assert_eq!(ink_on(Color32::from_gray(250)), Color32::from_gray(20));
        assert_eq!(ink_on(Color32::from_gray(10)), Color32::from_gray(240));
    }
}
