//! Cardano wallet types

use serde::{Deserialize, Serialize};

/// Information about an available wallet extension
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletInfo {
    /// The API name (e.g., "eternl", "nami")
    pub api_name: String,
    /// Display name from the wallet extension
    pub name: String,
    /// Base64-encoded icon (data URL) from the wallet extension
    pub icon: Option<String>,
}

/// CIP-8 DataSignature response from signData
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSignature {
    /// COSE_Sign1 signature (hex-encoded)
    pub signature: String,
    /// COSE_Key public key (hex-encoded)
    pub key: String,
}

/// Supported wallet providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WalletProvider {
    Nami,
    Eternl,
    Lace,
    Flint,
    Typhon,
    Vespr,
    NuFi,
    Gero,
    Yoroi,
}

impl WalletProvider {
    /// Get the window.cardano property name for this wallet
    pub fn api_name(&self) -> &'static str {
        match self {
            WalletProvider::Nami => "nami",
            WalletProvider::Eternl => "eternl",
            WalletProvider::Lace => "lace",
            WalletProvider::Flint => "flint",
            WalletProvider::Typhon => "typhon",
            WalletProvider::Vespr => "vespr",
            WalletProvider::NuFi => "nufi",
            WalletProvider::Gero => "gerowallet",
            WalletProvider::Yoroi => "yoroi",
        }
    }

    /// Get display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            WalletProvider::Nami => "Nami",
            WalletProvider::Eternl => "Eternl",
            WalletProvider::Lace => "Lace",
            WalletProvider::Flint => "Flint",
            WalletProvider::Typhon => "Typhon",
            WalletProvider::Vespr => "Vespr",
            WalletProvider::NuFi => "NuFi",
            WalletProvider::Gero => "Gero",
            WalletProvider::Yoroi => "Yoroi",
        }
    }

    /// All known wallet providers
    pub fn all() -> &'static [WalletProvider] {
        &[
            WalletProvider::Nami,
            WalletProvider::Eternl,
            WalletProvider::Lace,
            WalletProvider::Flint,
            WalletProvider::Typhon,
            WalletProvider::Vespr,
            WalletProvider::NuFi,
            WalletProvider::Gero,
            WalletProvider::Yoroi,
        ]
    }

    /// Get provider from API name (e.g., "eternl" -> Eternl)
    pub fn from_api_name(name: &str) -> Option<WalletProvider> {
        match name {
            "nami" => Some(WalletProvider::Nami),
            "eternl" => Some(WalletProvider::Eternl),
            "lace" => Some(WalletProvider::Lace),
            "flint" => Some(WalletProvider::Flint),
            "typhon" => Some(WalletProvider::Typhon),
            "vespr" => Some(WalletProvider::Vespr),
            "nufi" => Some(WalletProvider::NuFi),
            "gerowallet" => Some(WalletProvider::Gero),
            "yoroi" => Some(WalletProvider::Yoroi),
            _ => None,
        }
    }
}

/// Cardano network
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Network {
    Mainnet,
    Preprod,
    Preview,
}

impl Network {
    pub fn network_id(&self) -> u8 {
        match self {
            Network::Mainnet => 1,
            Network::Preprod | Network::Preview => 0,
        }
    }

    /// The CAIP-style `chain:network` wire form workers speak.
    ///
    /// This enum existed without it, so five places parsed the string by hand
    /// instead — `expected.contains("mainnet")` in `stake_session`, a
    /// `strip_prefix("cardano:")` in `collection_list`, a full-string match in
    /// `image-core`, and two more. Each was right about a different subset.
    pub fn as_chain_str(&self) -> &'static str {
        match self {
            Network::Mainnet => "cardano:mainnet",
            Network::Preprod => "cardano:preprod",
            Network::Preview => "cardano:preview",
        }
    }

    /// Parse the `chain:network` wire form.
    ///
    /// Tolerant of a bare network name (`"preprod"`) because some callers store
    /// it stripped, and of case because nothing guarantees it. `None` for
    /// anything unrecognised — **deliberately not a mainnet default**: guessing
    /// mainnet for an unknown string is how a preprod wallet gets told it is on
    /// the wrong network, or worse, how a mainnet check silently passes.
    pub fn from_chain_str(s: &str) -> Option<Self> {
        // Lowercase BEFORE stripping: the other order fails on `Cardano:PREPROD`
        // because the prefix no longer matches, and the whole string then fails
        // the arm too. Caught by `case_does_not_matter`.
        let lower = s.to_ascii_lowercase();
        let bare = lower.strip_prefix("cardano:").unwrap_or(&lower);
        match bare {
            "mainnet" => Some(Network::Mainnet),
            "preprod" => Some(Network::Preprod),
            "preview" => Some(Network::Preview),
            _ => None,
        }
    }
}

#[cfg(test)]
mod network_tests {
    use super::Network;

    #[test]
    fn the_wire_form_round_trips() {
        for n in [Network::Mainnet, Network::Preprod, Network::Preview] {
            assert_eq!(Network::from_chain_str(n.as_chain_str()), Some(n));
        }
    }

    #[test]
    fn a_bare_network_name_parses_too() {
        // Some callers store the stripped form.
        assert_eq!(Network::from_chain_str("preprod"), Some(Network::Preprod));
        assert_eq!(Network::from_chain_str("mainnet"), Some(Network::Mainnet));
    }

    #[test]
    fn case_does_not_matter() {
        assert_eq!(
            Network::from_chain_str("Cardano:PREPROD"),
            Some(Network::Preprod)
        );
    }

    #[test]
    fn an_unknown_network_is_none_and_never_mainnet() {
        // The dangerous default. If this ever returns `Mainnet`, a wrong-network
        // pre-check passes silently and a preprod wallet signs a mainnet
        // challenge.
        assert_eq!(Network::from_chain_str(""), None);
        assert_eq!(Network::from_chain_str("cardano:"), None);
        assert_eq!(Network::from_chain_str("ethereum:1"), None);
        assert_eq!(Network::from_chain_str("sanchonet"), None);
    }

    #[test]
    fn only_mainnet_has_network_id_one() {
        assert_eq!(Network::Mainnet.network_id(), 1);
        assert_eq!(Network::Preprod.network_id(), 0);
        assert_eq!(Network::Preview.network_id(), 0);
    }
}

/// Wallet connection state
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected {
        provider: WalletProvider,
        address: String,
        network: Network,
    },
    Error(String),
}
