//! Convert pallas-decoded transactions into the canonical pipeline
//! [`RawTxData`] shape.
//!
//! Used by chain-following indexers (e.g. mitos) that receive
//! blocks straight from a Cardano data node, in `pallas` types,
//! rather than via Maestro/Blockfrost JSON. The same downstream
//! pattern detection code (`tx-classifier`'s `RuleEngine`,
//! `MarketplaceDatumParser`, etc.) works on either source.
//!
//! The conversion is a *partial* one: a `MultiEraTx` carries its
//! own outputs, mints, fee, metadata, etc. but inputs are present
//! only as references (`TxoRef`) — the consumed UTxO's address,
//! value, and datum live somewhere outside the tx. The caller
//! resolves those (e.g. via a state store or archive) and passes
//! them in.
//!
//! Entry point: [`raw_tx_data_from_pallas`].
//!
//! Per-piece helpers ([`tx_output_from_pallas`],
//! [`tx_input_from_pallas`], [`tx_datum_from_pallas_inline`],
//! [`mint_operations_from_pallas`]) are also public so callers can
//! compose differently — e.g. a streaming indexer that only cares
//! about outputs and skips input resolution entirely.

use std::collections::HashMap;
use std::fmt;

use pallas_crypto::hash::Hasher;
use pallas_primitives::Hash;
use pallas_traverse::{MultiEraOutput, MultiEraTx};

use crate::{MintOperation, RawTxData, TxAsset, TxDatum, TxInput, TxOutput};

/// Input reference: `(prev_tx_hash, output_index)`. Same
/// information as a `pallas::traverse::OutputRef` but stored as
/// owned bytes + u32 so callers can use it as a HashMap key
/// without lifetime gymnastics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OutputRef {
    pub tx_hash: [u8; 32],
    pub index: u32,
}

impl OutputRef {
    pub fn new(tx_hash: [u8; 32], index: u32) -> Self {
        Self { tx_hash, index }
    }

    /// Hex form of `tx_hash`.
    pub fn tx_hash_hex(&self) -> String {
        hex::encode(self.tx_hash)
    }
}

impl fmt::Display for OutputRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.tx_hash_hex(), self.index)
    }
}

/// Errors the adapter can produce. All other shape mismatches —
/// invalid addresses, malformed datum structures — are tolerated
/// silently: the resulting `RawTxData` field is left empty/None
/// and downstream pattern detection handles missing fields the
/// same way it would for a malformed Maestro response.
#[derive(Debug, Clone, PartialEq)]
pub enum PallasAdapterError {
    /// An input referenced by `tx.consumes()` had no entry in the
    /// `resolved_inputs` map. The classifier needs the consumed
    /// UTxO's address + value to identify marketplace-contract
    /// interactions; missing it would silently produce a wrong
    /// classification, so we surface as an error rather than emit
    /// a partial `RawTxData`.
    UnresolvedInput(OutputRef),
}

impl fmt::Display for PallasAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedInput(r) => {
                write!(f, "input not present in resolved_inputs map: {r}")
            }
        }
    }
}

impl std::error::Error for PallasAdapterError {}

/// Produce a `TxOutput` from a pallas `MultiEraOutput`. Address is
/// best-effort bech32; if it fails to render we fall back to an
/// empty string (matches the upstream `RawTxData` shape, which has
/// always been resilient to malformed addresses).
pub fn tx_output_from_pallas(out: &MultiEraOutput<'_>) -> TxOutput {
    let address = out
        .address()
        .map(|a| a.to_string())
        .unwrap_or_default();
    let amount_lovelace = out.value().coin();
    let assets = collect_assets(out);
    let datum = tx_datum_from_pallas_output(out);
    let script_ref = out
        .script_ref()
        .map(|_| String::from("present"))
        // RawTxData stores `script_ref: Option<String>` but the
        // Maestro pipeline today populates it with the script
        // hash, not the body. We mirror that — see comment in
        // `script_ref_hex` if a richer representation is needed.
        .or_else(|| script_ref_hex(out));
    TxOutput {
        address,
        amount_lovelace,
        assets,
        datum,
        script_ref,
    }
}

/// Produce a `TxInput` from a `(prev_tx_hash, output_index,
/// resolved_output)` triple. The resolved output is the
/// previously-produced UTxO that this input is consuming.
pub fn tx_input_from_pallas(
    prev_tx_hash: Hash<32>,
    output_index: u32,
    resolved: &MultiEraOutput<'_>,
) -> TxInput {
    let out = tx_output_from_pallas(resolved);
    TxInput {
        address: out.address,
        tx_hash: hex::encode(prev_tx_hash),
        output_index,
        amount_lovelace: out.amount_lovelace,
        assets: out.assets,
        datum: out.datum,
    }
}

