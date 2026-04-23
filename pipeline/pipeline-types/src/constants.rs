/// Cardano-specific constants
pub mod cardano {
    /// Length of a Cardano policy ID in hexadecimal characters (28 bytes * 2)
    pub const POLICY_ID_LENGTH: usize = 56;

    /// Length of CIP-68 asset name prefix in hexadecimal characters (4 bytes * 2)
    pub const CIP68_PREFIX_LENGTH: usize = 8;
}

/// CIP (Cardano Improvement Proposal) standard constants
pub mod cip {
    /// CIP-25 metadata label for NFT metadata
    pub const CIP25_METADATA_LABEL: &str = "721";

    /// CIP-68 royalty-related metadata labels
    pub const CIP68_ROYALTY_LABELS: &[&str] = &["50", "51", "52", "53", "54", "55", "56"];
}

/// Display formatting constants
pub mod display {
    /// Default length for address truncation
    pub const ADDRESS_TRUNCATE_THRESHOLD: usize = 12;

    /// Length of address prefix to show when truncating
    pub const ADDRESS_PREFIX_LENGTH: usize = 6;

    /// Length of address suffix to show when truncating
    pub const ADDRESS_SUFFIX_LENGTH: usize = 6;

    /// Length of policy ID to show before truncating with "..."
    pub const POLICY_ID_DISPLAY_LENGTH: usize = 8;
}
