//! A simulated Cardano feed for the chain stories.
//!
//! Blocks are drawn on the chain's real tempo (each one-second slot a 5% chance)
//! from a fixed seed, so every load looks the same and screenshots are stable.
//! The events go through a real `chain_heartbeat::Heartbeat`, exactly as a host
//! would apply them, so the widgets are shown folding the same data they will
//! fold in production rather than a hand-built snapshot.
//!
//! The clock is the story's own: it advances with egui's frame time multiplied
//! by a speed the reader controls, and starts an hour into the chain so the
//! train and the hourly figures have history from the first frame.

use chain_heartbeat::{BlockBeat, ChainEvent, ChainPoint, Heartbeat, Network, SyncState};

/// A real mainnet slot and height, so epochs and heights read true.
const BASE_SLOT: u64 = 186_000_000;
const BASE_HEIGHT: u64 = 13_358_656;

/// History before the story's clock starts.
const HISTORY_SECS: u64 = 3_600;

/// How far into the future the script runs.
const FUTURE_SECS: u64 = 4 * 3_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// Following at the tip.
    Live,
    /// Connected, but nothing has arrived for a while.
    QuietFeed,
    /// Dropped, then back five seconds in, replaying what it missed.
    Reconnect,
    /// Ten seconds in, the last two blocks are replaced by a competing fork.
    Rollback,
    /// Lost the connection five minutes ago and has not come back.
    Offline,
}

impl Scenario {
    pub const ALL: [Scenario; 5] = [
        Scenario::Live,
        Scenario::QuietFeed,
        Scenario::Reconnect,
        Scenario::Rollback,
        Scenario::Offline,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Scenario::Live => "Live",
            Scenario::QuietFeed => "Quiet feed",
            Scenario::Reconnect => "Reconnect",
            Scenario::Rollback => "Rollback",
            Scenario::Offline => "Offline",
        }
    }
}

enum SimEvent {
    Chain(ChainEvent),
    Disconnect,
}

/// One scripted event, at seconds relative to the story's start.
struct Timed {
    at: f64,
    event: SimEvent,
}

pub struct ChainSim {
    pub heartbeat: Heartbeat,
    pub scenario: Scenario,
    pub speed: f32,
    script: Vec<Timed>,
    cursor: usize,
    elapsed: f64,
    last_frame: Option<f64>,
}

impl ChainSim {
    pub fn new(scenario: Scenario) -> Self {
        let mut sim = Self {
            heartbeat: Heartbeat::new(Network::Mainnet),
            scenario,
            speed: 1.0,
            script: script(scenario),
            cursor: 0,
            elapsed: 0.0,
            last_frame: None,
        };
        sim.apply_due();
        sim
    }

    /// Advance the clock by this frame's time and apply what is due.
    pub fn advance(&mut self, ui: &egui::Ui) {
        let now = ui.input(|i| i.time);
        // Clamp a frame gap (a background tab) so the story does not jump.
        let delta = self
            .last_frame
            .map(|last| (now - last).clamp(0.0, 0.5))
            .unwrap_or(0.0);
        self.last_frame = Some(now);
        self.elapsed += delta * self.speed as f64;
        self.apply_due();
        // A fallback so the clock keeps moving with nothing on screen asking for
        // frames. The widgets set their own cadence; this must not impose one,
        // or it hides whether theirs is right.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
    }

    /// The simulated wall clock, unix milliseconds.
    pub fn now_ms(&self) -> u64 {
        ((anchor_unix() as f64 + self.elapsed) * 1000.0) as u64
    }

    /// The simulated wall clock, unix seconds.
    pub fn now_secs(&self) -> u64 {
        self.now_ms() / 1000
    }

    pub fn restart(&mut self) {
        let speed = self.speed;
        *self = Self::new(self.scenario);
        self.speed = speed;
    }

