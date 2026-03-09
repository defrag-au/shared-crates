use pallas_crypto::hash::Hasher;
use pallas_primitives::babbage::{NativeScript, PolicyId};
use serde::{Deserialize, Serialize};

/// Minting policy definitions for native scripts
///
/// Native scripts are the simplest form of minting policies on Cardano.
/// They can enforce requirements like signature checks and time locks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MintingPolicy {
    /// Wallet-derived policy
    /// Uses the HD wallet's key hash automatically
    /// Most convenient for services using HD wallets
    /// The wallet-operations worker will derive the key hash from the HD wallet
    WalletDerived,

    /// Single signature policy
    /// Can be minted/burned by single key holder
    /// Most common for simple NFT collections
    SingleKey { key_hash: String },

    /// Time-locked policy
    /// Can only mint before specified slot
    /// Useful for limited-time mints (e.g., deadline-based launches)
    TimeLocked { key_hash: String, before_slot: u64 },

    /// Time-bounded policy
    /// Can only mint between two slots (after_slot and before_slot)
    /// Useful for specific launch windows
    TimeBounded {
        key_hash: String,
        after_slot: u64,
        before_slot: u64,
    },

    /// Multisig policy (M of N)
    /// Requires M signatures from N possible signers
    /// Better security for high-value collections
    MultiSig {
        required: u32,
        key_hashes: Vec<String>,
    },

    /// Combined policy (multisig + time lock)
    /// Most secure for collection launches
    /// Requires M signatures AND must be before specified slot
    MultiSigTimeLocked {
        required: u32,
        key_hashes: Vec<String>,
        before_slot: u64,
    },
}

impl MintingPolicy {
    /// Convert to Pallas NativeScript format
    ///
    /// This transforms our high-level policy definition into the low-level
    /// format that Cardano understands.
    pub fn to_native_script(&self) -> Result<NativeScript, String> {
        match self {
            Self::WalletDerived => {
                Err("WalletDerived policy must be resolved to SingleKey before calling to_native_script".to_string())
            }

            Self::SingleKey { key_hash } => {
                let key_bytes =
                    hex::decode(key_hash).map_err(|e| format!("Invalid key hash hex: {e}"))?;

                if key_bytes.len() != 28 {
                    return Err(format!(
                        "Invalid key hash length: expected 28 bytes, got {}",
                        key_bytes.len()
                    ));
                }

                let mut key_array = [0u8; 28];
                key_array.copy_from_slice(&key_bytes);

                Ok(NativeScript::ScriptPubkey(key_array.into()))
            }

            Self::TimeLocked {
                key_hash,
                before_slot,
            } => {
                let key_bytes =
                    hex::decode(key_hash).map_err(|e| format!("Invalid key hash hex: {e}"))?;

                if key_bytes.len() != 28 {
                    return Err(format!(
                        "Invalid key hash length: expected 28 bytes, got {}",
                        key_bytes.len()
                    ));
                }

                let mut key_array = [0u8; 28];
                key_array.copy_from_slice(&key_bytes);

                Ok(NativeScript::ScriptAll(vec![
                    NativeScript::ScriptPubkey(key_array.into()),
                    NativeScript::InvalidBefore(*before_slot),
                ]))
            }

            Self::TimeBounded {
                key_hash,
                after_slot,
                before_slot,
            } => {
                let key_bytes =
                    hex::decode(key_hash).map_err(|e| format!("Invalid key hash hex: {e}"))?;

                if key_bytes.len() != 28 {
                    return Err(format!(
                        "Invalid key hash length: expected 28 bytes, got {}",
                        key_bytes.len()
                    ));
                }

                let mut key_array = [0u8; 28];
                key_array.copy_from_slice(&key_bytes);

                Ok(NativeScript::ScriptAll(vec![
                    NativeScript::ScriptPubkey(key_array.into()),
                    NativeScript::InvalidHereafter(*after_slot),
                    NativeScript::InvalidBefore(*before_slot),
                ]))
            }

            Self::MultiSig {
                required,
                key_hashes,
            } => {
                if *required == 0 {
                    return Err("Required signatures must be > 0".to_string());
                }

                if *required > key_hashes.len() as u32 {
                    return Err(format!(
                        "Required signatures ({required}) cannot exceed total keys ({})",
                        key_hashes.len()
                    ));
                }

                let mut scripts = Vec::new();
                for kh in key_hashes {
                    let key_bytes =
                        hex::decode(kh).map_err(|e| format!("Invalid key hash hex: {e}"))?;

                    if key_bytes.len() != 28 {
                        return Err(format!(
                            "Invalid key hash length: expected 28 bytes, got {}",
                            key_bytes.len()
                        ));
                    }

                    let mut key_array = [0u8; 28];
                    key_array.copy_from_slice(&key_bytes);

                    scripts.push(NativeScript::ScriptPubkey(key_array.into()));
                }

                Ok(NativeScript::ScriptNOfK(*required, scripts))
            }

            Self::MultiSigTimeLocked {
                required,
                key_hashes,
                before_slot,
            } => {
                if *required == 0 {
                    return Err("Required signatures must be > 0".to_string());
                }

                if *required > key_hashes.len() as u32 {
                    return Err(format!(
                        "Required signatures ({required}) cannot exceed total keys ({})",
                        key_hashes.len()
                    ));
                }

                let mut sig_scripts = Vec::new();
                for kh in key_hashes {
                    let key_bytes =
                        hex::decode(kh).map_err(|e| format!("Invalid key hash hex: {e}"))?;

                    if key_bytes.len() != 28 {
                        return Err(format!(
                            "Invalid key hash length: expected 28 bytes, got {}",
                            key_bytes.len()
                        ));
                    }

                    let mut key_array = [0u8; 28];
                    key_array.copy_from_slice(&key_bytes);

                    sig_scripts.push(NativeScript::ScriptPubkey(key_array.into()));
                }

                Ok(NativeScript::ScriptAll(vec![
                    NativeScript::ScriptNOfK(*required, sig_scripts),
                    NativeScript::InvalidBefore(*before_slot),
                ]))
            }
        }
    }

