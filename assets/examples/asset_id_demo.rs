use assets::AssetId;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 AssetId Demo - Unified Cardano Asset Identification");
    println!("{}", "=".repeat(60));

    // Example policy ID and asset name from Blackflag collection
    let policy_id = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";
    let asset_name_hex = "50697261746531303836"; // "Pirate1086" - real Blackflag asset

    println!("\n📝 Creating AssetId from components:");
    let asset_id = AssetId::new(policy_id.to_string(), asset_name_hex.to_string())?;
    println!("Policy ID: {}", asset_id.policy_id());
    println!("Asset Name Hex: {}", asset_id.asset_name_hex());
    println!("Asset Name UTF-8: {}", asset_id.asset_name());

    println!("\n🔗 Format Representations:");
    println!("Concatenated: {}", asset_id.concatenated());
    println!("Dot-delimited: {}", asset_id.dot_delimited());
    println!("Display format: {}", asset_id);

    println!("\n🧩 Parsing from Different Formats:");
    
    // Parse from concatenated format
    let concatenated = asset_id.concatenated();
    let parsed_concat = AssetId::parse_concatenated(&concatenated)?;
    println!("From concatenated: ✅ {}", parsed_concat.asset_name());

    // Parse from dot-delimited format  
    let dot_delimited = asset_id.dot_delimited();
    let parsed_dot = AssetId::parse_dot_delimited(&dot_delimited)?;
    println!("From dot-delimited: ✅ {}", parsed_dot.asset_name());

    // Smart parsing (auto-detects format)
    let smart_concat: AssetId = concatenated.parse()?;
    let smart_dot: AssetId = dot_delimited.parse()?;
    println!("Smart parse concat: ✅ {}", smart_concat.asset_name());
    println!("Smart parse dotted: ✅ {}", smart_dot.asset_name());

    println!("\n🚫 Asset Name Validation:");
    let empty_name_result = AssetId::new(policy_id.to_string(), String::new());
    println!("Empty asset name rejected: ✅ {}", empty_name_result.is_err());

    println!("\n🎭 Creating from UTF-8 Names:");
    let nft_from_name = AssetId::from_utf8_name(
        policy_id.to_string(),
        "MyNFT #123".to_string()
    )?;
    println!("UTF-8 name: 'MyNFT #123'");
    println!("Hex encoded: {}", nft_from_name.asset_name_hex());
    println!("Back to UTF-8: {}", nft_from_name.asset_name());

    println!("\n🔢 Binary Operations:");
    let policy_bytes = asset_id.policy_id_bytes()?;
    let name_bytes = asset_id.asset_name_bytes()?;
    let full_bytes = asset_id.as_bytes()?;
    println!("Policy ID bytes: {} bytes", policy_bytes.len());
    println!("Asset name bytes: {} bytes", name_bytes.len());
    println!("Full asset bytes: {} bytes", full_bytes.len());

    println!("\n📊 JSON Serialization:");
    let json = serde_json::to_string_pretty(&asset_id)?;
    println!("JSON representation:");
    println!("{}", json);

    println!("\n🔄 JSON Deserialization:");
    let from_json: AssetId = serde_json::from_str(&json)?;
    println!("Deserialized: ✅ {}", from_json.asset_name());

    // Also test string format deserialization
    let string_json = format!("\"{}\"", asset_id.dot_delimited());
    let from_string_json: AssetId = serde_json::from_str(&string_json)?;
    println!("From string JSON: ✅ {}", from_string_json.asset_name());

    println!("\n🗂️  Usage in Collections:");
    let mut asset_quantities: HashMap<AssetId, u64> = HashMap::new();
    
    // Add some assets
    asset_quantities.insert(asset_id.clone(), 1);
    asset_quantities.insert(nft_from_name.clone(), 1);

    println!("Asset inventory:");
    for (asset, qty) in &asset_quantities {
        println!("  {}: {} units", asset.asset_name(), qty);
    }

    println!("\n✨ Backward Compatibility:");
    // Convert to/from String for existing APIs
    let as_string: String = asset_id.clone().into();
    let from_string: AssetId = as_string.parse().expect("Should parse back to AssetId");
    println!("String conversion: ✅ {}", from_string == asset_id);

    // Use as HashMap key (concatenated format)
    let mut legacy_map: HashMap<String, u64> = HashMap::new();
    legacy_map.insert(asset_id.to_string(), 1);
    println!("Legacy HashMap key: ✅ Found asset with qty {}", 
             legacy_map.get(&asset_id.concatenated()).unwrap_or(&0));

    println!("\n🎉 Demo completed successfully!");
    
    Ok(())
}