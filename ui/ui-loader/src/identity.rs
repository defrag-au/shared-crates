//! Cross-platform user identity
//!
//! The [`Identity`] enum represents a user's identity in a way that works across
//! all WASM frameworks (Leptos, Seed, macroquad, etc.). It's designed to be:
//!
//! - Serializable to/from JSON for localStorage
//! - Independent of wasm-bindgen (works with quad-storage for macroquad)
//! - Extensible to support multiple identity providers
//!
//! ## Storage Key
//!
//! Identity is stored in localStorage under the key `widget_identity`.
//! The launcher HTML decodes the JWT and writes the identity before loading WASM.

use serde::{Deserialize, Serialize};

/// localStorage key where identity is stored by the launcher
pub const IDENTITY_STORAGE_KEY: &str = "widget_identity";

/// User identity from the launcher.
///
/// This enum supports multiple identity providers. The launcher HTML decodes
/// the JWT token and stores the appropriate variant in localStorage.
///
/// # Examples
///
/// ```ignore
/// use ui_loader::Identity;
///
/// let identity = Identity::from_json(json_str)?;
///
/// match &identity {
///     Identity::Anonymous => {
///         println!("Guest user");
///     }
///     Identity::Discord { user_id, display_name, .. } => {
///         println!("Discord user: {}", identity.name());
///     }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "provider", rename_all = "snake_case")]
#[derive(Default)]
pub enum Identity {
    /// Anonymous/guest user (no authentication)
    #[default]
    Anonymous,

    /// Discord-authenticated user
    Discord {
        /// Discord user ID (snowflake as string)
        user_id: String,

        /// Display name (Discord username or server nickname)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,

        /// Avatar URL
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar_url: Option<String>,

        /// Guild/server ID where the widget was launched
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guild_id: Option<String>,

        /// The original JWT token (for API calls that need it)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<String>,
    },
}

impl Identity {
    /// Check if this is an anonymous identity
    pub fn is_anonymous(&self) -> bool {
        matches!(self, Self::Anonymous)
    }

    /// Check if this is a Discord identity
    pub fn is_discord(&self) -> bool {
        matches!(self, Self::Discord { .. })
    }

