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

pub(crate) async fn make_request_with_details<T: Serialize, R: DeserializeOwned>(
    default_headers: &HashMap<String, String>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<ResponseDetails<R>, HttpError> {
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

    // Extract headers from gloo-net Response
    let mut headers = HashMap::new();
    let response_headers = response.headers();

    // Common headers to extract (especially OpenAI rate limit headers)
    let header_names = vec![
        "retry-after",
        "x-ratelimit-limit",
        "x-ratelimit-remaining",
        "x-ratelimit-reset",
        "x-ratelimit-limit-requests",
        "x-ratelimit-remaining-requests",
        "x-ratelimit-reset-requests",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-remaining-tokens",
        "x-ratelimit-reset-tokens",
        "content-type",
        "date",
    ];

    for header_name in header_names {
        if let Some(value) = response_headers.get(header_name) {
            headers.insert(header_name.to_string(), value);
        }
    }

    // Check status before parsing
    if !response.ok() {
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
    default_headers: &HashMap<String, String>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<ResponseDetails<String>, HttpError> {
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

    // Extract headers from gloo-net Response (same as make_request_with_details)
    let mut headers = HashMap::new();
    let response_headers = response.headers();

    let header_names = vec![
        "retry-after",
        "x-ratelimit-limit",
        "x-ratelimit-remaining",
        "x-ratelimit-reset",
        "x-ratelimit-limit-requests",
        "x-ratelimit-remaining-requests",
        "x-ratelimit-reset-requests",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-remaining-tokens",
        "x-ratelimit-reset-tokens",
        "content-type",
        "date",
    ];

    for header_name in header_names {
        if let Some(value) = response_headers.get(header_name) {
            headers.insert(header_name.to_string(), value);
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
