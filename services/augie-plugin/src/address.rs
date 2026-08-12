//! How the host reaches a plugin.
//!
//! Strictly speaking this is host-side configuration rather than wire protocol
//! — a plugin never sees it. It lives here because several crates on the host
//! side need the same vocabulary (config parsing, command dispatch, and
//! component routing all live in different crates that don't depend on each
//! other), and duplicating it three times would be worse than a slightly loose
//! crate boundary.

use serde::{Deserialize, Serialize};

/// A plugin's address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAddress {
    /// Cloudflare service binding — preferred. No public surface, no egress.
    Binding(String),
    /// Plain HTTPS base URL, for services that aren't service-bound.
    Url(String),
}

impl ServiceAddress {
    /// Short label for logs — never includes the URL, which may carry a host
    /// that shouldn't end up in log aggregation.
    pub fn label(&self) -> &str {
        match self {
            ServiceAddress::Binding(binding) => binding,
            ServiceAddress::Url(_) => "url",
        }
    }
}
