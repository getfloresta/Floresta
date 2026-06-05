// SPDX-License-Identifier: MIT OR Apache-2.0

//! This module defines the structure for JSON-RPC requests and provides utility functions to
//! extract parameters from the request.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Represents a JSON-RPC request (versions 1.0 and 2.0).
pub struct RpcRequest {
    /// The JSON-RPC version, typically "2.0".
    ///
    /// For JSON-RPC 2.0, this field is required. For earlier versions, it may be omitted.
    ///
    /// Source: <`https://json-rpc.dev/docs/reference/version-diff`>
    pub jsonrpc: Option<String>,

    /// The method to be invoked, e.g., "getblock", "sendtransaction".
    pub method: String,

    /// The parameters for the method, json value that must be an array or an object.
    pub params: Option<Value>,

    /// An optional identifier for the request, which can be used to match responses.
    pub id: Value,
}

/// Some utility functions to extract parameters from the request. These
/// methods already handle the case where the parameter is missing or has an
/// unexpected type, returning an error if so.
pub mod arg_parser {

    use serde::Deserialize;
    use serde_json::Value;

    use crate::json_rpc::res::jsonrpc_interface::JsonRpcError;

    /// Extracts an optional parameter from the request by position (array params) or name (object params).
    ///
    /// Returns `Ok(None)` if the field is absent or `null`.
    /// Returns an error if `params` itself is `null` or has an unexpected structure.
    pub fn get_optional<'de, T: Deserialize<'de>>(
        params: &'de Value,
        index: usize,
        field_name: &str,
    ) -> Result<Option<T>, JsonRpcError> {
        let value = match params {
            Value::Null => {
                return Err(JsonRpcError::MissingParameter(field_name.to_string()));
            }
            Value::Array(values) => values.get(index),
            Value::Object(map) => map.get(field_name),
            _ => {
                return Err(JsonRpcError::InvalidParameterStructure(params.to_string()));
            }
        }
        .filter(|v| !v.is_null());

        value
            .map(|value| {
                T::deserialize(value)
                    .map_err(|e| JsonRpcError::InvalidParameterType(format!("{field_name}: {e}")))
            })
            .transpose()
    }

    /// Extracts a required parameter, returning [`JsonRpcError::MissingParameter`] if absent.
    pub fn get_at<'de, T: Deserialize<'de>>(
        params: &'de Value,
        index: usize,
        field_name: &str,
    ) -> Result<T, JsonRpcError> {
        get_optional(params, index, field_name)?
            .ok_or_else(|| JsonRpcError::MissingParameter(field_name.to_string()))
    }

    /// Like [`get_optional`], but substitutes `default` instead of returning `None`.
    pub fn get_with_default<'de, T: Deserialize<'de>>(
        v: &'de Value,
        index: usize,
        field_name: &str,
        default: T,
    ) -> Result<T, JsonRpcError> {
        Ok(get_optional(v, index, field_name)?.unwrap_or(default))
    }
}
