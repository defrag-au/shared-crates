//! `BlockTrain` — recent blocks spaced by real time, with the wait since the last one growing at the right edge.
//!
//! The macroquad twin of `egui_widgets::block_train`. Same form, same refusals;
//! see [`crate::block_pulse`] for the feed-state vocabulary both share.
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
//!   a quiet chain reads as a broken chart.
//! - **Emphasis, not categories.** The newest block wears the accent and the
//!   rest recede, because the story is "that one just landed".
//! - **The gap is the wait.** The band from the newest block to "now" is the
//!   chain being waited on, and the ticking figure under its end marks "now".
//!   When the feed is quiet or offline the band says so in its tint.
//! - **The pulse is a status light**, in the top-left corner beside "full",
//!   so the whole width goes to the blocks.
//!
//! ## Riders
//!
//! A [`TrainRider`] is a transaction the reader is waiting on. Each gets a
//! left-aligned status row above the plot with a leader from the end of its
//! words to its block, redrawn every frame so it tracks the block as it drifts.
//! [`RiderState::Waiting`] rides the gap as a breathing outline; once
//! [`RiderState::InBlock`] its block stands in a full-height column with a cap
//! on its bar, and a ring spreads from the cap the moment it lands.
//!
//! On a phone there is no hover, so **the rows are the status** — that is why
//! they exist, and why this port drops the egui twin's tooltips rather than
//! inventing a tooltip layer macroquad does not have.
//!
//! ## What the renderer gives us for free
//!
//! The egui twin carries a `ClockRate` smoother, a `repaint_interval` and a
//! `SUBPIXEL_STEP` budget, and draws through a `with_round_to_pixels(false)`
//! escape hatch. All four exist because egui repaints *reactively* and snaps
//! rects to whole pixels, so a drifting bar hops and judders. macroquad renders
//! every frame and `draw_rectangle` takes raw float coordinates. **Do not port
//! that machinery back in** — its absence here is correct, not an oversight.

use std::collections::HashMap;

use chain_heartbeat::{BlockBeat, Heartbeat, TrackedTx, TxProgress};
use macroquad::prelude::*;

use crate::block_pulse::{PulseState, PulseTicker, draw_mark, format_duration, format_number};
use crate::painter::Painter;
use crate::theme::{self, Theme};

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

/// How long a bar takes to grow in when it arrives at the tip.
const GROW_SECS: f32 = 0.30;
/// How long an orphaned block takes to fall away.
const GHOST_SECS: f64 = 0.9;
/// How long a rider's landing ring takes to spread and fade.
const LAND_SECS: f64 = 1.35;
/// One breath of a waiting rider's outline.
const BREATH_SECS: f32 = 1.4;

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

/// Tracked transactions as this train draws them.
///
/// The twin of `chain_live::tracker::riders`. The TRACKING itself —  what is
/// waiting, what a block means, what a rollback undoes — lives once, in
/// [`chain_heartbeat::Tracker`]; only this projection is per-renderer, because
/// only the rider type differs. If the two projections ever disagree, one of
/// them is wrong: they render the same [`TxProgress`].
pub fn riders(txs: &[TrackedTx]) -> Vec<TrainRider> {
    txs.iter()
        .filter_map(|tracked| {
            let state = match tracked.progress {
                TxProgress::Waiting => RiderState::Waiting,
                TxProgress::Landed {
                    block_height: Some(height),
                } => RiderState::InBlock { height },
                // Still a status row while its block is found: dropping it
                // there is what made the status vanish between "waiting" and
                // "in a block".
                TxProgress::Landed { block_height: None } => RiderState::Landed,
                TxProgress::FailedInBlock { block_height } => RiderState::Failed {
                    height: block_height,
                },
                // Given up on: no status worth a row, and no block to sit on.
                TxProgress::Dropped => return None,
            };
            Some(TrainRider::new(tracked.label.clone(), state))
        })
        .collect()
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

impl RiderState {
    /// The block this rider names, if it names one.
    fn height(self) -> Option<u64> {
        match self {
            RiderState::InBlock { height }
            | RiderState::Beaten { height }
            | RiderState::Failed { height } => Some(height),
            RiderState::Waiting | RiderState::Landed => None,
        }
    }
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

/// What was drawn last frame, so arrivals grow and rollbacks fall.
///
/// Not a widget: it holds state across frames, which the charter forbids inside
/// a widget precisely so it lives somewhere named. Hold one per train.
#[derive(Clone, Debug, Default)]
pub struct TrainState {
    pulse: PulseTicker,
    /// Animated bar height by block hash.
    bars: HashMap<String, f32>,
    seen_tip_height: Option<u64>,
    seen_bars: Vec<SeenBar>,
    ghosts: Vec<Ghost>,
    /// Heights riders were in last frame. `None` before the first frame, so a
    /// rider already landed when the train first draws does not pop.
    seen_riders: Option<Vec<u64>>,
    rider_pops: Vec<RiderPop>,
}

impl TrainState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug)]
struct SeenBar {
    hash: String,
    height: u64,
    block_secs: u64,
    height_px: f32,
}

