//! Framework-agnostic HTTP helpers using gloo-net
//!
//! These helpers work with any frontend framework (Seed, Leptos, Yew, etc.)

use crate::auth::AuthState;
use crate::error::WidgetError;
use gloo_net::http::Request;
use serde::de::DeserializeOwned;

/// Helper trait to add optional authorization to requests
trait RequestAuth {
    fn with_auth(self, auth: Option<&AuthState>) -> Self;
}

impl RequestAuth for gloo_net::http::RequestBuilder {
    fn with_auth(self, auth: Option<&AuthState>) -> Self {
        match auth.and_then(|a| a.token()) {
            Some(token) => self.header("Authorization", &format!("Bearer {token}")),
            None => self,
        }
    }
}

/// Helper to make GET requests with proper error handling
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn get_json<T: DeserializeOwned>(
    url: &str,
    auth: Option<&AuthState>,
) -> Result<T, WidgetError> {
    let response = Request::get(url)
        .with_auth(auth)
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_response(response).await
}

/// Helper to make POST requests with JSON body
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
    url: &str,
    auth: Option<&AuthState>,
    body: &B,
) -> Result<T, WidgetError> {
    let response = Request::post(url)
        .with_auth(auth)
        .json(body)
        .map_err(|e| WidgetError::Other(format!("Failed to serialize request: {e}")))?
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_response(response).await
}

/// Helper to handle HTTP responses with proper error detection
async fn handle_response<T: DeserializeOwned>(
    response: gloo_net::http::Response,
) -> Result<T, WidgetError> {
    let status = response.status();

    if !response.ok() {
        let text = response.text().await.unwrap_or_default();

        // Check for token expiration in response body
        if text.contains("expired") || text.contains("Token expired") {
            return Err(WidgetError::TokenExpired);
        }

        // Return HTTP error with status and message
        return Err(WidgetError::Http {
            status,
            message: text,
        });
    }

    response
        .json::<T>()
        .await
        .map_err(|e| WidgetError::Parse(format!("Failed to parse response: {e}")))
}

/// Helper to make GET requests that return nothing (just check for errors)
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn get_ok(url: &str, auth: Option<&AuthState>) -> Result<(), WidgetError> {
    let response = Request::get(url)
        .with_auth(auth)
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_ok_response(response).await
}

/// Helper to make POST requests that return nothing (just check for errors)
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn post_ok<B: serde::Serialize>(
    url: &str,
    auth: Option<&AuthState>,
    body: &B,
) -> Result<(), WidgetError> {
    let response = Request::post(url)
        .with_auth(auth)
        .json(body)
        .map_err(|e| WidgetError::Other(format!("Failed to serialize request: {e}")))?
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_ok_response(response).await
}

/// Helper to handle OK responses (no body expected)
async fn handle_ok_response(response: gloo_net::http::Response) -> Result<(), WidgetError> {
    let status = response.status();

    if !response.ok() {
        let text = response.text().await.unwrap_or_default();

        if text.contains("expired") || text.contains("Token expired") {
            return Err(WidgetError::TokenExpired);
        }

        return Err(WidgetError::Http {
            status,
            message: text,
        });
    }

    Ok(())
}

/// Helper to fetch binary data (e.g., MessagePack-encoded data)
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn get_binary(url: &str, auth: Option<&AuthState>) -> Result<Vec<u8>, WidgetError> {
    let response = Request::get(url)
        .with_auth(auth)
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_binary_response(response).await
}

/// Helper to handle binary responses
async fn handle_binary_response(
    response: gloo_net::http::Response,
) -> Result<Vec<u8>, WidgetError> {
    let status = response.status();

    if !response.ok() {
        let text = response.text().await.unwrap_or_default();

        if text.contains("expired") || text.contains("Token expired") {
            return Err(WidgetError::TokenExpired);
        }

        return Err(WidgetError::Http {
            status,
            message: text,
        });
    }

    response
        .binary()
        .await
        .map_err(|e| WidgetError::Network(format!("Failed to read binary data: {e}")))
}

/// Helper to fetch text response
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn get_text(url: &str, auth: Option<&AuthState>) -> Result<String, WidgetError> {
    let response = Request::get(url)
        .with_auth(auth)
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_text_response(response).await
}

/// Helper to handle text responses
async fn handle_text_response(response: gloo_net::http::Response) -> Result<String, WidgetError> {
    let status = response.status();

    if !response.ok() {
        let text = response.text().await.unwrap_or_default();

        if text.contains("expired") || text.contains("Token expired") {
            return Err(WidgetError::TokenExpired);
        }

        return Err(WidgetError::Http {
            status,
            message: text,
        });
    }

    response
        .text()
        .await
        .map_err(|e| WidgetError::Network(format!("Failed to read text: {e}")))
}

/// Helper to make PATCH requests with JSON body
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn patch_json<B: serde::Serialize, T: DeserializeOwned>(
    url: &str,
    auth: Option<&AuthState>,
    body: &B,
) -> Result<T, WidgetError> {
    let response = Request::patch(url)
        .with_auth(auth)
        .json(body)
        .map_err(|e| WidgetError::Other(format!("Failed to serialize request: {e}")))?
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_response(response).await
}

/// Helper to make DELETE requests that return JSON
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn delete_json<T: DeserializeOwned>(
    url: &str,
    auth: Option<&AuthState>,
) -> Result<T, WidgetError> {
    let response = Request::delete(url)
        .with_auth(auth)
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_response(response).await
}

/// Helper to make DELETE requests that return nothing (just check for errors)
///
/// Pass `Some(&auth)` for authenticated requests, or `None` for unauthenticated.
pub async fn delete_ok(url: &str, auth: Option<&AuthState>) -> Result<(), WidgetError> {
    let response = Request::delete(url)
        .with_auth(auth)
        .send()
        .await
        .map_err(|e| WidgetError::Network(format!("Request failed: {e}")))?;

    handle_ok_response(response).await
}
