//! CAIP-19 asset identity — *which asset*, on the chain CAIP-2 already names.
//!
//! [`ChainRef`] answers "which chain". This answers "which thing on it", in the
//! same family of standard, so a reference is one opaque string on the wire and a
//! structure in code:
//!
//! ```text
//! eip155:4663/erc721:0x7980aa64093853cb78c927e05b88fed96e945f81/1234
//! cardano:mainnet/cnt:9a4b1e2c…/4f4e4654
//! ```
//!
//! ## Why a standard rather than our own shape
//!
//! The property this has to have — that a token reference *embeds* its collection,
//! so deriving one from the other is a prefix strip — is the property Cardano's
//! own asset unit already has, and it is what CAIP-19 encodes: `chain_id` then
//! `asset_namespace:asset_reference` then an optional `token_id`. Inventing a
//! second spelling for the same idea would be the drift this crate exists to undo.
//!
//! ## Not through [`ChainRef`]
//!
//! [`ChainRef::from_str`] rejects a third `:`, deliberately, because that is
//! CAIP-10 (`namespace:reference:account`) rather than CAIP-2. So the chain half is
//! split off at the first `/` before it is parsed — the two never see each other's
//! separator.
//!
//! ## What is *not* verified here
//!
//! CAIP-19 is at status Review. The syntax and the charset below are read from the
//! specification; the per-namespace profiles it points at are not consulted, and
//! `cnt` is our own choice — CAIP-19 has no registered Cardano profile.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{ChainRef, ChainRefError};

/// CAIP-19's ceiling on an asset reference.
const MAX_REFERENCE_LEN: usize = 128;

/// CAIP-19's ceiling on a token id.
const MAX_TOKEN_ID_LEN: usize = 78;

/// The standard an asset reference is drawn from — `erc721`, `cnt`, `slip44`.
///
/// A newtype rather than an enum, because CAIP-19 defines the *syntax* and leaves
/// the namespace open to per-name standards. Naming only the two we use would mean
/// refusing the rest, and `erc20` on a chain we do not serve is not an error — it
/// is simply not ours.
///
/// The two constants are the namespaces this workspace writes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AssetNamespace(String);

impl AssetNamespace {
    /// ERC-721 — the NFT standard on EVM chains.
    pub const ERC721: &'static str = "erc721";

    /// A Cardano native token.
    ///
    /// Named for the ledger concept rather than for CIP-25 or CIP-68, because it
    /// has to cover both: the same collection can hold either, and a namespace
    /// named after one of them would mis-describe the other.
    pub const CNT: &'static str = "cnt";

    /// The namespace as written in a reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AssetNamespace {
    type Err = AssetRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `[-a-z0-9]{3,8}` — the lowercase-only rule is the specification's, and
        // it is what makes `erd` and `ERC721` invalid rather than merely unusual.
        let shaped = (3..=8).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !shaped {
            return Err(AssetRefError::BadNamespace(s.to_string()));
        }
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for AssetNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for AssetNamespace {
    type Error = AssetRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<AssetNamespace> for String {
    fn from(value: AssetNamespace) -> Self {
        value.0
    }
}

/// A chain-qualified asset *type* — which collection, on which chain.
///
/// This is decision 1's chain-qualified collection id as a type: the collection
/// half of a [`TokenRef`], and on its own the identity of a collection rather than
/// a token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AssetType {
    chain: ChainRef,
    namespace: AssetNamespace,
    reference: String,
}

impl AssetType {
    /// Which chain the asset is on.
    pub const fn chain(&self) -> ChainRef {
        self.chain
    }

    /// Which standard its reference is drawn from.
    pub fn namespace(&self) -> &str {
        self.namespace.as_str()
    }

    /// The collection: an EVM contract address, or a Cardano policy id.
    ///
    /// Case is preserved — CAIP-19 does not require canonicalisation, and an
    /// EIP-55 checksummed address is information nothing here should discard.
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Builds a reference, validating through [`AssetType::from_str`].
    ///
    /// The string is assembled and parsed rather than the fields checked here,
    /// because one parser is what stops two from disagreeing — the same reason
    /// [`ChainRef`] delegates to its own.
    pub fn new(chain: ChainRef, namespace: &str, reference: &str) -> Result<Self, AssetRefError> {
        let chain = chain.as_caip2();
        format!("{chain}/{namespace}:{reference}").parse()
    }
}

impl fmt::Display for AssetType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}/{}:{}",
            self.chain.as_caip2(),
            self.namespace,
            self.reference
        )
    }
}

