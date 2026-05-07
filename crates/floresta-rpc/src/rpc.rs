// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;

use bitcoin::BlockHash;
use bitcoin::Txid;
use serde::Serialize;
use serde::de::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Number;
use serde_json::Value;

use crate::rpc_interfaces::BlockchainRpc;
use crate::rpc_interfaces::ControlRpc;
use crate::rpc_interfaces::NetworkRpc;
use crate::rpc_interfaces::RawTransactionRpc;
use crate::rpc_interfaces::RpcMethods;
use crate::rpc_interfaces::WalletRpc;
use crate::rpc_types;
use crate::rpc_types::*;

type Result<T> = std::result::Result<T, rpc_types::Error>;

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

impl<T: JsonRPCClient> BlockchainRpc for T {
    type Error = rpc_types::Error;

    fn find_tx_out(
        &self,
        txid: Txid,
        vout: u32,
        script: String,
        height_hint: Option<u32>,
    ) -> Result<Option<GetTxOut>> {
        let params = rpc_params([txid.into(), vout.into(), script.into(), height_hint.into()]);

        self.call(&RpcMethods::FindTxOut, &params)
    }

    fn get_best_block_hash(&self) -> Result<BlockHash> {
        self.call(&RpcMethods::GetBestBlockHash, &[])
    }

    fn get_block(&self, hash: BlockHash, verbosity: Option<u32>) -> Result<GetBlockRes> {
        let params = rpc_params([hash.into(), verbosity.into()]);

        self.call(&RpcMethods::GetBlock, &params)
    }

    fn get_block_from_peer(&self, hash: BlockHash) -> Result<()> {
        let params = rpc_params([hash.into()]);

        self.call(&RpcMethods::GetBlockFromPeer, &params)
    }

    fn get_blockchain_info(&self) -> Result<GetBlockchainInfo> {
        self.call(&RpcMethods::GetBlockchainInfo, &[])
    }

    fn get_block_count(&self) -> Result<u32> {
        self.call(&RpcMethods::GetBlockCount, &[])
    }

    fn get_block_hash(&self, height: u32) -> Result<BlockHash> {
        let params = rpc_params([height.into()]);

        self.call(&RpcMethods::GetBlockHash, &params)
    }

    fn get_deployment_info(&self, blockhash: Option<BlockHash>) -> Result<GetDeploymentInfo> {
        let params = rpc_params([blockhash.into()]);
        self.call(&RpcMethods::GetDeploymentInfo, &params)
    }

    fn get_difficulty(&self) -> Result<f64> {
        self.call(&RpcMethods::GetDifficulty, &[])
    }

    fn get_tx_out(
        &self,
        txid: Txid,
        outpoint: u32,
        include_mempool: Option<bool>,
    ) -> Result<Option<GetTxOut>> {
        let params = rpc_params([txid.into(), outpoint.into(), include_mempool.into()]);

        let result: serde_json::Value = self.call(&RpcMethods::GetTxOut, &params)?;
        if result.is_null() {
            return Ok(None);
        }
        serde_json::from_value(result)
            .map(Some)
            .map_err(Error::Serde)
    }

    fn get_txout_proof(&self, tx_ids: &[Txid], blockhash: Option<BlockHash>) -> Result<String> {
        let params = rpc_params([tx_ids.to_vec().into(), blockhash.into()]);

        self.call(&RpcMethods::GetTxOutProof, &params)
    }

    fn get_roots(&self) -> Result<Vec<String>> {
        self.call(&RpcMethods::GetRoots, &[])
    }

    fn get_block_header(
        &self,
        hash: BlockHash,
        verbosity: Option<bool>,
    ) -> Result<GetBlockHeaderRes> {
        let params = rpc_params([hash.into(), verbosity.into()]);

        self.call(&RpcMethods::GetBlockHeader, &params)
    }
}

impl<T: JsonRPCClient> WalletRpc for T {
    type Error = rpc_types::Error;

    fn load_descriptor(&self, descriptor: String) -> Result<bool> {
        let params = rpc_params([descriptor.into()]);

        self.call(&RpcMethods::LoadDescriptor, &params)
    }

    fn list_descriptors(&self) -> Result<Vec<String>> {
        self.call(&RpcMethods::ListDescriptors, &[])
    }

    fn rescan_blockchain(
        &self,
        start_height: Option<u32>,
        stop_height: Option<u32>,
        use_timestamp: Option<bool>,
        confidence: Option<RescanConfidence>,
    ) -> Result<bool> {
        let params = rpc_params([
            start_height.into(),
            stop_height.into(),
            use_timestamp.into(),
            confidence.into(),
        ]);

        self.call(&RpcMethods::RescanBlockchain, &params)
    }
}