    /// Derive policy ID from script
    ///
    /// The policy ID is the hash of the native script.
    /// This is what appears in asset IDs on Cardano.
    pub fn policy_id(&self) -> Result<PolicyId, String> {
        use pallas_primitives::Fragment;

        let script = self.to_native_script()?;

        // Encode the script using encode_fragment (Cardano-compatible encoding)
        let script_bytes = script
            .encode_fragment()
            .map_err(|e| format!("Failed to encode script: {e}"))?;

        // IMPORTANT: Native scripts require a 0x00 prefix before the CBOR bytes
        // when hashing to derive the policy ID. This is defined in the Cardano ledger spec.
        let mut prefixed_bytes = Vec::with_capacity(1 + script_bytes.len());
        prefixed_bytes.push(0x00); // Native script type prefix
        prefixed_bytes.extend_from_slice(&script_bytes);

        let policy_hash = Hasher::<224>::hash(&prefixed_bytes);

        Ok(policy_hash)
    }

    /// Get human-readable policy ID as hex string
    pub fn policy_id_hex(&self) -> Result<String, String> {
        let policy_id = self.policy_id()?;
        Ok(hex::encode(policy_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sample key hash for testing (28 bytes = 56 hex chars)
    const TEST_KEY_HASH: &str = "abcd1234ef567890abcd1234ef567890abcd1234ef567890abcd1234";

    #[test]
    fn test_single_key_policy_id() {
        let policy = MintingPolicy::SingleKey {
            key_hash: TEST_KEY_HASH.to_string(),
        };

        let policy_id = policy.policy_id_hex().expect("Should derive policy ID");
        assert!(!policy_id.is_empty());
        assert_eq!(policy_id.len(), 56); // 28 bytes = 56 hex chars
    }

    #[test]
    fn test_real_key_hash_policy_derivation() {
        // Real key hash from nft_tester wallet
        let key_hash = "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29";
        let policy = MintingPolicy::SingleKey {
            key_hash: key_hash.to_string(),
        };

        let policy_id = policy.policy_id_hex().expect("Should derive policy ID");

        // Verify the hash matches
        assert_eq!(
            policy_id,
            "88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9"
        );
    }

    #[test]
    fn test_time_locked_policy() {
        let policy = MintingPolicy::TimeLocked {
            key_hash: TEST_KEY_HASH.to_string(),
            before_slot: 50000000,
        };

        let script = policy.to_native_script().expect("Should create script");
        assert!(matches!(script, NativeScript::ScriptAll(_)));
    }

    #[test]
    fn test_multisig_policy() {
        let policy = MintingPolicy::MultiSig {
            required: 2,
            key_hashes: vec![
                TEST_KEY_HASH.to_string(),
                TEST_KEY_HASH.to_string(),
                TEST_KEY_HASH.to_string(),
            ],
        };

        let script = policy.to_native_script().expect("Should create script");
        assert!(matches!(script, NativeScript::ScriptNOfK(2, _)));
    }

    #[test]
    fn test_multisig_validation() {
        // Too many required signatures
        let policy = MintingPolicy::MultiSig {
            required: 5,
            key_hashes: vec![TEST_KEY_HASH.to_string(), TEST_KEY_HASH.to_string()],
        };

        assert!(policy.to_native_script().is_err());

        // Zero required signatures
        let policy = MintingPolicy::MultiSig {
            required: 0,
            key_hashes: vec![TEST_KEY_HASH.to_string()],
        };

        assert!(policy.to_native_script().is_err());
    }

    #[test]
    fn test_invalid_key_hash() {
        // Invalid hex
        let policy = MintingPolicy::SingleKey {
            key_hash: "not-hex".to_string(),
        };
        assert!(policy.to_native_script().is_err());

        // Wrong length
        let policy = MintingPolicy::SingleKey {
            key_hash: "abc123".to_string(),
        };
        assert!(policy.to_native_script().is_err());
    }

    #[test]
    fn test_time_bounded_policy() {
        let policy = MintingPolicy::TimeBounded {
            key_hash: TEST_KEY_HASH.to_string(),
            after_slot: 40000000,
            before_slot: 50000000,
        };

        let script = policy.to_native_script().expect("Should create script");
        assert!(matches!(script, NativeScript::ScriptAll(_)));

        let policy_id = policy.policy_id_hex().expect("Should derive policy ID");
        assert_eq!(policy_id.len(), 56);
    }

    #[test]
    fn test_multisig_time_locked_policy() {
        let policy = MintingPolicy::MultiSigTimeLocked {
            required: 2,
            key_hashes: vec![
                TEST_KEY_HASH.to_string(),
                TEST_KEY_HASH.to_string(),
                TEST_KEY_HASH.to_string(),
            ],
            before_slot: 50000000,
        };

        let script = policy.to_native_script().expect("Should create script");
        assert!(matches!(script, NativeScript::ScriptAll(_)));

        let policy_id = policy.policy_id_hex().expect("Should derive policy ID");
        assert_eq!(policy_id.len(), 56);
    }

    #[test]
    fn test_script_hash_methods() {
        let key_hash = "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29";
        let policy = MintingPolicy::SingleKey {
            key_hash: key_hash.to_string(),
        };

        let script = policy.to_native_script().unwrap();
        use pallas_primitives::Fragment;
        let script_bytes = script.encode_fragment().unwrap();

        // Method 1: With 0x00 prefix (what we use)
        let mut prefixed = vec![0x00];
        prefixed.extend_from_slice(&script_bytes);
        let hash1 = Hasher::<224>::hash(&prefixed);

        assert_eq!(
            hex::encode(hash1),
            "88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9"
        );
    }

    #[test]
    fn test_pubkey_to_keyhash() {
        // Public key from vkey witness
        let pubkey_hex = "048588980756191d008b48d6c5054f5449cfc9c664c0c71332fae79f3ed082fe";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        // Hash to get key hash
        let key_hash = Hasher::<224>::hash(&pubkey_bytes);
        let key_hash_hex = hex::encode(key_hash);

        assert_eq!(
            key_hash_hex,
            "9ad4da1c6da54e41ecbab2758323f1abcc7b6e6643f5b930065fcb29"
        );
    }

    #[test]
    fn test_verify_witness_script_hash() {
        // The raw bytes from the witness
        let raw: [u8; 32] = [
            130, 0, 88, 28, 154, 212, 218, 28, 109, 165, 78, 65, 236, 186, 178, 117, 131, 35, 241,
            171, 204, 123, 110, 102, 67, 245, 185, 48, 6, 95, 203, 41,
        ];

        // Hash with 0x00 prefix
        let mut prefixed = vec![0x00];
        prefixed.extend_from_slice(&raw);
        let hash = Hasher::<224>::hash(&prefixed);

        assert_eq!(
            hex::encode(hash),
            "88263ccf789c6849955b76a287a34d3732c925a1561d260906abfcf9"
        );
    }
}
