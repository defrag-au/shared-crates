//! Typed request/poll over an [`ArcadeBackend`].
//!
//! The backend deals in strings because that is what the JS bridges deliver.
//! This turns a call into a handle that remembers what it is waiting for, so
//! the play loop matches on `SeedGrant` and `SubmitOutcome` rather than
//! parsing JSON in the middle of a frame.

use crate::backend::{ArcadeBackend, Poll, ReqId};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::marker::PhantomData;

/// A request in flight, remembering the response it expects.
pub struct Pending<T> {
    id: ReqId,
    /// Set when the request could not even be sent — reported on the first
    /// poll rather than thrown away, so a serialisation failure surfaces as a
    /// visible error instead of a request that never completes.
    failed: Option<String>,
    _response: PhantomData<T>,
}

/// The result of polling a [`Pending`].
pub enum Resolved<T> {
    Pending,
    Ok(T),
    Err(String),
}

/// POST `body` to `path`, expecting a `T` back.
pub fn post<T, B>(backend: &mut B, path: &str, body: &impl Serialize) -> Pending<T>
where
    B: ArcadeBackend,
{
    match serde_json::to_string(body) {
        Ok(json) => Pending {
            id: backend.post(path, &json),
            failed: None,
            _response: PhantomData,
        },
        Err(e) => Pending {
            id: 0,
            failed: Some(format!("could not encode request: {e}")),
            _response: PhantomData,
        },
    }
}

/// Poll a request. Returns [`Resolved::Pending`] until it completes.
pub fn poll<T, B>(backend: &mut B, request: &Pending<T>) -> Resolved<T>
where
    T: DeserializeOwned,
    B: ArcadeBackend,
{
    if let Some(reason) = &request.failed {
        return Resolved::Err(reason.clone());
    }

    match backend.poll(request.id) {
        Poll::Pending => Resolved::Pending,
        Poll::Err(e) => Resolved::Err(e),
        Poll::Ok(body) => match serde_json::from_str::<T>(&body) {
            Ok(value) => Resolved::Ok(value),
            // Include a clipped body: "missing field `challenge`" alone does
            // not distinguish a protocol change from an HTML error page served
            // by a proxy, and those need opposite fixes.
            Err(e) => Resolved::Err(format!(
                "unexpected reply: {e} — {}",
                body.chars().take(120).collect::<String>()
            )),
        },
    }
}
