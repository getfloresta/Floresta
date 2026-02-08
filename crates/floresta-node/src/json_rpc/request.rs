//! This module defines the structure for JSON-RPC requests and provides utility functions to
//! extract parameters from the request.

use serde_json::Value;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
/// Represents a JSON-RPC 2.0 request.
pub struct RpcRequest {
    /// The JSON-RPC version, typically "2.0".
    ///
    /// For JSON-RPC 2.0, this field is required. For earlier versions, it may be omitted.
    ///
    /// Source: <`https://json-rpc.dev/docs/reference/version-diff`>
    pub jsonrpc: Option<String>,

    /// The method to be invoked, e.g., "getblock", "sendtransaction".
    pub method: String,

    /// The parameters for the method, as an array of json values.
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

    /// Extracts a parameter from the request parameters at the specified index.
    ///
    /// This function can extract any type that implements `FromStr`, such as `BlockHash` or
    /// `Txid`. It checks if the parameter exists and is a valid string representation of the type.
    /// Returns an error otherwise.
    pub fn get_at<T: Deserialize<'static>>(
        params: &Value,
        index: usize,
        field_name: &str,
    ) -> Result<T, JsonRpcError> {
        let v = match (params.is_array(), params.is_object()) {
            (true, false) => params.get(index),
            (false, true) => params.get(field_name),
            _ => None,
        };

        let unwrap = v
            .ok_or_else(|| JsonRpcError::MissingParameter(field_name.to_string()))?
            .clone();

        T::deserialize(unwrap).map_err(|_| {
            JsonRpcError::InvalidParameterType(format!("{field_name} has an invalid type"))
        })
    }

    /// Extracts an array of hashes from the request parameters at the specified index.
    ///
    /// This function can extract an array of any type that implements `FromStr`, such as
    /// `BlockHash` or `Txid`. It checks if the parameter exists and is an array of valid string
    /// representations of the type. Returns an error otherwise.
    pub fn get_arr_at<T: Deserialize<'static>>(
        params: &Value,
        index: usize,
        field_name: &str,
    ) -> Result<Vec<T>, JsonRpcError> {
        let v = match (params.is_array(), params.is_object()) {
            (true, false) => params.get(index),
            (false, true) => return Err(JsonRpcError::InvalidRequest),
            _ => None,
        };

        let unwrap = v
            .ok_or_else(|| JsonRpcError::MissingParameter(field_name.to_string()))?
            .as_array()
            .unwrap(); // Safe unwrap, we checked if this is an array earlier.

        unwrap
            .iter()
            .enumerate()
            .map(|(index, v)| get_at(v, index, field_name))
            .collect()
    }

    pub fn optional<T>(result: Result<T, JsonRpcError>) -> Result<Option<T>, JsonRpcError> {
        match result {
            Ok(t) => Ok(Some(t)),
            Err(JsonRpcError::MissingParameter(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Extracts a field from the request parameters at the specified index.
    ///
    /// This function checks if the parameter exists and is of the expected type. If the parameter
    /// doesn't exist, it returns `default`. If it exists but is of an unexpected type, it returns an
    /// error.
    pub fn get_with_default<T: Deserialize<'static>>(
        v: &Value,
        index: usize,
        field_name: &str,
        default: T,
    ) -> Result<T, JsonRpcError> {
        match get_at(v, index, field_name) {
            Ok(t) => Ok(t),
            Err(JsonRpcError::MissingParameter(_)) => Ok(default),
            Err(e) => Err(e),
        }
    }
}
