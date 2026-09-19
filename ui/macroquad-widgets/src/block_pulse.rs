//! `BlockPulse` — the chain's heartbeat on one line: a dot that pops on each new block, a ring filling towards "a block has probably landed", and the time since the last one.
//!
//! The macroquad twin of `egui_widgets::block_pulse`. Same vocabulary, same
//! refusals, different renderer — [`PulseState`] is a straight port so the two
//! surfaces can never disagree about what a feed is doing.
//!
//! ## What each part means, and what it refuses to mean
//!
//! - **The dot pops only on a block at the tip.** A reconnect replaying blocks
//!   it missed is catch-up, not activity, and pops nothing. Nor does the block
//!   already on screen when the widget first appears.
//! - **The ring is a likelihood, never a countdown.** Block production is
//!   memoryless: the chance a block has landed rises with time, but the chain is
//!   never "due". The ring fills towards certainty and claims no deadline, and
//!   is drawn only while the feed is following at the tip.
//! - **A quiet feed is not a quiet chain.** When the host has heard nothing for
//!   a while the dot goes hollow and the widget says the FEED is quiet, so a
//!   stretching gap can never read as a slow chain.
//!
//! ## What differs from the egui twin, and why
//!
//! - **State is the host's.** macroquad-widgets is stateless by charter, and the
//!   pop needs to remember the tip it last drew, so the timer lives in a
//!   [`PulseTicker`] the host owns — the same shape as `Gestures`.
//! - **No tooltip.** There is no hover on a phone, and macroquad has no tooltip
//!   layer. The block facts a tooltip would carry belong on screen or nowhere.
//! - **The clock is UTC.** The egui version reads the reader's timezone through
//!   `js_sys`, which does not exist under miniquad on either target. It says UTC
//!   rather than passing UTC off as local time.

use chain_heartbeat::{BlockBeat, FeedHealth, Heartbeat, HeartbeatSnapshot, SyncState};
use macroquad::prelude::*;
use ui_theme::TextSize;

use crate::painter::Painter;
use crate::theme::{self, Theme};

/// The dot's radius as a fraction of the ring's.
const DOT_FRACTION: f32 = 0.45;

/// How long a feed must be disconnected before it reads as offline. Reconnects
/// measured on the gateway take two to three seconds, so a shorter drop is a
/// reconnect in progress rather than an outage worth shouting about.
pub const OFFLINE_AFTER_SECS: u64 = 5;

/// How long a pop takes to fade, in seconds.
const POP_SECS: f64 = 0.45;

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
///
/// A straight port of the egui twin's state machine. Any change here is a change
/// there: the point of the type is that two renderers cannot drift.
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
            // A drop this short is a reconnect in progress: a browser socket the
            // edge recycled, or the gateway's relay hanging up (every few
            // minutes, back in about two seconds). Calling that "offline"
            // flashed a warning over a feed that never really went away, so the
            // live reading is held until the drop outlasts the grace.
            FeedHealth::Disconnected { disconnected_secs }
                if disconnected_secs < OFFLINE_AFTER_SECS =>
            {
                match snapshot.secs_since_block {
                    Some(secs) => Self::Live {
                        due: chain_heartbeat::block_probability_within(secs) as f32,
                    },
                    None => Self::AwaitingFirstBlock,
                }
            }
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

    /// One or two sentences on what the state means.
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

    /// Whether the feed is following at the tip, i.e. whether motion on screen
    /// is reporting the chain rather than a replay or a stale picture.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }
}

/// The pop timer, owned by the host.
///
/// Not a widget: it remembers the tip it last drew, which the charter forbids
/// inside a widget precisely so it lives somewhere named. Hold one per pulse (a
/// header pulse and a train share nothing), and call [`PulseTicker::progress`]
/// once per frame.
#[derive(Clone, Debug, Default)]
pub struct PulseTicker {
    tip: Option<(u64, String)>,
    popped_at: Option<f64>,
}

impl PulseTicker {
    pub fn new() -> Self {
        Self::default()
    }

