//! DEX and launchpad deployments we spend directly.
//!
//! These are OTHER PEOPLE'S contracts, discovered on chain — the case the note
//! in `lib.rs` says belongs in a constant. Nothing here may be duplicated at a
//! call site: a worker, a frontend or a test that needs a hash, an address or
//! a curve constant reads it from the record.
//!
//! Deliberately absent, as in [`crate::ScriptReference`]: the Plutus language
//! and the script size. Both are properties of the deployed script that the
//! reference UTxO already states on chain, and copying them here creates a
//! second, unverified source of truth. Callers resolve them from the
//! referenced UTxO and assert the hash matches [`Self::validator_hash`].

/// A native asset, as the two hex strings that identify it. Kept stringly here
/// because this crate stays dependency-light on purpose; consumers convert to
/// their own asset type (`cardano_assets::AssetId`) at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryAsset {
    pub policy_id: &'static str,
    pub asset_name_hex: &'static str,
}

impl RegistryAsset {
    pub fn dot_delimited(&self) -> String {
        format!("{}.{}", self.policy_id, self.asset_name_hex)
    }

    pub fn concatenated(&self) -> String {
        format!("{}{}", self.policy_id, self.asset_name_hex)
    }
}

/// A reference-script UTxO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceUtxo {
    pub tx_hash: &'static str,
    pub output_index: u32,
}

// ============================================================================
// LumpPad
// ============================================================================

/// LumpPad — LUMP-quoted constant-product launchpad pools.
///
/// One shared PlutusV3 pool validator, one one-shot mint policy per launch,
/// one state NFT per pool. Three redeemers: Buy, Sell, Claim. No withdraw, no
/// graduation, no admin. Every curve constant below was decoded from the
/// validator's UPLC and cross-checked against the lumptools.xyz bundle and 31
/// on-chain trades; see `docs/design/LUMPPAD_INTEGRATION.md` §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LumpPadDeployment {
    pub validator_hash: &'static str,
    /// Enterprise address (no stake part) every pool UTxO sits at.
    pub pool_address: &'static str,
    pub reference_utxo: ReferenceUtxo,
    /// Script credential of the burn address. No script with this hash has
    /// ever appeared on chain, so it is unspendable as far as anyone can tell
    /// — but that cannot be PROVEN until the script is published. Half the
    /// platform bucket is paid here on every Claim.
    pub burn_credential: &'static str,
    /// Key credential of the platform treasury. Hard-coded in the validator
    /// and in every mint policy.
    pub treasury_credential: &'static str,
    /// The quote asset. Everything in a LumpPad pool is priced in it.
    pub lump: RegistryAsset,
    /// Asset name of every pool's state NFT: CIP-67 label 100 + "POOL". Under
    /// the TOKEN's policy, one per pool. Its datum is pool state, NOT CIP-68
    /// metadata — see `LUMPPAD_INTEGRATION.md` §10 before pointing a metadata
    /// reader at it.
    pub state_nft_name_hex: &'static str,
    /// Virtual LUMP reserve added to the real reserve on the curve.
    pub virtual_reserve: u64,
    /// Numerator of the swap factor over 10,000 — the 0.3 % that stays in the
    /// pool rather than moving to a bucket.
    pub swap_fee_num: u64,
    pub creator_fee_bps: u64,
    pub platform_fee_bps: u64,
    /// Flat LUMP charged per trade to the platform bucket.
    pub flat_fee: u64,
    /// Lovelace that stays in the pool UTxO forever.
    pub pool_min_lovelace: u64,
    /// Lovelace LumpPad attaches to each of a Claim's three payouts.
    pub claim_payout_lovelace: u64,
}

