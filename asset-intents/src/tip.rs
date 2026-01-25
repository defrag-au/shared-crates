//! Tip intent for fungible token tips (e.g., FarmBot ctip)

use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Intent to tip fungible tokens via a tipping service (e.g., FarmBot ctip)
///
/// # Example
///
/// ```
/// use asset_intents::TipIntent;
///
/// // Tip 100 ADA
/// let tip = TipIntent::new("ADA", 100.0);
///
/// // Tip 50.5 CARN tokens
/// let tip = TipIntent::new("CARN", 50.5);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TipIntent {
    /// Token identifier (e.g., "ADA", "CARN", "GOLD")
    pub token: String,
    /// Amount to tip
    pub amount: f64,
}

impl TipIntent {
    /// Create a new tip intent
    pub fn new(token: impl Into<String>, amount: f64) -> Self {
        Self {
            token: token.into(),
            amount,
        }
    }

    /// Get a human-readable description (e.g., "100 ADA")
    pub fn description(&self) -> String {
        format!("{} {}", self.amount, self.token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tip_intent_creation() {
        let tip = TipIntent::new("ADA", 100.0);
        assert_eq!(tip.token, "ADA");
        assert_eq!(tip.amount, 100.0);
    }

    #[test]
    fn test_tip_description() {
        let tip = TipIntent::new("CARN", 50.5);
        assert_eq!(tip.description(), "50.5 CARN");
    }
}
