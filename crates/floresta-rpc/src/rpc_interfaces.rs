// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;
use core::fmt::Display;

use bitcoin::BlockHash;
use bitcoin::Txid;
use bitcoin::hashes::Hash;

use super::rpc_types::*;

#[maybe_async::maybe_async]
pub trait BlockchainRpc {
    type Error: Display + Debug;

    /// Finds an specific utxo in the chain
    ///
    /// You can use this to look for a utxo. If it exists, it will return the amount and
    /// scriptPubKey of this utxo. It returns an empty object if the utxo doesn't exist.
    /// You must have enabled block filters by setting the `blockfilters=1` option.
    fn find_tx_out(
        &self,
        txid: Txid,
        vout: u32,
        script: String,
        height_hint: u32,
    ) -> Result<Option<GetTxOut>, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getbestblockhash.md")]
    fn get_best_block_hash(&self) -> Result<BlockHash, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getblock.md")]
    fn get_block(
        &self,
        hash: BlockHash,
        verbosity: Option<u32>,
    ) -> Result<GetBlockRes, Self::Error>;

    /// Returns general information about the chain we are on
    ///
    /// This method returns a bunch of information about the chain we are on, including
    /// the current height, the best block hash, the difficulty, and whether we are
    /// currently in IBD (Initial Block Download) mode.
    fn get_blockchain_info(&self) -> Result<GetBlockchainInfo, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getblockcount.md")]
    fn get_block_count(&self) -> Result<u32, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getblockhash.md")]
    fn get_block_hash(&self, height: u32) -> Result<BlockHash, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getdeploymentinfo.md")]
    fn get_deployment_info(
        &self,
        blockhash: Option<BlockHash>,
    ) -> Result<GetDeploymentInfo, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getdifficulty.md")]
    fn get_difficulty(&self) -> Result<f64, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/gettxout.md")]
    /// TODO: `include_mempool` is not implemented yet; it depends on mempool support.
    fn get_tx_out(
        &self,
        txid: Txid,
        outpoint: u32,
        _include_mempool: bool,
    ) -> Result<Option<GetTxOut>, Self::Error>;

    /// Returns the proof that one or more transactions were included in a block
    ///
    /// This method returns the Merkle proof, showing that a transaction was included in a block.
    /// The proof is returned as a vector hexadecimal string.
    fn get_txout_proof(
        &self,
        tx_ids: &[Txid],
        blockhash: Option<BlockHash>,
    ) -> Result<GetTxOutProof, Self::Error>;

    /// Gets the current accumulator for the chain we're on
    ///
    /// This method returns the current accumulator for the chain we're on. The accumulator is
    /// a set of roots, that let's us prove that a UTXO exists in the chain. This method returns
    /// a vector of hexadecimal strings, each of which is a root in the accumulator.
    fn get_roots(&self) -> Result<Vec<String>, Self::Error>;

    /// Returns the block header for the given block hash
    ///
    /// This method returns the block header for the given block hash, as defined
    /// in the Bitcoin protocol specification. A header contains the block's version,
    /// the previous block hash, the merkle root, the timestamp, the difficulty target,
    /// and the nonce.
    fn get_block_header(
        &self,
        hash: BlockHash,
        verbosity: Option<bool>,
    ) -> Result<GetBlockHeaderRes, Self::Error>;
}

#[maybe_async::maybe_async]
pub trait WalletRpc {
    type Error: Display + Debug;

    /// Loads up a descriptor into the wallet
    ///
    /// This method loads up a descriptor into the wallet. If the rescan option is not None,
    /// the wallet will be rescanned for transactions matching the descriptor. If you have
    /// compact block filters enabled, this process will be much faster and use less bandwidth.
    /// The rescan parameter is the height at which to start the rescan, and should be at least
    /// as old as the oldest transaction this descriptor could have been used in.
    fn load_descriptor(&self, descriptor: String) -> Result<bool, Self::Error>;

    /// Returns a list of all descriptors currently loaded in the wallet
    fn list_descriptors(&self) -> Result<Vec<String>, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/rescanblockchain.md")]
    fn rescan_blockchain(
        &self,
        start: Option<u32>,
        stop: Option<u32>,
        use_timestamp: bool,
        confidence: Option<RescanConfidence>,
    ) -> Result<bool, Self::Error>;
}

