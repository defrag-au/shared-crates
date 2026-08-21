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
pub fn decimals(unit: &str) -> Option<u32> {
    match unit {
        "lovelace" => Some(6),
        // USDM — Moneta's fiat-backed stablecoin, 6 dp. Policy verified
        // against the Mekka S2 ledger's own flows (`0014df10` + "USDM").
        "c48cbb3d5e57ed56e276bc45f99ab39abe94e6cd7ac39fb402da47ad.0014df105553444d" => Some(6),
        _ => None,
    }
}

/// Format a raw on-chain quantity for display, WITHOUT a ticker.
///
/// Known decimals give `59.01`; unknown give a grouped integer `1,000,000,000`
/// so the reader can see it is an unscaled count. Negative quantities keep
/// their sign; callers that want a magnitude pass one.
pub fn format_quantity(unit: &str, qty: i128) -> String {
    match decimals(unit) {
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
