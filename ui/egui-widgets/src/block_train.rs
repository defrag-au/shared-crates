//! `BlockTrain` — recent blocks spaced by real time, with the wait since the last one growing at the right edge.
//!
//! ## The form
//!
//! - **Time, not index, on x.** Blocks arrive at random, so their spacing is
//!   irregular, and the irregularity is the truth about Cardano's tempo. An
//!   evenly spaced row would hide exactly the thing a reader is watching.
//! - **Height is fullness** against `maxBlockBodySize`, so the scale is bounded
//!   and needs no axis: the top rule is a full block, and a third faint rule
//!   marks what blocks in the window on screen actually average. Mainnet runs
//!   around 5% of the cap, so without that datum the bars hug the baseline and
//!   a quiet chain reads as a broken chart. Transaction counts are in the
//!   tooltip rather than a second colour channel.
//! - **Emphasis, not categories.** The newest block wears the accent and the
//!   rest recede, because the story is "that one just landed".
//! - **The gap is the wait.** The band from the newest block to "now" is the
//!   chain being waited on, and the ticking wait under its end marks "now".
//!   When the feed is quiet or offline the band says so in its tint.
//! - **The pulse is a status light.** The mark from [`crate::block_pulse`] sits
//!   in the top-left corner beside "full" rather than in a strip of its own, so
//!   the whole width goes to the blocks.
//!
//! ## Riders
//!
//! A [`TrainRider`] is a transaction the reader is waiting on. Each gets a
//! status row above the plot, left-aligned, with a leader from the end of its
//! words to its block, redrawn every frame so it tracks the block as it drifts.
//! The rows are the status on touch, where there is no hover.
//!
//! While [`RiderState::Waiting`] it rides the gap as an outline breathing like
//! `TxWatch`'s active stage. Once [`RiderState::InBlock`] its block stands in a
//! full-height column of the success colour with a cap on its bar, and a ring
//! spreads from the cap the moment it lands. [`RiderState::Beaten`] marks the
//! block where a competing transaction won. Hovering a rider's block, or its
//! row, leads the tooltip with the rider.
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
//! blocks snap. While live, the wait under the mark ticks in tenths, so the train
//! repaints ten times a second; it runs the frame clock only while a rider is
//! waiting or a ghost is falling, and under `MotionMode::None` snaps everything
//! and ticks in whole seconds.
//!
//! ## Labels
//!
//! The left names the newest block and when it began, rather than how far back
//! the span reaches: "20m ago" under a live chart reads as the opposite of live.
//! The right is the wait since that block, counting UP. Never a countdown to the
//! next one: arrivals are memoryless, so the expected wait is ~20 s at every
//! instant, and a countdown would reach zero and then be wrong a third of the
//! time.

use std::time::Duration;

use chain_heartbeat::{BlockBeat, Heartbeat};
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    pos2, vec2,
};

use crate::block_pulse::{PulseState, beat_lines, draw_mark, pop_progress};
use crate::motion::{Easing, forget, tween, tween_from};
use crate::theme::{Space, SpaceExt, Speed, TextSize, ThemeExt, line_height, with_alpha};
use crate::utils::{format_duration, format_number};

/// Repaint cadence while the wait is ticking in tenths.
const TENTH: Duration = Duration::from_millis(100);

/// The average gap between blocks, used ONLY to size bars. Never a claim about
/// when the next block will come.
const EXPECTED_GAP_SECS: f32 = 20.0;

const MIN_BAR_WIDTH: f32 = 2.0;
const MAX_BAR_WIDTH: f32 = 24.0;

/// An empty block is still a block.
const MIN_BAR_HEIGHT: f32 = 2.0;

/// Rider status rows above the plot. More riders than this show only the
/// newest; each still marks its own block.
const MAX_RIDER_ROWS: usize = 3;

