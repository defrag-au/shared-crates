//! `BlockTrain` — recent blocks spaced by real time, with the wait since the last one growing at the right edge.
//!
//! ## The form
//!
//! - **Time, not index, on x.** Blocks arrive at random, so their spacing is
//!   irregular, and the irregularity is the truth about Cardano's tempo. An
//!   evenly spaced row would hide exactly the thing a reader is watching.
//! - **Height is fullness** against `maxBlockBodySize`, so the scale is bounded
//!   and needs no axis: the top rule is a full block. Transaction counts are in
//!   the tooltip rather than a second colour channel.
//! - **Emphasis, not categories.** The newest block wears the accent and the
//!   rest recede, because the story is "that one just landed".
//! - **The gap is the wait.** The band from the newest block to "now" is the
//!   chain being waited on, and the pulse mark from [`crate::block_pulse`] sits
//!   at its end. When the feed is quiet or offline the band says so in its tint.
//!
//! ## Riders
//!
//! A [`TrainRider`] is a transaction the reader is waiting on. While
//! [`RiderState::Waiting`] it rides the gap as an outline breathing like
//! `TxWatch`'s active stage. Once [`RiderState::InBlock`] its block turns the
//! success colour and a bracket runs from it to the tip: every block to its
//! right is one more confirmation, so depth is something the reader can count.
//! [`RiderState::Beaten`] marks the block where a competing transaction won.
//!
//! ## Rollbacks
//!
//! A block that leaves the chain falls away tinted, over [`Speed::Slow`], rather
//! than vanishing between two frames where nobody could notice it happened.
//! Blocks that merely scroll off the left edge just go.
//!
//! ## Motion and cost
//!
//! Arrivals at the tip grow from the baseline on [`Speed::Normal`]; replayed
//! blocks snap. The train asks for one repaint a second for its clock, runs the
//! frame clock only while a rider is waiting or a ghost is falling, and under
//! `MotionMode::None` snaps everything.

use std::time::Duration;

use chain_heartbeat::{BlockBeat, Heartbeat};
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    pos2, vec2,
};

use crate::block_pulse::{PulseState, beat_lines, draw_mark, pop_progress};
use crate::motion::{Easing, forget, tween, tween_from};
use crate::theme::{Speed, TextSize, ThemeExt, line_height, with_alpha};
use crate::utils::format_duration;

/// The average gap between blocks, used ONLY to size bars. Never a claim about
/// when the next block will come.
const EXPECTED_GAP_SECS: f32 = 20.0;

const MIN_BAR_WIDTH: f32 = 2.0;
const MAX_BAR_WIDTH: f32 = 24.0;

/// An empty block is still a block.
const MIN_BAR_HEIGHT: f32 = 2.0;

/// Depth at which a rider's block is called settled, unless the host says
/// otherwise. Matches the reorg buffer the minting engine confirms behind.
pub const DEFAULT_SETTLED_DEPTH: u64 = 3;

/// A transaction the reader is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrainRider {
    /// What it does, in the reader's words: "your swap", "your buy".
    pub label: String,
    pub state: RiderState,
}

impl TrainRider {
    pub fn new(label: impl Into<String>, state: RiderState) -> Self {
        Self {
            label: label.into(),
            state,
        }
    }
}

/// Where a rider has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiderState {
    /// Accepted by a node, not yet in a block.
    Waiting,
    /// In the block at `height`.
    InBlock { height: u64 },
    /// A competing transaction took what this one needed, in the block at
    /// `height`.
    Beaten { height: u64 },
}

/// How deep a rider's block sits under the tip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiderDepth {
    Confirming { depth: u64 },
    Settled { depth: u64 },
}

/// The depth of the block at `block_height`, counting that block as one.
/// `None` when the block is above the tip (the heartbeat has not seen it yet).
pub fn rider_depth(tip_height: u64, block_height: u64, settled_depth: u64) -> Option<RiderDepth> {
    let depth = tip_height.checked_sub(block_height)? + 1;
    Some(if depth >= settled_depth {
        RiderDepth::Settled { depth }
    } else {
        RiderDepth::Confirming { depth }
    })
}

