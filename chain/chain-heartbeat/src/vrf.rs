//! Range-extended VRF values: the numbers a header's VRF output stands for.
//!
//! A VRF certificate's 64-byte output is not itself the number the chain
//! compares against a threshold. Praos range-extends it by hashing it with a
//! one-byte domain tag — `"L"` for the leader check, `"N"` for the evolving
//! nonce — as described in section 4.1 of *On UC-Secure Range Extension and
//! Batch Verification for ECVRF*.
//!
//! From `Cardano.Protocol.Praos.VRF` in `cardano-ledger`
//! (`libs/cardano-protocol/src/Cardano/Protocol/Praos/VRF.hs`):
//!
//! ```haskell
//! SVRFLeader -> castHash $ hashWith id $ "L" <> vrfOutputAsBytes
//!
//! vrfLeaderValue p cvrf =
//!   assertBoundedNatural
//!     ((2 :: Natural) ^ (8 * hashSize (Proxy @HASH)))   -- 2^256
//!     (bytesToNatural . hashToBytes $ hashVRF p SVRFLeader cvrf)
//! ```
//!
//! Before Babbage the leader check used the raw 64-byte output against a
//! bound of 2^512 instead, so [`HeaderVrf::leader_value`] is era-aware.
//!
//! # Do not select on the leader value
//!
//! It is tempting to reuse the leader value as a per-block lottery. It is the
//! wrong number for that, because **it is only uniform over the range the
//! producer's own stake allows.** A block exists precisely because its leader
//! value fell below `T(sigma) = 1 - (1-f)^sigma`, so across the blocks that
//! actually got made, leader values are uniform on `[0, T(sigma))` — a range
//! that shrinks with the pool's stake. An application threshold applied to it
//! would fire far more often per block for small pools, and for any pool whose
//! `T(sigma)` sits below the threshold it would fire on *every* block.
//!
//! [`HeaderVrf::lottery_value`] is the number to use: the same range-extension
//! construction with the application's own domain tag, which is uniform over
//! `[0, 2^256)` no matter who produced the block, while staying just as
//! ungrindable — the producer cannot choose its VRF output, only whether to
//! make the block at all.

use crate::blake2b;
use crate::header::HeaderVrf;

/// The domain tag the chain uses for the leader value.
const LEADER_DOMAIN: u8 = b'L';

/// Bits an `f64` mantissa holds exactly — the most a fraction can carry
/// without rounding.
const F64_MANTISSA_BITS: u32 = 53;

/// 2^53, the divisor that turns those bits into a fraction.
const TWO_POW_53: f64 = 9_007_199_254_740_992.0;

/// A range-extended VRF value, together with the bound it is drawn from.
///
/// Comparisons are expressed as a fraction of that bound rather than as a
/// big integer: the two eras have different bounds (2^256 and 2^512), and a
/// fraction is the only form in which they mean the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VrfValue {
    /// Babbage and Conway, and every application lottery: a blake2b-256 of
    /// the domain tag and the output, drawn from `[0, 2^256)`.
    Hashed([u8; 32]),
    /// Shelley to Alonzo: the raw 64-byte output, drawn from `[0, 2^512)`.
    Raw([u8; 64]),
}

impl VrfValue {
    /// The value's big-endian bytes.
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Hashed(h) => h,
            Self::Raw(o) => o,
        }
    }

    /// The value as a fraction of its bound, in `[0, 1)`.
    ///
    /// Taken from the leading 8 bytes, which both bounds are a whole number of
    /// bytes wider than, so this is the top of the value in either era, then
    /// narrowed to the bits an `f64` holds exactly. Dividing all 64 bits would
    /// round the largest values up to exactly `1.0` and break the half-open
    /// range this promises; 53 bits still resolve to about 1e-16, far finer
    /// than any threshold worth expressing as an `f64`.
    pub fn fraction(&self) -> f64 {
        let mut lead = [0u8; 8];
        lead.copy_from_slice(&self.bytes()[..8]);
        (u64::from_be_bytes(lead) >> (64 - F64_MANTISSA_BITS)) as f64 / TWO_POW_53
    }

    /// Whether the value falls below `fraction` of its bound — the shape of
    /// every VRF threshold test, including the chain's own leader check.
    ///
    /// A `fraction` of zero (or NaN) never fires; one at or above 1 always
    /// does. Both are answered without consulting the value, so a
    /// misconfigured threshold degrades to "never" or "always" rather than to
    /// something subtly stake-dependent.
    pub fn below(&self, fraction: f64) -> bool {
        if fraction.is_nan() || fraction <= 0.0 {
            return false;
        }
        if fraction >= 1.0 {
            return true;
        }
        self.fraction() < fraction
    }
}

/// A lottery draw over a raw VRF output: `blake2b-256(domain || output)`.
///
/// The one place the construction lives, so a draw taken from a header and a
/// draw taken from a relayed [`crate::BlockBeat`] are the same number.
pub fn lottery_value_from_output(domain: &[u8], output: &[u8]) -> VrfValue {
    let mut tagged = Vec::with_capacity(domain.len() + output.len());
    tagged.extend_from_slice(domain);
    tagged.extend_from_slice(output);
    VrfValue::Hashed(blake2b::<32>(&tagged))
}

