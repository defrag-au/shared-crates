use serde::{Deserialize, Serialize};

/// An email to be sent.
#[derive(Debug, Clone, Serialize)]
pub struct Email {
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub html: String,
    /// Plain-text alternative. Mail clients that show no HTML, and spam
    /// filters that score HTML-only mail down, both read this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Email {
    pub fn new(from: impl Into<String>, to: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: vec![to.into()],
            subject: subject.into(),
            html: String::new(),
            text: None,
        }
    }

    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.html = html.into();
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

/// A successful send.
#[derive(Debug, Deserialize)]
pub struct EmailResponse {
    pub id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("provider error: {0}")]
    Provider(String),

    #[error("request failed: {0}")]
    Request(String),

    #[error("missing API key")]
    MissingApiKey,
}

impl From<EmailError> for worker_stack::worker::Error {
    fn from(e: EmailError) -> Self {
        worker_stack::worker::Error::RustError(e.to_string())
    }
}
