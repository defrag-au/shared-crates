//! The tank and the protocol config — schema §2.8, protocol §1.2 and §1.5.

use serde::{Deserialize, Serialize};

use crate::codec::{Bytes, UnknownFields};
use crate::types::grant::EffectKind;
use crate::types::scalars::{PaymentKeyHash, PolicyId};
use action_definitions_derive::PlutusCodec;

/// The subscriber's credit balance — our body, carried in a **real CIP-68
/// datum's `extra`** so wallets and explorers still render the user token as
/// a named subscription with an image.
///
/// The fuel validator decodes this and nothing else; `metadata` stays opaque
/// bytes it compares unchanged across every continuing output.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct FuelBody {
    /// Credits. **Only ever created by a fee payment or a burn, and only
    /// ever consumed by an authorized spender's `Reconcile`** — no key can
    /// conjure it.
    #[plutus(id = 0, default)]
    #[serde(default)]
    pub balance: u64,
    /// When the last `Reconcile` happened — **POSIX milliseconds, not a
    /// slot** — which is what the daily cap is measured against.
    ///
    /// It has to be milliseconds because `fuel.ak` is what writes it, and a
    /// Plutus validator cannot see slots at all: its only clock is
    /// `Transaction.validity_range`, which the ledger hands over as POSIX
    /// time in milliseconds. A field named `_slot` holding milliseconds is
    /// the kind of thing that reads fine for a year and then produces a
    /// 1970-vs-now comparison, so it is named for what it holds.
    #[plutus(id = 1, default)]
    #[serde(default)]
    pub reconciled_at: u64,
    /// Monotonic; `Reconcile` requires `seq == old + 1`, so a replayed or
    /// reordered reconciliation cannot land.
    #[plutus(id = 2, default)]
    #[serde(default)]
    pub reconciled_seq: u64,
    /// `blake2b(sorted receipt ids since the last reconciliation)`.
    ///
    /// The validator cannot check that a debit matches the receipts; this
    /// hash plus the published list (R2, stable URL) is what makes it
    /// checkable. **A balance is never a number we assert; it is a sum
    /// anyone can redo.**
    #[plutus(id = 3, default)]
    #[serde(default)]
    pub receipts_hash: Bytes,
    /// **Reserved and unset** (protocol §1.2b): a partner-sponsored tank
    /// redeemable only against that partner's definitions. The id exists so
    /// that restriction never needs a second token policy.
    #[plutus(id = 4)]
    #[serde(default)]
    pub scope: Option<Scope>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

/// Reserved. No variant is accepted by `validate()` yet.
#[derive(Debug, Clone, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum Scope {
    /// Spendable only against definitions owned by this key.
    #[plutus(tag = 0)]
    Owner {
        #[plutus(id = 1)]
        owner: PaymentKeyHash,
        #[plutus(unknown)]
        #[serde(skip)]
        unknown: UnknownFields,
    },
    #[plutus(unknown)]
    Unknown {
        tag: i64,
        #[serde(skip)]
        fields: UnknownFields,
    },
}

