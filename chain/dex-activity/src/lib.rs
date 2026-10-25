//! DEX activity decoded from a Cardano block body.
//!
//! The gateway already fetches every block body for the chain heartbeat, so
//! turning one into venue activity costs no extra network — only the CPU to
//! walk transaction outputs that were already in memory.
//!
//! # What is exact, and what is not
//!
//! A swap's amounts live on both sides of the transaction: the pool UTxO it
//! consumed and the one it left behind. A block body carries only the second
//! half; the first is a `TxoRef` whose address and value are in an earlier
//! block. So this crate keeps a [`PoolBook`] of the venue UTxOs it has seen
//! created, and emits an event **only** when it can account for the UTxO a
//! transaction spent:
//!
//! - [`Activity::Flow`] — the UTxO was replaced in the same transaction, so
//!   the net flow into the venue is exact.
//! - [`Activity::Closed`] — the UTxO was spent and nothing at that credential
//!   replaced it. The assets are the ones it held.
//!
//! A UTxO is matched to its replacement by credential **and asset set**, so a
//! transaction touching several pools or orders of one venue keeps their
//! amounts apart instead of averaging them into one wrong number.
//!
//! A venue output with no tracked predecessor is **not** reported. It is
//! either a genuinely new pool or the first update of one created before this
//! process started, and the two cannot be told apart from a body alone. It is
//! still inserted, so the next transaction that touches it resolves.
//!
//! Nothing here claims a *trade*: a flow can be a swap, a liquidity add or a
//! deposit. The amounts and direction are facts; the label is the venue's.
//!
//! # Launchpads, and the memos that name them
//!
//! A bonding curve is not an AMM: the token trades there before it graduates,
//! and its supply is not float. [`VenueKind::Launchpad`] keeps the two apart,
//! so a consumer that prices every flow as a constant-product swap cannot do
//! it by accident (`snek.fun` was already in the registry this way; `Solada`
//! joins it).
//!
//! Two launchpad facts have no generic counterpart and are their own
//! [`Activity`] variants:
//!
//! - [`Activity::Launch`] — a funding transaction that seeds a curve which did
//!   not exist before. The generic rule would skip it, because there is no
//!   predecessor to diff against; the platform's own CIP-20 memo says it.
//! - [`Activity::LaunchMint`] — a CIP-67 (222) fungible supply minted into a
//!   curve. The mint **changes the curve's asset set**, so there is no matching
//!   group to diff either — and the ADA-only UTxO it replaced would otherwise
//!   read as a close.
//!
//! Every other Solada product is named by its memo ([`VenueTag`]): a
//! `solada:curveswap:` is a bonding-curve trade, `solada:pool-swap:buy:PERP`
//! an AMM pool trade, and so on. [`classify_memo`] is public so the box-side
//! ledger can share it rather than keep a second copy of the taxonomy.
//!
//! # Venues come from the registry, by credential
//!
//! [`address_registry`] carries every DEX and launchpad script by its 28-byte
//! payment credential, which is exactly the bytes a Shelley address carries
//! after its header byte. No bech32 decode, no pallas, no address string is
//! built per output.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::OnceLock;

use address_registry::registry::{AddressCategory, ScriptCategory, all_known_credentials};
use chain_heartbeat::{
    BlockError, BlockHeader, HeaderError, Network, block_transactions, split_block,
};
use minicbor::Decoder;
use minicbor::data::Type;
use serde::{Deserialize, Serialize};

/// The ui-flow domain this feed's notifications travel on.
pub const DEX_DOMAIN: &str = "dex";

/// Venue UTxOs remembered by default. Only outputs at a registered DEX or
/// launchpad credential are kept, so this is a few thousand active pools and
/// orders rather than a chain index.
pub const DEFAULT_BOOK_CAPACITY: usize = 10_000;

/// Which kind of venue a credential is, from the registry's own categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueKind {
    /// An AMM or order-book DEX.
    Exchange,
    /// A bonding curve, where a token trades before it graduates to an AMM.
    /// Not an AMM, and not priceable as one.
    Launchpad,
}

/// What a venue's own memo says the transaction is.
///
/// Solada writes a CIP-20 memo on every product it runs, and the taxonomy is
/// the platform's rather than this crate's: `solada:curveswap:…` is a
/// bonding-curve trade, `solada:pool-swap:buy:PERP` is an AMM pool trade, and
/// so on. [`classify_memo`] is public so a ledger reading the same memo cannot
/// drift from the feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VenueTag {
    /// `solada:launch:<slug>` — the funding transaction that seeds a curve.
    Launch { slug: String },
    /// `solada:curveswap:<swap-id>` — a bonding-curve trade.
    CurveSwap { id: String },
    /// `solada:curvebuy:` — the earlier curve form.
    CurveBuy,
    /// `solada:curvesell:` — the earlier curve form.
    CurveSell,
    /// `solada:pool-swap:<buy|sell>:<TICKER>` — an AMM pool trade.
    PoolSwap { side: PoolSide, ticker: String },
    /// `solada:swap:` — router-shaped, no ticker.
    Swap,
    /// `solada:adabuy:` — the ADA/USDC cash rail, not a token trade.
    AdaBuy,
    /// `solada:adasell:` — the ADA/USDC cash rail.
    AdaSell,
    /// `solada:walletclaim:` — a user withdrawal.
    WalletClaim,
    /// `solada:lock:` — lifecycle.
    Lock,
    /// `solada:launcherrelease:` — a creator release.
    LauncherRelease,
    /// A `solada:` memo this crate does not interpret yet.
    Other { memo: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolSide {
    Buy,
    Sell,
}

/// One change to a venue's UTxOs in one transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DexEvent {
    /// The venue's label from the registry: `"Splash"`, `"CSWAP"`, `"Solada"`.
    pub venue: String,
    /// Whether the venue is an exchange or a launchpad curve.
    pub venue_kind: VenueKind,
    /// `txhash#index` of the UTxO that was spent, when the transaction spent
    /// exactly one at this venue. `None` for a batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// The transaction that moved it. Always hex, so every event is checkable.
    pub tx_hash: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub height: u64,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub slot: u64,
    #[serde(default, with = "wasm_safe_serde::u64_option")]
    pub block_time_unix: Option<u64>,
    /// What the venue's own memo calls this transaction, when it carries one.
    /// `None` for a launch, whose [`Activity::Launch`] already names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<VenueTag>,
    pub activity: Activity,
}

/// What happened to the venue's UTxOs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Activity {
    /// The UTxO was replaced in place: the net flow **into** the venue.
    ///
    /// Positive `ada` is ADA paid into the pool; negative is ADA paid out.
    /// An asset delta is positive when the venue received that asset and
    /// negative when it released it — a token bought shows as negative.
    Flow {
        #[serde(with = "wasm_safe_serde::i64")]
        ada: i64,
        assets: Vec<AssetDelta>,
        /// UTxOs on each side of this pool. `1` is the normal case; more means
        /// several UTxOs with the same asset set moved together, and the flow
        /// is their combined net.
        utxos: u32,
    },
    /// The UTxO was spent and nothing replaced it: an order left the book, or
    /// a pool was closed.
    Closed {
        #[serde(with = "wasm_safe_serde::u64_required")]
        ada: u64,
        assets: Vec<AssetAmount>,
        utxos: u32,
    },
    /// A launchpad launch: the funding transaction that seeds a new curve.
    ///
    /// Reported from the platform's own memo, because there is no earlier
    /// curve UTxO to compare against and the generic flow rule would skip it.
    Launch {
        slug: String,
        #[serde(with = "wasm_safe_serde::u64_required")]
        ada: u64,
        utxos: u32,
    },
    /// A launch mint: a CIP-67 (222) fungible supply minted into a curve.
    ///
    /// Derived from the mint and the curve output, not from a memo. It is its
    /// own kind because the mint *changes the curve's asset set*: the previous
    /// UTxO held ADA only, so there is no matching group to diff against and
    /// the generic flow rule would report nothing at all.
    LaunchMint {
        policy_id: String,
        name_hex: String,
        #[serde(with = "wasm_safe_serde::u64_required")]
        supply: u64,
        /// Net ADA into the curve in the same transaction.
        #[serde(with = "wasm_safe_serde::i64")]
        ada: i64,
        utxos: u32,
    },
}

/// A native asset's net movement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDelta {
    pub policy_id: String,
    pub name_hex: String,
    #[serde(with = "wasm_safe_serde::i64")]
    pub delta: i64,
}

