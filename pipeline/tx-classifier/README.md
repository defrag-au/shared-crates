# Transaction Classifier

A Rust crate for classifying Cardano blockchain transactions by their purpose and marketplace interactions.

## Overview

This transaction classifier can analyze Cardano transaction hashes and determine:
- **Transaction Type**: mint, sale, listing, transfer, burn, etc.
- **Marketplace**: jpg.store, SpaceBudz, CNFT.io, Wayup, etc.
- **Asset Operations**: which assets were minted, burned, or transferred
- **ADA Flows**: payment amounts and directions
- **Confidence Level**: how certain the classification is

## Features

- 🔍 **Multi-indexer Support**: Works with Maestro, Kupo, and Blockfrost APIs
- 🏪 **Marketplace Detection**: Identifies transactions from known NFT marketplaces
- 📊 **Pattern Recognition**: Uses sophisticated patterns to classify transaction types
- 🎯 **High Accuracy**: Confidence scoring and validation for reliable results
- ⚡ **Async/Await**: Built for modern async Rust applications
- 🧪 **Extensible**: Easy to add new marketplaces and transaction patterns

## Usage

```rust
use tx_classifier::{TxClassifier, IndexerPool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create classifier from environment (requires API keys)
    let indexer_pool = IndexerPool::from_env(&env)?;
    let classifier = TxClassifier::new(indexer_pool);

    // Classify a transaction
    let tx_hash = "fdba011794eef2717512a534e3b751b4b44fdbd11e7b8b3e9875ebb9a86b65e6";
    let classification = classifier.classify_transaction(tx_hash).await?;

    println!("Transaction Type: {:?}", classification.primary_type);
    println!("Marketplace: {:?}", classification.marketplace);
    println!("Confidence: {:?}", classification.confidence);
    
    Ok(())
}
```

## Transaction Types

The classifier can detect these transaction types:

- **Mint**: New asset creation
- **Burn**: Asset destruction
- **Sale**: Marketplace or P2P asset sales
- **Listing**: Assets listed for sale
- **Transfer**: Simple asset transfers
- **FrankenAddress**: Wayup franken address interactions
- **DeFi**: DeFi protocol interactions
- **SmartContract**: Generic smart contract calls

## Supported Marketplaces

- **jpg.store**: Leading Cardano NFT marketplace (patterns implemented)
- **SpaceBudz**: Popular NFT collection marketplace (patterns implemented)
- **CNFT.io**: Community-driven marketplace (patterns implemented)
- **Wayup**: Franken address trading (⚠️ **needs real transaction examples**)
- **TokiaNft**: Additional marketplace support

### ⚠️ Wayup Detection Status

Currently, Wayup detection is **incomplete** and needs real transaction examples to implement proper patterns. The current implementation:

- ✅ Looks for Wayup-specific metadata keys
- ❌ Does NOT assume long addresses = Wayup (removed this assumption)
- 🔄 **Needs**: Actual Wayup script addresses and transaction hashes

**To improve Wayup detection, please provide:**
- Real Wayup transaction hashes
- Known Wayup script addresses
- Wayup metadata patterns from actual transactions

## Environment Setup

Set these environment variables to enable transaction fetching:

```bash
# Required for Maestro (recommended)
MAESTRO_API_KEY=your_maestro_api_key

# Optional for additional indexers
KUPO_URL=http://your-kupo-instance:1442
BLOCKFROST_API_KEY=your_blockfrost_project_id
```

## Example Output

```rust
TxClassification {
    tx_hash: "fdba011794eef2717512a534e3b751b4b44fdbd11e7b8b3e9875ebb9a86b65e6",
    primary_type: Mint {
        policy_id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6",
        asset_count: 1
    },
    marketplace: Some(JpgStore),
    confidence: High,
    score: 0.87,
    assets: [
        AssetOperation {
            asset_id: "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f650697261746531",
            operation: Mint,
            quantity: 1,
            from_address: None,
            to_address: Some("addr1q9...")
        }
    ],
    ada_amounts: AdaFlows {
        total_input: 15_000_000,
        total_output: 14_500_000,
        fee: 500_000,
        largest_transfer: Some(10_000_000)
    }
}
```

## Running Examples

```bash
# Run the classifier example (will show setup instructions)
cargo run --example classify_transaction

# With proper API setup
MAESTRO_API_KEY=your_key cargo run --example classify_transaction
```

## Architecture

The classifier uses a layered architecture:

1. **Indexer Pool**: Manages multiple blockchain indexers for data redundancy
2. **Pattern Engine**: Applies sophisticated patterns to detect transaction types  
3. **Marketplace Detection**: Identifies known marketplace signatures
4. **Rule Engine**: Combines patterns and marketplace data for final classification
5. **Confidence Scoring**: Provides reliability metrics for classifications

## Monetary Field Naming Convention

**Important**: All monetary values in this crate use the `_lovelace` suffix to clearly indicate they are denominated in lovelace (the smallest unit of ADA), not ADA directly.

### Examples:
- `offer_lovelace: 25000000` = 25 ADA
- `total_lovelace: 175000000` = 175 ADA  
- `price_lovelace: 50000000` = 50 ADA

### Affected Fields:
- **CreateOffer**: `offer_lovelace`, `total_lovelace`
- **OfferAccept**: `offer_lovelace` 
- **Offer**: `offer_lovelace`
- **Sale**: `total_lovelace` (in SaleBreakdown)
- **MarketplaceBundle**: `total_lovelace`
- **Listing**: `price_lovelace`
- **Mint**: `total_lovelace`
- **TxTag variants**: `total_lovelace`, `avg_ppa_lovelace`, `original_lovelace`, `discounted_lovelace`

### Conversion:
To convert lovelace to ADA for display:
```rust
let ada_amount = lovelace_value as f64 / 1_000_000.0;
println!("Price: ₳{:.2}", ada_amount);
```

This naming convention prevents confusion between ADA and lovelace units and makes the codebase self-documenting.

## Contributing

To add support for new marketplaces or transaction patterns:

1. Add marketplace patterns to `src/marketplace.rs`
2. Add transaction patterns to `src/patterns.rs`
3. Update rule combinations in `src/rules.rs`
4. Add comprehensive tests

## License

This crate is part of the larger CNFT.dev ecosystem and follows the same licensing terms.