/// Build a `TxDatum::Bytes` from a pallas inline datum's CBOR
/// bytes, computing the Blake2b-256 hash on the fly. The hash is
/// what scripts reference; storing both keeps the pipeline's
/// hash-or-bytes lookup uniform.
pub fn tx_datum_from_pallas_inline(cbor: &[u8]) -> TxDatum {
    let hash = Hasher::<256>::hash(cbor);
    TxDatum::Bytes {
        hash: hex::encode(hash),
        bytes: hex::encode(cbor),
    }
}

/// Build a `TxDatum::Hash` from a 32-byte datum hash that wasn't
/// inlined.
pub fn tx_datum_from_pallas_hash(hash: Hash<32>) -> TxDatum {
    TxDatum::Hash {
        hash: hex::encode(hash),
    }
}

/// Mint/burn operations from a pallas tx. Multi-asset mints in a
/// single tx (rare but valid) all flow into the returned vector.
pub fn mint_operations_from_pallas(tx: &MultiEraTx<'_>) -> Vec<MintOperation> {
    let mut out = Vec::new();
    for policy_assets in tx.mints() {
        let policy_hex = hex::encode(policy_assets.policy());
        for asset in policy_assets.assets() {
            let unit = format!("{policy_hex}{}", hex::encode(asset.name()));
            let amount = asset.mint_coin().unwrap_or(0);
            out.push(MintOperation { unit, amount });
        }
    }
    out
}

/// Convert a pallas tx + already-resolved inputs into the canonical
/// pipeline `RawTxData`. The caller is responsible for resolving
/// each `tx.consumes()` reference to its `MultiEraOutput` (e.g.
/// via dolos's state store, archive index, or a Maestro batch
/// lookup).
///
/// `block_slot` is filled into `RawTxData.block_height` (the
/// existing pipeline conflates "slot" and "block height" for
/// historical reasons; we keep that for compatibility).
///
/// Returns `Err(UnresolvedInput)` if any consumed input is missing
/// from `resolved_inputs` — surfacing rather than silently emitting
/// a `RawTxData` with incomplete inputs (which would mis-classify
/// downstream).
pub fn raw_tx_data_from_pallas<'tx, 'inputs>(
    tx: &MultiEraTx<'tx>,
    resolved_inputs: &HashMap<OutputRef, MultiEraOutput<'inputs>>,
    block_slot: Option<u64>,
) -> Result<RawTxData, PallasAdapterError> {
    let tx_hash = hex::encode(tx.hash());

    // Inputs: each consumed reference must resolve.
    let mut inputs = Vec::new();
    for input in tx.consumes() {
        let key = OutputRef::new(**input.hash(), input.index() as u32);
        let resolved = resolved_inputs
            .get(&key)
            .ok_or_else(|| PallasAdapterError::UnresolvedInput(key.clone()))?;
        inputs.push(tx_input_from_pallas(
            *input.hash(),
            input.index() as u32,
            resolved,
        ));
    }

    // Reference inputs (read-only). Same resolution requirement —
    // their datums often carry marketplace pricing logic, so missing
    // them is just as bad as a missing consumed input.
    let mut reference_inputs = Vec::new();
    for input in tx.reference_inputs() {
        let key = OutputRef::new(**input.hash(), input.index() as u32);
        if let Some(resolved) = resolved_inputs.get(&key) {
            reference_inputs.push(tx_input_from_pallas(
                *input.hash(),
                input.index() as u32,
                resolved,
            ));
        }
        // Reference inputs that aren't in the resolver are silently
        // skipped — the caller might intentionally omit them when
        // they're known to be irrelevant (e.g. reference scripts).
    }

    // Outputs: produced UTxOs.
    let outputs: Vec<TxOutput> = tx
        .produces()
        .into_iter()
        .map(|(_idx, out)| tx_output_from_pallas(&out))
        .collect();

    // Collateral inputs: best-effort. Same resolution model as
    // primary inputs; missing entries are skipped (collateral is
    // only consumed on script failure, not classification-relevant
    // in the steady state).
    let mut collateral_inputs = Vec::new();
    for input in tx.collateral() {
        let key = OutputRef::new(**input.hash(), input.index() as u32);
        if let Some(resolved) = resolved_inputs.get(&key) {
            collateral_inputs.push(tx_input_from_pallas(
                *input.hash(),
                input.index() as u32,
                resolved,
            ));
        }
    }

    // Collateral return: a single output, no resolution needed.
    let collateral_outputs: Vec<TxOutput> = tx
        .collateral_return()
        .as_ref()
        .map(|out| vec![tx_output_from_pallas(out)])
        .unwrap_or_default();

    let mint = mint_operations_from_pallas(tx);

    Ok(RawTxData {
        tx_hash,
        inputs,
        outputs,
        collateral_inputs,
        collateral_outputs,
        reference_inputs,
        mint,
        metadata: None, // TODO: pallas metadata → serde_json::Value mapping
        fee: tx.fee(),
        block_height: block_slot,
        timestamp: None,
        size: None,
        scripts: Vec::new(), // TODO: tx.scripts() → hex-encoded entries
        redeemers: None,
    })
}