/// One UTxO per network, marked by a one-shot token, updatable **only by the
/// cold admin key**.
///
/// Everything here can change without redeploying a validator — which is the
/// point: redeploying `fuel.ak` would move every tank's address, so rotating
/// the hot spender key has to be a datum change instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct ProtocolConfigBody {
    /// Which tokens are fuel currency, at what rate.
    #[plutus(id = 0, default)]
    #[serde(default)]
    pub currencies: Vec<Currency>,
    /// Key hashes allowed to sign a `Reconcile`. Rotating one is an
    /// `Update`, never a redeploy.
    #[plutus(id = 1, default)]
    #[serde(default)]
    pub authorized_spenders: Vec<PaymentKeyHash>,
    /// What each kind of work costs, in credits.
    #[plutus(id = 2, default)]
    #[serde(default)]
    pub cost_table: Vec<CostEntry>,
    #[plutus(id = 3, default)]
    #[serde(default)]
    pub cost_table_version: u32,
    /// The most a single tank may be debited in one day. Bounds what a
    /// compromised spender key can take to one day's plausible usage.
    #[plutus(id = 4, default)]
    #[serde(default)]
    pub max_debit_per_day: u64,
    /// Lovelace per credit, for the ADA top-up path.
    #[plutus(id = 5, default)]
    #[serde(default)]
    pub ada_per_credit: u64,
    /// Credits debited from a tank by `Register`. Default 0 — there is no
    /// listing fee; a tank authorising the posting is the whole check.
    #[plutus(id = 6, default)]
    #[serde(default)]
    pub posting_cost: u64,
    /// Who may change this config. **The cold admin, and the reason this
    /// config is self-amending.**
    ///
    /// In the datum rather than a validator parameter: a parameter change
    /// recompiles the script, which moves its address, which moves the one
    /// UTxO every other validator references. The hot `authorized_spenders`
    /// above already avoided that trap; this is the same rule applied to
    /// the key that governs them.
    #[plutus(id = 7, default)]
    #[serde(default)]
    pub authorized_updaters: Vec<PaymentKeyHash>,
    /// How many of [`Self::authorized_updaters`] must sign.
    ///
    /// A list-and-threshold even for a single signer, because the shape
    /// buys two things that have nothing to do with multisig: **rotation
    /// is reversible** (add the new device, prove it signs, then drop the
    /// old — rather than one swap that bricks the config if the hash is
    /// wrong), and **1-of-2 survives a dead device**. Raising it to 2-of-3
    /// later is a datum update, not a redeploy.
    #[plutus(id = 8, default)]
    #[serde(default)]
    pub updater_threshold: u32,
    /// Where an ADA top-up must pay, as a **payment credential** — the
    /// stake part is deliberately not constrained, so the fees can be
    /// delegated without the address the validator checks changing.
    ///
    /// `None` means **the ADA top-up path is closed**, not "pay anywhere".
    /// A defaulted field's absent value has to be the one that grants
    /// nothing; the alternative reading would let a fresh config mint
    /// credits for a payment to nobody.
    ///
    /// In the config rather than a `fuel.ak` parameter because a parameter
    /// change moves every tank's address. It is not a parameter of the
    /// fuel-pair policy either, for a worse version of the same reason: it
    /// would change the fuel policy id, hence every tank's asset id, hence
    /// `escrow.ak`'s parameter.
    #[plutus(id = 9)]
    #[serde(default)]
    pub fee_credential: Option<Credential>,
    /// Payment credentials that count as burn sinks for the burn → fuel
    /// path. An empty list closes that path.
    ///
    /// The validator has to authenticate the sink, or a "burn" to an
    /// address the burner controls buys credits while keeping the tokens.
    #[plutus(id = 10, default)]
    #[serde(default)]
    pub sinks: Vec<Credential>,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

/// A payment credential: a key hash or a script hash.
///
/// Mirrors aiken's `cardano/address.Credential` in meaning, not in encoding
/// — this is our integer-keyed map shape, because nothing decodes an
/// on-chain `Credential` from here; the validator rebuilds the comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct Credential {
    /// 28 bytes.
    #[plutus(id = 0, default)]
    #[serde(default)]
    pub hash: Bytes,
    /// `false` = verification key, `true` = script.
    #[plutus(id = 1, default)]
    #[serde(default)]
    pub is_script: bool,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

impl Credential {
    pub fn key(hash: [u8; 28]) -> Self {
        Self {
            hash: Bytes::from(hash.to_vec()),
            is_script: false,
            unknown: UnknownFields::default(),
        }
    }

    pub fn script(hash: [u8; 28]) -> Self {
        Self {
            hash: Bytes::from(hash.to_vec()),
            is_script: true,
            unknown: UnknownFields::default(),
        }
    }
}

impl ProtocolConfigBody {
    /// The rate for a currency, if it is admitted. A fuel definition naming
    /// a token that is not here is harmless and inert.
    pub fn currency_rate(&self, policy: PolicyId, name: &[u8]) -> Option<u64> {
        self.currencies
            .iter()
            .find(|c| {
                c.policy == policy
                    && match &c.name {
                        Some(n) => n.as_slice() == name,
                        // No name means any asset under the policy.
                        None => true,
                    }
            })
            .map(|c| c.credits_per_unit)
    }

    /// What one delivery of this effect costs.
    pub fn cost_of(&self, effect: EffectKind) -> Option<u64> {
        let key = effect.cost_key()?;
        self.cost_table
            .iter()
            .find(|entry| entry.effect_kind == key)
            .map(|entry| entry.credits)
    }

    pub fn is_authorized_spender(&self, key: &PaymentKeyHash) -> bool {
        self.authorized_spenders.contains(key)
    }

