use crate::{HttpError, HttpMethod, ResponseDetails};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use tracing::debug;

pub(crate) async fn make_request<T: Serialize, R: DeserializeOwned>(
    client: &reqwest::Client,
    default_headers: &HashMap<String, String>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<R, HttpError> {
    let mut builder = client
        .request(method.to_reqwest(), url)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json");

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

pub(crate) async fn make_request_with_details<T: Serialize>(
    client: &reqwest::Client,
    default_headers: &HashMap<String, String>,
    method: HttpMethod,
    url: &str,
    body: Option<&T>,
) -> Result<ResponseDetails, HttpError> {
    let mut builder = client
        .request(method.to_reqwest(), url)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json");

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

    // Extract headers
    let mut headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(value_str) = value.to_str() {
            headers.insert(key.as_str().to_string(), value_str.to_string());
        }
    }

    let body = response.text().await?;
    debug!("Got response from API: status {}", status_code);

    Ok(ResponseDetails {
        status_code,
        body,
        headers,
    })
}
