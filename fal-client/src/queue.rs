//! fal.ai queue API — submit, poll, cancel, and receive webhooks.
//!
//! The synchronous `fal.run` endpoint used by the image wrappers holds the
//! connection open for the whole generation. That is fine for a few seconds of
//! image work and for H3 Max's sub-3s clips, but not for slower video models,
//! and not from a Worker that wants to hand the request back immediately.
//!
//! The queue path instead returns a [`QueueHandle`] straight away. Either poll
//! [`FalClient::queue_status`] or — better — register a webhook at submit time
//! and parse the callback with [`WebhookPayload`].
//!
//! The handle carries fal's own absolute `status_url` / `response_url`, so we
//! never have to reconstruct them. That matters for three-segment model ids
//! like `minimax/h3-max/text-to-video`, where the app id and the sub-path split
//! is not something a caller can infer.

use std::fmt::Write as _;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{FalClient, FalError};

const QUEUE_BASE_URL: &str = "https://queue.fal.run";

/// Handle to an in-flight queue request, returned by [`FalClient::submit`].
///
/// Worth persisting: it is the only way to recover a result if a webhook is
/// missed, and `request_id` is the correlation key the webhook arrives with.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueHandle {
    /// Correlation id — matches [`WebhookPayload::request_id`].
    pub request_id: String,
    pub status_url: String,
    pub response_url: String,
    pub cancel_url: String,
}

/// Queue position/progress for a request that has not finished yet.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status")]
pub enum QueueStatus {
    #[serde(rename = "IN_QUEUE")]
    InQueue {
        #[serde(default)]
        queue_position: u32,
    },
    #[serde(rename = "IN_PROGRESS")]
    InProgress {},
    #[serde(rename = "COMPLETED")]
    Completed {},
}

impl QueueStatus {
    /// `true` once the result is fetchable via [`FalClient::queue_result`].
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed {})
    }
}

/// Outcome reported by a fal webhook.
///
/// Note these are **not** the queue-status strings: a webhook fires once, at
/// the end, with `OK` or `ERROR`. Matching on `"COMPLETED"` here silently
/// discards every callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum WebhookStatus {
    #[serde(rename = "OK")]
    Ok,
    #[serde(rename = "ERROR")]
    Error,
}

/// Body fal POSTs to the `fal_webhook` URL when a queued request settles.
///
/// `T` is the model's output type — e.g. [`crate::VideoOutput`] or
/// [`crate::ImageOutput`].
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookPayload<T> {
    /// Matches [`QueueHandle::request_id`].
    pub request_id: String,
    /// The last *retried* request id; differs from `request_id` only when fal
    /// retried internally.
    #[serde(default)]
    pub gateway_request_id: Option<String>,
    pub status: WebhookStatus,
    /// The model output on success. `None` on error, and also on success when
    /// the output failed to serialize — see `payload_error`.
    #[serde(default = "Option::default")]
    pub payload: Option<T>,
    /// Error detail; may be a string or an object (e.g.
    /// `{"type": "CONTENT_MODERATION_ERROR"}`).
    #[serde(default)]
    pub error: Option<serde_json::Value>,
    /// Set when the output itself could not be JSON-encoded; fetch the result
    /// from `response_url` instead.
    #[serde(default)]
    pub payload_error: Option<String>,
}

impl<T> WebhookPayload<T> {
    /// Take the output, converting an `ERROR` callback (or an empty payload)
    /// into a [`FalError`].
    pub fn into_result(self) -> Result<T, FalError> {
        match self.payload {
            Some(payload) if self.status == WebhookStatus::Ok => Ok(payload),
            _ => Err(FalError::Other(self.failure_message())),
        }
    }

    /// Human-readable failure reason, for logging.
    pub fn failure_message(&self) -> String {
        if let Some(err) = &self.error {
            return match err {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
        }
        if let Some(err) = &self.payload_error {
            return format!("payload not serializable: {err}");
        }
        format!("fal webhook {:?} with no payload", self.status)
    }
}

#[derive(Deserialize)]
struct CancelResponse {
    #[allow(dead_code)]
    status: String,
}

impl FalClient {
    /// Submit to the async queue: `POST https://queue.fal.run/{model_id}`.
    ///
    /// Returns as soon as the request is accepted — nothing has been generated
    /// yet. Poll [`Self::queue_status`] then [`Self::queue_result`], or prefer
    /// [`Self::submit_with_webhook`].
    pub async fn submit<I: Serialize>(
        &self,
        model_id: &str,
        input: &I,
    ) -> Result<QueueHandle, FalError> {
        let url = format!("{QUEUE_BASE_URL}/{model_id}");
        Ok(self.client.post(&url, input).await?)
    }

