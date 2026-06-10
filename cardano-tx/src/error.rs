//! Transaction error types
//!
//! - [`SubmissionResult`] — platform-agnostic detection of permanent vs retriable submission failures
//! - [`TxBuildError`] — errors from transaction building operations

/// Known Cardano ledger validation error patterns that indicate a permanent rejection.
/// These come from the Cardano node's ledger rules and will never succeed on retry.
const PERMANENT_PATTERNS: &[&str] = &[
    "TxValidationError",
    "ValueNotConservedUTxO",
    "OutputTooSmallUTxO",
    "ExtraneousScriptWitnesses",
    "InsufficientCollateral",
    "BadInputsUTxO",
    "FeeTooSmallUTxO",
    "ScriptWitnessNotValidatingUTXOW",
    "MissingRequiredSigners",
    "MissingScriptWitnessesUTXOW",
];

/// Result of submitting a transaction to the network.
///
/// Consumers construct this from their HTTP layer — the status code and response body
/// are the two meaningful signals for classifying submission outcomes.
#[derive(Debug, Clone)]
pub struct SubmissionResult {
    pub status_code: u16,
    pub message: String,
}

impl SubmissionResult {
    /// Returns `true` if this is a permanent ledger rejection that should not be retried.
    ///
    /// HTTP 400 from any Cardano submit API indicates the transaction is fundamentally
    /// invalid (bad inputs, insufficient fee, script failure, etc.). Known ledger
    /// validation error patterns in the message body are also treated as permanent.
    ///
    /// Network errors, timeouts, and 5xx responses are *not* permanent — those should
    /// be retried.
    pub fn is_permanent_rejection(&self) -> bool {
        if self.status_code == 400 {
            return true;
        }

        PERMANENT_PATTERNS
            .iter()
            .any(|pattern| self.message.contains(pattern))
    }
}

/// Extract the `(tx_hash, output_index)` inputs the ledger rejected as
/// `BadInputsUTxO` — inputs the chain says aren't in the UTxO set (already spent
/// / never existed). A consumer that keeps a local UTxO ledger uses these to mark
/// the offending rows `spent`, so it stops re-selecting a stale UTxO (the
/// self-heal for an over-counting ledger). Lenient parse of the node's
/// CBOR-diagnostic error text — `… SafeHash "<64-hex>"} ) (TxIx {unTxIx = <n>} …`
/// — tolerant of JSON-escaped quotes. Empty when the error isn't a `BadInputsUTxO`.
pub fn extract_bad_input_refs(error: &str) -> Vec<(String, u32)> {
    if !error.contains("BadInputsUTxO") {
        return Vec::new();
    }
    let mut refs = Vec::new();
    for (i, _) in error.match_indices("SafeHash") {
        let after = &error[i + "SafeHash".len()..];
        // First run of 64 hex chars after `SafeHash` (skips the ` "` / ` \"`).
        let tx_hash: String = after
            .chars()
            .skip_while(|c| !c.is_ascii_hexdigit())
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if tx_hash.len() != 64 {
            continue;
        }
        // The matching `unTxIx = <n>` for THIS input (the first one after it).
        if let Some(tix) = after.find("unTxIx") {
            let digits: String = after[tix..]
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(ix) = digits.parse::<u32>() {
                refs.push((tx_hash, ix));
            }
        }
    }
    refs
}