#[maybe_async::maybe_async]
pub trait NetworkRpc {
    type Error: Display + Debug;

    /// Tells florestad to connect with a peer
    ///
    /// You can use this to connect with a given node, providing it's IP address and port.
    /// If the `v2transport` option is set, we won't retry connecting using the old, unencrypted
    /// P2P protocol.
    #[doc = include_str!("../../../doc/rpc/addnode.md")]
    fn add_node(
        &self,
        node: String,
        command: AddNodeCommand,
        v2transport: bool,
    ) -> Result<(), Self::Error>;

    /// Immediately disconnect from a peer.
    ///
    /// The peer can be referenced either by node_address or node_id.
    /// If referencing by node_id, an empty string must be passed as the node_address.
    fn disconnect_node(
        &self,
        node_address: String,
        node_id: Option<u32>,
    ) -> Result<(), Self::Error>;

    /// Gets information about the peers we're connected with
    ///
    /// This method returns information about the peers we're connected with. This includes
    /// the peer's IP address, the peer's version, the peer's user agent, the transport protocol
    /// and the peer's current height.
    fn get_peer_info(&self) -> Result<Vec<PeerInfo>, Self::Error>;

    /// Returns the number of peers currently connected to the node.
    fn get_connection_count(&self) -> Result<usize, Self::Error>;

    /// Returns information about the network we're connected to
    fn get_network_info(&self) -> Result<GetNetworkInfo, Self::Error>;

    /// Returns address manager statistics broken down by network.
    #[doc = include_str!("../../../doc/rpc/getaddrmaninfo.md")]
    fn get_addrman_info(&self) -> Result<GetAddrManInfo, Self::Error>;

    /// Sends a ping to all peers, checking if they are still alive
    fn ping(&self) -> Result<bool, Self::Error>;
}

#[maybe_async::maybe_async]
pub trait RawTransactionRpc {
    type Error: Display + Debug;

    /// Sends a hex-encoded transaction to the network
    ///
    /// This method sends a transaction to the network. The transaction should be encoded as a
    /// hexadecimal string. If the transaction is valid, it will be broadcast to the network, and
    /// return the transaction id. If the transaction is invalid, an error will be returned.
    fn send_raw_transaction(&self, tx: String) -> Result<Txid, Self::Error>;

    /// Gets a transaction from the blockchain
    ///
    /// This method returns a transaction that's cached in our wallet. If the verbosity flag is
    /// set to false, the transaction is returned as a hexadecimal string. If the verbosity
    /// flag is set to true, the transaction is returned as a json object.
    fn get_raw_transaction(
        &self,
        tx_id: Txid,
        verbosity: Option<u32>,
    ) -> Result<RawTxResp, Self::Error>;
}

#[maybe_async::maybe_async]
pub trait ControlRpc {
    type Error: Display + Debug;

    /// Stops the florestad process
    ///
    /// This can be used to gracefully stop the florestad process.
    fn stop(&self) -> Result<String, Self::Error>;

    /// Returns for how long florestad has been running, in seconds
    fn uptime(&self) -> Result<u64, Self::Error>;

    /// Returns statistics about Floresta's memory usage.
    ///
    /// Returns zeroed values for all runtimes that are not *-gnu or MacOS.
    fn get_memory_info(&self, mode: String) -> Result<GetMemInfoRes, Self::Error>;

    /// Returns stats about our RPC server
    fn get_rpc_info(&self) -> Result<GetRpcInfoRes, Self::Error>;
}

/// JSON-RPC parameters wrapper: supports both positional (array) and named (object) formats.
///
/// JSON-RPC allows params to be either an array `[42, "abc"]` (positional) or
/// an object `{"height": 42, "hash": "abc"}` (named). This enum deserializes
/// both forms via `#[serde(untagged)]` and exposes the underlying `Value` for
/// uniform extraction with helpers like `get_at`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Params {
    Array(Vec<serde_json::Value>),
    Object(serde_json::Map<String, serde_json::Value>),
}

impl Params {
    /// Converts into a `serde_json::Value` for use with extraction helpers like `get_at`.
    pub fn into_value(self) -> serde_json::Value {
        match self {
            Params::Array(a) => serde_json::Value::Array(a),
            Params::Object(o) => serde_json::Value::Object(o),
        }
    }
}

