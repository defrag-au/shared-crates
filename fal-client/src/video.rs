//! Video generation — MiniMax H3 Max, fal's post-trained H3 variant.
//!
//! H3 Max is fast enough (a 5s 768p clip in ~3s) that the synchronous
//! `fal.run` wrappers here are usable directly from a request handler. The
//! queue variants ([`FalClient::submit_h3_max_text_to_video`] and friends)
//! exist for when you want to hand the caller a response immediately and take
//! the result on a webhook — see [`crate::queue`].
//!
//! Unlike the image wrappers, video never sets `sync_mode`: the output is a
//! hosted `v3.fal.media` URL, not a base64 data URI. Inlining a multi-megabyte
//! MP4 through JSON is a waste on every path we have, and a CDN URL is what
//! both Discord and R2 want anyway.

use serde::{Deserialize, Serialize};

use crate::{FalClient, FalError, QueueHandle};

/// `minimax/h3-max/text-to-video` — prompt in, clip out.
pub const H3_MAX_TEXT_TO_VIDEO: &str = "minimax/h3-max/text-to-video";
/// `minimax/h3-max/image-to-video` — first frame (and optionally last frame)
/// drive the clip; output follows the input image's aspect ratio.
pub const H3_MAX_IMAGE_TO_VIDEO: &str = "minimax/h3-max/image-to-video";

/// Output resolution. 768P is fal's default and the price point quoted per
/// second; 480P is the cheaper/faster option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VideoResolution {
    #[serde(rename = "480P")]
    P480,
    #[serde(rename = "768P")]
    P768,
}

/// How much rewriting fal does to the prompt before generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptExpansion {
    /// ~1s of expansion (fal default). The right choice for anything on a
    /// user-visible latency budget.
    Balanced,
    /// ~30s of expansion before generation even starts — better adherence for
    /// terse prompts, but it dwarfs H3 Max's own generation time.
    Quality,
}

/// Parameters shared by the H3 Max text-to-video and image-to-video wrappers.
///
/// `aspect_ratio` applies to text-to-video only; `image_url`/`end_image_url`
/// apply to image-to-video only. The wrappers send only the fields their
/// endpoint accepts, so one struct can drive both.
pub struct VideoRequest<'a> {
    /// Up to 7,000 characters.
    pub prompt: &'a str,
    /// Clip length in seconds (fal default 5).
    pub duration: u32,
    pub resolution: VideoResolution,
    /// Text-to-video only. One of `21:9`, `16:9`, `4:3`, `1:1`, `3:4`, `9:16`.
    /// Ignored by the image-to-video wrapper, which follows the input image.
    pub aspect_ratio: &'a str,
    /// Image-to-video: the opening frame, as a URL or a data URI.
    pub image_url: Option<&'a str>,
    /// Image-to-video: an optional closing frame, for a first-to-last keyframe
    /// interpolation.
    pub end_image_url: Option<&'a str>,
    pub prompt_expansion_mode: PromptExpansion,
    pub enable_safety_checker: bool,
    pub seed: Option<u64>,
}

impl Default for VideoRequest<'_> {
    fn default() -> Self {
        Self {
            prompt: "",
            duration: 5,
            resolution: VideoResolution::P768,
            aspect_ratio: "16:9",
            image_url: None,
            end_image_url: None,
            prompt_expansion_mode: PromptExpansion::Balanced,
            enable_safety_checker: true,
            seed: None,
        }
    }
}

/// A generated clip. `video.url` is a hosted `v3.fal.media` MP4.
#[derive(Debug, Clone, Deserialize)]
pub struct VideoOutput {
    pub video: FalFile,
    /// The prompt after expansion; `None` when fal left it unchanged.
    #[serde(default)]
    pub expanded_prompt: Option<String>,
}

/// A file in a fal response.
#[derive(Debug, Clone, Deserialize)]
pub struct FalFile {
    pub url: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub file_size: Option<u64>,
}

impl FalClient {
    /// H3 Max text-to-video, run synchronously on `fal.run`.
    ///
    /// Blocks for the generation (seconds, not minutes). Use
    /// [`Self::submit_h3_max_text_to_video`] when that is too long to hold a
    /// request open.
    pub async fn h3_max_text_to_video(
        &self,
        req: &VideoRequest<'_>,
    ) -> Result<VideoOutput, FalError> {
        self.run(H3_MAX_TEXT_TO_VIDEO, &TextToVideoInput::from(req))
            .await
    }

    /// H3 Max image-to-video, run synchronously on `fal.run`.
    ///
    /// Errors if `req.image_url` is unset — the opening frame is what makes
    /// this endpoint different from text-to-video.
    pub async fn h3_max_image_to_video(
        &self,
        req: &VideoRequest<'_>,
    ) -> Result<VideoOutput, FalError> {
        self.run(H3_MAX_IMAGE_TO_VIDEO, &ImageToVideoInput::try_from(req)?)
            .await
    }

    /// Queue an H3 Max text-to-video generation. Pass `webhook_url` to have
    /// fal POST a [`crate::WebhookPayload<VideoOutput>`] on completion,
    /// otherwise poll the returned handle.
    pub async fn submit_h3_max_text_to_video(
        &self,
        req: &VideoRequest<'_>,
        webhook_url: Option<&str>,
    ) -> Result<QueueHandle, FalError> {
        let input = TextToVideoInput::from(req);
        match webhook_url {
            Some(url) => {
                self.submit_with_webhook(H3_MAX_TEXT_TO_VIDEO, &input, url)
                    .await
            }
            None => self.submit(H3_MAX_TEXT_TO_VIDEO, &input).await,
        }
    }