/// The mark on top of a rider's bar, where its leader lands.
const CAP_RADIUS: f32 = 2.5;
const CAP_GAP: f32 = 4.0;

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
    /// In a block, but which one is not known yet. Keeps its status row, with
    /// no leader, rather than dropping off the train until the block is named.
    Landed,
    /// In the block at `height`, but a script rejected it: its collateral was
    /// taken, and nothing else it did happened.
    Failed { height: u64 },
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
    /// Heights riders were in last frame. `None` before the first frame, so a
    /// rider already landed when the train first draws does not pop.
    seen_riders: Option<Vec<u64>>,
    rider_pops: Vec<RiderPop>,
}

/// A rider that just landed, spreading its ring.
#[derive(Clone)]
struct RiderPop {
    height: u64,
    since: f64,
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
        // Drives the repaint cadence — 10 Hz while a block is live — so it is
        // routinely the suspect when a frontend idles hot. Scoped so the
        // question of whether it is ALSO expensive to draw can be answered
        // with a measurement rather than an argument.
        profiling::function_scope!();

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
        // Air between the baseline and the labels under it, so the figures do
        // not sit on the rule.
        let tick_gap = ui.space(Space::Sm);
        let rider_band = line_height(ui, label);
        // The pulse sits in the top-left corner, sized to the "full" label's line.
        let mark_radius = tick_band * 0.4;
        // A status row above the plot per rider, newest last, only while
        // something rides. An empty strip there reads as padding, which is most
        // of the time.
        let shown = &self.riders[self.riders.len().saturating_sub(MAX_RIDER_ROWS)..];
        let label_band = if shown.is_empty() {
            0.0
        } else {
            // A little air under the rows, so the last one does not sit on
            // "full" and the leaders have a visible run before the plot.
            shown.len() as f32 * rider_band + ui.space(Space::Sm)
        };

        let (rect, response) = ui.allocate_exact_size(
            vec2(
                ui.available_width(),
                label_band + self.plot_height + tick_gap + tick_band,
            ),
            Sense::hover(),
        );
        let plot = Rect::from_min_max(
            pos2(rect.left(), rect.top() + label_band),
            pos2(rect.right(), rect.bottom() - tick_band - tick_gap),
        );

        let mut hovered_height = None;
        if !ui.is_rect_visible(rect) || plot.width() <= 0.0 {
            return BlockTrainResponse {
                response,
                hovered_height,
            };
        }
        let painter = ui.painter_at(rect);

