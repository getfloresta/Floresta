use corepc_types::v30::GetBlockVerboseOne;
use floresta_chain::extensions::HeaderExtError;
use floresta_common::impl_error_from;
use floresta_mempool::mempool::AcceptToMempoolError;
use serde::Deserialize;
use serde::Serialize;

/// Holds types and methods implementing the [JsonRpc spec](https://www.jsonrpc.org/specification) specifics.
///
/// These are floresta-flavored in a sense that this implementation is made to fit floresta necessities.
pub mod jsonrpc_interface {
    use std::fmt::Display;

    use axum::response::IntoResponse;
    use floresta_chain::extensions::HeaderExtError;
    use floresta_common::impl_error_from;
    use floresta_mempool::mempool::AcceptToMempoolError;
    use serde::Deserialize;
    use serde::Serialize;
    use serde_json::Value;

    pub type RpcResult = std::result::Result<Value, JsonRpcError>;

    /// A jsonrpc response object.
    #[derive(Debug, Serialize)]
    pub struct Response {
        /// The `result` field, should be Some when the method called correctly was executed, therefore error should be None.
        /// The opposite may happen too, when error is Some, `result` should be None.
        pub result: Option<Value>,

        /// The error field, should be Some when the method called returned an error, therefore result should be None.
        /// The opposite may happen too, when `result` is Some, error should be None.
        pub error: Option<RpcError>,

        /// A regular id. Should be a String or a Number, Null for notification.
        pub id: Value,
    }

    impl Response {
        pub fn success(result: Value, id: Value) -> Self {
            Self {
                result: Some(result),
                error: None,
                id,
            }
        }

        pub fn error(error: RpcError, id: Value) -> Self {
            Self {
                result: None,
                error: Some(error),
                id,
            }
        }

        pub fn from_result(result: RpcResult, id: Value) -> Self {
            match result {
                Ok(value) => Self::success(value, id),
                Err(e) => Self::error(e.rpc_error(), id),
            }
        }
    }

    /// Jsonrpc error object.
    #[derive(Debug, Deserialize, Serialize)]
    pub struct RpcError {
        /// A Number that indicates the error type that occurred.
        pub code: i16,
        /// A String providing a short description of the error.
        pub message: String,
        /// A Primitive or Structured value that contains additional information about the error.
        pub data: Option<Value>,
    }

