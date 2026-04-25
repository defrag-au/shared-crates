use crate::{
    patterns::PatternContext, AdaFlows, AssetId, AssetOpType, AssetOperation, Confidence, MintType,
    RawTxData, RawTxDataExt, TxClassification, TxContext, TxTag, TxType,
};
use address_registry::AddressLookup;
use cardano_assets::NftPurpose;
use tracing::{debug, info};

// Import marketplace classification functions from patterns module
use crate::patterns::marketplace_classification::*;

/// Enrichment tasks that require controlled access to raw transaction data
/// These tasks are explicit about what raw data they need and why
#[derive(Debug, Clone)]
pub enum EnrichmentTask {
    /// Extract policy ID from offer contract datums
    ExtractPolicyIdFromDatums {
        offer_script_address: String,
        output_indices: Vec<u32>, // Which outputs to check for datums
    },
    /// Extract mint cost from UTXO flows (for future use)
    ExtractMintCostFromUtxoFlows { minter_address: String },
    /// Extract JPG.store redeemer operations from spend redeemers
    ExtractJpgStoreRedeemerOperations,
}

/// JPG.store redeemer operation types extracted from constructor values
#[derive(Debug, Clone, PartialEq)]
pub enum JpgStoreOperation {
    /// Constructor 0 - Purchase operation
    Buy,
    /// Constructor 1 - Portfolio management (unlisting, relisting)
    WithdrawOrUpdate,
    /// Both Buy and WithdrawOrUpdate in same transaction
    Compound,
    /// No JPG.store operations detected
    None,
}

/// Result from a classification rule - includes pattern match and enrichment needs
#[derive(Debug)]
pub struct RuleResult {
    pub tx_type: TxType,
    pub confidence: f64,
    pub enrichment_tasks: Vec<EnrichmentTask>,
    /// Store enriched data that gets populated during enrichment processing
    pub jpg_store_operation: Option<JpgStoreOperation>,
}

/// Enrichment processor - handles controlled raw data access
pub struct EnrichmentProcessor;

impl Default for EnrichmentProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl EnrichmentProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Process enrichment tasks to enhance transaction type information
    /// This is the ONLY place that should access raw transaction data for enrichment
    pub fn process_tasks(
        &self,
        mut tx_type: TxType,
        tasks: Vec<EnrichmentTask>,
        raw_tx_data: &RawTxData,
    ) -> (TxType, Option<JpgStoreOperation>) {
        let mut jpg_store_operation: Option<JpgStoreOperation> = None;

        for task in tasks {
            match task {
                EnrichmentTask::ExtractPolicyIdFromDatums {
                    offer_script_address,
                    output_indices,
                } => {
                    if let TxType::CreateOffer {
                        ref mut policy_id, ..
                    } = tx_type
                    {
                        *policy_id = self.extract_policy_from_offer_datums(
                            raw_tx_data,
                            &offer_script_address,
                            &output_indices,
                        );
                    }
                }
                EnrichmentTask::ExtractMintCostFromUtxoFlows { .. } => {
                    // Future implementation for mint cost extraction
                    debug!("Mint cost enrichment not yet implemented");
                }
                EnrichmentTask::ExtractJpgStoreRedeemerOperations => {
                    jpg_store_operation = Some(self.extract_jpg_store_operations(raw_tx_data));
                }
            }
        }
        (tx_type, jpg_store_operation)
    }

    /// Extract policy ID from offer contract datums - delegates to marketplace-specific implementation
    /// CONTROLLED raw data access - only accesses specific outputs for datum analysis
    fn extract_policy_from_offer_datums(
        &self,
        raw_tx_data: &RawTxData,
        script_address: &str,
        output_indices: &[u32],
    ) -> String {
        // Determine which marketplace this script address belongs to
        if let Some(marketplace) = address_registry::Marketplace::from_address(script_address) {
            debug!(
                "Enrichment: Delegating to marketplace-specific extraction: {:?}",
                marketplace
            );
            extract_offer_policy_id(marketplace, raw_tx_data, script_address, output_indices)
        } else {
            debug!(
                "Enrichment: Unknown marketplace for address {}, returning unknown policy",
                script_address
            );
            "unknown".to_string()
        }
    }

    /// Extract JPG.store operations from spend redeemers
    /// CONTROLLED raw data access - only accesses redeemer JSON for constructor analysis
    fn extract_jpg_store_operations(&self, raw_tx_data: &RawTxData) -> JpgStoreOperation {
        debug!("Enrichment: Extracting JPG.store redeemer operations");

        let redeemer_data = match raw_tx_data.redeemers.as_ref() {
            Some(data) => data,
            None => {
                debug!("Enrichment: No redeemer data found");
                return JpgStoreOperation::None;
            }
        };

        let mut has_buy = false;
        let mut has_withdraw_or_update = false;

        // Parse spend redeemers for JPG.store constructor patterns
        if let Some(spends) = redeemer_data.get("spends").and_then(|s| s.as_array()) {
            for spend in spends {
                if let Some(constructor) = spend
                    .get("data")
                    .and_then(|data| data.get("json"))
                    .and_then(|json| json.get("constructor"))
                    .and_then(|c| c.as_u64())
                {
                    match constructor {
                        0 => {
                            debug!("Enrichment: Found Buy operation (constructor 0)");
                            has_buy = true;
                        }
                        1 => {
                            debug!("Enrichment: Found WithdrawOrUpdate operation (constructor 1)");
                            has_withdraw_or_update = true;
                        }
                        _ => continue,
                    }
                }
            }
        }

        // Determine operation type based on what was found
        let operation = match (has_buy, has_withdraw_or_update) {
            (true, true) => {
                debug!("Enrichment: Detected compound JPG.store operation");
                JpgStoreOperation::Compound
            }
            (true, false) => {
                debug!("Enrichment: Detected JPG.store buy operation");
                JpgStoreOperation::Buy
            }
            (false, true) => {
                debug!("Enrichment: Detected JPG.store withdraw/update operation");
                JpgStoreOperation::WithdrawOrUpdate
            }
            (false, false) => {
                debug!("Enrichment: No JPG.store operations detected");
                JpgStoreOperation::None
            }
        };

        operation
    }
}

