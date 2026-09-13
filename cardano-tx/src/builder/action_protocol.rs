//! The action protocol's own transactions — starting with the one UTxO every
//! other one depends on.
//!
//! The `ProtocolConfig` UTxO is the root of trust for pricing and for who may
//! debit a tank. `fuel.ak` reads it for the currency rate on a burn-funded
//! top-up and for the spender list and daily cap on a reconciliation;
//! `escrow.ak` reads it for the cost table and the sink list. Every one of them
//! finds it by its **one-shot token**, never by its address — anyone may park a
//! datum-bearing output at the config address, and a reader that trusted the
//! address would believe an invented currency table.
//!
//! So there are exactly two transactions here, and they are a pair:
//!
//! - [`mint_config`] creates it, once, ever. The one-shot policy consumes a
//!   named UTxO, so no second config can exist.
//! - [`update_config`] spends and recreates it. Everything that can change
//!   without redeploying a validator lives in this datum — prices, currencies,
//!   sinks, the fee destination, the hot spender keys, and the cold updater
//!   list itself.
//!
//! **Both return an unsigned transaction.** Nothing here holds a key. The same
//! builder serves a CLI with a dev key on preprod and a hardware wallet
//! through CIP-30 on mainnet, which is the point: the signing path you rehearse
//! is the signing path you use.

use cardano_assets::UtxoApi;
use pallas_addresses::{Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart};
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{Output, ScriptKind};

use super::fluent::TxBuilder;
use super::script::{
    constr0_empty, CollateralConfig, MintEntry, ScriptInput, ScriptSource,
};
use super::{TxDeps, UnsignedTx};
use crate::error::TxBuildError;
use crate::evaluate::TxEvaluator;

/// What the config UTxO is, in the terms a builder needs.
///
/// The datum arrives as **CBOR the caller already encoded**, not as a typed
/// body. `action-definitions` owns that encoding and is the only thing that
/// should ever produce it; re-deriving it here would be a second writer of the
/// same format, which is precisely the drift `lib/payment.ak` exists to prevent
/// on the contract side.
pub struct ConfigPlacement {
    /// `protocol_config.ak`'s hash, applied. The config lives at this address.
    pub validator_hash: Hash<28>,
    /// The one-shot policy's hash, applied — also the marker token's policy id.
    pub token_policy: Hash<28>,
    /// The marker token's asset name.
    pub token_name: Vec<u8>,
    /// `ProtocolConfigBody`, already encoded.
    pub datum_cbor: Vec<u8>,
}

impl ConfigPlacement {
    /// The address the config sits at: the validator's, with no stake part.
    ///
    /// Enterprise on purpose. A stake credential would make the address a
    /// deployment decision that every validator compiled against
    /// `protocol_config`'s hash would have to agree on, and the rewards on one
    /// UTxO's min-ADA are not worth a second thing to get wrong.
    pub fn address(&self, network: Network) -> Address {
        Address::Shelley(ShelleyAddress::new(
            network,
            ShelleyPaymentPart::Script(self.validator_hash),
            ShelleyDelegationPart::Null,
        ))
    }
}

/// Create the config UTxO, once, ever.
///
/// `seed` must be **unspent at submission**, and it is what makes the policy
/// one-shot: a UTxO is consumed exactly once in the history of the chain, so
/// after this transaction no transaction anywhere can satisfy the policy again.
/// It is forced into the inputs rather than left to coin selection — selection
/// picking it by luck is not a guarantee.
///
/// The `config_token_script` is supplied **inline**. At ~300 bytes it is
/// cheaper inline than the ~4 ₳ a reference UTxO would lock for a script that
/// runs exactly once in the protocol's lifetime.
#[allow(clippy::too_many_arguments)]
pub async fn mint_config<E>(
    deps: TxDeps,
    network: Network,
    placement: &ConfigPlacement,
    seed: &UtxoApi,
    config_token_script: Vec<u8>,
    config_min_lovelace: u64,
    evaluator: &E,
) -> Result<UnsignedTx, TxBuildError>
where
    E: TxEvaluator + ?Sized,
{
    let config_output = Output::new(placement.address(network), config_min_lovelace)
        .add_asset(placement.token_policy, placement.token_name.clone(), 1)
        .map_err(|e| TxBuildError::BuildFailed(format!("marker token into the config: {e}")))?
        .set_inline_datum(placement.datum_cbor.clone());

    // The policy takes no redeemer of its own — it reads the transaction and
    // nothing else — so `Constr 0 []` is the whole of it.
    let mint = MintEntry {
        policy: placement.token_policy,
        assets: vec![(placement.token_name.clone(), 1)],
        script: ScriptSource::Inline {
            language: ScriptKind::PlutusV3,
            bytes: config_token_script,
        },
        redeemer_cbor: super::script::encode_plutus_data(&constr0_empty())?,
        ex_units: MINT_EX_UNITS_ESTIMATE,
    };

    TxBuilder::new(deps)
        .input(seed)?
        .mint(mint)
        .output(config_output)
        .with_collateral(CollateralConfig::Auto)
        .build_evaluated(evaluator)
        .await
}