/// What the train reports back.
pub struct BlockTrainResponse {
    pub response: Response,
    /// The block under the pointer, if any.
    pub hovered_height: Option<u64>,
}

/// The train. Build per frame from the host's heartbeat.
pub struct BlockTrain<'a> {
    heartbeat: &'a Heartbeat,
    now_ms: u64,
    span: Duration,
    plot_height: f32,
    riders: &'a [TrainRider],
    settled_depth: u64,
    id_salt: Id,
}

/// Remembered between frames: what was drawn, for arrivals and rollbacks, and
/// how fast the host's clock is running.
#[derive(Clone, Default)]
struct TrainMemory {
    seen_tip_height: Option<u64>,
    bars: Vec<SeenBar>,
    ghosts: Vec<Ghost>,
    clock: ClockRate,
}

/// How many seconds of host clock pass per second of frame time, smoothed.
///
/// Usually 1. A replay, a simulation or a test harness can run the host clock
/// faster, and the drift the train must keep smooth scales with it, so the
/// repaint interval is derived from what the clock actually does rather than
/// assumed.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ClockRate {
    last: Option<(f64, f64)>,
    rate: f64,
}

impl Default for ClockRate {
    fn default() -> Self {
        Self {
            last: None,
            rate: 1.0,
        }
    }
}

impl ClockRate {
    /// Fold in one frame: `frame_secs` from egui's input, `host_secs` from the
    /// host's `now_ms`.
    fn observe(&mut self, frame_secs: f64, host_secs: f64) {
        if let Some((last_frame, last_host)) = self.last {
            let frame_delta = frame_secs - last_frame;
            if frame_delta > 1e-3 {
                let observed = ((host_secs - last_host) / frame_delta).clamp(0.0, 1_000.0);
                self.rate = self.rate * 0.7 + observed * 0.3;
            }
        }
        self.last = Some((frame_secs, host_secs));
    }
}

#[derive(Clone)]
struct SeenBar {
    hash: String,
    height: u64,
    block_secs: u64,
    height_px: f32,
}

#[derive(Clone)]
struct Ghost {
    block_secs: u64,
    height_px: f32,
    since: f64,
}

struct Placed<'b> {
    beat: &'b BlockBeat,
    x: f32,
    rect: Rect,
    target: f32,
}

impl<'a> BlockTrain<'a> {
    /// `now_ms` is unix milliseconds, from the host's clock.
    pub fn new(heartbeat: &'a Heartbeat, now_ms: u64) -> Self {
        Self {
            heartbeat,
            now_ms,
            span: Duration::from_secs(20 * 60),
            plot_height: 56.0,
            riders: &[],
            settled_depth: DEFAULT_SETTLED_DEPTH,
            id_salt: Id::new("block_train"),
        }
    }

    /// How far back the train reaches. At least a minute.
    pub fn span(mut self, span: Duration) -> Self {
        self.span = span;
        self
    }

    /// Height of the bar area in points, excluding labels.
    pub fn plot_height(mut self, height: f32) -> Self {
        self.plot_height = height;
        self
    }

    pub fn riders(mut self, riders: &'a [TrainRider]) -> Self {
        self.riders = riders;
        self
    }

    pub fn settled_depth(mut self, depth: u64) -> Self {
        self.settled_depth = depth.max(1);
        self
    }

    /// Distinguish two trains in one `Ui`.
    pub fn id_salt(mut self, salt: impl egui::AsIdSalt) -> Self {
        self.id_salt = Id::NULL.with(salt);
        self
    }

