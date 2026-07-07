//! Centralized Plutus cost models for Cardano transaction building.
//!
//! Cost models are required for computing `script_data_hash` in Plutus
//! transactions. The language view (cost model) must be included when calling
//! `StagingTransaction::add_language()`, and it MUST match the node's current
//! protocol parameters *exactly* — otherwise the ledger rejects the tx with
//! `PPViewHashesDontMatch` (the supplied script-integrity hash differs from the
//! one the node recomputes from its own cost models).
//!
//! Cost models change at protocol updates / hard forks — both the parameter
//! *values* and the *number* of parameters per language shift (e.g. PlutusV2
//! grew 175 → 332, PlutusV3 297 → 350 across the Conway updates). A hardcoded
//! snapshot therefore goes stale and silently breaks every script-spend.
//!
//! Prefer the **live** values carried on [`PlutusCostModels`], sourced from
//! protocol parameters at build time. The constants below are only a
//! current-as-of-Conway *fallback* for callers with no protocol-params source;
//! refresh them from a node / Maestro `protocol-parameters` when they drift.

/// PlutusV2 cost model — 332 values (mainnet, Conway; live-sourced fallback).
pub const PLUTUS_V2_COST_MODEL: [i64; 332] = [
    100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4, 16000, 100,
    16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100, 16000, 100, 94375, 32,
    132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 228465, 122, 0, 1,
    1, 1000, 42921, 4, 2, 30623, 28755, 75, 1, 898148, 27279, 1, 51775, 558, 1, 39184, 1000, 60594,
    1, 141895, 32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1,
    43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32,
    85848, 228465, 122, 0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848,
    228465, 122, 0, 1, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4,
    0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933,
    32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10, 1293828, 28716, 63, 0,
    1, 1006041, 43623, 251, 0, 1, 16000, 100, 16000, 100, 962335, 18, 2780678, 6, 442008, 1,
    52538055, 3756, 18, 267929, 18, 76433006, 8868, 18, 52948122, 18, 1995836, 36, 3227919, 12,
    901022, 1, 166917843, 4307, 36, 284546, 36, 158221314, 26549, 36, 74698472, 36, 333849714, 1,
    254006273, 72, 2174038, 72, 2261318, 64571, 4, 207616, 8310, 4, 100181, 726, 719, 0, 1, 100181,
    726, 719, 0, 1, 100181, 726, 719, 0, 1, 107878, 680, 0, 1, 95336, 1, 281145, 18848, 0, 1,
    180194, 159, 1, 1, 158519, 8942, 0, 1, 159378, 8813, 0, 1, 107490, 3298, 1, 106057, 655, 1,
    1964219, 24520, 3, 607153, 231697, 53144, 0, 1, 116711, 1957, 4, 231883, 10, 1000, 24838, 7, 1,
    232010, 32, 321837444, 25087669, 18, 617887431, 67302824, 36, 356924, 18413, 45, 21, 219951,
    9444, 1, 1000, 172116, 183150, 6, 24, 21, 213283, 618401, 1998, 28258, 1, 1000, 38159, 2, 22,
    1000, 95933, 1, 1, 11, 1000, 277577, 12, 21,
];

/// PlutusV3 cost model — 350 values (mainnet, Conway; live-sourced fallback).
pub const PLUTUS_V3_COST_MODEL: [i64; 350] = [
    100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4, 16000, 100,
    16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100, 16000, 100, 94375, 32,
    132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 123203, 7305, -900,
    1716, 960, 57, 85848, 0, 1, 1, 1000, 42921, 4, 2, 30623, 28755, 75, 1, 898148, 27279, 1, 51775,
    558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10,
    28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32,
    7243, 32, 7391, 32, 11546, 32, 85848, 123203, 7305, -900, 1716, 960, 57, 85848, 0, 1, 90434,
    519, 0, 1, 74433, 32, 85848, 123203, 7305, -900, 1716, 960, 57, 85848, 0, 1, 1, 85848, 123203,
    7305, -900, 1716, 960, 57, 85848, 0, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566,
    4, 20467, 1, 4, 0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32,
    20744, 32, 25933, 32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10, 16000,
    100, 16000, 100, 962335, 18, 2780678, 6, 442008, 1, 52538055, 3756, 18, 267929, 18, 76433006,
    8868, 18, 52948122, 18, 1995836, 36, 3227919, 12, 901022, 1, 166917843, 4307, 36, 284546, 36,
    158221314, 26549, 36, 74698472, 36, 333849714, 1, 254006273, 72, 2174038, 72, 2261318, 64571,
    4, 207616, 8310, 4, 1293828, 28716, 63, 0, 1, 1006041, 43623, 251, 0, 1, 100181, 726, 719, 0,
    1, 100181, 726, 719, 0, 1, 100181, 726, 719, 0, 1, 107878, 680, 0, 1, 95336, 1, 281145, 18848,
    0, 1, 180194, 159, 1, 1, 158519, 8942, 0, 1, 159378, 8813, 0, 1, 107490, 3298, 1, 106057, 655,
    1, 1964219, 24520, 3, 607153, 231697, 53144, 0, 1, 116711, 1957, 4, 231883, 10, 1000, 24838, 7,
    1, 232010, 32, 321837444, 25087669, 18, 617887431, 67302824, 36, 356924, 18413, 45, 21, 219951,
    9444, 1, 1000, 172116, 183150, 6, 24, 21, 213283, 618401, 1998, 28258, 1, 1000, 38159, 2, 22,
    1000, 95933, 1, 1, 11, 1000, 277577, 12, 21,
];

/// Per-language Plutus cost models, as carried on protocol parameters.
///
/// Threaded onto [`crate::params::TxBuildParams`] so script-spending builders
/// fold the node's *current* cost model into the script-integrity hash rather
/// than a hardcoded snapshot. A `None` variant means "not supplied"; the
/// resolvers ([`Self::v2`] / [`Self::v3`]) then fall back to the bundled
/// constants above so a missing field degrades rather than panics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlutusCostModels {
    pub plutus_v1: Option<Vec<i64>>,
    pub plutus_v2: Option<Vec<i64>>,
    pub plutus_v3: Option<Vec<i64>>,
}

impl PlutusCostModels {
    /// All-absent set — usable in `const` contexts and by callers that have no
    /// protocol-parameters source (resolvers fall back to the bundled constants).
    pub const EMPTY: Self = Self {
        plutus_v1: None,
        plutus_v2: None,
        plutus_v3: None,
    };

    /// The live PlutusV2 cost model if supplied, else the bundled fallback.
    pub fn v2(&self) -> Vec<i64> {
        self.plutus_v2
            .clone()
            .unwrap_or_else(|| PLUTUS_V2_COST_MODEL.to_vec())
    }

    /// The live PlutusV3 cost model if supplied, else the bundled fallback.
    pub fn v3(&self) -> Vec<i64> {
        self.plutus_v3
            .clone()
            .unwrap_or_else(|| PLUTUS_V3_COST_MODEL.to_vec())
    }
}