impl FromStr for AssetType {
    type Err = AssetRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (chain, namespace, reference, rest) = split_reference(s)?;
        if rest.is_some() {
            return Err(AssetRefError::TooManyParts(s.to_string()));
        }
        Ok(Self {
            chain,
            namespace,
            reference,
        })
    }
}

impl TryFrom<String> for AssetType {
    type Error = AssetRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<AssetType> for String {
    fn from(value: AssetType) -> String {
        value.to_string()
    }
}

/// A chain-qualified asset, addressed to one token.
///
/// CAIP-19's asset ID. The token id is optional, so this also parses the bare
/// asset type — an address that names a collection but no particular token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TokenRef {
    asset_type: AssetType,
    token_id: Option<String>,
}

impl TokenRef {
    /// The collection this token belongs to.
    pub const fn asset_type(&self) -> &AssetType {
        &self.asset_type
    }

    /// Which chain the token is on.
    pub const fn chain(&self) -> ChainRef {
        self.asset_type.chain()
    }

    /// The token's id — an ERC-721 `uint256` as a decimal string, which passes
    /// `u64` and is kept as text rather than narrowed.
    pub fn token_id(&self) -> Option<&str> {
        self.token_id.as_deref()
    }

    /// Builds a reference, validating through [`TokenRef::from_str`].
    pub fn new(asset_type: AssetType, token_id: Option<&str>) -> Result<Self, AssetRefError> {
        match token_id {
            Some(token_id) => format!("{asset_type}/{token_id}").parse(),
            None => asset_type.to_string().parse(),
        }
    }
}

impl fmt::Display for TokenRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.asset_type)?;
        if let Some(token_id) = &self.token_id {
            write!(formatter, "/{token_id}")?;
        }
        Ok(())
    }
}

impl FromStr for TokenRef {
    type Err = AssetRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (chain, namespace, reference, token_id) = split_reference(s)?;
        Ok(Self {
            asset_type: AssetType {
                chain,
                namespace,
                reference,
            },
            token_id,
        })
    }
}

impl TryFrom<String> for TokenRef {
    type Error = AssetRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<TokenRef> for String {
    fn from(value: TokenRef) -> String {
        value.to_string()
    }
}

/// Splits `chain_id/namespace:reference[/token_id]` into its four parts.
///
/// One function for both parsers, so they cannot disagree about what a reference
/// is — the same reason [`ChainRef::from_str`] has one parser.
fn split_reference(
    s: &str,
) -> Result<(ChainRef, AssetNamespace, String, Option<String>), AssetRefError> {
    let mut parts = s.split('/');

    let Some(chain_part) = parts.next() else {
        return Err(AssetRefError::NotAssetRef(s.to_string()));
    };
    let Some(assert_part) = parts.next() else {
        return Err(AssetRefError::NotAssetRef(s.to_string()));
    };
    let token_part = parts.next();
    if parts.next().is_some() {
        return Err(AssetRefError::TooManyParts(s.to_string()));
    }

    let chain: ChainRef = chain_part.parse()?;

    let Some((namespace, reference)) = assert_part.split_once(':') else {
        return Err(AssetRefError::NotAssetRef(s.to_string()));
    };
    let namespace: AssetNamespace = namespace.parse()?;
    check_reference(reference)?;
    let reference = reference.to_string();

    let token_id = match token_part {
        Some(token) => {
            check_token_id(token)?;
            Some(token.to_string())
        }
        None => None,
    };

    Ok((chain, namespace, reference, token_id))
}

/// CAIP-19 allows `-`, `.`, `%` and alphanumerics in a reference, and nothing
/// else — which is what makes a `/` an unambiguous separator.
fn is_reference_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '%')
}

