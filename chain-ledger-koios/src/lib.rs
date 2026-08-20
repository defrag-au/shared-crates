//! Koios → [`chain_ledger`] adapter.
//!
//! The seam between "what a provider happens to return" and the normalised
//! model. Keeping it in one place is what lets analysis code stop caring whether
//! a row came from Koios, Maestro or a mitos module — and it is where the
//! Cardano-specific identity rule lives.
//!
//! # Scope
//!
//! **Format conversion only.** No HTTP, no async, no caching: the caller fetches
//! however it likes and hands rows in. That is what lets one adapter serve a
//! blocking desktop tool, a Cloudflare worker, and a test fixture — the moment
//! it owned a client it would have to pick one of those worlds.
//!
//! # Input shape
//!
//! Rows are `tx_info` objects as Koios returns them (`_inputs=true`). The two
//! fields that matter are each output's `stake_addr` / `payment_addr.bech32`
//! pair, and each input's `tx_hash` — the UTxO it consumes, which is the edge a
//! provenance walk follows.

use chain_ledger::{Chain, Party, TxInput, TxOutput, TxView};
use serde_json::Value;
use std::collections::HashMap;

/// Lovelace from a Koios `value` field, which may be a string or a number.
pub fn lovelace(v: &Value) -> i128 {
    v.as_str()
        .and_then(|s| s.parse::<i128>().ok())
        .or_else(|| v.as_i64().map(i128::from))
        .unwrap_or(0)
}

/// Resolve a Koios input/output object to a [`Party`].
///
/// The stake key when there is one, else the payment address — and the
/// distinction is preserved rather than flattened. Koios omits `stake_addr`
/// entirely for enterprise addresses, so a resolver that falls back to the
/// payment address *and forgets it did so* reports every off-ramp leg as an
/// ordinary staking wallet. That is not hypothetical: it is why an earlier run
/// classified nothing as stakeless and quietly filed the whole off-ramp under
/// "unknown".
pub fn party(o: &Value) -> Option<Party> {
    if let Some(stake) = o["stake_addr"].as_str() {
        return Some(Party::cardano_stake(stake));
    }
    o["payment_addr"]["bech32"]
        .as_str()
        .map(Party::cardano_enterprise)
}

/// Convert a Koios `tx_info` row into a [`TxView`].
///
/// Returns `None` when the row carries no resolvable parties at all.
pub fn tx_view(row: &Value) -> Option<TxView> {
    let tx_id = row["tx_hash"].as_str()?.to_string();
    let timestamp = row["tx_timestamp"]
        .as_i64()
        .or_else(|| row["block_time"].as_i64())
        .unwrap_or(0);

    let mut parties: Vec<Party> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut intern = |p: Party, parties: &mut Vec<Party>| -> usize {
        *index.entry(p.key.clone()).or_insert_with(|| {
            parties.push(p);
            parties.len() - 1
        })
    };

    let mut inputs = Vec::new();
    for i in row["inputs"].as_array().into_iter().flatten() {
        let Some(p) = party(i) else { continue };
        let r = intern(p, &mut parties);
        inputs.push(TxInput {
            party: r,
            value: lovelace(&i["value"]),
            // The UTxO this input consumes — the edge a provenance walk follows.
            source: i["tx_hash"].as_str().map(String::from),
        });
    }

    let mut outputs = Vec::new();
    for o in row["outputs"].as_array().into_iter().flatten() {
        let Some(p) = party(o) else { continue };
        let r = intern(p, &mut parties);
        outputs.push(TxOutput {
            party: r,
            value: lovelace(&o["value"]),
        });
    }

    (!parties.is_empty()).then_some(TxView {
        chain: Chain::Cardano,
        tx_id,
        timestamp,
        parties,
        inputs,
        outputs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Koios omits `stake_addr` on enterprise addresses; the adapter must
    /// record that as stakeless rather than presenting the payment address as
    /// if it were a staking credential.
    #[test]
    fn enterprise_address_resolves_as_stakeless() {
        let o: Value = serde_json::from_str(
            r#"{"payment_addr":{"bech32":"addr1v9znf3q0reu7e"},"value":"1000000"}"#,
        )
        .unwrap();
        let p = party(&o).expect("resolvable");
        assert!(p.is_stakeless());
        assert_eq!(p.key, "addr1v9znf3q0reu7e");
    }

    #[test]
    fn stake_address_wins_when_present() {
        let o: Value = serde_json::from_str(
            r#"{"stake_addr":"stake1u9yf","payment_addr":{"bech32":"addr1q9"},"value":"5"}"#,
        )
        .unwrap();
        let p = party(&o).expect("resolvable");
        assert!(!p.is_stakeless());
        assert_eq!(p.key, "stake1u9yf");
    }

    /// A party appearing on both sides must intern to ONE ref, or its inputs
    /// and outputs will not cancel and its net will be wrong.
    #[test]
    fn a_party_on_both_sides_interns_once() {
        let row: Value = serde_json::from_str(
            r#"{
              "tx_hash":"t","tx_timestamp":10,
              "inputs":[{"stake_addr":"A","value":"4000","tx_hash":"prev"}],
              "outputs":[{"stake_addr":"B","value":"400"},{"stake_addr":"A","value":"3600"}]
            }"#,
        )
        .unwrap();
        let v = tx_view(&row).expect("view");
        assert_eq!(v.parties.len(), 2);
        let deltas = chain_ledger::net_deltas(&v);
        let a = deltas
            .iter()
            .find(|d| d.party.key == "A")
            .expect("A present");
        assert_eq!(a.delta, -400, "change must not read as a 4000 payment");
        assert_eq!(v.inputs[0].source.as_deref(), Some("prev"));
    }

    #[test]
    fn string_and_numeric_lovelace_both_parse() {
        assert_eq!(lovelace(&Value::from("1234")), 1234);
        assert_eq!(lovelace(&Value::from(1234)), 1234);
        assert_eq!(lovelace(&Value::Null), 0);
    }
}
