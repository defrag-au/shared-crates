//! fal.ai image generation/editing client.
//!
//! Platform-agnostic — uses `http-client` which selects reqwest (native) or
//! gloo-net (WASM) automatically. Targets fal's synchronous endpoint
//! (`https://fal.run/{model_id}`) and requests `sync_mode` so results come back
//! inline as data URIs (no second download).
//!
//! Primary use here is mask-based inpainting (Qwen / Flux Fill): the mask
//! defines exactly which pixels change, so the masked region *is* the extracted
//! layer's alpha.

mod base64;

use http_client::HttpClient;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const BASE_URL: &str = "https://fal.run";

/// `fal-ai/qwen-image-edit/inpaint` — Apache-licensed, ControlNet-capable.
pub const QWEN_INPAINT: &str = "fal-ai/qwen-image-edit/inpaint";
/// `fal-ai/flux-pro/v1/fill` — true composite-back masked inpaint.
pub const FLUX_FILL: &str = "fal-ai/flux-pro/v1/fill";
/// `fal-ai/qwen-image-edit` — full-frame instruction edit (no mask).
pub const QWEN_EDIT: &str = "fal-ai/qwen-image-edit";
/// `fal-ai/fast-sdxl-controlnet-canny/inpainting` — structure-locked colorize:
/// canny control keeps the linework in place, mask scopes the region.
pub const SDXL_CONTROLNET_CANNY_INPAINT: &str = "fal-ai/fast-sdxl-controlnet-canny/inpainting";

#[derive(Debug, thiserror::Error)]
pub enum FalError {
    #[error("HTTP error: {0}")]
    Http(#[from] http_client::HttpError),
    #[error("fal returned no images")]
    NoImages,
    #[error("{0}")]
    Other(String),
}

/// fal.ai client. Auth header is `Authorization: Key <FAL_API_KEY>`.
pub struct FalClient {
    client: HttpClient,
}

impl FalClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: HttpClient::new().with_header("Authorization", &format!("Key {api_key}")),
        }
    }

    /// Run any fal model synchronously: `POST https://fal.run/{model_id}`.
    /// Escape hatch for models without a typed wrapper here.
    pub async fn run<I: Serialize, O: DeserializeOwned>(
        &self,
        model_id: &str,
        input: &I,
    ) -> Result<O, FalError> {
        let url = format!("{BASE_URL}/{model_id}");
        Ok(self.client.post(&url, input).await?)
    }

    /// Qwen image-edit inpaint: paint `prompt` into the white region of `mask`.
    pub async fn qwen_inpaint(&self, req: &InpaintRequest<'_>) -> Result<ImageOutput, FalError> {
        let body = QwenInpaintInput {
            prompt: req.prompt,
            image_url: req.image,
            mask_url: req.mask,
            negative_prompt: req.negative_prompt,
            num_images: 1,
            output_format: "png",
            sync_mode: true,
            strength: req.strength,
            seed: req.seed,
        };
        self.finish(self.run(QWEN_INPAINT, &body).await?)
    }

    /// Flux Fill (pro): mask-based fill where outside-mask pixels are composited
    /// back unchanged — the byte-frozen registration path.
    pub async fn flux_fill(&self, req: &InpaintRequest<'_>) -> Result<ImageOutput, FalError> {
        let body = FluxFillInput {
            prompt: req.prompt,
            image_url: req.image,
            mask_url: req.mask,
            num_images: 1,
            output_format: "png",
            sync_mode: true,
            seed: req.seed,
        };
        self.finish(self.run(FLUX_FILL, &body).await?)
    }

    /// Qwen image-edit with no mask: a full-frame instruction edit driven by
    /// `prompt` — e.g. colorizing monotone linework while preserving shape.
    /// The result is opaque; reapply the source alpha afterward to re-register.
    pub async fn qwen_edit(&self, req: &EditRequest<'_>) -> Result<ImageOutput, FalError> {
        let body = QwenEditInput {
            prompt: req.prompt,
            image_url: req.image,
            negative_prompt: req.negative_prompt,
            num_images: 1,
            output_format: "png",
            sync_mode: true,
            guidance_scale: req.guidance_scale,
            seed: req.seed,
        };
        self.finish(self.run(QWEN_EDIT, &body).await?)
    }

    /// Structure-locked colorize via SDXL canny ControlNet inpaint. The canny
    /// edges of `control_image` (clean linework) pin the composition in place,
    /// `mask` scopes the colorized region, and `strength` controls how far the
    /// init image is reworked. Registration is preserved, so the source alpha
    /// stays valid.
    pub async fn controlnet_lineart(
        &self,
        req: &LineartColorizeRequest<'_>,
    ) -> Result<ImageOutput, FalError> {
        let body = SdxlControlnetInpaintInput {
            prompt: req.prompt,
            negative_prompt: req.negative_prompt,
            image_url: req.image,
            control_image_url: req.control_image,
            mask_url: req.mask,
            strength: req.strength,
            controlnet_conditioning_scale: req.controlnet_conditioning_scale,
            num_images: 1,
            format: "png",
            sync_mode: true,
            // Off by default: the checker false-positives on plain garment art.
            enable_safety_checker: false,
            seed: req.seed,
        };
        self.finish(self.run(SDXL_CONTROLNET_CANNY_INPAINT, &body).await?)
    }

    fn finish(&self, out: ImageOutput) -> Result<ImageOutput, FalError> {
        if out.images.is_empty() {
            return Err(FalError::NoImages);
        }
        Ok(out)
    }
}

