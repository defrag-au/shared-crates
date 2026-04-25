/// Enhanced mint type detection for UTXO analysis with CIP-68 support
/// Adapted from the sophisticated detection logic in rules.rs
pub(crate) fn detect_utxo_mint_type(
    minted_assets: &[(String, String, u64)], // (asset_id, address, amount)
    tx_data: &crate::RawTxData,
) -> crate::MintType {
    use cardano_assets::NftPurpose;
    use tracing::debug;

    // Type alias to reduce complexity
    type BaseNameGroups =
        std::collections::HashMap<String, Vec<((String, String, u64), NftPurpose)>>;

    // Extract asset info for analysis
    let mut asset_info: Vec<(String, String, u64)> = Vec::new(); // (policy_id, asset_name, amount)

    for (asset_id, _address, amount) in minted_assets {
        use pipeline_types::cardano::POLICY_ID_LENGTH;

        if asset_id.len() >= POLICY_ID_LENGTH {
            let policy_id = asset_id[..POLICY_ID_LENGTH].to_string();
            let asset_name = asset_id[POLICY_ID_LENGTH..].to_string();
            asset_info.push((policy_id, asset_name, *amount));
        }
    }

    // Check for fungible tokens (quantity > 1)
    if asset_info.iter().any(|(_, _, amount)| *amount > 1) {
        return crate::MintType::Fungible;
    }

    // Check if this could be a CIP-68 mint by grouping tokens by base name and checking for reference/user pairs
    if asset_info.len() >= 2 && asset_info.len().is_multiple_of(2) {
        // Group tokens by their base names (after CIP-68 prefix)
        let mut base_name_groups: BaseNameGroups = std::collections::HashMap::new();

        for asset in &asset_info {
            let asset_name = &asset.1;
            let purpose = NftPurpose::from(asset_name.as_str());

            // Only consider tokens with CIP-68 prefixes
            use pipeline_types::cardano::CIP68_PREFIX_LENGTH;

            if matches!(purpose, NftPurpose::ReferenceNft | NftPurpose::UserNft)
                && asset_name.len() >= CIP68_PREFIX_LENGTH
            {
                let base_name = asset_name[CIP68_PREFIX_LENGTH..].to_string();
                base_name_groups
                    .entry(base_name)
                    .or_default()
                    .push((asset.clone(), purpose));
            }
        }

        // Check if all tokens can be grouped into perfect reference/user pairs
        if !base_name_groups.is_empty() && base_name_groups.len() * 2 == asset_info.len() {
            let mut all_pairs_valid = true;
            let mut cip68_pairs = Vec::new();

            for (base_name, tokens) in &base_name_groups {
                if tokens.len() != 2 {
                    all_pairs_valid = false;
                    break;
                }

                // Check if we have exactly one reference and one user token
                let has_reference = tokens
                    .iter()
                    .any(|(_, purpose)| matches!(purpose, NftPurpose::ReferenceNft));
                let has_user = tokens
                    .iter()
                    .any(|(_, purpose)| matches!(purpose, NftPurpose::UserNft));

                if !has_reference || !has_user {
                    all_pairs_valid = false;
                    break;
                }

                // Find the reference token for datum check
                let reference_token = tokens
                    .iter()
                    .find(|(_, purpose)| matches!(purpose, NftPurpose::ReferenceNft))
                    .map(|(asset, _)| asset);

                let user_token = tokens
                    .iter()
                    .find(|(_, purpose)| matches!(purpose, NftPurpose::UserNft))
                    .map(|(asset, _)| asset);

                if let (Some(ref_asset), Some(user_asset)) = (reference_token, user_token) {
                    cip68_pairs.push((ref_asset, user_asset, base_name));
                }
            }

            if all_pairs_valid && !cip68_pairs.is_empty() {
                // Additional validations for CIP-68
                let mut valid_cip68_pairs = 0;

                for (ref_asset, _user_asset, base_name) in &cip68_pairs {
                    let full_reference_asset_id = format!("{}{}", ref_asset.0, ref_asset.1);

                    // Check if any output containing the reference NFT has a datum
                    let has_datum = tx_data.outputs.iter().any(|output| {
                        output
                            .assets
                            .iter()
                            .any(|(asset_id, _)| asset_id == &full_reference_asset_id)
                            && output.datum.is_some()
                    });

                    // Relax metadata requirement - some CIP-68 implementations include metadata
                    if has_datum {
                        valid_cip68_pairs += 1;
                        debug!(
                            "Detected CIP-68 pair via UTXO analysis: reference={}, user={}, base_name={}",
                            ref_asset.1,
                            _user_asset.1,
                            base_name
                        );
                    }
                }

                // If all pairs are valid CIP-68 pairs, classify as CIP-68
                if valid_cip68_pairs == cip68_pairs.len() {
                    debug!(
                        "Confirmed CIP-68 multi-mint via UTXO: {} NFTs ({} token pairs)",
                        cip68_pairs.len(),
                        asset_info.len()
                    );
                    return crate::MintType::Cip68;
                }
            }
        }
    }

    // Check for clear CIP-25 indicators (transaction metadata with label 721)
    if let Some(metadata) = &tx_data.metadata {
        use pipeline_types::cip::CIP25_METADATA_LABEL;

        if metadata.get(CIP25_METADATA_LABEL).is_some() {
            return crate::MintType::Cip25;
        }
    }

    // Default assumption for NFTs with quantity 1 and metadata
    if asset_info.iter().all(|(_, _, amount)| *amount == 1) && tx_data.metadata.is_some() {
        return crate::MintType::Cip25;
    }

    crate::MintType::Unknown
}

/// Separate minted assets into primary assets and reference assets based on mint type
/// For CIP-68: UserNfts (primary) and ReferenceNfts (reference)
/// For others: All assets are primary, no reference assets
pub(crate) fn separate_assets_by_mint_type(
    minted_assets: &[(String, String, u64)], // (asset_id, address, amount)
    mint_type: &crate::MintType,
) -> (Vec<crate::AssetId>, Vec<crate::AssetId>) {
    use cardano_assets::NftPurpose;

    if matches!(mint_type, crate::MintType::Cip68) {
        // For CIP-68, separate UserNfts (primary) from ReferenceNfts (reference)
        let mut user_nfts = Vec::new();
        let mut reference_nfts = Vec::new();

        for (asset_id, _address, _amount) in minted_assets {
            let Some(asset) = crate::AssetId::parse_concatenated(asset_id).ok() else {
                continue;
            };

            use pipeline_types::cardano::POLICY_ID_LENGTH;

            if asset_id.len() >= POLICY_ID_LENGTH + 8 {
                // Policy ID + minimum asset name
                let asset_name = &asset_id[POLICY_ID_LENGTH..];
                let purpose = NftPurpose::from(asset_name);

                match purpose {
                    NftPurpose::UserNft => user_nfts.push(asset),
                    NftPurpose::ReferenceNft => reference_nfts.push(asset),
                    _ => user_nfts.push(asset), // Fallback to primary assets
                }
            } else {
                user_nfts.push(asset); // Fallback for malformed asset IDs
            }
        }

        user_nfts.sort();
        reference_nfts.sort();
        (user_nfts, reference_nfts)
    } else {
        // For non-CIP-68 mints, all assets are primary assets
        let mut all_assets: Vec<crate::AssetId> = minted_assets
            .iter()
            .filter_map(|(asset_id, _address, _amount)| {
                crate::AssetId::parse_concatenated(asset_id).ok()
            })
            .collect();

        all_assets.sort();
        (all_assets, Vec::new())
    }
}