/// New classification rule trait - uses asset operations and returns enrichment tasks
/// This replaces the old ClassificationRule trait that used raw transaction data
pub trait AssetBasedClassificationRule {
    fn name(&self) -> &str;
    fn apply(&self, operations: &[AssetOperation], context: &PatternContext) -> Option<RuleResult>;
    fn priority(&self) -> u8; // Higher number = higher priority
}

/// Legacy classification rule trait - DEPRECATED, use AssetBasedClassificationRule instead
/// This trait exists for backwards compatibility during migration
#[warn(deprecated)]
pub trait ClassificationRule {
    fn name(&self) -> &str;
    fn apply(&self, tx_data: &RawTxData) -> Option<(TxType, f64)>;
    fn priority(&self) -> u8; // Higher number = higher priority
}

/// Rule engine for transaction classification
pub struct RuleEngine {
    enable_marketplace_detection: bool,
    enable_pattern_detection: bool,
    confidence_threshold: f64,
    contract_registry: Box<dyn AddressLookup>,
}

impl RuleEngine {
    /// Create a new rule engine with the given address registry.
    pub fn new(registry: Box<dyn AddressLookup>) -> Self {
        Self {
            enable_marketplace_detection: true,
            enable_pattern_detection: true,
            confidence_threshold: 0.4,
            contract_registry: registry,
        }
    }