/// Shared parameters for the masked-inpaint wrappers. `image` and `mask` are
/// data URIs (see [`png_data_uri`]); the mask's white pixels are the edit region.
pub struct InpaintRequest<'a> {
    pub prompt: &'a str,
    pub negative_prompt: &'a str,
    pub image: &'a str,
    pub mask: &'a str,
    pub seed: Option<u64>,
    /// Qwen denoise strength in the masked region (fal default 0.93).
    pub strength: f32,
}

impl Default for InpaintRequest<'_> {
    fn default() -> Self {
        Self {
            prompt: "",
            negative_prompt: " ",
            image: "",
            mask: "",
            seed: None,
            strength: 0.93,
        }
    }
}

/// Parameters for a no-mask full-frame edit ([`FalClient::qwen_edit`]).
/// `image` is a data URI (see [`png_data_uri`]).
pub struct EditRequest<'a> {
    pub prompt: &'a str,
    pub negative_prompt: &'a str,
    pub image: &'a str,
    pub seed: Option<u64>,
    /// CFG scale — prompt adherence (fal default 4.0).
    pub guidance_scale: f32,
}

impl Default for EditRequest<'_> {
    fn default() -> Self {
        Self {
            prompt: "",
            negative_prompt: " ",
            image: "",
            seed: None,
            guidance_scale: 4.0,
        }
    }
}

/// Parameters for structure-locked colorize ([`FalClient::controlnet_lineart`]).
/// `image`/`control_image`/`mask` are data URIs (see [`png_data_uri`]); typically
/// `image` and `control_image` are the same flattened linework, and `mask` is the
/// source asset's alpha silhouette (white = colorize).
pub struct LineartColorizeRequest<'a> {
    pub prompt: &'a str,
    pub negative_prompt: &'a str,
    pub image: &'a str,
    pub control_image: &'a str,
    pub mask: &'a str,
    /// Resemblance to the init image (fal default 0.95; higher = more recolour).
    pub strength: f32,
    /// How strongly the canny edges pin structure (fal default 0.5; raise to lock harder).
    pub controlnet_conditioning_scale: f32,
    pub seed: Option<u64>,
}

impl Default for LineartColorizeRequest<'_> {
    fn default() -> Self {
        Self {
            prompt: "",
            negative_prompt: " ",
            image: "",
            control_image: "",
            mask: "",
            strength: 0.95,
            controlnet_conditioning_scale: 0.8,
            seed: None,
        }
    }
}

/// fal image result. With `sync_mode = true`, `images[].url` is a data URI.
#[derive(Debug, Deserialize)]
pub struct ImageOutput {
    pub images: Vec<FalImage>,
    #[serde(default)]
    pub seed: Option<u64>,
}

impl ImageOutput {
    /// Decode the first image to raw bytes (expects a `sync_mode` data URI).
    pub fn first_bytes(&self) -> Result<Vec<u8>, FalError> {
        let img = self.images.first().ok_or(FalError::NoImages)?;
        decode_data_uri(&img.url)
    }
}

#[derive(Debug, Deserialize)]
pub struct FalImage {
    /// Data URI when `sync_mode` is set, otherwise a hosted fal.media URL.
    pub url: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Wrap PNG bytes as a `data:image/png;base64,...` URI for fal `image_url`/`mask_url`.
pub fn png_data_uri(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::encode(bytes))
}

/// Decode a `data:...;base64,...` URI to raw bytes.
pub fn decode_data_uri(uri: &str) -> Result<Vec<u8>, FalError> {
    let b64 = uri
        .split_once("base64,")
        .map(|(_, rest)| rest)
        .ok_or_else(|| FalError::Other("expected a base64 data URI (is sync_mode set?)".into()))?;
    base64::decode(b64).ok_or_else(|| FalError::Other("invalid base64 in data URI".into()))
}

// ─── Request body types ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct QwenInpaintInput<'a> {
    prompt: &'a str,
    image_url: &'a str,
    mask_url: &'a str,
    negative_prompt: &'a str,
    num_images: u32,
    output_format: &'a str,
    sync_mode: bool,
    strength: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

#[derive(Serialize)]
struct QwenEditInput<'a> {
    prompt: &'a str,
    image_url: &'a str,
    negative_prompt: &'a str,
    num_images: u32,
    output_format: &'a str,
    sync_mode: bool,
    guidance_scale: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

#[derive(Serialize)]
struct SdxlControlnetInpaintInput<'a> {
    prompt: &'a str,
    negative_prompt: &'a str,
    image_url: &'a str,
    control_image_url: &'a str,
    mask_url: &'a str,
    strength: f32,
    controlnet_conditioning_scale: f32,
    num_images: u32,
    // Note: this endpoint uses `format`, not `output_format`.
    format: &'a str,
    sync_mode: bool,
    enable_safety_checker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

#[derive(Serialize)]
struct FluxFillInput<'a> {
    prompt: &'a str,
    image_url: &'a str,
    mask_url: &'a str,
    num_images: u32,
    output_format: &'a str,
    sync_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrips_all_lengths() {
        for n in 0..32usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 + 1) as u8).collect();
            let enc = base64::encode(&data);
            assert_eq!(base64::decode(&enc).as_deref(), Some(data.as_slice()), "n={n}");
        }
    }

    #[test]
    fn data_uri_roundtrips() {
        let uri = png_data_uri(&[0, 1, 2, 253, 254, 255]);
        assert!(uri.starts_with("data:image/png;base64,"));
        assert_eq!(decode_data_uri(&uri).unwrap(), vec![0, 1, 2, 253, 254, 255]);
    }

    #[test]
    fn decode_rejects_non_data_uri() {
        assert!(decode_data_uri("https://fal.media/files/abc.png").is_err());
    }
}
