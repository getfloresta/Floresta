// SPDX-License-Identifier: MIT OR Apache-2.0

use core::error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;
use std::path::PathBuf;
use std::str::FromStr;

pub use corepc_types::ScriptPubkey;
pub use corepc_types::v26::AddrManInfoNetwork;
pub use corepc_types::v30::DeploymentInfo;
pub use corepc_types::v30::GetAddrManInfo;
pub use corepc_types::v30::GetBlockHeaderVerbose;
pub use corepc_types::v30::GetBlockVerboseOne;
pub use corepc_types::v30::GetBlockchainInfo;
pub use corepc_types::v30::GetDeploymentInfo;
pub use corepc_types::v30::GetNetworkInfo;
pub use corepc_types::v30::GetNetworkInfoNetwork;
pub use corepc_types::v30::GetTxOut;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Deserialize, Serialize)]
/// Return type for the `gettxoutproof` rpc command, the internal is
/// just the hex representation of the Merkle Block, which was defined
/// by btc core.
pub struct GetTxOutProof(pub Vec<u8>);

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RawTxResp {
    Zero(String),
    One(Box<RawTx>),
}

/// The information returned by a get_raw_transaction verbose one
#[derive(Debug, Deserialize, Serialize)]
pub struct RawTx {
    /// Whether this tx is in our best known chain
    pub in_active_chain: bool,
    /// The hex-encoded tx
    pub hex: String,
    /// The sha256d of the serialized transaction without witness
    pub txid: String,
    /// The sha256d of the serialized transaction including witness
    pub hash: String,
    /// The size this transaction occupies on disk
    pub size: u32,
    /// The virtual size of this transaction, as define by the segwit soft-fork
    pub vsize: u32,
    /// The weight of this transaction, as defined by the segwit soft-fork
    pub weight: u32,
    /// This transaction's version. The current bigger version is 2
    pub version: u32,
    /// This transaction's locktime
    pub locktime: u32,
    /// A list of inputs being spent by this transaction
    ///
    /// See [TxInJson] for more information about the contents of this
    pub vin: Vec<TxInJson>,
    /// A list of outputs being created by this tx
    ///
    /// See [TxOutJson] for more information
    pub vout: Vec<TxOutJson>,
    /// The hash of the block that included this tx, if any
    pub blockhash: String,
    /// How many blocks have been mined after this transaction's confirmation
    /// including the block that confirms it. A zero value means this tx is unconfirmed
    pub confirmations: u32,
    /// The timestamp for the block confirming this tx, if confirmed
    pub blocktime: u32,
    /// Same as blocktime
    pub time: u32,
}

/// A transaction output returned by some RPCs like gettransaction and getblock
#[derive(Debug, Deserialize, Serialize)]
pub struct TxOutJson {
    /// The amount in sats locked in this UTXO
    pub value: u64,
    /// This utxo's index inside the transaction
    pub n: u32,
    /// The locking script of this utxo
    pub script_pub_key: ScriptPubKeyJson,
}

/// The locking script inside a txout
#[derive(Debug, Deserialize, Serialize)]
pub struct ScriptPubKeyJson {
    /// A ASM representation for this script
    ///
    /// Assembly is a high-level representation of a lower level code. Instructions
    /// are turned into OP_XXXXX and data is hex-encoded.
    /// E.g: OP_DUP OP_HASH160 <0000000000000000000000000000000000000000> OP_EQUALVERIFY OP_CHECKSIG
    pub asm: String,
    /// The hex-encoded raw script
    pub hex: String,
    /// How many signatures are required to spend this UTXO.
    ///
    /// This field is deprecated and is here for compatibility with Core
    pub req_sigs: u32,
    #[serde(rename = "type")]
    /// The type of this spk. E.g: PKH, SH, WSH, WPKH, TR, non-standard...
    pub type_: String,
    /// Encode this script using one of the standard address types, if possible
    pub address: String,
}

/// A transaction input returned by some rpcs, like gettransaction and getblock
#[derive(Debug, Deserialize, Serialize)]
pub struct TxInJson {
    /// The txid that created this UTXO
    pub txid: String,
    /// The index of this UTXO inside the tx that created it
    pub vout: u32,
    /// Unlocking script that should solve the challenge and prove ownership over
    /// that UTXO
    pub script_sig: ScriptSigJson,
    /// The nSequence field, used in relative and absolute lock-times
    pub sequence: u32,
    /// A vector of witness elements for this input
    pub witness: Vec<String>,
}

/// A representation for the transaction ScriptSig, returned by some rpcs
/// like gettransaction and getblock
#[derive(Debug, Deserialize, Serialize)]
pub struct ScriptSigJson {
    /// A ASM representation for this scriptSig
    ///
    /// Assembly is a high-level representation of a lower level code. Instructions
    /// are turned into OP_XXXXX and data is hex-encoded.
    /// E.g: OP_PUSHBYTES32 <000000000000000000000000000000000000000000000000000000000000000000>
    pub asm: String,
    /// The hex-encoded script sig
    pub hex: String,
}

