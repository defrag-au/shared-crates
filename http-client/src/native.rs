use crate::{HttpError, HttpMethod, ResponseDetails};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;

/// A request builder for `url`, bounded by the client's timeout if it has one.
///
/// reqwest's per-request timeout runs from connect until the body is fully
/// read, which is the same span the wasm side bounds.
fn request_builder(
    client: &reqwest::Client,
    timeout: Option<Duration>,
    method: HttpMethod,
    url: &str,
) -> reqwest::RequestBuilder {
    let builder = client
        .request(method.to_reqwest(), url)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json");
    match timeout {
        Some(timeout) => builder.timeout(timeout),
        None => builder,
    }
}

/// Report reqwest's timeout as [`HttpError::Timeout`], so callers see the same
/// error on both targets.
pub(crate) fn name_timeout(error: HttpError, timeout: Option<Duration>) -> HttpError {
    match (error, timeout) {
        (HttpError::Reqwest(e), Some(after)) if e.is_timeout() => HttpError::Timeout { after },
        (error, _) => error,
    }
}

pub(crate) async fn make_request<T: Serialize, R: DeserializeOwned>(
    client: &reqwest::Client,
    default_headers: &HashMap<String, String>,
    timeout: Option<Duration>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<R, HttpError> {
    let mut builder = request_builder(client, timeout, method, url);

    // Add default headers
    for (key, value) in default_headers {
        builder = builder.header(key, value);
    }

    // Add body if present
    if let Some(body_data) = body {
        builder = builder.json(body_data);
    }

    let response = builder.send().await?.error_for_status()?;
    debug!("Got response from API: {:?}", response.status());

    response.json::<R>().await.map_err(HttpError::from)
}

pub(crate) async fn make_request_with_details<T: Serialize, R: DeserializeOwned>(
    client: &reqwest::Client,
    default_headers: &HashMap<String, String>,
    timeout: Option<Duration>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<ResponseDetails<R>, HttpError> {
    let mut builder = request_builder(client, timeout, method, url);

    // Add default headers
    for (key, value) in default_headers {
        builder = builder.header(key, value);
    }

    // Add body if present
    if let Some(body_data) = body {
        builder = builder.json(body_data);
    }

    let response = builder.send().await?;
    let status_code = response.status().as_u16();

    // Extract headers - native extracts ALL headers
    let mut headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(value_str) = value.to_str() {
            headers.insert(key.as_str().to_string(), value_str.to_string());
        }
    }

    // Check status before parsing body
    if !response.status().is_success() {
        // Get response body as text for error details
        let body = response.text().await?;
        return Err(HttpError::HttpStatus {
            status_code,
            headers,
            body,
        });
    }

    // Parse JSON body
    let data = response.json::<R>().await?;
    debug!("Got response from API: status {status_code}");

    Ok(ResponseDetails {
        data,
        status_code,
        headers,
    })
}

pub(crate) async fn make_request_text_with_details<T: Serialize>(
    client: &reqwest::Client,
    default_headers: &HashMap<String, String>,
    timeout: Option<Duration>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<ResponseDetails<String>, HttpError> {
    let mut builder = request_builder(client, timeout, method, url);

    // Add default headers
    for (key, value) in default_headers {
        builder = builder.header(key, value);
    }

    // Add body if present
    if let Some(body_data) = body {
        builder = builder.json(body_data);
    }

    let response = builder.send().await?;
    let status_code = response.status().as_u16();

    // Extract headers - native extracts ALL headers
    let mut headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(value_str) = value.to_str() {
            headers.insert(key.as_str().to_string(), value_str.to_string());
        }
    }

    // Get raw text body without checking status first (for custom error handling)
    let data = response.text().await?;
    debug!("Got text response from API: status {status_code}");

    Ok(ResponseDetails {
        data,
        status_code,
        headers,
    })
}
