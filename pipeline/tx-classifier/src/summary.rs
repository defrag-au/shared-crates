use serde::Serialize;

use crate::{AddressLookup, Confidence, SmartContractRegistry, TxClassification, TxTag, TxType};

#[derive(Serialize)]
pub struct TxSummary {
    pub description: String,
    pub tx_hash: String,
    pub tx_types: Vec<TxType>,
    pub tags: Vec<TxTag>,
    pub confidence: Confidence,
    pub score: f64,
}

impl From<TxClassification> for TxSummary {
    fn from(value: TxClassification) -> Self {
        let description = generate_insight_summary(&value.tx_types, &value.tags);

        Self {
            description,
            tx_hash: value.tx_hash,
            tx_types: value.tx_types,
            tags: value.tags,
            confidence: value.confidence,
            score: value.score,
        }
    }
}

/// Identify the primary human actor by filtering out script addresses
fn identify_primary_actor(addresses: &[&str]) -> Option<String> {
    let registry = SmartContractRegistry::new();

    // Filter out known script addresses and marketplace addresses
    for address in addresses {
        if !registry.is_known_script(address) && !registry.is_known_address(address) {
            return Some(address.to_string());
        }
    }

    // If all addresses are known scripts/contracts, return the first non-marketplace address
    for address in addresses {
        if !registry.is_known_address(address) {
            return Some(address.to_string());
        }
    }

    // Fallback: return the first address if no filtering worked
    addresses.first().map(|addr| addr.to_string())
}

/// Determine if a transaction type represents a primary user action vs a secondary side effect
fn is_primary_action(tx_type: &TxType) -> bool {
    match tx_type {
        // Primary user-initiated actions
        TxType::Sale { .. } => true,
        TxType::Mint { .. } => true,
        TxType::OfferAccept { .. } => true,
        TxType::OfferUpdate { .. } => true,
        TxType::OfferCancel { .. } => true,
        TxType::CreateOffer { .. } => true,
        TxType::AssetTransfer { .. } => true,
        TxType::Transfer { .. } => true,
        TxType::Burn { .. } => true,

        // Secondary actions/side effects - filter these out
        TxType::SmartContract { .. } => false, // Technical execution side effect

        // Other types default to primary for now
        _ => true,
    }
}

/// Generate concise insight points rather than verbose descriptions
fn generate_insight_summary(tx_types: &[TxType], tags: &[TxTag]) -> String {
    let mut insights = Vec::new();

    // Filter to only primary actions, excluding secondary side effects
    let primary_tx_types: Vec<&TxType> = tx_types
        .iter()
        .filter(|tx_type| is_primary_action(tx_type))
        .collect();

    // Group primary transaction types for aggregation
    let mut offer_updates = Vec::new();
    let mut offer_cancels = Vec::new();
    let mut offer_accepts = Vec::new();
    let mut sales = Vec::new();
    let mut mints = Vec::new();
    let mut create_offers = Vec::new();
    let mut other_primary_types = Vec::new();

    for tx_type in primary_tx_types {
        match tx_type {
            TxType::OfferUpdate { .. } => offer_updates.push(tx_type),
            TxType::OfferCancel { .. } => offer_cancels.push(tx_type),
            TxType::OfferAccept { .. } => offer_accepts.push(tx_type),
            TxType::Sale { .. } => sales.push(tx_type),
            TxType::Mint { .. } => mints.push(tx_type),
            TxType::CreateOffer { .. } => create_offers.push(tx_type),
            _ => other_primary_types.push(tx_type),
        }
    }

    // Generate insights for offer updates
    if !offer_updates.is_empty() {
        if let Some(insight) = generate_offer_update_insight(&offer_updates) {
            insights.push(insight);
        }
    }

    // Generate insights for offer cancellations
    if !offer_cancels.is_empty() {
        if let Some(insight) = generate_offer_cancel_insight(&offer_cancels) {
            insights.push(insight);
        }
    }

    // Generate insights for offer accepts
    if !offer_accepts.is_empty() {
        if let Some(insight) = generate_offer_accept_insight(&offer_accepts) {
            insights.push(insight);
        }
    }

    // Generate insights for sales
    if !sales.is_empty() {
        if let Some(insight) = generate_sale_insight(&sales) {
            insights.push(insight);
        }
    }

    // Generate insights for mints
    if !mints.is_empty() {
        if let Some(insight) = generate_mint_insight(&mints) {
            insights.push(insight);
        }
    }

    // Generate insights for offer creation
    if !create_offers.is_empty() {
        if let Some(insight) = generate_create_offer_insight(&create_offers) {
            insights.push(insight);
        }
    }

    // Handle other primary transaction types with basic insights
    for tx_type in other_primary_types {
        insights.push(generate_basic_insight(tx_type));
    }

    // Add tag insights - prioritize Bundle over Sweep when both are present
    let has_bundle = tags.iter().any(|tag| matches!(tag, TxTag::Bundle { .. }));

    for tag in tags {
        // Skip Sweep tag if Bundle tag is present (Bundle behavior takes precedence)
        if matches!(tag, TxTag::Sweep { .. }) && has_bundle {
            continue;
        }

        if let Some(tag_insight) = generate_tag_insight(tag) {
            insights.push(tag_insight);
        }
    }

    if insights.is_empty() {
        "Unknown activity".to_string()
    } else {
        insights.join(" • ")
    }
}