/// The mainnet LumpPad deployment.
pub const LUMPPAD: LumpPadDeployment = LumpPadDeployment {
    validator_hash: "60d7399911167a68d94731df96dfedc96436ee0d5bb2a7f0ef2981dc",
    pool_address: "addr1w9sdwwvezyt856xegucal9klahykgdhwp4dm9flsau5crhqx9sd5x",
    reference_utxo: ReferenceUtxo {
        tx_hash: "1b20058e9f43b0d964f91925364df9a6fc6181576c6cac66659ff187d41ddad6",
        output_index: 0,
    },
    burn_credential: "22c9a103ed3f2fa97c982d76d6e2af50c5d54ac306983b196c8fcdab",
    treasury_credential: "c79844a83ab36100765fbc19fb8d738a0d46657708f6ad08c8c637f3",
    lump: RegistryAsset {
        policy_id: "73797786382c0832b5787a5b306f5308488f14571b7061f79396ad2c",
        asset_name_hex: "4c756d70",
    },
    state_nft_name_hex: "000643b0504f4f4c",
    virtual_reserve: 10_000_000,
    swap_fee_num: 9_970,
    creator_fee_bps: 100,
    platform_fee_bps: 50,
    flat_fee: 10_000,
    pool_min_lovelace: 3_000_000,
    claim_payout_lovelace: 1_500_000,
};

/// The burn address the LumpPad reference script and half of every platform
/// bucket sit at. Bech32 form of [`LumpPadDeployment::burn_credential`].
pub const LUMPPAD_BURN_ADDRESS: &str = "addr1wy3vnggra5ljl2tunqkhd4hz4agvt422cvrfswcedj8um2c4cgds5";

/// The treasury address half of every platform bucket is paid to. Bech32 form
/// of [`LumpPadDeployment::treasury_credential`].
pub const LUMPPAD_TREASURY_ADDRESS: &str = "addr1q8res39g82ekzqrkt77pn7udww9q63n9wuy0dtggerrr0uac5v774xc5xy007n82dmmgfdwyy5dyhlkfddkg4n4v9njshq3x40";

// ============================================================================
// Splash royalty pools
// ============================================================================

/// Splash's royalty-pool validator — the AMM pools that are spendable
/// DIRECTLY, with no batcher. One script, shared by ~400 pools.
///
/// This is not the same thing as `cardano_tx::dex::splash`, which builds spot
/// ORDERS for a batcher to fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplashPoolValidator {
    pub script_hash: &'static str,
    /// Shared by every royalty pool.
    pub pool_address: &'static str,
    pub reference_utxo: ReferenceUtxo,
    /// Denominator the validator's fee arithmetic uses.
    pub fee_den: u64,
}

/// The mainnet Splash royalty-pool validator.
pub const SPLASH_ROYALTY_POOL: SplashPoolValidator = SplashPoolValidator {
    script_hash: "cb684a69e78907a9796b21fc150a758af5f2805e5ed5d5a8ce9f76f1",
    pool_address: "addr1x89ksjnfu7ys02tedvslc9g2wk90tu5qte0dt4dge60hdudj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrsg0g63z",
    reference_utxo: ReferenceUtxo {
        tx_hash: "8aa606ad8c995af6e59b19c0fee2f5bb5abc04552d105a146f0a4921cca9b600",
        output_index: 0,
    },
    fee_den: 100_000,
};

/// One named Splash royalty pool, identified by the NFT its continuing output
/// must carry. `x` and `y` are the datum's `poolX` / `poolY`, in that order —
/// the direction of a swap is stated against them, never against a ticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplashPool {
    pub nft: RegistryAsset,
    /// `poolX`. `None` means ADA, which the datum encodes as the empty
    /// policy and empty name.
    pub x: Option<RegistryAsset>,
    /// `poolY`.
    pub y: Option<RegistryAsset>,
}

/// The LUMP/ADA royalty pool — the ADA leg of the pilot route.
pub const SPLASH_LUMP_ADA: SplashPool = SplashPool {
    nft: RegistryAsset {
        policy_id: "d8eb52caf3289a2880288b23141ce3d2a7025dcf76f26fd5659add06",
        asset_name_hex: "9f4408661725f5f141a17e75dd0982bbfe6f6053ae779d1fb74fec800e752b44",
    },
    x: None,
    y: Some(LUMPPAD.lump),
};

#[cfg(test)]
mod tests {
    use super::*;
    use pallas_addresses::Address;

