/// Marketplace pricing information extracted from datums
#[derive(Debug, Clone)]
pub struct MarketplacePricing {
    /// Base asset price in lovelace (before marketplace fees)
    pub base_price_lovelace: u64,
    /// Marketplace fee in lovelace
    pub marketplace_fee_lovelace: u64,
    /// Total price including all fees
    pub total_price_lovelace: u64,
    /// Number of payment recipients (for analysis)
    pub payout_count: usize,
    /// Expiration timestamp if available
    pub expires_at: Option<u64>,
    /// Method used to extract this pricing
    pub extraction_method: ExtractionMethod,
    /// CDDL schema key used (if CDDL validation was used)
    pub schema_key: Option<String>,
}

/// Method used to extract pricing information
#[derive(Debug, Clone)]
pub enum ExtractionMethod {
    /// Extracted using CDDL validation
    CddlValidated,
    /// Extracted using legacy JSON parsing
    Legacy,
    /// Extracted from transaction metadata
    Metadata,
}

impl std::fmt::Display for ExtractionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CddlValidated => write!(f, "CDDL"),
            Self::Legacy => write!(f, "Legacy"),
            Self::Metadata => write!(f, "Metadata"),
        }
    }
}
