//! Address utilities for Cardano addresses
//!
//! Provides parsing, validation, and bech32 encoding/decoding for Cardano addresses.

use crate::PallasError;
use pallas_addresses::Address as PallasAddress;
use serde::{Deserialize, Serialize};

/// Cardano network identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    /// Get the network ID (0 = testnet, 1 = mainnet)
    pub fn id(&self) -> u8 {
        match self {
            Network::Mainnet => 1,
            Network::Testnet => 0,
        }
    }
}

/// A parsed Cardano address with utilities
#[derive(Debug, Clone)]
pub struct Address {
    inner: PallasAddress,
    raw_bytes: Vec<u8>,
}

impl Address {
    /// Parse an address from hex-encoded bytes (as returned by CIP-30)
    pub fn from_hex(hex_str: &str) -> Result<Self, PallasError> {
        let bytes = hex::decode(hex_str)?;
        Self::from_bytes(&bytes)
    }

    /// Parse an address from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PallasError> {
        let inner = PallasAddress::from_bytes(bytes)
            .map_err(|e| PallasError::InvalidAddress(e.to_string()))?;

        Ok(Self {
            inner,
            raw_bytes: bytes.to_vec(),
        })
    }

    /// Parse an address from bech32 string (addr1... or addr_test1...)
    pub fn from_bech32(bech32_str: &str) -> Result<Self, PallasError> {
        let inner = PallasAddress::from_bech32(bech32_str)
            .map_err(|e| PallasError::InvalidAddress(e.to_string()))?;

        let raw_bytes = inner.to_vec();

        Ok(Self { inner, raw_bytes })
    }

    /// Get the network this address belongs to
    pub fn network(&self) -> Network {
        match self.inner.network() {
            Some(pallas_addresses::Network::Mainnet) => Network::Mainnet,
            _ => Network::Testnet,
        }
    }

    /// Get the bech32 encoded address string
    pub fn to_bech32(&self) -> Result<String, PallasError> {
        self.inner
            .to_bech32()
            .map_err(|e| PallasError::InvalidAddress(e.to_string()))
    }

    /// Get the hex-encoded address bytes
    pub fn to_hex(&self) -> String {
        hex::encode(&self.raw_bytes)
    }

    /// Get the raw address bytes
    pub fn to_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }

    /// Get the payment credential hash (for base/enterprise addresses)
    pub fn payment_hash(&self) -> Option<[u8; 28]> {
        match &self.inner {
            PallasAddress::Shelley(addr) => {
                let hash = addr.payment().as_hash();
                let bytes: &[u8] = hash.as_ref();
                let mut arr = [0u8; 28];
                arr.copy_from_slice(bytes);
                Some(arr)
            }
            _ => None,
        }
    }

    /// Get the staking credential hash (for base addresses)
    pub fn stake_hash(&self) -> Option<[u8; 28]> {
        match &self.inner {
            PallasAddress::Shelley(addr) => match addr.delegation() {
                pallas_addresses::ShelleyDelegationPart::Key(hash) => {
                    let bytes: &[u8] = hash.as_ref();
                    let mut arr = [0u8; 28];
                    arr.copy_from_slice(bytes);
                    Some(arr)
                }
                pallas_addresses::ShelleyDelegationPart::Script(hash) => {
                    let bytes: &[u8] = hash.as_ref();
                    let mut arr = [0u8; 28];
                    arr.copy_from_slice(bytes);
                    Some(arr)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Check if this is a script address
    pub fn is_script(&self) -> bool {
        match &self.inner {
            PallasAddress::Shelley(addr) => {
                matches!(
                    addr.payment(),
                    pallas_addresses::ShelleyPaymentPart::Script(_)
                )
            }
            _ => false,
        }
    }

    /// Get the stake address (reward address) in bech32 format
    ///
    /// Returns the stake address derived from this address's staking credential,
    /// encoded as bech32 (stake1... for mainnet, stake_test1... for testnet).
    pub fn stake_address_bech32(&self) -> Option<String> {
        use pallas_addresses::{StakeAddress, StakePayload};

        match &self.inner {
            PallasAddress::Shelley(addr) => {
                let network = addr.network();
                let stake_payload = match addr.delegation() {
                    pallas_addresses::ShelleyDelegationPart::Key(hash) => {
                        StakePayload::Stake(pallas_addresses::StakeKeyHash::from(hash.as_ref()))
                    }
                    pallas_addresses::ShelleyDelegationPart::Script(hash) => {
                        StakePayload::Script(pallas_addresses::ScriptHash::from(hash.as_ref()))
                    }
                    _ => return None,
                };

                let stake_addr = StakeAddress::new(network, stake_payload);
                stake_addr.to_bech32().ok()
            }
            _ => None,
        }
    }

    /// Get a shortened display version of the address
    pub fn display_short(&self) -> String {
        match self.to_bech32() {
            Ok(bech32) if bech32.len() > 20 => {
                format!("{}...{}", &bech32[..12], &bech32[bech32.len() - 8..])
            }
            Ok(bech32) => bech32,
            Err(_) => {
                let hex = self.to_hex();
                if hex.len() > 20 {
                    format!("{}...{}", &hex[..10], &hex[hex.len() - 8..])
                } else {
                    hex
                }
            }
        }
    }
}