    impl Display for RpcError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", serde_json::to_string(self).unwrap())
        }
    }

    /// Invalid JSON was received by the server.
    pub const PARSE_ERROR: i16 = -32700;

    /// The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i16 = -32600;

    /// The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i16 = -32601;

    /// Invalid method parameter(s).
    pub const INVALID_METHOD_PARAMETERS: i16 = -32602;

    /// Internal JSON-RPC Error.
    ///
    /// We interpret these being the errors related to the infrastructure functionality,
    /// method related error are implemented in `SERVER_ERROR`.
    pub const INTERNAL_ERROR: i16 = -32603;

    /// Server Error Max code value, reserved for implementation-defined errors.
    ///
    /// In floresta we use these to map method errors.
    ///
    /// See also `SERVER_ERROR_MIN`.
    pub const SERVER_ERROR_MAX: i16 = -32099;

    /// Server Error Min code value, reserved for implementation-defined errors.
    ///
    /// In floresta we use these to map method errors.
    ///
    /// See also `SERVER_ERROR_MAX`.
    pub const SERVER_ERROR_MIN: i16 = -32000;

    #[derive(Debug)]
    pub enum JsonRpcError {
        /// There was a rescan request but we do not have any addresses in the watch-only wallet.
        NoAddressesToRescan,

        /// There was a rescan request with invalid values
        InvalidRescanVal,

        /// Missing parameter, e.g., if a required parameter is not provided in the request
        MissingParameter(String),

        /// The provided parameter is of the wrong type, e.g., if a string is expected but a number is
        /// provided
        InvalidParameterType(String),

        /// Verbosity level is not 0 or 1
        InvalidVerbosityLevel,

        /// The requested transaction is not found in the blockchain
        TxNotFound,

        /// The provided script is invalid, e.g., if it is not a valid P2PKH or P2SH script
        InvalidScript,

        /// The provided descriptor is invalid, e.g., if it does not match the expected format
        InvalidDescriptor(miniscript::Error),

        /// The requested block is not found in the blockchain
        BlockNotFound,

        /// There is an error with the chain, e.g., if the chain is not synced or when the chain is not valid
        Chain,

        /// The request is invalid, e.g., some parameters use an incorrect type
        InvalidRequest,

        /// The requested method is not found, e.g., if the method is not implemented or when the method is not available
        MethodNotFound,

        /// This error is returned when there is an error decoding the request, e.g., if the request is not valid JSON
        Decode(String),

        /// The provided address is invalid, e.g., when it is not a valid IP address or hostname
        InvalidAddress,

        /// This error is returned when there is an error with the node, e.g., if the node is not connected or when the node is not responding
        Node(String),

        /// This error is returned when the node does not have block filters enabled, which is required for some RPC calls
        NoBlockFilters,

        /// This error is returned when a hex value is invalid
        InvalidHex,

        /// This error is returned when the node is in initial block download, which means it is still syncing the blockchain
        InInitialBlockDownload,

        InvalidMemInfoMode,

        /// This error is returned when there is an error with the wallet, e.g., if the wallet is not loaded or when the wallet is not available
        Wallet(String),

        /// This error is returned when there is an error with block filters, e.g., if the filters are not available or when there is an issue with the filter data
        Filters(String),

        /// This error is returned when there is an error calculating the chain work
        ChainWorkOverflow,

        /// This error is returned when the addnode command is invalid, e.g., if the command is not recognized or when the parameters are incorrect
        InvalidAddnodeCommand,

        /// Invalid `disconnect` node command (both address and ID parameters are present).
        InvalidDisconnectNodeCommand,

        /// Peer was not found in the peer list.
        PeerNotFound,

        /// Raised if when the rescanblockchain command, with the timestamp flag activated, contains some timestamp thats less than the genesis one and not zero which is the default value for this arg.
        InvalidTimestamp,

        /// Something went wrong when attempting to publish a transaction to mempool
        MempoolAccept(AcceptToMempoolError),
    }

    impl_error_from!(JsonRpcError, AcceptToMempoolError, MempoolAccept);

    impl JsonRpcError {
        pub fn http_code(&self) -> u16 {
            use axum::http::StatusCode;

            match self {
                // 400 Bad Request - client sent invalid data
                JsonRpcError::InvalidHex
                | JsonRpcError::InvalidAddress
                | JsonRpcError::InvalidScript
                | JsonRpcError::InvalidRequest
                | JsonRpcError::InvalidDescriptor(_)
                | JsonRpcError::InvalidVerbosityLevel
                | JsonRpcError::Decode(_)
                | JsonRpcError::MempoolAccept(_)
                | JsonRpcError::InvalidMemInfoMode
                | JsonRpcError::InvalidAddnodeCommand
                | JsonRpcError::InvalidDisconnectNodeCommand
                | JsonRpcError::InvalidTimestamp
                | JsonRpcError::InvalidRescanVal
                | JsonRpcError::NoAddressesToRescan
                | JsonRpcError::InvalidParameterType(_)
                | JsonRpcError::MissingParameter(_)
                | JsonRpcError::Wallet(_) => StatusCode::BAD_REQUEST.as_u16(),

                // 404 Not Found - resource/method doesn't exist
                JsonRpcError::MethodNotFound
                | JsonRpcError::BlockNotFound
                | JsonRpcError::TxNotFound
                | JsonRpcError::PeerNotFound => StatusCode::NOT_FOUND.as_u16(),

                // 500 Internal Server Error - server messed up
                JsonRpcError::ChainWorkOverflow => StatusCode::INTERNAL_SERVER_ERROR.as_u16(),

                // 503 Service Unavailable - server can't handle right now
                JsonRpcError::InInitialBlockDownload
                | JsonRpcError::NoBlockFilters
                | JsonRpcError::Node(_)
                | JsonRpcError::Chain
                | JsonRpcError::Filters(_) => StatusCode::SERVICE_UNAVAILABLE.as_u16(),
            }
        }

        pub fn rpc_error(&self) -> RpcError {
            match self {
                // Parse error - invalid JSON received
                JsonRpcError::Decode(msg) => RpcError {
                    code: PARSE_ERROR,
                    message: "Parse error".into(),
                    data: Some(Value::String(msg.clone())),
                },

                // Invalid request - not a valid JSON-RPC request
                JsonRpcError::InvalidRequest => RpcError {
                    code: INVALID_REQUEST,
                    message: "Invalid request".into(),
                    data: None,
                },

                // Method not found
                JsonRpcError::MethodNotFound => RpcError {
                    code: METHOD_NOT_FOUND,
                    message: "Method not found".into(),
                    data: None,
                },

                // Invalid params - invalid method parameters
                JsonRpcError::InvalidHex => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid hex encoding".into(),
                    data: None,
                },
                JsonRpcError::InvalidAddress => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid address".into(),
                    data: None,
                },
                JsonRpcError::InvalidScript => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid script".into(),
                    data: None,
                },
                JsonRpcError::InvalidDescriptor(e) => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid descriptor".into(),
                    data: Some(Value::String(e.to_string())),
                },
                JsonRpcError::InvalidVerbosityLevel => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid verbosity level".into(),
                    data: None,
                },
                JsonRpcError::InvalidTimestamp => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid timestamp".into(),
                    data: None,
                },
                JsonRpcError::InvalidMemInfoMode => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid meminfo mode".into(),
                    data: None,
                },
                JsonRpcError::InvalidAddnodeCommand => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid addnode command".into(),
                    data: None,
                },
                JsonRpcError::InvalidDisconnectNodeCommand => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid disconnectnode command".into(),
                    data: None,
                },
                JsonRpcError::InvalidRescanVal => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid rescan values".into(),
                    data: None,
                },
                JsonRpcError::InvalidParameterType(param) => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Invalid parameter type".into(),
                    data: Some(Value::String(param.clone())),
                },
                JsonRpcError::MissingParameter(param) => RpcError {
                    code: INVALID_METHOD_PARAMETERS,
                    message: "Missing parameter".into(),
                    data: Some(Value::String(param.clone())),
                },

                // Internal error
                JsonRpcError::ChainWorkOverflow => RpcError {
                    code: INTERNAL_ERROR,
                    message: "Chain work overflow".into(),
                    data: None,
                },

                // Server errors (implementation-defined: -32000 to -32099)
                JsonRpcError::TxNotFound => RpcError {
                    code: SERVER_ERROR_MAX, // -32099
                    message: "Transaction not found".into(),
                    data: None,
                },
                JsonRpcError::BlockNotFound => RpcError {
                    code: SERVER_ERROR_MAX + 1, // -32098
                    message: "Block not found".into(),
                    data: None,
                },
                JsonRpcError::PeerNotFound => RpcError {
                    code: SERVER_ERROR_MAX + 2, // -32097
                    message: "Peer not found".into(),
                    data: None,
                },
                JsonRpcError::NoAddressesToRescan => RpcError {
                    code: SERVER_ERROR_MAX + 3, // -32096
                    message: "No addresses to rescan".into(),
                    data: None,
                },
                JsonRpcError::Wallet(msg) => RpcError {
                    code: SERVER_ERROR_MAX + 4, // -32095
                    message: "Wallet error".into(),
                    data: Some(Value::String(msg.clone())),
                },
                JsonRpcError::MempoolAccept(msg) => RpcError {
                    code: SERVER_ERROR_MAX + 5, // -32094
                    message: "Mempool error:".into(),
                    data: Some(Value::String(format!("{msg}"))),
                },
                JsonRpcError::InInitialBlockDownload => RpcError {
                    code: SERVER_ERROR_MAX + 6, // -32093
                    message: "Node is in initial block download".into(),
                    data: None,
                },
                JsonRpcError::NoBlockFilters => RpcError {
                    code: SERVER_ERROR_MAX + 7, // -32092
                    message: "Block filters not available".into(),
                    data: None,
                },
                JsonRpcError::Node(msg) => RpcError {
                    code: SERVER_ERROR_MAX + 8, // -32091
                    message: "Node error".into(),
                    data: Some(Value::String(msg.clone())),
                },
                JsonRpcError::Chain => RpcError {
                    code: SERVER_ERROR_MAX + 9, // -32090
                    message: "Chain error".into(),
                    data: None,
                },
                JsonRpcError::Filters(msg) => RpcError {
                    code: SERVER_ERROR_MIN, // -32000
                    message: "Filters error".into(),
                    data: Some(Value::String(msg.clone())),
                },
            }
        }
    }

    impl IntoResponse for JsonRpcError {
        fn into_response(self) -> axum::http::Response<axum::body::Body> {
            Response::error(self.rpc_error(), Value::Null).into_response()
        }
    }

    impl IntoResponse for Response {
        fn into_response(self) -> axum::http::Response<axum::body::Body> {
            let status = if self.error.is_some() {
                axum::http::StatusCode::BAD_REQUEST
            } else {
                axum::http::StatusCode::OK
            };

            axum::http::Response::builder()
                .status(status)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&self).unwrap()))
                .unwrap()
        }
    }

    impl From<HeaderExtError> for JsonRpcError {
        fn from(value: HeaderExtError) -> Self {
            match value {
                HeaderExtError::Chain(_) => JsonRpcError::Chain,
                HeaderExtError::BlockNotFound => JsonRpcError::BlockNotFound,
                HeaderExtError::ChainWorkOverflow => JsonRpcError::ChainWorkOverflow,
            }
        }
    }

    impl_error_from!(JsonRpcError, miniscript::Error, InvalidDescriptor);

    impl<T: std::fmt::Debug> From<floresta_watch_only::WatchOnlyError<T>> for JsonRpcError {
        fn from(e: floresta_watch_only::WatchOnlyError<T>) -> Self {
            JsonRpcError::Wallet(e.to_string())
        }
    }
}
#[derive(Deserialize, Serialize)]
pub struct GetBlockchainInfoRes {
    pub best_block: String,
    pub height: u32,
    pub ibd: bool,
    pub validated: u32,
    pub latest_work: String,
    pub latest_block_time: u32,
    pub leaf_count: u32,
    pub root_count: u32,
    pub root_hashes: Vec<String>,
    pub chain: String,
    pub progress: f32,
    pub difficulty: u64,
}

