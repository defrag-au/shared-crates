//! Transfer logs as a chain actually sends them.
//!
//! `topics[0] == keccak("Transfer(address,address,uint256)")` is the same for two
//! different shapes, and this contract is the second one.

use evm_chain_reader::{Log, decode_transfer};

/// A real `Transfer` off Robinhood Chain (4663), block 73,690,924. The collection
/// declared its token id **indexed**, so the log has four topics and an empty
/// `data` — not the three topics and populated `data` that EIP-721 specifies.
///
/// The node's answer also carries a `blockTimestamp` field that is not part of the
/// log object JSON-RPC defines; the fixture keeps it, so this doubles as the check
/// that an unexpected field does not fail the parse.
const INDEXED_TOKEN_ID: &str = include_str!("fixtures/transfer_indexed_token_id.json");

#[test]
fn a_captured_transfer_that_indexed_its_token_id_decodes() {
    let log: Log = serde_json::from_str(INDEXED_TOKEN_ID).expect("the log should parse");
    let transfer = decode_transfer(&log).expect("the log should decode");

    assert_eq!(
        transfer.contract.as_str(),
        "0xe9212e3ad53fe79c9fde2211e7980492edd75901"
    );
    assert_eq!(
        transfer.from.as_str(),
        "0x5ca36ac672f0c40ac4943efdf2c7695f00a34481"
    );
    assert_eq!(
        transfer.to.as_str(),
        "0x6e53e8380dc25cc9359e7150b619f51821f6d966"
    );
    assert_eq!(transfer.token_id.to_decimal(), "679");
    assert_eq!(transfer.block, Some(73_690_924));
    assert_eq!(transfer.log_index, Some(24));

    // Neither a mint nor a burn — the distinction the zero address carries.
    assert!(!transfer.is_mint());
    assert!(!transfer.is_burn());
}

#[test]
fn the_fixture_is_the_shape_the_standard_does_not_describe() {
    // Asserted against the bytes rather than the model: this is the fact that made
    // the decoder handle two shapes, so it should be pinned where it can be seen.
    let log: Log = serde_json::from_str(INDEXED_TOKEN_ID).unwrap();
    assert_eq!(log.topics.len(), 4);
    assert!(log.data.is_empty());
}