    pub fn is_sink(&self, credential: &Credential) -> bool {
        self.sinks
            .iter()
            .any(|sink| sink.hash == credential.hash && sink.is_script == credential.is_script)
    }

    /// Lovelace that must reach [`Self::fee_credential`] to buy `credits`.
    ///
    /// `None` closes the ADA path — when no fee credential is set, or when
    /// the rate is zero. **A zero rate would sell credits for nothing**,
    /// and it is the value a config carries before anyone sets one, so it
    /// cannot be allowed to mean "free".
    pub fn ada_topup_price(&self, credits: u64) -> Option<u64> {
        if self.ada_per_credit == 0 || self.fee_credential.is_none() {
            return None;
        }
        credits.checked_mul(self.ada_per_credit)
    }

    /// Do these signatories carry enough authority to update the config?
    ///
    /// Mirrors what `protocol_config.ak` enforces, so the operator surface
    /// can say "this transaction will not be accepted" before asking anyone
    /// to plug in a hardware wallet.
    ///
    /// **A threshold of zero authorises nobody.** The alternative reading —
    /// "zero signatures required" — would make an unset threshold mean
    /// anyone may rewrite the currencies and spender list, which is the
    /// worst possible default for a field that defaults.
    pub fn updater_quorum_met(&self, signatories: &[PaymentKeyHash]) -> bool {
        if self.updater_threshold == 0 {
            return false;
        }
        // DISTINCT signers, not list entries. Counting entries would let one
        // key listed twice satisfy a 2-of-N threshold on its own — a
        // duplicate, whether a copy-paste slip or deliberate, would silently
        // halve the bar it looks like it raises.
        let signed: std::collections::BTreeSet<_> = self
            .authorized_updaters
            .iter()
            .filter(|updater| signatories.contains(updater))
            .collect();
        signed.len() as u32 >= self.updater_threshold
    }
}

/// A token admitted as fuel currency, and its rate.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct Currency {
    #[plutus(id = 0)]
    pub policy: PolicyId,
    /// Absent means any asset name under the policy.
    #[plutus(id = 1)]
    #[serde(default)]
    pub name: Option<Bytes>,
    #[plutus(id = 2, default)]
    #[serde(default)]
    pub credits_per_unit: u64,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

/// What one delivery of one effect kind costs.
#[derive(Debug, Clone, Default, PartialEq, Eq, PlutusCodec, Serialize, Deserialize)]
pub struct CostEntry {
    /// An [`EffectKind`]'s wire tag — one vocabulary, so a cost entry can
    /// never name an effect that does not exist.
    #[plutus(id = 0, default)]
    #[serde(default)]
    pub effect_kind: u32,
    #[plutus(id = 1, default)]
    #[serde(default)]
    pub credits: u64,
    #[plutus(unknown)]
    #[serde(skip)]
    pub unknown: UnknownFields,
}

