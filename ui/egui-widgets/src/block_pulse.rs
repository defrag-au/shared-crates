//! `BlockPulse` — the chain's heartbeat on one line: a dot that pops on each new block, a ring filling towards "a block has probably landed", and the time since the last one.
//!
//! Built for an app shell's status strip, so every surface reads as alive
//! without a chart. It reads a [`chain_heartbeat::Heartbeat`] the host keeps
//! current, and never changes it.
//!
//! ## What each part means, and what it refuses to mean
//!
//! - **The dot pops only on a block at the tip.** A reconnect replaying the
//!   blocks it missed is catch-up, not activity, and pops nothing. Nor does the
//!   block already on screen when the widget first appears.
//! - **The ring is a likelihood, never a countdown.** Block production is
//!   memoryless: the chance that a block has landed rises with time, but the
//!   chain is never "due". The ring fills towards certainty and never claims a
//!   deadline, and it is drawn only while the feed is following at the tip.
//! - **A quiet feed is not a quiet chain.** When the host has heard nothing for
//!   a while the dot goes hollow and the widget says the feed is quiet, so a
//!   stretching gap can never read as a slow chain.
//!
//! ## Cost
//!
//! Between blocks nothing changes faster than the seconds counter, so the
//! widget asks for one repaint a second. The pop is a short ease on
//! [`Speed::Slow`], shrinks to a fade under reduced motion, and does not happen
//! at all under `MotionMode::None`.
//!
//! [`PulseState`], [`draw_mark`] and [`beat_lines`] are shared with
//! [`crate::block_train`], so the two widgets can never disagree about what the
//! feed is doing.

use std::time::Duration;

use chain_heartbeat::{BlockBeat, FeedHealth, Heartbeat, HeartbeatSnapshot, SyncState};
use egui::{Align, Id, Layout, Painter, Pos2, Response, RichText, Sense, Shape, Stroke, Ui, Vec2};

use crate::motion::Easing;
use crate::theme::{Space, SpaceExt, Speed, TextSize, Theme, ThemeExt, line_height, with_alpha};
use crate::utils::{format_duration, format_number, truncate_hex};

/// The ring's radius as a fraction of the row height.
const MARK_RADIUS: f32 = 0.36;

/// The dot's radius as a fraction of the ring's.
const DOT_FRACTION: f32 = 0.45;

/// How much a pulse shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PulseDetail {
    /// The mark and the time since the last block. For a crowded strip.
    Compact,
    /// Also the block height, and a word about the feed when it is not simply
    /// following.
    #[default]
    Full,
}

/// What the feed lets a chain widget say, decided once per frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PulseState {
    /// Following at the tip. `due` is the chance a block has landed by now.
    Live {
        due: f32,
    },
    /// Following, but no block has reached the tip yet.
    AwaitingFirstBlock,
    /// Replaying blocks missed while disconnected.
    CatchingUp,
    /// Connected, but silent for `silent_secs`. The chain's state is unknown.
    Quiet {
        silent_secs: u64,
    },
    Offline {
        disconnected_secs: u64,
    },
    NotStarted,
}

impl PulseState {
    pub fn of(snapshot: &HeartbeatSnapshot) -> Self {
        match snapshot.feed {
            FeedHealth::NotStarted => Self::NotStarted,
            FeedHealth::Disconnected { disconnected_secs } => Self::Offline { disconnected_secs },
            FeedHealth::Silent { silent_secs } => Self::Quiet { silent_secs },
            FeedHealth::Following {
                sync: SyncState::CatchingUp,
                ..
            } => Self::CatchingUp,
            FeedHealth::Following {
                sync: SyncState::AtTip,
                ..
            } => match snapshot.block_due_probability {
                Some(due) => Self::Live { due },
                None => Self::AwaitingFirstBlock,
            },
        }
    }

    /// A short word about the feed, or `None` while it is simply live.
    pub fn status(&self) -> Option<String> {
        match self {
            Self::Live { .. } => None,
            Self::AwaitingFirstBlock => Some("waiting for a block".to_string()),
            Self::CatchingUp => Some("catching up".to_string()),
            Self::Quiet { silent_secs } => {
                Some(format!("feed quiet {}", format_duration(*silent_secs)))
            }
            Self::Offline { .. } => Some("offline".to_string()),
            Self::NotStarted => Some("not started".to_string()),
        }
    }

