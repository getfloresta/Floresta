// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;

use bitcoin::block::Header as BlockHeader;
use bitcoin::BlockHash;
use bitcoin::Txid;
use corepc_types::v29::GetTxOut;
use serde::de::Deserialize;
use serde::de::DeserializeOwned;
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
        T: for<'a> Deserialize<'a> + DeserializeOwned + Debug;
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
    use std::cell::RefCell;
    use std::vec;

    use bitcoin::block::Version;
    use bitcoin::hashes::Hash;
    use bitcoin::CompactTarget;

    use super::*;

    struct MockRpcClient {
        method: RefCell<String>,
        params: RefCell<Vec<Value>>,
        result: RefCell<Option<Value>>,
    }

    impl MockRpcClient {
        fn set_result(&self, result: Value) {
            *self.result.borrow_mut() = Some(result);
        }
    }

    impl MockRpcClient {
        fn new() -> Self {
            Self {
                method: RefCell::new(String::new()),
                params: RefCell::new(Vec::new()),
                result: RefCell::new(None),
            }
        }
    }

    impl JsonRPCClient for MockRpcClient {
        fn call<T>(&self, method: &str, params: &[Value]) -> Result<T>
        where
            T: for<'a> Deserialize<'a> + DeserializeOwned + Debug,
        {
            *self.method.borrow_mut() = method.to_string();
            *self.params.borrow_mut() = params.to_vec();

            let result = self
                .result
                .borrow()
                .clone()
                .unwrap_or(serde_json::json!(null));

            serde_json::from_value(result)
                .map_err(|_| Error::Api(Value::String("Result parsing error".to_string())))
        }
    }

    #[test]
    fn test_get_block_filter_params() {
        let client = MockRpcClient::new();
        let expected_result = "abcdef1234567890".to_string();
        client.set_result(Value::String(expected_result.clone()));

        let height = 500u32;

        let result = client.get_block_filter(height).unwrap();
        assert_eq!(result, expected_result);

        let expected_params = rpc_params([height.into()]);

        assert_eq!(*client.method.borrow(), "getblockfilter");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_blockchain_info() {
        let client = MockRpcClient::new();
        let get_blockchain_info_res = GetBlockchainInfoRes {
            chain: "main".to_string(),
            best_block: "best_block".to_string(),
            difficulty: 123456789,
            height: 1000,
            ibd: false,
            latest_block_time: 1234567890,
            latest_work: "latest_work".to_string(),
            leaf_count: 1000,
            progress: 0.9999,
            root_count: 1000,
            root_hashes: vec!["root_hash".to_string()],
            validated: 10,
        };
        let expected_result = serde_json::to_value(get_blockchain_info_res).unwrap();
        client.set_result(expected_result.clone());

        let result = client.get_blockchain_info().unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        assert_eq!(result_serialized, expected_result);

        assert_eq!(*client.method.borrow(), "getblockchaininfo");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_get_best_block_hash() {
        let client = MockRpcClient::new();
        let expected_result = BlockHash::all_zeros();
        client.set_result(Value::String(expected_result.to_string()));

        let result = client.get_best_block_hash().unwrap();

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "getbestblockhash");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_get_block_hash() {
        let client = MockRpcClient::new();
        let expected_result = BlockHash::all_zeros();
        client.set_result(Value::String(expected_result.to_string()));

        let height = 100u32;

        let result = client.get_block_hash(height).unwrap();

        let expected_params = rpc_params([height.into()]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "getblockhash");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_block_header() {
        let client = MockRpcClient::new();
        let block_header = BlockHeader {
            version: Version::from_consensus(21),
            prev_blockhash: BlockHash::all_zeros(),
            merkle_root: bitcoin::TxMerkleNode::all_zeros(),
            time: 1234567890,
            bits: CompactTarget::from_consensus(21),
            nonce: 0,
        };
        let expected_result = serde_json::to_value(block_header).unwrap();
        client.set_result(expected_result.clone());

        let block_hash = BlockHash::all_zeros();

        let result = client.get_block_header(block_hash).unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        let expected_params = rpc_params([block_hash.into()]);

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getblockheader");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_transaction() {
        let client = MockRpcClient::new();
        let expected_result = Value::String("transaction".to_string());
        client.set_result(expected_result.clone());

        let tx_id = Txid::all_zeros();
        let verbosity = Some(true);

        let result = client.get_transaction(tx_id, verbosity).unwrap();

        let expected_params = rpc_params([tx_id.into(), true.into()]); // verbosity is always passed

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "gettransaction");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test with None verbosity (defaults to false)
        let _ = client.get_transaction(tx_id, None);

        let expected_params = rpc_params([tx_id.into(), None::<bool>.into()]);

        assert_eq!(*client.method.borrow(), "gettransaction");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_txout_proof() {
        let client = MockRpcClient::new();
        let expected_result = "proof".to_string();
        client.set_result(Value::String(expected_result.clone()));

        let txids = vec![Txid::all_zeros()];
        let blockhash = Some(BlockHash::all_zeros());

        let result = client.get_txout_proof(txids.clone(), blockhash).unwrap();

        let expected_params = rpc_params([txids.clone().into(), blockhash.into()]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "gettxoutproof");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test without blockhash parameter
        let _ = client.get_txout_proof(txids.clone(), None);

        let expected_params = rpc_params([txids.clone().into(), None::<BlockHash>.into()]);

        assert_eq!(*client.method.borrow(), "gettxoutproof");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_load_descriptor() {
        let client = MockRpcClient::new();
        let expected_result = true;
        client.set_result(Value::Bool(expected_result));

        let descriptor = "wpkh([aabbccdd/84'/0'/0']xpub6CatD...)".to_string();

        let result = client.load_descriptor(descriptor.clone()).unwrap();

        let expected_params = rpc_params([descriptor.clone().into()]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "loaddescriptor");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_rescanblockchain() {
        let client = MockRpcClient::new();
        let expected_result = true;
        client.set_result(Value::Bool(expected_result));

        let start_height = 100u32;
        let stop_height = 200u32;
        let use_timestamp = true;
        let confidence = RescanConfidence::High;

        let result = client
            .rescanblockchain(
                Some(start_height),
                Some(stop_height),
                use_timestamp,
                confidence.clone(),
            )
            .unwrap();

        let expected_params = [
            Value::Number(Number::from(start_height)),
            Value::Number(Number::from(stop_height)),
            Value::Bool(use_timestamp),
            serde_json::to_value(&confidence).expect("RescanConfidence implements Ser/De"),
        ];

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "rescanblockchain");
        assert_eq!(client.params.borrow().len(), 4);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test with None parameters
        let _ = client.rescanblockchain(None, None, use_timestamp, confidence.clone());

        let expected_params = [
            Value::Number(Number::from(0u32)), // Default start height
            Value::Number(Number::from(0u32)), // Default stop height
            Value::Bool(use_timestamp),
            serde_json::to_value(&confidence).expect("RescanConfidence implements Ser/De"),
        ];

        assert_eq!(*client.method.borrow(), "rescanblockchain");
        assert_eq!(client.params.borrow().len(), 4); // All parameters are passed, but start/stop heights are defaulted
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_block_count() {
        let client = MockRpcClient::new();
        let expected_result = 1000u32;
        client.set_result(Value::Number(Number::from(expected_result)));

        let result = client.get_block_count().unwrap();

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "getblockcount");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_send_raw_transaction() {
        let client = MockRpcClient::new();
        let expected_result = Txid::all_zeros();
        client.set_result(Value::String(expected_result.to_string()));

        let tx = "02000000010123456789abcdef...".to_string();

        let result = client.send_raw_transaction(tx.clone()).unwrap();

        let expected_params = rpc_params([tx.clone().into()]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "sendrawtransaction");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_roots() {
        let client = MockRpcClient::new();
        let expected_result = vec!["root1".to_string(), "root2".to_string()];
        client.set_result(Value::Array(
            expected_result
                .iter()
                .map(|root| Value::String(root.clone()))
                .collect(),
        ));

        let result = client.get_roots().unwrap();

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "getroots");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_get_peer_info() {
        let client = MockRpcClient::new();
        let peer_info = vec![PeerInfo {
            address: "address".to_string(),
            id: 1,
            initial_height: 100,
            kind: "kind".to_string(),
            services: "services".to_string(),
            state: "state".to_string(),
            transport_protocol: "transport_protocol".to_string(),
            user_agent: "user_agent".to_string(),
        }];
        let expected_result = serde_json::to_value(&peer_info).unwrap();
        client.set_result(expected_result.clone());

        let result = client.get_peer_info().unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getpeerinfo");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_get_connection_count() {
        let client = MockRpcClient::new();
        let expected_result = 8usize;
        client.set_result(Value::Number(Number::from(expected_result)));

        let result = client.get_connection_count().unwrap();

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "getconnectioncount");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_get_block() {
        let client = MockRpcClient::new();
        let get_block = GetBlockRes::Zero("block".to_string());
        let expected_result = serde_json::to_value(&get_block).unwrap();
        client.set_result(expected_result.clone());

        let block_hash = BlockHash::all_zeros();
        let verbosity = Some(1u32);

        let result = client.get_block(block_hash, verbosity).unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        let expected_params = rpc_params([block_hash.into(), verbosity.into()]);

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getblock");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test without verbosity parameter
        let _ = client.get_block(block_hash, None);

        let expected_params = rpc_params([block_hash.into(), None::<u32>.into()]);

        assert_eq!(*client.method.borrow(), "getblock");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_tx_out() {
        let client = MockRpcClient::new();
        let expected_result = GetTxOut {
            best_block: "best_block".to_string(),
            confirmations: 10,
            value: 0.1,
            coinbase: false,
            script_pubkey: corepc_types::ScriptPubkey {
                address: Some("address".to_string()),
                asm: "asm".to_string(),
                hex: "hex".to_string(),
                type_: "type".to_string(),
                addresses: None,
                descriptor: None,
                required_signatures: None,
            },
        };
        client.set_result(serde_json::to_value(&expected_result).unwrap());

        let tx_id = Txid::all_zeros();
        let outpoint = 0u32;

        let result = client.get_tx_out(tx_id, outpoint).unwrap();

        let expected_params = rpc_params([tx_id.into(), outpoint.into()]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "gettxout");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_stop() {
        let client = MockRpcClient::new();
        let _ = client.stop();

        assert_eq!(*client.method.borrow(), "stop");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_add_node() {
        let client = MockRpcClient::new();
        let expected_result = serde_json::json!({"success": true});
        client.set_result(expected_result.clone());

        let node = "192.168.1.1:8333".to_string();
        let command = AddNodeCommand::Add;
        let v2transport = Some(true);

        let result = client
            .add_node(node.clone(), command.clone(), v2transport)
            .unwrap();

        let expected_params = rpc_params([
            node.clone().into(),
            command.clone().to_string().into(),
            v2transport.into(),
        ]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "addnode");
        assert_eq!(client.params.borrow().len(), 3);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test without v2transport parameter
        let _ = client.add_node(node.clone(), command.clone(), None);

        let expected_params = rpc_params([
            node.clone().into(),
            command.to_string().into(),
            None::<bool>.into(),
        ]);

        assert_eq!(*client.method.borrow(), "addnode");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_disconnect_node() {
        let client = MockRpcClient::new();
        let expected_result = serde_json::json!({"success": true});
        client.set_result(expected_result.clone());

        let node_address = "192.168.1.1".to_string();
        let node_id = Some(1u32);

        let result = client
            .disconnect_node(node_address.clone(), node_id)
            .unwrap();

        let expected_params = rpc_params([node_address.clone().into(), node_id.into()]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "disconnectnode");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test with None node_id
        let _ = client.disconnect_node(node_address.clone(), None);

        let expected_params = rpc_params([node_address.clone().into(), None::<u32>.into()]);

        assert_eq!(*client.method.borrow(), "disconnectnode");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_find_tx_out() {
        let client = MockRpcClient::new();
        let expected_result = serde_json::json!({"success": true});
        client.set_result(expected_result.clone());

        let txid = Txid::all_zeros();
        let outpoint = 0;
        let script = "76a91488ac".to_string();
        let mut height_hint = Some(100);

        let result = client
            .find_tx_out(txid, outpoint, script.clone(), height_hint)
            .unwrap();

        let expetecd_params = rpc_params([
            txid.into(),
            outpoint.into(),
            script.clone().into(),
            height_hint.into(),
        ]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "findtxout");
        assert_eq!(client.params.borrow().len(), 4);
        assert_eq!(*client.params.borrow(), expetecd_params);

        // Test that None height hint is filtered out
        height_hint = None;

        let _ = client.find_tx_out(txid, outpoint, script.clone(), height_hint);

        let expetecd_params = rpc_params([
            txid.into(),
            outpoint.into(),
            script.clone().into(),
            height_hint.into(),
        ]);

        assert_eq!(*client.method.borrow(), "findtxout");
        assert_eq!(client.params.borrow().len(), 3);
        assert_eq!(*client.params.borrow(), expetecd_params);
    }

    #[test]
    fn test_get_memory_info() {
        let client = MockRpcClient::new();
        let memory_info = GetMemInfoRes::MallocInfo("Malloc".to_string());
        let expected_result = serde_json::to_value(&memory_info).unwrap();
        client.set_result(expected_result.clone());

        let mode = Some("all".to_string());

        let result = client.get_memory_info(mode.clone()).unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        let expected_params = rpc_params([mode.clone().into()]);

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getmemoryinfo");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test without mode parameter
        let _ = client.get_memory_info(None);

        let expected_params = rpc_params([None::<String>.into()]);

        assert_eq!(*client.method.borrow(), "getmemoryinfo");
        assert_eq!(client.params.borrow().len(), 0);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_rpc_info() {
        let client = MockRpcClient::new();
        let rpc_info = GetRpcInfoRes {
            logpath: "logpath".to_string(),
            active_commands: Vec::new(),
        };
        let expected_result = serde_json::to_value(&rpc_info).unwrap();
        client.set_result(expected_result.clone());

        let result = client.get_rpc_info().unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getrpcinfo");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_uptime() {
        let client = MockRpcClient::new();
        let expected_result = 3600u32;
        client.set_result(Value::Number(Number::from(expected_result)));

        let result = client.uptime().unwrap();

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "uptime");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_list_descriptors() {
        let client = MockRpcClient::new();
        let expect_ed_result = vec!["desc1".to_string(), "desc2".to_string()];
        client.set_result(Value::Array(
            expect_ed_result
                .iter()
                .map(|desc| Value::String(desc.clone()))
                .collect(),
        ));

        let result = client.list_descriptors().unwrap();

        assert_eq!(result, expect_ed_result);
        assert_eq!(*client.method.borrow(), "listdescriptors");
        assert!(client.params.borrow().is_empty());
    }

    #[test]
    fn test_ping() {
        let client = MockRpcClient::new();
        client.ping().unwrap();

        assert_eq!(*client.method.borrow(), "ping");
        assert!(client.params.borrow().is_empty());
    }

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
