//! Response types for floresta's JSON-RPC server.
//!
//! This module is split into two main sections:
//!
//! - [`jsonrpc_interface`] — Protocol-level types that implement the
//!   [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification): the [`Response`] /
//!   [`RpcError`] envelope, standard error code constants, and the [`JsonRpcError`] enum that
//!   maps every floresta-specific failure into the appropriate JSON-RPC error code and HTTP
//!   status.
//!
//! - **Serialization structs** (outside the inner module) — Rust representations of the JSON
//!   objects returned by individual RPC methods (`getblockchaininfo`, `getrawtransaction`,
//!   `getblock`, etc.). These structs are `Serialize`/`Deserialize` and mirror the Bitcoin Core
//!   JSON schema where applicable.
//!
//! [`Response`]: jsonrpc_interface::Response
//! [`RpcError`]: jsonrpc_interface::RpcError
//! [`JsonRpcError`]: jsonrpc_interface::JsonRpcError

use std::fmt::Debug;

use corepc_types::v30::GetBlockVerboseOne;
use serde::Deserialize;
use serde::Serialize;

/// Types and methods implementing the [JSON-RPC 2.0 spec](https://www.jsonrpc.org/specification),
/// tailored for floresta's RPC server.
pub mod jsonrpc_interface {
    use core::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;

    use axum::http::StatusCode;
    use floresta_chain::extensions::HeaderExtError;
    use floresta_chain::BlockchainError;
    use floresta_common::impl_error_from;
    use floresta_mempool::mempool::AcceptToMempoolError;
    use floresta_watch_only::WatchOnlyError;
    use serde::Deserialize;
    use serde::Serialize;
    use serde_json::Value;

    pub type RpcResult = std::result::Result<Value, JsonRpcError>;

    #[derive(Debug, Serialize)]
    /// A JSON-RPC response object.
    ///
    /// Exactly one of `result` or `error` will be `Some`.
    pub struct Response {
        /// Present on success, absent on error.
        pub result: Option<Value>,

        /// Present on error, absent on success.
        pub error: Option<RpcError>,

        /// Matches the `id` from the request. `Null` for notifications.
        pub id: Value,
    }

    impl Response {
        /// Creates a successful JSON-RPC response with the given result.
        pub fn success(result: Value, id: Value) -> Self {
            Self {
                result: Some(result),
                error: None,
                id,
            }
        }

        /// Creates an error JSON-RPC response with the given error.
        pub fn error(error: RpcError, id: Value) -> Self {
            Self {
                result: None,
                error: Some(error),
                id,
            }
        }

        /// Converts an [RpcResult] into a success or error response.
        pub fn from_result(result: RpcResult, id: Value) -> Self {
            match result {
                Ok(value) => Self::success(value, id),
                Err(e) => Self::error(e.rpc_error(), id),
            }
        }
    }

    #[derive(Debug, Deserialize, Serialize)]
    /// A JSON-RPC error object.
    pub struct RpcError {
        /// Numeric error code indicating the type of error.
        pub code: i16,

        /// Short description of the error.
        pub message: String,

        /// Optional additional data about the error.
        pub data: Option<Value>,
    }