    /// One or two sentences on what the state means, for a tooltip.
    pub fn explanation(&self) -> String {
        match self {
            Self::Live { due } => {
                let percent = (due * 100.0).round() as u32;
                format!(
                    "{percent}% chance a block has landed by now. Blocks arrive at random, \
                     about 20 seconds apart on average, so the chain is never late."
                )
            }
            Self::AwaitingFirstBlock => {
                "Connected and following. No block has reached the tip yet.".to_string()
            }
            Self::CatchingUp => {
                "Replaying blocks missed while the feed was away. They are not new activity."
                    .to_string()
            }
            Self::Quiet { silent_secs } => format!(
                "The feed has heard nothing for {}. Until it does, the chain's state is \
                 unknown, not quiet.",
                format_duration(*silent_secs)
            ),
            Self::Offline { disconnected_secs } => format!(
                "The feed lost its connection {} ago.",
                format_duration(*disconnected_secs)
            ),
            Self::NotStarted => "The feed has not connected yet.".to_string(),
        }
    }
}

/// The pulse. Build per frame; it holds no state of its own beyond the pop
/// timer egui keeps for it.
pub struct BlockPulse<'a> {
    heartbeat: &'a Heartbeat,
    now_ms: u64,
    detail: PulseDetail,
    text: TextSize,
    id_salt: Id,
}

impl<'a> BlockPulse<'a> {
    /// `now_ms` is unix milliseconds, from the host's clock.
    pub fn new(heartbeat: &'a Heartbeat, now_ms: u64) -> Self {
        Self {
            heartbeat,
            now_ms,
            detail: PulseDetail::default(),
            text: TextSize::Base,
            id_salt: Id::new("block_pulse"),
        }
    }

    pub fn detail(mut self, detail: PulseDetail) -> Self {
        self.detail = detail;
        self
    }

    /// The type step the line is set in. The mark scales with it.
    pub fn text(mut self, size: TextSize) -> Self {
        self.text = size;
        self
    }

    /// Distinguish two pulses in one `Ui`, so they keep separate pop timers.
    pub fn id_salt(mut self, salt: impl egui::AsIdSalt) -> Self {
        self.id_salt = Id::NULL.with(salt);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let snapshot = self.heartbeat.snapshot(self.now_ms);
        let state = PulseState::of(&snapshot);
        let size = ui.text_size(self.text);
        let row = line_height(ui, size);
        let id = ui.id().with(self.id_salt);
        let pop = pop_progress(ui.ctx(), id, snapshot.tip.as_ref(), state);
        let theme = ui.tokens();
        let detail = self.detail;

        // An explicit left-to-right row one line tall: `horizontal` would
        // inherit a right-to-left parent and read backwards, and a centred
        // cross axis needs a box of its own height to centre in.
        let inner = ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), row),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().interact_size = Vec2::ZERO;
                ui.set_item_gap_x(Space::Sm);

                let (rect, _) = ui.allocate_exact_size(Vec2::splat(row), Sense::hover());
                if ui.is_rect_visible(rect) {
                    draw_mark(
                        ui.painter(),
                        &theme,
                        rect.center(),
                        row * MARK_RADIUS,
                        state,
                        pop,
                    );
                }

                if detail == PulseDetail::Full
                    && let Some(tip) = &snapshot.tip
                {
                    ui.label(
                        RichText::new(format!("#{}", format_number(tip.height as i64)))
                            .size(size)
                            .strong()
                            .color(theme.color.text_primary),
                    );
                }
                if let Some(secs) = snapshot.secs_since_block {
                    ui.label(
                        RichText::new(format_duration(secs))
                            .size(size)
                            .color(theme.color.text_secondary),
                    );
                }
                if detail == PulseDetail::Full
                    && let Some(status) = state.status()
                {
                    ui.label(
                        RichText::new(status)
                            .size(size)
                            .color(theme.color.text_muted),
                    );
                }
            },
        );

        // Only tick while there is a clock to show.
        if snapshot.tip.is_some() {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }

        let max_body = self.heartbeat.network().max_block_body_bytes();
        ui.interact(inner.response.rect, id.with("hover"), Sense::hover())
            .on_hover_ui(|ui| {
                if let Some(tip) = &snapshot.tip {
                    beat_lines(ui, tip, snapshot.secs_since_block, max_body);
                    ui.gap(Space::Sm);
                }
                ui.label(
                    RichText::new(state.explanation())
                        .size(ui.text_size(TextSize::Sm))
                        .color(ui.tokens().color.text_secondary),
                );
            })
    }
}

/// Pop timer memory: the tip last drawn, and when the latest pop began.
#[derive(Clone, Default)]
struct PulseMemory {
    tip: Option<(u64, String)>,
    popped_at: Option<f64>,
}

