use serde::de::DeserializeOwned;
use thiserror::Error;
use worker_stack::worker;

#[derive(Error, Debug)]
pub enum ServiceBindingError {
    #[error("Worker error: {0}")]
    Worker(#[from] worker::Error),

    #[error("JSON deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Service binding not implemented for native builds (this only runs in WASM)")]
    NotImplemented,

    #[error("HTTP error: status {status}: {body}")]
    HttpError { status: u16, body: String },
}

impl ServiceBindingError {
    /// Returns true if this error is retryable (transport failures or 5xx).
    #[cfg(target_arch = "wasm32")]
    fn is_retryable(&self) -> bool {
        match self {
            ServiceBindingError::Worker(_) => true,
            ServiceBindingError::HttpError { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

/// Authentication mode for service binding calls.
#[derive(Debug, Clone, Copy, Default)]
pub enum Auth<'a> {
    #[default]
    None,
    Bearer(&'a str),
    InternalKey(&'a str),
}

/// Options for service binding calls. Controls retry behaviour and authentication.
///
/// Default: 3 attempts (1 + 2 retries) with 50ms/200ms backoff, no auth.
#[derive(Debug, Clone, Copy)]
pub struct CallOpts<'a> {
    pub max_attempts: u32,
    pub backoff_ms: &'a [i32],
    pub auth: Auth<'a>,
}

impl Default for CallOpts<'_> {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_ms: &[50, 200],
            auth: Auth::None,
        }
    }
}

impl<'a> CallOpts<'a> {
    /// No retry, no auth — single attempt.
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            backoff_ms: &[],
            auth: Auth::None,
        }
    }

    /// Default retry with an internal API key.
    pub fn with_internal_key(api_key: &'a str) -> Self {
        Self {
            auth: Auth::InternalKey(api_key),
            ..Default::default()
        }
    }

    /// Default retry with a bearer token.
    pub fn with_bearer(token: &'a str) -> Self {
        Self {
            auth: Auth::Bearer(token),
            ..Default::default()
        }
    }
}

/// Header name used for request tracing across service binding calls.
pub const REQUEST_ID_HEADER: &str = "X-Request-Id";

/// Generate a short unique request ID for tracing.
///
/// Format: `{timestamp_hex}-{random_hex}` e.g. `18f3a2b1c00-4a7f2e`
#[cfg(target_arch = "wasm32")]
fn generate_request_id() -> String {
    let ts = worker_stack::js_sys::Date::now() as u64;
    let rand = (worker_stack::js_sys::Math::random() * 0xFFFFFF as f64) as u32;
    format!("{ts:x}-{rand:06x}")
}

/// Helper for calling Cloudflare Workers service bindings
pub struct ServiceBinding {
    #[allow(dead_code)] // Only used in WASM builds
    fetcher: worker::Fetcher,
}

impl ServiceBinding {
    pub fn new(fetcher: worker::Fetcher) -> Self {
        Self { fetcher }
    }

    #[allow(dead_code)] // Only used in WASM builds
    fn normalize_url(&self, url: &str) -> String {
        if url.starts_with("https://") {
            url.to_string()
        } else {
            format!("https://service{url}")
        }
    }

