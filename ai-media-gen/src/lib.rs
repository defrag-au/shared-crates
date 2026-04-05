//! x.ai image generation and editing client.
//!
//! Platform-agnostic — uses `http-client` which selects reqwest (native) or
//! gloo-net (WASM) automatically.

use http_client::HttpClient;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://api.x.ai/v1";

/// Errors from media generation.
#[derive(Debug, thiserror::Error)]
pub enum MediaGenError {
    #[error("HTTP error: {0}")]
    Http(#[from] http_client::HttpError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

/// x.ai image generation client.
pub struct XaiClient {
    client: HttpClient,
}

impl XaiClient {
    /// Create a new client with the given API key.
    pub fn new(api_key: &str) -> Self {
        Self {
            client: HttpClient::with_bearer_token(api_key.to_string()),
        }
    }

    /// Generate an image from a text prompt (no reference image).
    pub async fn generate_image(&self, req: &ImageRequest<'_>) -> Result<ImageResponse, MediaGenError> {
        let body = GenerationBody {
            model: req.model,
            prompt: req.prompt,
            n: req.n,
            response_format: "url",
        };

        let url = format!("{BASE_URL}/images/generations");
        let response: ImageResponse = self.client.post(&url, &body).await?;
        Ok(response)
    }

    /// Edit/stylize an image using a base64-encoded PNG as reference.
    ///
    /// Uses the `/images/edits` endpoint with the image as a data URI.
    pub async fn edit_image_base64(
        &self,
        req: &ImageRequest<'_>,
        png_bytes: &[u8],
    ) -> Result<ImageResponse, MediaGenError> {
        let mut b64 = String::from("data:image/png;base64,");
        base64_encode(png_bytes, &mut b64);

        let body = EditBody {
            model: req.model,
            prompt: req.prompt,
            n: req.n,
            aspect_ratio: "auto",
            resolution: "1k",
            image: ImageInput { url: b64 },
        };

        let url = format!("{BASE_URL}/images/edits");
        let response: ImageResponse = self.client.post(&url, &body).await?;
        Ok(response)
    }

    /// Edit/stylize using a URL reference.
    pub async fn edit_image_url(
        &self,
        req: &ImageRequest<'_>,
        image_url: &str,
    ) -> Result<ImageResponse, MediaGenError> {
        let body = EditBody {
            model: req.model,
            prompt: req.prompt,
            n: req.n,
            aspect_ratio: "auto",
            resolution: "1k",
            image: ImageInput { url: image_url.to_string() },
        };

        let url = format!("{BASE_URL}/images/edits");
        let response: ImageResponse = self.client.post(&url, &body).await?;
        Ok(response)
    }
}

/// Request parameters for image generation.
pub struct ImageRequest<'a> {
    pub prompt: &'a str,
    pub model: &'a str,
    pub aspect_ratio: &'a str,
    pub n: u32,
}

impl<'a> Default for ImageRequest<'a> {
    fn default() -> Self {
        Self {
            prompt: "",
            model: "grok-imagine-image",
            aspect_ratio: "1:1",
            n: 1,
        }
    }
}

/// Response from the x.ai image API.
#[derive(Debug, Deserialize)]
pub struct ImageResponse {
    pub data: Vec<ImageData>,
}

/// A single generated image.
#[derive(Debug, Deserialize)]
pub struct ImageData {
    pub url: Option<String>,
    #[allow(dead_code)]
    pub b64_json: Option<String>,
}

// ─── Request body types ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct GenerationBody<'a> {
    model: &'a str,
    prompt: &'a str,
    n: u32,
    response_format: &'a str,
}

#[derive(Serialize)]
struct EditBody<'a> {
    model: &'a str,
    prompt: &'a str,
    n: u32,
    aspect_ratio: &'a str,
    resolution: &'a str,
    image: ImageInput,
}

#[derive(Serialize)]
struct ImageInput {
    url: String,
}

/// Simple base64 encoder.
fn base64_encode(input: &[u8], output: &mut String) {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut i = 0;
    while i + 2 < input.len() {
        let b0 = input[i] as u32;
        let b1 = input[i + 1] as u32;
        let b2 = input[i + 2] as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        output.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        output.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        output.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        output.push(CHARS[(triple & 0x3F) as usize] as char);
        i += 3;
    }

    let remaining = input.len() - i;
    if remaining == 1 {
        let b0 = input[i] as u32;
        output.push(CHARS[((b0 >> 2) & 0x3F) as usize] as char);
        output.push(CHARS[((b0 << 4) & 0x3F) as usize] as char);
        output.push('=');
        output.push('=');
    } else if remaining == 2 {
        let b0 = input[i] as u32;
        let b1 = input[i + 1] as u32;
        let pair = (b0 << 8) | b1;
        output.push(CHARS[((pair >> 10) & 0x3F) as usize] as char);
        output.push(CHARS[((pair >> 4) & 0x3F) as usize] as char);
        output.push(CHARS[((pair << 2) & 0x3F) as usize] as char);
        output.push('=');
    }
}