    /// Set minimum confidence threshold for classifications
    pub fn with_confidence_threshold(mut self, threshold: f64) -> Self {
        self.confidence_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Enable or disable marketplace detection
    pub fn with_marketplace_detection(mut self, enabled: bool) -> Self {
        self.enable_marketplace_detection = enabled;
        self
    }

    /// Enable or disable pattern detection
    pub fn with_pattern_detection(mut self, enabled: bool) -> Self {
        self.enable_pattern_detection = enabled;
        self
    }

    /// Classify a transaction using all available rules
    pub fn classify(&self, tx_data: &RawTxData) -> TxClassification {
        info!(
            "Starting classification for transaction: {}",
            tx_data.tx_hash
        );

        // Extract basic transaction info
        let context = self.build_context(tx_data);

        // UTXO Analysis: Classify asset operations while preserving positions
        let utxo_analysis = crate::utxo_analysis::analyze_utxo_operations(tx_data);

        // Extract only genuine operations for ADA flow calculation (excludes housekeeping)
        let genuine_operations: Vec<AssetOperation> = utxo_analysis
            .all_operations
            .iter()
            .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
            .cloned()
            .collect();

        let filtered_ada_flows = tx_data.calculate_filtered_ada_flows(&genuine_operations);

        info!(
            "UTXO Analysis: {} genuine movements, {} housekeeping operations",
            utxo_analysis.summary.genuine_count, utxo_analysis.summary.housekeeping_count
        );

        // Collect all addresses involved in the transaction
        let addresses = self.collect_addresses(tx_data);

        // Run marketplace detection using registry for informational purposes
        if self.enable_marketplace_detection {
            let input_addresses: Vec<String> =
                tx_data.inputs.iter().map(|i| i.address.clone()).collect();
            let output_addresses: Vec<String> =
                tx_data.outputs.iter().map(|o| o.address.clone()).collect();

            let detected_marketplaces = self
                .contract_registry
                .get_transaction_marketplaces(&input_addresses, &output_addresses);

            if !detected_marketplaces.is_empty() {
                debug!(
                    "Detected marketplaces from registry: {:?}",
                    detected_marketplaces
                );
            }
        }

        // Run pattern detection
        let mut transaction_types = Vec::new();
        let distributions = Vec::new();

        if self.enable_pattern_detection {
            #[allow(deprecated)]
            let pattern_context = PatternContext {
                asset_operations: &utxo_analysis.all_operations, // Use all operations with classifications preserved
                metadata: &tx_data.metadata,
                scripts: &tx_data.scripts,
                addresses,
                raw_tx_data: tx_data,
            };

            let pattern_results = crate::patterns::detect_patterns(&pattern_context);

            for (tx_type, confidence) in pattern_results.transactions {
                if confidence >= self.confidence_threshold {
                    debug!(
                        "Detected pattern: {:?} (confidence: {:.2})",
                        tx_type, confidence
                    );
                    transaction_types.push((tx_type, confidence));
                }
            }

            // Note: distributions not yet supported in new pattern system
            // distributions = pattern_results.distributions;
        }

        // Collect and deduplicate transaction types (no ranking)
        let (tx_types, final_confidence) = self.collect_transaction_types(transaction_types);

        // Calculate final score and confidence
        let genuine_operations_vec: Vec<AssetOperation> = utxo_analysis
            .all_operations
            .iter()
            .filter(|op| matches!(op.classification, crate::OperationClassification::Genuine))
            .cloned()
            .collect();

        let score = self.calculate_final_score(
            final_confidence,
            &genuine_operations_vec, // Use genuine operations for score calculation
            &filtered_ada_flows,     // Use filtered ADA flows for score calculation
        );

        let confidence = Confidence::from_score(score);

        // Build classification notes
        let mut notes = Vec::new();
        if !tx_data.scripts.is_empty() {
            notes.push(format!(
                "Smart contract interaction detected ({} scripts)",
                tx_data.scripts.len()
            ));
        }
        if filtered_ada_flows.largest_transfer.is_some() {
            notes.push("Significant ADA transfer detected".to_string());
        }

        // Add CIP-68 technical details to notes
        for tx_type in &tx_types {
            if let TxType::Mint {
                mint_type: MintType::Cip68,
                assets,
                reference_assets,
                ..
            } = tx_type
            {
                let nft_count = assets.len(); // UserNfts represent the logical NFT count
                let total_tokens = assets.len() + reference_assets.len(); // Total on-chain tokens
                if total_tokens > 0 {
                    notes.push(format!(
                        "CIP-68 implementation: {} on-chain tokens ({} reference + {} user) represent {} logical NFT{}",
                        total_tokens,
                        reference_assets.len(),
                        assets.len(),
                        nft_count,
                        if nft_count == 1 { "" } else { "s" }
                    ));
                }
                break; // Only add the note once even if there are multiple CIP-68 mints
            }
        }

        let mut final_context = context;
        notes.sort(); // Sort notes for deterministic order
        final_context.notes = notes;

        // Use all operations with classifications preserved for datum access
        let sorted_assets = utxo_analysis.all_operations;

        let mut classification = TxClassification {
            tx_hash: tx_data.tx_hash.clone(),
            tx_types,
            tags: Vec::new(), // Initialize empty tags
            confidence,
            score,
            context: final_context,
            assets: sorted_assets, // Use sorted filtered operations in final result
            ada_amounts: filtered_ada_flows, // Use filtered ADA flows in final result
            distributions,         // Use distributions from pattern detection
        };

        // Post-process to add classification tags
        self.add_classification_tags(&mut classification);

        // Extract payment distributions from marketplace datums
        self.extract_payment_distributions(&mut classification);

        info!(
            "Classification complete: {} (confidence: {:?}, score: {:.2})",
            classification.summary(),
            classification.confidence,
            classification.score
        );

        classification
    }

    /// Build transaction context from raw data
    fn build_context(&self, tx_data: &RawTxData) -> TxContext {
        let mut scripts = tx_data.scripts.clone();
        scripts.sort(); // Sort scripts for deterministic order

        TxContext {
            block_height: tx_data.block_height,
            timestamp: tx_data.timestamp,
            fee: tx_data.fee,
            size: tx_data.size,
            metadata: tx_data.metadata.clone(),
            scripts,
            notes: Vec::new(), // Will be populated later
        }
    }

    /// Collect all unique addresses from transaction inputs and outputs
    fn collect_addresses(&self, tx_data: &RawTxData) -> Vec<String> {
        let mut addresses = std::collections::HashSet::new();

        // Collect input addresses
        for input in &tx_data.inputs {
            addresses.insert(input.address.clone());
        }

        // Collect output addresses
        for output in &tx_data.outputs {
            addresses.insert(output.address.clone());
        }

        let mut addr_vec: Vec<String> = addresses.into_iter().collect();
        addr_vec.sort();
        addr_vec
    }

    /// Collect and deduplicate detected transaction types (no ranking)
    fn collect_transaction_types(&self, mut types: Vec<(TxType, f64)>) -> (Vec<TxType>, f64) {
        if types.is_empty() {
            return (vec![TxType::Unknown], 0.0);
        }

        // Sort deterministically: first by confidence (descending), then by transaction type (ascending)
        types.sort_by(|a, b| {
            match b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal) {
                std::cmp::Ordering::Equal => a.0.cmp(&b.0), // If confidence is equal, sort by TxType
                other => other, // Otherwise sort by confidence (descending)
            }
        });

        // Smart deduplication: allow multiple instances of same type if they have different details
        let mut deduplicated_types = Vec::new();
        let mut seen_keys = std::collections::HashSet::new();

        for (tx_type, confidence) in types {
            let dedup_key = self.get_deduplication_key(&tx_type);

            if !seen_keys.contains(&dedup_key) {
                deduplicated_types.push((tx_type, confidence));
                seen_keys.insert(dedup_key);
            }
        }

        // Calculate average confidence instead of just taking the first
        let avg_confidence = if deduplicated_types.is_empty() {
            0.0
        } else {
            deduplicated_types.iter().map(|(_, c)| c).sum::<f64>() / deduplicated_types.len() as f64
        };

        let mut tx_types: Vec<TxType> = deduplicated_types.into_iter().map(|(t, _)| t).collect();

        // Final sort to ensure deterministic order
        tx_types.sort();

        (tx_types, avg_confidence)
    }

    /// Generate a deduplication key that allows multiple instances of types with different details
    fn get_deduplication_key(&self, tx_type: &TxType) -> String {
        match tx_type {
            // For Sales, include asset ID to allow multiple sales of different assets
            TxType::Sale { asset, .. } => {
                format!(
                    "Sale:{}:{}",
                    asset.asset.policy_id, asset.asset.asset_name_hex
                )
            }
            // For OfferAccept, include asset ID to allow multiple offer accepts of different assets
            TxType::OfferAccept { asset, .. } => {
                format!("OfferAccept:{}:{}", asset.policy_id, asset.asset_name_hex)
            }
            // For other types, use discriminant-based deduplication
            TxType::Mint { assets, .. } => {
                // For Mint, allow multiple mints with different assets by including first policy
                if let Some(first_asset) = assets.first() {
                    format!(
                        "Mint:{}:{}",
                        first_asset.policy_id, first_asset.asset_name_hex
                    )
                } else {
                    "Mint:unknown".to_string()
                }
            }
            TxType::Burn { policy_id, .. } => {
                format!("Burn:{policy_id}")
            }
            TxType::MarketplaceBundle {
                primary_operation, ..
            } => {
                format!("MarketplaceBundle:{primary_operation}")
            }
            TxType::CreateOffer { policy_id, .. } => {
                format!("CreateOffer:{policy_id}")
            }
            TxType::OfferUpdate {
                policy_id,
                encoded_asset_name,
                original_amount,
                updated_amount,
                bidder,
                marketplace,
                offer_index,
                ..
            } => {
                // Allow multiple offer updates by including unique identifiers
                // Include amounts, bidder, marketplace, and offer_index for uniqueness
                let asset_part = encoded_asset_name
                    .as_ref()
                    .map(|name| format!(":{name}"))
                    .unwrap_or_default();
                format!("OfferUpdate:{policy_id}{asset_part}:{original_amount}:{updated_amount}:{bidder}:{marketplace}:{offer_index}")
            }
            TxType::OfferCancel {
                policy_id,
                encoded_asset_name,
                offer_count,
                bidder,
                marketplace,
                ..
            } => {
                // Allow multiple offer cancellations by including unique identifiers
                let asset_part = encoded_asset_name
                    .as_ref()
                    .map(|name| format!(":{name}"))
                    .unwrap_or_default();
                format!("OfferCancel:{policy_id}{asset_part}:{offer_count}:{bidder}:{marketplace}")
            }
            // For ListingUpdate, include asset ID to allow multiple listing updates of different assets
            TxType::ListingUpdate { assets, .. } => {
                if let Some(first_asset) = assets.first() {
                    format!(
                        "ListingUpdate:{}:{}",
                        first_asset.asset.policy_id, first_asset.asset.asset_name_hex
                    )
                } else {
                    "ListingUpdate:unknown".to_string()
                }
            }
            // For simple types, use discriminant
            _ => format!("{:?}", std::mem::discriminant(tx_type)),
        }
    }