    pub fn show(self, ui: &mut Ui) -> BlockTrainResponse {
        let theme = ui.tokens();
        let c = theme.color;
        let ctx = ui.ctx().clone();
        let id = ui.id().with(self.id_salt);
        let snapshot = self.heartbeat.snapshot(self.now_ms);
        let state = PulseState::of(&snapshot);
        let now_secs = self.now_ms as f64 / 1000.0;
        let span_secs = self.span.as_secs_f32().max(60.0);

        let tick = ui.text_size(TextSize::Xs);
        let label = ui.text_size(TextSize::Sm);
        let tick_band = line_height(ui, tick);
        let rider_band = line_height(ui, label);
        let mark_band = rider_band * 1.6;

        let (rect, response) = ui.allocate_exact_size(
            vec2(
                ui.available_width(),
                rider_band + self.plot_height + tick_band,
            ),
            Sense::hover(),
        );
        let plot = Rect::from_min_max(
            pos2(rect.left(), rect.top() + rider_band),
            pos2(rect.right() - mark_band, rect.bottom() - tick_band),
        );

        let mut hovered_height = None;
        if !ui.is_rect_visible(rect) || plot.width() <= 0.0 {
            return BlockTrainResponse {
                response,
                hovered_height,
            };
        }
        let painter = ui.painter_at(rect);

        // Scale: the baseline, and the top rule that means "a full block".
        let rule = Stroke::new(1.0, c.border);
        painter.line_segment([plot.left_bottom(), plot.right_bottom()], rule);
        painter.line_segment(
            [plot.left_top(), plot.right_top()],
            Stroke::new(1.0, with_alpha(c.border, 110)),
        );

        let memory: TrainMemory = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
        let mut clock = memory.clock;
        clock.observe(ui.input(|i| i.time), now_secs);
        let max_body = self.heartbeat.network().max_block_body_bytes();
        let bar_w = bar_width(plot.width(), span_secs);
        let grow = ui.duration(Speed::Normal);
        let easing = ui.easing(Easing::OutCubic);
        let travel = ui.travel_allowed();
        let tip_height = snapshot.tip.as_ref().map(|b| b.height);
        let tip_hash = snapshot.tip.as_ref().map(|b| b.hash.as_str());

        // Place every block inside the span.
        let mut placed: Vec<Placed> = Vec::new();
        for beat in self.heartbeat.beats() {
            let Some(block_secs) = beat.block_time_unix else {
                continue;
            };
            let Some(x) = x_at(plot, now_secs, span_secs, block_secs) else {
                continue;
            };
            let fullness = (beat.body_size as f32 / max_body.max(1) as f32).clamp(0.0, 1.0);
            let target = (fullness * plot.height()).max(MIN_BAR_HEIGHT);
            let key = id.with(("bar", beat.hash.as_str()));
            // Grows only when it arrived at the tip while live. A replay, or
            // the chain as first seen, snaps into place.
            let arrived = memory
                .seen_tip_height
                .is_some_and(|seen| beat.height > seen)
                && matches!(state, PulseState::Live { .. });
            let height = if arrived {
                tween_from(&ctx, key, 0.0, target, grow, easing)
            } else {
                tween(&ctx, key, target, grow, easing)
            };
            placed.push(Placed {
                beat,
                x,
                rect: Rect::from_min_max(
                    pos2(x - bar_w / 2.0, plot.bottom() - height),
                    pos2(x + bar_w / 2.0, plot.bottom()),
                ),
                target,
            });
        }

        // Bars.
        let corner = (bar_w / 2.0).min(2.0).round() as u8;
        for p in &placed {
            let colour = rider_colour(self.riders, p.beat.height, &c).unwrap_or(
                if Some(p.beat.hash.as_str()) == tip_hash {
                    c.accent
                } else {
                    with_alpha(c.text_muted, 150)
                },
            );
            fill_unrounded(
                &painter,
                p.rect,
                CornerRadius {
                    nw: corner,
                    ne: corner,
                    sw: 0,
                    se: 0,
                },
                colour,
            );
        }

        // The wait: from the newest block to now.
        let gap_from = placed
            .last()
            .map(|p| p.rect.right() + 1.0)
            .unwrap_or(plot.left());
        if gap_from < plot.right() {
            let tint = match state {
                PulseState::Quiet { .. } => with_alpha(c.warning, 26),
                PulseState::Offline { .. } | PulseState::NotStarted => with_alpha(c.text_muted, 16),
                _ => with_alpha(c.bg_highlight, 120),
            };
            fill_unrounded(
                &painter,
                Rect::from_min_max(pos2(gap_from, plot.top()), plot.right_bottom()),
                CornerRadius::ZERO,
                tint,
            );
        }
        painter.line_segment([plot.right_top(), plot.right_bottom()], rule);
        let pop = pop_progress(&ctx, id.with("pop"), snapshot.tip.as_ref(), state);
        draw_mark(
            &painter,
            &theme,
            pos2(plot.right() + mark_band / 2.0, plot.center().y),
            mark_band * 0.28,
            state,
            pop,
        );

        // Riders.
        let tip_x = placed.last().map(|p| p.x);
        let breath = breathing(ui);
        let rider_font = FontId::proportional(label);
        for rider in self.riders {
            match rider.state {
                RiderState::Waiting => {
                    let x = plot.right() - bar_w / 2.0 - 2.0;
                    let outline = Rect::from_min_max(
                        pos2(x - bar_w / 2.0, plot.bottom() - plot.height() * 0.5),
                        pos2(x + bar_w / 2.0, plot.bottom()),
                    );
                    painter.rect_stroke(
                        outline,
                        CornerRadius::same(corner),
                        Stroke::new(1.5, with_alpha(c.accent, (breath * 255.0) as u8)),
                        StrokeKind::Inside,
                    );
                    rider_label(
                        &painter,
                        pos2(plot.right(), rect.top()),
                        Align2::RIGHT_TOP,
                        format!("{} · waiting", rider.label),
                        &rider_font,
                        c.text_primary,
                        c.accent,
                    );
                }
                RiderState::InBlock { height } | RiderState::Beaten { height } => {
                    let beaten = matches!(rider.state, RiderState::Beaten { .. });
                    let mark = if beaten { c.warning } else { c.success };
                    let text = if beaten {
                        format!("{} · beaten to it", rider.label)
                    } else {
                        match tip_height
                            .and_then(|tip| rider_depth(tip, height, self.settled_depth))
                        {
                            Some(RiderDepth::Settled { .. }) => {
                                format!("{} · settled", rider.label)
                            }
                            Some(RiderDepth::Confirming { depth: 1 }) => {
                                format!("{} · in the latest block", rider.label)
                            }
                            Some(RiderDepth::Confirming { depth }) => {
                                format!("{} · {depth} deep", rider.label)
                            }
                            None => format!("{} · in a block", rider.label),
                        }
                    };
                    match placed.iter().find(|p| p.beat.height == height) {
                        Some(p) => {
                            if !beaten && let Some(tip_x) = tip_x {
                                let y = plot.top() - 3.0;
                                painter.line_segment(
                                    [pos2(p.x, y), pos2(tip_x, y)],
                                    Stroke::new(1.0, with_alpha(c.success, 170)),
                                );
                            }
                            let anchor = if p.x > plot.center().x {
                                Align2::RIGHT_TOP
                            } else {
                                Align2::LEFT_TOP
                            };
                            rider_label(
                                &painter,
                                pos2(p.x, rect.top()),
                                anchor,
                                text,
                                &rider_font,
                                c.text_primary,
                                mark,
                            );
                        }
                        None => rider_label(
                            &painter,
                            pos2(plot.left(), rect.top()),
                            Align2::LEFT_TOP,
                            text,
                            &rider_font,
                            c.text_secondary,
                            mark,
                        ),
                    }
                }
            }
        }

        // Rollback ghosts: blocks drawn last frame that have left the chain.
        let now_t = ui.input(|i| i.time);
        let fade = ui.duration(Speed::Slow) * 2.0;
        let mut ghosts = memory.ghosts.clone();
        let lowest_placed = placed.first().map(|p| p.beat.height);
        for seen in &memory.bars {
            if placed.iter().any(|p| p.beat.hash == seen.hash) {
                continue;
            }
            forget(&ctx, id.with(("bar", seen.hash.as_str())));
            // Scrolled off the left edge, or trimmed from history below
            // everything still drawn: not a rollback.
            let scrolled_out = x_at(plot, now_secs, span_secs, seen.block_secs).is_none();
            let trimmed = lowest_placed.is_some_and(|low| seen.height < low);
            if !scrolled_out && !trimmed && fade > 0.0 {
                ghosts.push(Ghost {
                    block_secs: seen.block_secs,
                    height_px: seen.height_px,
                    since: now_t,
                });
            }
        }
        ghosts.retain(|g| ((now_t - g.since) as f32) < fade);
        for ghost in &ghosts {
            let Some(x) = x_at(plot, now_secs, span_secs, ghost.block_secs) else {
                continue;
            };
            let progress = ((now_t - ghost.since) as f32 / fade).clamp(0.0, 1.0);
            let drop = if travel {
                progress * plot.height() * 0.35
            } else {
                0.0
            };
            let fallen = Rect::from_min_max(
                pos2(x - bar_w / 2.0, plot.bottom() - ghost.height_px + drop),
                pos2(x + bar_w / 2.0, plot.bottom() + drop),
            )
            .intersect(plot);
            fill_unrounded(
                &painter,
                fallen,
                CornerRadius::ZERO,
                with_alpha(c.error, ((1.0 - progress) * 200.0) as u8),
            );
        }

        // Labels: how far back the span reaches, and the clock under the mark.
        let tick_font = FontId::proportional(tick);
        painter.text(
            plot.left_top() + vec2(2.0, 1.0),
            Align2::LEFT_TOP,
            "full",
            tick_font.clone(),
            c.text_muted,
        );
        painter.text(
            pos2(plot.left(), rect.bottom()),
            Align2::LEFT_BOTTOM,
            format!("{} ago", format_duration(span_secs as u64)),
            tick_font.clone(),
            c.text_muted,
        );
        let clock_label = state.status().unwrap_or_else(|| {
            snapshot
                .secs_since_block
                .map(format_duration)
                .unwrap_or_default()
        });
        painter.text(
            pos2(rect.right(), rect.bottom()),
            Align2::RIGHT_BOTTOM,
            clock_label,
            tick_font,
            c.text_secondary,
        );

        if placed.is_empty() {
            painter.text(
                plot.center(),
                Align2::CENTER_CENTER,
                state
                    .status()
                    .unwrap_or_else(|| "waiting for the next block".to_string()),
                FontId::proportional(label),
                c.text_muted,
            );
        }

        // Hover: the nearest block within reach, or the gap itself.
        if let Some(pointer) = response.hover_pos() {
            let reach = bar_w.max(12.0);
            let nearest = placed
                .iter()
                .filter(|p| (p.x - pointer.x).abs() <= reach)
                .min_by(|a, b| (a.x - pointer.x).abs().total_cmp(&(b.x - pointer.x).abs()));
            if let Some(p) = nearest {
                hovered_height = Some(p.beat.height);
                painter.line_segment(
                    [pos2(p.x, plot.top()), pos2(p.x, plot.bottom())],
                    Stroke::new(1.0, with_alpha(c.text_primary, 90)),
                );
                let beat = p.beat.clone();
                let age = (now_secs as u64).saturating_sub(beat.block_time_unix.unwrap_or(0));
                let notes: Vec<String> = self
                    .riders
                    .iter()
                    .filter_map(|r| match r.state {
                        RiderState::InBlock { height } if height == beat.height => {
                            Some(format!("{} is in this block", r.label))
                        }
                        RiderState::Beaten { height } if height == beat.height => {
                            Some(format!("{} lost the race in this block", r.label))
                        }
                        _ => None,
                    })
                    .collect();
                let _ = response.clone().on_hover_ui_at_pointer(|ui| {
                    beat_lines(ui, &beat, Some(age), max_body);
                    for note in &notes {
                        ui.label(
                            egui::RichText::new(note)
                                .size(ui.text_size(TextSize::Sm))
                                .color(ui.tokens().color.text_primary),
                        );
                    }
                });
            } else if pointer.x >= gap_from {
                let _ = response.clone().on_hover_ui_at_pointer(|ui| {
                    ui.label(
                        egui::RichText::new(state.explanation())
                            .size(ui.text_size(TextSize::Sm))
                            .color(ui.tokens().color.text_secondary),
                    );
                });
            }
        }

        ctx.data_mut(|d| {
            d.insert_temp(
                id,
                TrainMemory {
                    seen_tip_height: tip_height.or(memory.seen_tip_height),
                    clock,
                    bars: placed
                        .iter()
                        .map(|p| SeenBar {
                            hash: p.beat.hash.clone(),
                            height: p.beat.height,
                            block_secs: p.beat.block_time_unix.unwrap_or(0),
                            height_px: p.target,
                        })
                        .collect(),
                    ghosts: ghosts.clone(),
                },
            )
        });

        // The bars drift left as time passes. Repaint just often enough that each
        // step moves them a fraction of a pixel, which the unrounded edges render
        // as continuous motion: about four frames a second at real speed on a
        // wide train, more only when the host clock runs faster. With motion off
        // the drift is still data, so it updates once a second.
        if snapshot.tip.is_some() {
            let interval = if ui.duration(Speed::Normal) > 0.0 {
                repaint_interval(drift_px_per_sec(plot.width(), span_secs, clock.rate))
            } else {
                Duration::from_secs(1)
            };
            ctx.request_repaint_after(interval);
        }
        let waiting = self.riders.iter().any(|r| r.state == RiderState::Waiting);
        if (waiting && ui.duration(Speed::Slow) > 0.0) || !ghosts.is_empty() {
            ctx.request_repaint();
        }

        BlockTrainResponse {
            response,
            hovered_height,
        }
    }
}

