//! Where the socket lives.

/// The stream endpoint, with the API key encoded into the query.
///
/// Version 2 is the array-frame protocol this crate speaks. The socket answers
/// with array frames whether or not it is asked for, so this is the documented
/// form rather than a requirement.
pub fn endpoint_url(api_key: &str) -> String {
    format!(
        "wss://stream-api.opensea.io/socket/websocket?token={}&vsn=2.0.0",
        encode_query_value(api_key)
    )
}

/// Percent-encode one query value, leaving the unreserved set alone.
///
/// An API key is opaque and need not be alphanumeric — base64 keys carry `+`,
/// `/` and `=`, each of which means something else in a query string — so it is
/// encoded rather than trusted.
fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alphanumeric_key_passes_through_unchanged() {
        assert_eq!(
            endpoint_url("0123456789abcdef"),
            "wss://stream-api.opensea.io/socket/websocket?token=0123456789abcdef&vsn=2.0.0"
        );
    }

    #[test]
    fn a_base64_key_has_its_reserved_characters_encoded() {
        assert_eq!(
            endpoint_url("a+b/c="),
            "wss://stream-api.opensea.io/socket/websocket?token=a%2Bb%2Fc%3D&vsn=2.0.0"
        );
    }
}
