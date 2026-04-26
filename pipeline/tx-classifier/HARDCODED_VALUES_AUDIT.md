# Hardcoded Values Audit - TX Classifier

This document identifies hardcoded values that should be replaced with configurable or calculated values to improve maintainability and flexibility.

## Summary

**High Priority Issues:**
- Policy ID hardcoding for Ancestors and Nikeverse collections
- Datum extraction with hardcoded policy ID check
- Metadata label "721" hardcoded for CIP-25 detection

**Medium Priority Issues:**
- CIP-68 prefix constants hardcoded in multiple places
- Asset name length assumptions (56 chars for policy ID)
- String truncation lengths hardcoded

## Detailed Findings

### 1. Policy ID Hardcoding in summary.rs:499-500

**Location:** `src/summary.rs:499-500`

**Issue:**
```rust
fn shorten_policy_id(policy_id: &str) -> String {
    match policy_id {
        "3966cf1c948109e34f2c5a9f9670445ccc85008e5b8a6e67f913b491" => "Ancestors".to_string(),
        "de79250af8caffc7a64645d86939159f665d4107c3f198562007bf32" => "Nikeverse".to_string(),
        _ => {
            // Fallback logic...
        }
    }
}
```

**Problem:** Collection names are hardcoded, making it impossible to support new collections without code changes.

**Recommendation:** Create a configurable collection registry or lookup service.

### 2. Policy ID Hardcoding in rules.rs:1285-1288

**Location:** `src/rules.rs:1285-1288`

**Issue:**
```rust
if datum_str.contains("3966cf1c948109e34f2c5a9f9670445ccc85008e5b8a6e67f913b491") {
    extracted_policy = Some("3966cf1c948109e34f2c5a9f9670445ccc85008e5b8a6e67f913b491".to_string());
}
```

**Problem:** Datum extraction logic specifically looks for Ancestors policy ID, making it non-generic.

**Recommendation:** Use regex pattern matching or generic policy ID extraction.

### 3. CIP-25 Metadata Label Hardcoding

**Location:** Multiple files (`src/mints.rs:134`, `src/rules.rs:952`)

**Issue:**
```rust
if metadata.get("721").is_some() {
    return crate::MintType::Cip25;
}
```

**Problem:** CIP-25 metadata label "721" is hardcoded.

**Recommendation:** Define as a constant or configuration value.

### 4. CIP-68 Label Hardcoding in patterns.rs:2755

**Location:** `src/patterns.rs:2755`

**Issue:**
```rust
for key in ["50", "51", "52", "53", "54", "55", "56"] {
```

**Problem:** CIP-68 royalty-related metadata labels are hardcoded.

**Recommendation:** Define as constants with clear documentation.

### 5. Asset Length Assumptions

**Location:** Multiple files

**Issue:**
```rust
if asset_id.len() >= 56 {
    let policy_id = asset_id[..56].to_string();
    let asset_name = asset_id[56..].to_string();
}
```

**Problem:** Policy ID length (56 chars) is hardcoded throughout the codebase.

**Recommendation:** Define as a constant `POLICY_ID_LENGTH = 56`.

### 6. Address/String Truncation Lengths

**Location:** `src/summary.rs:514-518`

**Issue:**
```rust
fn shorten_address(address: &str) -> String {
    if address.len() > 12 {
        format!("{}...{}", &address[..6], &address[address.len() - 6..])
    }
}
```

**Problem:** Truncation lengths (12, 6) are hardcoded.

**Recommendation:** Make configurable for different display contexts.

## Proposed Solutions

### 1. Configuration System

Create a configuration struct for all hardcoded values:

```rust
pub struct ClassifierConfig {
    pub policy_id_length: usize,
    pub cip25_metadata_label: String,
    pub cip68_royalty_labels: Vec<String>,
    pub collection_names: HashMap<String, String>,
    pub display_config: DisplayConfig,
}

pub struct DisplayConfig {
    pub address_truncate_length: usize,
    pub address_prefix_length: usize,
    pub address_suffix_length: usize,
    pub policy_id_display_length: usize,
}
```

### 2. Collection Registry

Implement a pluggable collection registry:

```rust
pub trait CollectionRegistry {
    fn get_collection_name(&self, policy_id: &str) -> Option<String>;
    fn is_known_collection(&self, policy_id: &str) -> bool;
}
```

### 3. Constants Module

Create a constants module for all hardcoded values:

```rust
pub mod constants {
    pub const POLICY_ID_LENGTH: usize = 56;
    pub const CIP25_METADATA_LABEL: &str = "721";
    pub const CIP68_PREFIX_LENGTH: usize = 8;
    pub const CIP68_ROYALTY_LABELS: &[&str] = &["50", "51", "52", "53", "54", "55", "56"];
}
```

## Priority Recommendations

1. **High Priority:** Replace policy ID hardcoding with configurable collection registry
2. **High Priority:** Extract metadata label constants 
3. **Medium Priority:** Create configuration system for display formatting
4. **Low Priority:** Extract asset length assumptions to constants

## Benefits of Fixing

- **Maintainability:** New collections can be added via configuration
- **Testability:** Different configurations can be tested independently
- **Flexibility:** Different deployment environments can have different settings
- **Standards Compliance:** Proper adherence to CIP standards without hardcoding