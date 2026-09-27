//! Reconcile one collection against the chain itself.
//!
//! ```text
//! cargo run -p evm-chain-reader --example reconcile -- \
//!     https://<rpc-endpoint> 0x7980aa64093853cb78c927e05b88fed96e945f81 [from-block] [to-block] [token-id]
//! ```
//!
//! Prints, on stderr, what the endpoint is (chain id and head), whether the
//! contract implements ERC-4906, and a count. On stdout, one JSON object per line:
//! every `Transfer` in the range, then — when a token id is given — that token's
//! owner and the owner's balance.
//!
//! This is the oracle the ownership design asks for, run by hand: the stream is
//! best-effort with no replay, so the chain is what a gap is checked against. A
//! wide block range may be refused by the endpoint; pass a range when it is.
//!
//! The endpoint is an argument rather than an environment variable, so nothing is
//! read that the caller did not state.

use std::time::Duration;

use evm_chain_reader::{Address, BlockTag, EvmRpcClient, U256};

const USAGE: &str = "usage: reconcile <rpc-url> <contract> [from-block] [to-block] [token-id]";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);

    let url = args.next().ok_or(USAGE)?;
    let contract = Address::parse(&args.next().ok_or(USAGE)?)?;

    let from = match args.next() {
        Some(raw) => BlockTag::Number(raw.parse()?),
        None => BlockTag::Earliest,
    };
    let to = match args.next() {
        Some(raw) => BlockTag::Number(raw.parse()?),
        None => BlockTag::Latest,
    };
    let token = match args.next() {
        Some(raw) => Some(U256::from_decimal(&raw)?),
        None => None,
    };

    let client = EvmRpcClient::new(url).with_timeout(Duration::from_secs(30));

    let chain = client.chain().await?;
    let head = client.block_number().await?;
    let caip2 = chain.as_caip2();
    eprintln!("{caip2} — head block {head}");
    eprintln!("contract {contract}");

    match client.supports_erc4906(&contract, BlockTag::Latest).await {
        Ok(true) => eprintln!("erc-4906: yes — metadata changes are a replayable log"),
        Ok(false) => eprintln!("erc-4906: no — metadata changes have no on-chain event here"),
        Err(error) => eprintln!("erc-4906: could not ask ({error})"),
    }

    let transfers = client.transfers(&contract, from, to).await?;
    eprintln!("{from}..{to}: {} transfers", transfers.len());
    for transfer in &transfers {
        println!("{}", serde_json::to_string(transfer)?);
    }

    if let Some(token) = token {
        let owner = client.owner_of(&contract, &token, BlockTag::Latest).await?;
        let balance = client
            .balance_of(&contract, &owner, BlockTag::Latest)
            .await?;
        eprintln!("token {token} is owned by {owner}, who holds {balance}");
    }

    Ok(())
}
