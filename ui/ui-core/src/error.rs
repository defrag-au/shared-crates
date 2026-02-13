//! Error types for widget operations

use thiserror::Error;

/// Errors that can occur in widget operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum WidgetError {
    /// Network/fetch error
    #[error("Network error: {0}")]
    Network(String),

    /// HTTP error with status code
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    /// JSON parsing error
    #[error("Parse error: {0}")]
    Parse(String),

    /// Authentication token expired
    #[error("Token expired")]
    TokenExpired,

    /// Not authenticated
    #[error("Not authenticated")]
    NotAuthenticated,

    /// Other errors
    #[error("{0}")]
    Other(String),
}

impl WidgetError {
    /// Check if this is an authentication-related error (401 or token expired)
    pub fn is_auth_error(&self) -> bool {
        match self {
            WidgetError::TokenExpired | WidgetError::NotAuthenticated => true,
            WidgetError::Http { status, .. } => *status == 401,
            WidgetError::Other(msg) => msg.contains("TOKEN_EXPIRED"),
            _ => false,
        }
    }

    /// Check if this is an HTTP 401 error
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, WidgetError::Http { status: 401, .. })
    }

    /// Check if this is an HTTP 403 error
    pub fn is_forbidden(&self) -> bool {
        matches!(self, WidgetError::Http { status: 403, .. })
    }

    /// Get a user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            WidgetError::TokenExpired | WidgetError::NotAuthenticated => {
                "Your session has expired. Please run the command again from Discord.".to_string()
            }
            WidgetError::Network(msg) => format!("Network error: {msg}"),
            WidgetError::Http { status: 401, .. } => {
                "Your session has expired. Please run the command again from Discord.".to_string()
            }
            WidgetError::Http { status, message } => {
                format!("Server error ({status}): {message}")
            }
            WidgetError::Parse(msg) => format!("Data parsing error: {msg}"),
            WidgetError::Other(msg) => {
                if msg.contains("TOKEN_EXPIRED") {
                    "Your session has expired. Please run the command again from Discord."
                        .to_string()
                } else {
                    msg.clone()
                }
            }
        }
    }

    /// Get technical details (for logging/debugging)
    pub fn technical_details(&self) -> String {
        format!("{self:?}")
    }
}

impl From<String> for WidgetError {
    fn from(s: String) -> Self {
        // Check for token expiration markers
        if s.contains("TOKEN_EXPIRED") || s.contains("expired") || s.contains("Token expired") {
            return WidgetError::TokenExpired;
        }

        // Check for HTTP status codes
        if s.starts_with("HTTP ") {
            if let Some(status_str) = s.split(':').next().and_then(|p| p.split(' ').nth(1)) {
                if let Ok(status) = status_str.parse::<u16>() {
                    let message = s
                        .split(':')
                        .skip(1)
                        .collect::<Vec<_>>()
                        .join(":")
                        .trim()
                        .to_string();
                    return WidgetError::Http { status, message };
                }
            }
        }

        // Check for specific error types
        if s.contains("Network error") || s.contains("Request failed") {
            WidgetError::Network(s)
        } else if s.contains("parse") || s.contains("Failed to parse") {
            WidgetError::Parse(s)
        } else {
            WidgetError::Other(s)
        }
    }
}

impl From<&str> for WidgetError {
    fn from(s: &str) -> Self {
        WidgetError::from(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_expired_detection() {
        assert!(WidgetError::from("TOKEN_EXPIRED").is_auth_error());
        assert!(WidgetError::from("Token expired").is_auth_error());
        assert!(WidgetError::TokenExpired.is_auth_error());
    }

    #[test]
    fn test_http_401_detection() {
        let err = WidgetError::from("HTTP 401: Unauthorized");
        assert!(err.is_auth_error());
        assert_eq!(
            err.user_message(),
            "Your session has expired. Please run the command again from Discord."
        );
    }

    #[test]
    fn test_http_error_parsing() {
        let err = WidgetError::from("HTTP 500: Internal server error");
        if let WidgetError::Http { status, message } = err {
            assert_eq!(status, 500);
            assert_eq!(message, "Internal server error");
        } else {
            panic!("Expected Http variant");
        }
    }

    #[test]
    fn test_network_error() {
        let err = WidgetError::from("Request failed: connection timeout");
        assert!(matches!(err, WidgetError::Network(_)));
        assert!(!err.is_auth_error());
    }
}