impl HeaderVrf {
    /// The value the chain itself compared against this producer's leadership
    /// threshold: `blake2b-256("L" || output)` from Babbage on, and the raw
    /// output before it.
    ///
    /// This is the chain's own number, useful for auditing or reproducing a
    /// leader check. It is **not** a uniform per-block random draw — see the
    /// module header — so reach for [`Self::lottery_value`] to select blocks.
    pub fn leader_value(&self) -> VrfValue {
        let output = self.leader().output;
        match self {
            Self::Praos { .. } => {
                let mut tagged = [0u8; 65];
                tagged[0] = LEADER_DOMAIN;
                tagged[1..].copy_from_slice(&output);
                VrfValue::Hashed(blake2b::<32>(&tagged))
            }
            Self::TPraos { .. } => VrfValue::Raw(output),
        }
    }

    /// A value for the caller's own lottery over blocks: `blake2b-256(domain
    /// || output)`, uniform on `[0, 2^256)` whoever produced the block.
    ///
    /// `domain` separates one application's draw from another's and from the
    /// chain's `"L"` and `"N"`; give each independent draw its own tag, and
    /// version it (`b"slotlings-v1"`) so a later rule change cannot be
    /// mistaken for the old one.
    ///
    /// The result is era-independent: it is built from the leader
    /// certificate's output, which both header shapes carry.
    pub fn lottery_value(&self, domain: &[u8]) -> VrfValue {
        lottery_value_from_output(domain, &self.leader().output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::split_block;
    use crate::header::{BlockHeader, VrfCert};

    const BLOCK: &[u8] = include_bytes!("../tests/fixtures/186000000.block.cbor");

    fn conway_header() -> BlockHeader {
        let parts = split_block(BLOCK).unwrap();
        BlockHeader::decode(parts.header_variant, parts.header_cbor).unwrap()
    }

    /// The derivation is checked against the chain itself rather than against a
    /// constant copied from this crate.
    ///
    /// A block exists only because its leader value fell below its producer's
    /// threshold, `1 - (1-f)^sigma`. With `f = 0.05`, even a pool holding a
    /// tenth of the stake is under 0.006, and real pools sit far below that.
    /// So the correct derivation must land very near zero, while a wrong one
    /// (a different tag, no tag, the raw output, the block hash) is a
    /// uniform draw on `[0, 1)` that clears 0.01 ninety-nine times in a
    /// hundred. Passing is therefore evidence about the derivation, not just a
    /// restatement of it.
    #[test]
    fn the_leader_value_of_a_real_conway_block_sits_below_its_producers_threshold() {
        let value = conway_header().vrf.leader_value();
        let fraction = value.fraction();
        assert!(
            fraction < 0.01,
            "leader value {fraction} is too large to have won a slot; \
             the range extension is probably wrong"
        );
        assert!(value.below(0.01));
        assert!(!value.below(0.0));
    }

    /// Locks the exact bytes, so a later refactor of the tagging cannot drift
    /// while still landing somewhere plausibly small.
    #[test]
    fn the_leader_value_is_stable() {
        let VrfValue::Hashed(bytes) = conway_header().vrf.leader_value() else {
            panic!("a Conway header range-extends to a 32-byte value");
        };
        assert_eq!(
            hex::encode(bytes),
            "00032ea239ff087a7a67d73c541df4bec680696ab7c069c97e3bf0cdf3d1295b"
        );
    }

    #[test]
    fn a_pre_babbage_leader_value_is_the_raw_output() {
        let vrf = HeaderVrf::TPraos {
            nonce: VrfCert {
                output: [3; 64],
                proof: [4; 80],
            },
            leader: VrfCert {
                output: [5; 64],
                proof: [6; 80],
            },
        };
        assert_eq!(vrf.leader_value(), VrfValue::Raw([5; 64]));
        // 0x0505…/2^64, the leading bytes of the output read as a fraction.
        assert!((vrf.leader_value().fraction() - 0.019_607_843).abs() < 1e-6);
    }

    /// The whole point of the lottery value: it is a different draw from the
    /// chain's leader check, and from every other application's.
    #[test]
    fn a_lottery_value_is_its_own_draw_per_domain() {
        let vrf = conway_header().vrf;
        let mine = vrf.lottery_value(b"slotlings-v1");
        assert_ne!(mine, vrf.leader_value());
        assert_ne!(mine, vrf.lottery_value(b"slotlings-v2"));
        assert_ne!(mine, vrf.lottery_value(b""));
        // Deterministic: the same header and tag always draw the same number.
        assert_eq!(mine, vrf.lottery_value(b"slotlings-v1"));
    }

    /// A lottery draw is unconstrained by the producer's stake, so unlike the
    /// leader value it is free to land anywhere. This block's draw is nowhere
    /// near a winning threshold, which a leader value could never be.
    #[test]
    fn a_lottery_value_is_not_pinned_near_zero() {
        let fraction = conway_header()
            .vrf
            .lottery_value(b"slotlings-v1")
            .fraction();
        assert!(
            fraction > 0.01,
            "expected a uniform draw, got {fraction}, which looks like a leader value"
        );
    }

    #[test]
    fn a_threshold_of_zero_or_one_never_consults_the_value() {
        let value = VrfValue::Hashed([0xff; 32]);
        assert!(!value.below(0.0));
        assert!(!value.below(-1.0));
        assert!(!value.below(f64::NAN));
        assert!(value.below(1.0));
        assert!(value.below(2.0));
        // The largest possible value is still below its bound.
        assert!(value.fraction() < 1.0);
    }
}