fn check_reference(reference: &str) -> Result<(), AssetRefError> {
    let shaped = !reference.is_empty()
        && reference.len() <= MAX_REFERENCE_LEN
        && reference.chars().all(is_reference_char);
    if shaped {
        Ok(())
    } else {
        Err(AssetRefError::BadReference(reference.to_string()))
    }
}

fn check_token_id(token_id: &str) -> Result<(), AssetRefError> {
    let shaped = !token_id.is_empty()
        && token_id.len() <= MAX_TOKEN_ID_LEN
        && token_id.chars().all(is_reference_char);
    if shaped {
        Ok(())
    } else {
        Err(AssetRefError::BadTokenId(token_id.to_string()))
    }
}

/// Why a string is not a CAIP-19 reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetRefError {
    /// Not `chain/namespace:reference`, or missing a part of it.
    NotAssetRef(String),
    /// More `/`-separated parts than a reference and an optional token id.
    TooManyParts(String),
    /// The chain half is not a CAIP-2 chain we can name.
    Chain(ChainRefError),
    /// The namespace half is not `[-a-z0-9]{3,8}`.
    BadNamespace(String),
    /// The asset reference is empty, over-long, or uses a reserved character.
    BadReference(String),
    /// The token id is empty, over-long, or uses a reserved character.
    BadTokenId(String),
}

impl fmt::Display for AssetRefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAssetRef(value) => {
                write!(
                    formatter,
                    "not an asset reference: {value:?} (expected chain/namespace:reference[/token])"
                )
            }
            Self::TooManyParts(value) => {
                write!(
                    formatter,
                    "too many parts for an asset reference: {value:?}"
                )
            }
            Self::Chain(error) => write!(formatter, "chain: {error}"),
            Self::BadNamespace(value) => {
                write!(
                    formatter,
                    "not an asset namespace: {value:?} (expected [-a-z0-9]{{3,8}})"
                )
            }
            Self::BadReference(value) => {
                write!(formatter, "not an asset reference: {value:?}")
            }
            Self::BadTokenId(value) => write!(formatter, "not a token id: {value:?}"),
        }
    }
}

impl std::error::Error for AssetRefError {}

