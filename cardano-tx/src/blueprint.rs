//! Aiken blueprints (`plutus.json`) — parse one, prove its bytes against the
//! hash it declares, and derive the addresses a deployed validator lives at.
//!
//! The one fact everything here rests on: a blueprint's `compiledCode` IS the
//! byte string the ledger stores as the script, single-CBOR-wrapped (not the
//! double-wrapped text-envelope form), and the ledger's script hash is
//! `blake2b_224(language_tag || compiledCode)`. That was verified against
//! jpg.store's live validator: its blueprint bytes hash to `c727443d…` and
//! Koios reports the on-chain reference script at exactly that byte count.
//! So a blueprint whose bytes re-hash to its declared hash can be deployed as
//! those bytes and will land at the address the hash implies.
//!
//! Shared by `ncli deploy-ref-script` and the abandonware worker's ops
//! endpoint so the two can never disagree about what a blueprint means.

use pallas_addresses::{
    Address, Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
};
use pallas_crypto::hash::{Hash, Hasher};
use pallas_txbuilder::ScriptKind;
use serde::Deserialize;

/// An Aiken blueprint — only the fields deployment needs.
#[derive(Debug, Clone, Deserialize)]
pub struct Blueprint {
    pub preamble: BlueprintPreamble,
    pub validators: Vec<BlueprintValidator>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlueprintPreamble {
    #[serde(default)]
    pub title: String,
    #[serde(rename = "plutusVersion")]
    pub plutus_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlueprintValidator {
    pub title: String,
    #[serde(rename = "compiledCode")]
    pub compiled_code: String,
    pub hash: String,
}

/// The Plutus language a blueprint declares, with the byte the ledger
/// prefixes to the script when hashing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlutusLanguage {
    V1,
    V2,
    V3,
}

impl PlutusLanguage {
    pub const ALL: [PlutusLanguage; 3] =
        [PlutusLanguage::V1, PlutusLanguage::V2, PlutusLanguage::V3];

    /// From a blueprint preamble's `plutusVersion` (`"v1"`, `"v2"`, `"v3"`).
    pub fn parse(preamble: &str) -> Option<Self> {
        match preamble {
            "v1" => Some(PlutusLanguage::V1),
            "v2" => Some(PlutusLanguage::V2),
            "v3" => Some(PlutusLanguage::V3),
            _ => None,
        }
    }

    /// The tag byte in `blake2b_224(tag || script_bytes)`.
    pub fn hash_tag(self) -> u8 {
        match self {
            PlutusLanguage::V1 => 0x01,
            PlutusLanguage::V2 => 0x02,
            PlutusLanguage::V3 => 0x03,
        }
    }

    pub fn script_kind(self) -> ScriptKind {
        match self {
            PlutusLanguage::V1 => ScriptKind::PlutusV1,
            PlutusLanguage::V2 => ScriptKind::PlutusV2,
            PlutusLanguage::V3 => ScriptKind::PlutusV3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PlutusLanguage::V1 => "Plutus V1",
            PlutusLanguage::V2 => "Plutus V2",
            PlutusLanguage::V3 => "Plutus V3",
        }
    }
}

/// Why a blueprint could not be turned into a deployable script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueprintError {
    /// Not JSON, or not an Aiken blueprint.
    Parse(String),
    /// `plutusVersion` is not v1, v2 or v3.
    UnknownLanguage(String),
    /// No validator with that title; carries the titles present.
    NoSuchValidator {
        wanted: String,
        present: Vec<String>,
    },
    /// `compiledCode` is not hex.
    BadHex(String),
    /// The bytes do not hash to the declared hash under the declared
    /// language — these are not the bytes the chain would hash to that
    /// address, so nothing should be deployed from them.
    HashMismatch { declared: String, computed: String },
}

impl std::fmt::Display for BlueprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "not an Aiken blueprint: {e}"),
            Self::UnknownLanguage(v) => write!(f, "plutusVersion {v:?} is not v1, v2 or v3"),
            Self::NoSuchValidator { wanted, present } => {
                write!(f, "no validator titled {wanted:?}; found {present:?}")
            }
            Self::BadHex(e) => write!(f, "compiledCode is not hex: {e}"),
            Self::HashMismatch { declared, computed } => write!(
                f,
                "blueprint declares hash {declared} but its compiledCode hashes to {computed} — \
                 refusing to deploy bytes that do not match their declared hash"
            ),
        }
    }
}

impl std::error::Error for BlueprintError {}

/// A validator lifted out of a blueprint with its bytes proven against the
/// hash the blueprint declares.
#[derive(Debug, Clone)]
pub struct VerifiedScript {
    pub title: String,
    pub language: PlutusLanguage,
    pub bytes: Vec<u8>,
    pub hash: Hash<28>,
}

impl VerifiedScript {
    /// The script hash as lowercase hex — the form the address registry and
    /// Koios use.
    pub fn hash_hex(&self) -> String {
        self.hash.to_string()
    }