    /// Queue an H3 Max image-to-video generation. See
    /// [`Self::submit_h3_max_text_to_video`] for the webhook contract.
    pub async fn submit_h3_max_image_to_video(
        &self,
        req: &VideoRequest<'_>,
        webhook_url: Option<&str>,
    ) -> Result<QueueHandle, FalError> {
        let input = ImageToVideoInput::try_from(req)?;
        match webhook_url {
            Some(url) => {
                self.submit_with_webhook(H3_MAX_IMAGE_TO_VIDEO, &input, url)
                    .await
            }
            None => self.submit(H3_MAX_IMAGE_TO_VIDEO, &input).await,
        }
    }
}

// ─── Request body types ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct TextToVideoInput<'a> {
    prompt: &'a str,
    duration: u32,
    resolution: VideoResolution,
    aspect_ratio: &'a str,
    prompt_expansion_mode: PromptExpansion,
    enable_safety_checker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

impl<'a> From<&VideoRequest<'a>> for TextToVideoInput<'a> {
    fn from(req: &VideoRequest<'a>) -> Self {
        Self {
            prompt: req.prompt,
            duration: req.duration,
            resolution: req.resolution,
            aspect_ratio: req.aspect_ratio,
            prompt_expansion_mode: req.prompt_expansion_mode,
            enable_safety_checker: req.enable_safety_checker,
            seed: req.seed,
        }
    }
}

#[derive(Serialize)]
struct ImageToVideoInput<'a> {
    prompt: &'a str,
    image_url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_image_url: Option<&'a str>,
    duration: u32,
    resolution: VideoResolution,
    prompt_expansion_mode: PromptExpansion,
    enable_safety_checker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

impl<'a> TryFrom<&VideoRequest<'a>> for ImageToVideoInput<'a> {
    type Error = FalError;

    fn try_from(req: &VideoRequest<'a>) -> Result<Self, FalError> {
        let image_url = req.image_url.ok_or_else(|| {
            FalError::Other("image-to-video requires VideoRequest::image_url".into())
        })?;
        Ok(Self {
            prompt: req.prompt,
            image_url,
            end_image_url: req.end_image_url,
            duration: req.duration,
            resolution: req.resolution,
            prompt_expansion_mode: req.prompt_expansion_mode,
            enable_safety_checker: req.enable_safety_checker,
            seed: req.seed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_to_video_body_omits_image_fields() {
        let req = VideoRequest {
            prompt: "a chrome alien ship over Area 51",
            aspect_ratio: "9:16",
            image_url: Some("https://example.test/frame.png"),
            ..Default::default()
        };
        let body = serde_json::to_value(TextToVideoInput::from(&req)).unwrap();

        assert_eq!(body["aspect_ratio"], "9:16");
        assert_eq!(body["resolution"], "768P");
        assert_eq!(body["duration"], 5);
        assert_eq!(body["prompt_expansion_mode"], "balanced");
        assert_eq!(body["enable_safety_checker"], true);
        // image_url belongs to the other endpoint, even when the caller set it.
        assert!(body.get("image_url").is_none());
        // Unset seed must be absent, not null — fal treats null as invalid.
        assert!(body.get("seed").is_none());
    }

    #[test]
    fn image_to_video_body_omits_aspect_ratio() {
        let req = VideoRequest {
            prompt: "the ship banks left",
            image_url: Some("https://example.test/frame.png"),
            end_image_url: Some("https://example.test/last.png"),
            resolution: VideoResolution::P480,
            duration: 10,
            prompt_expansion_mode: PromptExpansion::Quality,
            seed: Some(42),
            ..Default::default()
        };
        let body = serde_json::to_value(ImageToVideoInput::try_from(&req).unwrap()).unwrap();

        assert_eq!(body["image_url"], "https://example.test/frame.png");
        assert_eq!(body["end_image_url"], "https://example.test/last.png");
        assert_eq!(body["resolution"], "480P");
        assert_eq!(body["duration"], 10);
        assert_eq!(body["prompt_expansion_mode"], "quality");
        assert_eq!(body["seed"], 42);
        // The endpoint derives framing from the input image.
        assert!(body.get("aspect_ratio").is_none());
    }

    #[test]
    fn image_to_video_requires_a_first_frame() {
        let req = VideoRequest {
            prompt: "no frame supplied",
            ..Default::default()
        };
        assert!(ImageToVideoInput::try_from(&req).is_err());
    }

    #[test]
    fn parses_video_output() {
        let body = r#"{
            "video": {
                "url": "https://v3.fal.media/files/x/out.mp4",
                "content_type": "video/mp4",
                "file_name": "out.mp4",
                "file_size": 1234567
            },
            "expanded_prompt": null,
            "timings": {"inference": 2.7}
        }"#;
        let out: VideoOutput = serde_json::from_str(body).unwrap();
        assert_eq!(out.video.file_size, Some(1_234_567));
        assert_eq!(out.expanded_prompt, None);
    }
}