/// How far into a pop the mark is: `1.0` the instant a new tip block arrives,
/// easing to `0.0`. Pops only when the tip advanced while live, so the first
/// sight of a chain and a catch-up replay stay still.
pub(crate) fn pop_progress(
    ctx: &egui::Context,
    id: Id,
    tip: Option<&BlockBeat>,
    state: PulseState,
) -> f32 {
    let now = ctx.input(|i| i.time);
    let current = tip.map(|b| (b.height, b.hash.clone()));
    let popped_at = ctx.data_mut(|d| {
        let memory = d.get_temp_mut_or_default::<PulseMemory>(id);
        if current != memory.tip {
            let advanced = matches!(
                (&memory.tip, &current),
                (Some((was, _)), Some((is, _))) if is >= was
            );
            if advanced && matches!(state, PulseState::Live { .. }) {
                memory.popped_at = Some(now);
            }
            memory.tip = current;
        }
        memory.popped_at
    });

    let duration = ctx.duration(Speed::Slow);
    let Some(started) = popped_at else {
        return 0.0;
    };
    if duration <= 0.0 {
        return 0.0;
    }
    let progress = ((now - started) as f32 / duration).clamp(0.0, 1.0);
    if progress < 1.0 {
        ctx.request_repaint();
    }
    1.0 - ctx.easing(Easing::OutCubic).apply(progress)
}

/// The mark: a ring (filled towards `due` while live) around a dot whose
/// fill says whether the feed can be trusted.
pub(crate) fn draw_mark(
    painter: &Painter,
    theme: &Theme,
    center: Pos2,
    radius: f32,
    state: PulseState,
    pop: f32,
) {
    let c = &theme.color;
    let weight = (radius * 0.2).max(1.5);
    let dot = radius * DOT_FRACTION;
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(weight, with_alpha(c.border, 200)),
    );

    match state {
        PulseState::Live { due } => {
            arc(painter, center, radius, due, Stroke::new(weight, c.accent));
            let travel = theme.motion.travel_allowed();
            if pop > 0.0 {
                // The halo grows only where travel is allowed; under reduced
                // motion it is a fade in place.
                let halo = if travel {
                    radius * (1.0 + 0.9 * (1.0 - pop))
                } else {
                    radius
                };
                painter.circle_stroke(
                    center,
                    halo,
                    Stroke::new(weight, with_alpha(c.accent, (pop * 180.0) as u8)),
                );
            }
            let scale = if travel { 1.0 + 0.45 * pop } else { 1.0 };
            painter.circle_filled(center, dot * scale, c.accent);
        }
        PulseState::AwaitingFirstBlock | PulseState::CatchingUp => {
            painter.circle_filled(center, dot, c.text_muted);
        }
        PulseState::Quiet { .. } => {
            painter.circle_stroke(center, dot, Stroke::new(weight, c.warning));
        }
        PulseState::Offline { .. } | PulseState::NotStarted => {
            painter.circle_stroke(center, dot, Stroke::new(weight, c.text_muted));
        }
    }
}