        // Scale: the baseline, the top rule that means "a full block", and a
        // third at what a block round here ACTUALLY carries, so the bars have
        // something to read against. Without it the honest absolute scale has
        // no datum between "empty" and "88 KB", and real blocks sit so low that
        // the chart reads as broken rather than as quiet.
        let rule = Stroke::new(1.0, c.border);
        painter.line_segment([plot.left_bottom(), plot.right_bottom()], rule);
        painter.line_segment(
            [plot.left_top(), plot.right_top()],
            Stroke::new(1.0, with_alpha(c.border, 110)),
        );
        // The mean over the window on screen, not a constant: it is a fact
        // about the chain right now and moves with it. Fainter than the other
        // two on purpose — a typical value, not a bound.
        if let Some(mean) = snapshot.window.mean_fullness {
            let y = plot.bottom() - mean.clamp(0.0, 1.0) * plot.height();
            painter.line_segment(
                [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                Stroke::new(1.0, with_alpha(c.text_muted, 70)),
            );
        }

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

        // A rider's block stands in a column the full height of the plot, so it
        // can be found at a glance at any zoom, however thin or short its bar.
        let corner = (bar_w / 2.0).min(2.0).round() as u8;
        let col_w = (bar_w * 3.0).max(8.0);
        for rider in self.riders {
            let (RiderState::InBlock { height }
            | RiderState::Beaten { height }
            | RiderState::Failed { height }) = rider.state
            else {
                continue;
            };
            let Some(p) = placed.iter().find(|p| p.beat.height == height) else {
                continue;
            };
            fill_unrounded(
                &painter,
                Rect::from_min_max(
                    pos2(p.x - col_w / 2.0, plot.top()),
                    pos2(p.x + col_w / 2.0, plot.bottom()),
                ),
                CornerRadius::ZERO,
                with_alpha(rider_mark(rider.state, &c), 34),
            );
        }

        // Bars.
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
        // A status light beside "full". Its own painter, clipped a little wider
        // than the train, so the arrival halo is not cut off at the edge.
        let mark_painter = ui
            .painter()
            .with_clip_rect(rect.expand(mark_radius * 3.0).intersect(ui.clip_rect()));
        draw_mark(
            &mark_painter,
            &theme,
            pos2(
                plot.left() + 2.0 + mark_radius,
                plot.top() + 1.0 + tick_band / 2.0,
            ),
            mark_radius,
            state,
            pop,
        );

        // Riders. Every rider's block wears a cap where its leader lands; a
        // waiting rider rides the wait as a breathing outline.
        let breath = breathing(ui);
        let waiting_x = plot.right() - bar_w / 2.0 - 2.0;
        let waiting_top = plot.bottom() - plot.height() * 0.5;
        if self.riders.iter().any(|r| r.state == RiderState::Waiting) {
            painter.rect_stroke(
                Rect::from_min_max(
                    pos2(waiting_x - bar_w / 2.0, waiting_top),
                    pos2(waiting_x + bar_w / 2.0, plot.bottom()),
                ),
                CornerRadius::same(corner),
                Stroke::new(1.5, with_alpha(c.accent, (breath * 255.0) as u8)),
                StrokeKind::Inside,
            );
        }
        for rider in self.riders {
            let (RiderState::InBlock { height }
            | RiderState::Beaten { height }
            | RiderState::Failed { height }) = rider.state
            else {
                continue;
            };
            if let Some(p) = placed.iter().find(|p| p.beat.height == height) {
                mark_painter.circle_filled(cap_center(p), CAP_RADIUS, rider_mark(rider.state, &c));
            }
        }

        // The moment a rider lands, a ring spreads from its cap, once. Blocks
        // arrive every ~20 s; the one carrying the reader's own transaction
        // should not look like the rest.
        let land_t = ui.input(|i| i.time);
        let land_fade = ui.duration(Speed::Slow) * 3.0;
        let landed_now: Vec<u64> = self
            .riders
            .iter()
            .filter_map(|r| match r.state {
                RiderState::InBlock { height } => Some(height),
                RiderState::Waiting
                | RiderState::Beaten { .. }
                | RiderState::Landed
                | RiderState::Failed { .. } => None,
            })
            .collect();
        let mut pops = memory.rider_pops.clone();
        // Not on first sight: a page opened on an already-landed transaction
        // did not just watch it land.
        if let Some(seen) = &memory.seen_riders
            && land_fade > 0.0
        {
            for height in landed_now.iter().filter(|h| !seen.contains(h)) {
                pops.push(RiderPop {
                    height: *height,
                    since: land_t,
                });
            }
        }
        pops.retain(|pop| ((land_t - pop.since) as f32) < land_fade);
        for pop in &pops {
            let Some(p) = placed.iter().find(|p| p.beat.height == pop.height) else {
                continue;
            };
            let progress = ((land_t - pop.since) as f32 / land_fade).clamp(0.0, 1.0);
            let spread = 1.0 - (1.0 - progress).powi(3);
            mark_painter.circle_stroke(
                cap_center(p),
                CAP_RADIUS + spread * 14.0,
                Stroke::new(1.5, with_alpha(c.success, ((1.0 - progress) * 230.0) as u8)),
            );
        }

        // A status row per rider, left-aligned above the plot, with a leader
        // from the end of its words to the block it rides. Redrawn every frame
        // from where the block is now, so the leader tracks it as the train
        // drifts. On touch, where there is no hover, these rows are the status.
        let rider_font = FontId::proportional(label);
        for (row, rider) in shown.iter().enumerate() {
            let mark = rider_mark(rider.state, &c);
            let row_top = rect.top() + row as f32 * rider_band;
            let target = match rider.state {
                RiderState::Waiting => Some(pos2(waiting_x, waiting_top - 2.0)),
                // Its block is not known yet: the row, and nothing to point at.
                RiderState::Landed => None,
                RiderState::InBlock { height }
                | RiderState::Beaten { height }
                | RiderState::Failed { height } => placed
                    .iter()
                    .find(|p| p.beat.height == height)
                    .map(cap_center),
            };
            let ink = if target.is_some() {
                c.text_primary
            } else {
                c.text_secondary
            };
            let end_x = rider_label(
                &painter,
                pos2(plot.left(), row_top),
                Align2::LEFT_TOP,
                rider_status(rider, tip_height, self.settled_depth),
                &rider_font,
                ink,
                mark,
            );
            if let Some(target) = target {
                let alpha = match rider.state {
                    RiderState::Waiting => (breath * 255.0) as u8,
                    RiderState::InBlock { .. }
                    | RiderState::Beaten { .. }
                    | RiderState::Landed
                    | RiderState::Failed { .. } => 200,
                };
                leader(
                    &mark_painter,
                    pos2(end_x + 6.0, row_top + rider_band / 2.0),
                    row_top + rider_band,
                    target,
                    Stroke::new(1.0, with_alpha(mark, alpha)),
                );
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

        // Labels: the newest block on the left, the wait since it on the right.
        let tick_font = FontId::proportional(tick);
        painter.text(
            // Right of the status light.
            plot.left_top() + vec2(2.0 + mark_radius * 2.0 + 4.0, 1.0),
            Align2::LEFT_TOP,
            "full",
            tick_font.clone(),
            c.text_muted,
        );
        // Before any block, a caption, so an empty box still says what it is.
        let (tip_text, tip_colour) = match snapshot.tip.as_ref() {
            Some(tip) => {
                let height = format_number(tip.height as i64);
                let text = match tip.block_time_unix {
                    Some(secs) => format!("Block {height} · {}", block_clock(secs)),
                    None => format!("Block {height}"),
                };
                (text, c.text_secondary)
            }
            None => ("Cardano blocks".to_string(), c.text_muted),
        };
        painter.text(
            pos2(plot.left(), rect.bottom()),
            Align2::LEFT_BOTTOM,
            tip_text,
            tick_font.clone(),
            tip_colour,
        );
        let precision = if ui.duration(Speed::Normal) > 0.0 {
            WaitPrecision::Tenths
        } else {
            WaitPrecision::Seconds
        };
        let wait = snapshot
            .tip
            .as_ref()
            .and_then(|tip| tip.block_time_unix)
            .map(|secs| format_wait(self.now_ms.saturating_sub(secs * 1000), precision));
        // A feed that is not simply live says so instead. The ticking figure is
        // monospace so its digits do not shuffle the label ten times a second.
        // Aligned to the train's right edge, where "now" is, not under the mark.
        let (clock_label, clock_font, clock_colour) = match (state.status(), wait) {
            // An empty train says it in the middle, in words. The one-word
            // status in the corner as well only doubled the puzzle.
            (Some(_), _) if placed.is_empty() => (String::new(), tick_font, c.text_secondary),
            (Some(_), _) => (corner_status(state), tick_font, c.text_secondary),
            (None, Some(wait)) => (wait, FontId::monospace(tick), c.text_primary),
            (None, None) => (String::new(), tick_font, c.text_secondary),
        };
        painter.text(
            pos2(plot.right(), rect.bottom()),
            Align2::RIGHT_BOTTOM,
            clock_label,
            clock_font,
            clock_colour,
        );

        if placed.is_empty() {
            painter.text(
                plot.center(),
                Align2::CENTER_CENTER,
                empty_message(state),
                FontId::proportional(label),
                c.text_muted,
            );
        } else if !matches!(state, PulseState::Live { .. }) {
            // Blocks are still on screen from before the feed went away. Say
            // what happened inside the wait band, where the missing blocks
            // would be, when it is wide enough to hold the sentence; a clipped
            // one would be worse than the corner word alone.
            let gap = Rect::from_min_max(pos2(gap_from, plot.top()), plot.right_bottom());
            let galley = painter.layout_no_wrap(
                empty_message(state).to_string(),
                FontId::proportional(label),
                c.text_muted,
            );
            if gap.width() >= galley.size().x + 16.0 {
                painter.galley(gap.center() - galley.size() / 2.0, galley, c.text_muted);
            }
        }

        // Hover: a status row, the nearest block within reach, or the gap. A
        // rider's block answers first and from further away, and its tooltip
        // leads with the rider, because that is what the reader came to check.
        if let Some(pointer) = response.hover_pos() {
            let is_rider = |p: &Placed<'_>| rider_colour(self.riders, p.beat.height, &c).is_some();
            let hovered_row = if pointer.y < plot.top() && rider_band > 0.0 {
                shown.get(((pointer.y - rect.top()).max(0.0) / rider_band) as usize)
            } else {
                None
            };
            let nearest = match hovered_row {
                Some(rider) => match rider.state {
                    RiderState::InBlock { height }
                    | RiderState::Beaten { height }
                    | RiderState::Failed { height } => {
                        placed.iter().find(|p| p.beat.height == height)
                    }
                    RiderState::Waiting | RiderState::Landed => None,
                },
                None => {
                    let reach = bar_w.max(12.0);
                    let rider_reach = reach.max(col_w / 2.0) + 4.0;
                    placed
                        .iter()
                        .filter(|p| {
                            let within = if is_rider(p) { rider_reach } else { reach };
                            (p.x - pointer.x).abs() <= within
                        })
                        .min_by(|a, b| {
                            is_rider(b)
                                .cmp(&is_rider(a))
                                .then((a.x - pointer.x).abs().total_cmp(&(b.x - pointer.x).abs()))
                        })
                }
            };
            if let Some(p) = nearest {
                hovered_height = Some(p.beat.height);
                painter.line_segment(
                    [pos2(p.x, plot.top()), pos2(p.x, plot.bottom())],
                    Stroke::new(1.0, with_alpha(c.text_primary, 90)),
                );
                let beat = p.beat.clone();
                let age = (now_secs as u64).saturating_sub(beat.block_time_unix.unwrap_or(0));
                let here: Vec<(Color32, String)> = self
                    .riders
                    .iter()
                    .filter(|r| {
                        matches!(
                            r.state,
                            RiderState::InBlock { height }
                                | RiderState::Beaten { height }
                                | RiderState::Failed { height }
                                if height == beat.height
                        )
                    })
                    .map(|r| {
                        (
                            rider_mark(r.state, &c),
                            rider_status(r, tip_height, self.settled_depth),
                        )
                    })
                    .collect();
                let _ = response.clone().on_hover_ui_at_pointer(|ui| {
                    for (mark, text) in &here {
                        rider_note(ui, *mark, text);
                    }
                    if !here.is_empty() {
                        ui.separator();
                    }
                    beat_lines(ui, &beat, Some(age), max_body);
                });
            } else if let Some(rider) = hovered_row {
                let mark = rider_mark(rider.state, &c);
                let text = rider_status(rider, tip_height, self.settled_depth);
                let detail = match rider.state {
                    RiderState::Waiting => {
                        "Accepted by a node, not in a block yet. It lands with a block, and \
                         blocks come about every 20 seconds, at random."
                    }
                    RiderState::Landed => {
                        "Confirmed in a block. Finding out which one, to put it on the train."
                    }
                    RiderState::InBlock { .. }
                    | RiderState::Beaten { .. }
                    | RiderState::Failed { .. } => "Its block has moved off the train.",
                };
                let _ = response.clone().on_hover_ui_at_pointer(|ui| {
                    rider_note(ui, mark, &text);
                    ui.label(
                        egui::RichText::new(detail)
                            .size(ui.text_size(TextSize::Sm))
                            .color(ui.tokens().color.text_secondary),
                    );
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
                    seen_riders: Some(landed_now.clone()),
                    rider_pops: pops.clone(),
                },
            )
        });

        // The bars drift left as time passes. Repaint just often enough that each
        // step moves them a fraction of a pixel, which the unrounded edges render
        // as continuous motion: about four frames a second at real speed on a
        // wide train, more only when the host clock runs faster. With motion off
        // the drift is still data, so it updates once a second.
        if snapshot.tip.is_some() {
            let drift = repaint_interval(drift_px_per_sec(plot.width(), span_secs, clock.rate));
            let interval = match (precision, state) {
                // The wait is ticking in tenths; each one has to be drawn.
                (WaitPrecision::Tenths, PulseState::Live { .. }) => drift.min(TENTH),
                (WaitPrecision::Tenths, _) => drift,
                (WaitPrecision::Seconds, _) => Duration::from_secs(1),
            };
            ctx.request_repaint_after(interval);
        }
        let waiting = self.riders.iter().any(|r| r.state == RiderState::Waiting);
        if (waiting && ui.duration(Speed::Slow) > 0.0) || !ghosts.is_empty() || !pops.is_empty() {
            ctx.request_repaint();
        }

        BlockTrainResponse {
            response,
            hovered_height,
        }
    }
}

/// What an empty train says, in words a reader who has never seen one can
/// follow. A bare "offline" in an empty box reads as a broken widget, not as a
/// feed that dropped.
fn empty_message(state: PulseState) -> &'static str {
    match state {
        PulseState::Offline { .. } => "Block feed offline · reconnecting",
        PulseState::NotStarted => "Connecting to the block feed…",
        PulseState::Quiet { .. } => "Block feed quiet · waiting to hear from it",
        PulseState::CatchingUp => "Catching up on recent blocks…",
        PulseState::AwaitingFirstBlock | PulseState::Live { .. } => "Waiting for the next block…",
    }
}

/// The corner word when the feed is not simply live. Names the FEED, because
/// "offline" under a row of blocks reads as the chain being offline.
fn corner_status(state: PulseState) -> String {
    match state {
        PulseState::Offline { .. } => "feed offline".to_string(),
        PulseState::Quiet { silent_secs } => format!("feed quiet {}", format_duration(silent_secs)),
        PulseState::NotStarted => "connecting".to_string(),
        PulseState::CatchingUp => "catching up".to_string(),
        PulseState::AwaitingFirstBlock | PulseState::Live { .. } => String::new(),
    }
}

/// How finely the wait since the last block is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitPrecision {
    /// Tenths, ticking: the chain visibly being waited on.
    Tenths,
    /// Whole seconds, under reduced motion, where a ticking decimal is motion.
    Seconds,
}

