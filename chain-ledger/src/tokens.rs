//! Token decimals — the few we KNOW, and honest formatting for the rest.
//!
//! ## Why this is a curated list and not a rule
//!
//! A native token's decimal places are **not on chain**. The ledger holds an
//! integer; how to render it lives in an off-chain registry the walk never
//! reads. So there is no rule to apply — only knowledge, and applying a
//! guessed one misstates a payment by orders of magnitude in the direction
//! nobody notices (a 59 USDM transfer read as 59,007,753 USDM, which is what
//! prompted this module).
//!
//! Hence: a table keyed by the FULL unit (`policy.asset_name_hex`), each
//! entry sourced. Keyed by full unit rather than by ticker on purpose —
//! anyone can mint a token called `USDM`, and matching on the name would let
//! them borrow a real stablecoin's scale.
//!
//! ## What happens to everything else
//!
//! Unknown tokens are **not scaled and are formatted differently**:
//! `1,000,000,000 WLK` (grouped integer) rather than `1000000000.00 WLK`.
//! The shape of the number is the disclosure — a grouped integer reads as a
//! raw count, a decimal reads as a quantity somebody has interpreted. Adding
//! a token here is a small, sourced edit; guessing is the thing to avoid.

/// Decimal places for units we can vouch for. `None` = unknown, render raw.
///
/// To add: verify the decimals against the token's own documentation or the
/// Cardano token registry, and cite it in the comment.
///
/// Every entry below was read from the token's own **Cardano token registry**
/// record via Koios `asset_info.token_registry_metadata` on 2026-08-23, not
/// inferred from its ticker. Two of them would have been wrong if guessed —
/// see `SNEK` and `USDC`.
pub fn decimals(unit: &str) -> Option<u32> {
    match unit {
        "lovelace" => Some(6),

        // ── stablecoins ────────────────────────────────────────────────────
        // USDM — Moneta, registry ticker "USDM", url moneta.global.
        "c48cbb3d5e57ed56e276bc45f99ab39abe94e6cd7ac39fb402da47ad.0014df105553444d" => Some(6),
        // iUSD — Indigo synthetic USD, registry ticker "iUSD".
        "f66d78b4a3cb3d37afa0ec36461e51ecbde00f26c8f0a68f94b69880.69555344" => Some(6),
        // USDA — Anzens, registry ticker "USDA".
        "fe7c786ab321f41c654ef6c1af7b3250a613c24e4213e0425a7ae456.55534441" => Some(6),
        // DJED — asset name is "DjedMicroUSD", registry ticker "DJED",
        // name "Djed USD". The on-chain name says MICRO and the registry says
        // 6 dp; both agree, so the micro-denomination IS the 6 dp.
        "8db269c3ec630e06ae29f74bc39edd1f87c819f1056206e879a1cd61.446a65644d6963726f555344" => {
            Some(6)
        }
        // USDCx — registry ticker "USDCx".
        "1f3aec8bfe7ea4fe14c5f121e2a92e301afe414147860d557cac7e34.5553444378" => Some(6),

        // ⚠️ USDC — **EIGHT** decimals, not six.
        //
        // This is the WANCHAIN-BRIDGED USDC: its registry record carries a
        // BLANK ticker and a `wanscan.org` url, and states 8 dp. Ethereum's
        // native USDC is 6, so the obvious assumption is wrong by 100× — in
        // the direction that makes a holding look larger than it is. The
        // blank ticker also means it cannot be matched by name.
        "25c5de5f5b286073c593edfd77b48abc7a48e5a4f3d4cd9d428ff935.55534443" => Some(8),

        // ── non-stables that move enough to be worth reading ───────────────
        // ⚠️ SNEK — **ZERO** decimals (registry ticker "Snek"). A meme token
        // with no fractional part; scaling it by 6 would divide every holding
        // by a million. Relevant because snek.fun launches trade in it.
        "279c909f348e533da5808898f87f9a14bb2c3dfbbacccd631d927a3f.534e454b" => Some(0),
        // NIGHT — registry ticker "NIGHT".
        "0691b2fecca1ac4f53cb6dfb00b7013e561d1f34403b957cbb5af1fa.4e49474854" => Some(6),
        // MIN — Minswap, registry ticker "MIN".
        "29d222ce763455e3d7a09a665ce554f00ac89d2e99a1a83d267170c6.4d494e" => Some(6),
        // IAG — IAGON, registry ticker "IAG".
        "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114.494147" => Some(6),

        _ => None,
    }
}