    impl Display for RpcError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "{}",
                serde_json::to_string(self).expect(SERIALIZATION_EXPECT)
            )
        }
    }

    /// An invalid JSON was received by the server.
    pub const PARSE_ERROR: i16 = -32700;

    /// The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i16 = -32600;

    /// The method does not exist or is not available.
    pub const METHOD_NOT_FOUND: i16 = -32601;

    /// Invalid method parameter(s).
    pub const INVALID_METHOD_PARAMETERS: i16 = -32602;

    /// Internal JSON-RPC error (infrastructure-level, not method-level).
    pub const INTERNAL_ERROR: i16 = -32603;

    /// Upper bound of the implementation-defined server error range (`-32099..=-32000`).
    ///
    /// Floresta maps method-level errors to codes within this range.
    pub const SERVER_ERROR_MIN: i16 = -32099;

    /// Lower bound of the implementation-defined server error range (`-32099..=-32000`).
    ///
    /// Floresta maps method-level errors to codes within this range.
    pub const SERVER_ERROR_MAX: i16 = -32000;

    // Floresta-specific server error codes within the -32099..=-32000 range.
    pub const TX_NOT_FOUND: i16 = SERVER_ERROR_MIN; // -32099
    pub const BLOCK_NOT_FOUND: i16 = SERVER_ERROR_MIN + 1; // -32098
    pub const PEER_NOT_FOUND: i16 = SERVER_ERROR_MIN + 2; // -32097
    pub const NO_ADDRESSES_TO_RESCAN: i16 = SERVER_ERROR_MIN + 3; // -32096
    pub const WALLET_ERROR: i16 = SERVER_ERROR_MIN + 4; // -32095
    pub const MEMPOOL_ERROR: i16 = SERVER_ERROR_MIN + 5; // -32094
    pub const IN_INITIAL_BLOCK_DOWNLOAD: i16 = SERVER_ERROR_MIN + 6; // -32093
    pub const NO_BLOCK_FILTERS: i16 = SERVER_ERROR_MIN + 7; // -32092
    pub const NODE_ERROR: i16 = SERVER_ERROR_MIN + 8; // -32091
    pub const CHAIN_ERROR: i16 = SERVER_ERROR_MIN + 9; // -32090
    pub const FILTERS_ERROR: i16 = SERVER_ERROR_MAX; // -32000

    #[derive(Debug)]
    pub enum JsonRpcError {
        /// Rescan requested but the watch-only wallet has no addresses.
        NoAddressesToRescan,

        /// Rescan requested with invalid values.
        InvalidRescanVal,

        /// A required parameter is missing from the request.
        MissingParameter(String),

        /// A parameter have an unexpected type (e.g. number where string was expected).
        InvalidParameterType(String),

        /// Verbosity level received does not fit on available values.
        InvalidVerbosityLevel,

        /// Transaction not found.
        TxNotFound,

        /// The provided script is invalid.
        InvalidScript,

        /// The provided descriptor is invalid.
        InvalidDescriptor(miniscript::Error),

        /// Block not found in the blockchain.
        BlockNotFound,

        /// Chain-level error (e.g. chain not synced or invalid).
        Chain,

        /// The JSON-RPC request itself is malformed.
        InvalidRequest,

        /// The requested RPC method does not exist.
        MethodNotFound,

        /// Failed to decode the request payload.
        Decode(String),

        /// The provided network address is invalid.
        InvalidAddress,

        /// Node-level error (e.g. not connected or unresponsive).
        Node(String),

        /// Block filters are not enabled, but the requested RPC requires them.
        NoBlockFilters,

        /// The provided hex string is invalid.
        InvalidHex,

        /// The node is still performing initial block download.
        InInitialBlockDownload,

        /// Invalid mode passed to `getmemoryinfo`.
        InvalidMemInfoMode,

        /// Wallet error (e.g. wallet not loaded or unavailable).
        Wallet(String),

        /// Block filter error (e.g. filter data unavailable or corrupt).
        Filters(String),

        /// Overflow when calculating cumulative chain work.
        ChainWorkOverflow,

        /// Invalid `addnode` command or parameters.
        InvalidAddnodeCommand,

        /// Invalid `disconnectnode` command (both address and node ID were provided).
        InvalidDisconnectNodeCommand,

        /// Peer not found in the peer list.
        PeerNotFound,

        /// Timestamp argument to `rescanblockchain` is before the genesis block
        /// (and not zero, which is the default).
        InvalidTimestamp,

        /// Transaction was rejected by the mempool.
        MempoolAccept(AcceptToMempoolError),
    }

    impl_error_from!(JsonRpcError, AcceptToMempoolError, MempoolAccept);

    impl JsonRpcError {
        pub fn http_code(&self) -> StatusCode {
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
                | JsonRpcError::Wallet(_) => StatusCode::BAD_REQUEST,

                // 404 Not Found - resource/method doesn't exist
                JsonRpcError::MethodNotFound
                | JsonRpcError::BlockNotFound
                | JsonRpcError::TxNotFound
                | JsonRpcError::PeerNotFound => StatusCode::NOT_FOUND,

                // 500 Internal Server Error - server messed up
                JsonRpcError::ChainWorkOverflow => StatusCode::INTERNAL_SERVER_ERROR,

                // 503 Service Unavailable - server can't handle right now
                JsonRpcError::InInitialBlockDownload
                | JsonRpcError::NoBlockFilters
                | JsonRpcError::Node(_)
                | JsonRpcError::Chain
                | JsonRpcError::Filters(_) => StatusCode::SERVICE_UNAVAILABLE,
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

                // Server errors (implementation-defined: -32099..=-32000)
                JsonRpcError::TxNotFound => RpcError {
                    code: TX_NOT_FOUND,
                    message: "Transaction not found".into(),
                    data: None,
                },
                JsonRpcError::BlockNotFound => RpcError {
                    code: BLOCK_NOT_FOUND,
                    message: "Block not found".into(),
                    data: None,
                },
                JsonRpcError::PeerNotFound => RpcError {
                    code: PEER_NOT_FOUND,
                    message: "Peer not found".into(),
                    data: None,
                },
                JsonRpcError::NoAddressesToRescan => RpcError {
                    code: NO_ADDRESSES_TO_RESCAN,
                    message: "No addresses to rescan".into(),
                    data: None,
                },
                JsonRpcError::Wallet(msg) => RpcError {
                    code: WALLET_ERROR,
                    message: "Wallet error".into(),
                    data: Some(Value::String(msg.clone())),
                },
                JsonRpcError::MempoolAccept(msg) => RpcError {
                    code: MEMPOOL_ERROR,
                    message: "Mempool error".into(),
                    data: Some(Value::String(format!("{msg}"))),
                },
                JsonRpcError::InInitialBlockDownload => RpcError {
                    code: IN_INITIAL_BLOCK_DOWNLOAD,
                    message: "Node is in initial block download".into(),
                    data: None,
                },
                JsonRpcError::NoBlockFilters => RpcError {
                    code: NO_BLOCK_FILTERS,
                    message: "Block filters not available".into(),
                    data: None,
                },
                JsonRpcError::Node(msg) => RpcError {
                    code: NODE_ERROR,
                    message: "Node error".into(),
                    data: Some(Value::String(msg.clone())),
                },
                JsonRpcError::Chain => RpcError {
                    code: CHAIN_ERROR,
                    message: "Chain error".into(),
                    data: None,
                },
                JsonRpcError::Filters(msg) => RpcError {
                    code: FILTERS_ERROR,
                    message: "Filters error".into(),
                    data: Some(Value::String(msg.clone())),
                },
            }
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
    impl<T: fmt::Debug> From<WatchOnlyError<T>> for JsonRpcError {
        fn from(e: WatchOnlyError<T>) -> Self {
            JsonRpcError::Wallet(e.to_string())
        }
    }

    impl From<BlockchainError> for JsonRpcError {
        fn from(e: BlockchainError) -> Self {
            match e {
                BlockchainError::BlockNotPresent => JsonRpcError::BlockNotFound,
                _ => JsonRpcError::Chain,
            }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
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
