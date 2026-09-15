//! `ChainTempo` — the chain's pace as stat tiles plus the epoch clock: blocks in the last hour, the gap between blocks, transactions per minute and how full blocks run.
//!
//! Every figure that has an expectation carries it (about 180 blocks an hour,
//! about 20 seconds apart), so a reader can tell a slow hour from a normal one
//! without knowing how Cardano schedules blocks.
//!
//! ## Why the epoch gets a countdown and a block does not
//!
//! An epoch ends on a fixed slot, so "ends in 3d 4h" is a fact. A block has no
//! deadline, so nothing in this suite counts down to one. Putting the two side
//! by side is deliberate: the reader learns which of the chain's clocks are
//! schedules and which are chance.
//!
//! Figures that need more history than the feed has (an hour for the hourly
//! count) say what they are waiting for instead of showing a partial number as
//! if it were whole.

use std::time::Duration;

use chain_heartbeat::Heartbeat;
use egui::Ui;

use crate::metric_card::{MetricCard, MetricRow};
use crate::progress_bar::ProgressBar;
use crate::theme::{Space, SpaceExt};
use crate::utils::format_duration;

/// 3,600 one-second slots at an active slot coefficient of 0.05.
pub const EXPECTED_BLOCKS_PER_HOUR: u32 = 180;

/// How many recent blocks each sparkline looks back over.
pub const SERIES_LEN: usize = 24;

const MISSING: &str = "\u{2014}";

/// Where the current epoch is.
#[derive(Clone, Debug, PartialEq)]
pub struct EpochClock {
    pub epoch: u64,
    /// How much of the epoch has passed, `0..=1`.
    pub fraction: f32,
    pub remaining_secs: u64,
}

/// Everything the row shows, computed without a `Ui` so it can be tested.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TempoFigures {
    pub blocks_last_hour: Option<u32>,
    pub mean_gap_secs: Option<f32>,
    pub txs_per_minute: Option<f32>,
    pub mean_fullness: Option<f32>,
    /// Seconds between consecutive blocks, oldest first. A gap in heights is
    /// skipped rather than counted as one long wait.
    pub gaps: Vec<f64>,
    /// Transactions per block, oldest first.
    pub txs: Vec<f64>,
    /// Fullness per block as a percentage, oldest first.
    pub fullness: Vec<f64>,
    pub epoch: Option<EpochClock>,
}

impl TempoFigures {
    pub fn of(heartbeat: &Heartbeat, now_ms: u64) -> Self {
        let snapshot = heartbeat.snapshot(now_ms);
        let max_body = heartbeat.network().max_block_body_bytes().max(1) as f64;

        let skip = heartbeat.beats().len().saturating_sub(SERIES_LEN + 1);
        let recent: Vec<_> = heartbeat.beats().skip(skip).collect();
        let gaps = recent
            .windows(2)
            .filter(|pair| pair[1].height == pair[0].height + 1)
            .map(|pair| pair[1].slot.saturating_sub(pair[0].slot) as f64)
            .collect();
        let tail = &recent[recent.len().saturating_sub(SERIES_LEN)..];
        let txs = tail
            .iter()
            .filter_map(|b| b.tx_count)
            .map(f64::from)
            .collect();
        let fullness = tail
            .iter()
            .map(|b| b.body_size as f64 / max_body * 100.0)
            .collect();

        // The tip's slot plus the time since it began: slots are one second.
        let epoch = snapshot.epoch.map(|position| {
            let elapsed = (position.slot_in_epoch + snapshot.secs_since_block.unwrap_or(0))
                .min(position.epoch_slots);
            EpochClock {
                epoch: position.epoch,
                fraction: elapsed as f32 / position.epoch_slots.max(1) as f32,
                remaining_secs: position.epoch_slots - elapsed,
            }
        });

        Self {
            blocks_last_hour: snapshot.window.blocks_last_hour,
            mean_gap_secs: snapshot.window.mean_interval_secs,
            txs_per_minute: snapshot.window.txs_per_minute,
            mean_fullness: snapshot.window.mean_fullness,
            gaps,
            txs,
            fullness,
            epoch,
        }
    }
}

/// The row.
pub struct ChainTempo<'a> {
    heartbeat: &'a Heartbeat,
    now_ms: u64,
}

impl<'a> ChainTempo<'a> {
    /// `now_ms` is unix milliseconds, from the host's clock.
    pub fn new(heartbeat: &'a Heartbeat, now_ms: u64) -> Self {
        Self { heartbeat, now_ms }
    }

