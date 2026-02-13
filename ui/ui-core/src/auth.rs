//! Authentication state and context
//!
//! Provides authentication state management with explicit error variants.
//! When authenticated, an `AuthContext` holds the token and decoded claims.

use crate::token::{decode_token_claims, WidgetClaims};

/// Authentication state
#[derive(Debug, Clone)]
pub enum AuthState {
    /// No authentication - anonymous access
    Anonymous,
    /// Authentication in progress (e.g., OAuth redirect, token refresh)
    Authenticating,
    /// Authenticated with token and decoded claims
    Authenticated(Box<AuthContext>),
    /// Token has expired - user needs to get a fresh link
    TokenExpired,
    /// Token was invalid (malformed or unverifiable)
    AuthError(String),
}

/// Authenticated user context with token and decoded claims
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// The raw JWT token for API calls
    token: String,
    /// Decoded claims from the token
    claims: WidgetClaims,
}

impl AuthContext {
    /// Create a new auth context from a token
    ///
    /// Returns None if the token cannot be decoded.
    pub fn from_token(token: String) -> Option<Self> {
        let claims = decode_token_claims(&token)?;
        Some(Self { token, claims })
    }

    /// Create a mock auth context for dev mode
    ///
    /// This creates a context that behaves like authenticated but uses mock claims.
    /// The token will be a placeholder that won't work for API calls.
    #[cfg(feature = "dev")]
    pub fn mock(user_id: impl Into<String>, guild_id: impl Into<String>) -> Self {
        Self {
            token: "mock_token_for_dev_mode".to_string(),
            claims: WidgetClaims::mock(user_id, guild_id),
        }
    }

    /// Get the raw token for API passthrough
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get the decoded claims
    pub fn claims(&self) -> &WidgetClaims {
        &self.claims
    }

    /// Get the user ID as u64
    pub fn user_id(&self) -> u64 {
        self.claims.user_id()
    }

    /// Get the user ID as a string
    pub fn user_id_str(&self) -> &str {
        self.claims.user_id_str()
    }

    /// Get the guild ID
    pub fn guild_id(&self) -> &str {
        &self.claims.guild_id
    }

    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        self.claims.is_expired()
    }

    /// Get seconds until token expiry
    pub fn seconds_until_expiry(&self) -> u64 {
        self.claims.seconds_until_expiry()
    }

    /// Get the user's display name if available
    pub fn display_name(&self) -> Option<&str> {
        self.claims.display_name.as_deref()
    }

    /// Get the user's avatar URL if available
    pub fn avatar_url(&self) -> Option<String> {
        self.claims.avatar_url()
    }
}

impl AuthState {
    /// Create auth state from a token string
    ///
    /// Returns appropriate variant based on token validity:
    /// - `Authenticated` if token is valid and not expired
    /// - `TokenExpired` if token is valid but expired
    /// - `AuthError` if token cannot be decoded
    /// - `Anonymous` if no token provided
    pub fn from_token(token: Option<String>) -> Self {
        match token.and_then(|t| if t.is_empty() { None } else { Some(t) }) {
            Some(token) => match AuthContext::from_token(token) {
                Some(ctx) => {
                    if ctx.is_expired() {
                        Self::TokenExpired
                    } else {
                        Self::Authenticated(Box::new(ctx))
                    }
                }
                None => Self::AuthError("Could not decode token".to_string()),
            },
            None => Self::Anonymous,
        }
    }

    /// Create auth state from URL query parameters
    ///
    /// Looks for a `token` parameter in the current URL.
    pub fn from_current_url() -> Self {
        Self::from_token(get_url_param("token"))
    }

    /// Check if authenticated (valid, non-expired token)
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated(_))
    }

    /// Check if anonymous (no token provided)
    pub fn is_anonymous(&self) -> bool {
        matches!(self, Self::Anonymous)
    }

    /// Check if authentication is in progress
    pub fn is_authenticating(&self) -> bool {
        matches!(self, Self::Authenticating)
    }

    /// Check if token has expired
    pub fn is_token_expired(&self) -> bool {
        matches!(self, Self::TokenExpired)
    }

    /// Check if there was an auth error
    pub fn is_error(&self) -> bool {
        matches!(self, Self::AuthError(_))
    }

    /// Get the token if authenticated
    pub fn token(&self) -> Option<&str> {
        match self {
            Self::Authenticated(ctx) => Some(ctx.token()),
            _ => None,
        }
    }

    /// Get the auth context if authenticated
    pub fn context(&self) -> Option<&AuthContext> {
        match self {
            Self::Authenticated(ctx) => Some(ctx),
            _ => None,
        }
    }

    /// Get the user ID if authenticated
    pub fn user_id(&self) -> Option<u64> {
        self.context().map(|ctx| ctx.user_id())
    }

    /// Get the guild ID if authenticated
    pub fn guild_id(&self) -> Option<&str> {
        self.context().map(|ctx| ctx.guild_id())
    }

    /// Get error message if in error state
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::AuthError(msg) => Some(msg),
            _ => None,
        }
    }

    /// Create a mock authenticated state for dev mode
    ///
    /// This creates an Authenticated state with mock claims.
    /// Use for local development without real tokens.
    #[cfg(feature = "dev")]
    pub fn mock_authenticated(user_id: impl Into<String>, guild_id: impl Into<String>) -> Self {
        Self::Authenticated(Box::new(AuthContext::mock(user_id, guild_id)))
    }
}

/// Get a URL query parameter from the current window location
pub fn get_url_param(name: &str) -> Option<String> {
    let window = web_sys::window()?;
    let location = window.location();
    let search = location.search().ok()?;

    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get(name)
}
