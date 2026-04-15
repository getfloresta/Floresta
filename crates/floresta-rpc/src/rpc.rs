// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;

use bitcoin::block::Header as BlockHeader;
use bitcoin::BlockHash;
use bitcoin::Txid;
use corepc_types::v29::GetTxOut;
use serde::Serialize;
use serde_json::Number;
use serde_json::Value;

use crate::rpc_types;
use crate::rpc_types::*;

type Result<T> = std::result::Result<T, rpc_types::Error>;

/// A trait specifying all possible methods for floresta's json-rpc
pub trait FlorestaRPC {
    /// Get the BIP158 filter for a given block height
    ///
    /// BIP158 filters are a compact representation of the set of transactions in a block,
    /// designed for efficient light client synchronization. This method returns the filter
    /// for a given block height, encoded as a hexadecimal string.
    /// You need to have enabled block filters by setting the `blockfilters=1` option
    fn get_block_filter(&self, height: u32) -> Result<String>;
    /// Returns general information about the chain we are on
    ///
    /// This method returns a bunch of information about the chain we are on, including
    /// the current height, the best block hash, the difficulty, and whether we are
    /// currently in IBD (Initial Block Download) mode.
    fn get_blockchain_info(&self) -> Result<GetBlockchainInfoRes>;
    /// Returns the hash of the best (tip) block in the most-work fully-validated chain.
    fn get_best_block_hash(&self) -> Result<BlockHash>;
    /// Returns the hash of the block at the given height
    ///
    /// This method returns the hash of the block at the given height. If the height is
    /// invalid, an error is returned.
    fn get_block_hash(&self, height: u32) -> Result<BlockHash>;
    /// Returns the block header for the given block hash
    ///
    /// This method returns the block header for the given block hash, as defined
    /// in the Bitcoin protocol specification. A header contains the block's version,
    /// the previous block hash, the merkle root, the timestamp, the difficulty target,
    /// and the nonce.
    fn get_block_header(&self, hash: BlockHash) -> Result<BlockHeader>;
    /// Gets a transaction from the blockchain
    ///
    /// This method returns a transaction that's cached in our wallet. If the verbosity flag is
    /// set to false, the transaction is returned as a hexadecimal string. If the verbosity
    /// flag is set to true, the transaction is returned as a json object.
    fn get_transaction(&self, tx_id: Txid, verbosity: Option<bool>) -> Result<Value>;
    /// Returns the proof that one or more transactions were included in a block
    ///
    /// This method returns the Merkle proof, showing that a transaction was included in a block.
    /// The pooof is returned as a vector hexadecimal string.
    fn get_txout_proof(&self, txids: Vec<Txid>, blockhash: Option<BlockHash>) -> Option<String>;
    /// Loads up a descriptor into the wallet
    ///
    /// This method loads up a descriptor into the wallet. If the rescan option is not None,
    /// the wallet will be rescanned for transactions matching the descriptor. If you have
    /// compact block filters enabled, this process will be much faster and use less bandwidth.
    /// The rescan parameter is the height at which to start the rescan, and should be at least
    /// as old as the oldest transaction this descriptor could have been used in.
    fn load_descriptor(&self, descriptor: String) -> Result<bool>;

    #[doc = include_str!("../../../doc/rpc/rescanblockchain.md")]
    fn rescanblockchain(
        &self,
        start_block: Option<u32>,
        stop_block: Option<u32>,
        use_timestamp: bool,
        confidence: RescanConfidence,
    ) -> Result<bool>;

