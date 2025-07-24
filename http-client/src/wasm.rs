use crate::{HttpError, HttpMethod, ResponseDetails};
use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use tracing::debug;

pub(crate) async fn make_request<T: Serialize, R: DeserializeOwned>(
    default_headers: &HashMap<String, String>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<R, HttpError> {
    // Create request using the appropriate static method
    let mut request = match method {
        HttpMethod::GET => Request::get(url),
        HttpMethod::POST => Request::post(url),
        HttpMethod::PUT => Request::put(url),
        HttpMethod::DELETE => Request::delete(url),
        HttpMethod::PATCH => Request::patch(url),
    };

    // Add default headers
    for (key, value) in default_headers {
        request = request.header(key, value);
    }

    // Set content type for JSON
    request = request.header("Content-Type", "application/json");
    request = request.header("Accept", "application/json");

    // Add body if present and send request
    let response = if let Some(body_data) = body {
        request.json(body_data)?.send().await?
    } else {
        request.send().await?
    };
    debug!("Got response from API: {}", response.status());

    if !response.ok() {
        return Err(HttpError::Custom(format!(
            "HTTP request failed with status: {}",
            response.status()
        )));
    }

    response.json::<R>().await.map_err(HttpError::from)
}

pub(crate) async fn make_request_with_details<T: Serialize>(
    default_headers: &HashMap<String, String>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<ResponseDetails, HttpError> {
    // Create request using the appropriate static method
    let mut request = match method {
        HttpMethod::GET => Request::get(url),
        HttpMethod::POST => Request::post(url),
        HttpMethod::PUT => Request::put(url),
        HttpMethod::DELETE => Request::delete(url),
        HttpMethod::PATCH => Request::patch(url),
    };

    // Add default headers
    for (key, value) in default_headers {
        request = request.header(key, value);
    }

    // Set content type for JSON
    request = request.header("Content-Type", "application/json");
    request = request.header("Accept", "application/json");

    // Add body if present and send request
    let response = if let Some(body_data) = body {
        request.json(body_data)?.send().await?
    } else {
        request.send().await?
    };

    let status_code = response.status();
    
    // Extract headers - gloo-net Response has headers() method
    let mut headers = HashMap::new();
    // Note: gloo-net doesn't expose all headers easily, we'll implement basic support
    if let Some(retry_after) = response.headers().get("retry-after") {
        headers.insert("retry-after".to_string(), retry_after);
    }

    let body = response.text().await?;
    debug!("Got response from API: status {}", status_code);

    Ok(ResponseDetails {
        status_code,
        body,
        headers,
    })
}