/// Error returned by [`RpcCommand::from_method_and_params`] when the method
/// name or parameters are invalid.
#[derive(Debug)]
pub enum RpcCommandError {
    /// The method name does not correspond to any known RPC command.
    MethodNotFound(String),
    /// A required parameter is missing.
    MissingParameter { method: String, field: String },
    /// A parameter has the wrong type.
    InvalidParameterType { method: String, detail: String },
    /// Some other deserialization error.
    Other(serde_json::Error),
}

impl RpcCommandError {
    /// Classifies an opaque `serde_json::Error` into a structured
    /// `RpcCommandError` for the given method.
    ///
    /// Serde does not expose structured error variants, so string matching
    /// on the error message is the only available approach.
    fn from_serde(method: String, err: serde_json::Error) -> Self {
        let msg = err.to_string();
        if msg.contains("missing field") {
            let field = msg
                .split("missing field `")
                .nth(1)
                .and_then(|s| s.split('`').next())
                .unwrap_or("unknown")
                .to_string();
            Self::MissingParameter { method, field }
        } else if msg.contains("invalid type") || msg.contains("invalid value") {
            Self::InvalidParameterType {
                method,
                detail: msg,
            }
        } else {
            Self::Other(err)
        }
    }
}

impl Display for RpcCommandError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MethodNotFound(m) => write!(f, "unknown method: {m}"),
            Self::MissingParameter { method, field } => {
                write!(f, "{method}: missing parameter: {field}")
            }
            Self::InvalidParameterType { method, detail } => {
                write!(f, "{method}: invalid parameter type: {detail}")
            }
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

/// Unified RPC command enum — the single source of truth for all RPC commands.
///
/// This enum uses serde's adjacently tagged representation (`tag = "method"`,
/// `content = "params"`) which produces exactly the JSON-RPC wire format:
///
/// ```json
/// {"method": "getblock", "params": {"hash": "00...", "verbosity": 1}}
/// ```
///
/// Unit variants (no params) serialize without a `params` key:
///
/// ```json
/// {"method": "getblockcount"}
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "clap", derive(clap::Subcommand))]
#[serde(tag = "method", content = "params")]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "clap", command(rename_all = "lowercase"))]
pub enum RpcCommand {
    // -- Blockchain --
    FindTxOut {
        txid: Txid,
        vout: u32,
        script: String,
        height: u32,
    },

    GetBestBlockHash,

    GetBlock {
        block_hash: BlockHash,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verbosity: Option<u32>,
    },

    GetBlockFromPeer {
        block_hash: BlockHash,
    },

    GetBlockchainInfo,

    GetBlockCount,

    GetBlockHash {
        block_height: u32,
    },

    GetDeploymentInfo {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blockhash: Option<BlockHash>,
    },

    GetDifficulty,

    GetTxOut {
        txid: Txid,
        vout: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        include_mempool: Option<bool>,
    },

    GetTxOutProof {
        #[cfg_attr(feature = "clap", arg(required = true, value_parser = crate::rpc_types::parse_json_array::<Txid>))]
        txids: Vec<Txid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block_hash: Option<BlockHash>,
    },

    GetRoots,

    GetBlockHeader {
        block_hash: BlockHash,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verbosity: Option<bool>,
    },

    // -- Wallet --
    LoadDescriptor {
        descriptor: String,
    },

    ListDescriptors,

    RescanBlockchain {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_height: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_height: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        use_timestamp: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<RescanConfidence>,
    },

    // -- Network --
    AddNode {
        node: String,
        command: AddNodeCommand,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        v2transport: Option<bool>,
    },

    DisconnectNode {
        node_address: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<u32>,
    },

    GetAddrManInfo,

    GetConnectionCount,

    GetNetworkInfo,

    GetPeerInfo,

    Ping,

    // -- RawTransactions --
    SendRawTransaction {
        hex: String,
    },

    GetRawTransaction {
        txid: Txid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        verbosity: Option<u32>,
    },

    // -- Control --
    Stop,

    Uptime,

    GetMemoryInfo {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },

    GetRpcInfo,
}

impl RpcCommand {
    /// Returns the wire-format method name (e.g. `"getblock"`).
    ///
    /// Delegates to serde serialization: the adjacently-tagged representation
    /// always produces a `"method"` key whose value is the lowercased variant name.
    pub fn method_name(&self) -> String {
        let v = serde_json::to_value(self).expect("RpcCommand always serializes");
        v["method"]
            .as_str()
            .expect("adjacently-tagged enum always has 'method' key")
            .to_string()
    }