    /// Calculate final classification score
    fn calculate_final_score(
        &self,
        pattern_confidence: f64,
        asset_operations: &[AssetOperation],
        ada_flows: &AdaFlows,
    ) -> f64 {
        let mut score = pattern_confidence;

        // Boost score for clear asset operations
        if !asset_operations.is_empty() {
            score += 0.05;
        }

        // Boost score for significant ADA flows
        if let Some(largest_transfer) = ada_flows.largest_transfer {
            if largest_transfer > 1_000_000 {
                // > 1 ADA
                score += 0.05;
            }
            if largest_transfer > 10_000_000 {
                // > 10 ADA
                score += 0.05;
            }
        }

        // Apply penalties for unclear transactions
        if ada_flows.flows.is_empty() {
            score -= 0.05; // No clear ADA flows
        }

        if asset_operations.is_empty() && ada_flows.largest_transfer.unwrap_or(0) < 1_000_000 {
            score -= 0.1; // No assets and minimal ADA
        }

        // Ensure score is within bounds
        score.clamp(0.0, 1.0)
    }

    /// Add classification tags based on detected patterns
    fn add_classification_tags(&self, classification: &mut TxClassification) {
        self.detect_sweep_tag(classification);
        self.detect_bundle_tag(classification);
        self.detect_high_value_tag(classification);
        self.detect_smart_contract_tags(classification);
    }

    /// Detect if this is a sweep transaction and add appropriate tag
    fn detect_sweep_tag(&self, classification: &mut TxClassification) {
        // Count sales by buyer to detect sweep patterns
        let mut buyer_sales: std::collections::HashMap<String, Vec<&TxType>> =
            std::collections::HashMap::new();

        for tx_type in &classification.tx_types {
            if let TxType::Sale { buyer, .. } = tx_type {
                buyer_sales.entry(buyer.clone()).or_default().push(tx_type);
            }
        }

        // Look for buyers with multiple purchases (sweep pattern)
        for (_buyer, sales) in buyer_sales {
            if sales.len() >= 2 {
                // Calculate sweep metrics
                let total_value: u64 = sales
                    .iter()
                    .filter_map(|tx_type| {
                        if let TxType::Sale { breakdown, .. } = tx_type {
                            Some(breakdown.total_lovelace)
                        } else {
                            None
                        }
                    })
                    .sum();

                let avg_price = if !sales.is_empty() {
                    total_value / sales.len() as u64
                } else {
                    0
                };

                classification.tags.push(TxTag::Sweep {
                    asset_count: sales.len() as u32,
                    total_lovelace: total_value,
                    avg_ppa_lovelace: avg_price,
                });
                break; // Only add one sweep tag even if multiple buyers sweep
            }
        }
    }