/// A native asset's amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetAmount {
    pub policy_id: String,
    pub name_hex: String,
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub amount: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    #[error(transparent)]
    Block(#[from] BlockError),
    #[error(transparent)]
    Header(#[from] HeaderError),
    #[error("transaction did not decode: {0}")]
    Tx(String),
}

/// A transaction output's identity: the transaction that created it and its
/// position in that transaction's output list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UtxoRef {
    pub tx_hash: [u8; 32],
    pub index: u32,
}

impl UtxoRef {
    fn label(&self) -> String {
        format!("{}#{}", hex::encode(self.tx_hash), self.index)
    }
}

/// Venue UTxOs seen, so the transaction that spends one has both sides.
///
/// Insertion-ordered and bounded: the oldest insertion is evicted first. A pool
/// that is active is spent and reinserted, so it stays; an order that sits for
/// longer than the book holds is simply resolved as `Closed` only if still
/// present, and is otherwise ignored rather than guessed at.
///
/// A rollback drops entries created above it. Entries the rolled-back block
/// *spent* are not restored, because the spend already removed them; the pool
/// they belonged to is therefore unresolved until it is next seen, never wrong.
pub struct PoolBook {
    capacity: usize,
    utxos: HashMap<UtxoRef, VenueUtxo>,
    /// Insertion order, with slots, for eviction and rollback. Removed entries
    /// leave stale keys here; they are skipped by `remove` on the way out.
    order: VecDeque<(UtxoRef, u64)>,
}

#[derive(Debug, Clone)]
struct VenueUtxo {
    credential: [u8; 28],
    venue: Venue,
    coin: u64,
    assets: BTreeMap<(String, String), u64>,
    slot: u64,
}

/// A registered venue: its label and which kind of contract it is.
#[derive(Debug, Clone, Copy)]
struct Venue {
    label: &'static str,
    kind: VenueKind,
}

impl Default for PoolBook {
    fn default() -> Self {
        Self::new(DEFAULT_BOOK_CAPACITY)
    }
}

impl PoolBook {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            utxos: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// How many venue UTxOs are currently remembered.
    pub fn len(&self) -> usize {
        self.utxos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.utxos.is_empty()
    }

    /// Forget everything. Used when a gap in the chain makes the book's
    /// entries unknown rather than stale: an entry believed unspent may have
    /// been spent in the blocks nobody saw.
    pub fn clear(&mut self) {
        self.utxos.clear();
        self.order.clear();
    }

    /// Drop entries created after `slot`, because they are no longer on the
    /// chain.
    pub fn rollback_to(&mut self, slot: u64) {
        while let Some((key, entry_slot)) = self.order.back() {
            if *entry_slot <= slot {
                break;
            }
            let key = *key;
            self.order.pop_back();
            self.utxos.remove(&key);
        }
    }

    /// Remember a venue output learned some other way than a block: the
    /// current UTxO of a pool, read from an indexer when a client starts, so
    /// the first transaction that spends it resolves like any other. Ignored,
    /// returning false, unless `address` is at a registered venue.
    pub fn seed(
        &mut self,
        utxo: UtxoRef,
        address: &[u8],
        coin: u64,
        assets: BTreeMap<(String, String), u64>,
        slot: u64,
    ) -> bool {
        let Some((credential, venue)) = venue_credential(address) else {
            return false;
        };
        self.insert(
            utxo,
            VenueUtxo {
                credential,
                venue,
                coin,
                assets,
                slot,
            },
        );
        true
    }

    fn take(&mut self, key: &UtxoRef) -> Option<VenueUtxo> {
        self.utxos.remove(key)
    }

    fn insert(&mut self, key: UtxoRef, utxo: VenueUtxo) {
        while self.order.len() >= self.capacity {
            let Some((oldest, _)) = self.order.pop_front() else {
                break;
            };
            self.utxos.remove(&oldest);
        }
        self.order.push_back((key, utxo.slot));
        self.utxos.insert(key, utxo);
    }
}

/// Decode one era-wrapped block into the venue activity it proves.
///
/// The book is advanced even when no event is emitted, which is what makes a
/// later transaction resolvable. Failed phase-2 transactions are ignored: they
/// are in the block and paid collateral, but their outputs do not exist.
pub fn trace_block(
    book: &mut PoolBook,
    network: Network,
    block: &[u8],
) -> Result<Vec<DexEvent>, TraceError> {
    let parts = split_block(block)?;
    let header = BlockHeader::decode(parts.header_variant, parts.header_cbor)?;
    let transactions = block_transactions(block)?;
    let hashes = transactions.hashes();
    let mut events = Vec::new();

    let at = At {
        height: header.height,
        slot: header.slot,
    };
    for (index, body) in transactions.bodies.iter().enumerate() {
        if transactions.invalid.contains(&(index as u32)) {
            continue;
        }
        let tx_hash = hashes[index];
        let parsed = match parse_tx(body) {
            Ok(parsed) => parsed,
            Err(e) => {
                tracing::warn!(
                    "dex-activity: tx {index} in block {} did not parse: {e}",
                    header.height
                );
                continue;
            }
        };
        let memo = transactions
            .aux
            .get(&(index as u32))
            .and_then(|aux| cip20_memo(aux));
        collect(book, at, network, tx_hash, parsed, memo, &mut events);
    }

    Ok(events)
}

/// Where a transaction sits on the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    pub height: u64,
    pub slot: u64,
}

/// Decode ONE valid transaction into the venue activity it proves, against
/// `book`, exactly as [`trace_block`] does for each transaction of a block.
///
/// For a consumer that is handed transactions rather than blocks: a client of
/// a watch socket (`chain_heartbeat::WatchedTx`), which receives only the
/// transactions touching one token. Every update of that token's pools
/// touches it, so such a client's book stays continuous for those pools from
/// the second transaction it sees; for the first, [`PoolBook::seed`] it with
/// the pool's current output if it can.
///
/// `tx_hash` is the blake2b-256 of `body`; `aux` the transaction's auxiliary
/// data, where a venue writes its memo. A transaction that failed phase-2
/// validation must not be passed: its outputs never existed.
pub fn trace_tx(
    book: &mut PoolBook,
    network: Network,
    at: At,
    tx_hash: [u8; 32],
    body: &[u8],
    aux: Option<&[u8]>,
) -> Result<Vec<DexEvent>, TraceError> {
    let parsed = parse_tx(body).map_err(|e| TraceError::Tx(e.to_string()))?;
    let memo = aux.and_then(cip20_memo);
    let mut events = Vec::new();
    collect(book, at, network, tx_hash, parsed, memo, &mut events);
    Ok(events)
}

/// One output of a transaction, whoever it pays: the address's raw bytes,
/// its ADA and its assets, and the venue it is at, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOutput {
    pub index: u32,
    /// The address as the body carries it (header byte, then credentials).
    pub address: Vec<u8>,
    pub coin: u64,
    /// `(policy hex, asset name hex) => quantity`.
    pub assets: BTreeMap<(String, String), u64>,
    /// The registry's label and kind, for an output at a venue.
    pub venue: Option<(&'static str, VenueKind)>,
}

/// Every output of a transaction body, in order. [`trace_tx`] reads only the
/// outputs at venues; this reads them all, for the consumer that wants to
/// know who a trade paid.
pub fn tx_outputs(body: &[u8]) -> Result<Vec<TxOutput>, TraceError> {
    read_all_outputs(body).map_err(|e| TraceError::Tx(e.to_string()))
}

/// Everything on one side of a transaction, summed for one pool or order: a
/// credential and the set of assets its UTxO held. Grouping by the asset set
/// keeps a transaction that touches several pools of one venue from averaging
/// them together — a batched DEX fill stays several answers, not one wrong one.
type Group = ([u8; 28], BTreeSet<(String, String)>);

#[derive(Debug)]
struct Side {
    venue: Venue,
    coin: u128,
    assets: BTreeMap<(String, String), u128>,
    utxos: Vec<UtxoRef>,
}

impl Side {
    fn of(venue: Venue) -> Self {
        Self {
            venue,
            coin: 0,
            assets: BTreeMap::new(),
            utxos: Vec::new(),
        }
    }
}

fn group(credential: [u8; 28], assets: &BTreeMap<(String, String), u64>) -> Group {
    (credential, assets.keys().cloned().collect())
}