/// Where a block that began at `block_secs` sits, or `None` once it is older
/// than the span. Now is the right edge.
fn x_at(plot: Rect, now_secs: f64, span_secs: f32, block_secs: u64) -> Option<f32> {
    let age = (now_secs - block_secs as f64) as f32;
    if age > span_secs {
        return None;
    }
    Some(plot.right() - (age.max(0.0) / span_secs) * plot.width())
}

/// How far a drifting step may move the bars, in pixels.
const SUBPIXEL_STEP: f32 = 0.25;

/// Pixels the bars drift per second of frame time.
fn drift_px_per_sec(plot_width: f32, span_secs: f32, clock_rate: f64) -> f32 {
    if span_secs <= 0.0 {
        return 0.0;
    }
    plot_width / span_secs * clock_rate as f32
}

/// The longest wait between repaints that keeps each drift step within
/// [`SUBPIXEL_STEP`], never faster than a 60 Hz frame and never slower than the
/// once-a-second clock.
fn repaint_interval(drift_px_per_sec: f32) -> Duration {
    if drift_px_per_sec <= 0.0 || !drift_px_per_sec.is_finite() {
        return Duration::from_secs(1);
    }
    Duration::from_secs_f32((SUBPIXEL_STEP / drift_px_per_sec).clamp(1.0 / 60.0, 1.0))
}

