//! Axum utilities for Cloudflare Workers
//!
//! This module provides utilities specific to using Axum with Cloudflare Workers,
//! particularly around handling the WASM execution constraints.

/// Macro to execute non-Send operations in WASM
///
/// This macro works around Cloudflare Workers' WASM constraints by spawning
/// async operations locally and using oneshot channels to return results.
///
/// Based on: https://github.com/cloudflare/workers-rs/issues/702#issuecomment-2702062889
///
/// # Example
///
/// ```rust
/// use worker_utils::exec_nonsend;
///
/// // Execute database operation that doesn't implement Send
/// let result = exec_nonsend! {
///     database.get_user(&user_id).await
/// };
/// ```
#[macro_export]
macro_rules! exec_nonsend {
    ($($code:tt)*) => {{
        let (tx, rx) = futures_channel::oneshot::channel();
        worker_stack::wasm_bindgen_futures::spawn_local(async move {
            let result = { $($code)* };
            let _ = tx.send(result);
        });
        rx.await
    }};
}

/// Re-export the macro for convenient access
pub use exec_nonsend;

use axum::http::{HeaderName, HeaderValue, Method};
use axum::response::IntoResponse;
use tower_http::cors::CorsLayer;

/// Extension trait for common Axum response patterns in Workers
pub trait WorkerResponseExt {
    /// Create a JSON success response
    fn success_json<T: serde::Serialize>(data: T) -> axum::response::Response;

    /// Create a JSON error response
    fn error_json(message: &str, status: axum::http::StatusCode) -> axum::response::Response;
}

impl WorkerResponseExt for axum::response::Response {
    fn success_json<T: serde::Serialize>(data: T) -> axum::response::Response {
        axum::Json(serde_json::json!({
            "success": true,
            "data": data
        }))
        .into_response()
    }

    fn error_json(message: &str, status: axum::http::StatusCode) -> axum::response::Response {
        (
            status,
            axum::Json(serde_json::json!({
                "success": false,
                "message": message
            })),
        )
            .into_response()
    }
}

/// Common result type for Worker operations that might fail with oneshot channel errors
pub type WasmResult<T> = Result<T, futures_channel::oneshot::Canceled>;

/// Create a CORS layer with required allowed origins
///
/// This function provides a reusable way to configure CORS for Axum applications with
/// credential support while maintaining security best practices.
///
/// Returns None if no domains are provided, as credentials cannot be used with wildcard origins.
/// This is safer than falling back to an insecure configuration.
///
/// This approach requires explicit CORS configuration:
/// - Set `CORS_ALLOWED_ORIGINS` in wrangler.toml for each environment
/// - No CORS layer is applied if the variable is missing (safer default)
///
/// # Security Notes
/// - Always uses explicit method and header lists (required when credentials are enabled)
/// - Validates domain parsing and falls back to permissive mode on errors
/// - Supports common headers needed for modern web applications
///
/// # Arguments
/// * `domains` - Optional list of allowed domains. None means allow any origin.
///
/// # Supported Methods
/// - GET, POST, PUT, DELETE, OPTIONS, PATCH
///
/// # Supported Headers
/// - authorization (for Bearer tokens)
/// - content-type (for JSON requests)
/// - accept (for content negotiation)
/// - x-requested-with (for AJAX identification)
/// - cache-control (for caching directives)
///
/// # Example
/// ```rust
/// // Production with specific origins
/// if let Some(cors) = create_cors_layer(Some(vec![
///     "https://app.example.com".to_string(),
///     "https://admin.example.com".to_string(),
/// ])) {
///     router = router.layer(cors);
/// }
///
/// // From environment variable (typical usage)
/// let domains = env::var("CORS_ALLOWED_ORIGINS")
///     .ok()
///     .map(|origins| origins.split(',').map(|s| s.trim().to_string()).collect());
/// if let Some(cors) = create_cors_layer(domains) {
///     router = router.layer(cors);
/// }
/// ```
pub fn create_cors_layer(domains: Option<Vec<String>>) -> Option<CorsLayer> {
    let base_cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
            Method::PATCH,
        ])
        .allow_headers([
            HeaderName::from_static("authorization"),
            HeaderName::from_static("content-type"),
            HeaderName::from_static("accept"),
            HeaderName::from_static("x-requested-with"),
            HeaderName::from_static("cache-control"),
        ])
        .allow_credentials(true);

    match domains {
        Some(domain_list) => {
            // Restrict to specific domains
            let origins: std::result::Result<
                Vec<HeaderValue>,
                axum::http::header::InvalidHeaderValue,
            > = domain_list
                .into_iter()
                .map(|domain| domain.parse::<HeaderValue>())
                .collect();

            match origins {
                Ok(valid_origins) => Some(base_cors.allow_origin(valid_origins)),
                Err(_) => {
                    tracing::warn!("Invalid domain in CORS configuration, CORS layer disabled");
                    None
                }
            }
        }
        None => {
            // No origins specified - no CORS layer applied
            tracing::info!("No CORS_ALLOWED_ORIGINS specified, CORS layer disabled");
            None
        }
    }
}

/// Create a CORS layer from environment variable
///
/// Convenience function that reads a comma-separated list of origins from an environment
/// variable and creates a CORS layer. This is the most common usage pattern for workers.
///
/// # Arguments
/// * `env` - The worker environment
/// * `var_name` - Name of the environment variable containing comma-separated origins
///
/// # Returns
/// * `Some(CorsLayer)` - If the variable exists and contains valid domains
/// * `None` - If the variable is missing or contains invalid domains
///
/// # Example
/// ```rust
/// // Typical usage in worker
/// let mut router = create_router(app_state);
/// if let Some(cors) = create_cors_layer_from_env(&env, "CORS_ALLOWED_ORIGINS") {
///     router = router.layer(cors);
/// }
///
/// // Works with any variable name
/// if let Some(cors) = create_cors_layer_from_env(&env, "API_ALLOWED_ORIGINS") {
///     router = router.layer(cors);
/// }
/// ```
pub fn create_cors_layer_from_env(
    env: &worker_stack::worker::Env,
    var_name: &str,
) -> Option<CorsLayer> {
    let domains = env.var(var_name).ok().map(|origins| {
        // Parse comma-separated list of origins
        origins
            .to_string()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    create_cors_layer(domains)
}