// ---- internal helpers ----

fn collect_assets(out: &MultiEraOutput<'_>) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for policy_assets in out.value().assets() {
        let policy_hex = hex::encode(policy_assets.policy());
        for asset in policy_assets.assets() {
            let unit = format!("{policy_hex}{}", hex::encode(asset.name()));
            let qty = asset.any_coin() as u64;
            map.insert(unit, qty);
        }
    }
    map
}

fn tx_datum_from_pallas_output(out: &MultiEraOutput<'_>) -> Option<TxDatum> {
    use pallas_primitives::babbage::DatumOption;
    match out.datum()? {
        DatumOption::Hash(h) => Some(tx_datum_from_pallas_hash(h)),
        DatumOption::Data(cbor_wrap) => {
            // KeepRaw preserves the original CBOR bytes; that's
            // what marketplace decoders parse, so emit Bytes
            // (which auto-computes the hash).
            let raw_cbor = cbor_wrap.0.raw_cbor();
            Some(tx_datum_from_pallas_inline(raw_cbor))
        }
    }
}

fn script_ref_hex(_out: &MultiEraOutput<'_>) -> Option<String> {
    // The current Maestro-fed pipeline populates `scripts` (the
    // top-level `RawTxData.scripts: Vec<String>`) rather than the
    // per-output `script_ref`. We mirror that: emit `None` here
    // and leave `RawTxData.scripts` for callers to fill if they
    // need it. A future iteration could traverse `out.script_ref()`
    // and serialise to the same wire-shape (hex-encoded
    // PlutusV1/V2/V3 or NativeScript CBOR) but it's not on the
    // marketplace-decode path.
    None
}

// ---- TxAsset helper for callers that prefer the typed shape ----

/// Convert the `assets: HashMap<String, u64>` representation used
/// inside `TxInput`/`TxOutput` into the typed `TxAsset` shape some
/// downstream code prefers. Provided here so callers don't roll
/// their own.
pub fn assets_to_typed(assets: &HashMap<String, u64>) -> Vec<TxAsset> {
    assets
        .iter()
        .map(|(id, qty)| TxAsset {
            id: id.clone(),
            qty: *qty,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_ref_display() {
        let r = OutputRef::new([0xab; 32], 0);
        assert!(r.to_string().starts_with("ab"));
        assert!(r.to_string().ends_with("#0"));
    }

    #[test]
    fn datum_from_inline_bytes_computes_hash() {
        // Tiny synthetic CBOR — empty-list constructor.
        let cbor = vec![0x80];
        let datum = tx_datum_from_pallas_inline(&cbor);
        match datum {
            TxDatum::Bytes { hash, bytes } => {
                assert_eq!(bytes, "80");
                assert_eq!(hash.len(), 64); // 32-byte hash hex-encoded
                // The Blake2b-256 of `[0x80]` is deterministic;
                // this verifies we're hashing the bytes we got,
                // not something derived.
                let recomputed =
                    pallas_crypto::hash::Hasher::<256>::hash(&[0x80]);
                assert_eq!(hash, hex::encode(recomputed));
            }
            other => panic!("expected Bytes, got {other:?}"),
        }
    }

    #[test]
    fn datum_from_hash_only() {
        let h = Hash::<32>::from([0x42u8; 32]);
        let datum = tx_datum_from_pallas_hash(h);
        match datum {
            TxDatum::Hash { hash } => {
                assert_eq!(hash, "42".repeat(32));
            }
            other => panic!("expected Hash, got {other:?}"),
        }
    }

    #[test]
    fn unresolved_input_error_displays() {
        let e = PallasAdapterError::UnresolvedInput(OutputRef::new([0u8; 32], 7));
        let s = e.to_string();
        assert!(s.contains("not present"));
        assert!(s.contains("#7"));
    }

    // Full block round-trip tests (taking real CBOR bytes and
    // diffing the resulting RawTxData against expected) live in
    // mitos's integration tests rather than here — they need a
    // dolos data dir to resolve inputs from, which isn't a
    // dependency this crate should pull in.
}