/// A filled rect that is NOT snapped to the pixel grid.
///
/// egui rounds rects to whole pixels by default, which is right for chrome and
/// wrong for anything that drifts: a bar moving a pixel a second hops a whole
/// pixel at a time, each on its own schedule, and the train judders however
/// often it repaints. Left unrounded, the feathered edge carries the sub-pixel
/// position and the drift reads as motion.
fn fill_unrounded(painter: &egui::Painter, rect: Rect, corner: CornerRadius, colour: Color32) {
    painter.add(egui::epaint::RectShape::filled(rect, corner, colour).with_round_to_pixels(false));
}

/// A bar about half the average spacing, within bounds.
fn bar_width(plot_width: f32, span_secs: f32) -> f32 {
    let expected_blocks = (span_secs / EXPECTED_GAP_SECS).max(1.0);
    (plot_width / expected_blocks * 0.55).clamp(MIN_BAR_WIDTH, MAX_BAR_WIDTH)
}

fn rider_colour(
    riders: &[TrainRider],
    height: u64,
    c: &crate::theme::ColorTokens,
) -> Option<Color32> {
    riders.iter().find_map(|r| match r.state {
        RiderState::InBlock { height: h } if h == height => Some(c.success),
        RiderState::Beaten { height: h } if h == height => Some(c.warning),
        _ => None,
    })
}

