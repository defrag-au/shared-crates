//! Sending email from a Cloudflare Worker.
//!
//! Provider-agnostic behind [`EmailProvider`]; [`ResendProvider`] is the one
//! implemented. Moved from cnft.dev-workers' `services/email` (which had no
//! consumers left) when montager's magic-link sign-in needed it, so it lives
//! where more than one repository can use it.

mod provider;
mod resend;
mod types;

pub use provider::EmailProvider;
pub use resend::ResendProvider;
pub use types::{Email, EmailError, EmailResponse};