fn collect(
    book: &mut PoolBook,
    header: At,
    network: Network,
    tx_hash: [u8; 32],
    parsed: ParsedTx,
    memo: Option<String>,
    events: &mut Vec<DexEvent>,
) {
    // Both sides are keyed by credential and asset set, and ordered, so the
    // events a block produces do not depend on hash-map iteration.
    let mut before: BTreeMap<Group, Side> = BTreeMap::new();
    for input in &parsed.inputs {
        let Some(utxo) = book.take(input) else {
            continue;
        };
        let side = before
            .entry(group(utxo.credential, &utxo.assets))
            .or_insert_with(|| Side::of(utxo.venue));
        side.utxos.push(*input);
        side.coin += u128::from(utxo.coin);
        for (unit, amount) in utxo.assets {
            *side.assets.entry(unit).or_default() += u128::from(amount);
        }
    }

    let mut after: BTreeMap<Group, Side> = BTreeMap::new();
    for output in &parsed.outputs {
        let Some((credential, venue)) = output.credential else {
            continue;
        };
        let side = after
            .entry(group(credential, &output.assets))
            .or_insert_with(|| Side::of(venue));
        side.utxos.push(UtxoRef {
            tx_hash,
            index: output.index,
        });
        side.coin += u128::from(output.coin);
        for (unit, amount) in &output.assets {
            *side.assets.entry(unit.clone()).or_default() += u128::from(*amount);
        }
    }

    let block_time_unix = network.slot_to_unix_secs(header.slot);
    let tag = memo.as_deref().and_then(classify_memo);

    if let Some(slug) = match &tag {
        Some(VenueTag::Launch { slug }) => Some(slug.clone()),
        _ => None,
    } {
        // A launch seeds a curve that does not exist yet, so there is no
        // predecessor to diff against. The platform's own memo is the fact.
        let mut ada = 0u128;
        let mut utxos = 0usize;
        let mut label = memo_venue(memo.as_deref());
        for side in after
            .values()
            .filter(|side| side.venue.kind == VenueKind::Launchpad)
        {
            ada += side.coin;
            utxos += side.utxos.len();
            label = Some(side.venue.label);
        }
        events.push(DexEvent {
            venue: label.unwrap_or("Solada").to_string(),
            venue_kind: VenueKind::Launchpad,
            pool: None,
            tx_hash: hex::encode(tx_hash),
            height: header.height,
            slot: header.slot,
            block_time_unix,
            tag: None,
            activity: Activity::Launch {
                slug,
                ada: saturate_u64(ada),
                utxos: utxos as u32,
            },
        });
        remember(book, header, tx_hash, &parsed.outputs);
        return;
    }

    if let Some(mint) = launch_mint(&parsed.mints, &after) {
        let mut ada = 0i128;
        let mut utxos = 0u32;
        let mut label = None;
        for side in after
            .values()
            .filter(|side| side.venue.kind == VenueKind::Launchpad)
        {
            ada += side.coin as i128;
            utxos += side.utxos.len() as u32;
            label = Some(side.venue.label);
        }
        for side in before
            .values()
            .filter(|side| side.venue.kind == VenueKind::Launchpad)
        {
            ada -= side.coin as i128;
        }
        events.push(DexEvent {
            venue: label.unwrap_or("Solada").to_string(),
            venue_kind: VenueKind::Launchpad,
            pool: None,
            tx_hash: hex::encode(tx_hash),
            height: header.height,
            slot: header.slot,
            block_time_unix,
            tag: None,
            activity: Activity::LaunchMint {
                policy_id: mint.policy_id.clone(),
                name_hex: mint.name_hex.clone(),
                supply: mint.quantity as u64,
                ada: saturate_i64(ada),
                utxos,
            },
        });
        remember(book, header, tx_hash, &parsed.outputs);
        return;
    }

    // A credential that still holds something after the transaction did not
    // close: an unmatched asset set means the pool's composition changed (it
    // gave away the last of a token), not that it vanished.
    let still_open: BTreeSet<[u8; 28]> = after.keys().map(|(credential, _)| *credential).collect();
    for (key, pre) in &before {
        let Some(post) = after.get(key) else {
            if still_open.contains(&key.0) {
                continue;
            }
            events.push(DexEvent {
                venue: pre.venue.label.to_string(),
                venue_kind: pre.venue.kind,
                pool: single(&pre.utxos),
                tx_hash: hex::encode(tx_hash),
                height: header.height,
                slot: header.slot,
                block_time_unix,
                tag: tag.clone(),
                activity: Activity::Closed {
                    ada: saturate_u64(pre.coin),
                    assets: amounts(&pre.assets),
                    utxos: pre.utxos.len() as u32,
                },
            });
            continue;
        };

        let assets = deltas(&pre.assets, &post.assets);
        let ada = post.coin as i128 - pre.coin as i128;
        events.push(DexEvent {
            venue: pre.venue.label.to_string(),
            venue_kind: pre.venue.kind,
            pool: single(&pre.utxos),
            tx_hash: hex::encode(tx_hash),
            height: header.height,
            slot: header.slot,
            block_time_unix,
            tag: tag.clone(),
            activity: Activity::Flow {
                ada: saturate_i64(ada),
                assets,
                utxos: pre.utxos.len().max(post.utxos.len()) as u32,
            },
        });
    }

    remember(book, header, tx_hash, &parsed.outputs);
}

fn remember(book: &mut PoolBook, header: At, tx_hash: [u8; 32], outputs: &[ParsedOutput]) {
    for output in outputs {
        let Some((credential, venue)) = output.credential else {
            continue;
        };
        book.insert(
            UtxoRef {
                tx_hash,
                index: output.index,
            },
            VenueUtxo {
                credential,
                venue,
                coin: output.coin,
                assets: output.assets.clone(),
                slot: header.slot,
            },
        );
    }
}

fn single(utxos: &[UtxoRef]) -> Option<String> {
    match utxos {
        [one] => Some(one.label()),
        _ => None,
    }
}

fn amounts(assets: &BTreeMap<(String, String), u128>) -> Vec<AssetAmount> {
    assets
        .iter()
        .map(|((policy_id, name_hex), amount)| AssetAmount {
            policy_id: policy_id.clone(),
            name_hex: name_hex.clone(),
            amount: saturate_u64(*amount),
        })
        .collect()
}

/// Every unit either side touched, with the net flow into the venue. A unit
/// that did not move is omitted rather than reported as zero.
fn deltas(
    before: &BTreeMap<(String, String), u128>,
    after: &BTreeMap<(String, String), u128>,
) -> Vec<AssetDelta> {
    let units: BTreeSet<&(String, String)> = before.keys().chain(after.keys()).collect();
    units
        .into_iter()
        .filter_map(|unit| {
            let pre = before.get(unit).copied().unwrap_or(0);
            let post = after.get(unit).copied().unwrap_or(0);
            let delta = post as i128 - pre as i128;
            (delta != 0).then(|| AssetDelta {
                policy_id: unit.0.clone(),
                name_hex: unit.1.clone(),
                delta: saturate_i64(delta),
            })
        })
        .collect()
}

fn saturate_u64(value: u128) -> u64 {
    value.min(u128::from(u64::MAX)) as u64
}

fn saturate_i64(value: i128) -> i64 {
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

// ── Block body decoding ──────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct ParsedTx {
    inputs: Vec<UtxoRef>,
    outputs: Vec<ParsedOutput>,
    mints: Vec<Mint>,
}

#[derive(Debug)]
struct ParsedOutput {
    index: u32,
    credential: Option<([u8; 28], Venue)>,
    coin: u64,
    assets: BTreeMap<(String, String), u64>,
}

/// A minted (positive) or burned (negative) quantity from the body's mint field.
#[derive(Debug)]
struct Mint {
    policy_id: String,
    name_hex: String,
    quantity: i64,
}

/// A transaction body's fields this feed needs: inputs (0), outputs (1) and
/// mints (9). Everything else is skipped, including certificates and redeemers.
fn parse_tx(body: &[u8]) -> Result<ParsedTx, minicbor::decode::Error> {
    let mut d = Decoder::new(body);
    let mut tx = ParsedTx::default();
    let count = d.map()?;
    let mut index = 0u64;
    while !done(count, index, &mut d)? {
        let key = d.u32()?;
        match key {
            0 => tx.inputs = read_inputs(&mut d)?,
            1 => tx.outputs = read_outputs(&mut d)?,
            9 => tx.mints = read_mints(&mut d)?,
            _ => d.skip()?,
        }
        index += 1;
    }
    Ok(tx)
}

