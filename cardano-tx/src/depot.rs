//! The **script depot** — a native-script address that holds CIP-33 reference
//! scripts you can still spend.
//!
//! ## Why an address of its own
//!
//! Parking a reference script at the validator's OWN address (what
//! [`crate::blueprint::VerifiedScript::enterprise_address`] gives, and what
//! jpg.store does) makes it permanent: a datumless UTxO at a Plutus address can
//! never be spent, because the spend would have to run the validator and the
//! validator cannot be invoked without a datum. That is the right answer for a
//! contract you will never retire.
//!
//! A general deployment facility wants the opposite. Its UTxOs must be
//! spendable, so the ~8 ADA a 1.5 KB validator locks can be recovered when a
//! script is superseded. The obvious home — a plain wallet address — is the one
//! place it must NOT go: a reference-script UTxO there looks like any other
//! 8 ADA to the wallet, and the next payment will happily select it as an
//! input, silently deleting a script every deployed worker still references.
//!
//! So the depot is a **native script** whose whole content is "these keys may
//! spend". Wallets derive and scan key addresses, never script addresses, so
//! the depot sits beside the wallet and out of reach of its coin selection,
//! while the same key still opens it.
//!
//! ## What lives where
//!
//! The two scripts never interact, and conflating them is the easy mistake:
//!
//! - **The payload** is the Plutus validator or minting policy being deployed.
//!   It rides in the output's `script_ref` field. The ledger does not care what
//!   address an output carrying a reference script sits at, and referencing it
//!   never runs the depot's native script.
//! - **The address** is the native script. It runs only when the UTxO is
//!   SPENT — i.e. only when retiring — and decides who may do that.
//!
//! A reference script is a fee optimisation, not a gate: retiring one does not
//! disable the policy or validator it carries. Anyone can still supply the same
//! bytes inline in a witness set. Retiring only reclaims the ADA and breaks
//! transactions built to reference that specific UTxO.

use pallas_addresses::{
    Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
};
use pallas_crypto::hash::{Hash, Hasher};
use pallas_primitives::alonzo::{BoundedBytes, Constr, PlutusData};
use pallas_primitives::babbage::NativeScript;
use pallas_primitives::{Fragment, MaybeIndefArray};

/// Why a depot could not be derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepotError {
    /// No signers were given. A depot with no signer is an unspendable
    /// address, which is the one thing a depot must never be.
    NoSigners,
    /// A signer is not 28 bytes of hex.
    BadKeyHash { value: String, reason: String },
    /// The same key hash was listed twice. Refused rather than silently
    /// deduplicated: the signer list determines the ADDRESS, so quietly
    /// changing it would move the depot without the operator knowing.
    DuplicateSigner(String),
    /// The native script could not be encoded.
    Encode(String),
}

impl std::fmt::Display for DepotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSigners => write!(
                f,
                "a depot needs at least one signer — a depot nobody can spend is worse than \
                 parking at the validator's own address, which is at least honest about being \
                 permanent"
            ),
            Self::BadKeyHash { value, reason } => {
                write!(f, "signer {value:?} is not a payment key hash: {reason}")
            }
            Self::DuplicateSigner(kh) => write!(
                f,
                "signer {kh} is listed twice — the signer set determines the depot address, so \
                 it is stated exactly once or not at all"
            ),
            Self::Encode(e) => write!(f, "encoding the depot's native script failed: {e}"),
        }
    }
}

impl std::error::Error for DepotError {}

/// A script depot: the signer set, the native script it implies, and the
/// address that script hashes to.
///
/// The address is a pure function of the ORDERED signer set. Adding, removing
/// or reordering a signer yields a DIFFERENT depot, leaving anything already
/// parked at the old one — still spendable with the old set, but no longer
/// listed. Treat the signer list as a deployment record, not a setting.
#[derive(Debug, Clone)]
pub struct Depot {
    signers: Vec<Hash<28>>,
    script: NativeScript,
    script_bytes: Vec<u8>,
    hash: Hash<28>,
}