impl CostEntry {
    pub fn new(effect: EffectKind, credits: u64) -> Option<Self> {
        Some(Self {
            effect_kind: effect.cost_key()?,
            credits,
            unknown: UnknownFields::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{Cip68Envelope, MapWriter, PlutusCodec as _};
    use pallas_primitives::{Fragment, PlutusData};

    fn tank() -> FuelBody {
        FuelBody {
            balance: 1_000,
            reconciled_at: 12_345,
            reconciled_seq: 7,
            receipts_hash: Bytes::from(vec![0xab; 32]),
            scope: None,
            unknown: UnknownFields::default(),
        }
    }

    #[test]
    fn a_fuel_body_round_trips_inside_a_real_cip68_datum() {
        let body = tank();
        let mut metadata = MapWriter::new();
        metadata.field(0, &"Defrag Fuel".to_string());
        let metadata = metadata.finish();

        let envelope = Cip68Envelope {
            metadata: metadata.clone(),
            version: Cip68Envelope::CIP68_VERSION,
            extra: body.to_data(),
        };

        let bytes = envelope.to_data().encode_fragment().unwrap();
        let reparsed = PlutusData::decode_fragment(&bytes).unwrap();
        let back = Cip68Envelope::from_data(&reparsed).unwrap();

        assert_eq!(
            back.metadata, metadata,
            "the validator compares metadata byte-for-byte across a TopUp"
        );
        assert_eq!(FuelBody::from_data(&back.extra).unwrap(), body);
    }

    #[test]
    fn a_fresh_tank_encodes_almost_nothing() {
        // Every field defaults, so a newly minted empty tank's extra is an
        // empty map — which keeps the mint's min-ADA down.
        let data = FuelBody::default().to_data();
        assert!(crate::codec::as_map(&data).unwrap().is_empty());
        assert_eq!(FuelBody::from_data(&data).unwrap(), FuelBody::default());
    }

    #[test]
    fn scope_is_reserved_and_absent() {
        assert_eq!(tank().scope, None);
        let data = tank().to_data();
        let ids: Vec<i64> = crate::codec::as_map(&data)
            .unwrap()
            .iter()
            .map(|(k, _)| crate::codec::as_i128(k).unwrap() as i64)
            .collect();
        assert_eq!(ids, vec![0, 1, 2, 3], "scope must not be written");
    }

    #[test]
    fn the_config_round_trips_and_answers_rates_and_costs() {
        let policy = PolicyId([5u8; 28]);
        let config = ProtocolConfigBody {
            currencies: vec![Currency {
                policy,
                name: Some(Bytes::from(b"PERP".to_vec())),
                credits_per_unit: 3,
                unknown: UnknownFields::default(),
            }],
            authorized_spenders: vec![PaymentKeyHash([6u8; 28])],
            cost_table: vec![
                CostEntry::new(EffectKind::Notify, 1).unwrap(),
                CostEntry::new(EffectKind::Onchain, 5).unwrap(),
                CostEntry::new(EffectKind::Fuel, 0).unwrap(),
            ],
            cost_table_version: 1,
            max_debit_per_day: 1_000,
            ada_per_credit: 500_000,
            posting_cost: 0,
            authorized_updaters: vec![PaymentKeyHash([7u8; 28])],
            updater_threshold: 1,
            fee_credential: Some(Credential::key([8u8; 28])),
            sinks: vec![Credential::script([9u8; 28])],
            unknown: UnknownFields::default(),
        };

        let data = config.to_data();
        assert_eq!(ProtocolConfigBody::from_data(&data).unwrap(), config);

        assert_eq!(config.currency_rate(policy, b"PERP"), Some(3));
        assert_eq!(config.currency_rate(policy, b"SNEK"), None);
        assert_eq!(config.cost_of(EffectKind::Notify), Some(1));
        assert_eq!(config.cost_of(EffectKind::Onchain), Some(5));
        // A self-settled burn→fuel top-up costs the tank nothing: the
        // burner's own tx did the work and paid its fee.
        assert_eq!(config.cost_of(EffectKind::Fuel), Some(0));
        assert_eq!(config.cost_of(EffectKind::Manual), None);
        assert!(config.is_authorized_spender(&PaymentKeyHash([6u8; 28])));
        assert!(!config.is_authorized_spender(&PaymentKeyHash([7u8; 28])));
    }

    #[test]
    fn a_single_signer_threshold_behaves_as_one_of_one() {
        let me = PaymentKeyHash([0xaa; 28]);
        let config = ProtocolConfigBody {
            authorized_updaters: vec![me],
            updater_threshold: 1,
            ..ProtocolConfigBody::default()
        };
        assert!(config.updater_quorum_met(&[me]));
        assert!(!config.updater_quorum_met(&[]));
        assert!(!config.updater_quorum_met(&[PaymentKeyHash([0xbb; 28])]));
        // Signing alongside others is still signing.
        assert!(config.updater_quorum_met(&[PaymentKeyHash([0xbb; 28]), me]));
    }

    /// The shape that makes a lost hardware wallet survivable: either key
    /// acts alone.
    #[test]
    fn one_of_two_lets_a_backup_key_act_without_the_primary() {
        let ledger = PaymentKeyHash([0xaa; 28]);
        let backup = PaymentKeyHash([0xbb; 28]);
        let config = ProtocolConfigBody {
            authorized_updaters: vec![ledger, backup],
            updater_threshold: 1,
            ..ProtocolConfigBody::default()
        };
        assert!(config.updater_quorum_met(&[ledger]));
        assert!(config.updater_quorum_met(&[backup]));
    }

    /// …and raising the bar later is a datum change, not a redeploy.
    #[test]
    fn two_of_three_needs_two_and_the_code_is_unchanged() {
        let keys: Vec<PaymentKeyHash> = (0..3).map(|i| PaymentKeyHash([i; 28])).collect();
        let config = ProtocolConfigBody {
            authorized_updaters: keys.clone(),
            updater_threshold: 2,
            ..ProtocolConfigBody::default()
        };
        assert!(!config.updater_quorum_met(&[keys[0]]), "one is not enough");
        assert!(config.updater_quorum_met(&[keys[0], keys[2]]));
    }

    /// A threshold of zero must authorise NOBODY.
    ///
    /// It is a defaulted field, so the value that appears when somebody
    /// forgets is the one that must be safe. Read the other way — "zero
    /// signatures required" — an unset threshold would let anyone rewrite
    /// the currency list and the spender set.
    #[test]
    fn a_zero_threshold_locks_rather_than_opens() {
        let me = PaymentKeyHash([0xaa; 28]);
        let config = ProtocolConfigBody {
            authorized_updaters: vec![me],
            updater_threshold: 0,
            ..ProtocolConfigBody::default()
        };
        assert!(!config.updater_quorum_met(&[me]));
        assert!(!config.updater_quorum_met(&[]));
        assert!(!ProtocolConfigBody::default().updater_quorum_met(&[]));
    }

    /// Duplicates in the list cannot manufacture a quorum.
    #[test]
    fn one_key_listed_twice_still_counts_once_toward_a_threshold_of_two() {
        let me = PaymentKeyHash([0xaa; 28]);
        let config = ProtocolConfigBody {
            authorized_updaters: vec![me, me],
            updater_threshold: 2,
            ..ProtocolConfigBody::default()
        };
        assert!(
            !config.updater_quorum_met(&[me]),
            "a duplicated member must not let one signature satisfy 2-of-N"
        );
    }

    /// Both new paths must be **closed** in a default config, for the same
    /// reason `updater_threshold: 0` locks: these are defaulted fields, so
    /// the value present when somebody forgets is the one that has to be
    /// safe. An unset fee credential meaning "pay anywhere" would let a
    /// fresh config mint credits for a payment to the buyer themselves.
    #[test]
    fn a_default_config_sells_no_credits_and_knows_no_sinks() {
        let config = ProtocolConfigBody::default();
        assert_eq!(config.ada_topup_price(1), None);
        assert!(!config.is_sink(&Credential::key([0u8; 28])));
    }

    #[test]
    fn a_zero_rate_closes_the_ada_path_rather_than_making_credits_free() {
        let config = ProtocolConfigBody {
            fee_credential: Some(Credential::key([1u8; 28])),
            ada_per_credit: 0,
            ..ProtocolConfigBody::default()
        };
        assert_eq!(config.ada_topup_price(100), None);
    }

    #[test]
    fn the_ada_price_is_credits_times_the_rate() {
        let config = ProtocolConfigBody {
            fee_credential: Some(Credential::key([1u8; 28])),
            ada_per_credit: 500_000,
            ..ProtocolConfigBody::default()
        };
        assert_eq!(config.ada_topup_price(4), Some(2_000_000));
        // An overflowing ask is refused, not wrapped into a small price.
        assert_eq!(config.ada_topup_price(u64::MAX), None);
    }

    /// A key hash and a script hash of the same bytes are different
    /// credentials. Conflating them would let a script at hash H collect
    /// fees destined for the key at hash H.
    #[test]
    fn a_sink_matches_on_both_the_hash_and_the_kind() {
        let config = ProtocolConfigBody {
            sinks: vec![Credential::script([9u8; 28])],
            ..ProtocolConfigBody::default()
        };
        assert!(config.is_sink(&Credential::script([9u8; 28])));
        assert!(!config.is_sink(&Credential::key([9u8; 28])));
        assert!(!config.is_sink(&Credential::script([8u8; 28])));
    }

    #[test]
    fn a_currency_with_no_name_admits_every_asset_under_the_policy() {
        let policy = PolicyId([5u8; 28]);
        let config = ProtocolConfigBody {
            currencies: vec![Currency {
                policy,
                name: None,
                credits_per_unit: 2,
                unknown: UnknownFields::default(),
            }],
            ..ProtocolConfigBody::default()
        };
        assert_eq!(config.currency_rate(policy, b"anything"), Some(2));
        assert_eq!(config.currency_rate(PolicyId([9u8; 28]), b""), None);
    }
}