fn read_mints(d: &mut Decoder<'_>) -> Result<Vec<Mint>, minicbor::decode::Error> {
    let policies = d.map()?;
    let mut mints = Vec::new();
    let mut policy_index = 0u64;
    while !done(policies, policy_index, d)? {
        let policy_id = hex::encode(d.bytes()?);
        let names = d.map()?;
        let mut name_index = 0u64;
        while !done(names, name_index, d)? {
            mints.push(Mint {
                policy_id: policy_id.clone(),
                name_hex: hex::encode(d.bytes()?),
                quantity: d.i64()?,
            });
            name_index += 1;
        }
        policy_index += 1;
    }
    Ok(mints)
}

fn read_inputs(d: &mut Decoder<'_>) -> Result<Vec<UtxoRef>, minicbor::decode::Error> {
    let count = enter_array(d)?;
    let mut inputs = Vec::new();
    let mut index = 0u64;
    while !done(count, index, d)? {
        d.array()?;
        let hash = d.bytes()?;
        let tx_hash: [u8; 32] = hash
            .try_into()
            .map_err(|_| minicbor::decode::Error::message("input hash is not 32 bytes"))?;
        let output_index = d.u32()?;
        inputs.push(UtxoRef {
            tx_hash,
            index: output_index,
        });
        index += 1;
    }
    Ok(inputs)
}

fn read_outputs(d: &mut Decoder<'_>) -> Result<Vec<ParsedOutput>, minicbor::decode::Error> {
    let count = enter_array(d)?;
    let mut outputs = Vec::new();
    let mut index = 0u64;
    while !done(count, index, d)? {
        if let Some(output) = read_output(d, index as u32)? {
            outputs.push(output);
        }
        index += 1;
    }
    Ok(outputs)
}

fn read_output(
    d: &mut Decoder<'_>,
    index: u32,
) -> Result<Option<ParsedOutput>, minicbor::decode::Error> {
    match d.datatype()? {
        // Post-Alonzo: `[address, value, datum_option?, script_ref?]`.
        Type::Array => {
            let fields = d.array()?;
            let address = d.bytes()?;
            let credential = venue_credential(address);
            let mut output = ParsedOutput {
                index,
                credential,
                coin: 0,
                assets: BTreeMap::new(),
            };
            if credential.is_some() {
                read_value(d, &mut output)?;
            } else {
                d.skip()?;
            }
            let mut field = 2u64;
            while !done(fields, field, d)? {
                d.skip()?;
                field += 1;
            }
            Ok(credential.map(|_| output))
        }
        // Pre-Babbage: `{0: address, 1: value, 2: datum_hash}`.
        Type::Map => {
            let fields = d.map()?;
            let mut output = ParsedOutput {
                index,
                credential: None,
                coin: 0,
                assets: BTreeMap::new(),
            };
            let mut field = 0u64;
            while !done(fields, field, d)? {
                match d.u32()? {
                    0 => output.credential = venue_credential(d.bytes()?),
                    1 if output.credential.is_some() => read_value(d, &mut output)?,
                    _ => d.skip()?,
                }
                field += 1;
            }
            Ok(output.credential.map(|_| output))
        }
        other => Err(minicbor::decode::Error::message(format!(
            "output is {other:?}"
        ))),
    }
}

fn read_value(
    d: &mut Decoder<'_>,
    output: &mut ParsedOutput,
) -> Result<(), minicbor::decode::Error> {
    read_value_into(d, &mut output.coin, &mut output.assets)
}

fn read_value_into(
    d: &mut Decoder<'_>,
    coin: &mut u64,
    assets: &mut BTreeMap<(String, String), u64>,
) -> Result<(), minicbor::decode::Error> {
    match d.datatype()? {
        Type::U8 | Type::U16 | Type::U32 | Type::U64 => *coin = d.u64()?,
        Type::Array => {
            d.array()?;
            *coin = d.u64()?;
            if d.datatype()? == Type::Map {
                read_multiasset(d, assets)?;
            }
        }
        other => {
            return Err(minicbor::decode::Error::message(format!(
                "value is {other:?}"
            )));
        }
    }
    Ok(())
}

/// Every output of a body, whoever it pays (see [`tx_outputs`]).
fn read_all_outputs(body: &[u8]) -> Result<Vec<TxOutput>, minicbor::decode::Error> {
    let mut d = Decoder::new(body);
    let count = d.map()?;
    let mut index = 0u64;
    while !done(count, index, &mut d)? {
        if d.u32()? != 1 {
            d.skip()?;
            index += 1;
            continue;
        }
        let outputs = enter_array(&mut d)?;
        let mut found = Vec::new();
        let mut i = 0u64;
        while !done(outputs, i, &mut d)? {
            found.push(read_any_output(&mut d, i as u32)?);
            i += 1;
        }
        return Ok(found);
    }
    Ok(Vec::new())
}

fn read_any_output(d: &mut Decoder<'_>, index: u32) -> Result<TxOutput, minicbor::decode::Error> {
    let mut out = TxOutput {
        index,
        address: Vec::new(),
        coin: 0,
        assets: BTreeMap::new(),
        venue: None,
    };
    match d.datatype()? {
        // Post-Alonzo: `[address, value, datum_option?, script_ref?]`.
        Type::Array => {
            let fields = d.array()?;
            out.address = d.bytes()?.to_vec();
            read_value_into(d, &mut out.coin, &mut out.assets)?;
            let mut field = 2u64;
            while !done(fields, field, d)? {
                d.skip()?;
                field += 1;
            }
        }
        // Pre-Babbage: `{0: address, 1: value, 2: datum_hash}`.
        Type::Map => {
            let fields = d.map()?;
            let mut field = 0u64;
            while !done(fields, field, d)? {
                match d.u32()? {
                    0 => out.address = d.bytes()?.to_vec(),
                    1 => read_value_into(d, &mut out.coin, &mut out.assets)?,
                    _ => d.skip()?,
                }
                field += 1;
            }
        }
        other => {
            return Err(minicbor::decode::Error::message(format!(
                "output is {other:?}"
            )));
        }
    }
    out.venue = venue_credential(&out.address).map(|(_, v)| (v.label, v.kind));
    Ok(out)
}

fn read_multiasset(
    d: &mut Decoder<'_>,
    assets: &mut BTreeMap<(String, String), u64>,
) -> Result<(), minicbor::decode::Error> {
    let policies = d.map()?;
    let mut policy_index = 0u64;
    while !done(policies, policy_index, d)? {
        let policy_id = hex::encode(d.bytes()?);
        let names = d.map()?;
        let mut name_index = 0u64;
        while !done(names, name_index, d)? {
            let name_hex = hex::encode(d.bytes()?);
            assets.insert((policy_id.clone(), name_hex), d.u64()?);
            name_index += 1;
        }
        policy_index += 1;
    }
    Ok(())
}

