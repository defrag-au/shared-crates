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

/// Errors from transaction building operations.
#[derive(Debug, thiserror::Error)]
pub enum TxBuildError {
    #[error("Invalid hex: {0}")]
    InvalidHex(String),
    #[error("Insufficient funds: need {needed} lovelace, have {available}")]
    InsufficientFunds { needed: u64, available: u64 },
    #[error("No suitable UTxO found")]
    NoSuitableUtxo,
    #[error("Asset not found in wallet UTxOs: {0}")]
    AssetNotFound(String),
    #[error("Policy ID mismatch: expected {expected}, got {actual}")]
    PolicyMismatch { expected: String, actual: String },
    #[error("Transaction build failed: {0}")]
    BuildFailed(String),
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
