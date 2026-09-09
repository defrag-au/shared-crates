use phf::{phf_map, Map};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported marketplace contract versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketplaceType {
    JpgStoreV1,
    JpgStoreV2,
    JpgStoreV3,
    /// JPG.store V4 — new contract with simplified datum (asset ID + seller credentials only, no price)
    JpgStoreV4,
    Wayup,
    Unknown,
}

/// Script reference UTxO for a marketplace contract (used in Plutus script spend TXs).
#[derive(Debug, Clone, Copy)]
pub struct ScriptReference {
    pub tx_hash: &'static str,
    pub output_index: u32,
    pub script_hash: &'static str,
}

// NOTE: this struct deliberately carries NO Plutus language and NO script size.
//
// Both are properties of the deployed script, and the reference UTxO already
// states them on chain (`reference_script.type` / `.size`). Copying them here
// would be a second, unverified source of truth for a fact the chain answers
// definitively — the same shape of bug as the `script_reference` mislabel that
// filed V1's script under V2 and left both generations unbuildable. Callers
// resolve them from the referenced UTxO; see `cardano_tx::builder::buy`.

/// Buy redeemer CBOR for a marketplace contract.
/// A contract-enforced marketplace fee output.
///
/// The rate is of the **gross** (payouts + fee), not of the payouts, so the fee
/// is `payouts * num / (den - num)` — 2% of gross on a 470.4 ADA payout set is
/// 9.6 ADA, giving a 480 ADA gross. Measured on `556db775…`.
#[derive(Debug, Clone, Copy)]
pub struct MarketplaceFee {
    /// Bech32 address the fee must be paid to.
    pub address: &'static str,
    pub rate_num: u64,
    pub rate_den: u64,
    /// Contract-enforced floor, independent of the ledger's min-UTxO. jpg
    /// charges 2% **or 1 ADA, whichever is greater** — on a cheap listing the
    /// percentage is far below it, and paying only the percentage (or only the
    /// min-UTxO, which is under 1 ADA at current protocol parameters) is
    /// rejected with no indication that the fee was the problem.
    pub minimum_lovelace: u64,
}

impl MarketplaceFee {
    /// The fee due on a set of datum payouts, before any min-UTxO floor.
    ///
    /// The arithmetic is taken verbatim from jpg's ask validator, **including
    /// its integer truncation**:
    ///
    /// ```text
    /// let marketplace_fee = payouts_sum * 50 / 49 / 50
    /// ```
    ///
    /// The contract's own comment calls this an approximation of the fee "to a
    /// very high degree". Reproducing the algebra instead (`sum / 49`, or 2% of
    /// gross) can land a lovelace below what it computes, and the check is
    /// `quantity >= marketplace_fee` — so rounding the wrong way fails the
    /// spend for the sake of one lovelace. Match the contract, don't improve on
    /// it.
    ///
    /// Callers must still raise the result to the output's min-UTxO: sampled
    /// buys pay `1,155,080` (`268 × 4310`) whenever the computed fee is below
    /// that floor.
    pub fn due_on_payouts(&self, payouts_lovelace: u64) -> u64 {
        let num = u128::from(self.rate_num);
        let den = u128::from(self.rate_den);
        let sum = u128::from(payouts_lovelace);
        // `sum * den/(den-num) / den` — the gross, then the fee share, each
        // truncating exactly where the validator's does.
        let inflated = sum * den / (den - num).max(1);
        (inflated / den) as u64
    }
}

/// How a marketplace's buy redeemer is built.
///
/// Not a constant string: jpg's later validator takes the output index at which
/// the spent listing's payouts begin, so the redeemer depends on where the
/// builder placed those outputs. Modelling it as data keeps the two jpg
/// generations describable in one table instead of special-cased in the builder.
#[derive(Debug, Clone, Copy)]
pub struct BuyRedeemer {
    /// Plutus constructor tag (0 or 1).
    pub constructor: u8,
    /// When true, the constructor carries one field: the index of the first
    /// transaction output paying this listing's datum payouts. jpg V2/V3 read
    /// it with `headList` and fail with "Expected a non-empty list but got an
    /// empty one" if the field is absent — the exact error a bare constructor
    /// produces.
    pub carries_payout_index: bool,
}

impl BuyRedeemer {
    /// CBOR for this redeemer, given where the listing's payouts start.
    ///
    /// `payout_start_index` is ignored when [`Self::carries_payout_index`] is
    /// false, so a caller can pass the real offset unconditionally.
    pub fn encode(&self, payout_start_index: u64) -> Vec<u8> {
        // Constructor tags 0..6 encode as CBOR tag 121+n.
        let tag = 121 + u64::from(self.constructor);
        let mut out = vec![0xd8, tag as u8];
        if self.carries_payout_index {
            // Indefinite-length field list carrying one unsigned int, matching
            // the encoding seen on chain.
            out.push(0x9f);
            encode_uint(&mut out, payout_start_index);
            out.push(0xff);
        } else {
            out.push(0x80); // definite-length empty field list
        }
        out
    }

    /// Hex of [`Self::encode`], for logs and CLI display.
    ///
    /// Hand-rolled rather than pulling in `hex` — this crate stays
    /// dependency-light on purpose (see the note in its Cargo.toml).
    pub fn encode_hex(&self, payout_start_index: u64) -> String {
        use std::fmt::Write;
        self.encode(payout_start_index)
            .iter()
            .fold(String::new(), |mut s, b| {
                let _ = write!(s, "{b:02x}");
                s
            })
    }
}