/// Advance to the next item of a definite or indefinite container, returning
/// whether there is none left. Consumes the break of an indefinite one.
fn done(
    count: Option<u64>,
    index: u64,
    d: &mut Decoder<'_>,
) -> Result<bool, minicbor::decode::Error> {
    match count {
        Some(len) => Ok(index >= len),
        None => {
            if d.datatype()? == Type::Break {
                d.set_position(d.position() + 1);
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

/// Enter an array that may be tagged as a set (258), as transaction inputs are.
fn enter_array(d: &mut Decoder<'_>) -> Result<Option<u64>, minicbor::decode::Error> {
    if d.datatype()? == Type::Tag {
        d.tag()?;
    }
    d.array()
}

// ── Venues ───────────────────────────────────────────────────────────────────

/// The payment credentials this feed follows, decoded once from the registry.
fn venues() -> &'static HashMap<[u8; 28], Venue> {
    static VENUES: OnceLock<HashMap<[u8; 28], Venue>> = OnceLock::new();
    VENUES.get_or_init(|| {
        all_known_credentials()
            .iter()
            .filter_map(|(hex, entry)| {
                let venue = venue_of(&entry.category)?;
                let mut credential = [0u8; 28];
                hex::decode_to_slice(hex, &mut credential).ok()?;
                Some((credential, venue))
            })
            .collect()
    })
}

/// The venue a registry category describes, or `None` for everything else.
///
/// Marketplaces are deliberately absent: their listings and offers belong on
/// their own feed, where a `Closed` is a sale rather than an order leaving a
/// book. Adding them here is this match and nothing else.
fn venue_of(category: &AddressCategory) -> Option<Venue> {
    match category {
        AddressCategory::Script(ScriptCategory::Exchange { label }) => Some(Venue {
            label,
            kind: VenueKind::Exchange,
        }),
        AddressCategory::Script(ScriptCategory::Launchpad { label }) => Some(Venue {
            label,
            kind: VenueKind::Launchpad,
        }),
        _ => None,
    }
}

/// The venue credential of a Shelley address, if the address is a script
/// address at a registered venue.
///
/// The header's high nibble is the address type: 0-7 are payment addresses and
/// an odd type is a script payment credential. The credential is the 28 bytes
/// after the header, which is the same key the registry's credential table uses.
fn venue_credential(address: &[u8]) -> Option<([u8; 28], Venue)> {
    let (&header, rest) = address.split_first()?;
    let kind = header >> 4;
    if kind > 7 || kind % 2 == 0 || rest.len() < 28 {
        return None;
    }
    let credential: [u8; 28] = rest[..28].try_into().ok()?;
    venues().get(&credential).map(|venue| (credential, *venue))
}

// ── Venue memos ──────────────────────────────────────────────────────────────

/// The CIP-67 label prefix for a fungible asset (label 222), hex.
const CIP67_FUNGIBLE: &str = "0014df10";

/// Classify a platform memo into the kind of transaction it names.
///
/// This is the shared copy of a taxonomy `solada-decode` also reads; keeping it
/// here is what lets the block feed and the box ledger agree. `None` is a memo
/// that is not a venue's at all.
pub fn classify_memo(memo: &str) -> Option<VenueTag> {
    let rest = memo.strip_prefix("solada:")?;
    Some(if let Some(slug) = rest.strip_prefix("launch:") {
        VenueTag::Launch {
            slug: slug.to_string(),
        }
    } else if let Some(id) = rest.strip_prefix("curveswap:") {
        VenueTag::CurveSwap { id: id.to_string() }
    } else if rest.starts_with("curvebuy:") {
        VenueTag::CurveBuy
    } else if rest.starts_with("curvesell:") {
        VenueTag::CurveSell
    } else if let Some(rest) = rest.strip_prefix("pool-swap:") {
        let mut parts = rest.splitn(2, ':');
        let side = match parts.next() {
            Some("buy") => Some(PoolSide::Buy),
            Some("sell") => Some(PoolSide::Sell),
            _ => None,
        };
        // A side this crate cannot read is not guessed at.
        match side {
            Some(side) => VenueTag::PoolSwap {
                side,
                ticker: parts.next().unwrap_or_default().to_string(),
            },
            None => VenueTag::Other {
                memo: memo.to_string(),
            },
        }
    } else if rest.starts_with("swap:") {
        VenueTag::Swap
    } else if rest.starts_with("adabuy:") {
        VenueTag::AdaBuy
    } else if rest.starts_with("adasell:") {
        VenueTag::AdaSell
    } else if rest.starts_with("walletclaim:") {
        VenueTag::WalletClaim
    } else if rest.starts_with("lock:") {
        VenueTag::Lock
    } else if rest.starts_with("launcherrelease:") {
        VenueTag::LauncherRelease
    } else {
        VenueTag::Other {
            memo: memo.to_string(),
        }
    })
}

/// The venue a memo's namespace names, for a launch that creates a curve this
/// process has not seen. The memo is the platform's own marker, so the name is
/// read from it rather than guessed from an address.
fn memo_venue(memo: Option<&str>) -> Option<&'static str> {
    memo?.strip_prefix("solada:").map(|_| "Solada")
}

/// A launch mint: a CIP-67 (222) fungible asset minted and landing in a
/// launchpad curve in the same transaction.
fn launch_mint<'a>(mints: &'a [Mint], after: &BTreeMap<Group, Side>) -> Option<&'a Mint> {
    let mint = mints
        .iter()
        .find(|mint| mint.quantity > 0 && mint.name_hex.starts_with(CIP67_FUNGIBLE))?;
    let unit = (mint.policy_id.clone(), mint.name_hex.clone());
    let lands_in_curve = after.iter().any(|(_, side)| {
        side.venue.kind == VenueKind::Launchpad && side.assets.contains_key(&unit)
    });
    lands_in_curve.then_some(mint)
}

/// The CIP-20 `msg` carried in a transaction's auxiliary data, if any.
///
/// Aux data is either a map (Alonzo and earlier) or `#6.259(map)` (Babbage
/// and later). Label 674 holds `{ "msg": [text, …] }`; the first fragment is
/// the message, which is how the platform writes it.
fn cip20_memo(aux: &[u8]) -> Option<String> {
    let mut d = Decoder::new(aux);
    if d.datatype().ok()? == Type::Tag {
        d.tag().ok()?;
    }
    let fields = d.map().ok()?;
    let mut index = 0u64;
    while !done(fields, index, &mut d).ok()? {
        let key = d.u64().ok()?;
        if key != 0 {
            d.skip().ok()?;
            index += 1;
            continue;
        }
        let labels = d.map().ok()?;
        let mut label_index = 0u64;
        while !done(labels, label_index, &mut d).ok()? {
            let label = d.u64().ok()?;
            if label == 674 {
                return cip20_msg(&mut d).ok()?;
            }
            d.skip().ok()?;
            label_index += 1;
        }
        index += 1;
    }
    None
}

