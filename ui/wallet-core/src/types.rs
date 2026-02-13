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