/// The same breath as `TxWatch`'s active stage: one cycle per [`Speed::Slow`],
/// never dimmer than a pending mark, and flat when motion is off.
fn breathing(ui: &Ui) -> f32 {
    let period = ui.duration(Speed::Slow);
    if period <= 0.0 {
        return 1.0;
    }
    let t = ui.input(|i| i.time) as f32;
    let wave = 0.5 - 0.5 * (t / period * std::f32::consts::TAU).cos();
    0.35 + 0.65 * wave
}

/// A rider's label: a dot in the rider's colour, then the words in a text
/// colour, so identity never rides on coloured text.
fn rider_label(
    painter: &egui::Painter,
    at: Pos2,
    anchor: Align2,
    text: String,
    font: &FontId,
    ink: Color32,
    mark: Color32,
) {
    let galley = painter.layout_no_wrap(text, font.clone(), ink);
    let dot = font.size * 0.3;
    let gap = font.size * 0.4;
    let width = dot * 2.0 + gap + galley.size().x;
    let left = match anchor.x() {
        egui::Align::Max => at.x - width,
        egui::Align::Center => at.x - width / 2.0,
        egui::Align::Min => at.x,
    };
    let center_y = at.y + galley.size().y / 2.0;
    painter.circle_filled(pos2(left + dot, center_y), dot, mark);
    painter.galley(pos2(left + dot * 2.0 + gap, at.y), galley, ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(600.0, 50.0))
    }

    #[test]
    fn now_is_the_right_edge_and_the_span_is_the_left() {
        let now = 1_000_000.0;
        assert_eq!(x_at(plot(), now, 1200.0, 1_000_000), Some(600.0));
        assert_eq!(x_at(plot(), now, 1200.0, 1_000_000 - 600), Some(300.0));
        assert_eq!(x_at(plot(), now, 1200.0, 1_000_000 - 1200), Some(0.0));
        assert_eq!(x_at(plot(), now, 1200.0, 1_000_000 - 1201), None);
        // A block stamped a moment in the future (clock skew) pins to now.
        assert_eq!(x_at(plot(), now, 1200.0, 1_000_005), Some(600.0));
    }

    #[test]
    fn bars_stay_readable_at_any_zoom() {
        assert_eq!(bar_width(600.0, 60.0), MAX_BAR_WIDTH);
        assert_eq!(bar_width(200.0, 86_400.0), MIN_BAR_WIDTH);
        let twenty_minutes = bar_width(600.0, 1200.0);
        assert!((5.0..6.0).contains(&twenty_minutes), "{twenty_minutes}");
    }

    #[test]
    fn repaints_follow_the_drift_not_a_fixed_tick() {
        // A 20-minute train 1,270 px wide at real speed drifts ~1 px/s: a
        // quarter-pixel step every quarter second.
        let real = repaint_interval(drift_px_per_sec(1270.0, 1200.0, 1.0));
        assert!((0.2..0.3).contains(&real.as_secs_f32()), "{real:?}");
        // A replay at 30x wants every frame, and no more than every frame.
        let fast = repaint_interval(drift_px_per_sec(1270.0, 1200.0, 30.0));
        assert!((fast.as_secs_f32() - 1.0 / 60.0).abs() < 1e-4, "{fast:?}");
        // A day on a narrow card barely moves: the once-a-second clock.
        let slow = repaint_interval(drift_px_per_sec(200.0, 86_400.0, 1.0));
        assert_eq!(slow, Duration::from_secs(1));
        // A stopped clock never spins.
        assert_eq!(repaint_interval(0.0), Duration::from_secs(1));
    }

    #[test]
    fn the_clock_rate_tracks_the_host() {
        let mut clock = ClockRate::default();
        for frame in 0..40 {
            let t = frame as f64 * 0.1;
            clock.observe(t, 1_000.0 + t * 30.0);
        }
        assert!((clock.rate - 30.0).abs() < 0.1, "{}", clock.rate);
        // Two frames at the same instant do not divide by zero.
        clock.observe(3.9, 2_000.0);
        assert!(clock.rate.is_finite());
    }

    #[test]
    fn depth_counts_the_riders_own_block() {
        assert_eq!(
            rider_depth(100, 100, 3),
            Some(RiderDepth::Confirming { depth: 1 })
        );
        assert_eq!(
            rider_depth(101, 100, 3),
            Some(RiderDepth::Confirming { depth: 2 })
        );
        assert_eq!(
            rider_depth(102, 100, 3),
            Some(RiderDepth::Settled { depth: 3 })
        );
        // Not seen yet: no claim.
        assert_eq!(rider_depth(99, 100, 3), None);
    }
}
