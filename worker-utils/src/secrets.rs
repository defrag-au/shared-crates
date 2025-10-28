use worker_stack::worker::{Env, Error, RouteContext};

/// Get a secret by name, trying secrets store first, then env.secret as fallback
pub async fn get_secret(env: &Env, name: &str) -> Result<String, Error> {
    tracing::debug!("🔍 Looking up secret: {}", name);

    // Try secrets store first
    match env.secret_store(name) {
        Ok(secret_store) => {
            tracing::debug!("📦 Trying secrets store for: {}", name);
            if let Ok(Some(value)) = secret_store.get().await {
                tracing::debug!("✅ Found secret in secrets store: {}", name);
                return Ok(value);
            } else {
                return Err(Error::RustError(format!(
                    "Secret '{name}' in secret store, but cannot access value"
                )));
            }
        }
        Err(e) => {
            tracing::debug!("❌ No secrets store binding for {} (error: {e:?})", name);
        }
    }

    // Fallback to environment secret
    tracing::debug!("🔄 Falling back to env.secret for: {}", name);
    match env.secret(name) {
        Ok(secret) => {
            tracing::debug!("✅ Found secret in environment: {}", name);
            Ok(secret.to_string())
        }
        Err(e) => {
            tracing::warn!("❌ Secret not found in either location: {}", name);
            Err(Error::RustError(format!(
                "Secret '{name}' not found in secrets store or environment: {e}"
            )))
        }
    }
}

/// Get a secret from RouteContext, trying secrets store first, then env.secret as fallback
pub async fn get_secret_from_context(ctx: &RouteContext<()>, name: &str) -> Result<String, Error> {
    get_secret(&ctx.env, name).await
}