    /// Every address in this module must actually decode, and must be a
    /// mainnet address. A table nothing parses can hold a string that is not
    /// an address — which has happened here before.
    #[test]
    fn every_address_decodes_as_mainnet() {
        for (label, bech32) in [
            ("LumpPad pool", LUMPPAD.pool_address),
            ("LumpPad burn", LUMPPAD_BURN_ADDRESS),
            ("LumpPad treasury", LUMPPAD_TREASURY_ADDRESS),
            ("Splash pool", SPLASH_ROYALTY_POOL.pool_address),
        ] {
            let addr = Address::from_bech32(bech32)
                .unwrap_or_else(|e| panic!("{label} is not an address: {e}"));
            assert_eq!(
                addr.network(),
                Some(pallas_addresses::Network::Mainnet),
                "{label} is not a mainnet address"
            );
        }
    }

    /// The pool addresses must carry the validator hash the record names —
    /// this is the check that catches a hash and an address drifting apart.
    #[test]
    fn pool_addresses_carry_their_validators_hash() {
        for (label, bech32, hash) in [
            ("LumpPad", LUMPPAD.pool_address, LUMPPAD.validator_hash),
            (
                "Splash",
                SPLASH_ROYALTY_POOL.pool_address,
                SPLASH_ROYALTY_POOL.script_hash,
            ),
        ] {
            let Address::Shelley(addr) = Address::from_bech32(bech32).expect("decodes") else {
                panic!("{label} pool address is not a Shelley address");
            };
            assert_eq!(
                addr.payment().to_hex(),
                hash,
                "{label} pool address does not carry its validator hash"
            );
            assert!(
                addr.payment().is_script(),
                "{label} pool address must be a script address"
            );
        }
    }

    /// The burn and treasury credentials must match their bech32 forms.
    #[test]
    fn lumppad_credentials_match_their_addresses() {
        let Address::Shelley(burn) = Address::from_bech32(LUMPPAD_BURN_ADDRESS).unwrap() else {
            panic!("burn address is not Shelley");
        };
        assert_eq!(burn.payment().to_hex(), LUMPPAD.burn_credential);
        assert!(burn.payment().is_script(), "the burn address is a script");

        let Address::Shelley(treasury) = Address::from_bech32(LUMPPAD_TREASURY_ADDRESS).unwrap()
        else {
            panic!("treasury address is not Shelley");
        };
        assert_eq!(treasury.payment().to_hex(), LUMPPAD.treasury_credential);
        assert!(
            !treasury.payment().is_script(),
            "the treasury is an ordinary operator wallet, not a script"
        );
    }

    /// Hex fields are hex, and credentials are 28 bytes.
    #[test]
    fn hashes_are_well_formed() {
        for (label, hex_str, bytes) in [
            ("LumpPad validator", LUMPPAD.validator_hash, 28),
            ("LumpPad burn cred", LUMPPAD.burn_credential, 28),
            ("LumpPad treasury cred", LUMPPAD.treasury_credential, 28),
            ("LUMP policy", LUMPPAD.lump.policy_id, 28),
            ("Splash validator", SPLASH_ROYALTY_POOL.script_hash, 28),
            (
                "Splash LUMP/ADA nft policy",
                SPLASH_LUMP_ADA.nft.policy_id,
                28,
            ),
            ("LumpPad ref utxo", LUMPPAD.reference_utxo.tx_hash, 32),
            (
                "Splash ref utxo",
                SPLASH_ROYALTY_POOL.reference_utxo.tx_hash,
                32,
            ),
        ] {
            let decoded = hex::decode(hex_str).unwrap_or_else(|e| panic!("{label}: {e}"));
            assert_eq!(decoded.len(), bytes, "{label}: wrong length");
        }
    }

    /// The state NFT name is CIP-67 label 100 followed by "POOL".
    #[test]
    fn state_nft_name_is_label_100_pool() {
        let bytes = hex::decode(LUMPPAD.state_nft_name_hex).unwrap();
        assert_eq!(&bytes[..4], &[0x00, 0x06, 0x43, 0xb0], "CIP-67 label 100");
        assert_eq!(&bytes[4..], b"POOL");
    }

    /// The pilot pool's quote side is LUMP, and its X side is ADA.
    #[test]
    fn the_pilot_pool_is_lump_over_ada() {
        assert_eq!(SPLASH_LUMP_ADA.x, None, "poolX is ADA");
        assert_eq!(SPLASH_LUMP_ADA.y, Some(LUMPPAD.lump));
    }
}
