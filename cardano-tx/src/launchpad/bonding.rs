//! Bonding-lab pool datum — the writer for the pool state anchor.
//!
//! Spec: `projects/bonding-lab/SETTLEMENT.md` §3 in `cnft.dev-workers`. The Aiken
//! reader is pinned to the golden vectors at the bottom of this file, the same
//! split `contracts/action-protocol` uses for its own schema.
//!
//! Two shapes live here, and they are **different constructors on purpose**:
//!
//! ```text
//! PoolState  Constr 0 [phase, tick, root, supply, deadline, target, operator]
//! Deposit    Constr 1 [policy, name, account]
//! ```
//!
//! A deposit output and the pool UTxO can sit at the same address. If both were
//! `Constr 0`, a reader would be telling them apart by field count — exactly the
//! ambiguity that makes one decoder quietly read another venue's pool
//! (`crate::launchpad::lumpfun` and `crate::route::lumppad` document the same
//! trap from the other side).
//!
//! The datum is **pool state, not CIP-68 metadata**, even though the pool NFT's
//! asset name wears a CIP-67 `(100)` label. Do not point a metadata reader at it.
//!
//! Fields a validator can observe are deliberately absent: `reserve` is the pool
//! UTxO's own lovelace, read directly, because a datum copy could disagree with
//! the balance it describes. `root` binds the accounts, `supply` is what a bond
//! mint must equal and what a death split divides by, and `deadline`/`target` are
//! duplicated from the compiled script parameter so a validator holding only the
//! datum can check them.
//!
//! Encoding is definite-length (`constr`, not `constr_indef`): inline datums are
//! never hashed, and the canonical form makes two writers byte-identical. Aiken
//! parses both forms, so this is a determinism choice, not a compatibility one.

use pallas_primitives::conway::PlutusData;
use pallas_primitives::{BigInt, Fragment};

use cardano_assets::UtxoApi;
use pallas_addresses::Address;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{ExUnits, ScriptKind};

use crate::blueprint::{PlutusLanguage, script_hash};
use crate::builder::fluent::TxBuilder;
use crate::builder::script::{
    CollateralConfig, MintEntry, ScriptSource, bytes, constr, constr0_empty, encode_plutus_data,
    int,
};
use crate::builder::{TxDeps, UnsignedTx};
use crate::helpers::output::{add_assets_to_output, create_ada_output};
use crate::utxo::{OutputParams, calculate_min_ada_with_params};

/// `Constr` index of the pool state datum.
pub const POOL_STATE_CONSTRUCTOR: u32 = 0;
/// `Constr` index of a deposit attestation.
pub const DEPOSIT_CONSTRUCTOR: u32 = 1;

pub const POOL_STATE_FIELDS: usize = 7;
pub const DEPOSIT_FIELDS: usize = 3;

/// A settlement root: the hash of the canonical preimage in
/// `bonding_core::chain` (`bonding/settlement-root/v1`, blake2b-256).
pub const ROOT_LEN: usize = 32;
/// A payment key hash.
pub const KEY_HASH_LEN: usize = 28;
/// A minting policy id.
pub const POLICY_ID_LEN: usize = 28;
/// A Cardano asset name is 1..=32 bytes.
pub const ASSET_NAME_MAX: usize = 32;
/// `open | bonded | dead` — the order is the datum's `Constr` index.
pub const MAX_PHASE: u64 = 2;

#[derive(Debug, thiserror::Error)]
pub enum BondingError {
    #[error("invalid datum: {0}")]
    Invalid(String),
    #[error("datum is not CBOR: {0}")]
    Cbor(String),
    #[error("datum encoding failed: {0}")]
    Encoding(String),
    #[error("transaction build failed: {0}")]
    Build(String),
}

/// The inline datum on the pool UTxO, as `Constr 0` with seven fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolState {
    /// `0 = open`, `1 = bonded`, `2 = dead`.
    pub phase: u64,
    /// The last tick this state covers. A settlement's root must be this tick's.
    pub tick: u64,
    /// 32 bytes.
    pub root: Vec<u8>,
    /// Units outstanding at `tick`.
    pub supply: u64,
    /// POSIX **milliseconds** — what a validator's validity range is measured in.
    pub deadline: u64,
    pub target: u64,
    /// 28 bytes.
    pub operator: Vec<u8>,
}

