//! Token amount formatting
//!
//! Provides human-readable formatting for large token amounts.

use std::fmt;

/// A token amount with human-readable formatting.
///
/// Formats large numbers with K, M, B, T suffixes for readability.
///
/// # Examples
///
/// ```
/// use asset_intents::TokenAmount;
///
/// assert_eq!(TokenAmount::new(500.0, "ADA").to_string(), "500 ADA");
/// assert_eq!(TokenAmount::new(1500.0, "ADA").to_string(), "1.5K ADA");
/// assert_eq!(TokenAmount::new(100_000_000.0, "BANK").to_string(), "100M BANK");
/// assert_eq!(TokenAmount::new(2_500_000_000.0, "SNEK").to_string(), "2.5B SNEK");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TokenAmount {
    pub amount: f64,
    pub token: String,
}

impl TokenAmount {
    /// Create a new token amount
    pub fn new(amount: f64, token: impl Into<String>) -> Self {
        Self {
            amount,
            token: token.into(),
        }
    }

    /// Format the amount with appropriate suffix (K, M, B, T)
    pub fn format_amount(&self) -> String {
        format_number(self.amount)
    }
}

impl fmt::Display for TokenAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.format_amount(), self.token)
    }
}

/// Format a number with K, M, B, T suffixes for large values.
///
/// - Values < 1,000: shown as-is (with up to 2 decimal places if needed)
/// - Values >= 1,000: use K suffix
/// - Values >= 1,000,000: use M suffix
/// - Values >= 1,000,000,000: use B suffix
/// - Values >= 1,000,000,000,000: use T suffix
///
/// # Examples
///
/// ```
/// use asset_intents::format_number;
///
/// assert_eq!(format_number(500.0), "500");
/// assert_eq!(format_number(1500.0), "1.5K");
/// assert_eq!(format_number(1000.0), "1K");
/// assert_eq!(format_number(1_234_567.0), "1.23M");
/// assert_eq!(format_number(100_000_000.0), "100M");
/// assert_eq!(format_number(2_500_000_000.0), "2.5B");
/// assert_eq!(format_number(1_000_000_000_000.0), "1T");
/// ```
pub fn format_number(n: f64) -> String {
    let abs = n.abs();

    let (divisor, suffix) = if abs >= 1_000_000_000_000.0 {
        (1_000_000_000_000.0, "T")
    } else if abs >= 1_000_000_000.0 {
        (1_000_000_000.0, "B")
    } else if abs >= 1_000_000.0 {
        (1_000_000.0, "M")
    } else if abs >= 1_000.0 {
        (1_000.0, "K")
    } else {
        // No suffix needed
        return format_decimal(n);
    };

    let scaled = n / divisor;
    format!("{}{}", format_decimal(scaled), suffix)
}

/// Format a decimal number, removing unnecessary trailing zeros
fn format_decimal(n: f64) -> String {
    if n == n.floor() {
        // Whole number
        format!("{}", n as i64)
    } else {
        // Up to 2 decimal places, trim trailing zeros
        let formatted = format!("{:.2}", n);
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(42.0), "42");
        assert_eq!(format_number(999.0), "999");
        assert_eq!(format_number(100.5), "100.5");
        assert_eq!(format_number(99.99), "99.99");
    }

    #[test]
    fn test_format_number_thousands() {
        assert_eq!(format_number(1_000.0), "1K");
        assert_eq!(format_number(1_500.0), "1.5K");
        assert_eq!(format_number(10_000.0), "10K");
        assert_eq!(format_number(100_000.0), "100K");
        assert_eq!(format_number(999_000.0), "999K");
        assert_eq!(format_number(1_234.0), "1.23K");
    }

    #[test]
    fn test_format_number_millions() {
        assert_eq!(format_number(1_000_000.0), "1M");
        assert_eq!(format_number(1_500_000.0), "1.5M");
        assert_eq!(format_number(100_000_000.0), "100M");
        assert_eq!(format_number(999_000_000.0), "999M");
    }

    #[test]
    fn test_format_number_billions() {
        assert_eq!(format_number(1_000_000_000.0), "1B");
        assert_eq!(format_number(2_500_000_000.0), "2.5B");
        assert_eq!(format_number(32_000_000_000.0), "32B");
    }

    #[test]
    fn test_format_number_trillions() {
        assert_eq!(format_number(1_000_000_000_000.0), "1T");
        assert_eq!(format_number(5_500_000_000_000.0), "5.5T");
    }

    #[test]
    fn test_token_amount_display() {
        assert_eq!(TokenAmount::new(500.0, "ADA").to_string(), "500 ADA");
        assert_eq!(TokenAmount::new(1_500.0, "ADA").to_string(), "1.5K ADA");
        assert_eq!(
            TokenAmount::new(100_000_000.0, "BANK").to_string(),
            "100M BANK"
        );
    }
}
