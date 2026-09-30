use crate::types::{Email, EmailError, EmailResponse};

/// An email delivery provider.
pub trait EmailProvider {
    /// Send an email, returning the provider's message id.
    fn send(
        &self,
        email: &Email,
    ) -> impl std::future::Future<Output = Result<EmailResponse, EmailError>>;
}