    /// Detect if this is a bundle sale and add appropriate tag
    fn detect_bundle_tag(&self, classification: &mut TxClassification) {
        // Group sales by seller to detect bundle patterns
        let mut seller_sales: std::collections::HashMap<String, Vec<&TxType>> =
            std::collections::HashMap::new();

        for tx_type in &classification.tx_types {
            if let TxType::Sale { seller, .. } = tx_type {
                seller_sales
                    .entry(seller.clone())
                    .or_default()
                    .push(tx_type);
            }
        }

        // Look for sellers with multiple sales (bundle pattern)
        for (_seller, sales) in seller_sales {
            if sales.len() >= 2 {
                // Calculate bundle metrics
                let total_value: u64 = sales
                    .iter()
                    .filter_map(|tx_type| {
                        if let TxType::Sale { breakdown, .. } = tx_type {
                            Some(breakdown.total_lovelace)
                        } else {
                            None
                        }
                    })
                    .sum();

                let per_asset_price = if !sales.is_empty() {
                    sales
                        .first()
                        .and_then(|tx_type| {
                            if let TxType::Sale { breakdown, .. } = tx_type {
                                Some(breakdown.total_lovelace)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0)
                } else {
                    0
                };

                classification.tags.push(TxTag::Bundle {
                    asset_count: sales.len() as u32,
                    total_lovelace: total_value,
                    per_asset_lovelace: per_asset_price,
                });
                break; // Only add one bundle tag even if multiple sellers have bundles
            }
        }
    }

    /// Detect high-value transactions
    fn detect_high_value_tag(&self, classification: &mut TxClassification) {
        let high_value_threshold = 100_000_000; // 100 ADA threshold

        // Check total sale volume
        let total_sales_volume = classification.total_sale_volume();
        if total_sales_volume >= high_value_threshold {
            classification.tags.push(TxTag::HighValue {
                total_lovelace: total_sales_volume,
            });
        }
    }

    /// Detect smart contract interactions and add tags with purpose if known
    fn detect_smart_contract_tags(&self, classification: &mut TxClassification) {
        // Look through transaction context for script addresses
        for script_address in &classification.context.scripts {
            if let Some(contract_info) = self.contract_registry.get_contract_info(script_address) {
                classification.tags.push(TxTag::SmartContractInteraction {
                    contract_purpose: Some(format!("{:?}", contract_info.category)),
                    contract_address: script_address.clone(),
                });
            }
        }

        // Also check for smart contract TxTypes and enhance them
        for tx_type in &classification.tx_types {
            if let TxType::SmartContract {
                contract_address, ..
            } = tx_type
            {
                // Only add tag if we don't already have one for this address
                let already_tagged = classification.tags.iter().any(|tag| {
                    matches!(tag, TxTag::SmartContractInteraction { contract_address: addr, .. } if addr == contract_address)
                });

                if !already_tagged {
                    let contract_purpose = self
                        .contract_registry
                        .get_contract_info(contract_address)
                        .map(|info| format!("{:?}", info.category));

                    classification.tags.push(TxTag::SmartContractInteraction {
                        contract_purpose,
                        contract_address: contract_address.clone(),
                    });
                }
            }
        }
    }

    /// Extract payment distributions from pattern matching
    /// Distributions are now detected during sale pattern matching for better accuracy
    fn extract_payment_distributions(&self, classification: &mut TxClassification) {
        // Only create fallback distributions if no distributions were found during pattern matching
        if !classification.distributions.is_empty() {
            return;
        }

        // Fallback: create basic distributions based on sale data if no pattern distributions found
        use crate::{Distribution, DistributionType};

        let sale_assets: Vec<String> = classification
            .get_by::<crate::Sale>()
            .into_iter()
            .filter_map(|sale| {
                if let crate::TxType::Sale { asset, .. } = sale {
                    Some(asset.asset.concatenated())
                } else {
                    None
                }
            })
            .collect();

        if sale_assets.is_empty() {
            return;
        }

        if let Some(crate::TxType::Sale {
            seller, breakdown, ..
        }) = classification.get_by::<crate::Sale>().first()
        {
            // Create a basic payment distribution based on sale data
            let total_sale_amount = breakdown.total_lovelace * sale_assets.len() as u64;
            classification.distributions.push(Distribution {
                to: seller.clone(),
                amount_lovelace: total_sale_amount,
                assets: Some(sale_assets),
                distribution_type: DistributionType::Payment,
            });
        }
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new(Box::new(address_registry::SmartContractRegistry::new()))
    }
}

/// Detect the type of mint based on the minted assets
fn detect_mint_type(mint_ops: &[&AssetOperation], tx_data: &RawTxData) -> MintType {
    // Check if this could be a CIP-68 mint by grouping tokens by base name and checking for reference/user pairs
    if mint_ops.len() >= 2 && mint_ops.len().is_multiple_of(2) {
        // Group tokens by their base names (after CIP-68 prefix)
        let mut base_name_groups: std::collections::HashMap<
            String,
            Vec<(&AssetOperation, NftPurpose)>,
        > = std::collections::HashMap::new();

        for mint_op in mint_ops {
            let asset_name = mint_op.asset_name().unwrap_or(&String::new()).clone();
            let purpose = NftPurpose::from(asset_name.as_str());

            // Only consider tokens with CIP-68 prefixes
            if matches!(purpose, NftPurpose::ReferenceNft | NftPurpose::UserNft)
                && asset_name.len() >= 8
            {
                let base_name = asset_name[8..].to_string();
                base_name_groups
                    .entry(base_name)
                    .or_default()
                    .push((mint_op, purpose));
            }
        }

        // Check if all tokens can be grouped into perfect reference/user pairs
        if !base_name_groups.is_empty() && base_name_groups.len() * 2 == mint_ops.len() {
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
                    .map(|(op, _)| op);

                let user_token = tokens
                    .iter()
                    .find(|(_, purpose)| matches!(purpose, NftPurpose::UserNft))
                    .map(|(op, _)| op);

                if let (Some(ref_op), Some(user_op)) = (reference_token, user_token) {
                    cip68_pairs.push((ref_op, user_op, base_name));
                }
            }

            if all_pairs_valid && !cip68_pairs.is_empty() {
                // Additional validations for CIP-68
                let no_metadata = tx_data.metadata.is_none();
                let mut valid_cip68_pairs = 0;

                for (ref_op, user_op, base_name) in &cip68_pairs {
                    let full_reference_asset_id = format!(
                        "{}{}",
                        ref_op.policy_id().unwrap_or(&String::new()),
                        ref_op.asset_name().unwrap_or(&String::new())
                    );

                    // Check if any output containing the reference NFT has a datum
                    let has_datum = tx_data.outputs.iter().any(|output| {
                        output
                            .assets
                            .iter()
                            .any(|(asset_id, _)| asset_id == &full_reference_asset_id)
                            && output.datum.is_some()
                    });

                    if has_datum && no_metadata {
                        valid_cip68_pairs += 1;
                        debug!(
                            "Detected CIP-68 pair: reference={}, user={}, base_name={}",
                            ref_op.asset_name().unwrap_or(&String::new()),
                            user_op.asset_name().unwrap_or(&String::new()),
                            base_name
                        );
                    }
                }

                // If all pairs are valid CIP-68 pairs, classify as CIP-68
                if valid_cip68_pairs == cip68_pairs.len() {
                    debug!(
                        "Detected CIP-68 multi-mint: {} NFTs ({} token pairs)",
                        cip68_pairs.len(),
                        mint_ops.len()
                    );
                    return MintType::Cip68;
                }
            }
        }
    }

    // Check for fungible tokens (quantity > 1)
    if mint_ops.iter().any(|op| op.amount() > 1) {
        return MintType::Fungible;
    }

    // Check for clear CIP-25 indicators (transaction metadata with label 721)
    if let Some(metadata) = &tx_data.metadata {
        use pipeline_types::cip::CIP25_METADATA_LABEL;

        if metadata.get(CIP25_METADATA_LABEL).is_some() {
            return MintType::Cip25;
        }
    }

    // Default assumption for NFTs with quantity 1 and metadata
    if mint_ops.iter().all(|op| op.amount() == 1) && tx_data.metadata.is_some() {
        return MintType::Cip25;
    }

    MintType::Unknown
}

/// Built-in rule implementations
pub struct MintRule;

impl ClassificationRule for MintRule {
    fn name(&self) -> &str {
        "mint_rule"
    }