/// Rewrite the config.
///
/// `current` is the config UTxO being spent, found by its marker token. The
/// validator checks the quorum against the **old** datum, so this transaction
/// is authorised by the outgoing updaters — which is what makes rotation safe
/// and a one-step takeover impossible. It also refuses an update that would
/// leave the config unmaintainable, so a threshold above the updater count
/// fails here rather than bricking the UTxO.
///
/// The signers are declared with [`TxBuilder::with_signer`] so they land in
/// `required_signers` and the validator can see them. **A wallet signature
/// alone is not enough** — `extra_signatories` is what a validator reads, and a
/// transaction merely signed by a key that does not appear there proves nothing
/// on chain.
#[allow(clippy::too_many_arguments)]
pub async fn update_config<E>(
    deps: TxDeps,
    network: Network,
    placement: &ConfigPlacement,
    current: &UtxoApi,
    config_validator_script: ScriptSource,
    updater_signers: &[Hash<28>],
    config_min_lovelace: u64,
    evaluator: &E,
) -> Result<UnsignedTx, TxBuildError>
where
    E: TxEvaluator + ?Sized,
{
    if updater_signers.is_empty() {
        return Err(TxBuildError::BuildFailed(
            "an update needs at least one updater signature — the validator reads \
             `extra_signatories`, and a transaction with none authorises nobody"
                .to_string(),
        ));
    }

    let continuing = Output::new(placement.address(network), config_min_lovelace)
        .add_asset(placement.token_policy, placement.token_name.clone(), 1)
        .map_err(|e| TxBuildError::BuildFailed(format!("marker token onward: {e}")))?
        .set_inline_datum(placement.datum_cbor.clone());

    let mut builder = TxBuilder::new(deps)
        .spend_script_utxo(
            current,
            ScriptInput {
                script: config_validator_script,
                // The config carries an INLINE datum, so the spend supplies no
                // datum witness — the validator reads it off the input.
                datum_cbor: None,
                // `Update` is the validator's only redeemer.
                redeemer_cbor: super::script::encode_plutus_data(&constr0_empty())?,
                ex_units: SPEND_EX_UNITS_ESTIMATE,
            },
        )?
        .output(continuing)
        .with_collateral(CollateralConfig::Auto);

    for signer in updater_signers {
        builder = builder.with_signer(*signer);
    }

    builder.build_evaluated(evaluator).await
}

/// First-pass ExUnits for the one-shot mint, replaced by the evaluator's real
/// figures before the fee is settled. Only the first pass uses them, so being
/// generous costs nothing and being stingy costs a rebuild.
const MINT_EX_UNITS_ESTIMATE: pallas_txbuilder::ExUnits = pallas_txbuilder::ExUnits {
    mem: 500_000,
    steps: 200_000_000,
};

/// As above, for the config spend. `protocol_config.ak` measured at ~91 M CPU
/// and ~305 K memory in its own tests, which include building fixtures a real
/// spend never pays for — so this is an upper bound with room.
const SPEND_EX_UNITS_ESTIMATE: pallas_txbuilder::ExUnits = pallas_txbuilder::ExUnits {
    mem: 700_000,
    steps: 300_000_000,
};

/// The inputs a config transaction consumes, named so a caller cannot pass
/// them in the wrong order.
///
/// Both are looked up the same way — by the marker token — but they mean
/// opposite things, and swapping them would spend the config to mint a second
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigInput {
    /// The UTxO the one-shot policy consumes. Any UTxO; it is destroyed.
    Seed,
    /// The existing config UTxO, carrying the marker token.
    Current,
}

/// Find the config UTxO in a set, by its marker token.
///
/// **Never by address.** Anyone may create an output at the config address; the
/// token is what makes one of them the config, which is the same rule every
/// validator follows.
pub fn find_config<'a>(
    utxos: &'a [UtxoApi],
    placement: &ConfigPlacement,
) -> Option<&'a UtxoApi> {
    let policy = hex::encode(placement.token_policy);
    let name = hex::encode(&placement.token_name);
    utxos.iter().find(|u| {
        u.assets.iter().any(|a| {
            a.asset_id.policy_id().eq_ignore_ascii_case(&policy)
                && a.asset_id.asset_name_hex().eq_ignore_ascii_case(&name)
                && a.quantity == 1
        })
    })
}