impl<T: JsonRPCClient> NetworkRpc for T {
    type Error = rpc_types::Error;

    fn add_node(
        &self,
        node: String,
        command: AddNodeCommand,
        v2transport: Option<bool>,
    ) -> Result<()> {
        let params = rpc_params([node.into(), command.to_string().into(), v2transport.into()]);

        self.call(&RpcMethods::AddNode, &params)
    }

    fn disconnect_node(&self, node_address: String, node_id: Option<u32>) -> Result<()> {
        let params = rpc_params([node_address.into(), node_id.into()]);

        self.call(&RpcMethods::DisconnectNode, &params)
    }

    fn get_peer_info(&self) -> Result<Vec<PeerInfo>> {
        self.call(&RpcMethods::GetPeerInfo, &[])
    }

    fn get_connection_count(&self) -> Result<usize> {
        self.call(&RpcMethods::GetConnectionCount, &[])
    }

    fn get_network_info(&self) -> Result<GetNetworkInfo> {
        self.call(&RpcMethods::GetNetworkInfo, &[])
    }

    fn get_addrman_info(&self) -> Result<GetAddrManInfo> {
        self.call(&RpcMethods::GetAddrManInfo, &[])
    }

    fn ping(&self) -> Result<()> {
        self.call(&RpcMethods::Ping, &[])
    }
}

impl<T: JsonRPCClient> RawTransactionRpc for T {
    type Error = rpc_types::Error;

    fn send_raw_transaction(&self, tx: String) -> Result<Txid> {
        let params = rpc_params([tx.into()]);

        self.call(&RpcMethods::SendRawTransaction, &params)
    }

    fn get_raw_transaction(
        &self,
        tx_id: Txid,
        verbosity: Option<u8>,
    ) -> Result<GetRawTransactionRes> {
        let params = rpc_params([tx_id.into(), verbosity.into()]);

        self.call(&RpcMethods::GetRawTransaction, &params)
    }
}

impl<T: JsonRPCClient> ControlRpc for T {
    type Error = rpc_types::Error;

    fn stop(&self) -> Result<String> {
        self.call(&RpcMethods::Stop, &[])
    }

    fn uptime(&self) -> Result<u64> {
        self.call(&RpcMethods::Uptime, &[])
    }

    fn get_memory_info(&self, mode: Option<&str>) -> Result<GetMemInfoRes> {
        let params = rpc_params([mode.into()]);

        self.call(&RpcMethods::GetMemoryInfo, &params)
    }

    fn get_rpc_info(&self) -> Result<GetRpcInfoRes> {
        self.call(&RpcMethods::GetRpcInfo, &[])
    }
}

enum RpcArg {
    Value(Value),
    Optional(Option<Value>),
}

impl From<String> for RpcArg {
    fn from(v: String) -> Self {
        Self::Value(Value::String(v))
    }
}

impl From<&str> for RpcArg {
    fn from(v: &str) -> Self {
        Self::Value(Value::String(v.to_owned()))
    }
}

impl From<bool> for RpcArg {
    fn from(v: bool) -> Self {
        Self::Value(Value::Bool(v))
    }
}

impl From<u32> for RpcArg {
    fn from(v: u32) -> Self {
        Self::Value(Value::Number(Number::from(v)))
    }
}

impl From<Txid> for RpcArg {
    fn from(value: Txid) -> Self {
        Self::Value(Value::String(value.to_string()))
    }
}

impl From<BlockHash> for RpcArg {
    fn from(value: BlockHash) -> Self {
        Self::Value(Value::String(value.to_string()))
    }
}

impl<T: Serialize> From<Vec<T>> for RpcArg {
    fn from(v: Vec<T>) -> Self {
        let values: Vec<Value> = v
            .into_iter()
            .filter_map(|item| serde_json::to_value(item).ok())
            .collect();
        Self::Value(Value::Array(values))
    }
}

impl<T: Serialize> From<Option<T>> for RpcArg {
    fn from(v: Option<T>) -> Self {
        Self::Optional(v.and_then(|x| serde_json::to_value(x).ok()))
    }
}

impl From<Value> for RpcArg {
    fn from(v: Value) -> Self {
        Self::Value(v)
    }
}