#[derive(Clone, Debug)]
struct Ghost {
    block_secs: u64,
    height_px: f32,
    since: f64,
}

#[derive(Clone, Debug)]
struct RiderPop {
    height: u64,
    since: f64,
}

/// One placed block: where it sits and how tall it is drawn right now.
struct Placed<'b> {
    beat: &'b BlockBeat,
    x: f32,
    rect: Rect,
    target: f32,
}

/// Everything the train renders, projected by the host.
pub struct BlockTrainVm<'a> {
    pub heartbeat: &'a Heartbeat,
    /// Unix milliseconds, from the host's clock.
    pub now_ms: u64,
    pub riders: &'a [TrainRider],
    /// How far back the train reaches. At least a minute.
    pub span_secs: f32,
    /// Height of the bar area, excluding labels.
    pub plot_height: f32,
    pub settled_depth: u64,
}

impl<'a> BlockTrainVm<'a> {
    pub fn new(heartbeat: &'a Heartbeat, now_ms: u64) -> Self {
        Self {
            heartbeat,
            now_ms,
            riders: &[],
            span_secs: 20.0 * 60.0,
            plot_height: 56.0,
            settled_depth: DEFAULT_SETTLED_DEPTH,
        }
    }

    pub fn riders(mut self, riders: &'a [TrainRider]) -> Self {
        self.riders = riders;
        self
    }

    pub fn span_secs(mut self, secs: f32) -> Self {
        self.span_secs = secs.max(60.0);
        self
    }

    pub fn plot_height(mut self, height: f32) -> Self {
        self.plot_height = height;
        self
    }

    pub fn settled_depth(mut self, depth: u64) -> Self {
        self.settled_depth = depth.max(1);
        self
    }
}

pub struct TrainResponse {
    /// Bottom y after rendering — hosts stack content below.
    pub bottom: f32,
    /// The block the reader tapped, if any.
    pub tapped_height: Option<u64>,
}

