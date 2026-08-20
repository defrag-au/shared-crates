use cardano_assets::AssetId;
use maestro::{AddressUtxo, ProtocolParameters};

/// Find a UTxO containing a specific asset unit.
///
/// Returns `None` if no UTxO contains the target asset.
pub fn find_asset<'a>(utxos: &'a [AddressUtxo], target: &str) -> Option<&'a AddressUtxo> {
    utxos
        .iter()
        .find(|utxo| utxo.assets.iter().any(|asset| asset.unit == target))
}

/// An asset together with the quantity that will sit in the output.
///
/// The quantity is load-bearing for min-ADA: it is CBOR-encoded inline in the
/// value, so a large quantity makes the output bigger and the min-ADA higher.
pub type AssetAmount = (AssetId, u64);

/// Byte width of `n` as a CBOR unsigned integer.
///
/// Values below 24 pack into the initial byte; above that CBOR uses a 1/2/4/8-byte
/// payload after a type byte. Getting this wrong under-sizes the output and the
/// ledger rejects the tx with `BabbageOutputTooSmallUTxO`.
fn cbor_uint_len(n: u64) -> u64 {
    match n {
        0..=23 => 1,
        24..=0xFF => 2,
        0x100..=0xFFFF => 3,
        0x1_0000..=0xFFFF_FFFF => 5,
        _ => 9,
    }
}

/// CBOR-encoded size of the *value* portion of an output carrying `assets`.
///
/// This is the quantity the ledger checks against the `maxValueSize` protocol
/// parameter (5000 bytes on mainnet); exceeding it is a permanent
/// `OutputTooBigUTxO` rejection. It is also the dominant term in min-ADA, so
/// both callers share this one implementation — a second, drifting copy is
/// exactly how the quantity width came to be wrong in the first place.
///
/// Structure: `[coin, {policy: {asset_name: quantity}}]`.
pub fn value_size(assets: &[AssetAmount]) -> u64 {
    if assets.is_empty() {
        // Pure lovelace: 1a + 4 bytes = 5 bytes typical
        return 5;
    }

    // Array [lovelace, asset_map]: 82 (1 byte)
    let array_tag: u64 = 1;
    // Lovelace: 1a + 4 bytes = 5 bytes. The coin we are solving for is always
    // well under 2^32 (that would need ~1M bytes of output), and an output
    // carrying more than 4295 ADA clears its own min by orders of magnitude,
    // so a fixed 5 never under-sizes in practice.
    let lovelace_bytes: u64 = 5;

    // Group assets by policy — same-policy assets share the 30-byte policy header.
    let mut policy_assets: std::collections::BTreeMap<&str, Vec<&AssetAmount>> =
        std::collections::BTreeMap::new();
    for asset in assets {
        policy_assets
            .entry(&asset.0.policy_id)
            .or_default()
            .push(asset);
    }

    // Policies map tag: a1-b7 (1 byte) or b9 + 2 bytes
    let policies_map_tag: u64 = if policy_assets.len() < 24 { 1 } else { 3 };

    let mut policy_size: u64 = 0;
    for assets_in_policy in policy_assets.values() {
        // Policy ID: 581c (2 bytes tag) + 28 bytes = 30 bytes
        policy_size += 30;

        // Inner assets map tag
        let inner_map_tag: u64 = if assets_in_policy.len() < 24 { 1 } else { 3 };
        policy_size += inner_map_tag;

        for (asset, quantity) in assets_in_policy {
            // Asset name: tag + bytes
            let name_len = (asset.asset_name_hex.len() / 2) as u64;
            let name_tag: u64 = if name_len < 24 {
                1 // 40-57
            } else if name_len < 256 {
                2 // 58 xx
            } else {
                3 // 59 xx xx
            };
            policy_size += name_tag + name_len;

            // Quantity, CBOR-encoded inline. NFTs (quantity 1) take a single
            // byte, which is why assuming 1 went unnoticed until a fungible
            // balance was swept.
            policy_size += cbor_uint_len(*quantity);
        }
    }

    array_tag + lovelace_bytes + policies_map_tag + policy_size
}