    /// Returns the current height of the blockchain
    fn get_block_count(&self) -> Result<u32>;
    /// Sends a hex-encoded transaction to the network
    ///
    /// This method sends a transaction to the network. The transaction should be encoded as a
    /// hexadecimal string. If the transaction is valid, it will be broadcast to the network, and
    /// return the transaction id. If the transaction is invalid, an error will be returned.
    fn send_raw_transaction(&self, tx: String) -> Result<Txid>;
    /// Gets the current accumulator for the chain we're on
    ///
    /// This method returns the current accumulator for the chain we're on. The accumulator is
    /// a set of roots, that let's us prove that a UTXO exists in the chain. This method returns
    /// a vector of hexadecimal strings, each of which is a root in the accumulator.
    fn get_roots(&self) -> Result<Vec<String>>;
    /// Gets information about the peers we're connected with
    ///
    /// This method returns information about the peers we're connected with. This includes
    /// the peer's IP address, the peer's version, the peer's user agent, the transport protocol
    /// and the peer's current height.
    fn get_peer_info(&self) -> Result<Vec<PeerInfo>>;
    /// Returns the number of peers currently connected to the node.
    fn get_connection_count(&self) -> Result<usize>;
    /// Returns a block, given a block hash
    ///
    /// This method returns a block, given a block hash. If the verbosity flag is 0, the block
    /// is returned as a hexadecimal string. If the verbosity flag is 1, the block is returned
    /// as a json object.
    fn get_block(&self, hash: BlockHash, verbosity: Option<u32>) -> Result<GetBlockRes>;
    /// Return a cached transaction output
    ///
    /// This method returns a cached transaction output. If the output is not in the cache,
    /// or is spent, an empty object is returned. If you want to find a utxo that's not in
    /// the cache, you can use the findtxout method.
    fn get_tx_out(&self, tx_id: Txid, outpoint: u32) -> Result<GetTxOut>;
    /// Stops the florestad process
    ///
    /// This can be used to gracefully stop the florestad process.
    fn stop(&self) -> Result<String>;
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
        v2transport: Option<bool>,
    ) -> Result<Value>;
    /// Immediately disconnect from a peer.
    ///
    /// The peer can be referenced either by node_address or node_id.
    /// If referencing by node_id, an empty string must be passed as the node_address.
    fn disconnect_node(&self, node_address: String, node_id: Option<u32>) -> Result<Value>;
    /// Finds an specific utxo in the chain
    ///
    /// You can use this to look for a utxo. If it exists, it will return the amount and
    /// scriptPubKey of this utxo. It returns an empty object if the utxo doesn't exist.
    /// You must have enabled block filters by setting the `blockfilters=1` option.
    fn find_tx_out(
        &self,
        tx_id: Txid,
        outpoint: u32,
        script: String,
        height_hint: Option<u32>,
    ) -> Result<Value>;
    /// Returns statistics about Floresta's memory usage.
    ///
    /// Returns zeroed values for all runtimes that are not *-gnu or MacOS.
    fn get_memory_info(&self, mode: Option<String>) -> Result<GetMemInfoRes>;
    /// Returns stats about our RPC server
    fn get_rpc_info(&self) -> Result<GetRpcInfoRes>;
    /// Returns for how long florestad has been running, in seconds
    fn uptime(&self) -> Result<u32>;
    /// Returns a list of all descriptors currently loaded in the wallet
    fn list_descriptors(&self) -> Result<Vec<String>>;
    /// Sends a ping to all peers, checking if they are still alive
    fn ping(&self) -> Result<()>;
}

/// Since the workflow for jsonrpc is the same for all methods, we can implement a trait
/// that will let us call any method on the client, and then implement the methods on any
/// client that implements this trait.
pub trait JsonRPCClient: Sized {
    /// Calls a method on the client
    ///
    /// This should call the appropriated rpc method and return a parsed response or error.
    fn call<T>(&self, method: &str, params: &[Value]) -> Result<T>
    where
        T: for<'a> serde::de::Deserialize<'a> + serde::de::DeserializeOwned + Debug;
}

impl<T: JsonRPCClient> FlorestaRPC for T {
    fn find_tx_out(
        &self,
        tx_id: Txid,
        outpoint: u32,
        script: String,
        height_hint: Option<u32>,
    ) -> Result<Value> {
        let params = rpc_params([
            tx_id.into(),
            outpoint.into(),
            script.into(),
            height_hint.into(),
        ]);

        self.call("findtxout", &params)
    }

    fn uptime(&self) -> Result<u32> {
        self.call("uptime", &[])
    }

