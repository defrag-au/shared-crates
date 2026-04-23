#[cfg(test)]
mod integration_tests {
    use crate::{parse_block_cbor_hex_to_utxorpc, OuraBlock};

    use test_utils::{init_test_tracing, test_case};
    use utxorpc_spec::utxorpc::v1alpha::cardano as u5c;

    #[test]
    fn test_decode_oura_block() {
        init_test_tracing();

        let block = serde_json::from_str::<OuraBlock>(test_case!("oura.json")).unwrap();

        println!("🔍 Parsing Oura block CBOR...");
        println!("   Block CBOR length: {} characters", block.hex.len());

        match parse_block_cbor_hex_to_utxorpc(&block.hex) {
            Ok(utxorpc_block) => {
                println!("✅ Oura block CBOR successfully parsed to UTxORPC format!");
                println!("   Block header: {:?}", utxorpc_block.header.is_some());
                println!("   Block body: {:?}", utxorpc_block.body.is_some());

                // Basic validation that we got a UTxORPC block
                assert!(
                    utxorpc_block.header.is_some() || utxorpc_block.body.is_some(),
                    "Block should have header or body"
                );

                println!("\n🎯 UTxORPC conversion successful");
            }
            Err(err) => {
                println!("❌ Oura block CBOR parsing failed: {err:?}");

                // Let's also try to get diagnostic information about the structure
                println!("\n🔍 CBOR structure analysis:");
                println!(
                    "   Block hex starts with: {}",
                    &block.hex[..std::cmp::min(50, block.hex.len())]
                );

                panic!("Failed to parse Oura block CBOR: {err:?}");
            }
        }
    }

    #[test]
    fn test_extract_ug_mint() {
        init_test_tracing();

        let block = serde_json::from_str::<OuraBlock>(test_case!("oura_ug_mint.json")).unwrap();

        println!("🔍 Parsing Oura block CBOR...");
        println!("   Block CBOR length: {} characters", block.hex.len());

        match parse_block_cbor_hex_to_utxorpc(&block.hex) {
            Ok(u5c::Block {
                body: Some(block_body),
                ..
            }) => {
                // // Basic validation that we got a UTxORPC block
                // assert!(
                //     utxorpc_block.header.is_some() || utxorpc_block.body.is_some(),
                //     "Block should have header or body"
                // );

                println!("\n🎯 UTxORPC conversion successful {:?}", block_body);
            }
            _ => {
                panic!("parsing did not work as expected");
            }
        }
    }

    #[test]
    #[ignore = "Test needs to be rewritten for UTxORPC format - u64 precision handling is now in RawTxData domain"]
    fn test_u64_precision_issue() {
        init_test_tracing();

        println!("🔍 Testing UTxORPC CBOR parsing...");

        // Load the test data
        let block = serde_json::from_str::<OuraBlock>(test_case!("oura_u64.json")).unwrap();
        println!("   Block CBOR length: {} characters", block.hex.len());

        // Parse the block using UTxORPC decoder
        match parse_block_cbor_hex_to_utxorpc(&block.hex) {
            Ok(utxorpc_block) => {
                println!("✅ UTxORPC parsing successful");
                println!("   Block has header: {:?}", utxorpc_block.header.is_some());
                println!("   Block has body: {:?}", utxorpc_block.body.is_some());

                // TODO: Rewrite this test to validate UTxORPC structure
                // u64 precision handling is now in the RawTxData domain (indexers like Maestro)
            }
            Err(err) => {
                println!("❌ UTxORPC parsing failed: {err:?}");
                panic!("Failed to parse block as UTxORPC: {err:?}");
            }
        }
    }
}