    /// Get the user ID if available
    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::Anonymous => None,
            Self::Discord { user_id, .. } => Some(user_id),
        }
    }

    /// Get a display name, falling back to user ID or "Guest"
    pub fn name(&self) -> &str {
        match self {
            Self::Anonymous => "Guest",
            Self::Discord {
                display_name,
                user_id,
                ..
            } => display_name.as_deref().unwrap_or(user_id),
        }
    }

    /// Get the avatar URL if available
    pub fn avatar_url(&self) -> Option<&str> {
        match self {
            Self::Anonymous => None,
            Self::Discord { avatar_url, .. } => avatar_url.as_deref(),
        }
    }

    /// Get the JWT token if available (for API calls)
    pub fn token(&self) -> Option<&str> {
        match self {
            Self::Anonymous => None,
            Self::Discord { token, .. } => token.as_deref(),
        }
    }

    /// Get the guild ID if available
    pub fn guild_id(&self) -> Option<&str> {
        match self {
            Self::Anonymous => None,
            Self::Discord { guild_id, .. } => guild_id.as_deref(),
        }
    }

    /// Parse identity from a JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize identity to a JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Read identity from localStorage (web_sys version).
    ///
    /// This is for use with wasm-bindgen-based frameworks (Leptos, Seed, etc.).
    /// For macroquad, use `from_quad_storage()` instead.
    #[cfg(feature = "web")]
    pub fn from_local_storage() -> Self {
        let storage = match web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            Some(s) => s,
            None => {
                tracing::warn!("localStorage not available");
                return Self::Anonymous;
            }
        };

        let json = match storage.get_item(IDENTITY_STORAGE_KEY) {
            Ok(Some(json)) => json,
            Ok(None) => {
                tracing::debug!("No identity in localStorage");
                return Self::Anonymous;
            }
            Err(e) => {
                tracing::warn!("Failed to read localStorage: {:?}", e);
                return Self::Anonymous;
            }
        };

        match Self::from_json(&json) {
            Ok(identity) => {
                tracing::info!("Loaded identity from localStorage: {:?}", identity.name());
                identity
            }
            Err(e) => {
                tracing::warn!("Failed to parse identity from localStorage: {}", e);
                Self::Anonymous
            }
        }
    }

    /// Read identity from quad-storage (for macroquad).
    ///
    /// This is for use with macroquad which uses quad-storage for localStorage access.
    /// The caller must have quad-storage set up with the required JS files.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In your macroquad main.rs
    /// let identity = Identity::from_quad_storage();
    /// ```
    #[cfg(feature = "macroquad")]
    pub fn from_quad_storage() -> Self {
        let storage = match quad_storage::STORAGE.lock() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to lock quad-storage: {}", e);
                return Self::Anonymous;
            }
        };

        let json = match storage.get(IDENTITY_STORAGE_KEY) {
            Some(json) => json,
            None => {
                log::debug!("No identity in quad-storage");
                return Self::Anonymous;
            }
        };

        match Self::from_json(&json) {
            Ok(identity) => {
                log::info!("Loaded identity from quad-storage: {}", identity.name());
                identity
            }
            Err(e) => {
                log::warn!("Failed to parse identity from quad-storage: {}", e);
                Self::Anonymous
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_serialization() {
        let identity = Identity::Anonymous;
        let json = identity.to_json().unwrap();
        assert_eq!(json, r#"{"provider":"anonymous"}"#);

        let parsed = Identity::from_json(&json).unwrap();
        assert_eq!(parsed, Identity::Anonymous);
    }

    #[test]
    fn test_discord_serialization() {
        let identity = Identity::Discord {
            user_id: "123456789".to_string(),
            display_name: Some("TestUser".to_string()),
            avatar_url: Some("https://cdn.discord.com/avatar.png".to_string()),
            guild_id: Some("987654321".to_string()),
            token: Some("jwt.token.here".to_string()),
        };

        let json = identity.to_json().unwrap();
        assert!(json.contains(r#""provider":"discord""#));
        assert!(json.contains(r#""user_id":"123456789""#));

        let parsed = Identity::from_json(&json).unwrap();
        assert_eq!(parsed, identity);
    }

    #[test]
    fn test_discord_minimal() {
        // Only required field is user_id
        let json = r#"{"provider":"discord","user_id":"123"}"#;
        let identity = Identity::from_json(json).unwrap();

        match identity {
            Identity::Discord {
                user_id,
                display_name,
                avatar_url,
                guild_id,
                token,
            } => {
                assert_eq!(user_id, "123");
                assert!(display_name.is_none());
                assert!(avatar_url.is_none());
                assert!(guild_id.is_none());
                assert!(token.is_none());
            }
            _ => panic!("Expected Discord identity"),
        }
    }

    #[test]
    fn test_identity_helpers() {
        let anon = Identity::Anonymous;
        assert!(anon.is_anonymous());
        assert!(!anon.is_discord());
        assert_eq!(anon.name(), "Guest");
        assert!(anon.user_id().is_none());

        let discord = Identity::Discord {
            user_id: "123".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: None,
            guild_id: None,
            token: None,
        };
        assert!(!discord.is_anonymous());
        assert!(discord.is_discord());
        assert_eq!(discord.name(), "Alice");
        assert_eq!(discord.user_id(), Some("123"));
    }

    #[test]
    fn test_name_fallback() {
        // With display_name
        let with_name = Identity::Discord {
            user_id: "123".to_string(),
            display_name: Some("Alice".to_string()),
            avatar_url: None,
            guild_id: None,
            token: None,
        };
        assert_eq!(with_name.name(), "Alice");

        // Without display_name - falls back to user_id
        let without_name = Identity::Discord {
            user_id: "123".to_string(),
            display_name: None,
            avatar_url: None,
            guild_id: None,
            token: None,
        };
        assert_eq!(without_name.name(), "123");
    }
}
