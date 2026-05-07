// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;
use std::vec;

use bitcoin::BlockHash;
use bitcoin::Txid;
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
        T: for<'a> serde::de::Deserialize<'a> + serde::de::DeserializeOwned + Debug;
}

impl<T: JsonRPCClient> BlockchainRpc for T {
    type Error = rpc_types::Error;

    fn find_tx_out(
        &self,
        txid: Txid,
        vout: u32,
        script: String,
        height_hint: u32,
    ) -> Result<Option<GetTxOut>> {
        self.call(
            &RpcMethods::FindTxOut,
            &[
                Value::String(txid.to_string()),
                Value::Number(Number::from(vout)),
                Value::String(script),
                Value::Number(Number::from(height_hint)),
            ],
        )
    }

    fn get_best_block_hash(&self) -> Result<BlockHash> {
        self.call(&RpcMethods::GetBestBlockHash, &[])
    }

    fn get_block(&self, hash: BlockHash, verbosity: Option<u32>) -> Result<GetBlockRes> {
        let mut params = vec![Value::String(hash.to_string())];
        if let Some(verbosity) = verbosity {
            params.push(Value::Number(Number::from(verbosity)));
        }
        self.call(&RpcMethods::GetBlock, &params)
    }

    fn get_block_from_peer(&self, hash: BlockHash) -> Result<()> {
        self.call(
            &RpcMethods::GetBlockFromPeer,
            &[Value::String(hash.to_string())],
        )
    }

    fn get_blockchain_info(&self) -> Result<GetBlockchainInfo> {
        self.call(&RpcMethods::GetBlockchainInfo, &[])
    }

    fn get_block_count(&self) -> Result<u32> {
        self.call(
            &RpcMethods::GetBlockCount,
            &[Value::Number(Number::from(0))],
        )
    }

    fn get_block_hash(&self, height: u32) -> Result<BlockHash> {
        self.call(
            &RpcMethods::GetBlockHash,
            &[Value::Number(Number::from(height))],
        )
    }

    fn get_deployment_info(&self, blockhash: Option<BlockHash>) -> Result<GetDeploymentInfo> {
        let params = match blockhash {
            Some(h) => vec![Value::String(h.to_string())],
            None => vec![],
        };
        self.call(&RpcMethods::GetDeploymentInfo, &params)
    }

    fn get_difficulty(&self) -> Result<f64> {
        self.call(&RpcMethods::GetDifficulty, &[])
    }

    fn get_tx_out(
        &self,
        txid: Txid,
        outpoint: u32,
        _include_mempool: bool,
    ) -> Result<Option<GetTxOut>> {
        let result: serde_json::Value = self.call(
            &RpcMethods::GetTxOut,
            &[
                Value::String(txid.to_string()),
                Value::Number(Number::from(outpoint)),
            ],
        )?;
        if result.is_null() {
            return Ok(None);
        }
        serde_json::from_value(result)
            .map(Some)
            .map_err(Error::Serde)
    }

    fn get_txout_proof(&self, tx_ids: &[Txid], blockhash: Option<BlockHash>) -> Result<String> {
        let params: Vec<Value> = match blockhash {
            Some(blockhash) => vec![
                serde_json::to_value(tx_ids).map_err(Error::Serde)?,
                Value::String(blockhash.to_string()),
            ],
            None => {
                let txids = serde_json::to_value(tx_ids).map_err(Error::Serde)?;
                vec![txids]
            }
        };
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
        let mut params = vec![Value::String(hash.to_string())];
        if let Some(verbosity) = verbosity {
            params.push(Value::Bool(verbosity));
        }
        self.call(&RpcMethods::GetBlockHeader, &params)
    }
}

impl<T: JsonRPCClient> WalletRpc for T {
    type Error = rpc_types::Error;

    fn load_descriptor(&self, descriptor: String) -> Result<bool> {
        self.call(&RpcMethods::LoadDescriptor, &[Value::String(descriptor)])
    }

    fn list_descriptors(&self) -> Result<Vec<String>> {
        self.call(&RpcMethods::ListDescriptors, &[])
    }

    fn rescan_blockchain(
        &self,
        start_height: Option<u32>,
        stop_height: Option<u32>,
        use_timestamp: bool,
        confidence: Option<RescanConfidence>,
    ) -> Result<bool> {
        let start_height = start_height.unwrap_or(0u32);
        let stop_height = stop_height.unwrap_or(0u32);
        let confidence = confidence.unwrap_or(RescanConfidence::Medium);

        self.call(
            &RpcMethods::RescanBlockchain,
            &[
                Value::Number(Number::from(start_height)),
                Value::Number(Number::from(stop_height)),
                Value::Bool(use_timestamp),
                serde_json::to_value(&confidence).map_err(Error::Serde)?,
            ],
        )
    }
}

impl<T: JsonRPCClient> NetworkRpc for T {
    type Error = rpc_types::Error;

    fn add_node(&self, node: String, command: AddNodeCommand, v2transport: bool) -> Result<()> {
        self.call(
            &RpcMethods::AddNode,
            &[
                Value::String(node),
                Value::String(command.to_string()),
                Value::Bool(v2transport),
            ],
        )
    }

    fn disconnect_node(&self, node_address: String, node_id: Option<u32>) -> Result<()> {
        match node_id {
            Some(node_id) => self.call(
                &RpcMethods::DisconnectNode,
                &[
                    Value::String(node_address),
                    Value::Number(Number::from(node_id)),
                ],
            ),
            None => self.call(&RpcMethods::DisconnectNode, &[Value::String(node_address)]),
        }
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

    fn ping(&self) -> Result<bool> {
        self.call(&RpcMethods::Ping, &[])
    }
}

impl<T: JsonRPCClient> RawTransactionRpc for T {
    type Error = rpc_types::Error;

    fn send_raw_transaction(&self, tx: String) -> Result<Txid> {
        self.call(&RpcMethods::SendRawTransaction, &[Value::String(tx)])
    }

    fn get_raw_transaction(
        &self,
        tx_id: Txid,
        verbosity: Option<u8>,
    ) -> Result<GetRawTransactionRes> {
        let mut params = vec![Value::String(tx_id.to_string())];

        if let Some(verbosity) = verbosity {
            params.push(Value::Number(Number::from(verbosity)));
        }

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

    fn get_memory_info(&self, mode: String) -> Result<GetMemInfoRes> {
        self.call(&RpcMethods::GetMemoryInfo, &[Value::String(mode)])
    }

    fn get_rpc_info(&self) -> Result<GetRpcInfoRes> {
        self.call(&RpcMethods::GetRpcInfo, &[])
    }
}
