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

    /// Fetch a URL and deserialize the JSON response
    ///
    /// # WASM Build
    /// Uses `worker::Response::json()` which works in the Cloudflare Workers runtime.
    ///
    /// # Native Build
    /// Returns `NotImplemented` error. This is only for LSP/cargo check and won't run.
    pub async fn fetch_json<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            self.fetch_json_wasm(url).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = url;
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// Fetch a URL and get the text response
    ///
    /// # WASM Build
    /// Uses `worker::Response::text()` which works in the Cloudflare Workers runtime.
    ///
    /// # Native Build
    /// Returns `NotImplemented` error. This is only for LSP/cargo check and won't run.
    pub async fn fetch_text(&self, url: impl AsRef<str>) -> Result<String, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            self.fetch_text_wasm(url).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = url;
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// POST a JSON body to a URL and deserialize the JSON response
    ///
    /// Optionally pass an auth token to set the `Authorization: Bearer` header.
    ///
    /// # WASM Build
    /// Uses `worker::Response::json()` which works in the Cloudflare Workers runtime.
    ///
    /// # Native Build
    /// Returns `NotImplemented` error. This is only for LSP/cargo check and won't run.
    pub async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        auth_token: Option<&str>,
    ) -> Result<T, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            self.post_json_wasm(url, body, auth_token).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, body, auth_token);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// WASM implementation: fetch and deserialize JSON
    #[cfg(target_arch = "wasm32")]
    async fn fetch_json_wasm<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, ServiceBindingError> {
        let normalized_url = self.normalize_url(url.as_ref());
        let mut response = self.fetcher.fetch(&normalized_url, None).await?;

        // Check status code (accept any 2xx)
        let status = response.status_code();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ServiceBindingError::HttpError { status, body });
        }

        // Deserialize JSON directly from worker::Response
        let data = response.json().await?;
        Ok(data)
    }

    /// WASM implementation: fetch and get text
    #[cfg(target_arch = "wasm32")]
    async fn fetch_text_wasm(&self, url: impl AsRef<str>) -> Result<String, ServiceBindingError> {
        let normalized_url = self.normalize_url(url.as_ref());
        let mut response = self.fetcher.fetch(&normalized_url, None).await?;

        // Check status code (accept any 2xx)
        let status = response.status_code();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ServiceBindingError::HttpError { status, body });
        }

        // Get text from worker::Response
        let text = response.text().await?;
        Ok(text)
    }

    /// GET a URL with an internal API key (X-Internal-Key header) and deserialize JSON
    pub async fn get_json_internal<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
        api_key: &str,
    ) -> Result<T, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            self.get_json_internal_wasm(url, api_key).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, api_key);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// WASM implementation: GET JSON with internal API key
    #[cfg(target_arch = "wasm32")]
    async fn get_json_internal_wasm<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
        api_key: &str,
    ) -> Result<T, ServiceBindingError> {
        let normalized_url = self.normalize_url(url.as_ref());

        let mut init = worker::RequestInit::new();
        init.method = worker::Method::Get;

        let headers = worker::Headers::new();
        headers.set("X-Internal-Key", api_key)?;
        init.headers = headers;

        let request = worker::Request::new_with_init(&normalized_url, &init)?;
        let mut response = self.fetcher.fetch_request(request).await?;

        let status = response.status_code();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ServiceBindingError::HttpError { status, body });
        }

        let data = response.json().await?;
        Ok(data)
    }

    /// POST a JSON body with an internal API key, ignoring the response body.
    /// Returns `Ok(())` on 2xx, or `HttpError` otherwise.
    pub async fn post_internal<B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        api_key: &str,
    ) -> Result<(), ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            self.post_internal_wasm(url, body, api_key).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, body, api_key);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// WASM implementation: POST with internal API key, no response deserialization
    #[cfg(target_arch = "wasm32")]
    async fn post_internal_wasm<B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        api_key: &str,
    ) -> Result<(), ServiceBindingError> {
        let normalized_url = self.normalize_url(url.as_ref());

        let mut init = worker::RequestInit::new();
        init.method = worker::Method::Post;

        let body_json = serde_json::to_string(body)?;
        init.body = Some(body_json.into());

        let headers = worker::Headers::new();
        headers.set("Content-Type", "application/json")?;
        headers.set("X-Internal-Key", api_key)?;
        init.headers = headers;

        let request = worker::Request::new_with_init(&normalized_url, &init)?;
        let mut response = self.fetcher.fetch_request(request).await?;

        let status = response.status_code();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ServiceBindingError::HttpError { status, body });
        }

        Ok(())
    }

    /// POST a JSON body with an internal API key (X-Internal-Key header)
    pub async fn post_json_internal<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        api_key: &str,
    ) -> Result<T, ServiceBindingError> {
        #[cfg(target_arch = "wasm32")]
        {
            self.post_json_internal_wasm(url, body, api_key).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (url, body, api_key);
            Err(ServiceBindingError::NotImplemented)
        }
    }

    /// WASM implementation: POST JSON with internal API key
    #[cfg(target_arch = "wasm32")]
    async fn post_json_internal_wasm<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        api_key: &str,
    ) -> Result<T, ServiceBindingError> {
        let normalized_url = self.normalize_url(url.as_ref());

        let mut init = worker::RequestInit::new();
        init.method = worker::Method::Post;

        let body_json = serde_json::to_string(body)?;
        init.body = Some(body_json.into());

        let headers = worker::Headers::new();
        headers.set("Content-Type", "application/json")?;
        headers.set("X-Internal-Key", api_key)?;
        init.headers = headers;

        let request = worker::Request::new_with_init(&normalized_url, &init)?;
        let mut response = self.fetcher.fetch_request(request).await?;

        let status = response.status_code();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ServiceBindingError::HttpError { status, body });
        }

        let data = response.json().await?;
        Ok(data)
    }

    /// WASM implementation: POST JSON and deserialize response
    #[cfg(target_arch = "wasm32")]
    async fn post_json_wasm<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        url: impl AsRef<str>,
        body: &B,
        auth_token: Option<&str>,
    ) -> Result<T, ServiceBindingError> {
        let normalized_url = self.normalize_url(url.as_ref());

        // Create request with JSON body
        let mut init = worker::RequestInit::new();
        init.method = worker::Method::Post;

        let body_json = serde_json::to_string(body)?;
        init.body = Some(body_json.into());

        let headers = worker::Headers::new();
        headers.set("Content-Type", "application/json")?;
        if let Some(token) = auth_token {
            headers.set("Authorization", &format!("Bearer {token}"))?;
        }
        init.headers = headers;

        let request = worker::Request::new_with_init(&normalized_url, &init)?;
        let mut response = self.fetcher.fetch_request(request).await?;

        // Check status code (accept any 2xx)
        let status = response.status_code();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ServiceBindingError::HttpError { status, body });
        }

        // Deserialize JSON directly from worker::Response
        let data = response.json().await?;
        Ok(data)
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
