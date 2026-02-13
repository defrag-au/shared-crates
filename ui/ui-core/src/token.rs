//! JWT token parsing utilities
//!
//! Provides client-side token decoding for extracting claims without verification.
//! Note: This is for display/routing purposes only - the server always verifies tokens.

use base64::Engine;
use serde::{Deserialize, Serialize};

/// Claims extracted from a widget JWT token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetClaims {
    /// Subject (user ID as string)
    pub sub: String,
    /// Guild ID
    pub guild_id: String,
    /// Expiration timestamp
    pub exp: u64,
    /// Issued at timestamp
    pub iat: u64,
    /// Token type (should be "widget")
    #[serde(default)]
    pub token_type: String,
    /// Action details
    #[serde(default)]
    pub action: ActionClaims,
    /// User display name (guild nickname > global_name > username)
    #[serde(default)]
    pub display_name: Option<String>,
    /// User avatar hash (for constructing CDN URL)
    #[serde(default)]
    pub avatar_hash: Option<String>,
    /// Whether the user is a platform admin
    #[serde(default)]
    pub is_admin: bool,
}

/// Action-specific claims
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionClaims {
    /// Action type (e.g., "view_map", "tribe_builder")
    #[serde(default, rename = "type")]
    pub action_type: String,
    /// Additional params (flattened from widget_params in config)
    /// Contains world, season, project, template, etc.
    #[serde(flatten)]
    pub params: std::collections::HashMap<String, String>,
}

impl ActionClaims {
    /// Get a param value by key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    /// Get world param
    pub fn world(&self) -> Option<&str> {
        self.get("world")
    }

    /// Get season param
    pub fn season(&self) -> Option<&str> {
        self.get("season")
    }

    /// Get project param
    pub fn project(&self) -> Option<&str> {
        self.get("project")
    }

    /// Get deep link path param (for navigation after load)
    pub fn deep_link_path(&self) -> Option<String> {
        self.get("deep_link").map(|s| s.to_string())
    }
}

impl WidgetClaims {
    /// Create mock claims for dev mode
    ///
    /// This creates claims that look valid but are not backed by a real token.
    /// Only for local development/testing.
    #[cfg(feature = "dev")]
    pub fn mock(user_id: impl Into<String>, guild_id: impl Into<String>) -> Self {
        let now = (js_sys::Date::now() as u64) / 1000;
        Self {
            sub: user_id.into(),
            guild_id: guild_id.into(),
            exp: now + 3600, // 1 hour from now
            iat: now,
            token_type: "widget".to_string(),
            action: ActionClaims::default(),
            display_name: Some("Dev User".to_string()),
            avatar_hash: None,
            is_admin: true, // Dev users are admins
        }
    }

    /// Get the user ID as u64
    pub fn user_id(&self) -> u64 {
        self.sub.parse().unwrap_or(0)
    }

    /// Get the user ID as a string reference
    pub fn user_id_str(&self) -> &str {
        &self.sub
    }

    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        let now = js_sys::Date::now() as u64 / 1000;
        self.exp < now
    }

    /// Get seconds until expiration (0 if already expired)
    pub fn seconds_until_expiry(&self) -> u64 {
        let now = js_sys::Date::now() as u64 / 1000;
        self.exp.saturating_sub(now)
    }

    /// Construct the Discord CDN avatar URL from user_id and avatar_hash
    pub fn avatar_url(&self) -> Option<String> {
        self.avatar_hash.as_ref().map(|hash| {
            format!(
                "https://cdn.discordapp.com/avatars/{}/{}.png",
                self.sub, hash
            )
        })
    }

    /// Get deep link path from action params (for navigation after load)
    pub fn deep_link_path(&self) -> Option<String> {
        self.action.deep_link_path()
    }

    /// Get a param value by key from action params
    pub fn get_param(&self, key: &str) -> Option<String> {
        self.action.get(key).map(|s| s.to_string())
    }

    /// Get Discord channel URL for deep linking back to Discord
    pub fn discord_channel_url(&self) -> Option<String> {
        self.action
            .get("discord_channel_url")
            .map(|s| s.to_string())
    }
}

/// Decode JWT claims without verification
///
/// This extracts the payload from a JWT for client-side use.
/// The server always verifies tokens - this is just for display/routing.
pub fn decode_token_claims(token: &str) -> Option<WidgetClaims> {
    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        tracing::warn!("Invalid JWT format: expected 3 parts, got {}", parts.len());
        return None;
    }

    // Decode the payload (middle part)
    let payload = parts[1];

    // JWT uses base64url encoding (no padding)
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;

    let claims: WidgetClaims = serde_json::from_slice(&decoded).ok()?;
    Some(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_claims() {
        // This would need a real token to test properly
        // Just verify the function doesn't panic on invalid input
        assert!(decode_token_claims("invalid").is_none());
        assert!(decode_token_claims("a.b.c").is_none());
    }
}