    /// The validator's own enterprise address on `network` — where listings
    /// sit and, for a permanent reference script, where the reference UTxO
    /// is parked (a datumless UTxO at a Plutus script address can never be
    /// spent).
    pub fn enterprise_address(&self, network: Network) -> Address {
        Address::Shelley(ShelleyAddress::new(
            network,
            ShelleyPaymentPart::Script(self.hash),
            ShelleyDelegationPart::Null,
        ))
    }
}

impl Blueprint {
    pub fn parse(json: &str) -> Result<Self, BlueprintError> {
        serde_json::from_str(json).map_err(|e| BlueprintError::Parse(e.to_string()))
    }

    pub fn language(&self) -> Result<PlutusLanguage, BlueprintError> {
        PlutusLanguage::parse(&self.preamble.plutus_version)
            .ok_or_else(|| BlueprintError::UnknownLanguage(self.preamble.plutus_version.clone()))
    }

    /// Lift the validator titled `title`, re-deriving its hash from its bytes
    /// and refusing if that disagrees with what the blueprint declares.
    pub fn verified_script(&self, title: &str) -> Result<VerifiedScript, BlueprintError> {
        let language = self.language()?;
        let validator = self
            .validators
            .iter()
            .find(|v| v.title == title)
            .ok_or_else(|| BlueprintError::NoSuchValidator {
                wanted: title.to_string(),
                present: self.validators.iter().map(|v| v.title.clone()).collect(),
            })?;
        let bytes = hex::decode(&validator.compiled_code)
            .map_err(|e| BlueprintError::BadHex(e.to_string()))?;
        let hash = script_hash(language, &bytes);
        if !hash.to_string().eq_ignore_ascii_case(&validator.hash) {
            return Err(BlueprintError::HashMismatch {
                declared: validator.hash.clone(),
                computed: hash.to_string(),
            });
        }
        Ok(VerifiedScript {
            title: validator.title.clone(),
            language,
            bytes,
            hash,
        })
    }
}

/// The ledger's hash of a Plutus script: blake2b-224 over the language tag
/// followed by the script bytes.
pub fn script_hash(language: PlutusLanguage, script_bytes: &[u8]) -> Hash<28> {
    let mut preimage = Vec::with_capacity(script_bytes.len() + 1);
    preimage.push(language.hash_tag());
    preimage.extend_from_slice(script_bytes);
    Hasher::<224>::hash(&preimage)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial Plutus V2 program (`(program 1.0.0 (con unit ()))`, flat,
    /// CBOR-wrapped) as a blueprint. The declared hash below was computed by
    /// this module; the test pins that a round-trip agrees and that a wrong
    /// declaration is refused.
    const CODE: &str = "4e4d01000033222220051200120011";

    fn blueprint(hash: &str) -> String {
        format!(
            r#"{{"preamble":{{"title":"t","plutusVersion":"v2"}},"validators":[{{"title":"ask.spend","compiledCode":"{CODE}","hash":"{hash}"}}]}}"#
        )
    }

    #[test]
    fn hash_is_blake2b224_over_tag_and_bytes() {
        let bytes = hex::decode(CODE).unwrap();
        let computed = script_hash(PlutusLanguage::V2, &bytes).to_string();
        let bp = Blueprint::parse(&blueprint(&computed)).unwrap();
        let script = bp.verified_script("ask.spend").unwrap();
        assert_eq!(script.hash_hex(), computed);
        assert_eq!(script.language, PlutusLanguage::V2);
        assert_eq!(script.bytes, bytes);
        // V1 tags differently, so the same bytes are a different script.
        assert_ne!(script_hash(PlutusLanguage::V1, &bytes), script.hash);
    }

    #[test]
    fn a_mismatched_declaration_is_refused() {
        let bp = Blueprint::parse(&blueprint(&"00".repeat(28))).unwrap();
        let err = bp.verified_script("ask.spend").unwrap_err();
        assert!(matches!(err, BlueprintError::HashMismatch { .. }), "{err}");
    }

    #[test]
    fn missing_validator_names_what_is_there() {
        let bp = Blueprint::parse(&blueprint(&"00".repeat(28))).unwrap();
        let err = bp.verified_script("bid.spend").unwrap_err();
        assert_eq!(
            err,
            BlueprintError::NoSuchValidator {
                wanted: "bid.spend".into(),
                present: vec!["ask.spend".into()],
            }
        );
    }

    #[test]
    fn enterprise_address_is_the_script_hash() {
        use pallas_addresses::Address;
        let bytes = hex::decode(CODE).unwrap();
        let hash = script_hash(PlutusLanguage::V2, &bytes);
        let bp = Blueprint::parse(&blueprint(&hash.to_string())).unwrap();
        let script = bp.verified_script("ask.spend").unwrap();
        let addr = script.enterprise_address(Network::Testnet);
        let bech = addr.to_bech32().unwrap();
        assert!(bech.starts_with("addr_test1w"), "{bech}");
        let Address::Shelley(sh) = Address::from_bech32(&bech).unwrap() else {
            panic!("shelley");
        };
        assert_eq!(sh.payment().to_hex(), hash.to_string());
    }
}
