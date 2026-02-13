//! Widget URL building utilities
//!
//! Provides consistent URL construction for cross-widget navigation.
//! This ensures all widgets use the correct URL format with JWT tokens.

/// Build a widget URL with the given token.
///
/// # Arguments
/// * `widget_path` - The widget path (e.g., "/reports/", "/map/")
/// * `token` - The JWT token for authentication
///
/// # Returns
/// A properly formatted widget URL with token query parameter
///
/// # Example
/// ```
/// use ui_core::urls::build_widget_url;
///
/// let url = build_widget_url("/reports/", "eyJhbGci...");
/// assert_eq!(url, "/reports/?token=eyJhbGci...");
/// ```
pub fn build_widget_url(widget_path: &str, token: &str) -> String {
    let path = widget_path.trim_end_matches('/');
    format!("{path}/?token={token}")
}

/// Build a widget URL with the given token and additional query parameters.
///
/// # Arguments
/// * `widget_path` - The widget path (e.g., "/reports/", "/map/")
/// * `token` - The JWT token for authentication
/// * `params` - Additional query parameters as (key, value) pairs
///
/// # Example
/// ```
/// use ui_core::urls::build_widget_url_with_params;
///
/// let url = build_widget_url_with_params(
///     "/reports/",
///     "eyJhbGci...",
///     &[("user_id", "123456789")]
/// );
/// assert_eq!(url, "/reports/?token=eyJhbGci...&user_id=123456789");
/// ```
pub fn build_widget_url_with_params(
    widget_path: &str,
    token: &str,
    params: &[(&str, &str)],
) -> String {
    let path = widget_path.trim_end_matches('/');
    let mut url = format!("{path}/?token={token}");
    for (key, value) in params {
        url.push_str(&format!("&{key}={value}"));
    }
    url
}

/// Build a widget URL from an optional token.
///
/// Returns `None` if no token is available.
///
/// # Arguments
/// * `widget_path` - The widget path (e.g., "/reports/", "/map/")
/// * `token` - Optional token string
///
/// # Example
/// ```
/// use ui_core::urls::build_widget_url_optional;
///
/// let url = build_widget_url_optional("/reports/", Some("abc123"));
/// assert_eq!(url, Some("/reports/?token=abc123".to_string()));
///
/// let url = build_widget_url_optional("/reports/", None);
/// assert_eq!(url, None);
/// ```
pub fn build_widget_url_optional(widget_path: &str, token: Option<&str>) -> Option<String> {
    token.map(|t| build_widget_url(widget_path, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_widget_url() {
        let url = build_widget_url("/reports/", "abc123");
        assert_eq!(url, "/reports/?token=abc123");
    }

    #[test]
    fn test_build_widget_url_strips_trailing_slash() {
        let url = build_widget_url("/reports", "abc123");
        assert_eq!(url, "/reports/?token=abc123");
    }

    #[test]
    fn test_build_widget_url_with_params() {
        let url = build_widget_url_with_params("/reports/", "abc123", &[("user_id", "999")]);
        assert_eq!(url, "/reports/?token=abc123&user_id=999");
    }

    #[test]
    fn test_build_widget_url_with_multiple_params() {
        let url = build_widget_url_with_params(
            "/reports/",
            "abc123",
            &[("user_id", "999"), ("mode", "admin")],
        );
        assert_eq!(url, "/reports/?token=abc123&user_id=999&mode=admin");
    }

    #[test]
    fn test_build_widget_url_optional_with_token() {
        let url = build_widget_url_optional("/map/", Some("xyz"));
        assert_eq!(url, Some("/map/?token=xyz".to_string()));
    }

    #[test]
    fn test_build_widget_url_optional_without_token() {
        let url = build_widget_url_optional("/map/", None);
        assert_eq!(url, None);
    }
}