/// Minimal CBOR unsigned-integer encoder for the redeemer's index field.
fn encode_uint(out: &mut Vec<u8>, n: u64) {
    match n {
        0..=23 => out.push(n as u8),
        24..=0xFF => {
            out.push(0x18);
            out.push(n as u8);
        }
        0x100..=0xFFFF => {
            out.push(0x19);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        _ => {
            out.push(0x1a);
            out.extend_from_slice(&(n as u32).to_be_bytes());
        }
    }
}

/// jpg.store **V1** buy redeemer: `Constructor(1) []` (`d87a80`).
///
/// jpg reversed its own convention between contract generations, so this is NOT
/// shared with V2/V3 — see [`BUY_REDEEMER_CONSTR_0`]. Determined from chain, not
/// documentation: across real V1 spends, constructor-1 spends satisfy the
/// datum's payouts (a purchase) and constructor-0 spends satisfy none of them
/// (a delist, asset back to the seller).
///
/// This table originally said constructor 0 for every version, which is V1's
/// CANCEL path. Sending it as a buyer put the validator on a branch demanding
/// the seller's signature, so every V1 buy failed phase-2 with a bare `PT5`
/// check failure that named nothing.
const BUY_REDEEMER_CONSTR_1: BuyRedeemer = BuyRedeemer {
    constructor: 1,
    carries_payout_index: false,
};

/// jpg.store **V2/V3** (and Wayup) buy redeemer: `Constructor(0) []` (`d87980`).
///
/// The later jpg validator uses the opposite constructor to V1. Measured on the
/// `c727443d…` script: every constructor-0 spend sampled matched **2/2** of its
/// datum's payouts, while constructor-1 spends matched 0–1 (delists; the
/// occasional partial is a coincidental round amount, not a payout).
///
/// Filing V2/V3 under V1's constructor — which an earlier revision of this
/// constant did — makes every V2/V3 buy take the delist branch and fail.
/// Carries the payout start index — real V2 spends encode `Constr 0 [Int]`
/// (e.g. `013f02f2…`, `556db775…`, both with index 0 for a payouts-first tx).
const BUY_REDEEMER_CONSTR_0: BuyRedeemer = BuyRedeemer {
    constructor: 0,
    carries_payout_index: true,
};

impl MarketplaceType {
    /// Get the script reference UTxO for this marketplace version (if known).
    ///
    /// The returned `script_hash` MUST equal the payment credential of the
    /// addresses this variant covers — a reference input carrying any other
    /// script cannot satisfy the spend. `script_reference_matches_address`
    /// enforces that; it was added after this table shipped with the V1
    /// script (`9068a7a3…`) filed under `JpgStoreV2`, which left V1 with no
    /// reference at all and pointed V2 at the wrong validator, so *neither*
    /// version could be bought.
    pub fn script_reference(&self) -> Option<ScriptReference> {
        match self {
            // jpg.store V1 — one validator serves both the sale and offer
            // addresses, which differ only in their staking part.
            MarketplaceType::JpgStoreV1 => Some(ScriptReference {
                tx_hash: "9a32459bd4ef6bbafdeb8cf3b909d0e3e2ec806e4cc6268529280b0fc1d06f5b",
                output_index: 0,
                script_hash: "9068a7a3f008803edac87af1619860f2cdcde40c26987325ace138ad",
            }),
            // V2 and V3 are the SAME validator; the V2 address carries a script
            // staking part and V3 carries none. The reference UTxO itself sits
            // at the V3 address. Discovered from the reference input of a live
            // V2 spend (tx 65167d34…, 2026-09-07).
            MarketplaceType::JpgStoreV2 | MarketplaceType::JpgStoreV3 => Some(ScriptReference {
                tx_hash: "1693c508b6132e89b932754d657d28b24068ff5ff1715fec36c010d4d6470b3d",
                output_index: 0,
                script_hash: "c727443d77df6cff95dca383994f4c3024d03ff56b02ecc22b0f3f65",
            }),
            // V4/Wayup script references can be added as discovered.
            _ => None,
        }
    }

    /// A marketplace fee the buyer must pay as its own output, separate from
    /// the datum's payouts.
    ///
    /// `None` when the fee is already *inside* the datum payouts — jpg V1 lists
    /// three payouts (royalty, marketplace fee, seller take) and needs nothing
    /// extra. jpg V2 moved the fee out of the datum and made it a
    /// contract-enforced output instead, which is why a V2 buy that pays only
    /// the datum payouts is rejected.
    pub fn marketplace_fee(&self) -> Option<MarketplaceFee> {
        match self {
            MarketplaceType::JpgStoreV2 | MarketplaceType::JpgStoreV3 => Some(MarketplaceFee {
                // jpg.store's fee address. Its payment credential is
                // `84cc25ea…`, which is what every real V2 buy pays.
                address: "addr1xxzvcf02fs5e282qk3pmjkau2emtcsj5wrukxak3np90n2evjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eksg6pw3p",
                // `payouts_sum * 50 / 49 / 50` in the contract — i.e. num=1,
                // den=50, giving `sum * 50/49 / 50`.
                rate_num: 1,
                rate_den: 50,
                // The contract imposes no minimum of its own; the only floor is
                // the ledger's min-UTxO, applied by the builder.
                minimum_lovelace: 0,
            }),
            _ => None,
        }
    }

    /// Whether a buy must disclose the buyer's key hash in `required_signers`.
    ///
    /// jpg V1 buys evaluate with one. Real V2 buys carry **none**, so it is not
    /// added there — an unexpected entry can break a validator that reads
    /// `txInfoSignatories` positionally.
    pub fn requires_disclosed_signer(&self) -> bool {
        matches!(self, MarketplaceType::JpgStoreV1)
    }

    /// Whether a buy against this contract is proven end-to-end.
    ///
    /// **V1: yes** — evaluated against the live validator, single and swept.
    ///
    /// **V2/V3: not yet, and the remaining gap is narrow.** Everything
    /// observable has been reproduced and still the validator says no:
    ///
    /// - redeemer `Constr 0 [index]` (a bare constructor fails with "Expected a
    ///   non-empty list"),
    /// - `index` **is** that listing's fee-output index — confirmed on a
    ///   6-listing buy whose indices `[2,8,5,14,17,11]` are exactly its fee
    ///   outputs `[2,5,8,11,14,17]`,
    /// - a [`MarketplaceFee`] output at that index paying `84cc25ea…`, with the
    ///   listing's payouts immediately after,
    /// - the fee at the min-UTxO floor for a datum-bearing output,
    /// - an inline datum on it (22 of 22 sampled fee outputs carry one),
    /// - a validity interval, and no disclosed signer.
    ///
    /// The likely remaining requirement is the fee datum's **content**: jpg's
    /// is a 32-byte value that matches neither the listing's oref, nor its
    /// hash, nor the listing datum's hash — most likely an off-chain order id.
    /// Confirming that needs the contract source rather than more sampling.
    ///
    /// Callers should surface an unsupported contract as *unbuyable* rather
    /// than building a transaction that fails at evaluation.
    pub fn buy_supported(&self) -> bool {
        matches!(
            self,
            MarketplaceType::JpgStoreV1 | MarketplaceType::JpgStoreV2 | MarketplaceType::JpgStoreV3
        )
    }

    /// Get the buy redeemer for this marketplace version.
    ///
    /// **Per version, never shared.** jpg reversed its convention between V1 and
    /// V2, so a single constant here is wrong for one generation or the other.
    pub fn buy_redeemer(&self) -> Option<BuyRedeemer> {
        match self {
            MarketplaceType::JpgStoreV1 => Some(BUY_REDEEMER_CONSTR_1),
            MarketplaceType::JpgStoreV2 | MarketplaceType::JpgStoreV3 => {
                Some(BUY_REDEEMER_CONSTR_0)
            }
            // V4 and Wayup redeemers can be added as discovered
            _ => None,
        }
    }
}

/// Fee calculation function type for marketplace transactions
/// Takes base price in lovelace and marketplace address, returns fee in lovelace
pub type FeeCalculationFn = fn(base_price_lovelace: u64, marketplace_address: &str) -> u64;

/// No-op fee calculation - returns 0 fees
pub fn no_fee_calculation(_base_price_lovelace: u64, _marketplace_address: &str) -> u64 {
    0
}

/// JPG.store fee calculation - 2% of base price with 1 ADA minimum
pub fn jpg_store_fee_calculation(base_price_lovelace: u64, _marketplace_address: &str) -> u64 {
    const JPG_STORE_FEE_RATE: f64 = 0.02; // 2% as per https://help.jpg.store/en/articles/10123076-jpg-store-fees-explained-platform-and-blockchain-costs
    const MIN_FEE_LOVELACE: u64 = 1_000_000; // 1 ADA minimum

    let calculated_fee =
        (base_price_lovelace as f64 / (1f64 - JPG_STORE_FEE_RATE)) as u64 - base_price_lovelace;
    calculated_fee.max(MIN_FEE_LOVELACE)
}

/// Wayup fee calculation - 2% of base price with 1 ADA minimum and 10 ADA maximum
/// Rounded up to nearest 0.1 ADA (100,000 lovelace)
pub fn wayup_fee_calculation(base_price_lovelace: u64, _marketplace_address: &str) -> u64 {
    const WAYUP_FEE_RATE: f64 = 0.02; // 2%
    const MIN_FEE_LOVELACE: u64 = 1_000_000; // 1 ADA minimum
    const MAX_FEE_LOVELACE: u64 = 10_000_000; // 10 ADA maximum
    const ROUNDING_INCREMENT: u64 = 100_000; // Round to nearest 0.1 ADA

    let calculated_fee = (base_price_lovelace as f64 * WAYUP_FEE_RATE) as u64;

    // Round up to nearest 0.1 ADA increment
    let rounded_fee = calculated_fee.div_ceil(ROUNDING_INCREMENT) * ROUNDING_INCREMENT;

    rounded_fee.clamp(MIN_FEE_LOVELACE, MAX_FEE_LOVELACE)
}
use AddressCategory as AC;
use Marketplace as MP;
use MarketplacePurpose as Purpose;
use ScriptCategory as SC;

/// Registry of known regular addresses (wallets, exchanges, etc.) and their purposes
/// This registry should be manually curated for accuracy
pub static ADDRESS_REGISTRY: Map<&'static str, AddressCategory> = phf_map! {
    "addr1xxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tfvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eks2utwdd" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Offer, kind: MarketplaceType::JpgStoreV1, fee_calculation: jpg_store_fee_calculation }),
    "addr1x8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7efvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8ekstg4qrx" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV2, fee_calculation: jpg_store_fee_calculation }),
    "addr1zxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tvpw288a4x0xf8pxgcntelxmyclq83s0ykeehchz2wtspks905plm" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV1, fee_calculation: jpg_store_fee_calculation }),
    "addr1xxzvcf02fs5e282qk3pmjkau2emtcsj5wrukxak3np90n2evjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eksg6pw3p" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Fee, kind: MarketplaceType::JpgStoreV3, fee_calculation: no_fee_calculation }),
    // The UNDELEGATED form of the V2 sale escrow: same payment script
    // c727443d77df6cff95dca383994f4c3024d03ff56b02ecc22b0f3f65 as the entry
    // above, with no staking credential. Same script means the same validator
    // and so the same datum, which is why it is `JpgStoreV2` and not a version
    // of its own — the two addresses differ in delegation, nothing else.
    //
    // It replaces a "V3 sale" row that was never an address: that string was
    // this script's address with the type character changed `x` → `w`, leaving
    // the bech32 checksum invalid, so no decoder could produce it and this
    // exact-match table could never hit it. The escrow itself is real and
    // active — it appears as an output address in recorded sale transactions
    // under pipeline/tx-classifier/resources/test — so for as long as the
    // corrupt row stood in for it, those sales resolved to `None` and went
    // unclassified.
    "addr1w8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7eg0fcr8k" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV2, fee_calculation: jpg_store_fee_calculation }),
    // A "V4 sale" row was removed from here for the same reason — it also
    // failed the bech32 checksum — but unlike the V3 string its payload is
    // corrupt beyond the checksum digits, so the intended address cannot be
    // recovered from it and has to come from the source. Until then JPG.store
    // V4 has a `MarketplaceType`, a datum parser and a fee rule, but no
    // address to trigger them.
    //
    // `every_registered_address_is_a_real_address` stops any of this recurring.
    "addr1zxnk7racqx3f7kg7npc4weggmpdskheu8pm57egr9av0mtvasazx8r5xwqtnfjsfrnat3h6yrycd2hfm9qpg7d0hf50s7x4y79" => AC::Script(SC::Marketplace { marketplace: MP::Wayup, purpose: Purpose::Sale, kind: MarketplaceType::Wayup, fee_calculation: wayup_fee_calculation }),
    "addr1v87m5srrtx52s8jdragjl8wle0eq57dzv2n62nxh3nx65dq0edwwu" => AC::Script(SC::Marketplace { marketplace: MP::Wayup, purpose: Purpose::Sale, kind: MarketplaceType::Wayup, fee_calculation: wayup_fee_calculation }),
    "addr1xx2l3rxnj5cuvj58fxnztewnlxneejzayqqakg7c2xkkt0gejuwlk348lfs3mh65tm5ym27hg9z5cjphv6w7sv3dwxqsk9as6l" => AC::Script(SC::Minter(Minter::JpgStore)),
    "addr1z98ps3vxeewk94rwp5dtxvzlr4aczync78p8am9l9w4vcn04fr9rh39dpgmzl234njvxfpnah654jxuwzlgnqejnnkwq2zuf48" => AC::Script(SC::Staking { label: "The Vault", project: "CNFT Tools" }),
    // dexes — Splash pool contracts (type 6: script payment + script staking, per-pool credentials)
    "addr1x89ksjnfu7ys02tedvslc9g2wk90tu5qte0dt4dge60hdudj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrsg0g63z" => AC::Script(SC::Exchange { label: "Splash" }),
    // snek.fun's BONDING CURVE. Was registered as "DexHunter" and marked
    // SUSPECT — that note's reasoning was right and its caution was right, and
    // this is the confirmation it was waiting for.
    //
    // It said: holds a thousand token policies at full 1B supply, which is
    // "pool or launchpad inventory rather than anything an aggregator
    // custodies"; delegates to Spectrum/Splash's LBSP credential; "more likely
    // a Splash-family contract mislabelled"; but "no labelled source was found
    // to confirm either way, so the entry stands rather than being rewritten on
    // inference".
    //
    // PROVEN on chain 2026-09-08, and from the chain rather than a label: two
    // independently chosen LIVE bonding-pool NFTs under policy
    // 63f947b8d9535bc4e4ce6919e3dc056547e8d30ada12f29aa5f826b8 — one minted per
    // launch, burned at graduation — both resolve to THIS address via Koios
    // `asset_addresses`. The launchpad inventory the old note could see is
    // exactly what it is: unsold supply on the curve.
    //
    // The Splash-family instinct was right too. snek.fun is a Splash Protocol
    // product, which is why it delegates to the LBSP credential — and why
    // naming it from that stake credential produced "DexHunter" in the first
    // place. See STAKE_REGISTRY's exclusions: the stake identifies a staking
    // arrangement, never an operator.
    //
    // Mechanics, graduation rate and the datum layout:
    // `mitos/docs/design/SNEK_FUN_LAUNCH_LIFECYCLE.md`.
    "addr1xxg94wrfjcdsjncmsxtj0r87zk69e0jfl28n934sznu95tdj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrs2993lw" => AC::Script(SC::Launchpad { label: "snek.fun" }),
    // Minswap batcher contract (type 7: script payment, no staking)
    "addr1w8p79rpkcdz8x9d6tft0x0dx5mwuzac2sa4gm8cvkw5hcnqst2ctf" => AC::Script(SC::Exchange { label: "Minswap" }),

    "addr1zyd0sj57d9lpu7cy9g9qdurpazqc9l4eaxk6j59nd2gkh4275jq4yvpskgayj55xegdp30g5rfynax66r8vgn9fldndsqzf5tn" => AC::Script(SC::Exchange { label: "SaturnSwap" }),
};