fn generate_offer_update_insight(updates: &[&TxType]) -> Option<String> {
    if updates.is_empty() {
        return None;
    }

    // Group by policy ID to detect collection vs individual updates
    let mut policy_groups: std::collections::HashMap<String, Vec<&TxType>> =
        std::collections::HashMap::new();

    for update in updates {
        if let TxType::OfferUpdate { policy_id, .. } = update {
            policy_groups
                .entry(policy_id.clone())
                .or_default()
                .push(update);
        }
    }

    let mut policy_insights = Vec::new();

    for (policy_id, policy_updates) in policy_groups {
        let count = policy_updates.len();

        // Calculate average delta and collect bidder addresses for this policy
        let mut deltas = Vec::new();
        let mut bidders = Vec::new();

        for update in policy_updates {
            if let TxType::OfferUpdate {
                delta_amount,
                bidder,
                ..
            } = update
            {
                deltas.push(*delta_amount);
                bidders.push(bidder.clone());
            }
        }

        if !deltas.is_empty() {
            let avg_delta = deltas.iter().sum::<i64>() as f64 / deltas.len() as f64;
            let ada_delta = avg_delta / 1_000_000.0;

            // Identify primary actor by filtering out script addresses
            let bidder_refs: Vec<&str> = bidders.iter().map(|s| s.as_str()).collect();
            let primary_actor = identify_primary_actor(&bidder_refs);

            let collection_name = shorten_policy_id(&policy_id);
            let direction = if avg_delta > 0.0 {
                "increased"
            } else {
                "decreased"
            };
            let sign = if avg_delta > 0.0 { "+" } else { "" };

            let actor_part = match primary_actor {
                Some(actor) => format!(" by {}", shorten_address(&actor)),
                None => String::new(),
            };

            if count == 1 {
                policy_insights.push(format!(
                    "Offer {} ({sign}₳{:.1}) on {}{}",
                    direction,
                    ada_delta.abs(),
                    collection_name,
                    actor_part
                ));
            } else {
                policy_insights.push(format!(
                    "{}x offers {} ({sign}₳{:.1} each) on {}{}",
                    count,
                    direction,
                    ada_delta.abs(),
                    collection_name,
                    actor_part
                ));
            }
        }
    }

    if policy_insights.is_empty() {
        None
    } else {
        Some(policy_insights.join(" • "))
    }
}

fn generate_offer_cancel_insight(cancels: &[&TxType]) -> Option<String> {
    if cancels.is_empty() {
        return None;
    }

    // Group by policy ID and collect bidder addresses
    let mut policy_groups: std::collections::HashMap<String, (u32, u64, bool, Vec<String>)> =
        std::collections::HashMap::new();

    for cancel in cancels {
        if let TxType::OfferCancel {
            policy_id,
            offer_count,
            total_cancelled_lovelace,
            encoded_asset_name,
            bidder,
            ..
        } = cancel
        {
            let (existing_count, existing_total, _, mut bidders) = policy_groups
                .get(policy_id)
                .cloned()
                .unwrap_or((0, 0, false, Vec::new()));
            let has_asset = encoded_asset_name.is_some();
            bidders.push(bidder.clone());
            policy_groups.insert(
                policy_id.clone(),
                (
                    existing_count + offer_count,
                    existing_total + total_cancelled_lovelace,
                    has_asset,
                    bidders,
                ),
            );
        }
    }

    let mut policy_insights = Vec::new();

    for (policy_id, (total_count, total_lovelace, has_specific_asset, bidders)) in policy_groups {
        let collection_name = shorten_policy_id(&policy_id);
        let avg_ada = (total_lovelace as f64 / total_count as f64) / 1_000_000.0;

        // Identify primary actor by filtering out script addresses
        let bidder_refs: Vec<&str> = bidders.iter().map(|s| s.as_str()).collect();
        let primary_actor = identify_primary_actor(&bidder_refs);

        let asset_type = if has_specific_asset {
            "asset"
        } else {
            "collection"
        };

        let actor_part = match primary_actor {
            Some(actor) => format!(" by {}", shorten_address(&actor)),
            None => String::new(),
        };

        if total_count == 1 {
            policy_insights.push(format!(
                "{} offer cancelled (₳{:.1}) on {}{}",
                asset_type.to_title_case(),
                avg_ada,
                collection_name,
                actor_part
            ));
        } else {
            policy_insights.push(format!(
                "{total_count}x {asset_type} offers cancelled (₳{avg_ada:.1} each) on {collection_name}{actor_part}"
            ));
        }
    }

    if policy_insights.is_empty() {
        None
    } else {
        Some(policy_insights.join(" • "))
    }
}