impl PoolState {
    pub fn to_data(&self) -> Result<PlutusData, BondingError> {
        if self.phase > MAX_PHASE {
            return Err(BondingError::Invalid(format!(
                "phase: {} is not one of open|bonded|dead",
                self.phase
            )));
        }
        check_len("root", &self.root, ROOT_LEN)?;
        check_len("operator", &self.operator, KEY_HASH_LEN)?;

        Ok(constr(
            POOL_STATE_CONSTRUCTOR,
            vec![
                amount(self.phase, "phase")?,
                amount(self.tick, "tick")?,
                bytes(self.root.clone()),
                amount(self.supply, "supply")?,
                amount(self.deadline, "deadline")?,
                amount(self.target, "target")?,
                bytes(self.operator.clone()),
            ],
        ))
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, BondingError> {
        encode_plutus_data(&self.to_data()?).map_err(|e| BondingError::Encoding(e.to_string()))
    }

    pub fn from_cbor(cbor: &[u8]) -> Result<Self, BondingError> {
        let data = PlutusData::decode_fragment(cbor)
            .map_err(|e| BondingError::Cbor(format!("pool state is not PlutusData: {e}")))?;
        Self::from_data(&data)
    }

    pub fn from_data(data: &PlutusData) -> Result<Self, BondingError> {
        let fields = fields_of(
            data,
            POOL_STATE_CONSTRUCTOR,
            POOL_STATE_FIELDS,
            "pool state",
        )?;

        let phase = field_u64(&fields[0], "phase")?;
        if phase > MAX_PHASE {
            return Err(BondingError::Invalid(format!(
                "phase: {phase} is not one of open|bonded|dead"
            )));
        }
        let root = field_bytes(&fields[2], "root")?;
        check_len("root", &root, ROOT_LEN)?;
        let operator = field_bytes(&fields[6], "operator")?;
        check_len("operator", &operator, KEY_HASH_LEN)?;

        Ok(Self {
            phase,
            tick: field_u64(&fields[1], "tick")?,
            root,
            supply: field_u64(&fields[3], "supply")?,
            deadline: field_u64(&fields[4], "deadline")?,
            target: field_u64(&fields[5], "target")?,
            operator,
        })
    }
}

/// The inline datum a deposit output carries: `Constr 1 [policy, name, account]`.
///
/// The pool address is key-controlled on preprod, so this is **evidence, not
/// enforcement** — any output can carry any datum. A deposit naming a foreign
/// pool or an unknown account is quarantined rather than credited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deposit {
    /// 28 bytes.
    pub policy: Vec<u8>,
    /// 1..=32 bytes.
    pub name: Vec<u8>,
    /// The sequencer's account id, mapped off-chain from the stake credential.
    pub account: u64,
}