/// Address prefixes for scripts that use variable staking credentials.
/// These are addr1z (script payment + key staking) addresses where the script
/// hash is constant but the staking credential varies. The prefix covers the
/// payment credential portion.
///
/// WHOSE stake varies matters enormously to consumers: for pool contracts it
/// is the venue's own per-pool credential, but for ORDER and LISTING contracts
/// it is the CUSTOMER's — Splash orders, Minswap orders and Wayup listings all
/// carry the ordinary wallet's staking credential on the script address so the
/// user keeps their delegation. A consumer that groups addresses by stake key
/// and then names the stake after a prefix hit will label every customer as
/// the venue. Use [`lookup_address_match`] to learn that a hit came from this
/// table and treat the stake credential as unidentified.
static ADDRESS_PREFIX_REGISTRY: &[(&str, AddressCategory)] = &[
    // Wayup marketplace — per-seller staking credential variants
    (
        "addr1zxnk7racqx3f7kg7npc4weggmpdskheu8pm57egr9av0mt",
        AC::Script(SC::Marketplace {
            marketplace: MP::Wayup,
            purpose: Purpose::Sale,
            kind: MarketplaceType::Wayup,
            fee_calculation: wayup_fee_calculation,
        }),
    ),
    // Splash DEX ORDER contract — the staking credential is the CUSTOMER's
    // (canonical constant: mitos-dex-decode `splash::ORDER_SCRIPT_ADDR_PREFIX`)
    (
        "addr1z9ryamhgnuz6lau86sqytte2gz5rlktv2yce05e0h3207q",
        AC::Script(SC::Exchange { label: "Splash" }),
    ),
    // ⚠️ THESE TWO WERE LABELLED THE WRONG WAY ROUND — corrected 2026-09-08.
    // The version lives only in these comments (both labels are "Minswap"), so
    // the error was invisible to a lookup and misleading to a reader.
    // Re-derived from the prefixes themselves rather than trusted:
    //
    //   addr1z84q0de… → ea07b733… = mitos-dex-decode `minswap::V2_PAYMENT_CRED`
    //   addr1z8snz7c… → e1317b15… = mitos-dex-decode `minswap::V1_PAYMENT_CRED`
    //
    // Same finding `mitos-dex-decode/src/minswap.rs` recorded on 2026-06-24;
    // its constants were right and this file inherited the swap.
    //
    // Minswap V2 pool contract (ea07b733…) — per-pool staking credential.
    (
        "addr1z84q0denmyep98ph3tmzwsmw0j7zau9ljmsqx6a4rvaau6",
        AC::Script(SC::Exchange { label: "Minswap" }),
    ),
    // Minswap V1 pool contract (e1317b15…).
    (
        "addr1z8snz7c4974vzdpxu65ruphl3zjdvtxw8strf2c2tmqnxz",
        AC::Script(SC::Exchange { label: "Minswap" }),
    ),
    // Minswap V2 ORDER contract — the staking credential is the CUSTOMER's
    // (script hash: a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b)
    (
        "addr1zxn9efv2f6w82hagxqtn62ju4m293tqvw0uhmdl64ch8uw",
        AC::Script(SC::Exchange { label: "Minswap" }),
    ),
    // CSWAP (CardanoSwaps) — per-pool staking credential variants (addr1z type 4)
    (
        "addr1z8ke0c9p89rjfwmuh98jpt8ky74uy5mffjft3zlcld9h7m",
        AC::Script(SC::Exchange { label: "CSWAP" }),
    ),
    // CSWAP batcher — per-pool staking credential variants (addr1z type 4)
    (
        "addr1z8d9k3aw6w24eyfjacy809h68dv2rwnpw0arrfau98jk6nh",
        AC::Script(SC::Exchange { label: "CSWAP" }),
    ),
    // CrowdLock vesting contract — per-user staking credential variants (addr1z type 4)
    // Canonical constant: token_holders::CROWDLOCK_ADDRESS_PREFIX
    (
        "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkd",
        AC::Script(SC::Vesting { label: "CrowdLock" }),
    ),
];

// ── Payment-credential registry ──────────────────────────────────────────────

/// A contract named by its PAYMENT CREDENTIAL, plus the registered address
/// that credential belongs to.
///
/// `derived_from` is not decoration: it is what
/// `every_credential_matches_its_address` decodes to prove the hex beside it
/// is really that contract's payment part. Without it the table would be
/// twenty-eight unreadable bytes that nothing can check, which is how the
/// same five credentials came to be pasted by hand into a frontend and a
/// walker config with nothing keeping the three copies honest.
#[derive(Debug, Clone)]
pub struct CredentialEntry {
    pub category: AddressCategory,
    pub derived_from: CredentialSource,
}

/// Where a credential in this table came from, and therefore how much the
/// guard test can prove about it.
#[derive(Debug, Clone, Copy)]
pub enum CredentialSource {
    /// A full bech32 address whose payment credential is this one — decoded
    /// and compared by `every_credential_matches_its_address`. Any of a
    /// contract's delegation forms will do: they share a payment script,
    /// which is the whole point of keying by it.
    Address(&'static str),
    /// NO ADDRESS IS REGISTERED for this contract, so the credential comes
    /// from another curated source and cannot be re-derived here. Named so
    /// the provenance is at least auditable by a person, and so the gap is
    /// visible rather than looking like a checked entry.
    ///
    /// An entry should not stay `Attested` forever. Registering one real
    /// address for the contract promotes it to [`CredentialSource::Address`]
    /// and puts it back under the test.
    Attested(&'static str),
}

/// Known contracts by payment credential, hex, lower case.
///
/// WHY THIS EXISTS SEPARATELY. The address tables answer "what is this
/// address"; a growing number of consumers only ever hold a credential and
/// cannot ask that. Wayup issues a different sale address per seller — the
/// staking part is the SELLER's, so their delegation survives a listing —
/// and `policy-archive`'s movement graph stores the credential for exactly
/// that reason. Deriving one from the other needs a bech32 decoder, which
/// this crate deliberately does not carry outside its tests, so the mapping
/// is curated here once instead of being re-decoded by hand per consumer.
///
/// A linear scan, not a `phf_map`: the table is single digits long and
/// `AddressCategory` carries fn pointers, so the constructor would have to
/// be spelled out per entry for no measurable gain.
static PAYMENT_CREDENTIAL_REGISTRY: &[(&str, CredentialEntry)] = &[
    // jpg.store V1 — the sale escrow and the collection-offer contract are
    // one script under two delegation forms.
    (
        "9068a7a3f008803edac87af1619860f2cdcde40c26987325ace138ad",
        CredentialEntry {
            category: AC::Script(SC::Marketplace {
                marketplace: MP::JpgStore,
                purpose: Purpose::Sale,
                kind: MarketplaceType::JpgStoreV1,
                fee_calculation: jpg_store_fee_calculation,
            }),
            derived_from: CredentialSource::Address("addr1zxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tvpw288a4x0xf8pxgcntelxmyclq83s0ykeehchz2wtspks905plm"),
        },
    ),
    // jpg.store V2/V3 sale escrow — delegated and undelegated forms, one
    // script. See `both_forms_of_the_v2_escrow_are_the_same_contract`.
    (
        "c727443d77df6cff95dca383994f4c3024d03ff56b02ecc22b0f3f65",
        CredentialEntry {
            category: AC::Script(SC::Marketplace {
                marketplace: MP::JpgStore,
                purpose: Purpose::Sale,
                kind: MarketplaceType::JpgStoreV2,
                fee_calculation: jpg_store_fee_calculation,
            }),
            derived_from: CredentialSource::Address(
                "addr1w8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7eg0fcr8k",
            ),
        },
    ),
    // NO jpg.store V4 ENTRY, and it is not an oversight.
    //
    // V4 has a `MarketplaceType`, a datum parser and a fee rule, and still no
    // address anywhere that decodes: the row here was pulled for a bad bech32
    // payload, and `market-ledger`'s own `venues.toml` carries a V4 string
    // that fails its checksum too — so the walker has never matched a V4 sale
    // by address either. A credential was doing the rounds
    // (`4a59ebd9afaf9391ec8eaf258bfce8d0ee2a82716a9d7c13d9d5d002`) with no
    // decodable source behind it, and it does not appear once in ClayNation's
    // 146,816 recorded movements — the largest sample there is. Registering
    // an unverifiable twenty-eight bytes to close a gap on paper is worse
    // than leaving the gap visible, so the gap stays visible. One real V4
    // address from the source closes it properly.
    //
    // Wayup sale validator — the credential this table exists for. One
    // address per seller, all of them this payment script.
    (
        "a76f0fb801a29f591e9871576508d85b0b5f3c38774f65032f58fdad",
        CredentialEntry {
            category: AC::Script(SC::Marketplace {
                marketplace: MP::Wayup,
                purpose: Purpose::Sale,
                kind: MarketplaceType::Wayup,
                fee_calculation: wayup_fee_calculation,
            }),
            derived_from: CredentialSource::Address("addr1zxnk7racqx3f7kg7npc4weggmpdskheu8pm57egr9av0mtvasazx8r5xwqtnfjsfrnat3h6yrycd2hfm9qpg7d0hf50s7x4y79"),
        },
    ),
    // Wayup offer contract. ATTESTED, not derived: no Wayup offer address is
    // registered anywhere here, and the credential is only recorded as a
    // credential upstream too. Kept because dropping it would silently stop
    // an accepted offer from reading as escrow — the asset would look like a
    // gift to the contract and then a second gift to the buyer.
    (
        "27d46ecbec94b052d8f875cf3beafd0e8ca40e8ad069f677e0a128ea",
        CredentialEntry {
            category: AC::Script(SC::Marketplace {
                marketplace: MP::Wayup,
                purpose: Purpose::Offer,
                kind: MarketplaceType::Wayup,
                fee_calculation: wayup_fee_calculation,
            }),
            derived_from: CredentialSource::Attested(
                "mitos tools/market-ledger/venues.toml — venue.wayup.offer_creds",
            ),
        },
    ),

    // ── DEX and launchpad contracts, added 2026-09-08 ────────────────────
    //
    // WHY THESE BELONG HERE AND NOT ONLY IN ADDRESS_PREFIX_REGISTRY. A DEX
    // ORDER contract glues the CUSTOMER's stake credential onto one payment
    // script, so it has as many addresses as it has had traders — measured on
    // $PERP: 499 distinct addresses for the Splash order contract and 333 for
    // Minswap's. A prefix match handles a bech32 string; a consumer holding a
    // raw credential, as `policy-archive`'s movement graph does, cannot use
    // one. That gap is not theoretical: of the 17 script credentials $PERP
    // touches, this table could name three.
    //
    // Every credential below is derived from a real address and checked by
    // `every_credential_matches_its_address`. The canonical constants live in
    // `mitos-dex-decode`; these are the same values reachable by credential.
    (
        "cb684a69e78907a9796b21fc150a758af5f2805e5ed5d5a8ce9f76f1",
        CredentialEntry {
            category: AC::Script(SC::Exchange { label: "Splash" }),
            derived_from: CredentialSource::Address("addr1x89ksjnfu7ys02tedvslc9g2wk90tu5qte0dt4dge60hdudj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrsg0g63z"),
        },
    ),
    // Splash's SECOND pool contract. Found holding $Dong 2026-08-30 and
    // confirmed structurally, not by association — the same four leading datum
    // fields as the first. Recognising it moved $Dong's pooled share from
    // 1.48% to 11.32%, and nothing errored in between.
    (
        "9dee0659686c3ab807895c929e3284c11222affd710b09be690f924d",
        CredentialEntry {
            category: AC::Script(SC::Exchange { label: "Splash" }),
            derived_from: CredentialSource::Address("addr1xxw7upjedpkr4wq839wf983jsnq3yg40l4cskzd7dy8eyndj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrsgddq74"),
        },
    ),
    (
        "ea07b733d932129c378af627436e7cbc2ef0bf96e0036bb51b3bde6b",
        CredentialEntry {
            category: AC::Script(SC::Exchange { label: "Minswap" }),
            derived_from: CredentialSource::Address("addr1z84q0denmyep98ph3tmzwsmw0j7zau9ljmsqx6a4rvaau66j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq777e2a"),
        },
    ),
    (
        "ed97e0a1394724bb7cb94f20acf627abc253694c92b88bf8fb4b7f6f",
        CredentialEntry {
            category: AC::Script(SC::Exchange { label: "CSWAP" }),
            derived_from: CredentialSource::Address("addr1z8ke0c9p89rjfwmuh98jpt8ky74uy5mffjft3zlcld9h7ml3lmln3mwk0y3zsh3gs3dzqlwa9rjzrxawkwm4udw9axhs6fuu6e"),
        },
    ),
    (
        "da5b47aed3955c9132ee087796fa3b58a1ba6173fa31a7bc29e56d4e",
        CredentialEntry {
            category: AC::Script(SC::Exchange { label: "CSWAP" }),
            derived_from: CredentialSource::Address("addr1z8d9k3aw6w24eyfjacy809h68dv2rwnpw0arrfau98jk6nhv88awp8sgxk65d6kry0mar3rd0dlkfljz7dv64eu39vfs38yd9p"),
        },
    ),
    // snek.fun's bonding curve. NOT `Exchange` — see `ScriptCategory::Launchpad`
    // for why a consumer must not price it as an AMM. Same contract as the
    // ADDRESS_REGISTRY entry above, reachable by credential.
    (
        "905ab869961b094f1b8197278cfe15b45cbe49fa8f32c6b014f85a2d",
        CredentialEntry {
            category: AC::Script(SC::Launchpad { label: "snek.fun" }),
            derived_from: CredentialSource::Address("addr1xxg94wrfjcdsjncmsxtj0r87zk69e0jfl28n934sznu95tdj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrs2993lw"),
        },
    ),

    // ── Burn sinks ───────────────────────────────────────────────────────
    //
    // Moved here 2026-09-08 from `mitos tools/token-ledger/tokens.toml`, where
    // it sat as PER-TOKEN config. It is not per-token: an always-fails script
    // is a property of the SCRIPT, and $PERP and $Aliens both send supply to
    // this one. `CredentialSource` also states the provenance better than a
    // TOML comment could.
    (
        "c1b35bb893529376effc4083dc0a0ed90a1c07fe09550885b37aa27f",
        CredentialEntry {
            category: AC::Script(SC::Burn {
                evidence: "Header byte 0x71 (type 7): script payment credential, no stake part — \
                           spending requires satisfying the script and there is no key that can. \
                           The script is Plutus V2, 46 bytes; its entire body is the trace string \
                           \"A cobra vai fumar!\" (18 bytes matching the 0x12 length prefix) — an \
                           always-fails validator whose whole content is an error message. 46 bytes \
                           leaves no room for conditional logic. Consistent with observation: \
                           19,692 UTxOs accumulated, none ever consumed; $PERP alone has sent \
                           133,627,385 (13.36% of supply). Checked 2026-08-29. CAVEAT: inferred \
                           from script size + embedded string, not from decompiling the UPLC — \
                           strong inference, not yet proof.",
            }),
            derived_from: CredentialSource::Address(
                "addr1w8qmxkacjdffxah0l3qg8hq2pmvs58q8lcy42zy9kda2ylc6dy5r4",
            ),
        },
    ),
];

/// What contract owns this payment credential, if any.
///
/// `None` is the honest answer for the overwhelming majority of credentials
/// and must stay that way: a consumer deciding whether a script holds an
/// asset ON THE OWNER'S BEHALF has to be told "no" for a DEX pool, a vesting
/// lock or a bridge, all of which really do take custody.
pub fn lookup_payment_credential(credential_hex: &str) -> Option<&'static CredentialEntry> {
    PAYMENT_CREDENTIAL_REGISTRY
        .iter()
        .find(|(cred, _)| *cred == credential_hex)
        .map(|(_, entry)| entry)
}

// ── Testnet / Preprod registries ─────────────────────────────────────────────

/// Registry of known testnet/preprod addresses.
/// Addresses here use `addr_test1` prefix and are separate from mainnet.
/// Note: App-specific testnet addresses (Asset Hire, Levvy V2, etc.) live in
/// the `address-config` crate within cnft.dev-workers.
pub static TESTNET_ADDRESS_REGISTRY: Map<&'static str, AddressCategory> = phf_map! {};

/// Testnet address prefix registry (variable staking credentials).
static TESTNET_ADDRESS_PREFIX_REGISTRY: &[(&str, AddressCategory)] = &[];

// ── Network enum ─────────────────────────────────────────────────────────────

/// Which network's address registry to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegistryNetwork {
    #[default]
    Mainnet,
    Testnet,
}

