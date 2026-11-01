//! Running the flow's effects: auth-service over HTTP, the wallet over its
//! miniquad plugin. [`LinkIo`] is the seam, so a demo or a test runs the same
//! screen with no network and no wallet.

use std::task::Poll;

use chains::ChainFamily;
use macroquad_app::{Method, Pending, Request};
use wallet_link_flow::{Answer, ApiCall, CallKind, Event, Reply, SignRequest, Signature};
use wallet_miniquad::{DataSignature, FailureKind, PollResultDetailed, ReqId};

/// Where the flow's calls and signatures go. Every request is fire and poll:
/// [`LinkIo::poll`] is asked once a frame for whatever has finished.
pub trait LinkIo {
    /// Make `call` to auth-service.
    fn call(&mut self, call: &ApiCall);
    /// Ask the connected wallet to sign.
    fn sign(&mut self, request: &SignRequest);
    /// What finished since the last poll, as events for the flow.
    fn poll(&mut self) -> Vec<Event>;
}

/// auth-service over HTTP with a bearer token, and the browser's wallets.
pub struct LiveIo {
    base: String,
    token: String,
    calls: Vec<(CallKind, Pending)>,
    signing: Option<(ChainFamily, ReqId)>,
}

impl LiveIo {
    /// Calls go to `base` (`https://auth.hodlcroft.com`) with `token` as the
    /// bearer.
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            token: token.into(),
            calls: Vec::new(),
            signing: None,
        }
    }
}

impl LinkIo for LiveIo {
    fn call(&mut self, call: &ApiCall) {
        let method = match call.method() {
            "GET" => Method::Get,
            _ => Method::Post,
        };
        let url = format!("{}{}", self.base, call.path());
        let mut request = Request::new(method, &url).bearer(&self.token);
        if let Some(body) = call.body() {
            request = request.body(body.into_bytes(), "application/json");
        }
        self.calls.push((call.kind(), request.send()));
    }

    fn sign(&mut self, request: &SignRequest) {
        self.signing = Some(match request {
            SignRequest::Cardano {
                address_hex,
                payload_hex,
            } => (
                ChainFamily::Cardano,
                wallet_miniquad::sign_data(address_hex, payload_hex),
            ),
            SignRequest::Evm {
                address,
                message_hex,
            } => (
                ChainFamily::Evm,
                wallet_miniquad::evm::personal_sign(message_hex, address),
            ),
            // The message goes over as text: the wallet does the
            // `midnight_signed_message:` prefixing and the hashing itself.
            SignRequest::Midnight { address, message } => (
                ChainFamily::Midnight,
                wallet_miniquad::midnight::sign_message(address, message),
            ),
        });
    }

    fn poll(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        self.calls
            .retain_mut(|(kind, pending)| match pending.poll() {
                Poll::Pending => true,
                Poll::Ready(reply) => {
                    let reply = match reply {
                        Ok(r) => Reply::from_status(r.status, r.text()),
                        Err(e) => Reply::Offline(e.0),
                    };
                    out.push(Event::Answer(Answer { call: *kind, reply }));
                    false
                }
            });
        if let Some((family, req)) = self.signing {
            let polled = match family {
                ChainFamily::Evm => wallet_miniquad::evm::poll(req),
                ChainFamily::Midnight => wallet_miniquad::midnight::poll(req),
                _ => wallet_miniquad::poll_detailed(req),
            };
            if let Some(event) = signature_event(family, polled) {
                self.signing = None;
                out.push(event);
            }
        }
        out
    }
}

/// A wallet's answer to a signature request, as the flow's event. `None`
/// while it is still pending.
pub fn signature_event(family: ChainFamily, polled: PollResultDetailed) -> Option<Event> {
    Some(match polled {
        PollResultDetailed::Pending => return None,
        PollResultDetailed::Ok { data } => match family {
            ChainFamily::Evm => Event::Signed(Signature::Evm { signature: data }),
            ChainFamily::Midnight => match wallet_miniquad::midnight::parse_signature(&data) {
                Some(s) => Event::Signed(Signature::Midnight {
                    signature: s.signature,
                    key: s.verifying_key,
                    scheme: s.scheme,
                }),
                None => Event::SignFailed("The wallet's signature couldn't be read.".into()),
            },
            _ => match serde_json::from_str::<DataSignature>(&data) {
                Ok(s) => Event::Signed(Signature::Cardano {
                    signature: s.signature,
                    key: s.key,
                }),
                Err(_) => Event::SignFailed("The wallet's signature couldn't be read.".into()),
            },
        },
        PollResultDetailed::Err {
            kind: FailureKind::Declined,
            ..
        } => Event::SignRefused,
        PollResultDetailed::Err { data, .. } => Event::SignFailed(data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cip30_signature_reads_into_its_two_halves() {
        let event = signature_event(
            ChainFamily::Cardano,
            PollResultDetailed::Ok {
                data: r#"{"signature":"84a4","key":"a401"}"#.into(),
            },
        );
        assert_eq!(
            event,
            Some(Event::Signed(Signature::Cardano {
                signature: "84a4".into(),
                key: "a401".into()
            }))
        );
    }

    #[test]
    fn a_midnight_signature_reads_into_its_key_and_scheme() {
        let event = signature_event(
            ChainFamily::Midnight,
            PollResultDetailed::Ok {
                data: r#"{"signature":"ef","verifyingKey":"12","scheme":null}"#.into(),
            },
        );
        assert_eq!(
            event,
            Some(Event::Signed(Signature::Midnight {
                signature: "ef".into(),
                key: "12".into(),
                scheme: None,
            }))
        );
    }

    #[test]
    fn a_refusal_is_not_a_failure() {
        assert_eq!(
            signature_event(
                ChainFamily::Evm,
                PollResultDetailed::Err {
                    data: "User rejected".into(),
                    kind: FailureKind::Declined
                }
            ),
            Some(Event::SignRefused)
        );
        assert_eq!(
            signature_event(ChainFamily::Evm, PollResultDetailed::Pending),
            None
        );
    }
}