/// The wait since a block began: `26.4s`, then `1:02.4` past a minute.
fn format_wait(elapsed_ms: u64, precision: WaitPrecision) -> String {
    let secs = elapsed_ms / 1000;
    let tenths = (elapsed_ms % 1000) / 100;
    let minutes = secs / 60;
    let rem = secs % 60;
    match (precision, minutes) {
        (WaitPrecision::Tenths, 0) => format!("{secs}.{tenths}s"),
        (WaitPrecision::Seconds, 0) => format!("{secs}s"),
        (WaitPrecision::Tenths, _) => format!("{minutes}:{rem:02}.{tenths}"),
        (WaitPrecision::Seconds, _) => format!("{minutes}:{rem:02}"),
    }
}

/// `HH:MM:SS` of a unix time, shifted `offset_secs` east of UTC.
fn time_of_day(unix_secs: u64, offset_secs: i64) -> String {
    let tod = (unix_secs as i64 + offset_secs).rem_euclid(86_400);
    let (h, m, s) = (tod / 3600, tod % 3600 / 60, tod % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

/// When a block began, on the reader's clock. The browser knows the reader's
/// zone; natively there is no dependency-free way to ask, so it says UTC rather
/// than pass UTC off as local.
fn block_clock(unix_secs: u64) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(unix_secs as f64 * 1000.0));
        // Minutes WEST of UTC, so negated.
        let offset_secs = -(date.get_timezone_offset() as i64) * 60;
        time_of_day(unix_secs, offset_secs)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{} UTC", time_of_day(unix_secs, 0))
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
        RiderState::Failed { height: h } if h == height => Some(c.error),
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

/// The colour a rider's state wears.
fn rider_mark(state: RiderState, c: &crate::theme::ColorTokens) -> Color32 {
    match state {
        RiderState::Waiting => c.accent,
        RiderState::InBlock { .. } | RiderState::Landed => c.success,
        RiderState::Beaten { .. } => c.warning,
        RiderState::Failed { .. } => c.error,
    }
}

/// A rider's status, in words: the row above the plot and the tooltip's lead.
fn rider_status(rider: &TrainRider, tip_height: Option<u64>, settled_depth: u64) -> String {
    let label = &rider.label;
    match rider.state {
        RiderState::Waiting => format!("{label} · waiting for a block"),
        RiderState::Beaten { .. } => format!("{label} · beaten to it"),
        RiderState::Landed => format!("{label} · in a block"),
        RiderState::Failed { .. } => format!("{label} · failed in its block, collateral taken"),
        RiderState::InBlock { height } => {
            match tip_height.and_then(|tip| rider_depth(tip, height, settled_depth)) {
                Some(RiderDepth::Settled { depth }) => {
                    format!("{label} · settled, {depth} blocks deep")
                }
                Some(RiderDepth::Confirming { depth: 1 }) => {
                    format!("{label} · in the latest block")
                }
                Some(RiderDepth::Confirming { depth }) => {
                    format!("{label} · in a block, {depth} deep")
                }
                None => format!("{label} · in a block"),
            }
        }
    }
}

/// Where a rider's leader lands: just above its bar.
fn cap_center(p: &Placed<'_>) -> Pos2 {
    pos2(p.x, p.rect.top() - CAP_GAP)
}

/// An elbow from the end of a status row to the point it names: along the row,
/// then down. When the point sits under the words themselves it drops straight
/// from beneath the row instead, rather than striking through them.
fn leader(painter: &egui::Painter, start: Pos2, row_bottom: f32, target: Pos2, stroke: Stroke) {
    if target.x >= start.x {
        let elbow = pos2(target.x, start.y);
        painter.line_segment([start, elbow], stroke);
        painter.line_segment([elbow, target], stroke);
    } else {
        painter.line_segment([pos2(target.x, row_bottom), target], stroke);
    }
}

/// A rider's line in a tooltip: its dot, then its status in strong text.
fn rider_note(ui: &mut Ui, mark: Color32, text: &str) {
    ui.horizontal(|ui| {
        let size = ui.text_size(TextSize::Sm);
        let (dot, _) = ui.allocate_exact_size(vec2(size * 0.7, size), Sense::hover());
        ui.painter().circle_filled(dot.center(), size * 0.3, mark);
        ui.label(
            egui::RichText::new(text)
                .size(size)
                .strong()
                .color(ui.tokens().color.text_primary),
        );
    });
}

/// A rider's label: a dot in the rider's colour, then the words in a text
/// colour, so identity never rides on coloured text. Returns where the words
/// end, for the leader.
fn rider_label(
    painter: &egui::Painter,
    at: Pos2,
    anchor: Align2,
    text: String,
    font: &FontId,
    ink: Color32,
    mark: Color32,
) -> f32 {
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
    left + width
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
    fn the_wait_ticks_in_tenths_and_rolls_into_minutes() {
        assert_eq!(format_wait(0, WaitPrecision::Tenths), "0.0s");
        assert_eq!(format_wait(26_437, WaitPrecision::Tenths), "26.4s");
        assert_eq!(format_wait(62_450, WaitPrecision::Tenths), "1:02.4");
        // Reduced motion: no ticking decimal.
        assert_eq!(format_wait(26_937, WaitPrecision::Seconds), "26s");
        assert_eq!(format_wait(62_450, WaitPrecision::Seconds), "1:02");
    }

    #[test]
    fn time_of_day_applies_the_offset_across_midnight() {
        // 2026-04-30 16:24:51 UTC.
        let t = 1_777_566_291;
        assert_eq!(time_of_day(t, 0), "16:24:51");
        assert_eq!(time_of_day(t, 10 * 3600), "02:24:51");
        assert_eq!(time_of_day(t, -17 * 3600), "23:24:51");
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