/// Errors from transaction building operations.
#[derive(Debug, thiserror::Error)]
pub enum TxBuildError {
    #[error("Invalid hex: {0}")]
    InvalidHex(String),
    #[error("Insufficient funds: need {needed} lovelace, have {available}")]
    InsufficientFunds { needed: u64, available: u64 },
    #[error("No suitable UTxO found")]
    NoSuitableUtxo,
    /// Wallet has no UTxOs at all to select from. Distinct from
    /// `InsufficientFunds` (which means the wallet has UTxOs, just not enough)
    /// so callers can surface a clearer message ("connect a funded wallet").
    #[error("Wallet has no UTxOs to select from")]
    NoUtxoCandidates,
    /// No single UTxO covers the requested amount. Carries `largest` and
    /// `total` separately so callers can distinguish "you have plenty of ADA
    /// but it's fragmented across small UTxOs" (consolidate) from "your
    /// wallet is genuinely thin" (top up).
    #[error(
        "No single UTxO covers {needed} lovelace — largest is {largest}, total wallet balance is {total}"
    )]
    NoSingleUtxoLargeEnough {
        needed: u64,
        largest: u64,
        total: u64,
    },
    /// `UtxoSelectionConfig` required pure-ADA but no pure-ADA UTxO was large
    /// enough. Useful for surfacing "you need a separate collateral UTxO" in
    /// strict-mode call sites.
    #[error(
        "No pure-ADA UTxO covers {needed} lovelace — largest pure-ADA is {largest_pure_ada}, total pure-ADA is {total_pure_ada}"
    )]
    NoPureAdaUtxoLargeEnough {
        needed: u64,
        largest_pure_ada: u64,
        total_pure_ada: u64,
    },
    #[error("Asset not found in wallet UTxOs: {0}")]
    AssetNotFound(String),
    #[error("Policy ID mismatch: expected {expected}, got {actual}")]
    PolicyMismatch { expected: String, actual: String },
    #[error("Transaction build failed: {0}")]
    BuildFailed(String),
    #[error("CBOR parse error: {0}")]
    CborParse(String),
    #[error("Signing failed: {0}")]
    SignFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_400_is_permanent() {
        let result = SubmissionResult {
            status_code: 400,
            message: "whatever".to_string(),
        };
        assert!(result.is_permanent_rejection());
    }

    #[test]
    fn test_500_is_not_permanent() {
        let result = SubmissionResult {
            status_code: 500,
            message: "internal server error".to_string(),
        };
        assert!(!result.is_permanent_rejection());
    }

    #[test]
    fn test_ledger_validation_error_pattern() {
        let result = SubmissionResult {
            status_code: 202,
            message: "TxValidationError: ValueNotConservedUTxO".to_string(),
        };
        assert!(result.is_permanent_rejection());
    }

    #[test]
    fn test_bad_inputs_pattern() {
        let result = SubmissionResult {
            status_code: 202,
            message: "BadInputsUTxO: input not found".to_string(),
        };
        assert!(result.is_permanent_rejection());
    }

    #[test]
    fn test_extract_bad_input_refs() {
        // The real shape from a Maestro submit rejection (JSON-escaped quotes).
        let err = r#"["ConwayUtxowFailure (UtxoFailure (BadInputsUTxO (NonEmptySet (fromList [TxIn (TxId {unTxId = SafeHash \"385a530b03a9f05000e493053775c283634781279bd5763aa331f086b221a373\"}) (TxIx {unTxIx = 0})]))))"]"#;
        let refs = extract_bad_input_refs(err);
        assert_eq!(
            refs,
            vec![(
                "385a530b03a9f05000e493053775c283634781279bd5763aa331f086b221a373".to_string(),
                0
            )]
        );
        // Two inputs, plain quotes, different indices.
        let (h1, h2) = ("a".repeat(64), "b".repeat(64));
        let two = format!(
            "BadInputsUTxO fromList [TxIn (TxId {{unTxId = SafeHash \"{h1}\"}}) (TxIx {{unTxIx = 2}}), \
             TxIn (TxId {{unTxId = SafeHash \"{h2}\"}}) (TxIx {{unTxIx = 7}})]"
        );
        let refs2 = extract_bad_input_refs(&two);
        assert_eq!(refs2, vec![(h1, 2), (h2, 7)]);
        // Not a BadInputs error → empty.
        assert!(extract_bad_input_refs("FeeTooSmallUTxO ...").is_empty());
    }

    #[test]
    fn test_fee_too_small_pattern() {
        let result = SubmissionResult {
            status_code: 202,
            message: "FeeTooSmallUTxO: minimum fee is 200000".to_string(),
        };
        assert!(result.is_permanent_rejection());
    }

    #[test]
    fn test_network_error_not_permanent() {
        let result = SubmissionResult {
            status_code: 0,
            message: "connection timeout".to_string(),
        };
        assert!(!result.is_permanent_rejection());
    }

    #[test]
    fn test_all_permanent_patterns_detected() {
        for pattern in PERMANENT_PATTERNS {
            let result = SubmissionResult {
                status_code: 200,
                message: format!("error: {pattern}"),
            };
            assert!(
                result.is_permanent_rejection(),
                "Pattern '{pattern}' should be detected as permanent"
            );
        }
    }
}
