//! The domain vocabulary, derived from primitives.
//!
//! A reading is a label over the same axes, never a struct with its own fields.
//! That is the difference from `tx-classifier`'s `TxType`: adding a marketplace
//! event adds a facet or a derivation, not a variant with a bespoke shape.

use serde::{Deserialize, Serialize};

/// What a group of primitives reads as.
///
/// A transaction can have several. The card shows one cell per reading; the
/// detail sheet shows the primitives beneath them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reading {
    Sale,
    Accept,
    Listing,
    Offer,
    Transfer,
    SelfTransfer,
    Mint,
    Burn,
    Swap,
    Lock,
    Release,
    Stake,
    Unstake,
    Vest,
    Borrow,
    Repay,
    Liquidate,
    Approve,
    RevokeApproval,
    Wrap,
    Unwrap,
    Shield,
    Unshield,
    PriceChange,
    Delist,
    CancelOffer,
    Cancel,
    Unknown,
}

impl Reading {
    /// The one word a card draws beside its glyph. Every reading has one, so a
    /// cell can never be an unlabelled icon in a capture report.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::Accept => "accept",
            Self::Listing => "list",
            Self::Offer => "offer",
            Self::Transfer => "transfer",
            Self::SelfTransfer => "self",
            Self::Mint => "mint",
            Self::Burn => "burn",
            Self::Swap => "swap",
            Self::Lock => "lock",
            Self::Release => "release",
            Self::Stake => "stake",
            Self::Unstake => "unstake",
            Self::Vest => "vest",
            Self::Borrow => "borrow",
            Self::Repay => "repay",
            Self::Liquidate => "liquidate",
            Self::Approve => "approve",
            Self::RevokeApproval => "revoke",
            Self::Wrap => "wrap",
            Self::Unwrap => "unwrap",
            Self::Shield => "shield",
            Self::Unshield => "unshield",
            Self::PriceChange => "price",
            Self::Delist => "delist",
            Self::CancelOffer => "cancel",
            Self::Cancel => "cancel",
            Self::Unknown => "unknown",
        }
    }

    /// Did value change hands for a price?
    pub const fn is_settlement(self) -> bool {
        matches!(self, Self::Sale | Self::Accept)
    }

    /// Does this reading change custody without changing ownership?
    pub const fn is_custody(self) -> bool {
        matches!(
            self,
            Self::Listing
                | Self::Offer
                | Self::Lock
                | Self::Release
                | Self::Stake
                | Self::Unstake
                | Self::Vest
                | Self::Delist
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Reading;

    const ALL: [Reading; 28] = [
        Reading::Sale,
        Reading::Accept,
        Reading::Listing,
        Reading::Offer,
        Reading::Transfer,
        Reading::SelfTransfer,
        Reading::Mint,
        Reading::Burn,
        Reading::Swap,
        Reading::Lock,
        Reading::Release,
        Reading::Stake,
        Reading::Unstake,
        Reading::Vest,
        Reading::Borrow,
        Reading::Repay,
        Reading::Liquidate,
        Reading::Approve,
        Reading::RevokeApproval,
        Reading::Wrap,
        Reading::Unwrap,
        Reading::Shield,
        Reading::Unshield,
        Reading::PriceChange,
        Reading::Delist,
        Reading::CancelOffer,
        Reading::Cancel,
        Reading::Unknown,
    ];

    #[test]
    fn every_reading_has_a_word() {
        for reading in ALL {
            assert!(!reading.label().is_empty(), "{reading:?}");
        }
    }

    #[test]
    fn only_a_settlement_is_a_settlement() {
        for reading in ALL {
            assert_eq!(
                reading.is_settlement(),
                matches!(reading, Reading::Sale | Reading::Accept),
                "{reading:?}"
            );
        }
    }
}