impl From<ChainRefError> for AssetRefError {
    fn from(error: ChainRefError) -> Self {
        Self::Chain(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MADJACKET: &str = "eip155:4663/erc721:0x7980aa64093853cb78c927e05b88fed96e945f81";

    #[test]
    fn the_specifications_own_examples_parse() {
        // Straight from CAIP-19's test cases, so the parser is checked against
        // the standard rather than against our own spelling of it.
        for example in [
            "eip155:1/erc721:0x06012c8cf97BEaD5deAe237070F9587f8E7A266d",
            "eip155:1/erc721:0x06012c8cf97BEaD5deAe237070F9587f8E7A266d/771769",
            "eip155:1/erc20:0x6b175474e89094c44da98b954eedeac495271d0f",
            "eip155:1/slip44:60",
        ] {
            let parsed: TokenRef = example.parse().expect("should parse");
            assert_eq!(parsed.to_string(), example);
        }
    }

    #[test]
    fn an_evm_token_reference_round_trips() {
        let parsed: TokenRef = format!("{MADJACKET}/1234").parse().expect("should parse");

        assert_eq!(parsed.chain(), ChainRef::from_str("eip155:4663").unwrap());
        assert_eq!(parsed.asset_type().namespace(), AssetNamespace::ERC721);
        assert_eq!(
            parsed.asset_type().reference(),
            "0x7980aa64093853cb78c927e05b88fed96e945f81"
        );
        assert_eq!(parsed.token_id(), Some("1234"));
        assert_eq!(parsed.to_string(), format!("{MADJACKET}/1234"));
    }

    #[test]
    fn the_collection_half_parses_on_its_own() {
        let parsed: AssetType = MADJACKET.parse().expect("should parse");
        assert_eq!(parsed.chain().as_caip2(), "eip155:4663");
        assert_eq!(parsed.to_string(), MADJACKET);

        // A bare asset type is also a valid reference, with no token addressed.
        let as_ref: TokenRef = MADJACKET.parse().expect("should parse");
        assert_eq!(as_ref.token_id(), None);
    }

    #[test]
    fn a_cardano_collection_is_the_same_shape() {
        let policy = "9a4b1e2c0d5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d";
        let reference = format!("cardano:mainnet/cnt:{policy}/4f4e4654");
        let parsed: TokenRef = reference.parse().expect("should parse");

        assert_eq!(parsed.chain().as_caip2(), "cardano:mainnet");
        assert_eq!(parsed.asset_type().namespace(), AssetNamespace::CNT);
        assert_eq!(parsed.asset_type().reference(), policy);
        assert_eq!(parsed.token_id(), Some("4f4e4654"));
        assert_eq!(parsed.to_string(), reference);
    }

    #[test]
    fn a_checksummed_address_keeps_its_case() {
        // CAIP-19 does not require canonicalisation, so EIP-55 capitalisation is
        // information rather than noise — lowercasing it would be a lossy parse.
        let reference = "eip155:1/erc721:0x06012c8cf97BEaD5deAe237070F9587f8E7A266d/1";
        let parsed: TokenRef = reference.parse().expect("should parse");
        assert!(
            parsed
                .to_string()
                .contains("97BEaD5deAe237070F9587f8E7A266d")
        );
    }

    #[test]
    fn a_reference_to_a_chain_we_cannot_name_is_refused() {
        // CAIP-19's own Hedera example is syntactically valid and still refused:
        // `chains` never guesses a chain it cannot name, and this crate inherits
        // that rather than weakening it.
        let error = "hedera:mainnet/nft:0.0.55492/12"
            .parse::<TokenRef>()
            .expect_err("should be refused");
        assert!(matches!(error, AssetRefError::Chain(_)));
    }

    #[test]
    fn a_chain_qualified_account_is_not_an_asset_reference() {
        // CAIP-10 is `namespace:reference:account`. Reading it as an asset would
        // make an account look like an asset type, and nothing here should.
        assert!(
            "eip155:1:0x6b175474e89094c44da98b954eedeac495271d0f"
                .parse::<TokenRef>()
                .is_err()
        );
    }

    #[test]
    fn malformed_references_are_refused() {
        for bad in [
            "eip155:1",                  // no asset half
            "eip155:1/erc721",           // no reference
            "eip155:1/erc721:",          // empty reference
            "eip155:1/ERC721:0xabc",     // namespace is lower-case only
            "eip155:1/er:0xabc",         // namespace too short
            "eip155:1/erc721:0xabc/1/2", // a second token id
            "eip155:1/erc721:0xabc:1",   // ':' inside the reference
        ] {
            assert!(
                bad.parse::<TokenRef>().is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn a_reference_can_be_built_as_well_as_parsed() {
        let chain = ChainRef::from_str("eip155:4663").unwrap();
        let built = TokenRef::new(
            AssetType::new(chain, AssetNamespace::ERC721, "0xabc").unwrap(),
            Some("7"),
        )
        .expect("should build");

        assert_eq!(built.to_string(), "eip155:4663/erc721:0xabc/7");
    }

    #[test]
    fn building_rejects_what_parsing_rejects() {
        let chain = ChainRef::from_str("eip155:4663").unwrap();
        assert!(AssetType::new(chain, "ERC721", "0xabc").is_err());
        assert!(AssetType::new(chain, AssetNamespace::ERC721, "0xabc/1").is_err());
    }

    #[test]
    fn a_reference_round_trips_through_serde_as_one_string() {
        let reference = format!("{MADJACKET}/1234");
        let parsed: TokenRef = reference.parse().expect("should parse");

        let json = serde_json::to_string(&parsed).expect("should serialize");
        assert_eq!(json, format!("\"{reference}\""));

        let read_back: TokenRef = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(read_back, parsed);
    }

    #[test]
    fn an_unparseable_string_fails_to_deserialize_rather_than_defaulting() {
        // A persisted column read back malformed must be an error, not a
        // plausible-looking default — the failure this crate exists to prevent.
        assert!(serde_json::from_str::<TokenRef>("\"not-a-reference\"").is_err());
    }
}