    fn apply_due(&mut self) {
        while let Some(timed) = self.script.get(self.cursor) {
            if timed.at > self.elapsed {
                break;
            }
            let at_ms = ((anchor_unix() as f64 + timed.at) * 1000.0) as u64;
            match &timed.event {
                SimEvent::Chain(event) => self.heartbeat.apply(event, at_ms),
                SimEvent::Disconnect => self.heartbeat.disconnected(at_ms),
            }
            self.cursor += 1;
        }
    }
}

/// Speed and scenario controls, shared by the chain stories. Returns whether the
/// scenario changed (so a story holding several sims can restart them).
pub fn controls(ui: &mut egui::Ui, sim: &mut ChainSim) {
    ui.horizontal_wrapped(|ui| {
        for scenario in Scenario::ALL {
            if ui
                .selectable_label(sim.scenario == scenario, scenario.label())
                .clicked()
                && sim.scenario != scenario
            {
                sim.scenario = scenario;
                sim.restart();
            }
        }
        ui.separator();
        ui.label(egui::RichText::new("speed").color(crate::muted(ui)));
        ui.add(egui::Slider::new(&mut sim.speed, 1.0..=30.0).suffix("x"));
        if ui.button("Restart").clicked() {
            sim.restart();
        }
    });
}

/// Unix seconds at the story's start: an hour after `BASE_SLOT`.
fn anchor_unix() -> u64 {
    Network::Mainnet
        .slot_to_unix_secs(BASE_SLOT + HISTORY_SECS)
        .expect("a Shelley-era slot")
}

/// Seconds from the story's start to a slot.
fn at_slot(slot: u64) -> f64 {
    slot as f64 - (BASE_SLOT + HISTORY_SECS) as f64
}