fn generate_offer_accept_insight(accepts: &[&TxType]) -> Option<String> {
    if accepts.is_empty() {
        return None;
    }

    let count = accepts.len();
    let total_volume: u64 = accepts
        .iter()
        .filter_map(|accept| {
            if let TxType::OfferAccept { offer_lovelace, .. } = accept {
                Some(*offer_lovelace)
            } else {
                None
            }
        })
        .sum();

    let avg_ada = (total_volume as f64 / count as f64) / 1_000_000.0;

    if count == 1 {
        Some(format!("Offer accepted (₳{avg_ada:.1})"))
    } else {
        let total_ada = total_volume as f64 / 1_000_000.0;
        Some(format!("{count}x offers accepted (₳{total_ada:.1} total)"))
    }
}

fn generate_sale_insight(sales: &[&TxType]) -> Option<String> {
    if sales.is_empty() {
        return None;
    }

    let count = sales.len();
    let total_volume: u64 = sales
        .iter()
        .filter_map(|sale| {
            if let TxType::Sale { breakdown, .. } = sale {
                Some(breakdown.total_lovelace)
            } else {
                None
            }
        })
        .sum();

    let avg_ada = (total_volume as f64 / count as f64) / 1_000_000.0;

    if count == 1 {
        Some(format!("Sale (₳{avg_ada:.1})"))
    } else {
        let total_ada = total_volume as f64 / 1_000_000.0;
        Some(format!("{count}x sales (₳{total_ada:.1} total)"))
    }
}

fn generate_mint_insight(mints: &[&TxType]) -> Option<String> {
    if mints.is_empty() {
        return None;
    }

    let total_assets: usize = mints
        .iter()
        .map(|mint| {
            if let TxType::Mint { assets, .. } = mint {
                assets.len()
            } else {
                0
            }
        })
        .sum();

    if total_assets == 1 {
        Some("Asset minted".to_string())
    } else {
        Some(format!("{total_assets}x assets minted"))
    }
}

fn generate_create_offer_insight(creates: &[&TxType]) -> Option<String> {
    if creates.is_empty() {
        return None;
    }

    let total_offers: u32 = creates
        .iter()
        .filter_map(|create| {
            if let TxType::CreateOffer { offer_count, .. } = create {
                Some(*offer_count)
            } else {
                None
            }
        })
        .sum();

    if total_offers == 1 {
        Some("Offer created".to_string())
    } else {
        Some(format!("{total_offers}x offers created"))
    }
}

fn generate_basic_insight(tx_type: &TxType) -> String {
    match tx_type {
        TxType::Transfer { assets } => {
            let count = assets.len();
            if count == 1 {
                "Asset transferred".to_string()
            } else {
                format!("{count}x assets transferred")
            }
        }
        TxType::AssetTransfer { assets, .. } => {
            let count = assets.len();
            if count == 1 {
                "Asset transfer".to_string()
            } else {
                format!("{count}x asset transfers")
            }
        }
        TxType::Burn { asset_count, .. } => {
            if *asset_count == 1 {
                "Asset burned".to_string()
            } else {
                format!("{asset_count}x assets burned")
            }
        }
        _ => "Activity detected".to_string(),
    }
}

fn generate_tag_insight(tag: &TxTag) -> Option<String> {
    match tag {
        TxTag::Sweep { asset_count, .. } => Some(format!("Sweep ({asset_count}x assets)")),
        TxTag::Bundle {
            asset_count,
            total_lovelace,
            per_asset_lovelace,
        } => {
            let total_ada = *total_lovelace as f64 / 1_000_000.0;
            let per_asset_ada = *per_asset_lovelace as f64 / 1_000_000.0;
            Some(format!(
                "Bundle ({asset_count}x assets, ₳{total_ada:.2} total, ₳{per_asset_ada:.2} each)"
            ))
        }
        TxTag::HighValue { total_lovelace } => {
            let ada_value = *total_lovelace as f64 / 1_000_000.0;
            Some(format!("High-value (₳{ada_value:.0})"))
        }
        TxTag::BundleDiscount {
            discount_percent, ..
        } => Some(format!("Bundle discount ({discount_percent}%)")),
        _ => None,
    }
}

/// Return full policy ID without shortening
fn shorten_policy_id(policy_id: &str) -> String {
    // Return the full policy ID without truncation
    policy_id.to_string()
}

/// Return full address without shortening
fn shorten_address(address: &str) -> String {
    // Return the full address without truncation
    address.to_string()
}

// Helper trait for string title case
trait ToTitleCase {
    fn to_title_case(&self) -> String;
}

impl ToTitleCase for str {
    fn to_title_case(&self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            }
        }
    }
}