    /// How far into a pop the mark is: `1.0` the instant a new tip block
    /// arrives, easing to `0.0`.
    ///
    /// Pops only when the tip ADVANCED while live, so the first sight of a chain
    /// and a catch-up replay stay still. `now` is [`macroquad::time::get_time`].
    pub fn progress(&mut self, tip: Option<&BlockBeat>, state: PulseState, now: f64) -> f32 {
        let current = tip.map(|b| (b.height, b.hash.clone()));
        if current != self.tip {
            let advanced = matches!(
                (&self.tip, &current),
                (Some((was, _)), Some((is, _))) if is >= was
            );
            if advanced && state.is_live() {
                self.popped_at = Some(now);
            }
            self.tip = current;
        }
        let Some(started) = self.popped_at else {
            return 0.0;
        };
        let progress = ((now - started) / POP_SECS).clamp(0.0, 1.0) as f32;
        // Ease out cubic, inverted: full at the instant of arrival, gone after.
        1.0 - (1.0 - (1.0 - progress).powi(3))
    }
}

/// The mark: a ring (filled towards `due` while live) around a dot whose fill
/// says whether the feed can be trusted.
///
/// Shared with `block_train`, which embeds it as a status light, so the two
/// widgets cannot render the same feed differently.
pub fn draw_mark(t: &Theme, center: Vec2, radius: f32, state: PulseState, pop: f32) {
    let weight = (radius * 0.2).max(1.5);
    draw_circle_lines(
        center.x,
        center.y,
        radius,
        weight,
        theme::with_alpha(t.color.bg_highlight, 0.8),
    );
    let dot = radius * DOT_FRACTION;

    match state {
        PulseState::Live { due } => {
            arc(center, radius, due, weight, t.color.accent);
            if pop > 0.0 {
                let halo = radius * (1.0 + 0.9 * (1.0 - pop));
                draw_circle_lines(
                    center.x,
                    center.y,
                    halo,
                    weight,
                    theme::with_alpha(t.color.accent, pop * 0.7),
                );
            }
            draw_circle(center.x, center.y, dot * (1.0 + 0.45 * pop), t.color.accent);
        }
        PulseState::AwaitingFirstBlock | PulseState::CatchingUp => {
            draw_circle(center.x, center.y, dot, t.color.text_muted);
        }
        PulseState::Quiet { .. } => {
            draw_circle_lines(center.x, center.y, dot, weight, t.color.warning);
        }
        PulseState::Offline { .. } | PulseState::NotStarted => {
            draw_circle_lines(center.x, center.y, dot, weight, t.color.text_muted);
        }
    }
}

/// An arc from twelve o'clock, clockwise, over `fraction` of the circle.
///
/// macroquad has no arc primitive, so this walks the circle in segments. The
/// step count follows the fraction, so a nearly-full ring is not visibly a
/// polygon while a sliver costs two lines.
fn arc(center: Vec2, radius: f32, fraction: f32, weight: f32, color: Color) {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction <= 0.0 {
        return;
    }
    let steps = ((fraction * 48.0).ceil() as usize).max(2);
    let at = |i: usize| {
        let angle = -std::f32::consts::FRAC_PI_2
            + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
        center + vec2(angle.cos(), angle.sin()) * radius
    };
    for i in 0..steps {
        let (a, b) = (at(i), at(i + 1));
        draw_line(a.x, a.y, b.x, b.y, weight, color);
    }
}

/// Everything the pulse renders, projected by the host.
pub struct BlockPulseVm<'a> {
    pub heartbeat: &'a Heartbeat,
    /// Unix milliseconds, from the host's clock.
    pub now_ms: u64,
    pub detail: PulseDetail,
    /// Type step for the line. The mark scales with it, so this sizes the whole
    /// widget — a `TextSize` rather than an `f32` so a host that opens the ramp
    /// out moves the pulse with everything else.
    pub size: TextSize,
}

impl<'a> BlockPulseVm<'a> {
    pub fn new(heartbeat: &'a Heartbeat, now_ms: u64) -> Self {
        Self {
            heartbeat,
            now_ms,
            detail: PulseDetail::default(),
            size: TextSize::Lg,
        }
    }

    pub fn detail(mut self, detail: PulseDetail) -> Self {
        self.detail = detail;
        self
    }

    pub fn size(mut self, size: TextSize) -> Self {
        self.size = size;
        self
    }
}

