//! The JSON-RPC transport.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use http_client::{HttpClient, HttpMethod};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::abi::AbiError;
use crate::address::Address;
use crate::block::BlockTag;
use crate::hex::{HexError, Quantity};
use crate::log::{HexData, Log, LogFilter};

/// The `params` an argument-less method takes — the empty array, not `null`.
///
/// A tuple struct with no fields serialises as `[]`; a unit struct would serialise
/// as `null`, and `"params": null` is not what `eth_chainId` is called with. The
/// difference is invisible until a strict node refuses it, so it has a test.
#[derive(Debug, Clone, Copy, Serialize)]
struct NoParams();

/// The parameters of an `eth_call`.
#[derive(Debug, Serialize)]
struct CallRequest<'a> {
    to: &'a Address,
    data: &'a HexData,
}

/// One JSON-RPC request.
#[derive(Debug, Serialize)]
struct RpcRequest<'a, P> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: P,
}

/// The part of a JSON-RPC response this crate inspects before the body.
///
/// Read separately from the result so the result type does not have to be
/// `Default`: `#[serde(default)]` on a generic field makes serde demand
/// `R: Default`, and defaulting a result is exactly the wrong move — a missing
/// result is an error, not a zero.
#[derive(Debug, Deserialize)]
struct RpcBody {
    #[serde(default)]
    error: Option<RpcFailure>,
}

/// A JSON-RPC response's result. Always present on success — JSON-RPC 2.0 says a
/// response carries either a result or an error — and `null` when the call
/// succeeded with no data.
#[derive(Debug, Deserialize)]
struct RpcResult<R> {
    result: Option<R>,
}

/// A node's own reason for refusing.
///
/// `data` is deliberately not modelled: it is a revert blob on one node, a string on
/// another, and an object on a third, and there is nothing this crate could do with
/// it beyond passing it through. The code and the message are what a caller can act
/// on.
#[derive(Debug, Deserialize)]
struct RpcFailure {
    code: i64,
    message: String,
}

/// Anything that can go wrong reading the chain.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// The request never got a response, or the transport failed.
    #[error(transparent)]
    Http(#[from] http_client::HttpError),
    /// The node answered non-2xx — a gateway or a rate limit, rather than a
    /// JSON-RPC error, which arrives with status 200 and an `error` object.
    #[error("the node answered {status}: {body}")]
    Transport { status: u16, body: String },
    /// A 2xx body that is not the JSON-RPC shape.
    #[error("could not decode the json-rpc response: {0}")]
    Decode(#[from] serde_json::Error),
    /// The node reported an error in its own envelope — `execution reverted` and
    /// the like.
    #[error("json-rpc error {code}: {message}")]
    Rpc { code: i64, message: String },
    /// Neither a result nor an error. Not a shape a node should send.
    #[error("the json-rpc response carried neither a result nor an error")]
    Empty,
    /// The result was well-formed JSON of the wrong shape for the call — an
    /// `ownerOf` that did not return an address.
    #[error(transparent)]
    Abi(#[from] AbiError),
    /// A hex field that did not decode.
    #[error(transparent)]
    Hex(#[from] HexError),
}

/// A JSON-RPC client for one EVM node.
///
/// Cheap to clone: the HTTP client and the request counter are shared.
#[derive(Clone)]
pub struct EvmRpcClient {
    http: HttpClient,
    url: Arc<str>,
    next_id: Arc<AtomicU64>,
}

impl EvmRpcClient {
    /// A client for one endpoint.
    ///
    /// The URL is passed in rather than read from the environment, for the same
    /// reason the OpenSea key is: sourcing a credential or an endpoint is the
    /// caller's job, and this crate having no notion of an environment is what lets
    /// its tests run offline.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(),
            url: Arc::from(url.into()),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Bound every request, body included. Unbounded by default.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.http = self.http.with_timeout(timeout);
        self
    }

    /// A header on every request — a bearer token for a private endpoint.
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.http = self.http.with_header(key, value);
        self
    }

    /// The endpoint this client reads.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// `eth_chainId` — which chain this endpoint is, which is how a reader proves
    /// it is pointed at the chain it thinks it is.
    pub async fn chain_id(&self) -> Result<u64, RpcError> {
        let quantity: Quantity = self.call("eth_chainId", NoParams()).await?;
        Ok(quantity.get())
    }

    /// `eth_chainId` as a chain identity.
    ///
    /// Infallible on purpose: any `u64` is an EIP-155 id, so there is no unknown
    /// case to report — a chain this crate has not named reads as
    /// [`chains::EvmChain::Id`] and survives intact.
    pub async fn chain(&self) -> Result<chains::ChainRef, RpcError> {
        let id = self.chain_id().await?;
        Ok(chains::ChainRef::Evm(chains::EvmChain::from_chain_id(id)))
    }

    /// `eth_blockNumber` — the head, which is the upper bound of a log scan.
    pub async fn block_number(&self) -> Result<u64, RpcError> {
        let quantity: Quantity = self.call("eth_blockNumber", NoParams()).await?;
        Ok(quantity.get())
    }

    /// `eth_call` against one block.
    pub async fn eth_call(
        &self,
        to: &Address,
        data: &HexData,
        block: BlockTag,
    ) -> Result<HexData, RpcError> {
        let request = CallRequest { to, data };
        self.call("eth_call", (request, block)).await
    }

    /// `eth_getLogs`.
    ///
    /// Logs come back in block order from every node this has been used against,
    /// but ordering is not part of the JSON-RPC contract, so a consumer that cares
    /// sorts by `(blockNumber, logIndex)` itself rather than assuming.
    pub async fn get_logs(&self, filter: &LogFilter) -> Result<Vec<Log>, RpcError> {
        self.call("eth_getLogs", (filter,)).await
    }

    /// Any method, with your own parameters and result type.
    ///
    /// The escape hatch, and how a method gets modelled: call it with a candidate
    /// type, confirm the shape against a live answer, then move it into a typed
    /// method with a test.
    pub async fn call<P, R>(&self, method: &str, params: P) -> Result<R, RpcError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = RpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };

        let response = self
            .http
            .request_text_with_details(HttpMethod::POST, &self.url, Some(&request))
            .await?;

        if !(200..300).contains(&response.status_code) {
            return Err(RpcError::Transport {
                status: response.status_code,
                body: truncate(&response.data),
            });
        }

        parse_response(&response.data)
    }
}