    /// GET a URL and deserialize the JSON response.
    pub async fn fetch_json<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
        opts: &CallOpts<'_>,
    ) -> Result<T, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            let mut response = self.get_with_retry(url.as_ref(), opts).await?;
            let data = response.json().await?;
            Ok(data)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, opts);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// GET a URL and return the text response.
    pub async fn fetch_text(
        &self,
        url: impl AsRef<str>,
        opts: &CallOpts<'_>,
    ) -> Result<String, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            let mut response = self.get_with_retry(url.as_ref(), opts).await?;
            let text = response.text().await?;
            Ok(text)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, opts);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// POST a JSON body and deserialize the JSON response.
    pub async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        opts: &CallOpts<'_>,
    ) -> Result<T, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            let mut response = self.post_impl(url.as_ref(), body, opts).await?;
            let data = response.json().await?;
            Ok(data)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, body, opts);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// POST a JSON body, ignoring the response body.
    /// Returns `Ok(())` on 2xx, or `HttpError` otherwise.
    pub async fn post<B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        opts: &CallOpts<'_>,
    ) -> Result<(), ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = self.post_impl(url.as_ref(), body, opts).await?;
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, body, opts);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// Apply auth headers from CallOpts to a Headers object.
    #[cfg(target_arch = "wasm32")]
    fn apply_auth(headers: &worker::Headers, opts: &CallOpts<'_>) -> Result<(), worker::Error> {
        match opts.auth {
            Auth::None => {}
            Auth::Bearer(token) => {
                headers.set("Authorization", &format!("Bearer {token}"))?;
            }
            Auth::InternalKey(key) => {
                headers.set("X-Internal-Key", key)?;
            }
        }
        Ok(())
    }

    /// Execute a GET request with optional retry on transient failures.
    #[cfg(target_arch = "wasm32")]
    async fn get_with_retry(
        &self,
        url: &str,
        opts: &CallOpts<'_>,
    ) -> Result<worker::Response, ServiceBindingError> {
        let normalized_url = self.normalize_url(url);
        let request_id = generate_request_id();
        let mut last_err: Option<ServiceBindingError> = None;

        for attempt in 0..opts.max_attempts {
            let mut init = worker::RequestInit::new();
            init.method = worker::Method::Get;
            let headers = worker::Headers::new();
            headers.set(REQUEST_ID_HEADER, &request_id)?;
            Self::apply_auth(&headers, opts)?;
            init.headers = headers;

            let request = worker::Request::new_with_init(&normalized_url, &init)?;

            match self.fetcher.fetch_request(request).await {
                Ok(mut response) => {
                    let status = response.status_code();
                    if (200..300).contains(&status) {
                        return Ok(response);
                    }

                    let body = response.text().await.unwrap_or_default();
                    let err = ServiceBindingError::HttpError { status, body };

                    if err.is_retryable() && attempt + 1 < opts.max_attempts {
                        tracing::warn!(
                            "Service binding GET {url} returned {status}, attempt {}/{} — retrying [req_id={request_id}]",
                            attempt + 1,
                            opts.max_attempts,
                        );
                        if let Some(&delay) = opts.backoff_ms.get(attempt as usize) {
                            crate::sleep::sleep(delay).await;
                        }
                        last_err = Some(err);
                        continue;
                    }

                    return Err(err);
                }
                Err(e) => {
                    if attempt + 1 < opts.max_attempts {
                        tracing::warn!(
                            "Service binding GET {url} failed: {e}, attempt {}/{} — retrying [req_id={request_id}]",
                            attempt + 1,
                            opts.max_attempts,
                        );
                        if let Some(&delay) = opts.backoff_ms.get(attempt as usize) {
                            crate::sleep::sleep(delay).await;
                        }
                        last_err = Some(ServiceBindingError::Worker(e));
                        continue;
                    }

                    return Err(ServiceBindingError::Worker(e));
                }
            }
        }

        Err(last_err.unwrap_or(ServiceBindingError::NotImplemented))
    }

    /// Execute a POST request (single attempt — no retry by default for non-idempotent operations).
    #[cfg(target_arch = "wasm32")]
    async fn post_impl<B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
        opts: &CallOpts<'_>,
    ) -> Result<worker::Response, ServiceBindingError> {
        let normalized_url = self.normalize_url(url);

        let mut init = worker::RequestInit::new();
        init.method = worker::Method::Post;

        let body_json = serde_json::to_string(body)?;
        init.body = Some(body_json.into());

        let request_id = generate_request_id();
        let headers = worker::Headers::new();
        headers.set("Content-Type", "application/json")?;
        headers.set(REQUEST_ID_HEADER, &request_id)?;
        Self::apply_auth(&headers, opts)?;
        init.headers = headers;

        let request = worker::Request::new_with_init(&normalized_url, &init)?;
        let mut response = self.fetcher.fetch_request(request).await?;

        let status = response.status_code();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ServiceBindingError::HttpError { status, body });
        }

        Ok(response)
    }
}

/// Extension trait for `worker::Env` to get service bindings more ergonomically
pub trait ServiceBindingExt {
    /// Get a service binding helper
    fn service_binding(&self, binding: &str) -> worker::Result<ServiceBinding>;
}

impl ServiceBindingExt for worker::Env {
    fn service_binding(&self, binding: &str) -> worker::Result<ServiceBinding> {
        let fetcher = self.service(binding)?;
        Ok(ServiceBinding::new(fetcher))
    }
}