fn rpc_params(args: impl IntoIterator<Item = RpcArg>) -> Vec<Value> {
    args.into_iter()
        .map(|arg| match arg {
            RpcArg::Value(v) => v,
            RpcArg::Optional(Some(v)) => v,
            RpcArg::Optional(None) => Value::Null,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::vec;

    use bitcoin::hashes::Hash;

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
    fn test_get_blockchain_info() {
        let client = MockRpcClient::new();
        let get_blockchain_info_res = GetBlockchainInfo {
            chain: "main".to_string(),
            blocks: 1000,
            headers: 1000,
            best_block_hash: "".to_string(),
            difficulty: 1.0,
            automatic_pruning: None,
            bits: "1d00ffff".to_string(),
            chain_work: "".to_string(),
            initial_block_download: false,
            median_time: 0,
            prune_height: None,
            prune_target_size: None,
            pruned: false,
            signet_challenge: None,
            size_on_disk: 0,
            target: "1d00ffff".to_string(),
            time: 0,
            verification_progress: 1.0,
            warnings: vec![],
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
        let client: MockRpcClient = MockRpcClient::new();
        let block_header = GetBlockHeaderRes::Raw("Header".to_string());
        let expected_result = serde_json::to_value(block_header).unwrap();
        client.set_result(expected_result.clone());

        let block_hash = BlockHash::all_zeros();

        let result = client.get_block_header(block_hash, None).unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        let expected_params = rpc_params([block_hash.into(), None::<bool>.into()]);

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getblockheader");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        let result = client.get_block_header(block_hash, Some(true)).unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        let expected_params = rpc_params([block_hash.into(), Some(true).into()]);

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getblockheader");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_transaction() {
        let client = MockRpcClient::new();
        let expected_result: GetRawTransactionRes =
            GetRawTransactionRes::Zero("transactionhex".to_string());
        let expected_result_serialize = serde_json::to_value(expected_result).unwrap();
        client.set_result(expected_result_serialize.clone());

        let tx_id = Txid::all_zeros();
        let mut verbosity = Some(1);

        let result = client.get_raw_transaction(tx_id, verbosity).unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        let expected_params = rpc_params([tx_id.into(), verbosity.into()]); // verbosity is always passed

        assert_eq!(result_serialized, expected_result_serialize);
        assert_eq!(*client.method.borrow(), "getrawtransaction");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        verbosity = None;
        let _ = client.get_raw_transaction(tx_id, None);

        let expected_params = rpc_params([tx_id.into(), verbosity.into()]);

        assert_eq!(*client.method.borrow(), "getrawtransaction");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_txout_proof() {
        let client = MockRpcClient::new();
        let expected_result = "proof".to_string();
        client.set_result(Value::String(expected_result.clone()));

        let txids = vec![Txid::all_zeros()];
        let blockhash = Some(BlockHash::all_zeros());

        let result = client.get_txout_proof(&txids, blockhash).unwrap();

        let expected_params = rpc_params([txids.clone().into(), blockhash.into()]);

        assert_eq!(result, expected_result);
        assert_eq!(*client.method.borrow(), "gettxoutproof");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test without blockhash parameter
        let _ = client.get_txout_proof(&txids, None);

        let expected_params = rpc_params([txids.into(), None::<BlockHash>.into()]);

        assert_eq!(*client.method.borrow(), "gettxoutproof");
        assert_eq!(client.params.borrow().len(), 2);
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
    fn test_rescan_blockchain() {
        let client = MockRpcClient::new();
        let expected_result = true;
        client.set_result(Value::Bool(expected_result));

        let start_height = 100u32;
        let stop_height = 200u32;
        let use_timestamp = true;
        let confidence = RescanConfidence::High;

        let result = client
            .rescan_blockchain(
                Some(start_height),
                Some(stop_height),
                Some(use_timestamp),
                Some(confidence.clone()),
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
        let _ = client.rescan_blockchain(None, None, None, None);

        let expected_params = [Value::Null, Value::Null, Value::Null, Value::Null];

        assert_eq!(*client.method.borrow(), "rescanblockchain");
        assert_eq!(client.params.borrow().len(), 4);
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
            bip152_hb_from: false,
            bip152_hb_to: false,
            inbound: true,
            permissions: vec![],
            relay_txs: true,
            services_names: vec![],
            time_offset: 1,
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
        assert_eq!(client.params.borrow().len(), 2);
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
            script_pubkey: corepc_types::ScriptPubKey {
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
        let include_mempool = Some(false);

        let result = client.get_tx_out(tx_id, outpoint, include_mempool).unwrap();

        let expected_params = rpc_params([tx_id.into(), outpoint.into(), include_mempool.into()]);

        assert_eq!(result, Some(expected_result));
        assert_eq!(*client.method.borrow(), "gettxout");
        assert_eq!(client.params.borrow().len(), 3);
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
        client.set_result(serde_json::json!(null));

        let node = "192.168.1.1:8333".to_string();
        let command = AddNodeCommand::Add;
        let v2transport = Some(true);

        client
            .add_node(node.clone(), command.clone(), v2transport)
            .unwrap();

        let expected_params = rpc_params([
            node.clone().into(),
            command.clone().to_string().into(),
            v2transport.into(),
        ]);

        assert_eq!(*client.method.borrow(), "addnode");
        assert_eq!(client.params.borrow().len(), 3);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test without v2transport parameter
        client
            .add_node(node.clone(), command.clone(), None)
            .unwrap();

        let expected_params = rpc_params([
            node.clone().into(),
            command.to_string().into(),
            None::<bool>.into(),
        ]);

        assert_eq!(*client.method.borrow(), "addnode");
        assert_eq!(client.params.borrow().len(), 3);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_disconnect_node() {
        let client = MockRpcClient::new();
        client.set_result(serde_json::json!(null));

        let node_address = "192.168.1.1".to_string();
        let node_id = Some(1u32);

        client
            .disconnect_node(node_address.clone(), node_id)
            .unwrap();

        let expected_params = rpc_params([node_address.clone().into(), node_id.into()]);

        assert_eq!(*client.method.borrow(), "disconnectnode");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test with None node_id
        client.disconnect_node(node_address.clone(), None).unwrap();

        let expected_params = rpc_params([node_address.clone().into(), None::<u32>.into()]);

        assert_eq!(*client.method.borrow(), "disconnectnode");
        assert_eq!(client.params.borrow().len(), 2);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_find_tx_out() {
        let client = MockRpcClient::new();
        let expected_result = GetTxOut {
            best_block: "best_block".to_string(),
            confirmations: 10,
            value: 0.1,
            coinbase: false,
            script_pubkey: corepc_types::ScriptPubKey {
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

        assert_eq!(result, Some(expected_result));
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
        assert_eq!(client.params.borrow().len(), 4);
        assert_eq!(*client.params.borrow(), expetecd_params);
    }

    #[test]
    fn test_get_memory_info() {
        let client = MockRpcClient::new();
        let memory_info = GetMemInfoRes::MallocInfo("Malloc".to_string());
        let expected_result = serde_json::to_value(&memory_info).unwrap();
        client.set_result(expected_result.clone());

        let mode = Some("all");

        let result = client.get_memory_info(mode).unwrap();
        let result_serialized = serde_json::to_value(result).unwrap();

        let expected_params = rpc_params([mode.into()]);

        assert_eq!(result_serialized, expected_result);
        assert_eq!(*client.method.borrow(), "getmemoryinfo");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);

        // Test without mode parameter
        let _ = client.get_memory_info(None);

        let expected_params = rpc_params([None::<String>.into()]);

        assert_eq!(*client.method.borrow(), "getmemoryinfo");
        assert_eq!(client.params.borrow().len(), 1);
        assert_eq!(*client.params.borrow(), expected_params);
    }

    #[test]
    fn test_get_rpc_info() {
        let client = MockRpcClient::new();
        let rpc_info = GetRpcInfoRes {
            logpath: "logpath".to_string().into(),
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
        let expected_result = 3600u64;
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
    fn test_rpc_params_preserves_null_for_none() {
        let params = rpc_params([
            "node1".into(),
            true.into(),
            Some(123u32).into(),
            None::<u32>.into(),
            None::<String>.into(),
            "node2".into(),
            Some(321u32).into(),
        ]);

        // Should have only 3 elements (None was filtered)
        assert_eq!(params.len(), 7);
        assert!(matches!(params[0], Value::String(ref s) if s == "node1"));
        assert!(matches!(params[1], Value::Bool(true)));
        assert!(matches!(&params[2], Value::Number(n) if n.as_u64() == Some(123)));
        assert!(matches!(params[3], Value::Null));
        assert!(matches!(params[4], Value::Null));
        assert!(matches!(params[5], Value::String(ref s) if s == "node2"));
        assert!(matches!(&params[6], Value::Number(n) if n.as_u64() == Some(321)));
    }

    #[test]
    fn test_rpc_params_all_none() {
        let params = rpc_params([None::<u32>.into(), None::<String>.into()]);

        assert_eq!(params.len(), 2);
        assert!(matches!(params[0], Value::Null));
        assert!(matches!(params[1], Value::Null));
    }
}