    /// Decomposes into the method name and optional params object,
    /// ready for a JSON-RPC request.
    pub fn into_request(self) -> (String, Option<serde_json::Value>) {
        let mut obj = serde_json::to_value(self).expect("RpcCommand always serializes");
        let method = obj["method"]
            .as_str()
            .expect("adjacently-tagged enum always has 'method' key")
            .to_string();
        let params = obj.get_mut("params").map(serde_json::Value::take);
        (method, params)
    }

    /// Returns `true` if the given method name corresponds to a known RPC command.
    pub fn is_known_method(method: &str) -> bool {
        Self::from_method(method).is_some()
    }

    /// Resolves a method name to the corresponding `RpcCommand` variant
    /// **without** parsing parameters.
    ///
    /// Struct variant fields are filled with dummy defaults — the returned
    /// value is only useful for serialization introspection (e.g. `param_names`),
    /// not for reading field values.
    fn from_method(method: &str) -> Option<Self> {
        // All comparisons are lowercase to be case-insensitive.
        //
        // Optional fields use `Some(Default)` rather than `None` so that
        // serialization (used by `param_names`) emits every field name
        // instead of skipping those guarded by `skip_serializing_if`.
        match method.to_lowercase().as_str() {
            "findtxout" => Some(Self::FindTxOut {
                txid: Txid::all_zeros(),
                vout: 0,
                script: String::new(),
                height: 0,
            }),
            "getbestblockhash" => Some(Self::GetBestBlockHash),
            "getblock" => Some(Self::GetBlock {
                block_hash: BlockHash::all_zeros(),
                verbosity: Some(0),
            }),
            "getblockfrompeer" => Some(Self::GetBlockFromPeer {
                block_hash: BlockHash::all_zeros(),
            }),
            "getblockchaininfo" => Some(Self::GetBlockchainInfo),
            "getblockcount" => Some(Self::GetBlockCount),
            "getblockhash" => Some(Self::GetBlockHash { block_height: 0 }),
            "getdeploymentinfo" => Some(Self::GetDeploymentInfo {
                blockhash: Some(BlockHash::all_zeros()),
            }),
            "getdifficulty" => Some(Self::GetDifficulty),
            "gettxout" => Some(Self::GetTxOut {
                txid: Txid::all_zeros(),
                vout: 0,
                include_mempool: Some(false),
            }),
            "gettxoutproof" => Some(Self::GetTxOutProof {
                txids: Vec::new(),
                block_hash: Some(BlockHash::all_zeros()),
            }),
            "getroots" => Some(Self::GetRoots),
            "getblockheader" => Some(Self::GetBlockHeader {
                block_hash: BlockHash::all_zeros(),
                verbosity: Some(false),
            }),
            "loaddescriptor" => Some(Self::LoadDescriptor {
                descriptor: String::new(),
            }),
            "listdescriptors" => Some(Self::ListDescriptors),
            "rescanblockchain" => Some(Self::RescanBlockchain {
                start_height: Some(0),
                stop_height: Some(0),
                use_timestamp: Some(false),
                confidence: Some(RescanConfidence::Medium),
            }),
            "addnode" => Some(Self::AddNode {
                node: String::new(),
                command: AddNodeCommand::Add,
                v2transport: Some(false),
            }),
            "disconnectnode" => Some(Self::DisconnectNode {
                node_address: String::new(),
                node_id: Some(0),
            }),
            "getaddrmaninfo" => Some(Self::GetAddrManInfo),
            "getconnectioncount" => Some(Self::GetConnectionCount),
            "getnetworkinfo" => Some(Self::GetNetworkInfo),
            "getpeerinfo" => Some(Self::GetPeerInfo),
            "ping" => Some(Self::Ping),
            "sendrawtransaction" => Some(Self::SendRawTransaction { hex: String::new() }),
            "getrawtransaction" => Some(Self::GetRawTransaction {
                txid: Txid::all_zeros(),
                verbosity: Some(0),
            }),
            "stop" => Some(Self::Stop),
            "uptime" => Some(Self::Uptime),
            "getmemoryinfo" => Some(Self::GetMemoryInfo {
                mode: Some(String::new()),
            }),
            "getrpcinfo" => Some(Self::GetRpcInfo),
            _ => None,
        }
    }