/// Whether a unit is **money** — something a payment can be settled in.
///
/// ## Why this is a list and not a label test
///
/// The obvious shortcut is to ask whether the asset carries the CIP-67
/// fungible-token label (`0014df10`, class 333). Do not: that label says
/// "this is fungible", not "this is money". Any project can mint a fungible
/// token, so the test admits every meme and utility token ever launched —
/// measured on the Mekka S1 ledger it accepted WLK, ASCEND, ANGELS, ATLAS,
/// SKULLY, TITAN, BETFI, FLDT, PBG, Shards and GENS as settlement, ~17,300
/// legs of it.
///
/// And it fails the other way at the same time. Real stablecoins mostly use
/// PLAIN asset names — iUSD, USDA, DJED, USDC, USDCx carry no CIP-68 label —
/// so the same test rejected 139,157 legs of genuine payment. USDM passed
/// only by the accident of being CIP-68 minted.
///
/// So: an explicit list, on the same terms as [`decimals`]. A unit is money
/// because we can say why, not because of the shape of its name.
///
/// ## What is deliberately NOT here
///
/// Traded assets with known decimals — SNEK, MIN, IAG, NIGHT. Knowing a
/// token's scale is not the same as calling it settlement, and a meme token
/// counted as money is the very failure this replaces. If a case needs SNEK
/// treated as settlement (a snek.fun launch priced in it, say), add it
/// deliberately with the reason.
pub fn is_settlement_unit(unit: &str) -> bool {
    matches!(
        unit,
        "lovelace"
            // USDM — Moneta
            | "c48cbb3d5e57ed56e276bc45f99ab39abe94e6cd7ac39fb402da47ad.0014df105553444d"
            // iUSD — Indigo
            | "f66d78b4a3cb3d37afa0ec36461e51ecbde00f26c8f0a68f94b69880.69555344"
            // USDA — Anzens
            | "fe7c786ab321f41c654ef6c1af7b3250a613c24e4213e0425a7ae456.55534441"
            // DJED — COTI
            | "8db269c3ec630e06ae29f74bc39edd1f87c819f1056206e879a1cd61.446a65644d6963726f555344"
            // USDCx
            | "1f3aec8bfe7ea4fe14c5f121e2a92e301afe414147860d557cac7e34.5553444378"
            // USDC — Wanchain-bridged, 8 dp
            | "25c5de5f5b286073c593edfd77b48abc7a48e5a4f3d4cd9d428ff935.55534443"
    )
}

/// Format a raw on-chain quantity for display, WITHOUT a ticker.
///
/// Known decimals give `59.01`; unknown give a grouped integer `1,000,000,000`
/// so the reader can see it is an unscaled count. Negative quantities keep
/// their sign; callers that want a magnitude pass one.
pub fn format_quantity(unit: &str, qty: i128) -> String {
    match decimals(unit) {
        // A token we KNOW has no fractional part — SNEK is the live case.
        //
        // Grouped, like an unknown token, and that is not a loss of
        // information: the disclosure the grouped form carries is "this is a
        // raw count, nobody has scaled it", and at 0 dp the raw count IS the
        // quantity. Appending `.00` would be the actual lie — it asserts two
        // decimal places of precision the token does not have.
        Some(0) => group_digits(qty),
        Some(d) => {
            let scale = 10f64.powi(d as i32);
            format!("{:.2}", qty as f64 / scale)
        }
        None => group_digits(qty),
    }
}

