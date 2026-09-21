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

/// The Cardano networks, re-exported from `chains` under the name this crate
/// has always used.
///
/// The type moved because it was never wasm-specific — a pure enum living in a
/// wasm-bindgen crate is why `egui-widgets` could not name a chain without
/// turning on its `cardano` feature. Same shape as `data::ownership_policies`'s
/// re-export of `SyncSource`, and for the same reason: existing importers keep
/// working.
///
/// `Network` here means a Cardano network and nothing else. For a whole chain,
/// including EVM, see `chains::ChainRef`.
pub use chains::CardanoNetwork as Network;

#[cfg(test)]
mod network_alias_tests {
    use super::Network;

    /// The parsing rules moved to `chains` with the type and are tested there,
    /// against the implementation. What is pinned here is the compatibility
    /// surface itself: an importer written against `wallet_core::Network` still
    /// resolves, and still behaves.
    #[test]
    fn the_re_export_still_resolves_and_parses() {
        assert_eq!(
            Network::from_chain_str("cardano:preprod"),
            Some(Network::Preprod)
        );
        assert_eq!(Network::Mainnet.network_id(), 1);
        assert_eq!(Network::Preprod.as_chain_str(), "cardano:preprod");
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