/// Draw the pulse as one line starting at `x`, vertically centred on `y`.
/// Returns the x just past the last thing drawn, so a header can continue.
pub fn block_pulse(
    p: &Painter,
    vm: &BlockPulseVm<'_>,
    ticker: &mut PulseTicker,
    x: f32,
    y: f32,
) -> f32 {
    let t = &p.theme;
    let snapshot = vm.heartbeat.snapshot(vm.now_ms);
    let state = PulseState::of(&snapshot);
    let pop = ticker.progress(snapshot.tip.as_ref(), state, get_time());

    let size = p.size(vm.size);
    let radius = size * 0.36;
    draw_mark(t, vec2(x + radius, y), radius, state, pop);
    let mut cursor = x + radius * 2.0 + size * 0.45;
    // Baseline for text centred on `y`, independent of the string's glyphs.
    let baseline = p.centre_baseline(y - size, size * 2.0, size);

    if vm.detail == PulseDetail::Full
        && let Some(tip) = &snapshot.tip
    {
        let text = format!("#{}", format_number(tip.height));
        p.text(&text, cursor, baseline, size, t.color.text_primary);
        cursor += p.measure(&text, size).width + size * 0.45;
    }
    if let Some(secs) = snapshot.secs_since_block {
        let text = format_duration(secs);
        p.text(&text, cursor, baseline, size, t.color.text_muted);
        cursor += p.measure(&text, size).width + size * 0.45;
    }
    if vm.detail == PulseDetail::Full
        && let Some(status) = state.status()
    {
        p.text(
            &status,
            cursor,
            baseline,
            size,
            theme::with_alpha(t.color.text_muted, 0.8),
        );
        cursor += p.measure(&status, size).width;
    }
    cursor
}

// ── shared formatting ────────────────────────────────────────────────────────

/// A duration in the coarsest unit that still says something: `9s`, `4m`, `2h`.
pub fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3_600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

/// Thousands-separated, so a block height is readable at a glance.
pub fn format_number(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A block's load as a percentage of the network's maximum body size.
pub fn fullness_percent(body_size: u32, max_body: u32) -> u32 {
    if max_body == 0 {
        return 0;
    }
    ((body_size as f32 / max_body as f32).clamp(0.0, 1.0) * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use chain_heartbeat::{ChainEvent, Network};

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
            vrf_output: None,
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
    }

    #[test]
    fn a_brief_drop_stays_live_and_a_long_one_is_offline() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        let t0 = ms_at(SLOT);
        hb.apply(&ChainEvent::Connected { version: 14 }, t0);
        hb.apply(
            &ChainEvent::RollForward {
                beat: beat(1, SLOT),
                sync: SyncState::AtTip,
            },
            t0,
        );
        hb.disconnected(t0 + 10_000);

        assert!(PulseState::of(&hb.snapshot(t0 + 13_000)).is_live());
        assert_eq!(
            PulseState::of(&hb.snapshot(t0 + 16_000)),
            PulseState::Offline {
                disconnected_secs: 6
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
        // The live explanation states a likelihood, not a deadline.
        let text = PulseState::Live { due: 0.64 }.explanation();
        assert!(text.starts_with("64% chance"), "{text}");
        assert!(text.contains("never late"), "{text}");
    }

    #[test]
    fn a_pop_needs_a_new_tip_while_live() {
        let mut ticker = PulseTicker::new();
        let live = PulseState::Live { due: 0.2 };
        let first = beat(10, SLOT);

        // First sight of a chain: still.
        assert_eq!(ticker.progress(Some(&first), live, 0.0), 0.0);

        // A catch-up block: still.
        let second = beat(11, SLOT + 20);
        assert_eq!(
            ticker.progress(Some(&second), PulseState::CatchingUp, 1.0),
            0.0
        );

        // A new block at the tip: pops at full strength, then settles.
        let third = beat(12, SLOT + 40);
        assert_eq!(ticker.progress(Some(&third), live, 2.0), 1.0);
        assert_eq!(ticker.progress(Some(&third), live, 2.0 + POP_SECS), 0.0);
    }

    #[test]
    fn figures_read_as_figures() {
        assert_eq!(format_number(13_358_656), "13,358,656");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(0), "0");
        assert_eq!(format_duration(9), "9s");
        assert_eq!(format_duration(95), "1m");
        assert_eq!(format_duration(7_200), "2h");
        assert_eq!(fullness_percent(45_056, 90_112), 50);
        assert_eq!(fullness_percent(200_000, 90_112), 100);
        assert_eq!(fullness_percent(1, 0), 0);
    }
}