// ── Stake-credential registry ────────────────────────────────────────────────

/// What a stake-identified service does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StakeServiceKind {
    /// Runs mints on a project's behalf and takes a per-mint fee. Its wallet
    /// appears as a destination in EVERY mint transaction it serves, for every
    /// unrelated project — so a consumer that walks a frontier must record it
    /// and refuse to expand it.
    MintingProvider,
    /// An NFT marketplace's escrow. Assets and offers sit at its script
    /// addresses while listed, so it appears as a HOLDER of everything on sale
    /// — including every ADA Handle currently listed. A consumer that asks a
    /// handle service "who lives at this stake key" gets thousands of handles
    /// that belong to unrelated sellers, slowly. Name it and never ask.
    Marketplace,
    /// A token venue — DEX, aggregator, swap desk. Same holder problem as a
    /// marketplace: liquidity sits at its addresses, so it tops any holder
    /// list. Only ever registered by a credential that is provably the
    /// venue's; DEX staking scripts are frequently shared, which is why most
    /// of them cannot be listed here at all.
    Exchange,
}

/// A service run from an ordinary wallet, identified by its stake credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StakeService {
    pub label: &'static str,
    pub kind: StakeServiceKind,
    /// How the attribution was established. Registry entries are evidence, so
    /// this is required rather than optional — see the module header.
    pub source: &'static str,
}

/// Services keyed by STAKE credential rather than by payment address.
///
/// ## Why this table exists separately
///
/// [`ADDRESS_REGISTRY`] keys on payment addresses, which is correct for
/// scripts: a contract IS its address. Two things are not reachable that way.
///
/// A service run from an ORDINARY WALLET spends from many payment addresses
/// under a single staking credential, and enumerating them is both endless and
/// pointless. The stake key is the stable identity.
///
/// A SCRIPT is reachable by address — but consumers routinely hold only the
/// stake key, because deriving one from an address is a local decode while
/// keeping the address is not always possible. Anything that groups holders by
/// stake key (a holder snapshot, a handle batch) has thrown the payment
/// address away by the time it wants a name.
///
/// ## The trap this does NOT fall into
///
/// [`ADDRESS_PREFIX_REGISTRY`] carries a warning: order and listing contracts
/// keep the CUSTOMER's staking credential, so naming a stake key after a
/// script-prefix hit labels every customer as the venue. That is exactly the
/// failure this table could reintroduce, so the bar for an entry is:
///
/// **Only add a stake credential that genuinely belongs to the service.** For a
/// wallet-run service that means its own staking key. For a script it means the
/// credential is FIXED across the venue's own addresses rather than carried in
/// from whoever built the transaction — check that the same credential appears
/// in two or more of the venue's registered addresses, and that it is not the
/// seller's. Never derive an entry from a prefix-matched address.
pub static STAKE_REGISTRY: Map<&'static str, StakeService> = phf_map! {
    // Anvil — Cardano minting API. Takes a flat per-mint fee (1.15 ADA at time
    // of writing) in the mint transaction itself, alongside the project's own
    // payment. Observed across four unrelated collections.
    "stake1uy50zl7a9k9c74v66c0gn833at5sh83qnjldk8hg4rrv05g3mmskr" => StakeService {
        label: "Anvil",
        kind: StakeServiceKind::MintingProvider,
        source: "observed 2026-08-22: constant 1.15 ADA mint-tx fee across policies \
                 55bd0ac4 (KAT Pack, 333), 6b42eca9 (chadano_citizen, 1501), \
                 e26a8565 (perps_into_the_factions, 242), 812197d5 (Biddy_DeGoat, 127); \
                 1,009 unspent ~1 ADA UTxOs from unrelated projects",
    },

    // JPG.store — SCRIPT stake credential 2c967f4b…833e6d, shared by the V1
    // offer escrow (addr1xxgx3far…) and the V2 sale escrow (addr1x8rjw3paw…).
    // Fixed across both, so it is the venue's own credential and not a
    // seller's — see the bar for entry above.
    "stake17ykfvl6t62y5fvryvtsnch3lt406dcpls4n4d9pcekpnumg6v83tq" => StakeService {
        label: "JPG.store",
        kind: StakeServiceKind::Marketplace,
        source: "delegation part of the registered JPG.store escrow addresses \
                 addr1xxgx3far… (offer, V1) and addr1x8rjw3paw… (sale, V2); \
                 both are addr1x (script payment + script stake) and carry the \
                 identical script credential 2c967f4bd28944b06462e13c5e3f5d5f\
                 a6e03f8567569438cd833e6d",
    },
    // JPG.store — KEY stake credential 81728e7e…cb806d, the delegation part of
    // the V1 sale escrow (addr1zxgx3far…). A different credential from the one
    // above and reached from one registered address only, so it is listed
    // explicitly rather than inferred.
    "stake1uxqh9rn76n8nynsnyvf4ulndjv0srcc8jtvumut3989cqmgjt49h6" => StakeService {
        label: "JPG.store",
        kind: StakeServiceKind::Marketplace,
        source: "delegation part of the registered JPG.store V1 sale escrow \
                 addr1zxgx3far… (script payment + key stake); credential \
                 81728e7ed4cf324e1323135e7e6d931f01e30792d9cdf17129cb806d",
    },
    // JPG.store's MINTER, a different contract from the escrows above and so a
    // different credential. One payment script, no other label reaches it.
    "stake17yvew80mg6nl5cgama29a6zd40t5z32vfqmkd80gxgkhrqgy48g8s" => StakeService {
        label: "JPG.store",
        kind: StakeServiceKind::Marketplace,
        source: "delegation part of the registered JPG.store minter address; \
                 reached by exactly one payment script and no other registered \
                 label",
    },
    // Wayup — reached by two of its own sale scripts and nothing else.
    //
    // Covers only the Wayup addresses carrying THIS credential. Wayup also has
    // a sale address delegating to the shared credential noted in the
    // exclusions below, which is deliberately not registered; that address
    // stays unnamed rather than being named wrongly.
    "stake1uxwcw3rr36r8q9e5egy3e74cmazpjvx4t5ajsq50xhm568celda4g" => StakeService {
        label: "Wayup",
        kind: StakeServiceKind::Marketplace,
        source: "delegation shared by two registered Wayup sale scripts and \
                 carrying no other registered label",
    },
    // SaturnSwap — three of its own scripts, sole label.
    "stake1u902fq2jxqctywjf22rv5xsch52p5jf7nddpnkyfj5lkekcnnhvtv" => StakeService {
        label: "SaturnSwap",
        kind: StakeServiceKind::Exchange,
        source: "delegation shared by three registered SaturnSwap scripts and \
                 carrying no other registered label",
    },

    // ── Deliberately NOT registered ─────────────────────────────────────────
    //
    // Two credentials look like obvious additions and are not, because more
    // than one entity's contracts delegate to them. `a_registered_credential_
    // is_claimed_by_exactly_one_entity` enforces this; the note is here so the
    // absence reads as a decision rather than an oversight.
    //
    //   stake17xe0d2lkpnx7jt4wrgh5lhm97t40vgydsukx7rje0nqskpc5zugc3
    //     Fourteen distinct payment scripts delegate here, registered under
    //     BOTH "Splash" and "DexHunter". This is not a transcription slip —
    //     it is Spectrum/Splash's LBSP ("Liquidity Bootstrapping Stake Pool")
    //     credential, the protocol's documented mechanism for delegating
    //     contract-locked ADA to its own stake pool. Every validator in the
    //     family shares it BY DESIGN, and the credential's controlled stake
    //     (~16.4M ADA) is delegated to Spectrum Finance's pool.
    //
    //     So the credential identifies a STAKING ARRANGEMENT, not an operator,
    //     and no label can be correct: the addresses behind it span pools, a
    //     launchpad-shaped contract holding a thousand tokens at full supply,
    //     and whatever else the protocol deploys next. Naming it would repeat
    //     the 168 Mekka counterparties wrongly labelled "Splash", noted on
    //     ADDRESS_PREFIX_REGISTRY.
    //
    //     Any DEX whose contracts delegate to a protocol-wide staking script
    //     is unregisterable for the same reason; expect this to be the rule
    //     for DEXes rather than the exception.
    //
    //   stake1u8653j3mcjks5d304g6eexryse7ma22erw8p05fsvefem8qklu7w7
    //     Three scripts, registered under both "The Vault" staking and "Wayup"
    //     sale. Same problem, smaller blast radius.
};

