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
    pub id: u64,
}

/// Some utility functions to extract parameters from the request. These
/// methods already handle the case where the parameter is missing or has an
/// unexpected type, returning an error if so.
pub mod arg_parser {

    use std::fmt::Debug;
    use std::fmt::Display;

    use serde::de::DeserializeOwned;
    use serde_json::from_value;
    use serde_json::Value;

    /// Extracts a u64 parameter from the request parameters at the specified index.
    ///
    /// This function checks if the parameter exists, is of type u64 and can be converted to `T`.
    /// Returns an error otherwise.
    pub fn get_numeric<T>(
        params: &Value,
        index: usize,
        field_name: &str,
    ) -> Result<T, ArgGetterError>
    where
        T: TryFrom<i64> + Debug,
    {
        let v = get_arg_by(params, index, field_name).ok_or(ArgGetterError::MissingParameter {
            param: field_name.to_string(),
        })?;

        T::try_from(
            v.as_i64()
                .ok_or(ArgGetterError::InvalidParameterType(format!(
                    "{field_name} must be an array a integer"
                )))?,
        )
        .map_err(|_| {
            ArgGetterError::InvalidParameterType(format!("{field_name} must be an array a integer"))
        })
    }

    /// Extracts a string parameter from the request parameters at the specified index.
    ///
    /// This function checks if the parameter exists and is of type string. Returns an error
    /// otherwise.
    pub fn get_string(
        params: &Value,
        index: usize,
        field_name: &str,
    ) -> Result<String, ArgGetterError> {
        let v = get_arg_by(params, index, field_name).ok_or(ArgGetterError::MissingParameter {
            param: field_name.to_string(),
        })?;

        let str = v.as_str().ok_or_else(|| {
            ArgGetterError::InvalidParameterType(format!("{field_name} must be a string"))
        })?;

        Ok(str.to_string())
    }

    /// Extracts a boolean parameter from the request parameters at the specified index.
    ///
    /// This function checks if the parameter exists and is of type boolean. Returns an error
    /// otherwise.
    pub fn get_bool(
        params: &Value,
        index: usize,
        field_name: &str,
    ) -> Result<bool, ArgGetterError> {
        let v = get_arg_by(params, index, field_name).ok_or(ArgGetterError::MissingParameter {
            param: field_name.to_string(),
        })?;

        v.as_bool().ok_or_else(|| {
            ArgGetterError::InvalidParameterType(format!("{field_name} must be a boolean"))
        })
    }

    /// Extracts a hash parameter from the request parameters at the specified index.
    ///
    /// This function can extract any type that implements `FromStr`, such as `BlockHash` or
    /// `Txid`. It checks if the parameter exists and is a valid string representation of the type.
    /// Returns an error otherwise.
    pub fn get_hash<T>(params: &Value, index: usize, field_name: &str) -> Result<T, ArgGetterError>
    where
        T: DeserializeOwned,
    {
        let v = get_arg_by(params, index, field_name).ok_or(ArgGetterError::MissingParameter {
            param: field_name.to_string(),
        })?;

        from_value(v.clone()).map_err(|_| {
            ArgGetterError::InvalidParameterType(format!("{field_name} must be an array of hashes"))
        })
    }

    /// Extracts an array of hashes from the request parameters at the specified index.
    ///
    /// This function can extract an array of any type that implements `FromStr`, such as
    /// `BlockHash` or `Txid`. It checks if the parameter exists and is an array of valid string
    /// representations of the type. Returns an error otherwise.
    pub fn get_hashes_array<T>(
        params: &Value,
        index: usize,
        field_name: &str,
    ) -> Result<Vec<T>, ArgGetterError>
    where
        T: DeserializeOwned,
    {
        let v = get_arg_by(params, index, field_name).ok_or(ArgGetterError::MissingParameter {
            param: field_name.to_string(),
        })?;

        from_value(v.clone()).map_err(|_| {
            ArgGetterError::InvalidParameterType(format!("{field_name} must be an array of hashes"))
        })
    }

    /// Extracts an optional field from the request parameters at the specified index.
    ///
    /// This function checks if the parameter exists and is of the expected type. If the parameter
    /// doesn't exist, it returns `None`. If it exists but is of an unexpected type, it returns an
    /// error.
    pub fn get_optional_field<T, F>(
        params: &Value,
        index: usize,
        field_name: &str,
        extractor_fn: F,
    ) -> Result<Option<T>, ArgGetterError>
    where
        F: Fn(&Value, usize, &str) -> Result<T, ArgGetterError>,
    {
        match extractor_fn(params, index, field_name) {
            Ok(t) => Ok(Some(t)),
            Err(ArgGetterError::MissingParameter { param: _ }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get an arg by a numeric index on by its field_name.
    ///
    /// This checks whether the object should contain named or positional parameters
    pub fn get_arg_by<'a>(from: &'a Value, index: usize, field_name: &str) -> Option<&'a Value> {
        if from.is_object() {
            from.get(field_name)
        } else {
            from.get(index)
        }
    }

    #[derive(Debug)]
    /// Errors that can occur while using the getters.
    pub enum ArgGetterError {
        /// Such a parameter is missing and is not optional
        MissingParameter { param: String },
        /// Error message explaining that a json field contains a value that dont parse into the expected type.
        InvalidParameterType(String),
    }

    impl Display for ArgGetterError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ArgGetterError::InvalidParameterType(e) => {
                    write!(f, "{e}")
                }
                ArgGetterError::MissingParameter { param: e } => {
                    write!(f, "The {e} parameter is missing and is not optional")
                }
            }
        }
    }
}