/// Fraction of `max_value_size` a bundle is allowed to reach.
///
/// [`value_size`] is a model of the encoder, not the encoder, so the split
/// leaves headroom. The trade is deliberately lopsided: overshooting is a
/// permanent `OutputTooBigUTxO` rejection, while an extra output costs one more
/// min-UTxO (~0.7 ADA). Matches the margin the swap builder already used.
const VALUE_SIZE_MARGIN_NUM: u64 = 9;
const VALUE_SIZE_MARGIN_DEN: u64 = 10;

/// Split `assets` into bundles that each fit within `max_value_size`.
///
/// A wallet holding hundreds of assets cannot be emptied into a single output —
/// the ledger caps an output's value at `maxValueSize` (5000 bytes) and rejects
/// the whole transaction otherwise. Each returned bundle becomes its own output
/// with its own min-ADA.
///
/// Assets are grouped by policy first so same-policy holdings stay together
/// (they share the 30-byte policy header, so splitting a policy across bundles
/// wastes space), but a single policy larger than the limit is itself split —
/// one collection can exceed 5000 bytes on its own.
///
/// Every input asset appears in exactly one bundle with its quantity intact:
/// this runs on the sweep path, where dropping one would strand it.
pub fn split_by_value_size(assets: &[AssetAmount], max_value_size: u64) -> Vec<Vec<AssetAmount>> {
    if assets.is_empty() {
        return Vec::new();
    }

    let threshold = max_value_size * VALUE_SIZE_MARGIN_NUM / VALUE_SIZE_MARGIN_DEN;

    if value_size(assets) <= threshold {
        return vec![assets.to_vec()];
    }

    // Policy-grouped, deterministic order — the same holdings must always split
    // the same way, so a rebuilt tx is the same tx.
    let mut by_policy: std::collections::BTreeMap<&str, Vec<&AssetAmount>> =
        std::collections::BTreeMap::new();
    for asset in assets {
        by_policy.entry(&asset.0.policy_id).or_default().push(asset);
    }

    let mut bundles: Vec<Vec<AssetAmount>> = Vec::new();
    let mut current: Vec<AssetAmount> = Vec::new();

    for (_policy, mut policy_assets) in by_policy {
        policy_assets.sort_by_key(|a| &a.0.asset_name_hex);

        for asset in policy_assets {
            current.push(asset.clone());

            // Recomputing per asset is O(n²), but n is bounded by what fits in a
            // transaction and the arithmetic is trivial — clarity wins here.
            if value_size(&current) > threshold {
                // Overflowed: this asset starts the next bundle. A lone asset
                // that doesn't fit is emitted anyway — the ledger will reject it,
                // which is louder and more diagnosable than silently dropping it.
                let overflow = current.pop().expect("just pushed");
                if current.is_empty() {
                    bundles.push(vec![overflow]);
                } else {
                    bundles.push(std::mem::take(&mut current));
                    current.push(overflow);
                }
            }
        }
    }

    if !current.is_empty() {
        bundles.push(current);
    }

    bundles
}

/// Parameters for calculating minimum ADA for an output
#[derive(Debug, Clone, Default)]
pub struct OutputParams {
    /// Optional inline datum size in bytes (for CIP-68 reference tokens)
    pub datum_size: Option<usize>,
}

impl OutputParams {
    /// Create params for an output with an inline datum
    pub fn with_datum(datum_bytes: &[u8]) -> Self {
        Self {
            datum_size: Some(datum_bytes.len()),
        }
    }
}

/// Calculate the minimum ADA (lovelace) required for a UTxO output based on Cardano protocol.
///
/// The minimum ADA is calculated using the formula:
/// `minUTxOValue = (160 + |serialized_output|) * coinsPerUTxOByte`
///
/// # Arguments
///
/// * `protocol_params` - Protocol parameters from the network
/// * `assets` - The assets **and quantities** that will be in the output
///
/// # Returns
///
/// Minimum lovelace required for the output
pub fn calculate_min_ada(protocol_params: &ProtocolParameters, assets: &[AssetAmount]) -> u64 {
    calculate_min_ada_with_params(protocol_params, assets, &OutputParams::default())
}

