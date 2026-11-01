//! Who holds a collection, and how the holding is spread — derived from the
//! contract's own `Transfer` log.
//!
//! ```text
//! cargo run -p evm-chain-reader --example holder_census -- \
//!     https://rpc.mainnet.chain.robinhood.com 0x2370…3484 [0x505a…9146 …]
//! ```
//!
//! # Why the log and not an owner sweep
//!
//! The same reason the ownership design's decision 5 gives: state is pruned, so
//! `ownerOf` at anything but `latest` is refused, and a sweep at `latest` is one
//! `eth_call` per token. Logs are not pruned — a scan reaches genesis — and the
//! collection's whole history is one `eth_getLogs` per window.
//!
//! # The window comes from the node
//!
//! Robinhood Chain's endpoint names its own cap rather than paging:
//!
//! ```text
//! query spans 102945838 blocks (0 to 102945837), but only 10000000 are allowed
//! for this request; narrow the block range
//! ```
//!
//! (measured 2026-11-01). So the default window is that number and a refusal
//! halves it and retries the same cursor, which is what makes this work against a
//! node whose cap is smaller without being told which one it is.
//!
//! # What it counts
//!
//! Balances are movements, not a state read: a mint credits, a burn debits, a
//! transfer moves one. A run that starts above genesis will see debits for credits
//! it never saw — those are counted as `orphan debits` and reported rather than
//! clamped silently, because a clamped balance is a wrong holder count that looks
//! right.
//!
//! # One thing this cannot answer
//!
//! Whether a holder is a person. A marketplace that escrows on sale moves the token
//! to a contract address, which is a holder here the same as any wallet. That is the
//! same distinction the achievements design draws with burn and escrow, and it needs
//! the marketplace script's address to make — not a rule to guess at here.

use std::collections::HashMap;
use std::time::Duration;

use evm_chain_reader::{Address, BlockTag, EvmRpcClient, RpcError};

const USAGE: &str = "usage: holder_census <rpc-url> <contract>... [--from <block>] \
[--to <block>] [--window <blocks>] [--top <n>] [--out <dir>]";

/// Robinhood Chain's own `eth_getLogs` cap, measured 2026-11-01. A smaller-capped
/// node is handled by the halving rather than by a flag.
const DEFAULT_WINDOW: u64 = 10_000_000;

/// The narrowest window worth asking for: a node that cannot serve a thousand
/// blocks of one contract's logs is not one this can page.
const MIN_WINDOW: u64 = 1_000;

/// Gap between windows. The public endpoint rate-limits a burst (observed 429 on
/// back-to-back calls in the ownership worker).
const SPACING: Duration = Duration::from_millis(250);

/// How long to stand off after a 429.
const RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(5);

/// The rungs the spread is reported at — a holding ladder, not a histogram, so the
/// numbers line up with how a gate would be written.
const RUNGS: [u64; 8] = [2, 3, 5, 10, 25, 50, 100, 250];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(url) = args.next() else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };

    let mut contracts = Vec::new();
    let mut from = 0u64;
    let mut to: Option<u64> = None;
    let mut window = DEFAULT_WINDOW;
    let mut top = 5usize;
    let mut out: Option<String> = None;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--from" => from = args.next().ok_or(USAGE)?.parse()?,
            "--to" => to = Some(args.next().ok_or(USAGE)?.parse()?),
            "--window" => window = args.next().ok_or(USAGE)?.parse()?,
            "--top" => top = args.next().ok_or(USAGE)?.parse()?,
            "--out" => out = Some(args.next().ok_or(USAGE)?),
            flag if flag.starts_with('-') => {
                eprintln!("unknown flag {flag}\n{USAGE}");
                std::process::exit(2);
            }
            address => contracts.push(Address::parse(address)?),
        }
    }

    if contracts.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }

    let client = EvmRpcClient::new(url).with_timeout(Duration::from_secs(120));
    let chain = client.chain().await?;
    let head = client.block_number().await?;
    let to = to.unwrap_or(head);

    println!("holder census · {} — head block {head}", chain.as_caip2());
    println!("range {from}..{to} · windows of at most {window} blocks");
    println!();

    for contract in contracts {
        println!("{contract}");
        let census = walk(&client, &contract, from, to, window).await?;
        census.report(top);

        if let Some(dir) = &out {
            let path = format!("{dir}/{contract}.holders.jsonl");
            census.write(&path)?;
            println!("  wrote  {path}");
        }
        println!();
    }

    Ok(())
}

/// Every movement applied, and the balances it leaves behind.
struct Census {
    transfers: u64,
    mints: u64,
    burns: u64,
    /// A debit against an address with no balance — only reachable when the walk
    /// starts above the mint it missed.
    orphan_debits: u64,
    balances: HashMap<Address, u64>,
}

