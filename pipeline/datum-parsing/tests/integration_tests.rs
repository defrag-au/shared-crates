//! Integration tests for datum-parsing with TOML schema approach

use datum_parsing::{
    CborExtractor, MarketplaceDatumParser, MarketplaceOperation, MarketplaceType, TomlSchemaLoader,
};
use pipeline_types::OperationPayload;

#[test]
fn test_toml_schema_loading() {
    let mut loader = TomlSchemaLoader::new();

    // Test that no schemas are loaded initially (lazy loading)
    let initial_schemas = loader.list_schemas();
    assert!(
        initial_schemas.is_empty(),
        "Should start with no schemas loaded (lazy loading)"
    );

    // Test that all expected schemas can be loaded on demand
    let available_schemas = loader.list_available_schemas();
    assert!(
        !available_schemas.is_empty(),
        "Should have available schemas"
    );

    // Check for specific schemas (lazy loaded)
    assert!(loader.get_schema("jpg_store_v1_ask").is_some());
    assert!(loader.get_schema("jpg_store_v1_bid").is_some());
    assert!(loader.get_schema("jpg_store_v2_ask").is_some());
    assert!(loader.get_schema("jpg_store_v2_bid").is_some());
    assert!(loader.get_schema("jpg_store_v3_ask").is_some());
    assert!(loader.get_schema("jpg_store_v3_bid").is_some());
    assert!(loader.get_schema("jpg_store_v3_fee").is_some());
    assert!(loader.get_schema("wayup_ask").is_some());

    // Now we should have loaded schemas
    let loaded_schemas = loader.list_schemas();
    assert_eq!(loaded_schemas.len(), 8, "Should have 8 schemas loaded");

    println!("Available schemas: {available_schemas:?}");
    println!("Loaded schemas: {loaded_schemas:?}");
}

#[test]
fn test_cbor_extractor_creation() {
    let mut loader = TomlSchemaLoader::new();

    if let Some(schema) = loader.get_schema("jpg_store_v3_ask") {
        let _extractor = CborExtractor::new(schema);
        // Basic creation test - if this compiles, the API is working
        assert_eq!(schema.schema.name, "jpg_store_v3_ask");
    } else {
        panic!("JPG.store V3 ask schema should be available");
    }
}

#[test]
fn test_marketplace_datum_parser_creation() {
    let parser = MarketplaceDatumParser::new(MarketplaceType::JpgStoreV3);

    // Test with some dummy CBOR data (will likely fail parsing but shouldn't crash)
    let dummy_cbor = vec![0xd8, 0x79, 0x9f, 0xff]; // Basic CBOR array structure

    // This should fail gracefully, not panic
    let result = parser.parse_cbor(&dummy_cbor);
    assert!(result.is_err(), "Dummy CBOR should fail to parse");
}

#[test]
fn test_marketplace_operation_methods() {
    use pipeline_types::AssetId;

    // Test Ask operation
    let ask_operation = MarketplaceOperation::Ask {
        asset: Some(
            AssetId::new(
                "e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0".to_string(),
                "5065727033393733".to_string(),
            )
            .unwrap(),
        ),
        targets: vec![
            datum_parsing::LockedTarget {
                target: "addr1".to_string(),
                payload: OperationPayload::Lovelace { amount: 1_000_000 },
            },
            datum_parsing::LockedTarget {
                target: "addr2".to_string(),
                payload: OperationPayload::Lovelace { amount: 2_000_000 },
            },
        ],
    };

    assert_eq!(ask_operation.total_price_lovelace(), Some(3_000_000));
    assert_eq!(
        ask_operation.get_policy_id(),
        Some("e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0")
    );
    assert!(!ask_operation.is_collection_operation());

    // Test Bid operation
    let bid_operation = MarketplaceOperation::Bid {
        policy_id: "test_policy".to_string(),
        asset_name_hex: Some("test_asset".to_string()),
        offer_lovelace: 5_000_000,
    };

    assert_eq!(bid_operation.total_price_lovelace(), Some(5_000_000));
    assert_eq!(bid_operation.get_policy_id(), Some("test_policy"));
    assert!(!bid_operation.is_collection_operation());

    // Test collection bid
    let collection_bid = MarketplaceOperation::Bid {
        policy_id: "test_policy".to_string(),
        asset_name_hex: None,
        offer_lovelace: 10_000_000,
    };

    assert!(collection_bid.is_collection_operation());
}

#[test]
fn test_schema_structure_validation() {
    let mut loader = TomlSchemaLoader::new();

    if let Some(schema) = loader.get_schema("jpg_store_v3_ask") {
        // Validate schema structure
        assert_eq!(schema.schema.version, "3.0");
        assert_eq!(schema.schema.root_type, "ask_datum");

        // Check that the root type exists in types
        assert!(schema.types.contains_key(&schema.schema.root_type));

        // Check extraction methods
        assert!(schema.extraction.price_extraction.is_some());

        println!("JPG.store V3 Ask schema validation passed");
    }
}