/// `1234567` → `1,234,567`.
fn group_digits(v: i128) -> String {
    let neg = v < 0;
    let digits = v.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if neg {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const USDM: &str = "c48cbb3d5e57ed56e276bc45f99ab39abe94e6cd7ac39fb402da47ad.0014df105553444d";

    /// The bug this module exists for: a 59 USDM payment displayed as
    /// 59,007,753 USDM because the raw integer went straight to the screen.
    #[test]
    fn a_known_token_is_scaled_and_an_unknown_one_is_visibly_raw() {
        assert_eq!(format_quantity(USDM, 59_007_753), "59.01");
        assert_eq!(format_quantity("lovelace", 1_370_580), "1.37");

        // Unknown decimals: grouped integer, never a decimal point — the
        // format itself says "this is a raw count".
        let wlk = "017af5d958fffdf65f3e5b8b3ff5abefd210a03464a9fc48ea0f4a39.0014df10574c4b";
        assert_eq!(decimals(wlk), None);
        assert_eq!(format_quantity(wlk, 1_000_000_000), "1,000,000,000");
        assert!(!format_quantity(wlk, 1_000_000_000).contains('.'));
    }

    /// The two entries that would be WRONG if anyone reasoned from the
    /// ticker instead of reading the registry. Both fail in the direction
    /// nobody notices, so they are pinned.
    #[test]
    fn the_two_traps_are_pinned() {
        // SNEK has ZERO decimals. Assuming the usual 6 divides every holding
        // by a million — a 1,000,000 SNEK position would read as "1.00".
        let snek = "279c909f348e533da5808898f87f9a14bb2c3dfbbacccd631d927a3f.534e454b";
        assert_eq!(decimals(snek), Some(0));
        assert_eq!(
            format_quantity(snek, 1_000_000),
            "1,000,000",
            "0 dp renders as the count it is — never scaled, and never given \
             a fractional part the token does not have"
        );
        assert!(!format_quantity(snek, 1_000_000).contains('.'));

        // Cardano's bridged USDC is EIGHT dp, not Ethereum's six. At 6 the
        // same integer reads 100× too large.
        let usdc = "25c5de5f5b286073c593edfd77b48abc7a48e5a4f3d4cd9d428ff935.55534443";
        assert_eq!(decimals(usdc), Some(8));
        assert_eq!(format_quantity(usdc, 100_000_000), "1.00");
        assert_ne!(
            format_quantity(usdc, 100_000_000),
            "100.00",
            "six decimals would say 100 USDC where the truth is 1"
        );
    }

    /// Every stablecoin we claim to know renders a whole unit as "1.00".
    /// A single wrong entry here is a misstated payment, so they are checked
    /// as a set rather than trusted individually.
    #[test]
    fn every_curated_stablecoin_agrees_on_what_one_unit_looks_like() {
        for (unit, dp) in [
            (USDM, 6u32),
            (
                "f66d78b4a3cb3d37afa0ec36461e51ecbde00f26c8f0a68f94b69880.69555344",
                6,
            ),
            (
                "fe7c786ab321f41c654ef6c1af7b3250a613c24e4213e0425a7ae456.55534441",
                6,
            ),
            (
                "8db269c3ec630e06ae29f74bc39edd1f87c819f1056206e879a1cd61.446a65644d6963726f555344",
                6,
            ),
            (
                "1f3aec8bfe7ea4fe14c5f121e2a92e301afe414147860d557cac7e34.5553444378",
                6,
            ),
            (
                "25c5de5f5b286073c593edfd77b48abc7a48e5a4f3d4cd9d428ff935.55534443",
                8,
            ),
        ] {
            assert_eq!(decimals(unit), Some(dp), "decimals for {unit}");
            let one = 10i128.pow(dp);
            assert_eq!(format_quantity(unit, one), "1.00", "one unit of {unit}");
        }
    }

    /// The CIP-68 label test this replaced, in both its failure directions.
    #[test]
    fn settlement_is_a_list_not_a_label() {
        // FALSE POSITIVE the label test allowed: a project token minted with
        // the fungible label 333. Fungible is not money.
        let skully =
            "861d38630fb4541234567890123456789012345678901234567890ab.0014df10534b554c4c59";
        assert!(
            skully.contains("0014df10"),
            "this is exactly the shape the old test accepted"
        );
        assert!(
            !is_settlement_unit(skully),
            "a meme token is not settlement"
        );

        // FALSE NEGATIVE the label test caused: real stablecoins with plain
        // asset names, 139,157 legs of them on one ledger.
        for stable in [
            "f66d78b4a3cb3d37afa0ec36461e51ecbde00f26c8f0a68f94b69880.69555344", // iUSD
            "fe7c786ab321f41c654ef6c1af7b3250a613c24e4213e0425a7ae456.55534441", // USDA
            "8db269c3ec630e06ae29f74bc39edd1f87c819f1056206e879a1cd61.446a65644d6963726f555344",
            "1f3aec8bfe7ea4fe14c5f121e2a92e301afe414147860d557cac7e34.5553444378", // USDCx
            "25c5de5f5b286073c593edfd77b48abc7a48e5a4f3d4cd9d428ff935.55534443",   // USDC
        ] {
            assert!(!stable.contains("0014df10"), "no CIP-68 label to match on");
            assert!(is_settlement_unit(stable), "but it IS money: {stable}");
        }

        assert!(is_settlement_unit("lovelace"));
        assert!(is_settlement_unit(USDM));
    }

    /// Knowing a token's SCALE is not the same as calling it MONEY.
    #[test]
    fn known_decimals_do_not_make_a_token_settlement() {
        for traded in [
            "279c909f348e533da5808898f87f9a14bb2c3dfbbacccd631d927a3f.534e454b", // SNEK
            "29d222ce763455e3d7a09a665ce554f00ac89d2e99a1a83d267170c6.4d494e",   // MIN
            "5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114.494147",   // IAG
        ] {
            assert!(decimals(traded).is_some(), "we know its scale");
            assert!(!is_settlement_unit(traded), "but it is not money: {traded}");
        }
    }

    /// Keyed by FULL unit: a token that merely calls itself USDM under
    /// another policy must not inherit the real one's scale.
    #[test]
    fn an_impostor_does_not_borrow_a_real_tokens_scale() {
        let fake = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef.0014df105553444d";
        assert_eq!(decimals(fake), None);
        assert_eq!(format_quantity(fake, 59_007_753), "59,007,753");
    }

    #[test]
    fn grouping_handles_signs_and_short_numbers() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(-1_234), "-1,234");
        assert_eq!(group_digits(999), "999");
    }
}