/// Reads a response body, preferring the node's error over its result.
fn parse_response<R: DeserializeOwned>(body: &str) -> Result<R, RpcError> {
    let envelope: RpcBody = serde_json::from_str(body)?;
    if let Some(failure) = envelope.error {
        return Err(RpcError::Rpc {
            code: failure.code,
            message: failure.message,
        });
    }

    let carrier: RpcResult<R> = serde_json::from_str(body)?;
    carrier.result.ok_or(RpcError::Empty)
}

/// Bounds a body kept for an error message, so a proxy's HTML page cannot become
/// the error.
fn truncate(body: &str) -> String {
    const LIMIT: usize = 400;
    if body.chars().count() <= LIMIT {
        return body.to_owned();
    }
    body.chars().take(LIMIT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_params_is_the_empty_array_not_null() {
        assert_eq!(serde_json::to_string(&NoParams()).unwrap(), "[]");
    }

    #[test]
    fn a_request_is_the_json_rpc_shape() {
        let request = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "eth_getLogs",
            params: NoParams(),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"jsonrpc":"2.0","id":1,"method":"eth_getLogs","params":[]}"#
        );
    }

    #[test]
    fn a_call_puts_its_request_and_block_in_a_positional_array() {
        let to = Address::parse("0x7980aa64093853cb78c927e05b88fed96e945f81").unwrap();
        let data = HexData::new("0x6352211e").unwrap();
        let params = (
            CallRequest {
                to: &to,
                data: &data,
            },
            BlockTag::Latest,
        );
        assert_eq!(
            serde_json::to_string(&params).unwrap(),
            format!(r#"[{{"to":"{to}","data":"0x6352211e"}},"latest"]"#)
        );
    }

    #[test]
    fn a_result_is_read_from_the_envelope() {
        let value: Vec<Log> = parse_response(r#"{"jsonrpc":"2.0","id":1,"result":[]}"#).unwrap();
        assert!(value.is_empty());
    }

    #[test]
    fn an_error_wins_over_a_null_result() {
        // What a node sends for a revert: `result: null` beside the error.
        let error = parse_response::<HexData>(
            r#"{"jsonrpc":"2.0","id":1,"result":null,"error":{"code":3,"message":"execution reverted"}}"#,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RpcError::Rpc { code: 3, ref message } if message == "execution reverted"
        ));
    }

    #[test]
    fn a_null_result_with_no_error_says_there_was_no_result() {
        assert!(matches!(
            parse_response::<HexData>(r#"{"jsonrpc":"2.0","id":1,"result":null}"#).unwrap_err(),
            RpcError::Empty
        ));
    }

    #[test]
    fn a_long_error_body_is_truncated() {
        let body = "x".repeat(1000);
        assert_eq!(truncate(&body).chars().count(), 400);
    }
}