    /// Submit to the async queue and have fal POST the result to `webhook_url`
    /// when it settles. Parse that callback with [`WebhookPayload`].
    ///
    /// The webhook is best-effort: fal retries, but a handler that is down for
    /// the whole retry window loses the callback. Keep the [`QueueHandle`] so
    /// a sweep can recover the result from the queue.
    pub async fn submit_with_webhook<I: Serialize>(
        &self,
        model_id: &str,
        input: &I,
        webhook_url: &str,
    ) -> Result<QueueHandle, FalError> {
        let url = format!(
            "{QUEUE_BASE_URL}/{model_id}?fal_webhook={}",
            encode_query_component(webhook_url)
        );
        Ok(self.client.post(&url, input).await?)
    }

    /// Poll a submitted request's progress.
    pub async fn queue_status(&self, handle: &QueueHandle) -> Result<QueueStatus, FalError> {
        Ok(self.client.get(&handle.status_url).await?)
    }

    /// Fetch a completed request's output. Errors if the request has not
    /// finished — check [`QueueStatus::is_completed`] first.
    pub async fn queue_result<O: DeserializeOwned>(
        &self,
        handle: &QueueHandle,
    ) -> Result<O, FalError> {
        Ok(self.client.get(&handle.response_url).await?)
    }

    /// Request cancellation. Only has an effect while the request is still
    /// `IN_QUEUE` — fal will not interrupt one that is already running.
    pub async fn cancel(&self, handle: &QueueHandle) -> Result<(), FalError> {
        let _: CancelResponse = self.client.put(&handle.cancel_url, &()).await?;
        Ok(())
    }
}

/// Percent-encode a string for use as a query-parameter *value*.
///
/// The webhook URL goes in `?fal_webhook=`, so its own `://`, `/` and any
/// query string must not be read as part of the submit URL.
fn encode_query_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VideoOutput;

    #[test]
    fn encodes_webhook_url() {
        assert_eq!(
            encode_query_component("https://x.dev/hook?k=1&v=2"),
            "https%3A%2F%2Fx.dev%2Fhook%3Fk%3D1%26v%3D2"
        );
    }

    #[test]
    fn parses_queue_statuses() {
        let queued: QueueStatus =
            serde_json::from_str(r#"{"status":"IN_QUEUE","queue_position":3}"#).unwrap();
        assert!(matches!(queued, QueueStatus::InQueue { queue_position: 3 }));
        assert!(!queued.is_completed());

        // Extra fields fal adds (response_url, logs, metrics) must not break us.
        let done: QueueStatus = serde_json::from_str(
            r#"{"status":"COMPLETED","response_url":"https://queue.fal.run/x","logs":[]}"#,
        )
        .unwrap();
        assert!(done.is_completed());
    }

    #[test]
    fn parses_ok_webhook() {
        let body = r#"{
            "request_id": "abc",
            "gateway_request_id": "abc",
            "status": "OK",
            "payload": {
                "video": {"url": "https://v3.fal.media/x.mp4", "content_type": "video/mp4"},
                "expanded_prompt": "a longer prompt"
            },
            "error": null
        }"#;
        let hook: WebhookPayload<VideoOutput> = serde_json::from_str(body).unwrap();
        assert_eq!(hook.status, WebhookStatus::Ok);
        let out = hook.into_result().unwrap();
        assert_eq!(out.video.url, "https://v3.fal.media/x.mp4");
    }

    #[test]
    fn error_webhook_becomes_err_with_object_error() {
        let body = r#"{
            "request_id": "abc",
            "status": "ERROR",
            "payload": null,
            "error": {"type": "CONTENT_MODERATION_ERROR"}
        }"#;
        let hook: WebhookPayload<VideoOutput> = serde_json::from_str(body).unwrap();
        assert!(hook.failure_message().contains("CONTENT_MODERATION_ERROR"));
        assert!(hook.into_result().is_err());
    }

    #[test]
    fn queue_handle_round_trips() {
        let body = r#"{
            "request_id": "abc",
            "status_url": "https://queue.fal.run/minimax/h3-max/requests/abc/status",
            "response_url": "https://queue.fal.run/minimax/h3-max/requests/abc",
            "cancel_url": "https://queue.fal.run/minimax/h3-max/requests/abc/cancel"
        }"#;
        let handle: QueueHandle = serde_json::from_str(body).unwrap();
        assert_eq!(handle.request_id, "abc");
        assert!(handle.status_url.ends_with("/status"));
    }
}