    pub fn show(self, ui: &mut Ui) {
        let figures = TempoFigures::of(self.heartbeat, self.now_ms);

        let blocks = figures
            .blocks_last_hour
            .map(|n| n.to_string())
            .unwrap_or_else(|| MISSING.to_string());
        let blocks_note = match figures.blocks_last_hour {
            Some(_) => format!("~{EXPECTED_BLOCKS_PER_HOUR} expected"),
            None => "needs an hour of feed".to_string(),
        };
        let gap = figures
            .mean_gap_secs
            .map(|secs| format!("{secs:.1}s"))
            .unwrap_or_else(|| MISSING.to_string());
        let txs = figures
            .txs_per_minute
            .map(|rate| format!("{rate:.1}"))
            .unwrap_or_else(|| MISSING.to_string());
        let full = figures
            .mean_fullness
            .map(|share| format!("{:.0}%", share * 100.0))
            .unwrap_or_else(|| MISSING.to_string());

        MetricRow::new()
            .push(MetricCard::new("Blocks, last hour", &blocks).subtitle(blocks_note))
            .push(with_series(
                MetricCard::new("Gap between blocks", &gap).subtitle("~20s expected"),
                &figures.gaps,
            ))
            .push(with_series(
                MetricCard::new("Transactions / min", &txs),
                &figures.txs,
            ))
            .push(with_series(
                MetricCard::new("Block fullness", &full),
                &figures.fullness,
            ))
            .show(ui);

        if let Some(epoch) = &figures.epoch {
            ui.gap(Space::Md);
            ProgressBar::new(epoch.fraction)
                .label(format!("Epoch {}", epoch.epoch))
                .detail(format!("ends in {}", format_duration(epoch.remaining_secs)))
                .height(6.0)
                .show(ui);
        }

        // The epoch clock and the tiles move at most once a second.
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }
}

/// Attach a sparkline only when there are enough points to draw one; a
/// one-point series would render the sparkline's "no data" placeholder inside a
/// card that does have a value.
fn with_series<'b>(card: MetricCard<'b>, series: &'b [f64]) -> MetricCard<'b> {
    if series.len() >= 2 {
        card.sparkline(series)
    } else {
        card
    }
}

#[cfg(test)]
mod tests {
    use chain_heartbeat::{BlockBeat, Network, SyncState};

    use super::*;

    const SLOT: u64 = 186_000_000;

    fn beat(height: u64, slot: u64, txs: u32) -> BlockBeat {
        BlockBeat {
            height,
            slot,
            hash: format!("{height:064x}"),
            issuer_pool: "cd".repeat(28),
            body_size: 9_011,
            tx_count: Some(txs),
            block_time_unix: Network::Mainnet.slot_to_unix_secs(slot),
        }
    }

    fn ms_at(slot: u64) -> u64 {
        Network::Mainnet.slot_to_unix_secs(slot).unwrap() * 1000
    }

    #[test]
    fn gaps_skip_breaks_in_height() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        hb.connected(ms_at(SLOT));
        for (height, slot) in [(1, SLOT), (2, SLOT + 20), (3, SLOT + 50), (9, SLOT + 400)] {
            hb.roll_forward(beat(height, slot, 5), SyncState::AtTip, ms_at(slot));
        }
        let figures = TempoFigures::of(&hb, ms_at(SLOT + 405));
        assert_eq!(figures.gaps, vec![20.0, 30.0]);
        assert_eq!(figures.txs.len(), 4);
        assert!((figures.fullness[0] - 10.0).abs() < 0.01);
        // An hour of feed has not happened, and the figure says so by being
        // absent rather than small.
        assert_eq!(figures.blocks_last_hour, None);
    }

    #[test]
    fn the_epoch_clock_counts_from_the_tip_forward() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        hb.connected(ms_at(SLOT));
        hb.roll_forward(beat(1, SLOT, 3), SyncState::AtTip, ms_at(SLOT));
        let figures = TempoFigures::of(&hb, ms_at(SLOT) + 30_000);
        let epoch = figures.epoch.expect("a Shelley-era tip has an epoch");
        assert_eq!(epoch.epoch, 628);
        // Slot 67,200 into the epoch, plus 30 seconds since the block.
        assert_eq!(epoch.remaining_secs, 432_000 - 67_230);
        assert!((epoch.fraction - 67_230.0 / 432_000.0).abs() < 1e-6);
    }

    #[test]
    fn series_are_bounded() {
        let mut hb = Heartbeat::new(Network::Mainnet);
        for i in 0..100 {
            hb.roll_forward(beat(i, SLOT + i * 20, 2), SyncState::AtTip, 0);
        }
        let figures = TempoFigures::of(&hb, ms_at(SLOT + 2000));
        assert_eq!(figures.txs.len(), SERIES_LEN);
        assert_eq!(figures.gaps.len(), SERIES_LEN);
    }
}