/// The input a `mint_config` must not touch.
///
/// The deployment that parks the reference scripts spends from the same wallet,
/// and its coin selection has no idea the seed is special — so a caller that
/// deploys before minting has to exclude it. Surfaced as a function rather than
/// a comment because "the seed was eaten" produces a policy that can never mint
/// and a chain of six hashes that can never be used.
pub fn seed_conflicts(candidate: &UtxoApi, seed: &UtxoApi) -> bool {
    candidate.tx_hash.eq_ignore_ascii_case(&seed.tx_hash)
        && candidate.output_index == seed.output_index
}

#[cfg(test)]
mod tests {
    use super::*;
    use cardano_assets::{AssetId, AssetQuantity};

    const POLICY: &str = "8c3504805a19be0dbaa80c514a46e2930e0f11700cfdf6f999cf3b37";
    const NAME: &str = "636f6e666967";

    fn placement() -> ConfigPlacement {
        ConfigPlacement {
            validator_hash: Hash::from(<[u8; 28]>::try_from(hex::decode(POLICY).unwrap().as_slice()).unwrap()),
            token_policy: Hash::from(<[u8; 28]>::try_from(hex::decode(POLICY).unwrap().as_slice()).unwrap()),
            token_name: hex::decode(NAME).unwrap(),
            datum_cbor: vec![0xa0],
        }
    }

    fn utxo(tx: &str, index: u32, assets: &[(&str, &str, u64)]) -> UtxoApi {
        UtxoApi {
            tx_hash: tx.to_string(),
            output_index: index,
            lovelace: 5_000_000,
            assets: assets
                .iter()
                .map(|(p, n, q)| AssetQuantity {
                    asset_id: AssetId::new_unchecked(p.to_string(), n.to_string()),
                    quantity: *q,
                })
                .collect(),
            tags: Vec::new(),
        }
    }

    /// **The config is found by its TOKEN, never by its address.** Anyone may
    /// park an output at the config address; only one output in existence
    /// carries the one-shot marker.
    #[test]
    fn the_config_is_found_by_its_marker_token() {
        let utxos = vec![
            utxo("aa", 0, &[]),
            utxo("bb", 1, &[("deadbeef", "00", 1)]),
            utxo("cc", 2, &[(POLICY, NAME, 1)]),
        ];
        let found = find_config(&utxos, &placement()).expect("the marked one");
        assert_eq!(found.tx_hash, "cc");
    }

    /// A pure-ADA output at the config address is not the config, and neither
    /// is one carrying somebody else's token.
    #[test]
    fn an_unmarked_utxo_is_not_the_config() {
        let utxos = vec![utxo("aa", 0, &[]), utxo("bb", 1, &[("deadbeef", "00", 1)])];
        assert!(find_config(&utxos, &placement()).is_none());
    }

    /// The marker is a one-shot NFT. A quantity that is not exactly one is not
    /// it, whatever its name — if that ever appears, something has gone wrong
    /// enough that guessing is the wrong move.
    #[test]
    fn a_quantity_other_than_one_is_not_the_marker() {
        let utxos = vec![utxo("cc", 2, &[(POLICY, NAME, 2)])];
        assert!(find_config(&utxos, &placement()).is_none());
    }

    /// The same token under a different policy is a look-alike.
    #[test]
    fn the_same_name_under_another_policy_is_not_the_config() {
        let other = "ff".repeat(28);
        let utxos = vec![utxo("cc", 2, &[(&other, NAME, 1)])];
        assert!(find_config(&utxos, &placement()).is_none());
    }

    /// The seed is matched on `(hash, index)` — an index alone collides across
    /// every transaction, and a hash alone across every output of one.
    #[test]
    fn a_seed_conflict_needs_both_the_hash_and_the_index() {
        let seed = utxo("AbCd", 3, &[]);
        assert!(seed_conflicts(&utxo("abcd", 3, &[]), &seed), "case differs");
        assert!(!seed_conflicts(&utxo("abcd", 4, &[]), &seed));
        assert!(!seed_conflicts(&utxo("bbbb", 3, &[]), &seed));
    }

    /// The config sits at the validator's own address, with no stake part.
    #[test]
    fn the_config_address_is_the_validators_enterprise_address() {
        let address = placement().address(Network::Testnet);
        let Address::Shelley(shelley) = address else {
            panic!("expected a shelley address");
        };
        assert!(matches!(
            shelley.payment(),
            ShelleyPaymentPart::Script(_)
        ));
        assert!(matches!(shelley.delegation(), ShelleyDelegationPart::Null));
    }
}