fn script(scenario: Scenario) -> Vec<Timed> {
    let chain = blocks(
        0x5eed,
        BASE_SLOT,
        BASE_HEIGHT,
        BASE_SLOT + HISTORY_SECS + FUTURE_SECS,
    );
    let mut out = vec![Timed {
        at: -(HISTORY_SECS as f64) - 1.0,
        event: SimEvent::Chain(ChainEvent::Connected { version: 14 }),
    }];

    // Keep-alive answers every 30 s up to `until`, as a following host sees.
    let keep_alive = |out: &mut Vec<Timed>, from: f64, until: f64| {
        let mut t = from;
        while t < until {
            out.push(Timed {
                at: t,
                event: SimEvent::Chain(ChainEvent::KeepAliveAcknowledged),
            });
            t += 30.0;
        }
    };
    let at_tip = |beat: &BlockBeat, at: f64| Timed {
        at,
        event: SimEvent::Chain(ChainEvent::RollForward {
            beat: beat.clone(),
            sync: SyncState::AtTip,
        }),
    };

    let horizon = FUTURE_SECS as f64;
    match scenario {
        Scenario::Live => {
            out.extend(chain.iter().map(|b| at_tip(b, at_slot(b.slot))));
            keep_alive(&mut out, -(HISTORY_SECS as f64), horizon);
        }
        Scenario::QuietFeed => {
            let cutoff = -100.0;
            out.extend(
                chain
                    .iter()
                    .filter(|b| at_slot(b.slot) <= cutoff)
                    .map(|b| at_tip(b, at_slot(b.slot))),
            );
            keep_alive(&mut out, -(HISTORY_SECS as f64), cutoff);
        }
        Scenario::Offline => {
            let cutoff = -300.0;
            out.extend(
                chain
                    .iter()
                    .filter(|b| at_slot(b.slot) <= cutoff)
                    .map(|b| at_tip(b, at_slot(b.slot))),
            );
            keep_alive(&mut out, -(HISTORY_SECS as f64), cutoff);
            out.push(Timed {
                at: cutoff + 1.0,
                event: SimEvent::Disconnect,
            });
        }
        Scenario::Reconnect => {
            let (dropped, back) = (-90.0, 5.0);
            out.extend(
                chain
                    .iter()
                    .filter(|b| at_slot(b.slot) <= dropped)
                    .map(|b| at_tip(b, at_slot(b.slot))),
            );
            keep_alive(&mut out, -(HISTORY_SECS as f64), dropped);
            out.push(Timed {
                at: dropped + 1.0,
                event: SimEvent::Disconnect,
            });
            out.push(Timed {
                at: back,
                event: SimEvent::Chain(ChainEvent::Connected { version: 14 }),
            });
            // Everything missed, replayed in a burst; the last reaches the tip.
            let missed: Vec<&BlockBeat> = chain
                .iter()
                .filter(|b| at_slot(b.slot) > dropped && at_slot(b.slot) <= back)
                .collect();
            for (i, beat) in missed.iter().enumerate() {
                let sync = if i + 1 == missed.len() {
                    SyncState::AtTip
                } else {
                    SyncState::CatchingUp
                };
                out.push(Timed {
                    at: back + 0.05 * i as f64,
                    event: SimEvent::Chain(ChainEvent::RollForward {
                        beat: (*beat).clone(),
                        sync,
                    }),
                });
            }
            out.extend(
                chain
                    .iter()
                    .filter(|b| at_slot(b.slot) > back)
                    .map(|b| at_tip(b, at_slot(b.slot))),
            );
            keep_alive(&mut out, back, horizon);
        }
        Scenario::Rollback => {
            let fork_at = 10.0;
            let before: Vec<&BlockBeat> = chain
                .iter()
                .filter(|b| at_slot(b.slot) <= fork_at)
                .collect();
            out.extend(before.iter().map(|b| at_tip(b, at_slot(b.slot))));
            keep_alive(&mut out, -(HISTORY_SECS as f64), horizon);

            // Roll back two blocks, then follow a different fork from there.
            let target = before[before.len().saturating_sub(3)];
            out.push(Timed {
                at: fork_at,
                event: SimEvent::Chain(ChainEvent::RollBackward {
                    to: target.point().map(|p| ChainPoint {
                        slot: p.slot,
                        hash: p.hash,
                    }),
                }),
            });
            let fork = blocks(
                0xf0f0,
                target.slot,
                target.height + 1,
                BASE_SLOT + HISTORY_SECS + FUTURE_SECS,
            );
            for (i, beat) in fork.iter().enumerate() {
                let natural = at_slot(beat.slot);
                let at = if natural <= fork_at {
                    fork_at + 0.4 + 0.3 * i as f64
                } else {
                    natural
                };
                out.push(at_tip(beat, at));
            }
        }
    }

    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    out
}

/// A chain from `after_slot` (exclusive) to `until_slot`, starting at `first_height`.
fn blocks(seed: u64, after_slot: u64, first_height: u64, until_slot: u64) -> Vec<BlockBeat> {
    let mut rng = XorShift(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut out = Vec::new();
    let mut slot = after_slot;
    let mut height = first_height;
    loop {
        slot += geometric_gap(&mut rng);
        if slot > until_slot {
            break;
        }
        let txs = (rng.next() % 38) as u32;
        let body = (txs * 1_900 + (rng.next() % 4_000) as u32).min(90_112);
        let mixed = height.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ seed;
        let hash = format!(
            "{mixed:016x}{:016x}{:016x}{:016x}",
            rng.next(),
            rng.next(),
            slot
        );
        let issuer_pool = format!("{:056x}", (rng.next() % 17) as u128 * 0x1f3a_9c2b_77e1);
        out.push(BlockBeat {
            height,
            slot,
            hash,
            issuer_pool,
            body_size: body,
            tx_count: Some(txs),
            block_time_unix: Network::Mainnet.slot_to_unix_secs(slot),
        });
        height += 1;
    }
    out
}

/// Seconds to the next block when each slot is an independent 5% chance.
fn geometric_gap(rng: &mut XorShift) -> u64 {
    let u = (rng.next() >> 11) as f64 / (1u64 << 53) as f64;
    ((1.0 - u).ln() / 0.95_f64.ln()).ceil().max(1.0) as u64
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}
