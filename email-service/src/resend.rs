use serde::Serialize;
use worker_stack::wasm_bindgen::JsValue;
use worker_stack::worker::{Fetch, Headers, Method, Request, RequestInit};

use crate::provider::EmailProvider;
use crate::types::{Email, EmailError, EmailResponse};

const RESEND_API_URL: &str = "https://api.resend.com/emails";

/// Sends through Resend (<https://resend.com>).
pub struct ResendProvider {
    api_key: String,
}

#[derive(Serialize)]
struct ResendRequest<'a> {
    from: &'a str,
    to: &'a [String],
    subject: &'a str,
    html: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
}

impl ResendProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }

    /// From the `RESEND_API_KEY` secret: the Secrets Store binding first, a
    /// plain secret or `.dev.vars` second (`worker_utils::secrets::get_secret`).
    pub async fn from_env(env: &worker_stack::worker::Env) -> Result<Self, EmailError> {
        let api_key = worker_utils::secrets::get_secret(env, "RESEND_API_KEY")
            .await
            .map_err(|_| EmailError::MissingApiKey)?;
        Ok(Self::new(api_key))
    }
}

impl EmailProvider for ResendProvider {
    async fn send(&self, email: &Email) -> Result<EmailResponse, EmailError> {
        let body = ResendRequest {
            from: &email.from,
            to: &email.to,
            subject: &email.subject,
            html: &email.html,
            text: email.text.as_deref(),
        };
        let json = serde_json::to_string(&body)
            .map_err(|e| EmailError::Request(format!("failed to serialize: {e}")))?;

        let headers = Headers::new();
        headers
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .map_err(|e| EmailError::Request(format!("header error: {e}")))?;
        headers
            .set("Content-Type", "application/json")
            .map_err(|e| EmailError::Request(format!("header error: {e}")))?;

        let request = Request::new_with_init(
            RESEND_API_URL,
            RequestInit::new()
                .with_method(Method::Post)
                .with_headers(headers)
                .with_body(Some(JsValue::from_str(&json))),
        )
        .map_err(|e| EmailError::Request(format!("request build error: {e}")))?;

        let mut response = Fetch::Request(request)
            .send()
            .await
            .map_err(|e| EmailError::Request(format!("fetch error: {e}")))?;

        if response.status_code() != 200 {
            let body = response.text().await.unwrap_or_else(|_| "(no body)".into());
            return Err(EmailError::Provider(format!(
                "resend returned {}: {body}",
                response.status_code()
            )));
        }

        response
            .json()
            .await
            .map_err(|e| EmailError::Request(format!("failed to parse response: {e}")))
    }
}