/// General information about our peers. Returned by get_peer_info
#[derive(Debug, Deserialize, Serialize)]
pub struct PeerInfo {
    /// This peer's ID in the peer manager.
    pub id: u32,
    /// The network address for this peer.
    pub address: String,
    /// A string with the services this peer advertises. E.g. NODE_NETWORK, UTREEXO, WITNESS...
    pub services: String,
    /// User agent is a string that represents the client being used by our peer. E.g.
    /// /Satoshi-26.0/ for bitcoin core version 26
    pub user_agent: String,
    /// This peer's height at the time we've opened a connection with them
    pub initial_height: u32,
    /// The connection type of this peer
    ///
    /// We can connect with peers for different reasons. E.g. we can connect to a peer to
    /// see if it has a block we're missing, or just to check if that address is still alive.
    /// Possible values are: Feeler, Regular and Extra
    pub kind: String,
    /// The state of this peer
    ///
    /// Can be either Ready, Connecting or Banned
    pub state: String,
    /// The transport protocol used with peer.
    pub transport_protocol: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GetBlockRes {
    Zero(String),

    One(Box<GetBlockVerboseOne>),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
/// The response for getblockheader, which can be either a raw hex-encoded block header or a verbose
/// one with all the fields parsed and decoded.
pub enum GetBlockHeaderRes {
    /// The raw hex-encoded block header, as returned by getblockheader with verbosity false
    Raw(String),

    /// A verbose block header, as returned by getblockheader with verbosity true
    Verbose(Box<GetBlockHeaderVerbose>),
}

/// A confidence enum to auxiliate rescan timestamp values.
///
/// Tells how much confidence you need for this rescan request. That is, the how conservative you want floresta to be when determining which block to start the rescan.
/// will make the rescan to start in a block that have an lower timestamp than the given in order to be more certain
/// about finding addresses and relevant transactions, a lower confidence will make the rescan to be closer to the given value.
///
/// This input is necessary to cover network variancy specially in testnet, for mainnet you can safely use low or medium confidences
/// depending on how much sure you are about the given timestamp covering the addresses you need.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum RescanConfidence {
    /// `high`: 99% confidence interval. Meaning 46 minutes in seconds.
    High,

    /// `medium` (default): 95% confidence interval. Meaning 30 minutes in seconds.
    Medium,

    /// `low`: 90% confidence interval. Meaning 23 minutes in seconds.
    Low,

    /// `exact`: Removes any lookback addition. Meaning 0 in seconds.
    Exact,
}

impl RescanConfidence {
    pub const fn as_secs(&self) -> u32 {
        match self {
            Self::Exact => 0,
            Self::Low => 1_380,
            Self::Medium => 1_800,
            Self::High => 2_760,
        }
    }
}

#[derive(Debug)]
/// All possible errors returned by the jsonrpc
pub enum Error {
    /// An error while deserializing our response
    Serde(serde_json::Error),

    #[cfg(feature = "with-jsonrpc")]
    /// An internal reqwest error
    JsonRpc(jsonrpc::Error),

    /// An error internal to our jsonrpc server
    Api(serde_json::Value),

    /// The server sent an empty response
    EmptyResponse,

    /// The provided verbosity level is invalid
    InvalidVerbosity,

    /// The user requested a rescan based on invalid values.
    InvalidRescanVal,

    /// The requested transaction output was not found
    TxOutNotFound,
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}

#[cfg(feature = "with-jsonrpc")]
impl From<jsonrpc::Error> for Error {
    fn from(value: jsonrpc::Error) -> Self {
        Self::JsonRpc(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "with-jsonrpc")]
            Self::JsonRpc(e) => write!(f, "JsonRpc returned an error {e}"),
            Self::Api(e) => write!(f, "general jsonrpc error: {e}"),
            Self::Serde(e) => write!(f, "error while deserializing the response: {e}"),
            Self::EmptyResponse => write!(f, "got an empty response from server"),
            Self::InvalidVerbosity => write!(f, "invalid verbosity level"),
            Self::InvalidRescanVal => write!(f, "Invalid rescan values"),
            Self::TxOutNotFound => write!(f, "Transaction output was not found"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GetMemInfoStats {
    pub locked: MemInfoLocked,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MemInfoLocked {
    /// Memory currently in use, in bytes
    pub used: u64,
    /// Memory currently free, in bytes
    pub free: u64,
    /// Total memory allocated, in bytes
    pub total: u64,
    /// Total memory locked, in bytes
    ///
    /// If total is less than total, then some pages may be on swap or not philysically allocated
    /// yet
    pub locked: u64,
    /// How many chunks are currently in use
    pub chunks_used: u64,
    /// How many chunks are currently free
    pub chunks_free: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetMemInfoRes {
    Stats(GetMemInfoStats),
    MallocInfo(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActiveCommand {
    pub method: String,
    pub duration: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetRpcInfoRes {
    pub active_commands: Vec<ActiveCommand>,
    pub logpath: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
/// Enum to represent the different subcommands for the addnode command
pub enum AddNodeCommand {
    /// Add a node to the addnode list (but not connect to it)
    Add,

    /// Remove a node from the addnode list (but not necessarily disconnect from it)
    Remove,

    /// Connect to a node once, but don't add it to the addnode list
    Onetry,
}

/// A simple implementation to convert the enum to a string.
/// Useful for get the subcommand name of addnode with
/// command.to_string()
impl Display for AddNodeCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let cmd = match self {
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Onetry => "onetry",
        };
        write!(f, "{cmd}")
    }
}

impl FromStr for AddNodeCommand {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "add" => Ok(Self::Add),
            "remove" => Ok(Self::Remove),
            "onetry" => Ok(Self::Onetry),
            _ => Err(format!("Invalid command: {}", s)),
        }
    }
}

impl error::Error for Error {}