/// A confidence enum to auxiliate rescan timestamp values.
///
/// Serves to tell how much confidence you need in such a rescan request. That is, the need for a high confidence rescan
/// will make the rescan to start in a block that have an lower timestamp than the given in order to be more secure
/// about finding addresses and relevant transactions, a lower confidence will make the rescan to be closer to the given value.
///
/// This input is necessary to cover network variancy specially in testnet, for mainnet you can safely use low or medium confidences
/// depending on how much sure you are about the given timestamp covering the addresses you need.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum RescanConfidence {
    /// `high`: 99% confidence interval. Returning 46 minutes in seconds for `val`.
    High,

    /// `medium` (default): 95% confidence interval. Returning 30 minutes in seconds for `val`.
    Medium,

    /// `low`: 90% confidence interval. Returning 23 minutes in seconds for `val`.
    Low,

    /// `exact`: Removes any lookback addition. Returning 0 for `val`
    Exact,
}

impl RescanConfidence {
    /// In cases where `use_timestamp` is set, tells how much confidence the user wants for finding its addresses from this rescan request, a higher confidence will add more lookback seconds to the targeted timestamp and rescanning more blocks.
    /// Under the hood this uses an [Exponential distribution](https://en.wikipedia.org/wiki/Exponential_distribution) [cumulative distribution function (CDF)](https:///en.wikipedia.org/wiki/Cumulative_distribution_function) where the parameter $\lambda$ (rate) is $\frac{1}{600}$ (1 block every 600 seconds, 10 minutes).
    ///   The supplied string can be one of:
    ///
    ///   - `high`: 99% confidence interval. Returning 46 minutes in seconds for `val`.
    ///   - `medium` (default): 95% confidence interval. Returning 30 minutes in seconds for `val`.
    ///   - `low`: 90% confidence interval. Returning 23 minutes in seconds for `val`.
    ///   - `exact`: Removes any lookback addition. Returning 0 for `val`
    pub const fn as_secs(&self) -> u32 {
        match self {
            Self::Exact => 0,
            Self::Low => 1_380,
            Self::Medium => 1_800,
            Self::High => 2_760,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct RawTxJson {
    pub in_active_chain: bool,
    pub hex: String,
    pub txid: String,
    pub hash: String,
    pub size: u32,
    pub vsize: u32,
    pub weight: u32,
    pub version: u32,
    pub locktime: u32,
    pub vin: Vec<TxInJson>,
    pub vout: Vec<TxOutJson>,
    pub blockhash: String,
    pub confirmations: u32,
    pub blocktime: u32,
    pub time: u32,
}

#[derive(Deserialize, Serialize)]
pub struct TxOutJson {
    pub value: u64,
    pub n: u32,
    pub script_pub_key: ScriptPubKeyJson,
}

#[derive(Deserialize, Serialize)]
pub struct ScriptPubKeyJson {
    pub asm: String,
    pub hex: String,
    pub req_sigs: u32,
    #[serde(rename = "type")]
    pub type_: String,
    pub address: String,
}

#[derive(Deserialize, Serialize)]
pub struct TxInJson {
    pub txid: String,
    pub vout: u32,
    pub script_sig: ScriptSigJson,
    pub sequence: u32,
    pub witness: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub struct ScriptSigJson {
    pub asm: String,
    pub hex: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GetBlockRes {
    Zero(String),
    One(Box<GetBlockVerboseOne>),
}

/// Return type for the `gettxoutproof` rpc command, the internal is
/// just the hex representation of the Merkle Block, which was defined
/// by btc core.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetTxOutProof(pub Vec<u8>);
