/// Axum utilities for Cloudflare Workers
/// 
/// This module provides utilities specific to using Axum with Cloudflare Workers,
/// particularly around handling the WASM execution constraints.

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
        wasm_bindgen_futures::spawn_local(async move {
            let result = { $($code)* };
            let _ = tx.send(result);
        });
        rx.await
    }};
}

/// Re-export the macro for convenient access
pub use exec_nonsend;

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
        })).into_response()
    }
    
    fn error_json(message: &str, status: axum::http::StatusCode) -> axum::response::Response {
        (status, axum::Json(serde_json::json!({
            "success": false,
            "message": message
        }))).into_response()
    }
}

/// Common result type for Worker operations that might fail with oneshot channel errors
pub type WasmResult<T> = Result<T, futures_channel::oneshot::Canceled>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exec_nonsend_macro() {
        // Simple test that the macro compiles and works
        let result: WasmResult<i32> = exec_nonsend! {
            Ok(42)
        };
        
        assert_eq!(result.unwrap(), 42);
    }
}