    fn apply(&self, tx_data: &RawTxData) -> Option<(TxType, f64)> {
        let asset_ops = tx_data.extract_asset_operations();
        let mint_ops: Vec<_> = asset_ops
            .iter()
            .filter(|op| op.op_type == crate::AssetOpType::Mint)
            .collect();

        // Check both UTXO-derived mint operations AND direct mint field from CBOR
        if mint_ops.is_empty() && tx_data.mint.is_empty() {
            return None;
        }

        // If UTXO analysis didn't find mints but we have direct mint data, use that
        let mint_ops = if mint_ops.is_empty() && !tx_data.mint.is_empty() {
            // Convert direct mint operations to AssetOperation format for processing
            tx_data
                .mint
                .iter()
                .map(|mint_op| {
                    // Create a synthetic AssetOperation from MintOperation
                    // This is a temporary workaround until we implement full input/output parsing
                    crate::AssetOperation {
                        input: None,
                        output: Some(crate::TxUtxo {
                            address: "unknown_minter_address".to_string(), // Will be refined later
                            idx: 0,                                        // Synthetic index
                        }),
                        payload: crate::OperationPayload::NativeToken {
                            policy_id: mint_op.policy_id(),
                            encoded_name: mint_op.asset_name(),
                            amount: mint_op.amount.unsigned_abs(),
                        },
                        op_type: crate::AssetOpType::Mint,
                        input_datum: None,
                        output_datum: None,
                        classification: crate::OperationClassification::Genuine,
                    }
                })
                .collect::<Vec<_>>()
        } else {
            mint_ops.into_iter().cloned().collect()
        };

        // CRITICAL CHECK: Only skip if there are large ADA inputs AND no actual mint operations
        // If tx_data.mint is not empty, this is a genuine mint transaction regardless of ADA amounts
        if tx_data.mint.is_empty() {
            let potential_sale_inputs = tx_data.get_potential_sale_price_inputs();
            if !potential_sale_inputs.is_empty() {
                if let Some(input) = potential_sale_inputs
                    .iter()
                    .max_by_key(|input| input.amount_lovelace)
                {
                    if input.amount_lovelace >= 10_000_000 {
                        // >= 10 ADA with no mint field suggests sale, not mint
                        return None; // Don't classify as mint - let sale detection handle it
                    }
                }
            }
        }

        // Detect mint type first to properly separate assets
        let mint_ops_refs: Vec<&AssetOperation> = mint_ops.iter().collect();
        let mint_type = detect_mint_type(&mint_ops_refs, tx_data);

        // Separate minted assets based on mint type
        let (primary_assets, reference_assets, minter_address) =
            if matches!(mint_type, MintType::Cip68) {
                // For CIP-68, separate UserNfts (primary) from reference NFTs
                let mut user_nfts = Vec::new();
                let mut reference_nfts = Vec::new();

                for op in &mint_ops_refs {
                    let Some(asset) = crate::AssetId::new(
                        op.policy_id().unwrap_or(&String::new()).clone(),
                        op.asset_name().unwrap_or(&String::new()).clone(),
                    )
                    .ok() else {
                        continue;
                    };
                    let purpose =
                        NftPurpose::from(op.asset_name().unwrap_or(&String::new()).as_str());

                    match purpose {
                        NftPurpose::UserNft => user_nfts.push(asset),
                        NftPurpose::ReferenceNft => reference_nfts.push(asset),
                        _ => user_nfts.push(asset), // Fallback to primary assets
                    }
                }

                user_nfts.sort();
                reference_nfts.sort();

                // For CIP-68, minter is the address receiving UserNfts (000de140 prefixed tokens)
                let minter = tx_data
                    .outputs
                    .iter()
                    .find(|output| {
                        output.assets.iter().any(|(asset_id, _)| {
                            // Check if this output contains any UserNft (000de140 prefix)
                            asset_id.len() > 56 && &asset_id[56..64] == "000de140"
                        })
                    })
                    .map(|output| output.address.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                (user_nfts, reference_nfts, minter)
            } else {
                // For non-CIP-68 mints, all assets are primary assets
                let mut all_assets = Vec::new();
                for op in &mint_ops_refs {
                    let Some(asset) = crate::AssetId::new(
                        op.policy_id().unwrap_or(&String::new()).clone(),
                        op.asset_name().unwrap_or(&String::new()).clone(),
                    )
                    .ok() else {
                        continue;
                    };
                    all_assets.push(asset);
                }
                all_assets.sort();

                // Find the minter address - the address that receives the minted asset
                let minter = tx_data
                    .outputs
                    .iter()
                    .find(|output| {
                        // Find output that contains any minted asset
                        output.assets.iter().any(|(asset_id, _)| {
                            mint_ops_refs.iter().any(|mint_op| {
                                format!(
                                    "{}{}",
                                    mint_op.policy_id().unwrap_or(&String::new()),
                                    mint_op.asset_name().unwrap_or(&String::new())
                                ) == *asset_id
                            })
                        })
                    })
                    .map(|output| output.address.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                (all_assets, Vec::new(), minter)
            };

        // Calculate mint cost: total ADA inputs minus change received
        let total_ada_inputs: u64 = tx_data
            .inputs
            .iter()
            .map(|input| input.amount_lovelace)
            .sum();

        // Calculate actual change: ADA returned to input addresses
        let input_addresses: std::collections::HashSet<String> = tx_data
            .inputs
            .iter()
            .map(|input| input.address.clone())
            .collect();

        let total_change_returned: u64 = tx_data
            .outputs
            .iter()
            .filter(|output| input_addresses.contains(&output.address))
            .map(|output| output.amount_lovelace)
            .sum();

        // Also track what the minter specifically receives (for logging)
        let ada_received_by_minter: u64 = tx_data
            .outputs
            .iter()
            .filter(|output| output.address == minter_address)
            .map(|output| output.amount_lovelace)
            .sum();

        debug!(
            "Mint cost calculation: total_inputs={}μ₳ (₳{:.2}), actual_change={}μ₳ (₳{:.2}), minter='{}', ada_to_minter={}μ₳ (₳{:.2})",
            total_ada_inputs, total_ada_inputs as f64 / 1_000_000.0,
            total_change_returned, total_change_returned as f64 / 1_000_000.0,
            minter_address,
            ada_received_by_minter, ada_received_by_minter as f64 / 1_000_000.0
        );

        let total_mint_cost = if total_ada_inputs > total_change_returned {
            // Calculate mint cost as: total inputs - actual change returned to input addresses
            let cost = total_ada_inputs - total_change_returned;
            debug!(
                "Mint cost: {}μ₳ - {}μ₳ (actual change) = {}μ₳ (₳{:.2})",
                total_ada_inputs,
                total_change_returned,
                cost,
                cost as f64 / 1_000_000.0
            );
            Some(cost)
        } else {
            debug!("No mint cost identified (inputs <= change returned)");
            None // No clear cost identified
        };

        // Adjust confidence based on mint type detection
        let mut confidence = if mint_ops.len() == 1 { 0.9 } else { 0.8 };

        // Boost confidence for well-identified CIP-68 mints
        if matches!(mint_type, MintType::Cip68) {
            confidence = 0.95;
        }

        Some((
            TxType::Mint {
                assets: primary_assets,
                reference_assets,
                total_lovelace: total_mint_cost,
                minter: minter_address,
                mint_type,
            },
            confidence,
        ))
    }

    fn priority(&self) -> u8 {
        100 // High priority for mint detection
    }
}

pub struct BurnRule;

impl ClassificationRule for BurnRule {
    fn name(&self) -> &str {
        "burn_rule"
    }

    fn apply(&self, tx_data: &RawTxData) -> Option<(TxType, f64)> {
        let asset_ops = tx_data.extract_asset_operations();
        let burn_ops: Vec<_> = asset_ops
            .iter()
            .filter(|op| op.op_type == crate::AssetOpType::Burn)
            .collect();

        if burn_ops.is_empty() {
            return None;
        }

        let mut policy_counts = std::collections::HashMap::new();
        for op in &burn_ops {
            *policy_counts
                .entry(op.policy_id().unwrap_or(&String::new()).clone())
                .or_insert(0) += op.amount() as u32;
        }

        let (primary_policy, asset_count) =
            policy_counts.into_iter().max_by_key(|(_, count)| *count)?;

        Some((
            TxType::Burn {
                policy_id: primary_policy,
                asset_count,
            },
            0.95, // Very high confidence for burn detection
        ))
    }

    fn priority(&self) -> u8 {
        100 // High priority for burn detection
    }
}

pub struct CreateOfferRule;

impl ClassificationRule for CreateOfferRule {
    fn name(&self) -> &str {
        "create_offer_rule"
    }

    fn apply(&self, tx_data: &RawTxData) -> Option<(TxType, f64)> {
        // Look for multiple outputs to known offer contract addresses with similar amounts
        let mut offer_outputs = Vec::new();
        let mut marketplace_addresses = std::collections::HashSet::new();

        for output in &tx_data.outputs {
            // Check if this output goes to a known offer contract
            if crate::is_script_address(&output.address) {
                // Check if it's a JPG.store offer address
                if let Some(crate::registry::AddressCategory::Script(
                    crate::registry::ScriptCategory::Marketplace {
                        marketplace: crate::registry::Marketplace::JpgStore,
                        purpose: crate::registry::MarketplacePurpose::Offer,
                        ..
                    },
                )) = crate::registry::lookup_address(output.address.as_str())
                {
                    // This is an offer output - check if it has a datum (offer terms)
                    if output.datum.is_some() {
                        offer_outputs.push(output);
                        marketplace_addresses.insert(output.address.clone());
                    }
                }
            }
        }

        // We need at least 1 offer to consider this offer creation (single or bulk)
        if offer_outputs.is_empty() {
            return None;
        }

        // Check that all offers are for the same amount (only relevant for multiple offers)
        let first_amount = offer_outputs[0].amount_lovelace;
        if offer_outputs.len() > 1
            && !offer_outputs
                .iter()
                .all(|o| o.amount_lovelace == first_amount)
        {
            return None; // Mixed offer amounts, not uniform creation
        }

        // Try to extract policy ID from datum - this is JPG.store specific
        let mut extracted_policy = None;
        if let Some(output) = offer_outputs.first() {
            if let Some(datum_info) = &output.datum {
                if let Some(datum) = datum_info.json() {
                    // Look for 56-character hex strings in the datum that could be policy IDs
                    if let Some(datum_str) = datum.to_string().as_str().get(..) {
                        // Extract policy IDs from the datum hex (56-character hex strings)
                        let found_known_policy = false;

                        if !found_known_policy {
                            // Try to find any 56-character hex string (policy ID length)
                            if let Some(policy_id) = find_policy_id_in_string(datum_str) {
                                extracted_policy = Some(policy_id.to_string());
                            }
                        }
                    }
                } // Close the if let Some(datum) = &datum_info.json
            }
        }

        // Find the bidder (input address that provided the ADA)
        let bidder = tx_data
            .inputs
            .iter()
            .max_by_key(|input| input.amount_lovelace)
            .map(|input| input.address.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let offer_count = offer_outputs.len() as u32;
        let offer_amount = first_amount;
        let total_amount = offer_amount * offer_count as u64;
        let has_policy = extracted_policy.is_some();
        let policy_id = extracted_policy.unwrap_or_else(|| "unknown".to_string());
        let marketplace = marketplace_addresses
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        debug!(
            "CreateOffer detected: {} offers of {}μ₳ (₳{:.2}) each on policy {} by {}",
            offer_count,
            offer_amount,
            offer_amount as f64 / 1_000_000.0,
            &policy_id[..8.min(policy_id.len())],
            bidder
        );

        // High confidence if we extracted a policy ID and have uniform offers
        let confidence = if has_policy && offer_count >= 5 {
            0.95
        } else if has_policy && offer_count >= 2 {
            0.85
        } else if has_policy {
            0.80 // Single offer with extracted policy
        } else if offer_count >= 2 {
            0.70 // Multiple offers without policy
        } else {
            0.65 // Single offer without policy
        };

        Some((
            TxType::CreateOffer {
                policy_id,
                encoded_asset_name: None,
                offer_count,
                offer_lovelace: offer_amount,
                total_lovelace: total_amount,
                bidder,
                marketplace,
            },
            confidence,
        ))
    }

    fn priority(&self) -> u8 {
        90 // High priority for offer creation detection
    }
}

pub struct AssetTransferRule;

impl AssetTransferRule {
    /// Apply rule using pre-filtered operations and ADA flows from pattern context
    pub fn apply_with_filtered_ops(
        &self,
        genuine_operations: &[AssetOperation],
        _tx_data: &RawTxData,
        filtered_ada_flows: &crate::AdaFlows,
    ) -> Option<(TxType, f64)> {
        if genuine_operations.is_empty() {
            return None;
        }

        // Look for asset transfers (excluding mints/burns)
        let transfer_ops: Vec<_> = genuine_operations
            .iter()
            .filter(|op| matches!(op.op_type, AssetOpType::Transfer))
            .collect();

        if transfer_ops.is_empty() {
            return None;
        }

        // Group transfers by recipient to identify the primary transfer destination
        let mut recipient_assets: std::collections::HashMap<String, Vec<&AssetOperation>> =
            std::collections::HashMap::new();

        for op in &transfer_ops {
            if let Some(to_utxo) = &op.output {
                let to_addr = &to_utxo.address;
                recipient_assets
                    .entry(to_addr.clone())
                    .or_default()
                    .push(op);
            }
        }

        // Find the main recipient (one who receives the most assets)
        let main_recipient = recipient_assets
            .iter()
            .max_by_key(|(_, ops)| ops.len())
            .map(|(addr, _)| addr.clone());

        let receiver = main_recipient?;

        // Check that the receiving address is NOT a known script address
        // (to distinguish from listings/marketplace interactions)
        if crate::is_script_address(&receiver) {
            return None;
        }

        // Find the sender (most common from_address in genuine transfers)
        let sender_counts: std::collections::HashMap<String, usize> = transfer_ops
            .iter()
            .filter_map(|op| op.input.as_ref().map(|utxo| &utxo.address))
            .fold(std::collections::HashMap::new(), |mut acc, addr| {
                *acc.entry(addr.clone()).or_insert(0) += 1;
                acc
            });

        let sender = sender_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(addr, _)| addr)
            .unwrap_or_else(|| "unknown".to_string());

        // Ensure this is a simple transfer (not a sale) by checking ADA flows
        // Only network fees should be involved, no large ADA payments TO sender FROM other addresses
        // Flows from sender to sender are UTXO consolidation and should be ignored
        let large_ada_to_sender: Vec<_> = filtered_ada_flows
            .flows
            .iter()
            .filter(|flow| {
                flow.to_address == sender && flow.from_address != sender && flow.amount > 5_000_000
                // > 5 ADA threshold
            })
            .collect();

        if !large_ada_to_sender.is_empty() {
            // This looks like a sale/payment rather than a simple transfer
            return None;
        }

        // Collect the genuinely transferred assets to the main recipient (native tokens only)
        let transferred_assets: Vec<AssetId> = recipient_assets
            .get(&receiver)
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|op| {
                op.asset_id()
                    .and_then(|id| AssetId::parse_concatenated(&id).ok())
            })
            .collect();

        if transferred_assets.is_empty() {
            return None;
        }

        // High confidence for clean asset transfers
        let confidence = if sender != "unknown" && !transferred_assets.is_empty() {
            0.90
        } else {
            0.70
        };

        Some((
            TxType::AssetTransfer {
                assets: transferred_assets,
                sender,
                receiver,
            },
            confidence,
        ))
    }
}

impl ClassificationRule for AssetTransferRule {
    fn name(&self) -> &str {
        "asset_transfer_rule"
    }

    fn apply(&self, tx_data: &RawTxData) -> Option<(TxType, f64)> {
        // Fallback: Get filtered operations if called directly (not from pattern system)
        let (genuine_operations, _) = crate::utxo_analysis::create_filtered_context(tx_data);
        let filtered_ada_flows = tx_data.calculate_filtered_ada_flows(&genuine_operations);
        self.apply_with_filtered_ops(&genuine_operations, tx_data, &filtered_ada_flows)
    }

    fn priority(&self) -> u8 {
        85 // High priority, but slightly lower than marketplace operations
    }
}