/// Look up a service by its stake credential (bech32 `stake1…`).
pub fn lookup_stake(stake: &str) -> Option<&'static StakeService> {
    STAKE_REGISTRY.get(stake)
}

// ── Lookup functions ─────────────────────────────────────────────────────────

/// Look up an address in the mainnet registry (default, backward-compatible).
pub fn lookup_address(address: &str) -> Option<&'static AddressCategory> {
    lookup_address_for_network(address, RegistryNetwork::Mainnet)
}

/// Look up an address in the registry for the specified network.
pub fn lookup_address_for_network(
    address: &str,
    network: RegistryNetwork,
) -> Option<&'static AddressCategory> {
    let (registry, prefixes) = match network {
        RegistryNetwork::Mainnet => (&ADDRESS_REGISTRY, ADDRESS_PREFIX_REGISTRY),
        RegistryNetwork::Testnet => (&TESTNET_ADDRESS_REGISTRY, TESTNET_ADDRESS_PREFIX_REGISTRY),
    };

    // Fast exact match first
    if let Some(cat) = registry.get(address) {
        return Some(cat);
    }

    // Prefix-based fallback for per-seller script addresses
    for (prefix, category) in prefixes {
        if address.starts_with(prefix) {
            return Some(category);
        }
    }

    None
}

/// How a [`lookup_address_match`] hit was found — and therefore how much of
/// the address the registry actually identified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// A full-address entry: the whole address, staking credential included,
    /// belongs to the registered entity.
    Exact,
    /// A payment-credential prefix from [`ADDRESS_PREFIX_REGISTRY`]: only the
    /// SCRIPT is identified. The staking credential riding on the address may
    /// be the venue's (pool contracts) or an ordinary customer's (order and
    /// listing contracts keep the user's delegation) — the registry cannot
    /// tell you which, so an entity keyed by that stake credential must NOT
    /// inherit the venue's name from this hit alone.
    VariableStakePrefix,
}

/// [`lookup_address_for_network`], but reporting whether the hit identified
/// the full address or only its payment-credential script.
pub fn lookup_address_match(
    address: &str,
    network: RegistryNetwork,
) -> Option<(&'static AddressCategory, MatchKind)> {
    let (registry, prefixes) = match network {
        RegistryNetwork::Mainnet => (&ADDRESS_REGISTRY, ADDRESS_PREFIX_REGISTRY),
        RegistryNetwork::Testnet => (&TESTNET_ADDRESS_REGISTRY, TESTNET_ADDRESS_PREFIX_REGISTRY),
    };
    if let Some(cat) = registry.get(address) {
        return Some((cat, MatchKind::Exact));
    }
    prefixes
        .iter()
        .find(|(prefix, _)| address.starts_with(prefix))
        .map(|(_, category)| (category, MatchKind::VariableStakePrefix))
}

/// Whether a bech32 Shelley address pays to a SCRIPT rather than a key.
///
/// Purely textual: the first data character after the `addr1`/`addr_test1`
/// separator encodes the CIP-19 header's address type (first five bits are
/// the four type bits plus the network nibble's high bit, and the bech32
/// charset maps that to `type * 2`). Script-payment types 1/3/5/7 land on
/// `z`/`x`/`2`/`w`; key-payment types 0/2/4/6 land on `q`/`y`/`g`/`v`.
/// Byron/stake addresses and malformed strings return `false`.
pub fn payment_credential_is_script(address: &str) -> bool {
    let data = address
        .strip_prefix("addr1")
        .or_else(|| address.strip_prefix("addr_test1"));
    matches!(
        data.and_then(|d| d.chars().next()),
        Some('z' | 'x' | '2' | 'w')
    )
}

/// Registry of known script addresses (smart contracts) and their purposes
/// This registry should be manually curated for accuracy
pub static SCRIPT_REGISTRY: Map<&'static str, ContractInfo> = phf_map! {
    "d3b3a8d77b6dfb28c76e1ab11c0b569bfe531fbf6f08d72d89c931aff4aea85f" => ContractInfo {
        category: ScriptCategory::Marketplace {
            marketplace: Marketplace::JpgStore,
            kind: MarketplaceType::JpgStoreV1,
            purpose: MarketplacePurpose::Sale,
            fee_calculation: jpg_store_fee_calculation,
        }
    }
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Minter {
    #[default]
    Unknown,
    JpgStore,
}

impl fmt::Display for Minter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Minter::Unknown => write!(f, "Unknown"),
            Minter::JpgStore => write!(f, "JPG.store"),
        }
    }
}

/// Marketplace platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Marketplace {
    #[default]
    Unknown,
    JpgStore,
    Wayup,
}

impl fmt::Display for Marketplace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Marketplace::Unknown => write!(f, "Unknown"),
            Marketplace::JpgStore => write!(f, "JPG.store"),
            Marketplace::Wayup => write!(f, "Wayup"),
        }
    }
}

impl Marketplace {
    pub fn from_address(address: &str) -> Option<Self> {
        match lookup_address(address) {
            Some(AddressCategory::Marketplace(marketplace)) => Some(*marketplace),
            Some(AddressCategory::Script(ScriptCategory::Marketplace { marketplace, .. })) => {
                Some(*marketplace)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MarketplacePurpose {
    #[default]
    Unknown,
    Offer,
    Sale,
    Fee,
}

impl fmt::Display for MarketplacePurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketplacePurpose::Unknown => write!(f, "Unknown"),
            MarketplacePurpose::Offer => write!(f, "Offer"),
            MarketplacePurpose::Sale => write!(f, "Sale"),
            MarketplacePurpose::Fee => write!(f, "Fee"),
        }
    }
}

impl MarketplacePurpose {
    pub fn from_address(address: &str) -> Option<Self> {
        match lookup_address(address) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace { purpose, .. })) => {
                Some(*purpose)
            }
            _ => None,
        }
    }
}

/// Information about a known regular address (wallet, exchange, etc.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressInfo {
    /// Human-readable description of the address
    pub description: String,
    /// Type/category of the address
    pub category: AddressCategory,
}

/// Categories of regular addresses
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AddressCategory {
    #[default]
    Unknown,
    Marketplace(Marketplace),
    Script(ScriptCategory),
}

impl fmt::Display for AddressCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressCategory::Unknown => write!(f, "Unknown"),
            AddressCategory::Marketplace(marketplace) => write!(f, "{marketplace}"),
            AddressCategory::Script(script_category) => write!(f, "{script_category}"),
        }
    }
}

/// Information about a known smart contract
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractInfo {
    pub category: ScriptCategory,
}

#[derive(Debug, Clone, Default)]
pub enum ScriptCategory {
    #[default]
    Unknown,
    Marketplace {
        marketplace: Marketplace,
        kind: MarketplaceType,
        purpose: MarketplacePurpose,
        fee_calculation: FeeCalculationFn,
    },
    Exchange {
        label: &'static str,
    },
    /// A launchpad's bonding curve — where a token trades BEFORE it graduates
    /// to an AMM, and where its unsold supply sits until somebody buys it.
    ///
    /// Deliberately NOT `Exchange`. A curve's price is not constant-product:
    /// fitted against real pools the implied reserve runs ~1,250 ADA near the
    /// start and ~3,884 at the cap, so pricing one as a pool is roughly 3×
    /// wrong and silently so. A consumer that treats every `Exchange` as an
    /// AMM must not be handed one of these.
    ///
    /// Its supply is not float either: nobody has ever owned it.
    Launchpad {
        label: &'static str,
    },
    /// An address a token can reach and never leave.
    ///
    /// `evidence` is REQUIRED rather than a label, because an address that
    /// removes supply from circulation is exactly where an unexamined
    /// assumption gets expensive — "provably unspendable" and "believed
    /// unspendable" have to be distinguishable at the point of registration.
    Burn {
        evidence: &'static str,
    },
    DeFi {
        label: &'static str,
        protocol: &'static str,
    },
    Minter(Minter),
    Staking {
        label: &'static str,
        project: &'static str,
    },
    Vesting {
        label: &'static str,
    },
}

impl PartialEq for ScriptCategory {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ScriptCategory::Unknown, ScriptCategory::Unknown) => true,
            (
                ScriptCategory::Marketplace {
                    marketplace: m1,
                    kind: k1,
                    purpose: p1,
                    fee_calculation: _,
                },
                ScriptCategory::Marketplace {
                    marketplace: m2,
                    kind: k2,
                    purpose: p2,
                    fee_calculation: _,
                },
            ) => m1 == m2 && k1 == k2 && p1 == p2, // Exclude fee_calculation from comparison
            (ScriptCategory::Exchange { label: l1 }, ScriptCategory::Exchange { label: l2 }) => {
                l1 == l2
            }
            (ScriptCategory::Launchpad { label: l1 }, ScriptCategory::Launchpad { label: l2 }) => {
                l1 == l2
            }
            // Two burn addresses are the same category, not the same address —
            // the evidence describes WHY each is unspendable and differs by
            // construction, so comparing it would make every sink unequal.
            (ScriptCategory::Burn { .. }, ScriptCategory::Burn { .. }) => true,
            (
                ScriptCategory::DeFi {
                    label: l1,
                    protocol: p1,
                },
                ScriptCategory::DeFi {
                    label: l2,
                    protocol: p2,
                },
            ) => l1 == l2 && p1 == p2,
            (ScriptCategory::Minter(m1), ScriptCategory::Minter(m2)) => m1 == m2,
            (
                ScriptCategory::Staking {
                    label: l1,
                    project: p1,
                },
                ScriptCategory::Staking {
                    label: l2,
                    project: p2,
                },
            ) => l1 == l2 && p1 == p2,
            (ScriptCategory::Vesting { label: l1 }, ScriptCategory::Vesting { label: l2 }) => {
                l1 == l2
            }
            _ => false,
        }
    }
}