    fn get_memory_info(&self, mode: Option<String>) -> Result<GetMemInfoRes> {
        let params = rpc_params([mode.into()]);
        self.call("getmemoryinfo", &params)
    }

    fn get_rpc_info(&self) -> Result<GetRpcInfoRes> {
        self.call("getrpcinfo", &[])
    }

    fn add_node(
        &self,
        node: String,
        command: AddNodeCommand,
        v2transport: Option<bool>,
    ) -> Result<Value> {
        let params = rpc_params([node.into(), command.to_string().into(), v2transport.into()]);

        self.call("addnode", &params)
    }

    fn disconnect_node(&self, node_address: String, node_id: Option<u32>) -> Result<Value> {
        let params = rpc_params([node_address.into(), node_id.into()]);

        self.call("disconnectnode", &params)
    }

    fn stop(&self) -> Result<String> {
        self.call("stop", &[])
    }

    fn rescanblockchain(
        &self,
        start_height: Option<u32>,
        stop_height: Option<u32>,
        use_timestamp: bool,
        confidence: RescanConfidence,
    ) -> Result<bool> {
        let start_height = start_height.unwrap_or(0u32);

        let stop_height = stop_height.unwrap_or(0u32);

        self.call(
            "rescanblockchain",
            &[
                Value::Number(Number::from(start_height)),
                Value::Number(Number::from(stop_height)),
                Value::Bool(use_timestamp),
                serde_json::to_value(&confidence).expect("RescanConfidence implements Ser/De"),
            ],
        )
    }

    fn get_roots(&self) -> Result<Vec<String>> {
        self.call("getroots", &[])
    }

    fn get_block(&self, hash: BlockHash, verbosity: Option<u32>) -> Result<GetBlockRes> {
        let params = rpc_params([hash.into(), verbosity.into()]);

        self.call("getblock", &params)
    }

    fn get_block_count(&self) -> Result<u32> {
        self.call("getblockcount", &[])
    }

    fn get_tx_out(&self, tx_id: Txid, outpoint: u32) -> Result<GetTxOut> {
        let params = rpc_params([tx_id.into(), outpoint.into()]);

        let result: serde_json::Value = self.call("gettxout", &params)?;
        if result.is_null() {
            return Err(Error::TxOutNotFound);
        }

        serde_json::from_value(result).map_err(Error::Serde)
    }

    fn get_txout_proof(&self, txids: Vec<Txid>, blockhash: Option<BlockHash>) -> Option<String> {
        let params = rpc_params([txids.into(), blockhash.into()]);
        self.call("gettxoutproof", &params).ok()
    }

    fn get_peer_info(&self) -> Result<Vec<PeerInfo>> {
        self.call("getpeerinfo", &[])
    }

    fn get_connection_count(&self) -> Result<usize> {
        self.call("getconnectioncount", &[])
    }

    fn get_best_block_hash(&self) -> Result<BlockHash> {
        self.call("getbestblockhash", &[])
    }

    fn get_block_hash(&self, height: u32) -> Result<BlockHash> {
        let params = rpc_params([height.into()]);
        self.call("getblockhash", &params)
    }

    fn get_transaction(&self, tx_id: Txid, verbosity: Option<bool>) -> Result<Value> {
        let params = rpc_params([tx_id.into(), verbosity.into()]);
        self.call("gettransaction", &params)
    }

    fn load_descriptor(&self, descriptor: String) -> Result<bool> {
        let params = rpc_params([descriptor.into()]);
        self.call("loaddescriptor", &params)
    }

    fn get_block_filter(&self, height: u32) -> Result<String> {
        let params = rpc_params([height.into()]);
        self.call("getblockfilter", &params)
    }

    fn get_block_header(&self, hash: BlockHash) -> Result<BlockHeader> {
        let params = rpc_params([hash.into()]);
        self.call("getblockheader", &params)
    }

    fn get_blockchain_info(&self) -> Result<GetBlockchainInfoRes> {
        self.call("getblockchaininfo", &[])
    }