/// Calculate the minimum ADA for an output with additional parameters (e.g., inline datum).
///
/// This extended version supports:
/// - Native assets (same as `calculate_min_ada`)
/// - Inline datums (for CIP-68 reference tokens)
///
/// # Arguments
///
/// * `protocol_params` - Protocol parameters from the network
/// * `assets` - The assets **and quantities** that will be in the output
/// * `params` - Additional output parameters (datum size, etc.)
///
/// # Returns
///
/// Minimum lovelace required for the output
///
/// # CBOR Structure Reference
///
/// From actual transaction analysis, a Babbage/Conway output with datum looks like:
/// ```text
/// a3                              -- map(3) - 1 byte
///   00                            -- key 0 - 1 byte
///   5839 [57 bytes]               -- address (59 bytes: 2 tag + 57 data)
///   01                            -- key 1 - 1 byte
///   82                            -- array(2) - 1 byte
///     1a [4 bytes]                -- lovelace (5 bytes)
///     a1                          -- map(1) policies - 1 byte
///       581c [28 bytes]           -- policy id (30 bytes: 2 tag + 28 data)
///       a1                        -- map(1) assets - 1 byte
///         54 [20 bytes]           -- asset name (21 bytes: 1 tag + 20 data)
///         01                      -- quantity (1-9 bytes, CBOR uint width)
///   02                            -- key 2 - 1 byte
///   82 01                         -- array [1, ...] inline datum - 2 bytes
///     d8 18                       -- tag(24) embedded CBOR - 2 bytes
///     58 f1 [241 bytes]           -- datum bytes (244 bytes: 2 len + 241 data)
/// ```
/// Total: 370 bytes for this example
pub fn calculate_min_ada_with_params(
    protocol_params: &ProtocolParameters,
    assets: &[AssetAmount],
    params: &OutputParams,
) -> u64 {
    // Conway/Babbage formula: (160 + |serialized_output|) * coinsPerUTxOByte
    const UTXO_OVERHEAD: u64 = 160;

    // Output map header: a2 (no datum) or a3 (with datum) = 1 byte
    let map_header: u64 = 1;

    // Key 0 (1 byte) + Address (59 bytes for typical address with stake key)
    // Address breakdown: 5839 (2 bytes) + 57 bytes raw = 59 bytes
    let address_with_key: u64 = 1 + 59;

    // Key 1 (1 byte) + Value
    let value_with_key: u64 = 1 + value_size(assets);

    // Key 2 (1 byte) + Datum (if present)
    // Inline datum structure: 82 01 d8 18 58/59 xx [datum bytes]
    let datum_with_key: u64 = match params.datum_size {
        Some(size) => {
            // 82 01 = array [1, ...] indicating inline datum = 2 bytes
            // d8 18 = tag(24) embedded CBOR = 2 bytes
            let datum_wrapper: u64 = 4;
            // Length prefix: 58 xx (2 bytes) or 59 xx xx (3 bytes)
            let length_prefix: u64 = if size < 256 { 2 } else { 3 };
            // Key 02 = 1 byte
            1 + datum_wrapper + length_prefix + size as u64
        }
        None => 0,
    };

    let serialized_output_size = map_header + address_with_key + value_with_key + datum_with_key;

    // Apply Babbage/Conway formula (no safety margin needed with correct calculation)
    (UTXO_OVERHEAD + serialized_output_size) * protocol_params.min_utxo_deposit_coefficient
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test against actual transaction data from logs
    #[test]
    fn test_min_ada_calculation_matches_real_tx() {
        // From failed transaction:
        // - Asset name hex: 000643b0686f646c63726f66745f303030303034 (20 bytes)
        // - Policy ID: 88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9
        // - Datum size: 241 bytes
        // - Required min UTxO: 2284300 lovelace

        let protocol_params = ProtocolParameters {
            min_utxo_deposit_coefficient: 4310,
            min_fee_coefficient: 44,
            min_fee_constant: maestro::AdaLovelace {
                ada: maestro::AdaAmount { lovelace: 155381 },
            },
            script_execution_prices: None,
            max_execution_units_per_transaction: None,
            max_transaction_size: None,
            plutus_cost_models: None,
        };

        // Create asset ID matching the transaction
        let asset_id: AssetId =
            "88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9.000643b0686f646c63726f66745f303030303034"
                .parse()
                .unwrap();

        let params = OutputParams {
            datum_size: Some(241),
        };

        let calculated = calculate_min_ada_with_params(&protocol_params, &[(asset_id, 1)], &params);

        // Expected: (160 + 370) * 4310 = 2284300
        assert_eq!(
            calculated, 2284300,
            "Calculated {calculated} but expected 2284300"
        );
    }

    /// Regression: a fungible balance was swept and the ledger rejected the tx with
    /// `BabbageOutputTooSmallUTxO (... Coin 1155080 ..., Coin 1163700)`. The
    /// quantity 46500 CBOR-encodes as `19 b5 a4` — 3 bytes, not the 1 byte the
    /// size estimate assumed — so the output was priced 2 bytes (8620 lovelace)
    /// short. 1163700 is the ledger's own number from that rejection.
    #[test]
    fn min_ada_accounts_for_multi_byte_token_quantities() {
        let protocol_params = params_with_coefficient(4310);
        let asset_id: AssetId =
            "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7.416c69656e73"
                .parse()
                .unwrap();

        assert_eq!(
            calculate_min_ada(&protocol_params, &[(asset_id.clone(), 46_500)]),
            1_163_700,
            "must match the ledger's required Coin for this output"
        );

        // The same asset at NFT quantity is 2 bytes smaller — the old behaviour,
        // still correct for quantity 1.
        assert_eq!(
            calculate_min_ada(&protocol_params, &[(asset_id, 1)]),
            1_155_080
        );
    }

    #[test]
    fn cbor_uint_len_matches_encoding_widths() {
        assert_eq!(cbor_uint_len(0), 1);
        assert_eq!(cbor_uint_len(23), 1);
        assert_eq!(cbor_uint_len(24), 2);
        assert_eq!(cbor_uint_len(255), 2);
        assert_eq!(cbor_uint_len(256), 3);
        assert_eq!(cbor_uint_len(46_500), 3);
        assert_eq!(cbor_uint_len(65_535), 3);
        assert_eq!(cbor_uint_len(65_536), 5);
        assert_eq!(cbor_uint_len(u32::MAX as u64), 5);
        assert_eq!(cbor_uint_len(u32::MAX as u64 + 1), 9);
        assert_eq!(cbor_uint_len(u64::MAX), 9);
    }

    /// Min-ADA must never shrink as a quantity grows, or a rehome output sized
    /// from a smaller balance would be rejected.
    #[test]
    fn min_ada_is_monotonic_in_quantity() {
        let protocol_params = params_with_coefficient(4310);
        let asset_id: AssetId =
            "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7.416c69656e73"
                .parse()
                .unwrap();

        let mut previous = 0;
        for quantity in [1u64, 23, 24, 255, 256, 46_500, 65_536, u64::MAX] {
            let min = calculate_min_ada(&protocol_params, &[(asset_id.clone(), quantity)]);
            assert!(min >= previous, "min-ADA shrank at quantity {quantity}");
            previous = min;
        }
    }

    /// Mainnet `maxValueSize`.
    const MAX_VALUE_SIZE: u64 = 5_000;

    fn amounts(spec: &[(&str, usize, u64, u64)]) -> Vec<AssetAmount> {
        let mut out = Vec::new();
        for (policy, name_bytes, count, quantity) in spec {
            for i in 0..*count {
                let name_hex = format!("{i:0width$x}", width = name_bytes * 2);
                let id: AssetId = format!("{policy}.{name_hex}")
                    .parse()
                    .expect("valid asset id");
                out.push((id, *quantity));
            }
        }
        out
    }

    /// The holdings of the wallet that failed with
    /// `OutputTooBigUTxO ((5051, 5000, ...))`, transcribed from that rejection:
    /// 22 policies, three of them large NFT collections, plus fungible balances.
    /// `(policy, asset_name_bytes, asset_count, quantity)`.
    ///
    /// Names within a collection vary by a byte or two on chain and are levelled
    /// here, so this reconstructs ~4999 bytes against the ledger's measured 5051
    /// — close enough to prove the model tracks the encoder, not close enough to
    /// treat as byte-exact.
    fn over_cap_wallet() -> Vec<AssetAmount> {
        amounts(&[
            (
                "0f95ac8149efc9c09e3894cc2e776503967e041d258570820b2206ba",
                8,
                5,
                3,
            ),
            (
                "23c9b05ee16199b4955f7c491c887b9bd53d77a4af65105c54fe13eb",
                20,
                1,
                3,
            ),
            (
                "25a549dffe519c58fb88f0c794f08a6b64f9e5aa505c4c09e3ffffb8",
                19,
                1,
                16_000,
            ),
            (
                "27fa5cc3d1c2ed825f799ee41c04621d8b6703d86e66390748e7b248",
                4,
                1,
                2_222,
            ),
            (
                "33723d57aa75ca98ba270e97a3fc5d3b780bfb49345f3cbe11aca896",
                19,
                1,
                1,
            ),
            (
                "3ae5d821583d7650db7eeabedf63875800100695045f1a27e6961360",
                11,
                9,
                1,
            ),
            (
                "49e423161ef818adc475c783571cb479d5f15ad52a01a240eacc0d3b",
                4,
                1,
                69,
            ),
            (
                "4f72dd6b2ac871e99424292129362027dbf4b7f7e378c04b404c641c",
                7,
                96,
                1,
            ),
            (
                "5c9164aae7f5f634c89a58eda90e71df0325775361a931c56524670e",
                5,
                1,
                4_800,
            ),
            (
                "5df06bb670682ee2b1b29f74b34797a7be684de02dad4abd14abd719",
                15,
                1,
                1,
            ),
            (
                "61e129c0530f21c0e32eff42ec16d90cc185ecdcbedb113dc70f1a91",
                8,
                3,
                1,
            ),
            (
                "812197d5f4cdd9ebb05d40e259c181982d4b3d8c2505b1a7ad800bdc",
                20,
                3,
                1,
            ),
            (
                "84c1b4ee5762b0407b07f3b9127f8744ad51b69961ce317f6d22ff7f",
                4,
                1,
                100,
            ),
            (
                "9436685ca32fc8c5a4ad2dd8597c545b084791d29ad40eea93cde57f",
                19,
                1,
                1,
            ),
            (
                "a2cdc9c73c59d668d5ae2c4dcd51447767362e8a325eaedc320c58f4",
                8,
                1,
                63_448,
            ),
            (
                "c05120ace508c83b95da99337d501907ae15004e21923c7baecd9387",
                18,
                65,
                1,
            ),
            (
                "db19ac77382b9f0907ea525a4a4aa5fca96cbc001574edd59b641fef",
                15,
                94,
                1,
            ),
            (
                "defe216460d594211631fcfbd354f361c04645d6a0cfeead3d6f6283",
                11,
                1,
                69,
            ),
            (
                "df9031ff5d9440c9416e599582e6a63e9c7547383460b4154054ab3a",
                8,
                1,
                1,
            ),
            (
                "e7434f9a052a882aba9564c673f2bd7bc02b22d0676e97b093f4aeaf",
                13,
                5,
                1,
            ),
            (
                "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a",
                12,
                1,
                1,
            ),
            (
                "f6e6b1c8e22d6024b9b1d18a4c50367e69fd37ac953bddcd1fb4539f",
                15,
                1,
                9_600,
            ),
        ])
    }

    /// The regression: this wallet's holdings do not fit in one output, and the
    /// ledger rejects the whole transaction rather than truncating. Every bundle
    /// the split produces must be under the cap.
    #[test]
    fn oversized_holdings_split_under_max_value_size() {
        let all = over_cap_wallet();

        // The ledger measured 5051 for the real output. If our model drifts far
        // from that, the split threshold is being computed against fiction.
        let estimated = value_size(&all);
        assert!(
            estimated.abs_diff(5_051) < 200,
            "estimate {estimated} drifted from the ledger's measured 5051 — the model is wrong"
        );

        let bundles = split_by_value_size(&all, MAX_VALUE_SIZE);
        assert!(bundles.len() > 1, "must actually split");

        for (i, bundle) in bundles.iter().enumerate() {
            assert!(
                value_size(bundle) <= MAX_VALUE_SIZE,
                "bundle {i} is {} bytes, over the {MAX_VALUE_SIZE} cap",
                value_size(bundle)
            );
        }
    }

    /// THE invariant for a sweep: splitting must not lose an asset or change a
    /// quantity. A dropped asset is stranded in a wallet being retired.
    #[test]
    fn splitting_preserves_every_asset_and_quantity() {
        let all = amounts(&[
            (
                "4f72dd6b2ac871e99424292129362027dbf4b7f7e378c04b404c641c",
                7,
                96,
                1,
            ),
            (
                "c05120ace508c83b95da99337d501907ae15004e21923c7baecd9387",
                9,
                65,
                3,
            ),
            (
                "db19ac77382b9f0907ea525a4a4aa5fca96cbc001574edd59b641fef",
                9,
                94,
                1,
            ),
        ]);

        let bundles = split_by_value_size(&all, MAX_VALUE_SIZE);

        let mut flattened: Vec<_> = bundles.into_iter().flatten().collect();
        flattened.sort_by(|a, b| a.0.concatenated().cmp(&b.0.concatenated()));
        let mut expected = all.clone();
        expected.sort_by(|a, b| a.0.concatenated().cmp(&b.0.concatenated()));

        assert_eq!(flattened, expected, "assets must survive the split intact");
    }

    /// A single collection can exceed the cap on its own — splitting only at
    /// policy boundaries would emit a bundle the ledger still rejects.
    #[test]
    fn a_single_oversized_policy_is_split() {
        let one_policy = amounts(&[(
            "4f72dd6b2ac871e99424292129362027dbf4b7f7e378c04b404c641c",
            32,
            400,
            1,
        )]);

        assert!(value_size(&one_policy) > MAX_VALUE_SIZE);

        let bundles = split_by_value_size(&one_policy, MAX_VALUE_SIZE);
        assert!(bundles.len() > 1);
        for bundle in &bundles {
            assert!(value_size(bundle) <= MAX_VALUE_SIZE);
        }
    }

    #[test]
    fn small_holdings_stay_in_one_bundle() {
        let few = amounts(&[(
            "16657df32ad8eaa8f8c628586ac6b8ba3771226c12bd69b582738fb7",
            6,
            3,
            46_500,
        )]);

        assert_eq!(split_by_value_size(&few, MAX_VALUE_SIZE).len(), 1);
        assert!(split_by_value_size(&[], MAX_VALUE_SIZE).is_empty());
    }

    /// Splitting must be deterministic — a rebuilt transaction has to be the
    /// same transaction, or retry/idempotency reasoning breaks.
    #[test]
    fn splitting_is_deterministic() {
        let all = amounts(&[
            (
                "c05120ace508c83b95da99337d501907ae15004e21923c7baecd9387",
                9,
                65,
                1,
            ),
            (
                "db19ac77382b9f0907ea525a4a4aa5fca96cbc001574edd59b641fef",
                9,
                94,
                1,
            ),
            (
                "4f72dd6b2ac871e99424292129362027dbf4b7f7e378c04b404c641c",
                7,
                96,
                1,
            ),
        ]);

        assert_eq!(
            split_by_value_size(&all, MAX_VALUE_SIZE),
            split_by_value_size(&all, MAX_VALUE_SIZE)
        );
    }

    fn params_with_coefficient(coefficient: u64) -> ProtocolParameters {
        ProtocolParameters {
            min_utxo_deposit_coefficient: coefficient,
            min_fee_coefficient: 44,
            min_fee_constant: maestro::AdaLovelace {
                ada: maestro::AdaAmount { lovelace: 155381 },
            },
            script_execution_prices: None,
            max_execution_units_per_transaction: None,
            max_transaction_size: None,
            plutus_cost_models: None,
        }
    }
}