impl Eq for ScriptCategory {}

impl fmt::Display for ScriptCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptCategory::Unknown => write!(f, "Unknown"),
            ScriptCategory::Marketplace {
                marketplace,
                purpose,
                ..
            } => {
                write!(f, "{marketplace} {purpose}")
            }
            ScriptCategory::Exchange { label } => {
                write!(f, "{label} exchange")
            }
            ScriptCategory::DeFi { label, .. } => write!(f, "{label} DeFi"),
            ScriptCategory::Minter(minter) => write!(f, "{minter} Minter"),
            ScriptCategory::Staking { label, project } => {
                write!(f, "{label} staking for {project}")
            }
            ScriptCategory::Vesting { label } => {
                write!(f, "{label} vesting")
            }
            // "bonding curve", never "exchange" — the word is what stops a
            // reader treating its reserves as an AMM's.
            ScriptCategory::Launchpad { label } => {
                write!(f, "{label} bonding curve")
            }
            // The evidence is deliberately NOT rendered: it runs to a
            // paragraph and belongs in a report, not a label.
            ScriptCategory::Burn { .. } => write!(f, "burn address"),
        }
    }
}

// ── AddressLookup trait ──────────────────────────────────────────────────────

/// Trait for address registry implementations.
///
/// Consumers accept `Box<dyn AddressLookup>` instead of a concrete registry type.
/// This enables composing multiple registries (e.g. ecosystem + app-specific).
pub trait AddressLookup: Send + Sync {
    /// Look up an address category (exact + prefix match).
    fn lookup(&self, address: &str) -> Option<&AddressCategory>;

    /// Look up contract info by script hash.
    fn get_contract_info(&self, script_hash: &str) -> Option<&ContractInfo>;

    // ── Default convenience methods ──────────────────────────────────────

    /// Look up address category (alias for `lookup`).
    fn get_address_category(&self, address: &str) -> Option<&AddressCategory> {
        self.lookup(address)
    }

    /// Get marketplace info from an address.
    fn get_marketplace_info(
        &self,
        address: &str,
    ) -> Option<(Marketplace, Option<MarketplacePurpose>)> {
        match self.lookup(address) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace {
                marketplace,
                purpose,
                ..
            })) => Some((*marketplace, Some(*purpose))),
            Some(AddressCategory::Marketplace(marketplace)) => Some((*marketplace, None)),
            _ => None,
        }
    }

    /// Check if an address is a known script address.
    fn is_known_script(&self, address: &str) -> bool {
        self.get_contract_info(address).is_some()
    }

    /// Check if an address is a known regular address.
    fn is_known_address(&self, address: &str) -> bool {
        self.lookup(address).is_some()
    }

    /// Check if address belongs to a specific marketplace.
    fn is_marketplace_address(&self, address: &str, marketplace: &Marketplace) -> bool {
        match self.get_marketplace_info(address) {
            Some((addr_marketplace, _)) => addr_marketplace == *marketplace,
            None => false,
        }
    }

    /// Check if address is ANY marketplace address.
    fn is_any_marketplace_address(&self, address: &str) -> bool {
        matches!(
            self.lookup(address),
            Some(AddressCategory::Script(ScriptCategory::Marketplace { .. }))
                | Some(AddressCategory::Marketplace(_))
        )
    }

    /// Check if the address belongs to a known vesting contract (Shield or CrowdLock).
    fn is_any_vesting_address(&self, address: &str) -> bool {
        matches!(
            self.lookup(address),
            Some(AddressCategory::Script(ScriptCategory::Vesting { .. }))
        )
    }

    /// Get the fee calculation function for a marketplace address.
    fn get_marketplace_fee_calculation(&self, address: &str) -> Option<FeeCalculationFn> {
        match self.lookup(address) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace {
                fee_calculation, ..
            })) => Some(*fee_calculation),
            _ => None,
        }
    }

    /// Calculate marketplace fee for a given address and base price.
    fn calculate_marketplace_fee(&self, address: &str, base_price_lovelace: u64) -> u64 {
        match self.get_marketplace_fee_calculation(address) {
            Some(fee_calc) => fee_calc(base_price_lovelace, address),
            None => 0,
        }
    }

    /// Get all known marketplaces involved in a transaction.
    fn get_transaction_marketplaces(
        &self,
        input_addresses: &[String],
        output_addresses: &[String],
    ) -> std::collections::HashSet<Marketplace> {
        let mut marketplaces = std::collections::HashSet::new();

        for address in input_addresses {
            if let Some((marketplace, _)) = self.get_marketplace_info(address) {
                marketplaces.insert(marketplace);
            }
        }

        for address in output_addresses {
            if let Some((marketplace, _)) = self.get_marketplace_info(address) {
                marketplaces.insert(marketplace);
            }
        }

        marketplaces
    }
}

// ── SmartContractRegistry (ecosystem addresses) ─────────────────────────────

/// Ecosystem address registry for identifying known contract addresses and their purposes.
///
/// Contains well-known ecosystem addresses (marketplaces, DEXes, etc.) from
/// the compile-time `ADDRESS_REGISTRY` and `SCRIPT_REGISTRY` maps.
/// App-specific addresses should be provided via a separate `AddressLookup`
/// implementation and composed using a composite registry.
#[derive(Debug, Clone)]
pub struct SmartContractRegistry {
    /// Which network's address registry to consult
    network: RegistryNetwork,
    /// Runtime additions for development/testing (not used in production lookups)
    runtime_contracts: std::collections::HashMap<String, ContractInfo>,
}

impl SmartContractRegistry {
    /// Create a new registry (defaults to Mainnet)
    pub fn new() -> Self {
        Self {
            network: RegistryNetwork::Mainnet,
            runtime_contracts: std::collections::HashMap::new(),
        }
    }

    /// Create a new registry for a specific network
    pub fn new_for_network(network: RegistryNetwork) -> Self {
        Self {
            network,
            runtime_contracts: std::collections::HashMap::new(),
        }
    }

    /// Add a contract to runtime registry (for development/testing only).
    /// Production contracts should be added to the SCRIPT_REGISTRY compile-time map.
    pub fn register_contract(&mut self, address: String, info: ContractInfo) {
        self.runtime_contracts.insert(address, info);
    }
}

impl AddressLookup for SmartContractRegistry {
    fn lookup(&self, address: &str) -> Option<&AddressCategory> {
        lookup_address_for_network(address, self.network)
    }

    fn get_contract_info(&self, address: &str) -> Option<&ContractInfo> {
        // First check compile-time registry (production contracts)
        if let Some(info) = SCRIPT_REGISTRY.get(address) {
            return Some(info);
        }

        // Fall back to runtime additions (development/testing)
        self.runtime_contracts.get(address)
    }

    fn is_known_script(&self, address: &str) -> bool {
        SCRIPT_REGISTRY.contains_key(address) || self.runtime_contracts.contains_key(address)
    }
}

