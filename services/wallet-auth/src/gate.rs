//! Access gate trait — application-specific authorization after CIP-8 verification.

/// Outcome of a successful access gate evaluation.
pub struct GateOutcome {
    /// Application-assigned tier (e.g. "basic", "pro", "ultra").
    pub tier: String,
    /// Whether this wallet has admin privileges.
    pub is_admin: bool,
    /// How long the JWT should be valid, in seconds.
    pub jwt_ttl_secs: u64,
    /// The quantity the gate measured, in WHOLE units of whatever it counted
    /// — tokens, NFTs, locked ADA. `None` when the gate is not a threshold on
    /// an amount (an allowlist, a pass/fail check) or did not need to measure.
    ///
    /// Carried so the app can show a holder *why* they landed on the tier they
    /// did. A tier alone is an assertion; the number behind it is checkable,
    /// and a gate that only reports its verdict makes every "why am I on
    /// basic?" a support question.
    pub holding: Option<u64>,
    /// Everything the gate measured, keyed by the source it measured — the
    /// ladder's own condition keys.
    ///
    /// [`Self::holding`] is the single number behind the VERDICT; this is the
    /// full picture, and the two answer different questions. An app showing
    /// per-route progress needs this: reusing the verdict figure for every
    /// route credits a holder of one token with holding all of them.
    pub holdings: std::collections::BTreeMap<String, u64>,
}

/// Application-specific access gate.
///
/// Called after the CIP-8 signature is verified and the challenge is consumed.
/// Implementations can check on-chain state (CrowdLock, NFT ownership, token balance),
/// off-chain databases, or simply allow all verified wallets.
///
/// # Example
///
/// ```rust,ignore
/// struct AllowAll;
///
/// impl AccessGate for AllowAll {
///     async fn evaluate(&self, _stake: &str, _pkh: &str) -> Result<GateOutcome, String> {
///         Ok(GateOutcome {
///             tier: "free".into(),
///             is_admin: false,
///             jwt_ttl_secs: 86400,
///             // Nothing was measured, so there is nothing to report. Empty
///             // reads as "no progress known", not as "holds nothing".
///             holding: None,
///             holdings: Default::default(),
///         })
///     }
/// }
/// ```
pub trait AccessGate {
    /// Evaluate whether a verified wallet should be granted access.
    ///
    /// - `stake_address`: bech32 stake address (stake1...)
    /// - `payment_pkh_hex`: Blake2b-224 of the Ed25519 public key (hex), useful for on-chain lookups
    ///
    /// Returns `GateOutcome` on success, or an error message to deny access.
    fn evaluate(
        &self,
        stake_address: &str,
        payment_pkh_hex: &str,
    ) -> impl std::future::Future<Output = Result<GateOutcome, String>>;
}