impl Deposit {
    pub fn to_data(&self) -> Result<PlutusData, BondingError> {
        check_len("policy", &self.policy, POLICY_ID_LEN)?;
        check_name(&self.name)?;

        Ok(constr(
            DEPOSIT_CONSTRUCTOR,
            vec![
                bytes(self.policy.clone()),
                bytes(self.name.clone()),
                amount(self.account, "account")?,
            ],
        ))
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, BondingError> {
        encode_plutus_data(&self.to_data()?).map_err(|e| BondingError::Encoding(e.to_string()))
    }

    pub fn from_cbor(cbor: &[u8]) -> Result<Self, BondingError> {
        let data = PlutusData::decode_fragment(cbor)
            .map_err(|e| BondingError::Cbor(format!("deposit is not PlutusData: {e}")))?;
        Self::from_data(&data)
    }

    pub fn from_data(data: &PlutusData) -> Result<Self, BondingError> {
        let fields = fields_of(data, DEPOSIT_CONSTRUCTOR, DEPOSIT_FIELDS, "deposit")?;

        let policy = field_bytes(&fields[0], "policy")?;
        check_len("policy", &policy, POLICY_ID_LEN)?;
        let name = field_bytes(&fields[1], "name")?;
        check_name(&name)?;

        Ok(Self {
            policy,
            name,
            account: field_u64(&fields[2], "account")?,
        })
    }
}

/// Everything a pool's creation transaction needs beyond the ledger's inputs.
#[derive(Clone, Debug)]
pub struct PoolCreation {
    /// The **applied** minting policy's compiled code (the blueprint's
    /// `compiledCode`, decoded). `create_pool` refuses it unless it hashes to
    /// `policy_id`, so a stale or unapplied blueprint cannot be signed by
    /// accident.
    pub policy_script: Vec<u8>,
    pub policy_id: Hash<28>,
    /// The state NFT's 8 bytes: CIP-67 `(100)` + `POOL`.
    pub asset_name: Vec<u8>,
    /// Where the pool UTxO lives. Key-controlled on preprod, a script on mainnet.
    pub pool_address: Address,
    /// The [`PoolState`] datum, already encoded.
    pub datum_cbor: Vec<u8>,
    /// The UTxO the policy is parameterised by. **It is spent here**, which is
    /// what makes the mint one-shot.
    pub seed: UtxoApi,
    /// The operator's payment key hash, which the transaction requires.
    pub operator_pkh: Hash<28>,
    /// The pool UTxO's lovelace. `None` is the ledger's minimum for a datum and
    /// one asset; `Some` below that is refused rather than signed into a
    /// `BabbageOutputTooSmallUTxO`.
    pub pool_lovelace: Option<u64>,
    /// The budget to build the mint with. An unevaluated build charges this;
    /// use `build_evaluated` when an evaluator is available.
    pub ex_units: ExUnits,
}

/// Build the transaction that mints a pool's state NFT and locks its first datum.
///
/// The seed is spent, one NFT is minted under the applied policy, and the pool
/// UTxO is created with the datum inline. Change returns to `deps.from_address`,
/// so the seed's lovelace is not destroyed — it comes back as change unless
/// `pool_lovelace` spends it.
///
/// **The caller signs.** This is the trusted build path, not a signer: the
/// operator key, submission and the retry policy stay outside it.
pub fn create_pool(deps: TxDeps, creation: &PoolCreation) -> Result<UnsignedTx, BondingError> {
    check_name(&creation.asset_name)?;
    if creation.datum_cbor.is_empty() {
        return Err(BondingError::Invalid(
            "datum: a pool UTxO without its state datum is not a pool".into(),
        ));
    }

    // The one fact the whole pool identity rests on: the policy id IS the hash of
    // the applied code. Checking it here means an unapplied or stale blueprint
    // fails at build time rather than at submit, where the diagnosis is a phase-1
    // failure with no useful message.
    let computed = script_hash(PlutusLanguage::V3, &creation.policy_script);
    if computed != creation.policy_id {
        return Err(BondingError::Invalid(format!(
            "policy id: the script hashes to {computed}, not {} — that is not the applied blueprint",
            creation.policy_id
        )));
    }

    let policy_hex = creation.policy_id.to_string();
    let name_hex = hex::encode(&creation.asset_name);
    let asset: cardano_assets::AssetId = format!("{policy_hex}.{name_hex}")
        .parse()
        .map_err(|error| BondingError::Invalid(format!("asset id: {error}")))?;
    let minimum = calculate_min_ada_with_params(
        &deps.params,
        &[(asset, 1)],
        &OutputParams::with_datum(&creation.datum_cbor),
    );
    let pool_lovelace = creation.pool_lovelace.unwrap_or(minimum);
    if pool_lovelace < minimum {
        return Err(BondingError::Invalid(format!(
            "pool lovelace: {pool_lovelace} is below the ledger minimum of {minimum} for this \
             datum and asset"
        )));
    }

    let pool = create_ada_output(creation.pool_address.clone(), pool_lovelace);
    let pool = add_assets_to_output(pool, &[(&policy_hex, &name_hex, 1)])
        .map_err(|error| BondingError::Build(error.to_string()))?;
    let pool = pool.set_inline_datum(creation.datum_cbor.clone());

    let redeemer = encode_plutus_data(&constr0_empty())
        .map_err(|error| BondingError::Build(error.to_string()))?;

    TxBuilder::new(deps)
        .input(&creation.seed)
        .map_err(|error| BondingError::Build(error.to_string()))?
        .mint(MintEntry {
            policy: creation.policy_id,
            assets: vec![(creation.asset_name.clone(), 1)],
            script: ScriptSource::Inline {
                language: ScriptKind::PlutusV3,
                bytes: creation.policy_script.clone(),
            },
            redeemer_cbor: redeemer,
            ex_units: creation.ex_units.clone(),
        })
        .output(pool)
        .with_collateral(CollateralConfig::Auto)
        .with_signer(creation.operator_pkh)
        .build()
        .map_err(|error| BondingError::Build(error.to_string()))
}

/// The constructor tag of a `Constr`, in either the CIP-121 short form (121–127)
/// or the explicit tag-102 form. Returns `None` for anything else.
fn constructor_index(data: &PlutusData) -> Option<u64> {
    let PlutusData::Constr(constr) = data else {
        return None;
    };
    if (121..=127).contains(&constr.tag) {
        Some(constr.tag - 121)
    } else {
        constr.any_constructor
    }
}

fn fields_of(
    data: &PlutusData,
    expected: u32,
    count: usize,
    what: &str,
) -> Result<Vec<PlutusData>, BondingError> {
    let PlutusData::Constr(constr) = data else {
        return Err(BondingError::Invalid(format!(
            "expected a {what} datum, got another PlutusData variant"
        )));
    };
    let Some(index) = constructor_index(data) else {
        return Err(BondingError::Invalid(format!(
            "expected a {what} datum, got CBOR tag {}",
            constr.tag
        )));
    };
    if index != u64::from(expected) {
        return Err(BondingError::Invalid(format!(
            "expected a {what} datum (Constr {expected}), got Constr {index}"
        )));
    }
    if constr.fields.len() != count {
        return Err(BondingError::Invalid(format!(
            "expected {count} fields in a {what} datum, got {}",
            constr.fields.len()
        )));
    }
    Ok(constr.fields.clone().to_vec())
}

fn check_len(name: &'static str, value: &[u8], expected: usize) -> Result<(), BondingError> {
    if value.len() != expected {
        return Err(BondingError::Invalid(format!(
            "{name}: expected {expected} bytes, got {}",
            value.len()
        )));
    }
    Ok(())
}

fn check_name(name: &[u8]) -> Result<(), BondingError> {
    if name.is_empty() || name.len() > ASSET_NAME_MAX {
        return Err(BondingError::Invalid(format!(
            "name: expected 1..={ASSET_NAME_MAX} bytes, got {}",
            name.len()
        )));
    }
    Ok(())
}

fn field_bytes(data: &PlutusData, name: &'static str) -> Result<Vec<u8>, BondingError> {
    match data {
        PlutusData::BoundedBytes(bytes) => Ok(bytes.as_slice().to_vec()),
        other => Err(BondingError::Invalid(format!(
            "{name}: expected bytes, got {other:?}"
        ))),
    }
}

fn field_u64(data: &PlutusData, name: &'static str) -> Result<u64, BondingError> {
    match data {
        PlutusData::BigInt(BigInt::Int(value)) => {
            let value: i128 = (*value).into();
            u64::try_from(value)
                .map_err(|_| BondingError::Invalid(format!("{name}: {value} is not a u64")))
        }
        other => Err(BondingError::Invalid(format!(
            "{name}: expected an integer, got {other:?}"
        ))),
    }
}

fn amount(value: u64, name: &'static str) -> Result<PlutusData, BondingError> {
    i64::try_from(value)
        .map(int)
        .map_err(|_| BondingError::Invalid(format!("{name}: {value} exceeds a Plutus Int")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> PoolState {
        PoolState {
            phase: 2,
            tick: 1_234,
            root: vec![0xab; ROOT_LEN],
            supply: 90,
            deadline: 1_700_000_000_000,
            target: 1_000_000_000,
            operator: vec![0xcd; KEY_HASH_LEN],
        }
    }

    fn deposit() -> Deposit {
        Deposit {
            policy: vec![0x11; POLICY_ID_LEN],
            name: b"bonded".to_vec(),
            account: 7,
        }
    }

    #[test]
    fn a_pool_state_round_trips() {
        let bytes = state().to_cbor().expect("encodes");
        assert_eq!(PoolState::from_cbor(&bytes).expect("decodes"), state());
    }

    #[test]
    fn the_pool_state_is_constr_zero_with_seven_fields() {
        let data = state().to_data().expect("encodes");
        let PlutusData::Constr(constr) = &data else {
            panic!("not a Constr");
        };
        assert_eq!(constr.tag, 121, "Constr 0 is CBOR tag 121");
        assert_eq!(constr.fields.len(), POOL_STATE_FIELDS);

        // Definite-length: the array header, not the `9f ... ff` form.
        let cbor = state().to_cbor().expect("encodes");
        assert_eq!(&cbor[..2], &[0xd8, 0x79], "Constr 0 header");
        assert_ne!(cbor[2], 0x9f, "a definite-length field array");
    }

    /// The encoding the Aiken reader is pinned to, stated structurally rather
    /// than as a blob: one wrong field order or a widened integer fails here.
    ///
    /// `d879` tag 121 (Constr 0), `87` seven fields, `02` phase, `1904d2` tick
    /// 1234, `5820` + root, `185a` supply 90, `1b…` deadline 1_700_000_000_000,
    /// `1a…` target 1_000_000_000, `581c` + operator.
    #[test]
    fn the_creation_transaction_mints_one_anchor_and_locks_the_datum() {
        use cardano_assets::UtxoApi;
        use pallas_addresses::{
            Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
        };
        use pallas_txbuilder::ExUnits;

        // A stand-in program: this test is about the transaction's SHAPE, so the
        // bytes only need to hash to the policy the creation names. A real one
        // comes from `Blueprint::verified_script` on the applied blueprint.
        let script = vec![0x46, 0x01, 0x00, 0x00, 0x00];
        let policy_id = script_hash(PlutusLanguage::V3, &script);
        let pool_address = Address::Shelley(ShelleyAddress::new(
            Network::Testnet,
            ShelleyPaymentPart::Key(Hash::from([9u8; 28])),
            ShelleyDelegationPart::Null,
        ));
        let operator = Hash::from([7u8; 28]);

        let seed = UtxoApi {
            tx_hash: "aa".repeat(32),
            output_index: 0,
            lovelace: 5_000_000,
            assets: Vec::new(),
            tags: Vec::new(),
        };
        let deps = TxDeps {
            utxos: vec![
                seed.clone(),
                UtxoApi {
                    tx_hash: "bb".repeat(32),
                    output_index: 1,
                    lovelace: 100_000_000,
                    assets: Vec::new(),
                    tags: Vec::new(),
                },
            ],
            params: crate::params::TxBuildParams::default(),
            from_address: pool_address.clone(),
            network_id: 0,
        };

        let datum = state().to_cbor().expect("encodes");
        let creation = PoolCreation {
            policy_script: script,
            policy_id,
            asset_name: b"\x00\x06\x43\xb0POOL".to_vec(),
            pool_address,
            datum_cbor: datum.clone(),
            seed,
            operator_pkh: operator,
            pool_lovelace: None,
            // A fixture budget: an unevaluated build charges this, and a real
            // caller uses `build_evaluated` for the measured figure.
            ex_units: ExUnits {
                mem: 200_000,
                steps: 100_000_000,
            },
        };

        let unsigned = create_pool(deps, &creation).expect("builds");

        // One policy, one name, one unit — the anchor.
        let mint = unsigned.staging.mint.as_ref().expect("a mint");
        assert_eq!(mint.len(), 1, "exactly one policy is minted");
        let (policy, assets) = mint.iter().next().expect("the policy");
        assert_eq!(policy.0, *policy_id);
        let (name, quantity) = assets.iter().next().expect("the asset");
        assert_eq!(name.0.as_slice(), creation.asset_name.as_slice());
        assert_eq!(*quantity, 1);

        // The pool output is at the pool address, carries the datum inline, and
        // holds the anchor. Compared against an output built through the public
        // API, because the datum's type is not nameable outside pallas.
        let outputs = unsigned.staging.outputs.as_ref().expect("outputs");
        let pool = outputs
            .iter()
            .find(|output| output.datum.is_some())
            .expect("the pool output");
        let expected =
            create_ada_output(creation.pool_address.clone(), pool.lovelace).set_inline_datum(datum);
        assert_eq!(
            pool.address, expected.address,
            "the pool output is at the pool address"
        );
        assert_eq!(
            pool.datum, expected.datum,
            "the datum is inline and is the one that was written"
        );
        assert!(pool.assets.is_some(), "the output carries the state NFT");

        // The operator must sign, and the transaction must serialise.
        let signers = unsigned
            .staging
            .disclosed_signers
            .as_ref()
            .expect("signers");
        assert!(signers.to_vec().iter().any(|signer| signer.0 == *operator));
        unsigned.build_conway().expect("serialises");
    }

    #[test]
    fn a_script_that_does_not_hash_to_the_policy_is_refused() {
        let script = vec![0x46, 0x01, 0x00, 0x00, 0x00];
        let creation = PoolCreation {
            policy_script: script,
            policy_id: Hash::from([1u8; 28]),
            asset_name: b"\x00\x06\x43\xb0POOL".to_vec(),
            pool_address: Address::Shelley(pallas_addresses::ShelleyAddress::new(
                pallas_addresses::Network::Testnet,
                pallas_addresses::ShelleyPaymentPart::Key(Hash::from([9u8; 28])),
                pallas_addresses::ShelleyDelegationPart::Null,
            )),
            datum_cbor: state().to_cbor().expect("encodes"),
            seed: cardano_assets::UtxoApi {
                tx_hash: "aa".repeat(32),
                output_index: 0,
                lovelace: 5_000_000,
                assets: Vec::new(),
                tags: Vec::new(),
            },
            operator_pkh: Hash::from([7u8; 28]),
            pool_lovelace: None,
            ex_units: pallas_txbuilder::ExUnits {
                mem: 200_000,
                steps: 100_000_000,
            },
        };
        let deps = TxDeps {
            utxos: vec![creation.seed.clone()],
            params: crate::params::TxBuildParams::default(),
            from_address: creation.pool_address.clone(),
            network_id: 0,
        };

        assert!(matches!(
            create_pool(deps, &creation),
            Err(BondingError::Invalid(message)) if message.starts_with("policy id:")
        ));
    }

    #[test]
    fn the_pool_state_encoding_is_pinned() {
        let expected = format!(
            "d87987021904d25820{}185a1b0000018bcfe568001a3b9aca00581c{}",
            "ab".repeat(ROOT_LEN),
            "cd".repeat(KEY_HASH_LEN),
        );
        let hex = hex::encode(state().to_cbor().expect("encodes"));
        assert_eq!(hex, expected);
    }

    #[test]
    fn a_deposit_round_trips() {
        let bytes = deposit().to_cbor().expect("encodes");
        assert_eq!(Deposit::from_cbor(&bytes).expect("decodes"), deposit());
    }

    #[test]
    fn a_deposit_is_constr_one() {
        let data = deposit().to_data().expect("encodes");
        let PlutusData::Constr(constr) = &data else {
            panic!("not a Constr");
        };
        assert_eq!(constr.tag, 122, "Constr 1 is CBOR tag 122");
        assert_eq!(constr.fields.len(), DEPOSIT_FIELDS);
    }

    #[test]
    fn the_two_datums_cannot_be_read_as_each_other() {
        let pool_cbor = state().to_cbor().expect("encodes");
        let deposit_cbor = deposit().to_cbor().expect("encodes");

        assert!(
            Deposit::from_cbor(&pool_cbor).is_err(),
            "a pool state must not decode as a deposit"
        );
        assert!(
            PoolState::from_cbor(&deposit_cbor).is_err(),
            "a deposit must not decode as a pool state"
        );
    }

    #[test]
    fn a_bad_field_is_refused_by_name() {
        let mut bad_phase = state();
        bad_phase.phase = 3;
        assert!(matches!(
            bad_phase.to_cbor(),
            Err(BondingError::Invalid(message)) if message.starts_with("phase:")
        ));

        let mut bad_root = state();
        bad_root.root = vec![0; ROOT_LEN - 1];
        assert!(matches!(
            bad_root.to_cbor(),
            Err(BondingError::Invalid(message)) if message.starts_with("root:")
        ));

        let mut bad_operator = state();
        bad_operator.operator = vec![0; KEY_HASH_LEN + 1];
        assert!(matches!(
            bad_operator.to_cbor(),
            Err(BondingError::Invalid(message)) if message.starts_with("operator:")
        ));

        let mut bad_policy = deposit();
        bad_policy.policy = vec![0; POLICY_ID_LEN - 1];
        assert!(matches!(
            bad_policy.to_cbor(),
            Err(BondingError::Invalid(message)) if message.starts_with("policy:")
        ));

        let mut empty_name = deposit();
        empty_name.name = Vec::new();
        assert!(matches!(
            empty_name.to_cbor(),
            Err(BondingError::Invalid(message)) if message.starts_with("name:")
        ));
    }

    #[test]
    fn a_truncated_field_array_is_refused() {
        // Six fields where seven are expected: the shape check, not a positional
        // read that would silently shift every field.
        let six = constr(
            POOL_STATE_CONSTRUCTOR,
            vec![
                int(0),
                int(1),
                bytes(vec![0; ROOT_LEN]),
                int(0),
                int(0),
                int(0),
            ],
        );
        assert!(matches!(
            PoolState::from_data(&six),
            Err(BondingError::Invalid(message)) if message.contains("expected 7 fields")
        ));
    }
}