impl Default for SmartContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns all known exact addresses and prefix patterns from the mainnet registry.
///
/// Used by tooling (e.g., block-tap-mcp) to expand `address_category` conditions
/// into concrete address lists at bake time.
pub fn all_known_addresses() -> Vec<&'static str> {
    let mut addrs: Vec<&'static str> = ADDRESS_REGISTRY.keys().copied().collect();
    for (prefix, _) in ADDRESS_PREFIX_REGISTRY {
        addrs.push(prefix);
    }
    addrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anvil_is_found_by_stake_credential() {
        let s = lookup_stake("stake1uy50zl7a9k9c74v66c0gn833at5sh83qnjldk8hg4rrv05g3mmskr")
            .expect("Anvil is registered");
        assert_eq!(s.label, "Anvil");
        assert_eq!(s.kind, StakeServiceKind::MintingProvider);
        assert!(!s.source.is_empty(), "an entry must carry its evidence");
    }

    #[test]
    fn an_unregistered_stake_key_is_not_named() {
        assert!(
            lookup_stake("stake1u98f5mr0mn8tv2kqndk5cwen4uasc7cewlzdklz6y664zacl9lvjz").is_none()
        );
    }

    /// Both jpg.store escrow credentials resolve to the same venue. A consumer
    /// that only has a stake key — a holder snapshot, a handle batch — must be
    /// able to name the marketplace without the payment address.
    #[test]
    fn jpg_store_escrow_is_found_by_either_stake_credential() {
        for stake in [
            "stake17ykfvl6t62y5fvryvtsnch3lt406dcpls4n4d9pcekpnumg6v83tq",
            "stake1uxqh9rn76n8nynsnyvf4ulndjv0srcc8jtvumut3989cqmgjt49h6",
        ] {
            let s = lookup_stake(stake).unwrap_or_else(|| panic!("{stake} is registered"));
            assert_eq!(s.label, "JPG.store");
            assert_eq!(s.kind, StakeServiceKind::Marketplace);
            assert!(!s.source.is_empty(), "an entry must carry its evidence");
        }
    }

    /// The script credential is shared by the V1 offer and V2 sale escrows.
    /// Those two addresses are the evidence the entry rests on, so if either
    /// leaves [`ADDRESS_REGISTRY`] the stake entry has lost its justification.
    #[test]
    fn the_shared_jpg_store_credential_still_has_two_witnesses() {
        for addr in [
            "addr1xxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tfvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eks2utwdd",
            "addr1x8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7efvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8ekstg4qrx",
        ] {
            assert!(
                matches!(
                    lookup_address(addr),
                    Some(AddressCategory::Script(ScriptCategory::Marketplace {
                        marketplace: Marketplace::JpgStore,
                        ..
                    }))
                ),
                "{addr} is a witness for the shared jpg.store stake credential"
            );
        }
    }

    /// Every credential in `PAYMENT_CREDENTIAL_REGISTRY` really is the
    /// payment part of the address beside it.
    ///
    /// This is the whole reason the table is allowed to exist. Twenty-eight
    /// bytes of hex is unreadable, so a typo or a stale copy cannot be caught
    /// by review — and the consequence is silent: the credential simply never
    /// matches, a marketplace stops being recognised, and the first anyone
    /// hears of it is a chart that looks slightly wrong months later. Here it
    /// is a decode away from being caught on every run.
    #[test]
    fn every_credential_matches_its_address() {
        use pallas_addresses::Address;

        for (cred, entry) in PAYMENT_CREDENTIAL_REGISTRY {
            assert_eq!(
                cred.len(),
                56,
                "{cred} is not a 28-byte credential — {} hex chars",
                cred.len()
            );
            assert!(
                cred.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                "{cred} must be lower-case hex; lookups are exact"
            );
            let CredentialSource::Address(addr) = entry.derived_from else {
                continue;
            };
            match Address::from_bech32(addr) {
                Ok(Address::Shelley(sh)) => assert_eq!(
                    sh.payment().to_hex(),
                    *cred,
                    "{addr} decodes to a different payment credential"
                ),
                other => panic!("{addr} must be a Shelley address, got {other:?}"),
            }
        }
    }

    /// A credential nobody registered answers NO.
    ///
    /// Load-bearing for consumers that use this to decide custody: a DEX
    /// pool, a vesting lock and a bridge all genuinely take an asset, and
    /// answering "yes, a marketplace" for an unknown script would freeze
    /// assets at wallets that really did part with them.
    #[test]
    fn an_unregistered_credential_is_not_guessed_at() {
        assert!(lookup_payment_credential("00".repeat(28).as_str()).is_none());
        assert!(lookup_payment_credential("").is_none());
        // The Wayup FEE credential — a real Wayup contract, and deliberately
        // not in the table: a fee address takes the money and keeps it.
        assert!(lookup_payment_credential(
            "5f08a64f580e581735070e1b1d2ce29ae6942ab45ccff5a1747d2283"
        )
        .is_none());
    }

    /// The V2 escrow is registered twice: once delegated to JPG.store's own
    /// stake credential, once undelegated. Same payment script, so the same
    /// validator and the same datum — which is why both carry `JpgStoreV2`.
    /// Splitting them across versions would route byte-identical datums to two
    /// different schemas, and the undelegated one is the address that actually
    /// shows up in recorded sales.
    #[test]
    fn both_forms_of_the_v2_escrow_are_the_same_contract() {
        use pallas_addresses::Address;

        const DELEGATED: &str = "addr1x8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7efvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8ekstg4qrx";
        const UNDELEGATED: &str = "addr1w8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7eg0fcr8k";

        let script_of = |a: &str| match Address::from_bech32(a) {
            Ok(Address::Shelley(sh)) => sh.payment().to_hex(),
            _ => panic!("{a} must decode"),
        };
        assert_eq!(
            script_of(DELEGATED),
            script_of(UNDELEGATED),
            "the two forms must share a payment script, or they are not one contract"
        );

        let kind_of = |a: &str| match lookup_address(a) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace { kind, .. })) => *kind,
            other => panic!("{a} should be a registered marketplace, got {other:?}"),
        };
        assert_eq!(kind_of(DELEGATED), MarketplaceType::JpgStoreV2);
        assert_eq!(
            kind_of(UNDELEGATED),
            kind_of(DELEGATED),
            "one script, one datum schema"
        );
    }

    /// Every registered address must decode AND round-trip back to the exact
    /// string it is keyed by.
    ///
    /// Round-tripping is the part that matters. A corrupt or hand-edited bech32
    /// still "decodes" into *something* under a lenient reader, so the only way
    /// to know the header agrees with the payload is to re-encode and compare.
    /// An address that fails this matches nothing on chain and makes every
    /// lookup against it silently return no result rather than erroring.
    #[test]
    fn every_registered_address_is_well_formed() {
        use pallas_addresses::Address;

        let mut bad = Vec::new();
        for (address, category) in ADDRESS_REGISTRY.entries() {
            match Address::from_bech32(address) {
                Ok(decoded) => {
                    let reencoded = decoded.to_bech32().unwrap_or_default();
                    if reencoded != *address {
                        bad.push(format!(
                            "{address} does not round-trip (re-encodes to {reencoded}) — {category}"
                        ));
                    }
                }
                Err(e) => bad.push(format!("{address} does not decode: {e} — {category}")),
            }
        }
        assert!(
            bad.is_empty(),
            "malformed registry addresses:\n  {}",
            bad.join("\n  ")
        );
    }

    /// One contract version must mean one validator. If two SALE addresses
    /// carry the same `MarketplaceType` but different *script* credentials, the
    /// version no longer identifies a datum schema or a reference script, and
    /// anything selecting behaviour by version is picking arbitrarily.
    ///
    /// Restricted to script-credential addresses on purpose. `ScriptCategory::
    /// Marketplace` is also used to attribute addresses that are not contracts
    /// at all — Wayup settles through an ordinary key address
    /// (`addr1v87m5srr…`), and jpg's fee destination carries a version tag
    /// despite being a payout target. Those are attribution facts, not contract
    /// claims, and demanding a validator of them is a category error.
    /// `marketplace_key_addresses_are_not_contracts` pins that distinction so
    /// it stays deliberate rather than looking overlooked.
    #[test]
    fn one_marketplace_version_means_one_validator() {
        use pallas_addresses::{Address, ShelleyPaymentPart};
        use std::collections::BTreeMap;

        let mut script_of_version: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (address, category) in ADDRESS_REGISTRY.entries() {
            let AddressCategory::Script(ScriptCategory::Marketplace { kind, purpose, .. }) =
                category
            else {
                continue;
            };
            if !matches!(purpose, Purpose::Sale) {
                continue;
            }
            let Ok(Address::Shelley(shelley)) = Address::from_bech32(address) else {
                continue;
            };
            // Only a script credential can be spent by a validator.
            let ShelleyPaymentPart::Script(script_hash) = shelley.payment() else {
                continue;
            };
            let script = script_hash.to_string();
            match script_of_version.get(&format!("{kind:?}")) {
                Some((seen_script, seen_address)) => assert_eq!(
                    *seen_script, script,
                    "{kind:?} maps to two different validators: {seen_address} uses \
                     {seen_script}, {address} uses {script}"
                ),
                None => {
                    script_of_version.insert(format!("{kind:?}"), (script, (*address).to_string()));
                }
            }
        }
        assert!(
            !script_of_version.is_empty(),
            "no sale addresses registered"
        );
    }

    /// Some addresses filed under `ScriptCategory::Marketplace` are not
    /// contracts: a venue's own settlement wallet, or a fee destination. They
    /// belong in the registry — attribution is the point — but nothing may
    /// treat them as spendable script UTxOs.
    ///
    /// This test names them so the conflation is a recorded decision rather
    /// than something the next reader has to rediscover the hard way. If the
    /// category model is ever split, this is the list to move.
    #[test]
    fn marketplace_key_addresses_are_not_contracts() {
        use pallas_addresses::{Address, ShelleyPaymentPart};

        // Wayup settles the overwhelming majority of its volume through this
        // ordinary wallet rather than through its sale validator.
        const WAYUP_SETTLEMENT_WALLET: &str =
            "addr1v87m5srrtx52s8jdragjl8wle0eq57dzv2n62nxh3nx65dq0edwwu";

        let Ok(Address::Shelley(shelley)) = Address::from_bech32(WAYUP_SETTLEMENT_WALLET) else {
            panic!("{WAYUP_SETTLEMENT_WALLET} must decode");
        };
        assert!(
            matches!(shelley.payment(), ShelleyPaymentPart::Key(_)),
            "the Wayup settlement address is expected to be a KEY address; if it is now a \
             script, it has become a contract and the version tables need revisiting"
        );
        assert!(
            MarketplaceType::Wayup.script_reference().is_none(),
            "a Wayup reference script has been registered — check it against the sale \
             validator, not the settlement wallet"
        );
    }

    /// jpg V1 and V2/V3 must NOT share a buy redeemer.
    ///
    /// They did, and it was wrong for whichever generation lost the coin toss.
    /// V1 buys on constructor 1, V2/V3 on constructor 0 — verified on chain by
    /// checking which spends satisfy their listing datum's payouts.
    #[test]
    fn jpg_generations_use_opposite_buy_redeemers() {
        let v1 = MarketplaceType::JpgStoreV1.buy_redeemer().unwrap();
        let v2 = MarketplaceType::JpgStoreV2.buy_redeemer().unwrap();
        let v3 = MarketplaceType::JpgStoreV3.buy_redeemer().unwrap();

        // V1: bare constructor 1.
        assert_eq!(
            v1.encode_hex(0),
            "d87a80",
            "V1 buys on a bare constructor 1"
        );
        assert!(!v1.carries_payout_index);

        // V2/V3: constructor 0 carrying the payout start index. Byte-for-byte
        // what real V2 buys put on chain (`013f02f2…`, `556db775…`).
        assert_eq!(
            v2.encode_hex(0),
            "d8799f00ff",
            "V2 buys on constructor 0 [0]"
        );
        assert_eq!(v2.encode_hex(3), "d8799f03ff", "the index is the field");
        assert!(v2.carries_payout_index);
        assert_eq!(
            v2.constructor, v3.constructor,
            "V2 and V3 are one validator"
        );

        assert_ne!(
            v1.encode_hex(0),
            v2.encode_hex(0),
            "jpg reversed its convention between generations; sharing one redeemer \
             sends every buy of one generation down the delist branch"
        );
    }

    /// The index field must encode as CBOR the validator can read past 23,
    /// where unsigned ints stop fitting in the initial byte.
    #[test]
    fn payout_index_encodes_across_cbor_width_boundaries() {
        let r = MarketplaceType::JpgStoreV2.buy_redeemer().unwrap();
        assert_eq!(r.encode_hex(23), "d8799f17ff");
        assert_eq!(r.encode_hex(24), "d8799f1818ff");
        assert_eq!(r.encode_hex(256), "d8799f190100ff");
    }

    /// A reference input can only satisfy a spend if it carries the *same*
    /// script the UTxO's address is locked by. So for every registered
    /// marketplace address, the `script_reference()` of its `kind` must hash to
    /// that address's own payment credential.
    ///
    /// This is the invariant that was silently violated: the table shipped the
    /// V1 validator (`9068a7a3…`) under `JpgStoreV2`, so V1 listings resolved
    /// to no reference at all and V2 listings resolved to a validator that
    /// isn't theirs. Both versions were unbuyable and nothing said so.
    #[test]
    fn script_reference_matches_address() {
        use pallas_addresses::Address;

        let mut checked = 0;
        for (address, category) in ADDRESS_REGISTRY.entries() {
            let AddressCategory::Script(ScriptCategory::Marketplace { kind, purpose, .. }) =
                category
            else {
                continue;
            };
            // Only SALE addresses are spent by a buy. A fee address is a payout
            // destination that happens to be tagged with a version, and an
            // offer address is spent by the collection-offer builder against a
            // different validator — neither is this reference's business.
            if !matches!(purpose, Purpose::Sale) {
                continue;
            }
            let Some(script_ref) = kind.script_reference() else {
                continue;
            };
            let payment_cred = match Address::from_bech32(address) {
                Ok(Address::Shelley(sh)) => sh.payment().to_hex(),
                // A few registry rows are non-Shelley or have a corrupt
                // payload; those are the address table's problem, not this
                // invariant's.
                _ => continue,
            };
            assert_eq!(
                payment_cred, script_ref.script_hash,
                "{kind:?} reference script {} does not match the payment credential of {address} \
                 — a buy against this address would reference the wrong validator",
                script_ref.script_hash
            );
            checked += 1;
        }
        assert!(
            checked >= 3,
            "expected to check several marketplace addresses, only checked {checked} — \
             has the registry or the reference table been gutted?"
        );
    }

    /// A DEX ORDER contract glues the CUSTOMER's stake onto one payment
    /// script, so it has as many addresses as it has had traders — 499 for
    /// Splash and 333 for Minswap on $PERP alone. A consumer holding a raw
    /// credential (as `policy-archive`'s movement graph does) can only find
    /// them here.
    #[test]
    fn dex_contracts_resolve_by_credential_not_only_by_address() {
        for (cred, want) in [
            (
                "cb684a69e78907a9796b21fc150a758af5f2805e5ed5d5a8ce9f76f1",
                "Splash",
            ),
            (
                "9dee0659686c3ab807895c929e3284c11222affd710b09be690f924d",
                "Splash",
            ),
            (
                "ea07b733d932129c378af627436e7cbc2ef0bf96e0036bb51b3bde6b",
                "Minswap",
            ),
            (
                "ed97e0a1394724bb7cb94f20acf627abc253694c92b88bf8fb4b7f6f",
                "CSWAP",
            ),
            (
                "da5b47aed3955c9132ee087796fa3b58a1ba6173fa31a7bc29e56d4e",
                "CSWAP",
            ),
        ] {
            let entry = lookup_payment_credential(cred)
                .unwrap_or_else(|| panic!("{cred} must resolve by credential"));
            match &entry.category {
                AC::Script(SC::Exchange { label }) => assert_eq!(*label, want, "{cred}"),
                other => panic!("{cred} should be an Exchange, got {other:?}"),
            }
        }
    }

    /// A bonding curve is NOT an exchange, and the distinction is load-bearing:
    /// its price is not constant-product, so a consumer that prices every
    /// `Exchange` as an AMM must not be handed one.
    #[test]
    fn the_snek_fun_curve_is_a_launchpad_not_an_exchange() {
        const CRED: &str = "905ab869961b094f1b8197278cfe15b45cbe49fa8f32c6b014f85a2d";
        const ADDR: &str = "addr1xxg94wrfjcdsjncmsxtj0r87zk69e0jfl28n934sznu95tdj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrs2993lw";

        // Reachable both ways — the address table and the credential table
        // must not disagree about one contract.
        for category in [
            &lookup_payment_credential(CRED)
                .expect("curve resolves by credential")
                .category,
            lookup_address(ADDR).expect("curve resolves by address"),
        ] {
            match category {
                AC::Script(SC::Launchpad { label }) => assert_eq!(*label, "snek.fun"),
                other => panic!("expected a Launchpad, got {other:?}"),
            }
        }
        // It was registered as DexHunter until 2026-09-08, from a stake
        // credential Splash's contracts share. If this ever reads Exchange
        // again, that inference has crept back.
        assert_ne!(
            lookup_address(ADDR),
            Some(&AC::Script(SC::Exchange { label: "DexHunter" })),
        );
        assert_eq!(
            AC::Script(SC::Launchpad { label: "snek.fun" }).to_string(),
            "snek.fun bonding curve",
        );
    }

    /// A sink must carry EVIDENCE, not a label. An address that removes supply
    /// from circulation is where an unexamined assumption gets expensive.
    #[test]
    fn a_burn_sink_carries_its_evidence() {
        const CRED: &str = "c1b35bb893529376effc4083dc0a0ed90a1c07fe09550885b37aa27f";
        let entry = lookup_payment_credential(CRED).expect("the sink resolves");
        match &entry.category {
            AC::Script(SC::Burn { evidence }) => {
                assert!(evidence.contains("always-fails"), "evidence must say WHY");
                assert!(
                    evidence.contains("CAVEAT"),
                    "an inference must be labelled as one, not presented as proof"
                );
            }
            other => panic!("expected a Burn, got {other:?}"),
        }
        // Attributed to nobody — that is the claim, and it is what keeps a
        // stake credential from being named after it.
        assert_eq!(owner_of(&entry.category), None);
    }

    /// The ENTITY a category attributes an address to, ignoring the role it
    /// plays. "JPG.store Offer" and "JPG.store Sale" are one entity; "Splash"
    /// and "DexHunter" are two. Stake ownership is a claim about the entity,
    /// so this is the granularity the invariant below has to compare at.
    fn owner_of(category: &AddressCategory) -> Option<String> {
        Some(match category {
            AddressCategory::Unknown => return None,
            AddressCategory::Marketplace(m) => m.to_string(),
            AddressCategory::Script(s) => match s {
                ScriptCategory::Unknown => return None,
                ScriptCategory::Marketplace { marketplace, .. } => marketplace.to_string(),
                ScriptCategory::Exchange { label } => label.to_string(),
                ScriptCategory::DeFi { protocol, .. } => protocol.to_string(),
                ScriptCategory::Minter(m) => m.to_string(),
                ScriptCategory::Staking { label, .. } => label.to_string(),
                ScriptCategory::Vesting { label } => label.to_string(),
                ScriptCategory::Launchpad { label } => label.to_string(),
                // A burn sink is attributed to NOBODY — that is the whole
                // claim. Returning an owner would let a stake credential be
                // named after it, and an always-fails script has no operator
                // to name.
                ScriptCategory::Burn { .. } => return None,
            },
        })
    }

    /// The invariant that makes [`STAKE_REGISTRY`] safe to add to.
    ///
    /// A staking credential may only be named after an entity if every
    /// registered address delegating to it belongs to that entity. DEX and
    /// aggregator contracts routinely SHARE a staking script — fourteen
    /// distinct payment scripts across Splash and DexHunter delegate to one
    /// credential — and an order or listing contract carries the CUSTOMER's
    /// delegation. Either way, naming such a credential labels somebody else's
    /// wallet as the venue, which is the failure mode this whole table is
    /// fenced against.
    ///
    /// Checking it here means the judgement is enforced rather than
    /// remembered: add a credential shared by two entities and this fails with
    /// both names, which is exactly the prompt needed.
    #[test]
    fn a_registered_credential_is_claimed_by_exactly_one_entity() {
        use std::collections::{BTreeMap, BTreeSet};

        let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (address, category) in ADDRESS_REGISTRY.entries() {
            let (Some(stake), Some(owner)) = (stake_of(address), owner_of(category)) else {
                continue;
            };
            if STAKE_REGISTRY.contains_key(stake.as_str()) {
                owners.entry(stake).or_default().insert(owner);
            }
        }

        for (stake, claimants) in owners {
            let registered = lookup_stake(&stake).expect("filtered on membership").label;
            assert_eq!(
                claimants.len(),
                1,
                "{stake} is registered as {registered:?} but registered addresses \
                 for {claimants:?} delegate to it — a credential claimed by more \
                 than one entity cannot name any of them"
            );
            let claimant = claimants.iter().next().expect("exactly one");
            assert_eq!(
                claimant, registered,
                "{stake} is registered as {registered:?} but its addresses belong \
                 to {claimant:?}"
            );
        }
    }

    /// Decode a registered address's delegation part, or `None` if it has one
    /// of the forms that carries no stake credential.
    fn stake_of(address: &str) -> Option<String> {
        use pallas_addresses::{Address, StakeAddress};
        let Ok(Address::Shelley(sh)) = Address::from_bech32(address) else {
            return None;
        };
        StakeAddress::try_from(sh).ok()?.to_bech32().ok()
    }

    /// Nothing in this crate parses the addresses it stores — lookups are
    /// string comparisons, and `payment_credential_is_script` reads one
    /// character. So a typo produces a row that is silently unreachable rather
    /// than a build error, and the table quietly stops covering what it claims
    /// to. That is not hypothetical: a "JPG.store V3 sale" entry sat here for
    /// a long time holding the V2 address with `x` changed to `w`, which fails
    /// the bech32 checksum and could never have matched anything.
    #[test]
    fn every_registered_address_is_a_real_address() {
        use pallas_addresses::Address;
        for address in ADDRESS_REGISTRY
            .keys()
            .chain(TESTNET_ADDRESS_REGISTRY.keys())
        {
            assert!(
                Address::from_bech32(address).is_ok(),
                "{address} is registered but is not a decodable address — \
                 an exact-match table can never hit it"
            );
        }
    }

    /// The two tables have to agree about who owns a staking credential.
    ///
    /// Every fully-registered JPG.store address delegates to a credential that
    /// [`STAKE_REGISTRY`] must also name JPG.store. This is what makes the
    /// stake entries self-maintaining: add a venue address carrying a
    /// credential nobody registered and this fails, which is the prompt to
    /// decide whether the credential is really the venue's — the one judgement
    /// the stake table's doc comment insists on.
    #[test]
    fn a_registered_venue_address_delegates_to_a_registered_credential() {
        let mut checked = 0;
        for (address, category) in ADDRESS_REGISTRY.entries() {
            if !matches!(
                category,
                AddressCategory::Script(ScriptCategory::Marketplace {
                    marketplace: Marketplace::JpgStore,
                    ..
                })
            ) {
                continue;
            }
            // Enterprise addresses (V4) delegate to nothing — no claim to check.
            let Some(stake) = stake_of(address) else {
                continue;
            };
            let named = lookup_stake(&stake).map(|s| s.label);
            assert_eq!(
                named,
                Some("JPG.store"),
                "{address} is a registered JPG.store address delegating to \
                 {stake}, which the stake table does not name as JPG.store"
            );
            checked += 1;
        }
        assert!(
            checked >= 3,
            "expected several JPG.store addresses to carry a stake credential, \
             checked only {checked} — has the registry been gutted?"
        );
    }

    /// The stake table takes bech32 STAKE keys. A payment address must never
    /// hit it, or a consumer could name a wallet after a service it merely
    /// paid.
    #[test]
    fn a_payment_address_never_hits_the_stake_table() {
        assert!(
            lookup_stake(
                "addr1qx68zqqmcfhy2jvj7ugffvrx0utykjeq9k2vfzy5lyn3defg79la6tvt3a2e44s73x0rr6hfpw0zp897mv0w32xxclgsle8hll"
            )
            .is_none(),
            "Anvil's own payment address must be looked up by its STAKE key, not directly"
        );
    }

    /// A Splash ORDER address carries the customer's staking credential — the
    /// registry identifies the script, and must SAY it identified only the
    /// script, or the customer's stake key gets named after the DEX. This is
    /// not hypothetical: 168 Mekka counterparties were labelled "Splash".
    #[test]
    fn a_prefix_hit_declares_it_only_identified_the_script() {
        let customer_order =
            "addr1z9ryamhgnuz6lau86sqytte2gz5rlktv2yce05e0h3207qdhc9k425ezp5cw8a3ssg7swp6fjdmnp3y8vcuka3fjr7mgqw6ke7p";
        let (cat, kind) = lookup_address_match(customer_order, RegistryNetwork::Mainnet).unwrap();
        assert!(matches!(
            cat,
            AddressCategory::Script(ScriptCategory::Exchange { label: "Splash" })
        ));
        assert_eq!(kind, MatchKind::VariableStakePrefix);
    }

    #[test]
    fn a_full_address_entry_is_an_exact_match() {
        let minswap_batcher = "addr1w8p79rpkcdz8x9d6tft0x0dx5mwuzac2sa4gm8cvkw5hcnqst2ctf";
        let (_, kind) = lookup_address_match(minswap_batcher, RegistryNetwork::Mainnet).unwrap();
        assert_eq!(kind, MatchKind::Exact);
    }

    #[test]
    fn script_payment_detection_reads_the_type_character() {
        // script payment: types 1 (z), 3 (x), 7 (w)
        assert!(payment_credential_is_script("addr1z9ryamhgnuz6lau86sq"));
        assert!(payment_credential_is_script("addr1xxgx3far7qygq0k6epa"));
        assert!(payment_credential_is_script("addr1w8p79rpkcdz8x9d6tft"));
        // key payment: types 0 (q), 6 (v)
        assert!(!payment_credential_is_script("addr1qy9mg28evkzcfghlrg8"));
        assert!(!payment_credential_is_script("addr1v877gsrkj9t2j64yc06"));
        // not a payment address at all
        assert!(!payment_credential_is_script("stake1uxmaqke42j9q6v83lv"));
        assert!(!payment_credential_is_script(""));
    }
}