    /// Returns the ordered field names for a struct variant by serializing
    /// the dummy instance from [`Self::from_method`] and extracting the keys
    /// from the `"params"` object.
    ///
    /// Unit variants (no `"params"` key) return an empty `Vec`.
    ///
    /// Requires the `preserve_order` feature on `serde_json` so that keys
    /// appear in declaration order rather than sorted alphabetically.
    fn param_names(method: &str) -> Vec<String> {
        let Some(dummy) = Self::from_method(method) else {
            return Vec::new();
        };
        let serialized = serde_json::to_value(dummy).expect("RpcCommand always serializes");
        match serialized.get("params") {
            Some(serde_json::Value::Object(map)) => map.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// Converts positional (array) parameters to named (object) parameters
    /// using the field names from [`Self::param_names`].
    ///
    /// Object params are passed through unchanged. Returns `None` if params
    /// are absent, null, or empty.
    fn to_named_params(
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Option<serde_json::Value> {
        match params {
            Some(serde_json::Value::Object(ref o)) if !o.is_empty() => {
                Some(serde_json::Value::Object(o.clone()))
            }
            Some(serde_json::Value::Array(ref arr)) if !arr.is_empty() => {
                let names = Self::param_names(method);
                let obj: serde_json::Map<String, serde_json::Value> =
                    names.into_iter().zip(arr.iter().cloned()).collect();
                if obj.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(obj))
                }
            }
            _ => None,
        }
    }

    /// Constructs an `RpcCommand` from separate method name and params fields,
    /// as they appear in a JSON-RPC request.
    ///
    /// This rebuilds the adjacently-tagged JSON `{"method": ..., "params": ...}`
    /// and lets serde parse it into the correct variant.
    ///
    /// - **Object params**: passed directly to serde for field extraction.
    /// - **Array params**: converted to an object using [`Self::param_names`]
    ///   to map positional values to their field names.
    /// - **Null / empty array / absent**: no params passed; unit variants
    ///   match directly, struct variants get default field values.
    pub fn from_method_and_params(
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<Self, RpcCommandError> {
        let method_lower = method.to_lowercase();

        if !Self::is_known_method(&method_lower) {
            return Err(RpcCommandError::MethodNotFound(method_lower));
        }

        let named_params = Self::to_named_params(&method_lower, params);
        let is_struct_variant = !Self::param_names(&method_lower).is_empty();

        let mut obj = serde_json::Map::new();
        obj.insert(
            "method".to_string(),
            serde_json::Value::String(method_lower.clone()),
        );
        if let Some(p) = named_params {
            obj.insert("params".to_string(), p);
        } else if is_struct_variant {
            // Struct variants need a `params` key even when all fields are
            // optional and none were provided — otherwise serde's adjacently-
            // tagged representation treats it as a unit variant and fails.
            obj.insert(
                "params".to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            );
        }

        serde_json::from_value(serde_json::Value::Object(obj))
            .map_err(|e| RpcCommandError::from_serde(method_lower, e))
    }
}

/// JSON-RPC request envelope that wraps an [`RpcCommand`].
///
/// Serialization uses `#[serde(flatten)]` so the `method` and `params` keys
/// from the `RpcCommand` appear at the top level alongside `jsonrpc` and `id`.
///
/// Deserialization is custom: it extracts `method` and `params` from the
/// top-level object and delegates to [`RpcCommand::from_method_and_params`],
/// which correctly handles empty arrays, null, and absent params for unit
/// variants.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JsonRpcEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<JsonRpcVersion>,
    pub id: serde_json::Value,
    #[serde(flatten)]
    pub command: RpcCommand,
}

impl<'de> serde::Deserialize<'de> for JsonRpcEnvelope {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let mut obj: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::deserialize(deserializer)?;

        let jsonrpc = obj
            .remove("jsonrpc")
            .map(|v| serde_json::from_value::<JsonRpcVersion>(v).map_err(D::Error::custom))
            .transpose()?;

        let id = obj
            .remove("id")
            .ok_or_else(|| D::Error::missing_field("id"))?;

        let method = obj
            .remove("method")
            .and_then(|v| v.as_str().map(String::from))
            .ok_or_else(|| D::Error::missing_field("method"))?;

