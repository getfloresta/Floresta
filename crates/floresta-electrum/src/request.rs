// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
pub struct Request {
    pub id: Value,
    pub method: String,

    /// The Electrum protocol accepts both JSON-RPC 1.0 and 2.0, so this member is optional.
    /// Responses are always sent in the 2.0 form.
    #[serde(default)]
    pub jsonrpc: Option<String>,

    #[serde(default)]
    pub params: Vec<Value>,
}

#[cfg(test)]
mod tests {
    use super::Request;

    #[test]
    fn test_deserialize_without_jsonrpc() {
        let req: Request =
            serde_json::from_str(r#"{"id":1,"method":"blockchain.headers.subscribe","params":[]}"#)
                .unwrap();

        assert_eq!(req.method, "blockchain.headers.subscribe");
        assert_eq!(req.jsonrpc, None);
    }

    #[test]
    fn test_deserialize_with_jsonrpc() {
        let req: Request = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":1,"method":"blockchain.headers.subscribe","params":[]}"#,
        )
        .unwrap();

        assert_eq!(req.jsonrpc.as_deref(), Some("2.0"));
    }
}