impl Census {
    fn new() -> Self {
        Self {
            transfers: 0,
            mints: 0,
            burns: 0,
            orphan_debits: 0,
            balances: HashMap::new(),
        }
    }

    fn apply(&mut self, transfers: Vec<evm_chain_reader::Transfer>) {
        for transfer in transfers {
            self.transfers += 1;
            if transfer.is_mint() {
                self.mints += 1;
            } else {
                let balance = self.balances.entry(transfer.from.clone()).or_insert(0);
                match balance.checked_sub(1) {
                    Some(held) => *balance = held,
                    None => self.orphan_debits += 1,
                }
            }

            if transfer.is_burn() {
                self.burns += 1;
            } else {
                *self.balances.entry(transfer.to.clone()).or_insert(0) += 1;
            }
        }
    }

    fn report(&self, top: usize) {
        let mut held: Vec<(Address, u64)> = self
            .balances
            .iter()
            .filter(|(_, balance)| **balance > 0)
            .map(|(address, balance)| (address.clone(), *balance))
            .collect();
        held.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));

        let holders = held.len();
        let tokens: u64 = held.iter().map(|(_, balance)| balance).sum();
        let median = if holders == 0 { 0 } else { held[holders / 2].1 };

        println!(
            "  log      {} transfers · {} mints · {} burns",
            self.transfers, self.mints, self.burns
        );
        if self.orphan_debits > 0 {
            println!(
                "  warning  {} debits for credits before the range — balances are short",
                self.orphan_debits
            );
        }
        println!(
            "  holders  {holders} holding {tokens} tokens — median {median}, mean {:.2}",
            if holders == 0 {
                0.0
            } else {
                tokens as f64 / holders as f64
            }
        );

        let rungs: Vec<String> = RUNGS
            .iter()
            .map(|rung| {
                let count = held.iter().filter(|(_, balance)| balance >= rung).count();
                let share = if holders == 0 {
                    0.0
                } else {
                    count as f64 * 100.0 / holders as f64
                };
                format!("≥{rung} {count} ({share:.0}%)")
            })
            .collect();
        println!("  spread   {}", rungs.join(" · "));

        if holders > 0 {
            let top_n: u64 = held.iter().take(top).map(|(_, balance)| balance).sum();
            let top_percent = (holders / 100).max(1);
            let top_one_percent: u64 = held
                .iter()
                .take(top_percent)
                .map(|(_, balance)| balance)
                .sum();
            println!(
                "  weight   top {top} {:.1}% · top 1% of holders ({top_percent}) {:.1}%",
                top_n as f64 * 100.0 / tokens as f64,
                top_one_percent as f64 * 100.0 / tokens as f64,
            );
            for (address, balance) in held.iter().take(top) {
                println!("           {address} {balance}");
            }
        }
    }

    fn write(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;

        let mut held: Vec<(&Address, &u64)> = self
            .balances
            .iter()
            .filter(|(_, balance)| **balance > 0)
            .collect();
        held.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));

        let mut file = std::fs::File::create(path)?;
        for (address, balance) in held {
            writeln!(file, "{{\"address\":\"{address}\",\"balance\":{balance}}}")?;
        }
        Ok(())
    }
}

/// Page the contract's `Transfer` log over `from..=to`, halving the window when the
/// node refuses the span.
async fn walk(
    client: &EvmRpcClient,
    contract: &Address,
    from: u64,
    to: u64,
    window: u64,
) -> Result<Census, Box<dyn std::error::Error>> {
    let mut census = Census::new();
    let mut cursor = from;
    let mut span = window.max(1);

    while cursor <= to {
        let end = cursor.saturating_add(span).saturating_sub(1).min(to);
        match client
            .transfers(contract, BlockTag::Number(cursor), BlockTag::Number(end))
            .await
        {
            Ok(transfers) => {
                let count = transfers.len();
                census.apply(transfers);
                eprintln!(
                    "  {cursor}..{end} ({span} blocks) — {count} transfers, {} held",
                    census
                        .balances
                        .values()
                        .filter(|balance| **balance > 0)
                        .count()
                );
                cursor = end.saturating_add(1);
            }
            Err(RpcError::Transport { status: 429, .. }) => {
                eprintln!("  429 at {cursor}..{end} — standing off");
                tokio::time::sleep(RATE_LIMIT_BACKOFF).await;
            }
            Err(RpcError::Rpc { message, .. }) if span > MIN_WINDOW => {
                span /= 2;
                eprintln!("  narrowed to {span} blocks at {cursor} ({message})");
            }
            Err(error) => return Err(error.into()),
        }

        tokio::time::sleep(SPACING).await;
    }

    Ok(census)
}
