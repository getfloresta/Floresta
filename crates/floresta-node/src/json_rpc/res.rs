use std::fmt::Display;

use axum::response::IntoResponse;
use corepc_types::v30::GetBlockVerboseOne;
use floresta_chain::extensions::HeaderExtError;
use floresta_common::impl_error_from;
use floresta_mempool::mempool::AcceptToMempoolError;
use floresta_watch_only::descriptor::DescriptorError;
use serde::Deserialize;
use serde::Serialize;

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

#[derive(Debug, Deserialize, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<String>,
}

/// Return type for the `gettxoutproof` rpc command, the internal is
/// just the hex representation of the Merkle Block, which was defined
/// by btc core.
#[derive(Debug, Deserialize, Serialize)]
pub struct GetTxOutProof(pub Vec<u8>);

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
    InvalidDescriptor(DescriptorError),

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

impl Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonRpcError::InvalidTimestamp => write!(f, "Invalid timestamp, ensure that it is between the genesis and the tip."),
            JsonRpcError::InvalidRescanVal => write!(f, "Your rescan request contains invalid values"),
            JsonRpcError::NoAddressesToRescan => write!(f, "You do not have any address to proceed with the rescan"),
            JsonRpcError::MissingParameter(opt) => write!(f, "Missing parameter: {opt}"),
            JsonRpcError::InvalidParameterType(opt) => write!(f, "Invalid parameter type for: {opt}"),
            JsonRpcError::InvalidRequest => write!(f, "Invalid request"),
            JsonRpcError::InvalidHex =>  write!(f, "Invalid hex"),
            JsonRpcError::MethodNotFound =>  write!(f, "Method not found"),
            JsonRpcError::Decode(e) =>  write!(f, "error decoding request: {e}"),
            JsonRpcError::TxNotFound =>  write!(f, "Transaction not found"),
            JsonRpcError::InvalidDescriptor(e) =>  write!(f, "Invalid descriptor: {e}"),
            JsonRpcError::BlockNotFound =>  write!(f, "Block not found"),
            JsonRpcError::Chain => write!(f, "Chain error"),
            JsonRpcError::InvalidAddress => write!(f, "Invalid address"),
            JsonRpcError::Node(e) => write!(f, "Node error: {e}"),
            JsonRpcError::NoBlockFilters => write!(f, "You don't have block filters enabled, please start florestad without --no-cfilters to run this RPC"),
            JsonRpcError::InInitialBlockDownload => write!(f, "Node is in initial block download, wait until it's finished"),
            JsonRpcError::InvalidScript => write!(f, "Invalid script"),
            JsonRpcError::InvalidVerbosityLevel => write!(f, "Invalid verbosity level"),
            JsonRpcError::InvalidMemInfoMode => write!(f, "Invalid meminfo mode, should be stats or mallocinfo"),
            JsonRpcError::Wallet(e) => write!(f, "Wallet error: {e}"),
            JsonRpcError::Filters(e) => write!(f, "Error with filters: {e}"),
            JsonRpcError::ChainWorkOverflow => write!(f, "Overflow while calculating the chain work"),
            JsonRpcError::InvalidAddnodeCommand => write!(f, "Invalid addnode command"),
            JsonRpcError::InvalidDisconnectNodeCommand => write!(f, "Invalid disconnectnode command"),
            JsonRpcError::PeerNotFound => write!(f, "Peer not found in the peer list"),
            JsonRpcError::MempoolAccept(e) => write!(f, "Could not send transaction to mempool due to {e}"),
        }
    }
}

impl IntoResponse for JsonRpcError {
    fn into_response(self) -> axum::http::Response<axum::body::Body> {
        let body = serde_json::json!({
            "error": self.to_string(),
            "result": serde_json::Value::Null,
            "id": serde_json::Value::Null,
        });
        axum::http::Response::builder()
            .status(axum::http::StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
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

impl_error_from!(JsonRpcError, DescriptorError, InvalidDescriptor);

impl<T: std::fmt::Debug> From<floresta_watch_only::WatchOnlyError<T>> for JsonRpcError {
    fn from(e: floresta_watch_only::WatchOnlyError<T>) -> Self {
        JsonRpcError::Wallet(e.to_string())
    }
}
