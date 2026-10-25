//! Launchpad order contracts.
//!
//! A launchpad buy is not a swap. There is no pool to name in the transaction,
//! no redeemer and no script input to balance: the wallet makes ONE plain
//! payment to the cohort's shared order address carrying an order datum, and a
//! settler spends that UTxO into the curve a block or two later. So these
//! modules build an OUTPUT, and they never decode a pool — the curve state a
//! quote needs comes from the index that already reads it.
//!
//! [`crate::dex`] is the wrong home for them: every venue in `dex` is a pool
//! with a redeemer. A cohort is an escrow with a datum.

pub mod lumpfun;

/// The order as a transaction output: an ADA payment at the cohort's shared
/// address, with the order datum inline.
///
/// Every launchpad cohort observed so far is this shape — a plain payment, no
/// token leg — so the helper lives beside them rather than in each cohort.
pub fn order_output(
    address: pallas_addresses::Address,
    lovelace: u64,
    datum: &[u8],
) -> pallas_txbuilder::Output {
    crate::helpers::output::create_ada_output(address, lovelace).set_inline_datum(datum.to_vec())
}