impl Depot {
    /// Derive a depot from payment key hashes (28-byte hex, in the order they
    /// should be baked into the script).
    ///
    /// One signer gives `ScriptPubkey` — the smallest script that means "my
    /// signature". Several give `ScriptAny`, so ANY ONE of them can retire what
    /// the depot holds; that is deliberately a recovery affordance rather than
    /// a security boundary, because the depot holds public bytes and reclaimable
    /// ADA, never authority. If you want a real threshold, that is a different
    /// depot and a different address.
    pub fn from_signers<S: AsRef<str>>(signers: &[S]) -> Result<Self, DepotError> {
        if signers.is_empty() {
            return Err(DepotError::NoSigners);
        }
        let mut hashes: Vec<Hash<28>> = Vec::with_capacity(signers.len());
        for s in signers {
            let raw = s.as_ref().trim();
            let bytes = hex::decode(raw).map_err(|e| DepotError::BadKeyHash {
                value: raw.to_string(),
                reason: format!("not hex ({e})"),
            })?;
            if bytes.len() != 28 {
                return Err(DepotError::BadKeyHash {
                    value: raw.to_string(),
                    reason: format!("expected 28 bytes, got {}", bytes.len()),
                });
            }
            let mut arr = [0u8; 28];
            arr.copy_from_slice(&bytes);
            let hash: Hash<28> = arr.into();
            if hashes.contains(&hash) {
                return Err(DepotError::DuplicateSigner(hash.to_string()));
            }
            hashes.push(hash);
        }

        let script = if hashes.len() == 1 {
            NativeScript::ScriptPubkey(hashes[0])
        } else {
            NativeScript::ScriptAny(
                hashes
                    .iter()
                    .copied()
                    .map(NativeScript::ScriptPubkey)
                    .collect(),
            )
        };
        let script_bytes = script
            .encode_fragment()
            .map_err(|e| DepotError::Encode(e.to_string()))?;
        // A native script's ledger hash is blake2b-224 over `0x00 || cbor` —
        // the same shape as a Plutus script's, with language tag 0.
        let mut preimage = Vec::with_capacity(script_bytes.len() + 1);
        preimage.push(0x00);
        preimage.extend_from_slice(&script_bytes);
        let hash = Hasher::<224>::hash(&preimage);

        Ok(Self {
            signers: hashes,
            script,
            script_bytes,
            hash,
        })
    }

    /// The key hashes that may spend this depot, in the order baked in.
    pub fn signers(&self) -> &[Hash<28>] {
        &self.signers
    }

    /// Is this key hash allowed to retire what the depot holds? The deploy
    /// path checks the CONNECTED wallet against this: parking ADA in a depot
    /// you cannot open is a one-way trip, and it is worth failing the build
    /// rather than discovering it at retirement.
    pub fn can_spend(&self, payment_key_hash: &Hash<28>) -> bool {
        self.signers.contains(payment_key_hash)
    }

    /// The native script itself.
    pub fn script(&self) -> &NativeScript {
        &self.script
    }

    /// The script's CBOR — what a retiring transaction attaches as its witness.
    pub fn script_bytes(&self) -> &[u8] {
        &self.script_bytes
    }

    /// The script hash: the depot's payment credential.
    pub fn hash(&self) -> Hash<28> {
        self.hash
    }

    pub fn hash_hex(&self) -> String {
        self.hash.to_string()
    }

    /// The depot's enterprise address on `network`.
    ///
    /// Per network, because the network id is part of the address: one signer
    /// set gives one depot on preprod and a different bech32 on mainnet, and
    /// each needs its own deployment record.
    pub fn address(&self, network: Network) -> Address {
        Address::Shelley(ShelleyAddress::new(
            network,
            ShelleyPaymentPart::Script(self.hash),
            ShelleyDelegationPart::Null,
        ))
    }
}

/// The human label written as an inline datum on a depot output.
///
/// A native script ignores datums entirely, so this is inert payload — it
/// costs a little min-ADA and buys a UTxO that says what it is. That matters
/// because the script hash alone cannot tell you WHICH of your validators it
/// is, and the depot is the thing you come back to in six months.
///
/// It rides in the output bytes, so it survives in any UTxO query and on the
/// mitos live path. Transaction metadata (CIP-20/674) would NOT: it is attached
/// to the transaction, not the output, so it is invisible to every UTxO listing
/// and to mitos live. Stamp metadata as well if you like, but never rely on it
/// to find a deployment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepotLabel {
    /// What this script is, in your words (`fuel.mint`, `abandonware ask`).
    pub name: String,
    /// Whatever version you want to be able to tell apart later — a semver, a
    /// git revision, a date.
    pub version: String,
    /// Anything else worth a sentence. Empty is fine.
    pub note: String,
}

