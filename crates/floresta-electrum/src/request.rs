// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
pub struct Request {
    pub id: Value,
    pub method: String,
    pub jsonrpc: String,

    #[serde(default)]
    pub params: Vec<Value>,
}