/// Draw the train in the column `[x, x + w]`, starting at top `y`.
pub fn block_train(
    p: &Painter,
    vm: &BlockTrainVm<'_>,
    state: &mut TrainState,
    x: f32,
    y: f32,
    w: f32,
) -> TrainResponse {
    let t = &p.theme;
    let snapshot = vm.heartbeat.snapshot(vm.now_ms);
    let feed = PulseState::of(&snapshot);
    let now_secs = vm.now_ms as f64 / 1000.0;
    let now_t = get_time();
    let dt = get_frame_time();
    let span_secs = vm.span_secs.max(60.0);

    let tick = 12.0_f32;
    let label = 14.0_f32;
    let tick_band = tick * 1.35;
    let tick_gap = 6.0;
    let rider_band = label * 1.35;
    let mark_radius = tick_band * 0.4;

    // A status row above the plot per rider, newest last, only while something
    // rides. An empty strip there reads as padding, which is most of the time.
    let shown = &vm.riders[vm.riders.len().saturating_sub(MAX_RIDER_ROWS)..];
    let label_band = if shown.is_empty() {
        0.0
    } else {
        shown.len() as f32 * rider_band + 6.0
    };

    let plot = Rect::new(x, y + label_band, w, vm.plot_height);
    let bottom = plot.bottom() + tick_gap + tick_band;
    let mut tapped_height = None;
    if w <= 0.0 {
        return TrainResponse {
            bottom,
            tapped_height,
        };
    }

    // Scale: the baseline, and the top rule that means "a full block".
    draw_line(
        plot.left(),
        plot.bottom(),
        plot.right(),
        plot.bottom(),
        1.0,
        t.track,
    );
    draw_line(
        plot.left(),
        plot.top(),
        plot.right(),
        plot.top(),
        1.0,
        theme::with_alpha(t.track, 0.45),
    );
    // A third rule at what a block round here ACTUALLY carries, so the bars
    // have something to read against. Without it the honest absolute scale has
    // no datum between "empty" and "88 KB", and real blocks sit so low that the
    // chart reads as broken rather than as quiet.
    //
    // The mean over the window on screen, not a constant: the figure is a fact
    // about the chain right now and moves with it. Fainter than the baseline
    // and the full-block rule on purpose — it is a typical value, not a bound.
    if let Some(mean) = snapshot.window.mean_fullness {
        let y = plot.bottom() - mean.clamp(0.0, 1.0) * plot.h;
        draw_line(
            plot.left(),
            y,
            plot.right(),
            y,
            1.0,
            theme::with_alpha(t.muted, 0.30),
        );
    }

    let max_body = vm.heartbeat.network().max_block_body_bytes();
    let bar_w = bar_width(plot.w, span_secs);
    let tip_height = snapshot.tip.as_ref().map(|b| b.height);
    let tip_hash = snapshot.tip.as_ref().map(|b| b.hash.as_str());

    // Place every block inside the span, animating arrivals up from the
    // baseline. A replay, or the chain as first seen, snaps into place.
    let mut placed: Vec<Placed> = Vec::new();
    for beat in vm.heartbeat.beats() {
        let Some(block_secs) = beat.block_time_unix else {
            continue;
        };
        let Some(bx) = x_at(plot, now_secs, span_secs, block_secs) else {
            continue;
        };
        let fullness = (beat.body_size as f32 / max_body.max(1) as f32).clamp(0.0, 1.0);
        let target = (fullness * plot.h).max(MIN_BAR_HEIGHT);
        let arrived =
            state.seen_tip_height.is_some_and(|seen| beat.height > seen) && feed.is_live();
        let current =
            state
                .bars
                .entry(beat.hash.clone())
                .or_insert(if arrived { 0.0 } else { target });
        ease_to(current, target, GROW_SECS, dt);
        let height = *current;
        placed.push(Placed {
            beat,
            x: bx,
            rect: Rect::new(bx - bar_w / 2.0, plot.bottom() - height, bar_w, height),
            target,
        });
    }

    // A rider's block stands in a column the full height of the plot, so it can
    // be found at a glance however thin or short its bar.
    let col_w = (bar_w * 3.0).max(8.0);
    for rider in vm.riders {
        let Some(height) = rider.state.height() else {
            continue;
        };
        let Some(pl) = placed.iter().find(|pl| pl.beat.height == height) else {
            continue;
        };
        draw_rectangle(
            pl.x - col_w / 2.0,
            plot.top(),
            col_w,
            plot.h,
            theme::with_alpha(rider_mark(rider.state, t), 0.13),
        );
    }

    // Bars.
    for pl in &placed {
        let colour = rider_colour(vm.riders, pl.beat.height, t).unwrap_or({
            if Some(pl.beat.hash.as_str()) == tip_hash {
                t.accent
            } else {
                theme::with_alpha(t.muted, 0.6)
            }
        });
        draw_rectangle(pl.rect.x, pl.rect.y, pl.rect.w, pl.rect.h, colour);
    }

    // The wait: from the newest block to now.
    let gap_from = placed
        .last()
        .map(|pl| pl.rect.right() + 1.0)
        .unwrap_or(plot.left());
    if gap_from < plot.right() {
        let tint = match feed {
            PulseState::Quiet { .. } => theme::with_alpha(t.warn, 0.10),
            PulseState::Offline { .. } | PulseState::NotStarted => theme::with_alpha(t.muted, 0.07),
            _ => theme::with_alpha(t.panel, 0.55),
        };
        draw_rectangle(gap_from, plot.top(), plot.right() - gap_from, plot.h, tint);
    }
    draw_line(
        plot.right(),
        plot.top(),
        plot.right(),
        plot.bottom(),
        1.0,
        t.track,
    );

    // A status light beside "full", so the full width goes to the blocks.
    let pop = state.pulse.progress(snapshot.tip.as_ref(), feed, now_t);
    draw_mark(
        t,
        vec2(
            plot.left() + 2.0 + mark_radius,
            plot.top() + 1.0 + tick_band / 2.0,
        ),
        mark_radius,
        feed,
        pop,
    );

    // A waiting rider rides the wait as a breathing outline.
    let breath = breathing(now_t);
    let waiting_x = plot.right() - bar_w / 2.0 - 2.0;
    let waiting_top = plot.bottom() - plot.h * 0.5;
    if vm.riders.iter().any(|r| r.state == RiderState::Waiting) {
        draw_rectangle_lines(
            waiting_x - bar_w / 2.0,
            waiting_top,
            bar_w,
            plot.bottom() - waiting_top,
            1.5,
            theme::with_alpha(t.accent, breath),
        );
    }

    // Every rider's block wears a cap where its leader lands.
    for rider in vm.riders {
        let Some(height) = rider.state.height() else {
            continue;
        };
        if let Some(pl) = placed.iter().find(|pl| pl.beat.height == height) {
            let c = cap_center(pl);
            draw_circle(c.x, c.y, CAP_RADIUS, rider_mark(rider.state, t));
        }
    }

    // The moment a rider lands, a ring spreads from its cap, once. Blocks arrive
    // every ~20 s; the one carrying the reader's own transaction should not look
    // like the rest.
    let landed_now: Vec<u64> = vm
        .riders
        .iter()
        .filter_map(|r| match r.state {
            RiderState::InBlock { height } => Some(height),
            _ => None,
        })
        .collect();
    // Not on first sight: a page opened on an already-landed transaction did not
    // just watch it land.
    if let Some(seen) = &state.seen_riders {
        for height in landed_now.iter().filter(|h| !seen.contains(h)) {
            state.rider_pops.push(RiderPop {
                height: *height,
                since: now_t,
            });
        }
    }
    state
        .rider_pops
        .retain(|pop| (now_t - pop.since) < LAND_SECS);
    for pop in &state.rider_pops {
        let Some(pl) = placed.iter().find(|pl| pl.beat.height == pop.height) else {
            continue;
        };
        let progress = ((now_t - pop.since) / LAND_SECS).clamp(0.0, 1.0) as f32;
        let spread = 1.0 - (1.0 - progress).powi(3);
        let c = cap_center(pl);
        draw_circle_lines(
            c.x,
            c.y,
            CAP_RADIUS + spread * 14.0,
            1.5,
            theme::with_alpha(t.success, 1.0 - progress),
        );
    }

    // A status row per rider, with a leader from the end of its words to the
    // block it rides, redrawn every frame so it tracks the drift.
    for (row, rider) in shown.iter().enumerate() {
        let mark = rider_mark(rider.state, t);
        let row_top = y + row as f32 * rider_band;
        let target = match rider.state {
            RiderState::Waiting => Some(vec2(waiting_x, waiting_top - 2.0)),
            // Its block is not known yet: the row, and nothing to point at.
            RiderState::Landed => None,
            _ => rider
                .state
                .height()
                .and_then(|h| placed.iter().find(|pl| pl.beat.height == h))
                .map(cap_center),
        };
        let ink = if target.is_some() { t.fg } else { t.muted };
        let text = rider_status(rider, tip_height, vm.settled_depth);
        let end_x = rider_label(p, plot.left(), row_top, &text, label, ink, mark);
        if let Some(target) = target {
            let alpha = match rider.state {
                RiderState::Waiting => breath,
                _ => 0.8,
            };
            leader(
                vec2(end_x + 6.0, row_top + rider_band / 2.0),
                row_top + rider_band,
                target,
                theme::with_alpha(mark, alpha),
            );
        }
    }

    // Rollback ghosts: blocks drawn last frame that have left the chain.
    let lowest_placed = placed.first().map(|pl| pl.beat.height);
    for seen in &state.seen_bars {
        if placed.iter().any(|pl| pl.beat.hash == seen.hash) {
            continue;
        }
        state.bars.remove(&seen.hash);
        // Scrolled off the left edge, or trimmed from history below everything
        // still drawn: not a rollback.
        let scrolled_out = x_at(plot, now_secs, span_secs, seen.block_secs).is_none();
        let trimmed = lowest_placed.is_some_and(|low| seen.height < low);
        if !scrolled_out && !trimmed {
            state.ghosts.push(Ghost {
                block_secs: seen.block_secs,
                height_px: seen.height_px,
                since: now_t,
            });
        }
    }
    state.ghosts.retain(|g| (now_t - g.since) < GHOST_SECS);
    for ghost in &state.ghosts {
        let Some(gx) = x_at(plot, now_secs, span_secs, ghost.block_secs) else {
            continue;
        };
        let progress = ((now_t - ghost.since) / GHOST_SECS).clamp(0.0, 1.0) as f32;
        let drop = progress * plot.h * 0.35;
        let top = (plot.bottom() - ghost.height_px + drop).max(plot.top());
        let h = (plot.bottom() + drop - top).min(plot.bottom() - top);
        if h > 0.0 {
            draw_rectangle(
                gx - bar_w / 2.0,
                top,
                bar_w,
                h,
                theme::with_alpha(t.danger, (1.0 - progress) * 0.8),
            );
        }
    }

    // Labels: the newest block on the left, the wait since it on the right.
    p.text(
        "full",
        plot.left() + 2.0 + mark_radius * 2.0 + 4.0,
        p.top_baseline(plot.top() + 1.0, tick),
        tick,
        theme::with_alpha(t.muted, 0.8),
    );
    // Before any block, a caption, so an empty box still says what it is.
    let (tip_text, tip_colour) = match snapshot.tip.as_ref() {
        Some(tip) => {
            let height = format_number(tip.height);
            let text = match tip.block_time_unix {
                Some(secs) => format!("Block {height} · {}", block_clock(secs)),
                None => format!("Block {height}"),
            };
            (text, t.muted)
        }
        None => (
            "Cardano blocks".to_string(),
            theme::with_alpha(t.muted, 0.8),
        ),
    };
    let tick_baseline = p.top_baseline(plot.bottom() + tick_gap, tick);
    p.text(&tip_text, plot.left(), tick_baseline, tick, tip_colour);

    // The wait since the tip, counting UP. Never a countdown to the next block:
    // arrivals are memoryless, so the expected wait is ~20 s at every instant,
    // and a countdown would reach zero and then be wrong a third of the time.
    // Monospace so the digits do not shuffle ten times a second.
    let wait = snapshot
        .tip
        .as_ref()
        .and_then(|tip| tip.block_time_unix)
        .map(|secs| format_wait(vm.now_ms.saturating_sub(secs * 1000)));
    match (feed.status(), wait) {
        // An empty train says it in the middle, in words. The one-word status in
        // the corner as well only doubled the puzzle.
        (Some(_), _) if placed.is_empty() => {}
        (Some(_), _) => {
            let text = corner_status(feed);
            let dim = p.measure(&text, tick);
            p.text(
                &text,
                plot.right() - dim.width,
                tick_baseline,
                tick,
                t.muted,
            );
        }
        (None, Some(text)) => {
            let dim = p.measure(&text, tick);
            p.mono(&text, plot.right() - dim.width, tick_baseline, tick, t.fg);
        }
        (None, None) => {}
    }

    // What an empty train says, in words a reader who has never seen one can
    // follow. A bare "offline" in an empty box reads as a broken widget.
    if placed.is_empty() {
        let text = empty_message(feed);
        let dim = p.measure(text, label);
        p.text(
            text,
            plot.left() + (plot.w - dim.width) / 2.0,
            p.centre_baseline(plot.top(), plot.h, label),
            label,
            theme::with_alpha(t.muted, 0.8),
        );
    } else if !feed.is_live() {
        // Blocks are still on screen from before the feed went away. Say what
        // happened inside the wait band, where the missing blocks would be, when
        // it is wide enough to hold the sentence; a clipped one would be worse
        // than the corner word alone.
        let text = empty_message(feed);
        let dim = p.measure(text, label);
        let gap_w = plot.right() - gap_from;
        if gap_w >= dim.width + 16.0 {
            p.text(
                text,
                gap_from + (gap_w - dim.width) / 2.0,
                p.centre_baseline(plot.top(), plot.h, label),
                label,
                theme::with_alpha(t.muted, 0.8),
            );
        }
    }

    // Tap: the nearest block within reach. A rider's block answers first and
    // from further away, because that is what the reader came to check.
    let reach = bar_w.max(12.0);
    let rider_reach = reach.max(col_w / 2.0) + 4.0;
    for pl in &placed {
        let is_rider = rider_colour(vm.riders, pl.beat.height, t).is_some();
        let within = if is_rider { rider_reach } else { reach };
        let hit = Rect::new(pl.x - within, plot.top(), within * 2.0, plot.h);
        if p.tapped(hit) {
            tapped_height = Some(pl.beat.height);
            if is_rider {
                break;
            }
        }
    }

    state.seen_tip_height = tip_height.or(state.seen_tip_height);
    state.seen_bars = placed
        .iter()
        .map(|pl| SeenBar {
            hash: pl.beat.hash.clone(),
            height: pl.beat.height,
            block_secs: pl.beat.block_time_unix.unwrap_or(0),
            height_px: pl.target,
        })
        .collect();
    state.seen_riders = Some(landed_now);

    TrainResponse {
        bottom,
        tapped_height,
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Move `current` towards `target`, covering most of the distance in `secs`.
///
/// Frame-rate independent by construction: the step is derived from `dt`, so a
/// 30 Hz frame and a 144 Hz frame travel the same distance per second.
fn ease_to(current: &mut f32, target: f32, secs: f32, dt: f32) {
    if secs <= 0.0 || dt <= 0.0 {
        *current = target;
        return;
    }
    let k = 1.0 - (-dt / (secs / 3.0)).exp();
    *current += (target - *current) * k.clamp(0.0, 1.0);
}

/// Where a block that began at `block_secs` sits, or `None` once it is older
/// than the span. Now is the right edge.
fn x_at(plot: Rect, now_secs: f64, span_secs: f32, block_secs: u64) -> Option<f32> {
    let age = (now_secs - block_secs as f64) as f32;
    if age > span_secs {
        return None;
    }
    Some(plot.right() - (age.max(0.0) / span_secs) * plot.w)
}

/// A bar about half the average spacing, within bounds.
fn bar_width(plot_width: f32, span_secs: f32) -> f32 {
    let expected_blocks = (span_secs / EXPECTED_GAP_SECS).max(1.0);
    (plot_width / expected_blocks * 0.55).clamp(MIN_BAR_WIDTH, MAX_BAR_WIDTH)
}

/// The wait since a block began: `26.4s`, then `1:02.4` past a minute.
///
/// Always in tenths: macroquad draws every frame, so there is no reduced-motion
/// path where a ticking decimal would itself be the motion.
fn format_wait(elapsed_ms: u64) -> String {
    let secs = elapsed_ms / 1000;
    let tenths = (elapsed_ms % 1000) / 100;
    let minutes = secs / 60;
    let rem = secs % 60;
    if minutes == 0 {
        format!("{secs}.{tenths}s")
    } else {
        format!("{minutes}:{rem:02}.{tenths}")
    }
}

/// `HH:MM:SS` of a unix time, shifted `offset_secs` east of UTC.
fn time_of_day(unix_secs: u64, offset_secs: i64) -> String {
    let tod = (unix_secs as i64 + offset_secs).rem_euclid(86_400);
    let (h, m, s) = (tod / 3600, tod % 3600 / 60, tod % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

/// When a block began. UTC on both targets: miniquad has no way to ask the
/// reader's timezone (there is no `js_sys` here), and saying UTC beats passing
/// UTC off as local time.
fn block_clock(unix_secs: u64) -> String {
    format!("{} UTC", time_of_day(unix_secs, 0))
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

fn empty_message(state: PulseState) -> &'static str {
    match state {
        PulseState::Offline { .. } => "Block feed offline · reconnecting",
        PulseState::NotStarted => "Connecting to the block feed...",
        PulseState::Quiet { .. } => "Block feed quiet · waiting to hear from it",
        PulseState::CatchingUp => "Catching up on recent blocks...",
        PulseState::AwaitingFirstBlock | PulseState::Live { .. } => "Waiting for the next block...",
    }
}

/// The colour a rider's state wears.
fn rider_mark(state: RiderState, t: &Theme) -> Color {
    match state {
        RiderState::Waiting => t.accent,
        RiderState::InBlock { .. } | RiderState::Landed => t.success,
        RiderState::Beaten { .. } => t.warn,
        RiderState::Failed { .. } => t.danger,
    }
}

fn rider_colour(riders: &[TrainRider], height: u64, t: &Theme) -> Option<Color> {
    riders.iter().find_map(|r| match r.state {
        RiderState::InBlock { height: h } if h == height => Some(t.success),
        RiderState::Beaten { height: h } if h == height => Some(t.warn),
        RiderState::Failed { height: h } if h == height => Some(t.danger),
        _ => None,
    })
}

/// A rider's status, in words.
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

/// The same breath a waiting transaction wears elsewhere: one cycle per
/// [`BREATH_SECS`], never dimmer than a pending mark.
fn breathing(now: f64) -> f32 {
    let wave = 0.5 - 0.5 * ((now as f32 / BREATH_SECS) * std::f32::consts::TAU).cos();
    0.35 + 0.65 * wave
}

/// Where a rider's leader lands: just above its bar.
fn cap_center(pl: &Placed<'_>) -> Vec2 {
    vec2(pl.x, pl.rect.top() - CAP_GAP)
}

/// An elbow from the end of a status row to the point it names: along the row,
/// then down. When the point sits under the words themselves it drops straight
/// from beneath the row instead, rather than striking through them.
fn leader(start: Vec2, row_bottom: f32, target: Vec2, color: Color) {
    if target.x >= start.x {
        let elbow = vec2(target.x, start.y);
        draw_line(start.x, start.y, elbow.x, elbow.y, 1.0, color);
        draw_line(elbow.x, elbow.y, target.x, target.y, 1.0, color);
    } else {
        draw_line(target.x, row_bottom, target.x, target.y, 1.0, color);
    }
}

/// A rider's label: a dot in the rider's colour, then the words in a text
/// colour, so identity never rides on coloured text. Returns where the words
/// end, for the leader.
fn rider_label(
    p: &Painter,
    x: f32,
    top: f32,
    text: &str,
    size: f32,
    ink: Color,
    mark: Color,
) -> f32 {
    let dot = size * 0.3;
    let gap = size * 0.4;
    let band = size * 1.35;
    draw_circle(x + dot, top + band / 2.0, dot, mark);
    let text_x = x + dot * 2.0 + gap;
    p.text(text, text_x, p.centre_baseline(top, band, size), size, ink);
    text_x + p.measure(text, size).width
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Rect {
        Rect::new(0.0, 0.0, 600.0, 50.0)
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
    fn the_wait_ticks_in_tenths_and_rolls_into_minutes() {
        assert_eq!(format_wait(0), "0.0s");
        assert_eq!(format_wait(26_437), "26.4s");
        assert_eq!(format_wait(62_450), "1:02.4");
    }

    #[test]
    fn time_of_day_applies_the_offset_across_midnight() {
        // 2026-04-30 16:24:51 UTC.
        let t = 1_777_566_291;
        assert_eq!(time_of_day(t, 0), "16:24:51");
        assert_eq!(time_of_day(t, 10 * 3600), "02:24:51");
        assert_eq!(time_of_day(t, -17 * 3600), "23:24:51");
        assert_eq!(block_clock(t), "16:24:51 UTC");
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

    #[test]
    fn a_bar_eases_towards_its_target_and_never_overshoots() {
        let mut h = 0.0;
        for _ in 0..60 {
            ease_to(&mut h, 40.0, GROW_SECS, 1.0 / 60.0);
            assert!(h <= 40.0, "overshot: {h}");
        }
        assert!(h > 39.0, "should have arrived: {h}");
        // A zero frame time must not divide by zero or stall.
        let mut snap = 0.0;
        ease_to(&mut snap, 12.0, GROW_SECS, 0.0);
        assert_eq!(snap, 12.0);
    }

    #[test]
    fn only_a_rider_that_names_a_block_can_be_pointed_at() {
        assert_eq!(RiderState::Waiting.height(), None);
        assert_eq!(RiderState::Landed.height(), None);
        assert_eq!(RiderState::InBlock { height: 7 }.height(), Some(7));
        assert_eq!(RiderState::Failed { height: 9 }.height(), Some(9));
    }
}