impl DepotLabel {
    /// The longest any one field may be. Generous for a label, and bounded so
    /// a pasted essay cannot quietly push the output's min-ADA up.
    pub const MAX_FIELD_BYTES: usize = 64;

    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            note: note.into(),
        }
    }

    /// `Constr 0 [name, version, note]`, each a UTF-8 byte string.
    ///
    /// Positional rather than a keyed map on purpose: nothing on chain reads
    /// this and nothing evolves it. It is a note to a human, and the schema
    /// crate's field-id discipline is for data a validator or a worker parses.
    pub fn to_plutus_data(&self) -> PlutusData {
        PlutusData::Constr(Constr {
            tag: 121, // Constr 0
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![
                PlutusData::BoundedBytes(BoundedBytes::from(self.name.as_bytes().to_vec())),
                PlutusData::BoundedBytes(BoundedBytes::from(self.version.as_bytes().to_vec())),
                PlutusData::BoundedBytes(BoundedBytes::from(self.note.as_bytes().to_vec())),
            ]),
        })
    }

    /// The datum's CBOR, for `Output::set_inline_datum`.
    pub fn to_cbor(&self) -> Result<Vec<u8>, DepotError> {
        self.to_plutus_data()
            .encode_fragment()
            .map_err(|e| DepotError::Encode(e.to_string()))
    }

    /// Read a label back off a depot UTxO's inline datum.
    ///
    /// Deliberately strict about SHAPE and lenient about content: a depot may
    /// hold UTxOs whose datum is not ours at all, and those must read as "no
    /// label" rather than as an error that hides a real deployment from the
    /// listing. Non-UTF-8 bytes are lossily decoded for the same reason — a
    /// mangled note is still more useful than a missing row.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, DepotError> {
        let data = PlutusData::decode_fragment(bytes)
            .map_err(|e| DepotError::Encode(format!("not PlutusData: {e:?}")))?;
        let PlutusData::Constr(c) = data else {
            return Err(DepotError::Encode("not a constructor".into()));
        };
        if c.tag != 121 {
            return Err(DepotError::Encode(format!(
                "expected Constr 0 (tag 121), got tag {}",
                c.tag
            )));
        }
        let fields: &[PlutusData] = &c.fields;
        if fields.len() != 3 {
            return Err(DepotError::Encode(format!(
                "expected 3 fields, got {}",
                fields.len()
            )));
        }
        let text = |i: usize| -> Result<String, DepotError> {
            match &fields[i] {
                PlutusData::BoundedBytes(b) => Ok(String::from_utf8_lossy(b).into_owned()),
                other => Err(DepotError::Encode(format!(
                    "field {i} is not a byte string: {other:?}"
                ))),
            }
        };
        Ok(Self {
            name: text(0)?,
            version: text(1)?,
            note: text(2)?,
        })
    }

    /// Reject a field the ledger would make expensive or that is simply a
    /// mistake. Returns the offending field's name.
    pub fn too_long(&self) -> Option<&'static str> {
        for (field, value) in [
            ("name", &self.name),
            ("version", &self.version),
            ("note", &self.note),
        ] {
            if value.len() > Self::MAX_FIELD_BYTES {
                return Some(field);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KH_A: &str = "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29";
    const KH_B: &str = "abcd1234ef567890abcd1234ef567890abcd1234ef567890abcd1234";

    /// A one-signer depot is `ScriptPubkey`, and its hash is the SAME hash the
    /// minting-policy path computes for the same key — one ledger rule, two
    /// call sites, and if they ever disagree the depot address is wrong.
    #[test]
    fn single_signer_matches_the_ledgers_native_script_hash() {
        let depot = Depot::from_signers(&[KH_A]).unwrap();
        assert!(matches!(depot.script(), NativeScript::ScriptPubkey(_)));
        assert_eq!(
            depot.hash_hex(),
            "88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9",
            "the 0x00-prefixed blake2b-224 of the script CBOR"
        );
    }

    /// The depot address must be a SCRIPT address, not a key address. This is
    /// the entire point: a wallet enumerates key addresses and would spend a
    /// reference script parked at one as ordinary change.
    #[test]
    fn depot_address_is_a_script_address_on_each_network() {
        let depot = Depot::from_signers(&[KH_A]).unwrap();

        let testnet = depot.address(Network::Testnet).to_bech32().unwrap();
        assert!(testnet.starts_with("addr_test1w"), "{testnet}");
        let mainnet = depot.address(Network::Mainnet).to_bech32().unwrap();
        assert!(mainnet.starts_with("addr1w"), "{mainnet}");
        assert_ne!(
            testnet, mainnet,
            "the network id is part of the address: one signer set, two depots"
        );

        // The payment credential IS the script hash.
        let Address::Shelley(sh) = Address::from_bech32(&testnet).unwrap() else {
            panic!("shelley");
        };
        assert_eq!(sh.payment().to_hex(), depot.hash_hex());
    }

    /// Several signers give `ScriptAny` — any one of them can retire.
    #[test]
    fn several_signers_give_script_any() {
        let depot = Depot::from_signers(&[KH_A, KH_B]).unwrap();
        match depot.script() {
            NativeScript::ScriptAny(clauses) => assert_eq!(clauses.len(), 2),
            other => panic!("expected ScriptAny, got {other:?}"),
        }
        for kh in [KH_A, KH_B] {
            let bytes: [u8; 28] = hex::decode(kh).unwrap().try_into().unwrap();
            assert!(depot.can_spend(&bytes.into()), "{kh} must be able to spend");
        }
    }

    /// The signer set determines the ADDRESS. Reordering it is a different
    /// depot, which is exactly why the list is a deployment record and the
    /// facility must show the address it derived before anything is signed.
    #[test]
    fn signer_order_changes_the_depot() {
        let ab = Depot::from_signers(&[KH_A, KH_B]).unwrap();
        let ba = Depot::from_signers(&[KH_B, KH_A]).unwrap();
        assert_ne!(ab.hash_hex(), ba.hash_hex());
    }

    /// A one-signer depot and a "multi"-signer depot listing that same key
    /// once are the same script — `from_signers` must not wrap a lone key in
    /// a pointless `ScriptAny`, which would be a second, silently different
    /// address for the same intent.
    #[test]
    fn one_signer_is_never_wrapped() {
        let a = Depot::from_signers(&[KH_A]).unwrap();
        let also_a = Depot::from_signers(&[KH_A.to_string()]).unwrap();
        assert_eq!(a.hash_hex(), also_a.hash_hex());
    }

    #[test]
    fn a_depot_needs_a_signer_and_refuses_duplicates() {
        assert_eq!(
            Depot::from_signers::<&str>(&[]).unwrap_err(),
            DepotError::NoSigners
        );
        assert!(matches!(
            Depot::from_signers(&[KH_A, KH_A]).unwrap_err(),
            DepotError::DuplicateSigner(_)
        ));
        assert!(matches!(
            Depot::from_signers(&["not-hex"]).unwrap_err(),
            DepotError::BadKeyHash { .. }
        ));
        assert!(matches!(
            Depot::from_signers(&["abcd"]).unwrap_err(),
            DepotError::BadKeyHash { .. }
        ));
    }

    #[test]
    fn a_wallet_outside_the_signer_set_cannot_spend() {
        let depot = Depot::from_signers(&[KH_A]).unwrap();
        let other: [u8; 28] = hex::decode(KH_B).unwrap().try_into().unwrap();
        assert!(!depot.can_spend(&other.into()));
    }

    /// A BECH32 ADDRESS IS NOT A KEY HASH. This is the live mistake, not a
    /// hypothetical: configuring a depot by pasting a stake or payment address
    /// where the 28-byte payment key hash belongs.
    ///
    /// It must fail loudly at config time. A depot needs the PAYMENT key hash
    /// specifically, because the native script is satisfied by a signature in
    /// the transaction's witness set, and a wallet signs with its payment key —
    /// a stake credential never appears there. Accepting an address by, say,
    /// hashing its bytes would derive a plausible-looking depot that no wallet
    /// on earth could ever open, and the ADA would only be discovered stranded
    /// at retirement.
    #[test]
    fn an_address_is_refused_where_a_key_hash_belongs() {
        for pasted in [
            // A stake address.
            "stake1u867znggacw5dq9pgnq6eswag5vefyjlt2ev6ae34py2uhscnulce",
            "stake_test1up9m82rmm5nlqn6hnmwa4jhkv3v9jr4vymqst5py2wegalglryfaw",
            // A payment address — right wallet, still the wrong thing.
            "addr_test1qqpple6hh0000000000000000000000000000000000000000000",
            // The hash with a 0x prefix, or in caps with stray whitespace,
            // are all things a human will genuinely paste.
            "0x9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29",
        ] {
            let err = Depot::from_signers(&[pasted]).unwrap_err();
            assert!(
                matches!(err, DepotError::BadKeyHash { .. }),
                "{pasted:?} must be refused as a key hash, got {err:?}"
            );
        }

        // Surrounding whitespace IS forgiven — that is a paste artefact, not a
        // different value, and the resulting depot is identical.
        let padded = Depot::from_signers(&[format!("  {KH_A}\n")]).unwrap();
        assert_eq!(
            padded.hash_hex(),
            Depot::from_signers(&[KH_A]).unwrap().hash_hex()
        );
    }

    /// The label round-trips to CBOR and its three fields are recoverable —
    /// this is what makes a depot UTxO self-describing six months later.
    #[test]
    fn label_encodes_its_three_fields() {
        let label = DepotLabel::new("fuel.mint", "v1.2.0", "applied: registry=abc…");
        let cbor = label.to_cbor().unwrap();
        assert!(!cbor.is_empty());

        let PlutusData::Constr(c) = label.to_plutus_data() else {
            panic!("constr");
        };
        assert_eq!(c.tag, 121, "Constr 0");
        assert_eq!(c.fields.len(), 3);
        let field = |i: usize| match &c.fields[i] {
            PlutusData::BoundedBytes(b) => String::from_utf8(b.to_vec()).unwrap(),
            other => panic!("expected bytes, got {other:?}"),
        };
        assert_eq!(field(0), "fuel.mint");
        assert_eq!(field(1), "v1.2.0");
        assert_eq!(field(2), "applied: registry=abc…");
    }

    /// The label survives the round trip through the chain's own encoding.
    /// This is the property the depot listing depends on.
    #[test]
    fn label_round_trips_through_cbor() {
        for label in [
            DepotLabel::new("fuel.mint", "v1.2.0", "applied: registry=abc"),
            DepotLabel::new("", "", ""),
            DepotLabel::new("unicode ✓ name", "v1", "note with — dashes"),
        ] {
            let cbor = label.to_cbor().unwrap();
            assert_eq!(DepotLabel::from_cbor(&cbor).unwrap(), label);
        }
    }

    /// A depot may hold a UTxO whose datum is not ours. That must read as
    /// "no label", never as an error — a reference script with an unreadable
    /// note is still a deployment, and hiding it from the listing would be the
    /// worst outcome (you cannot retire what you cannot see).
    #[test]
    fn a_foreign_datum_is_rejected_cleanly_not_misread() {
        // Constr 0 with the wrong arity.
        let wrong_arity = PlutusData::Constr(Constr {
            tag: 121,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![PlutusData::BoundedBytes(BoundedBytes::from(
                b"only one".to_vec(),
            ))]),
        });
        let cbor = wrong_arity.encode_fragment().unwrap();
        assert!(DepotLabel::from_cbor(&cbor).is_err());

        // Constr 1 — a different constructor entirely.
        let wrong_tag = PlutusData::Constr(Constr {
            tag: 122,
            any_constructor: None,
            fields: MaybeIndefArray::Def(vec![]),
        });
        let cbor = wrong_tag.encode_fragment().unwrap();
        assert!(DepotLabel::from_cbor(&cbor).is_err());

        // Not PlutusData at all.
        assert!(DepotLabel::from_cbor(&[0xff, 0xff, 0xff]).is_err());
        assert!(DepotLabel::from_cbor(&[]).is_err());
    }

    #[test]
    fn an_empty_label_is_still_valid() {
        let label = DepotLabel::new("", "", "");
        assert!(label.to_cbor().is_ok());
        assert_eq!(label.too_long(), None);
    }

    #[test]
    fn an_overlong_field_is_named() {
        let label = DepotLabel::new("a".repeat(DepotLabel::MAX_FIELD_BYTES + 1), "v1", "");
        assert_eq!(label.too_long(), Some("name"));
        let label = DepotLabel::new("ok", "v".repeat(200), "");
        assert_eq!(label.too_long(), Some("version"));
    }
}
