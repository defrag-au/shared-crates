//! The public networks: magic, default relay, slot time and epochs.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Probability that any given slot has a block (`activeSlotsCoeff` in the
/// Shelley genesis, the same on all three networks).
pub const ACTIVE_SLOT_COEFFICIENT: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet,
    Preprod,
    Preview,
}

/// A relay to follow from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relay {
    pub host: &'static str,
    pub port: u16,
}

/// Where a slot sits in its epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochPosition {
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub epoch: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub slot_in_epoch: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub epoch_slots: u64,
}

/// The first Shelley-era slot and what follows from it. Every slot from here on
/// is one second long.
struct ShelleyStart {
    slot: u64,
    unix_secs: u64,
    epoch: u64,
    epoch_slots: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown network {0:?}; expected mainnet, preprod or preview")]
pub struct UnknownNetwork(pub String);

impl Network {
    pub const ALL: [Network; 3] = [Network::Mainnet, Network::Preprod, Network::Preview];

    pub const fn magic(self) -> u64 {
        match self {
            Network::Mainnet => 764_824_073,
            Network::Preprod => 1,
            Network::Preview => 2,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Preprod => "preprod",
            Network::Preview => "preview",
        }
    }

    /// A public relay run by the network's stewards. Mainnet's is the one mitos
    /// and Oura on cardano-infra already follow.
    pub const fn default_relay(self) -> Relay {
        match self {
            Network::Mainnet => Relay {
                host: "backbone.mainnet.cardanofoundation.org",
                port: 3001,
            },
            Network::Preprod => Relay {
                host: "preprod-node.world.dev.cardano.org",
                port: 3001,
            },
            Network::Preview => Relay {
                host: "preview-node.world.dev.cardano.org",
                port: 3001,
            },
        }
    }

    /// `maxBlockBodySize` in bytes. A protocol parameter, so governance can
    /// change it; this is the value in force on all three networks as of
    /// 2026-09. Fullness figures are relative to it.
    pub const fn max_block_body_bytes(self) -> u32 {
        90_112
    }

    /// Byron slots were 20 s, so a naive `system_start + slot` is wrong on every
    /// network that had a Byron era (by ~19 days on preprod). These anchors
    /// are the first Shelley slot of each network.
    const fn shelley_start(self) -> ShelleyStart {
        match self {
            // The Shelley hard fork block: slot 4,492,800 at 2020-07-29T21:44:51Z.
            Network::Mainnet => ShelleyStart {
                slot: 4_492_800,
                unix_secs: 1_596_059_091,
                epoch: 208,
                epoch_slots: 432_000,
            },
            // System start 2022-06-01T00:00:00Z plus four Byron epochs of
            // 21,600 slots at 20 s.
            Network::Preprod => ShelleyStart {
                slot: 86_400,
                unix_secs: 1_655_769_600,
                epoch: 4,
                epoch_slots: 432_000,
            },
            // No Byron era: system start 2022-10-25T00:00:00Z, one-day epochs.
            Network::Preview => ShelleyStart {
                slot: 0,
                unix_secs: 1_666_656_000,
                epoch: 0,
                epoch_slots: 86_400,
            },
        }
    }

    /// Wall-clock time of `slot`, or `None` for a Byron-era slot.
    pub fn slot_to_unix_secs(self, slot: u64) -> Option<u64> {
        let start = self.shelley_start();
        slot.checked_sub(start.slot)
            .map(|since| start.unix_secs + since)
    }

    /// Epoch and offset of `slot`, or `None` for a Byron-era slot.
    pub fn epoch_position(self, slot: u64) -> Option<EpochPosition> {
        let start = self.shelley_start();
        let since = slot.checked_sub(start.slot)?;
        Some(EpochPosition {
            epoch: start.epoch + since / start.epoch_slots,
            slot_in_epoch: since % start.epoch_slots,
            epoch_slots: start.epoch_slots,
        })
    }
}

impl FromStr for Network {
    type Err = UnknownNetwork;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Network::ALL
            .into_iter()
            .find(|n| n.name().eq_ignore_ascii_case(s.trim()))
            .ok_or_else(|| UnknownNetwork(s.to_string()))
    }
}

/// Chance that at least one block lands within `elapsed_secs` of the last one.
///
/// Treats each slot as an independent draw at [`ACTIVE_SLOT_COEFFICIENT`], so
/// the hazard is constant: the chain is never "due". Missed leader slots and
/// slot battles make real gaps run slightly longer than this, so read it as an
/// upper bound on the likelihood, and never as a countdown.
pub fn block_probability_within(elapsed_secs: u64) -> f64 {
    1.0 - (1.0 - ACTIVE_SLOT_COEFFICIENT).powf(elapsed_secs as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mainnet_slot_time_is_anchored_on_the_shelley_hard_fork_block() {
        assert_eq!(
            Network::Mainnet.slot_to_unix_secs(4_492_800),
            Some(1_596_059_091)
        );
        assert_eq!(Network::Mainnet.slot_to_unix_secs(4_492_799), None);
    }

    /// Observed preprod block, the same anchor the minting engine's corrected
    /// helper is tested against.
    #[test]
    fn preprod_slot_time_matches_an_observed_block() {
        assert_eq!(
            Network::Preprod.slot_to_unix_secs(125_371_073),
            Some(1_781_054_273)
        );
    }

    #[test]
    fn epochs_start_at_the_shelley_boundary() {
        assert_eq!(
            Network::Mainnet.epoch_position(4_492_800),
            Some(EpochPosition {
                epoch: 208,
                slot_in_epoch: 0,
                epoch_slots: 432_000
            })
        );
        assert_eq!(
            Network::Mainnet.epoch_position(186_000_000),
            Some(EpochPosition {
                epoch: 628,
                slot_in_epoch: 67_200,
                epoch_slots: 432_000
            })
        );
        assert_eq!(Network::Preview.epoch_position(86_401).unwrap().epoch, 1);
    }

    #[test]
    fn parses_network_names() {
        assert_eq!("Mainnet".parse::<Network>().unwrap(), Network::Mainnet);
        assert_eq!(" preprod ".parse::<Network>().unwrap(), Network::Preprod);
        assert!("sanchonet".parse::<Network>().is_err());
    }

    #[test]
    fn block_probability_is_memoryless_and_bounded() {
        assert_eq!(block_probability_within(0), 0.0);
        let at_median = block_probability_within(14);
        assert!((0.5..0.52).contains(&at_median), "{at_median}");
        // Three silent minutes happen about once in ten thousand gaps.
        let three_minutes = 1.0 - block_probability_within(180);
        assert!(three_minutes < 1.2e-4, "{three_minutes}");
    }
}