fn cip20_msg(d: &mut Decoder<'_>) -> Result<Option<String>, minicbor::decode::Error> {
    let fields = d.map()?;
    let mut index = 0u64;
    while !done(fields, index, d)? {
        if d.datatype()? != Type::String {
            d.skip()?;
            index += 1;
            continue;
        }
        let is_msg = d.str()? == "msg";
        if !is_msg {
            d.skip()?;
            index += 1;
            continue;
        }
        let fragments = d.array()?;
        let mut fragment_index = 0u64;
        while !done(fragments, fragment_index, d)? {
            if d.datatype()? == Type::String {
                return Ok(Some(d.str()?.to_string()));
            }
            d.skip()?;
            fragment_index += 1;
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use chain_heartbeat::block_transactions;
    use minicbor::Encoder;

    use super::*;

    /// CSWAP's order/pool script, from `address-registry`'s credential table.
    const CSWAP: &str = "ed97e0a1394724bb7cb94f20acf627abc253694c92b88bf8fb4b7f6f";
    /// jpg.store V2's sale script. A marketplace, so this feed must ignore it.
    const JPG_SALE: &str = "c727443d77df6cff95dca383994f4c3024d03ff56b02ecc22b0f3f65";
    const POLICY: [u8; 28] = [0xAA; 28];
    const ALIENS: &[u8] = b"ALIENS";
    const POLICY_B: [u8; 28] = [0xBB; 28];
    const BUNNY: &[u8] = b"BUNNY";
    /// CIP-67 (222) fungible prefix + "MODR", as a Solada launch mints it.
    const MODR_NAME: &[u8] = &[0x00, 0x14, 0xDF, 0x10, b'M', b'O', b'D', b'R'];
    /// Solada's bonding curve, from `address-registry`'s credential table.
    const SOLADA: &str = "8a0cfe6369fd7d7295e684ab2d6e8f62398a3bb53fffbe788ec67033";
    const SLOT: u64 = 186_800_000;
    const HEIGHT: u64 = 13_000_000;

    /// $BUNNY's transactions around two curve buys on LumpFun v5, recorded
    /// from mainnet on 2026-10-25 (the bodies a watch socket delivers), with
    /// their hashes, in chain order.
    const BUNNY_POLICY: &str = "a1b085f414fb7799d378e90e39d24c9e54c841a311d72b2f57cba293";
    const BUNNY_TXS: [(u64, u64, &str, &[u8]); 4] = [
        (
            14_112_344,
            201_327_303,
            "0c95ba5453d68900927864347fa0100506c6fda1af344f4fe427b73b7b2e0685",
            include_bytes!("../tests/fixtures/bunny/201327303-0c95ba54.body.cbor"),
        ),
        (
            14_112_344,
            201_327_303,
            "fcc08b3a07544da5daca314ed0c2843c73e9bd4a846fe9039fd6951cc82b28a9",
            include_bytes!("../tests/fixtures/bunny/201327303-fcc08b3a.body.cbor"),
        ),
        (
            14_112_357,
            201_327_473,
            "072536b61c49932b2d984471282f31d817ce32e17e79a31140b5d91aee78dfe1",
            include_bytes!("../tests/fixtures/bunny/201327473-072536b6.body.cbor"),
        ),
        (
            14_112_780,
            201_335_799,
            "0624336e462487d7744650e2eb51ab94004f6b15b00187ca9c5ee5e590180811",
            include_bytes!("../tests/fixtures/bunny/201335799-0624336e.body.cbor"),
        ),
    ];

    /// One transaction at a time, as a watch client decodes them: the first
    /// curve update has no predecessor and only teaches the book; the fee
    /// claim that follows moves ADA and no token; the next buy is exact, the
    /// curve releasing 4,554,698 $BUNNY (the amount Koios shows the buyer
    /// receiving).
    #[test]
    fn a_watch_client_traces_bunny_one_transaction_at_a_time() {
        let mut book = PoolBook::default();
        let mut flows = Vec::new();
        for (height, slot, hash, body) in BUNNY_TXS {
            let mut tx_hash = [0u8; 32];
            hex::decode_to_slice(hash, &mut tx_hash).expect("hash");
            let at = At { height, slot };
            let events =
                trace_tx(&mut book, Network::Mainnet, at, tx_hash, body, None).expect("traces");
            for e in events {
                assert_eq!(e.venue, "LumpFun");
                assert_eq!(e.venue_kind, VenueKind::Launchpad);
                assert_eq!(e.tx_hash, hash);
                flows.push(e);
            }
        }
        let token = |e: &DexEvent| match &e.activity {
            Activity::Flow { ada, assets, .. } => (
                *ada,
                assets
                    .iter()
                    .find(|a| a.policy_id == BUNNY_POLICY)
                    .map(|a| a.delta),
            ),
            other => panic!("expected a flow, got {other:?}"),
        };
        assert_eq!(
            flows.len(),
            2,
            "the claim and the buy; the first only seeds"
        );
        assert_eq!(token(&flows[0]), (-2_560_784, None), "a fee claim: ADA out");
        let (ada, bunny) = token(&flows[1]);
        assert_eq!(bunny, Some(-4_554_698), "the curve released the buy");
        assert!(ada > 0, "ADA in for a buy, {ada}");

        // Who it paid: the one output outside a venue holding $BUNNY.
        let outputs = tx_outputs(BUNNY_TXS[3].3).expect("outputs");
        let paid: Vec<&TxOutput> = outputs
            .iter()
            .filter(|o| o.venue.is_none())
            .filter(|o| o.assets.keys().any(|(p, _)| p == BUNNY_POLICY))
            .collect();
        assert_eq!(paid.len(), 1);
        assert_eq!(paid[0].assets.values().copied().max(), Some(4_554_698));
        assert!(
            outputs
                .iter()
                .any(|o| o.venue == Some(("LumpFun", VenueKind::Launchpad)))
        );
    }

    /// A pool output learned elsewhere (an indexer, at start-up) seeds the
    /// book, so the very first transaction that spends it is exact: here the
    /// curve output the 9.1M buy left, seeded rather than traced, makes the
    /// fee claim resolve on its own.
    #[test]
    fn a_seeded_pool_resolves_the_first_spend() {
        let (_, _, fcc, buy) = BUNNY_TXS[1];
        let curve = tx_outputs(buy)
            .expect("outputs")
            .into_iter()
            .find(|o| o.venue.is_some())
            .expect("the curve output");
        let mut book = PoolBook::default();
        let mut tx_hash = [0u8; 32];
        hex::decode_to_slice(fcc, &mut tx_hash).expect("hash");
        assert!(book.seed(
            UtxoRef {
                tx_hash,
                index: curve.index
            },
            &curve.address,
            curve.coin,
            curve.assets.clone(),
            201_327_303,
        ));
        let (height, slot, claim, body) = BUNNY_TXS[2];
        let mut claim_hash = [0u8; 32];
        hex::decode_to_slice(claim, &mut claim_hash).expect("hash");
        let events = trace_tx(
            &mut book,
            Network::Mainnet,
            At { height, slot },
            claim_hash,
            body,
            None,
        )
        .expect("traces");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].activity,
            Activity::Flow {
                ada: -2_560_784,
                ..
            }
        ));
        // An address at no venue is not remembered.
        assert!(!book.seed(
            UtxoRef { tx_hash, index: 0 },
            &[0x01; 57],
            1,
            BTreeMap::new(),
            1
        ));
    }

    #[test]
    fn a_spend_against_a_known_pool_reports_the_net_flow() {
        let mut book = PoolBook::default();

        let created = block(HEIGHT, SLOT, vec![tx(&[], &[pool_output(100_000_000, 10)])]);
        assert_eq!(
            trace_block(&mut book, Network::Mainnet, &created).unwrap(),
            vec![]
        );
        assert_eq!(book.len(), 1, "the pool output is remembered");

        let first = block_transactions(&created).unwrap().hashes()[0];
        let swapped = block(
            HEIGHT + 1,
            SLOT + 20,
            vec![tx(&[(first, 0)], &[pool_output(105_000_000, 9)])],
        );
        let events = trace_block(&mut book, Network::Mainnet, &swapped).unwrap();

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.venue, "CSWAP");
        assert_eq!(
            event.pool.as_deref(),
            Some(format!("{}#0", hex::encode(first)).as_str())
        );
        assert_eq!(event.height, HEIGHT + 1);
        assert_eq!(event.slot, SLOT + 20);
        assert_eq!(
            event.activity,
            Activity::Flow {
                ada: 5_000_000,
                assets: vec![AssetDelta {
                    policy_id: hex::encode(POLICY),
                    name_hex: hex::encode(ALIENS),
                    delta: -1,
                }],
                utxos: 1,
            }
        );
    }

    #[test]
    fn a_batch_touching_two_pools_keeps_their_amounts_apart() {
        let mut book = PoolBook::default();
        let created = block(
            HEIGHT,
            SLOT,
            vec![tx(
                &[],
                &[pool_output(100_000_000, 10), bunny_output(50_000_000, 7)],
            )],
        );
        trace_block(&mut book, Network::Mainnet, &created).unwrap();
        let first = block_transactions(&created).unwrap().hashes()[0];

        let swapped = block(
            HEIGHT + 1,
            SLOT + 20,
            vec![tx(
                &[(first, 0), (first, 1)],
                &[pool_output(101_000_000, 9), bunny_output(52_000_000, 6)],
            )],
        );
        let events = trace_block(&mut book, Network::Mainnet, &swapped).unwrap();

        assert_eq!(events.len(), 2, "one event per pool, not one averaged pair");
        assert_eq!(
            events[0].activity,
            Activity::Flow {
                ada: 1_000_000,
                assets: vec![AssetDelta {
                    policy_id: hex::encode(POLICY),
                    name_hex: hex::encode(ALIENS),
                    delta: -1,
                }],
                utxos: 1,
            }
        );
        assert_eq!(
            events[1].activity,
            Activity::Flow {
                ada: 2_000_000,
                assets: vec![AssetDelta {
                    policy_id: hex::encode(POLICY_B),
                    name_hex: hex::encode(BUNNY),
                    delta: -1,
                }],
                utxos: 1,
            }
        );
    }

    /// An untracked pool spent alongside a tracked one must not be folded into
    /// its delta. It cannot be, and the tracked pool's answer must stay exact.
    #[test]
    fn an_untracked_pool_in_the_same_transaction_does_not_poison_the_delta() {
        let mut book = PoolBook::default();
        let created = block(HEIGHT, SLOT, vec![tx(&[], &[pool_output(100_000_000, 10)])]);
        trace_block(&mut book, Network::Mainnet, &created).unwrap();
        let first = block_transactions(&created).unwrap().hashes()[0];

        let swapped = block(
            HEIGHT + 1,
            SLOT + 20,
            vec![tx(
                &[(first, 0), ([0x99; 32], 0)],
                &[pool_output(105_000_000, 9), bunny_output(50_000_000, 7)],
            )],
        );
        let events = trace_block(&mut book, Network::Mainnet, &swapped).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].activity,
            Activity::Flow {
                ada: 5_000_000,
                assets: vec![AssetDelta {
                    policy_id: hex::encode(POLICY),
                    name_hex: hex::encode(ALIENS),
                    delta: -1,
                }],
                utxos: 1,
            }
        );
    }

    /// A pool whose asset set changed is not a pool that closed: the venue
    /// still holds a UTxO at that credential, so nothing may be claimed about
    /// the old one's disappearance.
    #[test]
    fn an_unmatched_asset_set_is_not_reported_as_closed() {
        let mut book = PoolBook::default();
        let created = block(HEIGHT, SLOT, vec![tx(&[], &[pool_output(100_000_000, 10)])]);
        trace_block(&mut book, Network::Mainnet, &created).unwrap();
        let first = block_transactions(&created).unwrap().hashes()[0];

        let changed = block(
            HEIGHT + 1,
            SLOT + 20,
            vec![tx(&[(first, 0)], &[bunny_output(100_000_000, 3)])],
        );
        assert_eq!(
            trace_block(&mut book, Network::Mainnet, &changed).unwrap(),
            vec![]
        );
    }

    #[test]
    fn a_spend_with_no_replacement_is_reported_as_closed() {
        let mut book = PoolBook::default();
        let created = block(HEIGHT, SLOT, vec![tx(&[], &[pool_output(100_000_000, 10)])]);
        trace_block(&mut book, Network::Mainnet, &created).unwrap();
        let first = block_transactions(&created).unwrap().hashes()[0];

        let closed = block(
            HEIGHT + 1,
            SLOT + 20,
            vec![tx(&[(first, 0)], &[key_output()])],
        );
        let events = trace_block(&mut book, Network::Mainnet, &closed).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].activity,
            Activity::Closed {
                ada: 100_000_000,
                assets: vec![AssetAmount {
                    policy_id: hex::encode(POLICY),
                    name_hex: hex::encode(ALIENS),
                    amount: 10,
                }],
                utxos: 1,
            }
        );
        assert!(book.is_empty(), "a spent entry is not remembered again");
    }

    #[test]
    fn a_pool_first_seen_being_spent_is_not_guessed_at() {
        let mut book = PoolBook::default();
        let key = ([0x77; 32], 0);
        let swapped = block(
            HEIGHT,
            SLOT,
            vec![tx(&[key], &[pool_output(105_000_000, 9)])],
        );

        assert_eq!(
            trace_block(&mut book, Network::Mainnet, &swapped).unwrap(),
            vec![]
        );
        assert_eq!(book.len(), 1, "the output is remembered for next time");
    }

    #[test]
    fn a_rollback_drops_only_what_it_created() {
        let mut book = PoolBook::default();
        let created = block(HEIGHT, SLOT, vec![tx(&[], &[pool_output(100_000_000, 10)])]);
        trace_block(&mut book, Network::Mainnet, &created).unwrap();
        assert_eq!(book.len(), 1);

        book.rollback_to(SLOT - 1);
        assert!(book.is_empty());
    }

    #[test]
    fn marketplaces_are_not_this_feed() {
        let mut address = vec![0x11];
        address.extend_from_slice(&hex::decode(JPG_SALE).unwrap());
        address.extend_from_slice(&[0u8; 28]);
        assert!(venue_credential(&address).is_none());

        let mut address = vec![0x11];
        address.extend_from_slice(&hex::decode(CSWAP).unwrap());
        address.extend_from_slice(&[0u8; 28]);
        assert_eq!(
            venue_credential(&address).map(|(_, venue)| venue.label),
            Some("CSWAP")
        );

        // A launchpad curve is a venue too, and keeps its own kind.
        let solada = venue_credential(&solada_address()).expect("Solada is registered");
        assert_eq!(solada.1.label, "Solada");
        assert_eq!(solada.1.kind, VenueKind::Launchpad);
    }

    /// A real mainnet block from mitos' CSWAP fixture: the parser must accept
    /// the chain's own encoding (tagged input sets, multiasset outputs).
    #[test]
    fn parses_a_real_cswap_block() {
        const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186892158.block.cbor");
        let transactions = block_transactions(BLOCK).unwrap();
        assert!(transactions.aux.is_empty(), "the fixture carries no memos");
        let mut book = PoolBook::default();
        assert_eq!(
            trace_block(&mut book, Network::Mainnet, BLOCK).unwrap(),
            vec![]
        );
        assert!(!book.is_empty(), "the block creates a venue UTxO");
    }

    /// The real parser, wired through to an event. A single block carries only
    /// the post state, so the pre state is seeded from the UTxO the first
    /// transaction spends, with a delta of exactly one unit put on it.
    #[test]
    fn a_real_block_resolves_against_a_seeded_book() {
        const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186892158.block.cbor");
        let transactions = block_transactions(BLOCK).unwrap();
        let parsed = parse_tx(transactions.bodies[0]).unwrap();
        let post = parsed
            .outputs
            .iter()
            .find(|output| output.credential.is_some())
            .expect("the transaction creates a venue UTxO");
        let (credential, venue) = post.credential.unwrap();
        let key = parsed.inputs[0];

        let mut book = PoolBook::default();
        book.insert(
            key,
            VenueUtxo {
                credential,
                venue,
                coin: post.coin + 1_000_000,
                assets: post
                    .assets
                    .iter()
                    .map(|(unit, amount)| (unit.clone(), amount + 1))
                    .collect(),
                slot: 1,
            },
        );

        let events = trace_block(&mut book, Network::Mainnet, BLOCK).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0].activity {
            Activity::Flow { ada, assets, utxos } => {
                assert_eq!(*ada, -1_000_000);
                assert_eq!(*utxos, 1);
                assert_eq!(assets.len(), post.assets.len());
                assert!(assets.iter().all(|asset| asset.delta == -1));
            }
            other => panic!("expected a flow, got {other:?}"),
        }
    }

    #[test]
    fn amounts_above_the_javascript_integer_limit_are_strings() {
        let event = DexEvent {
            venue: "Splash".to_string(),
            venue_kind: VenueKind::Exchange,
            pool: None,
            tx_hash: "00".to_string(),
            height: HEIGHT,
            slot: SLOT,
            block_time_unix: None,
            tag: None,
            activity: Activity::Flow {
                ada: 15_000_000_000_000_000,
                assets: vec![],
                utxos: 1,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""kind":"flow""#), "{json}");
        assert!(json.contains(r#""ada":"15000000000000000""#), "{json}");
        let back: DexEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn a_launch_is_reported_from_its_memo_with_no_predecessor() {
        let mut book = PoolBook::default();
        let created = block_with_aux(
            HEIGHT,
            SLOT,
            vec![tx(&[], &[solada_output(1_500_000_000, 0)])],
            vec![Some(cip20_aux("solada:launch:draft-modr"))],
        );

        let events = trace_block(&mut book, Network::Mainnet, &created).unwrap();
        assert_eq!(events.len(), 1, "the curve output is not a second event");
        let event = &events[0];
        assert_eq!(event.venue, "Solada");
        assert_eq!(event.venue_kind, VenueKind::Launchpad);
        assert_eq!(event.tag, None, "the activity already names the launch");
        assert_eq!(
            event.activity,
            Activity::Launch {
                slug: "draft-modr".to_string(),
                ada: 1_500_000_000,
                utxos: 1,
            }
        );
        assert_eq!(book.len(), 1, "the curve is remembered for its mint");
    }

    #[test]
    fn a_launch_that_never_reaches_the_curve_is_still_reported() {
        let mut book = PoolBook::default();
        let created = block_with_aux(
            HEIGHT,
            SLOT,
            vec![tx(&[], &[key_output()])],
            vec![Some(cip20_aux("solada:launch:ghost"))],
        );

        let events = trace_block(&mut book, Network::Mainnet, &created).unwrap();
        assert_eq!(events.len(), 1, "the memo is the marker, not the curve");
        assert_eq!(events[0].venue, "Solada");
        assert_eq!(events[0].venue_kind, VenueKind::Launchpad);
        assert_eq!(
            events[0].activity,
            Activity::Launch {
                slug: "ghost".to_string(),
                ada: 0,
                utxos: 0,
            }
        );
    }

    #[test]
    fn a_curve_swap_carries_the_memo_tag() {
        let mut book = PoolBook::default();
        let created = block(
            HEIGHT,
            SLOT,
            vec![tx(&[], &[solada_output(1_500_000_000, 10)])],
        );
        trace_block(&mut book, Network::Mainnet, &created).unwrap();
        let first = block_transactions(&created).unwrap().hashes()[0];

        let swapped = block_with_aux(
            HEIGHT + 1,
            SLOT + 20,
            vec![tx(&[(first, 0)], &[solada_output(1_569_000_000, 9)])],
            vec![Some(cip20_aux("solada:curveswap:51280017e3c14657"))],
        );
        let events = trace_block(&mut book, Network::Mainnet, &swapped).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].tag,
            Some(VenueTag::CurveSwap {
                id: "51280017e3c14657".to_string(),
            })
        );
        match &events[0].activity {
            Activity::Flow { ada, .. } => assert_eq!(*ada, 69_000_000),
            other => panic!("expected a flow, got {other:?}"),
        }
    }

    #[test]
    fn a_launch_mint_landing_in_the_curve_is_tagged() {
        let mut book = PoolBook::default();
        let created = block(
            HEIGHT,
            SLOT,
            vec![tx(&[], &[solada_output(1_500_000_000, 0)])],
        );
        trace_block(&mut book, Network::Mainnet, &created).unwrap();
        let first = block_transactions(&created).unwrap().hashes()[0];

        let minted = block(
            HEIGHT + 1,
            SLOT + 3,
            vec![tx_minting(
                &[(first, 0)],
                &[Output {
                    address: solada_address(),
                    coin: 1_500_000_000,
                    assets: vec![(POLICY, MODR_NAME, 1_000_000_000)],
                }],
                &[(POLICY, MODR_NAME, 1_000_000_000)],
            )],
        );
        let events = trace_block(&mut book, Network::Mainnet, &minted).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tag, None, "the activity already names the mint");
        assert_eq!(
            events[0].activity,
            Activity::LaunchMint {
                policy_id: hex::encode(POLICY),
                name_hex: hex::encode(MODR_NAME),
                supply: 1_000_000_000,
                ada: 0,
                utxos: 1,
            }
        );
    }

    #[test]
    fn the_memo_taxonomy_is_read_whole() {
        assert_eq!(
            classify_memo("solada:curveswap:abc"),
            Some(VenueTag::CurveSwap {
                id: "abc".to_string()
            })
        );
        assert_eq!(
            classify_memo("solada:pool-swap:buy:PERP"),
            Some(VenueTag::PoolSwap {
                side: PoolSide::Buy,
                ticker: "PERP".to_string()
            })
        );
        assert_eq!(
            classify_memo("solada:pool-swap:sell:SNEK"),
            Some(VenueTag::PoolSwap {
                side: PoolSide::Sell,
                ticker: "SNEK".to_string()
            })
        );
        assert_eq!(classify_memo("solada:adabuy:1"), Some(VenueTag::AdaBuy));
        assert_eq!(
            classify_memo("solada:launch:draft-modr"),
            Some(VenueTag::Launch {
                slug: "draft-modr".to_string()
            })
        );
        assert_eq!(classify_memo("something else"), None);
        // A side this crate cannot read is kept whole rather than guessed at.
        assert_eq!(
            classify_memo("solada:pool-swap:sideways:PERP"),
            Some(VenueTag::Other {
                memo: "solada:pool-swap:sideways:PERP".to_string()
            })
        );
    }

    // ── Test blocks ─────────────────────────────────────────────────────────

    struct Output {
        address: Vec<u8>,
        coin: u64,
        assets: Vec<([u8; 28], &'static [u8], u64)>,
    }

    fn pool_output(coin: u64, aliens: u64) -> Output {
        Output {
            address: venue_address(CSWAP),
            coin,
            assets: vec![(POLICY, ALIENS, aliens)],
        }
    }

    fn bunny_output(coin: u64, bunnies: u64) -> Output {
        Output {
            address: venue_address(CSWAP),
            coin,
            assets: vec![(POLICY_B, BUNNY, bunnies)],
        }
    }

    /// A payment address with a key credential: no venue, so a spend of one
    /// leaves nothing behind at the script.
    fn key_output() -> Output {
        let mut address = vec![0x01];
        address.extend_from_slice(&[0x22; 28]);
        address.extend_from_slice(&[0x33; 28]);
        Output {
            address,
            coin: 100_000_000,
            assets: vec![],
        }
    }

    fn solada_address() -> Vec<u8> {
        // Enterprise, script payment, mainnet — the curve address's own shape.
        let mut address = vec![0x71];
        address.extend_from_slice(&hex::decode(SOLADA).unwrap());
        address
    }

    fn solada_output(coin: u64, tokens: u64) -> Output {
        Output {
            address: solada_address(),
            coin,
            assets: if tokens == 0 {
                vec![]
            } else {
                vec![(POLICY, ALIENS, tokens)]
            },
        }
    }

    fn venue_address(credential_hex: &str) -> Vec<u8> {
        let mut address = vec![0x11];
        address.extend_from_slice(&hex::decode(credential_hex).unwrap());
        address.extend_from_slice(&[0u8; 28]);
        address
    }

    /// `#6.259({0: {674: {"msg": [memo]}}})` — the CIP-20 shape Solada writes.
    fn cip20_aux(memo: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = Encoder::new(&mut out);
        e.tag(minicbor::data::Tag::new(259)).unwrap();
        e.map(1).unwrap();
        e.u64(0).unwrap();
        e.map(1).unwrap();
        e.u64(674).unwrap();
        e.map(1).unwrap();
        e.str("msg").unwrap();
        e.array(1).unwrap();
        e.str(memo).unwrap();
        out
    }

    fn block(height: u64, slot: u64, txs: Vec<Vec<u8>>) -> Vec<u8> {
        let aux = vec![None; txs.len()];
        block_with_aux(height, slot, txs, aux)
    }

    fn block_with_aux(
        height: u64,
        slot: u64,
        txs: Vec<Vec<u8>>,
        aux: Vec<Option<Vec<u8>>>,
    ) -> Vec<u8> {
        let header = header(height, slot);
        let mut out = Vec::new();
        let mut e = Encoder::new(&mut out);
        e.array(2).unwrap();
        e.u8(7).unwrap(); // Conway
        e.array(5).unwrap();
        e.writer_mut().extend_from_slice(&header);
        e.array(txs.len() as u64).unwrap();
        for tx in &txs {
            e.writer_mut().extend_from_slice(tx);
        }
        e.array(0).unwrap(); // witness sets
        let present = aux.iter().filter(|item| item.is_some()).count();
        e.map(present as u64).unwrap();
        for (index, item) in aux.iter().enumerate() {
            if let Some(bytes) = item {
                e.u32(index as u32).unwrap();
                e.writer_mut().extend_from_slice(bytes);
            }
        }
        e.array(0).unwrap(); // invalid transactions
        out
    }

    fn header(height: u64, slot: u64) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = Encoder::new(&mut out);
        e.array(2).unwrap();
        e.array(8).unwrap();
        e.u64(height).unwrap();
        e.u64(slot).unwrap();
        e.null().unwrap();
        e.bytes(&[7; 32]).unwrap();
        e.bytes(&[8; 32]).unwrap();
        e.array(2).unwrap();
        e.bytes(&[9; 64]).unwrap();
        e.bytes(&[10; 80]).unwrap();
        e.u32(0).unwrap();
        e.bytes(&[11; 32]).unwrap();
        e.bytes(&[12; 64]).unwrap();
        out
    }

    fn tx(inputs: &[([u8; 32], u32)], outputs: &[Output]) -> Vec<u8> {
        tx_minting(inputs, outputs, &[])
    }

    fn tx_minting(
        inputs: &[([u8; 32], u32)],
        outputs: &[Output],
        mints: &[([u8; 28], &[u8], i64)],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = Encoder::new(&mut out);
        e.map(if mints.is_empty() { 3 } else { 4 }).unwrap();
        e.u32(0).unwrap();
        e.array(inputs.len() as u64).unwrap();
        for (hash, index) in inputs {
            e.array(2).unwrap();
            e.bytes(hash).unwrap();
            e.u32(*index).unwrap();
        }
        e.u32(1).unwrap();
        e.array(outputs.len() as u64).unwrap();
        for output in outputs {
            e.array(2).unwrap();
            e.bytes(&output.address).unwrap();
            e.array(2).unwrap();
            e.u64(output.coin).unwrap();
            e.map(output.assets.len() as u64).unwrap();
            for (policy, name, amount) in &output.assets {
                e.bytes(policy).unwrap();
                e.map(1).unwrap();
                e.bytes(name).unwrap();
                e.u64(*amount).unwrap();
            }
        }
        if let Some((policy, _, _)) = mints.first() {
            e.u32(9).unwrap();
            e.map(1).unwrap();
            e.bytes(policy).unwrap();
            e.map(mints.len() as u64).unwrap();
            for (_, name, quantity) in mints {
                e.bytes(name).unwrap();
                e.i64(*quantity).unwrap();
            }
        }
        e.u32(2).unwrap();
        e.u64(200_000).unwrap();
        out
    }
}