    fn send_raw_transaction(&self, tx: String) -> Result<Txid> {
        let params = rpc_params([tx.into()]);
        self.call("sendrawtransaction", &params)
    }

    fn list_descriptors(&self) -> Result<Vec<String>> {
        self.call("listdescriptors", &[])
    }

    fn ping(&self) -> Result<()> {
        self.call("ping", &[])
    }
}

enum RpcArg {
    Value(Value),
    Optional(Option<Value>),
}

impl From<String> for RpcArg {
    fn from(v: String) -> Self {
        RpcArg::Value(Value::String(v))
    }
}

impl From<&str> for RpcArg {
    fn from(v: &str) -> Self {
        RpcArg::Value(Value::String(v.to_owned()))
    }
}

impl From<bool> for RpcArg {
    fn from(v: bool) -> Self {
        RpcArg::Value(Value::Bool(v))
    }
}

impl From<u32> for RpcArg {
    fn from(v: u32) -> Self {
        RpcArg::Value(Value::Number(Number::from(v)))
    }
}

impl From<Txid> for RpcArg {
    fn from(value: Txid) -> Self {
        RpcArg::Value(Value::String(value.to_string()))
    }
}

impl From<BlockHash> for RpcArg {
    fn from(value: BlockHash) -> Self {
        RpcArg::Value(Value::String(value.to_string()))
    }
}

impl<T: Serialize> From<Vec<T>> for RpcArg {
    fn from(v: Vec<T>) -> Self {
        let values: Vec<Value> = v
            .into_iter()
            .filter_map(|item| serde_json::to_value(item).ok())
            .collect();
        RpcArg::Value(Value::Array(values))
    }
}

impl<T: Serialize> From<Option<T>> for RpcArg {
    fn from(v: Option<T>) -> Self {
        RpcArg::Optional(v.and_then(|x| serde_json::to_value(x).ok()))
    }
}

fn rpc_params(args: impl IntoIterator<Item = RpcArg>) -> Vec<Value> {
    args.into_iter()
        .filter_map(|arg| match arg {
            RpcArg::Value(v) => Some(v),
            RpcArg::Optional(v) => v,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_arg_from_string() {
        let arg = RpcArg::from("test".to_string());
        match arg {
            RpcArg::Value(Value::String(s)) => assert_eq!(s, "test"),
            _ => panic!("Expected RpcArg::Value"),
        }
    }

    #[test]
    fn test_rpc_arg_from_bool() {
        let arg = RpcArg::from(true);
        match arg {
            RpcArg::Value(Value::Bool(b)) => assert!(b),
            _ => panic!("Expected RpcArg::Value with bool"),
        }
    }

    #[test]
    fn test_rpc_arg_from_option_some() {
        let opt: Option<u32> = Some(42);
        let arg = RpcArg::from(opt);
        match arg {
            RpcArg::Optional(Some(Value::Number(n))) => {
                assert_eq!(n.as_u64(), Some(42));
            }
            _ => panic!("Expected RpcArg::Optional(Some(...))"),
        }
    }

    #[test]
    fn test_rpc_arg_from_option_none() {
        let opt: Option<u32> = None;
        let arg = RpcArg::from(opt);
        match arg {
            RpcArg::Optional(None) => {}
            _ => panic!("Expected RpcArg::Optional(None)"),
        }
    }

    #[test]
    fn test_rpc_params_filters_nones() {
        let params = rpc_params([
            "node1".into(),
            true.into(),
            Some(123u32).into(),
            None::<u32>.into(),
        ]);

        // Should have only 3 elements (None was filtered)
        assert_eq!(params.len(), 3);
        assert!(matches!(params[0], Value::String(ref s) if s == "node1"));
        assert!(matches!(params[1], Value::Bool(true)));
        assert!(matches!(params[2], Value::Number(_)));
    }

    #[test]
    fn test_rpc_params_all_none() {
        let params = rpc_params([None::<u32>.into(), None::<String>.into()]);

        assert_eq!(params.len(), 0);
    }
}