/// An arc from twelve o'clock, clockwise, over `fraction` of the circle.
fn arc(painter: &Painter, center: Pos2, radius: f32, fraction: f32, stroke: Stroke) {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction <= 0.0 {
        return;
    }
    let steps = ((fraction * 48.0).ceil() as usize).max(2);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

/// The block facts a tooltip shows: height, slot and age, load, producer.
pub(crate) fn beat_lines(ui: &mut Ui, beat: &BlockBeat, age_secs: Option<u64>, max_body: u32) {
    let theme = ui.tokens();
    let body = ui.text_size(TextSize::Base);
    let meta = ui.text_size(TextSize::Sm);

    ui.label(
        RichText::new(format!("Block {}", format_number(beat.height as i64)))
            .size(body)
            .strong()
            .color(theme.color.text_primary),
    );
    let when = age_secs
        .map(|secs| format!(" · {} ago", format_duration(secs)))
        .unwrap_or_default();
    ui.label(
        RichText::new(format!("slot {}{when}", format_number(beat.slot as i64)))
            .size(meta)
            .color(theme.color.text_muted),
    );
    let percent = fullness_percent(beat.body_size, max_body);
    let load = match beat.tx_count {
        Some(1) => format!("1 tx · {percent}% full"),
        Some(n) => format!("{n} txs · {percent}% full"),
        None => format!("{percent}% full"),
    };
    ui.label(
        RichText::new(load)
            .size(meta)
            .color(theme.color.text_secondary),
    );
    ui.label(
        RichText::new(format!("pool {}", truncate_hex(&beat.issuer_pool, 8, 6)))
            .size(meta)
            .monospace()
            .color(theme.color.text_muted),
    );
}

fn fullness_percent(body_size: u32, max_body: u32) -> u32 {
    if max_body == 0 {
        return 0;
    }
    ((body_size as f32 / max_body as f32).clamp(0.0, 1.0) * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use chain_heartbeat::{ChainEvent, Network};

    use super::*;
    use crate::motion::tests::step;
    use crate::test_pass::TestPass as _;

    const SLOT: u64 = 186_000_000;

    fn beat(height: u64, slot: u64) -> BlockBeat {
        BlockBeat {
            height,
            slot,
            hash: format!("{height:064x}"),
            issuer_pool: "ab".repeat(28),
            body_size: 45_056,
            tx_count: Some(12),
            block_time_unix: Network::Mainnet.slot_to_unix_secs(slot),
        }
    }

    fn ms_at(slot: u64) -> u64 {
        Network::Mainnet.slot_to_unix_secs(slot).unwrap() * 1000
    }

    #[test]
    fn the_state_follows_the_feed_not_the_gap() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        assert_eq!(PulseState::of(&hb.snapshot(0)), PulseState::NotStarted);

        let t0 = ms_at(SLOT);
        hb.apply(&ChainEvent::Connected { version: 14 }, t0);
        assert_eq!(PulseState::of(&hb.snapshot(t0)), PulseState::CatchingUp);

        hb.apply(
            &ChainEvent::RollForward {
                beat: beat(1, SLOT),
                sync: SyncState::AtTip,
            },
            t0,
        );
        let PulseState::Live { due } = PulseState::of(&hb.snapshot(t0 + 14_000)) else {
            panic!("a block at the tip is live");
        };
        assert!((0.5..0.52).contains(&due), "{due}");

        // Silence past the threshold is the FEED going quiet, whatever the ring
        // was showing a moment before.
        assert_eq!(
            PulseState::of(&hb.snapshot(t0 + 95_000)),
            PulseState::Quiet { silent_secs: 95 }
        );

        hb.disconnected(t0 + 100_000);
        assert_eq!(
            PulseState::of(&hb.snapshot(t0 + 160_000)),
            PulseState::Offline {
                disconnected_secs: 60
            }
        );
    }

    #[test]
    fn only_non_live_states_have_a_status_word() {
        assert_eq!(PulseState::Live { due: 0.3 }.status(), None);
        assert_eq!(
            PulseState::Quiet { silent_secs: 95 }.status().as_deref(),
            Some("feed quiet 1m")
        );
        assert_eq!(
            PulseState::CatchingUp.status().as_deref(),
            Some("catching up")
        );
        // The live explanation states a likelihood, not a deadline.
        let text = PulseState::Live { due: 0.64 }.explanation();
        assert!(text.starts_with("64% chance"), "{text}");
        assert!(text.contains("never late"), "{text}");
    }

    #[test]
    fn a_pop_needs_a_new_tip_while_live() {
        let ctx = egui::Context::default();
        let id = Id::new("pulse");
        let first = beat(10, SLOT);
        let second = beat(11, SLOT + 20);
        let live = PulseState::Live { due: 0.2 };

        // First sight of a chain: still.
        step(&ctx, 0.0);
        assert_eq!(pop_progress(&ctx, id, Some(&first), live), 0.0);
        let _ = ctx.end_test_pass();

        // A catch-up block: still.
        step(&ctx, 1.0);
        assert_eq!(
            pop_progress(&ctx, id, Some(&second), PulseState::CatchingUp),
            0.0
        );
        let _ = ctx.end_test_pass();

        // A new block at the tip: pops at full strength, then settles.
        let third = beat(12, SLOT + 40);
        step(&ctx, 2.0);
        assert_eq!(pop_progress(&ctx, id, Some(&third), live), 1.0);
        let _ = ctx.end_test_pass();
        step(&ctx, 2.0 + 10.0);
        assert_eq!(pop_progress(&ctx, id, Some(&third), live), 0.0);
        let _ = ctx.end_test_pass();
    }

    #[test]
    fn fullness_is_bounded() {
        assert_eq!(fullness_percent(45_056, 90_112), 50);
        assert_eq!(fullness_percent(200_000, 90_112), 100);
        assert_eq!(fullness_percent(1, 0), 0);
    }
}