        let params = obj.remove("params");

        let command =
            RpcCommand::from_method_and_params(&method, params).map_err(D::Error::custom)?;

        Ok(JsonRpcEnvelope {
            jsonrpc,
            id,
            command,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonRpcVersion {
    One,
    Two,
    Unknown(String),
}

impl serde::Serialize for JsonRpcVersion {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            JsonRpcVersion::One => serializer.serialize_str("1.0"),
            JsonRpcVersion::Two => serializer.serialize_str("2.0"),
            JsonRpcVersion::Unknown(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> serde::Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "1.0" => Ok(JsonRpcVersion::One),
            "2.0" => Ok(JsonRpcVersion::Two),
            _ => Ok(JsonRpcVersion::Unknown(s)),
        }
    }
}

/// Trait for dispatching an [`RpcCommand`] and getting back a JSON value.
///
/// Both the client and server implement this trait:
/// - **Client**: serializes the command into a JSON-RPC request, sends it over
///   HTTP, and returns the parsed response.
/// - **Server**: matches on the command variant and calls the appropriate
///   business-logic trait method, returning the result as a JSON value.
#[maybe_async::maybe_async]
pub trait RpcDispatch<
    Target: WalletRpc + ControlRpc + BlockchainRpc + WalletRpc + RawTransactionRpc,
>
{
    type Error: Display + Debug;

    fn dispatch(&self, cmd: RpcCommand) -> Result<serde_json::Value, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_command_unit_variant_serde_roundtrip() {
        let cmd = RpcCommand::GetBlockCount;
        let json = serde_json::to_value(&cmd).unwrap();
        assert_eq!(json["method"], "getblockcount");
        assert!(json.get("params").is_none());

        let deserialized: RpcCommand = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.method_name(), "getblockcount");
    }

    #[test]
    fn test_rpc_command_struct_variant_serde_roundtrip() {
        let cmd = RpcCommand::GetBlockHash { block_height: 42 };
        let json = serde_json::to_value(&cmd).unwrap();
        assert_eq!(json["method"], "getblockhash");
        assert_eq!(json["params"]["block_height"], 42);

        let deserialized: RpcCommand = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.method_name(), "getblockhash");
        match deserialized {
            RpcCommand::GetBlockHash { block_height } => assert_eq!(block_height, 42),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_rpc_command_optional_params() {
        let cmd = RpcCommand::GetBlock {
            block_hash: "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
                .parse()
                .unwrap(),
            verbosity: None,
        };
        let json = serde_json::to_value(&cmd).unwrap();
        assert_eq!(json["method"], "getblock");
        // verbosity is None and skip_serializing_if, so it shouldn't be present
        assert!(json["params"].get("verbosity").is_none());

        let deserialized: RpcCommand = serde_json::from_value(json).unwrap();
        match deserialized {
            RpcCommand::GetBlock { verbosity, .. } => assert!(verbosity.is_none()),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_rpc_command_from_method_and_params() {
        // Unit variant
        let cmd = RpcCommand::from_method_and_params("getblockcount", None).unwrap();
        assert_eq!(cmd.method_name(), "getblockcount");

        // Struct variant with params
        let params = serde_json::json!({"block_height": 100});
        let cmd = RpcCommand::from_method_and_params("getblockhash", Some(params)).unwrap();
        match cmd {
            RpcCommand::GetBlockHash { block_height } => assert_eq!(block_height, 100),
            _ => panic!("wrong variant"),
        }

        // Unknown method
        let result = RpcCommand::from_method_and_params("nosuchmethod", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_rpc_command_case_insensitive_parsing() {
        let cmd = RpcCommand::from_method_and_params("GetBlockCount", None).unwrap();
        assert_eq!(cmd.method_name(), "getblockcount");

        let cmd = RpcCommand::from_method_and_params("GETBLOCKCOUNT", None).unwrap();
        assert_eq!(cmd.method_name(), "getblockcount");
    }

    #[test]
    fn test_envelope_serde_roundtrip() {
        let envelope = JsonRpcEnvelope {
            jsonrpc: Some(JsonRpcVersion::Two),
            id: serde_json::json!(1),
            command: RpcCommand::GetBlockHash { block_height: 42 },
        };

        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "getblockhash");
        assert_eq!(json["params"]["block_height"], 42);

        let deserialized: JsonRpcEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.jsonrpc, Some(JsonRpcVersion::Two));
        assert_eq!(deserialized.id, serde_json::json!(1));
        match deserialized.command {
            RpcCommand::GetBlockHash { block_height } => assert_eq!(block_height, 42),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_envelope_unit_variant() {
        let envelope = JsonRpcEnvelope {
            jsonrpc: Some(JsonRpcVersion::Two),
            id: serde_json::json!(0),
            command: RpcCommand::Stop,
        };

        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(json["method"], "stop");
        assert!(json.get("params").is_none());

        let deserialized: JsonRpcEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.command.method_name(), "stop");
    }

    #[test]
    fn test_envelope_tolerates_sibling_fields() {
        // Simulate a real JSON-RPC request with extra fields alongside method/params
        let raw = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "getblockhash",
            "params": {"block_height": 10}
        });

        let envelope: JsonRpcEnvelope = serde_json::from_value(raw).unwrap();
        assert_eq!(envelope.id, serde_json::json!(42));
        match envelope.command {
            RpcCommand::GetBlockHash { block_height } => assert_eq!(block_height, 10),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_method_name_matches_serde_rename() {
        // Verify that method_name() returns the same string that serde serializes
        let commands: Vec<RpcCommand> = vec![
            RpcCommand::GetBlockCount,
            RpcCommand::GetBestBlockHash,
            RpcCommand::Stop,
            RpcCommand::Uptime,
            RpcCommand::GetRpcInfo,
            RpcCommand::Ping,
            RpcCommand::ListDescriptors,
            RpcCommand::GetRoots,
        ];

        for cmd in commands {
            let json = serde_json::to_value(&cmd).unwrap();
            let serde_method = json["method"].as_str().unwrap();
            assert_eq!(
                cmd.method_name(),
                serde_method,
                "method_name() and serde disagree for {:?}",
                cmd
            );
        }
    }

    #[test]
    fn test_params_deserialize_array() {
        let json = serde_json::json!([42, "abc"]);
        let params: Params = serde_json::from_value(json).unwrap();
        match params {
            Params::Array(a) => {
                assert_eq!(a.len(), 2);
                assert_eq!(a[0], 42);
                assert_eq!(a[1], "abc");
            }
            _ => panic!("expected Array variant"),
        }
    }

    #[test]
    fn test_params_deserialize_object() {
        let json = serde_json::json!({"height": 42, "hash": "abc"});
        let params: Params = serde_json::from_value(json).unwrap();
        match params {
            Params::Object(o) => {
                assert_eq!(o.len(), 2);
                assert_eq!(o["height"], 42);
                assert_eq!(o["hash"], "abc");
            }
            _ => panic!("expected Object variant"),
        }
    }

    #[test]
    fn test_params_into_value_roundtrip() {
        let array_json = serde_json::json!([1, 2, 3]);
        let params: Params = serde_json::from_value(array_json.clone()).unwrap();
        assert_eq!(params.into_value(), array_json);

        let obj_json = serde_json::json!({"a": 1});
        let params: Params = serde_json::from_value(obj_json.clone()).unwrap();
        assert_eq!(params.into_value(), obj_json);
    }

    #[test]
    fn test_from_method_resolves_all_commands() {
        let methods = [
            "findtxout",
            "getbestblockhash",
            "getblock",
            "getblockfrompeer",
            "getblockchaininfo",
            "getblockcount",
            "getblockhash",
            "getdeploymentinfo",
            "getdifficulty",
            "gettxout",
            "gettxoutproof",
            "getroots",
            "getblockheader",
            "loaddescriptor",
            "listdescriptors",
            "rescanblockchain",
            "addnode",
            "disconnectnode",
            "getaddrmaninfo",
            "getconnectioncount",
            "getnetworkinfo",
            "getpeerinfo",
            "ping",
            "sendrawtransaction",
            "getrawtransaction",
            "stop",
            "uptime",
            "getmemoryinfo",
            "getrpcinfo",
        ];

        for method in methods {
            let cmd = RpcCommand::from_method(method);
            assert!(cmd.is_some(), "from_method failed for {method}");
            assert_eq!(cmd.unwrap().method_name(), method);
        }
    }

    #[test]
    fn test_from_method_case_insensitive() {
        assert!(RpcCommand::from_method("GetBlockCount").is_some());
        assert!(RpcCommand::from_method("GETBLOCKCOUNT").is_some());
        assert!(RpcCommand::from_method("getblockcount").is_some());
    }

    #[test]
    fn test_from_method_unknown_returns_none() {
        assert!(RpcCommand::from_method("nosuchmethod").is_none());
        assert!(RpcCommand::from_method("").is_none());
    }

    #[test]
    fn test_is_known_method() {
        // Known methods
        assert!(RpcCommand::is_known_method("getblockcount"));
        assert!(RpcCommand::is_known_method("getblock"));
        assert!(RpcCommand::is_known_method("stop"));
        assert!(RpcCommand::is_known_method("findtxout"));

        // Case insensitive
        assert!(RpcCommand::is_known_method("GetBlockCount"));
        assert!(RpcCommand::is_known_method("GETBLOCKCOUNT"));

        // Unknown methods
        assert!(!RpcCommand::is_known_method("nosuchmethod"));
        assert!(!RpcCommand::is_known_method(""));
    }

    #[test]
    fn test_jsonrpc_version_deserialize_known_versions() {
        let v: JsonRpcVersion = serde_json::from_str(r#""1.0""#).unwrap();
        assert_eq!(v, JsonRpcVersion::One);

        let v: JsonRpcVersion = serde_json::from_str(r#""2.0""#).unwrap();
        assert_eq!(v, JsonRpcVersion::Two);
    }

    #[test]
    fn test_jsonrpc_version_deserialize_unknown_version() {
        let v: JsonRpcVersion = serde_json::from_str(r#""3.0""#).unwrap();
        assert_eq!(v, JsonRpcVersion::Unknown("3.0".to_string()));

        let v: JsonRpcVersion = serde_json::from_str(r#""not-a-version""#).unwrap();
        assert_eq!(v, JsonRpcVersion::Unknown("not-a-version".to_string()));
    }

    #[test]
    fn test_jsonrpc_version_serialize_roundtrip() {
        let json = serde_json::to_value(JsonRpcVersion::One).unwrap();
        assert_eq!(json, "1.0");

        let json = serde_json::to_value(JsonRpcVersion::Two).unwrap();
        assert_eq!(json, "2.0");

        let json = serde_json::to_value(JsonRpcVersion::Unknown("3.0".to_string())).unwrap();
        assert_eq!(json, "3.0");
    }

    #[test]
    fn test_jsonrpc_version_non_string_fails() {
        let result: std::result::Result<JsonRpcVersion, _> = serde_json::from_str("42");
        assert!(result.is_err());

        let result: std::result::Result<JsonRpcVersion, _> = serde_json::from_str("null");
        assert!(result.is_err());
    }

    #[test]
    fn test_envelope_without_jsonrpc_field() {
        // JSON-RPC 1.0 requests omit the "jsonrpc" field entirely
        let raw = serde_json::json!({
            "id": 1,
            "method": "getblockcount"
        });

        let envelope: JsonRpcEnvelope = serde_json::from_value(raw).unwrap();
        assert!(envelope.jsonrpc.is_none());
        assert_eq!(envelope.command.method_name(), "getblockcount");
    }

    #[test]
    fn test_envelope_with_unknown_jsonrpc_version() {
        let raw = serde_json::json!({
            "jsonrpc": "3.0",
            "id": 1,
            "method": "getblockcount"
        });

        let envelope: JsonRpcEnvelope = serde_json::from_value(raw).unwrap();
        assert_eq!(
            envelope.jsonrpc,
            Some(JsonRpcVersion::Unknown("3.0".to_string()))
        );
    }

    #[test]
    fn test_envelope_unit_variant_with_empty_params() {
        // This is what the jsonrpc crate sends: params as empty array
        let raw = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "getblockchaininfo",
            "params": []
        });
        let result = serde_json::from_value::<JsonRpcEnvelope>(raw);
        assert!(result.is_ok(), "Failed to deserialize: {:?}", result.err());
    }

    #[test]
    fn test_envelope_unit_variant_with_null_params() {
        let raw = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "stop",
            "params": null
        });
        let result = serde_json::from_value::<JsonRpcEnvelope>(raw);
        assert!(result.is_ok(), "Failed to deserialize: {:?}", result.err());
    }
}
