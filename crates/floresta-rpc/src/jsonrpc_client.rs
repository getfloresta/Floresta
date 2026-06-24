// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;

use serde::Deserialize;
use serde_json::Value;

use crate::rpc_interfaces::RpcCommand;
use crate::rpc_types;

type Result<T> = std::result::Result<T, rpc_types::Error>;

/// A JSON-RPC client backed by the `jsonrpc` crate.
#[derive(Debug)]
pub struct Client(jsonrpc::Client);

/// Configuration struct for JSON-RPC client.
pub struct JsonRPCConfig {
    pub url: String,
    pub user: Option<String>,
    pub pass: Option<String>,
}

impl Client {
    /// Create a new Client with a URL.
    pub fn new(url: String) -> Self {
        let client =
            jsonrpc::Client::simple_http(&url, None, None).expect("Failed to create client");
        Self(client)
    }

    /// Create a new Client with a configuration.
    pub fn new_with_config(config: JsonRPCConfig) -> Self {
        let client =
            jsonrpc::Client::simple_http(&config.url, config.user.clone(), config.pass.clone())
                .expect("Failed to create client");
        Self(client)
    }

    /// Make a raw RPC call, deserializing the response into `Response`.
    pub fn rpc_call<Response>(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<Response>
    where
        Response: for<'a> serde::de::Deserialize<'a> + Debug,
    {
        let raw = params
            .as_ref()
            .map(serde_json::value::to_raw_value)
            .transpose()?;
        let req = self.0.build_request(method, raw.as_deref());
        let resp = self
            .0
            .send_request(req)
            .map_err(crate::rpc_types::Error::from);
        Ok(resp?.result()?)
    }

    /// Sends an [`RpcCommand`] as a JSON-RPC request, deserializing the response.
    fn call_command<Response>(&self, command: RpcCommand) -> Result<Response>
    where
        Response: for<'a> serde::de::Deserialize<'a> + Debug,
    {
        let (method, params) = command.into_request();
        self.rpc_call(&method, params)
    }

    /// Dispatch an [`RpcCommand`], returning the result as a [`serde_json::Value`].
    ///
    /// This is the main entry point for the CLI: it takes a parsed command,
    /// sends the corresponding JSON-RPC request, and returns the raw result.
    pub fn dispatch(&self, command: RpcCommand) -> Result<Value> {
        self.call_command(command)
    }
}

// -- Trait implementations (sync only) --
//
// These implement the individual RPC traits so that the Client can be used
// directly with typed method calls (e.g. `client.get_block_count()`).
// They are only available in sync mode because the traits use `maybe_async`,
// which changes return types to `impl Future` in async mode.

#[cfg(not(feature = "async"))]
mod trait_impls {
    use bitcoin::BlockHash;
    use bitcoin::Txid;

    use super::*;
    use crate::rpc_interfaces::BlockchainRpc;
    use crate::rpc_interfaces::ControlRpc;
    use crate::rpc_interfaces::NetworkRpc;
    use crate::rpc_interfaces::RawTransactionRpc;
    use crate::rpc_interfaces::WalletRpc;
    use crate::rpc_types::*;

    impl BlockchainRpc for Client {
        type Error = rpc_types::Error;

        fn find_tx_out(
            &self,
            txid: Txid,
            vout: u32,
            script: String,
            height: u32,
        ) -> Result<Option<GetTxOut>> {
            self.call_command(RpcCommand::FindTxOut {
                txid,
                vout,
                script,
                height,
            })
        }

        fn get_best_block_hash(&self) -> Result<BlockHash> {
            self.call_command(RpcCommand::GetBestBlockHash)
        }

        fn get_block(&self, block_hash: BlockHash, verbosity: Option<u32>) -> Result<GetBlockRes> {
            self.call_command(RpcCommand::GetBlock {
                block_hash,
                verbosity,
            })
        }

        fn get_blockchain_info(&self) -> Result<GetBlockchainInfo> {
            self.call_command(RpcCommand::GetBlockchainInfo)
        }

        fn get_block_count(&self) -> Result<u32> {
            self.call_command(RpcCommand::GetBlockCount)
        }

        fn get_block_hash(&self, height: u32) -> Result<BlockHash> {
            self.call_command(RpcCommand::GetBlockHash {
                block_height: height,
            })
        }

        fn get_deployment_info(&self, blockhash: Option<BlockHash>) -> Result<GetDeploymentInfo> {
            self.call_command(RpcCommand::GetDeploymentInfo { blockhash })
        }

        fn get_difficulty(&self) -> Result<f64> {
            self.call_command(RpcCommand::GetDifficulty)
        }

        fn get_tx_out(
            &self,
            txid: Txid,
            outpoint: u32,
            _include_mempool: bool,
        ) -> Result<Option<GetTxOut>> {
            let result: serde_json::Value = self.call_command(RpcCommand::GetTxOut {
                txid,
                vout: outpoint,
                include_mempool: None,
            })?;
            if result.is_null() {
                return Ok(None);
            }
            serde_json::from_value(result)
                .map(Some)
                .map_err(Error::Serde)
        }

        fn get_txout_proof(
            &self,
            tx_ids: &[Txid],
            blockhash: Option<BlockHash>,
        ) -> Result<GetTxOutProof> {
            self.call_command(RpcCommand::GetTxOutProof {
                txids: tx_ids.to_vec(),
                block_hash: blockhash,
            })
        }

        fn get_roots(&self) -> Result<Vec<String>> {
            self.call_command(RpcCommand::GetRoots)
        }

        fn get_block_header(
            &self,
            block_hash: BlockHash,
            verbosity: Option<bool>,
        ) -> Result<GetBlockHeaderRes> {
            self.call_command(RpcCommand::GetBlockHeader {
                block_hash,
                verbosity,
            })
        }
    }

    impl WalletRpc for Client {
        type Error = rpc_types::Error;

        fn load_descriptor(&self, descriptor: String) -> Result<bool> {
            self.call_command(RpcCommand::LoadDescriptor { descriptor })
        }

        fn list_descriptors(&self) -> Result<Vec<String>> {
            self.call_command(RpcCommand::ListDescriptors)
        }

        fn rescan_blockchain(
            &self,
            start_height: Option<u32>,
            stop_height: Option<u32>,
            use_timestamp: bool,
            confidence: Option<RescanConfidence>,
        ) -> Result<bool> {
            self.call_command(RpcCommand::RescanBlockchain {
                start_height,
                stop_height,
                use_timestamp: Some(use_timestamp),
                confidence,
            })
        }
    }

    impl NetworkRpc for Client {
        type Error = rpc_types::Error;

        fn add_node(&self, node: String, command: AddNodeCommand, v2transport: bool) -> Result<()> {
            self.call_command(RpcCommand::AddNode {
                node,
                command,
                v2transport: Some(v2transport),
            })
        }

        fn disconnect_node(&self, node_address: String, node_id: Option<u32>) -> Result<()> {
            self.call_command(RpcCommand::DisconnectNode {
                node_address,
                node_id,
            })
        }

        fn get_peer_info(&self) -> Result<Vec<PeerInfo>> {
            self.call_command(RpcCommand::GetPeerInfo)
        }

        fn get_connection_count(&self) -> Result<usize> {
            self.call_command(RpcCommand::GetConnectionCount)
        }

        fn get_network_info(&self) -> Result<GetNetworkInfo> {
            self.call_command(RpcCommand::GetNetworkInfo)
        }

        fn get_addrman_info(&self) -> Result<GetAddrManInfo> {
            self.call_command(RpcCommand::GetAddrManInfo)
        }

        fn ping(&self) -> Result<bool> {
            self.call_command(RpcCommand::Ping)
        }
    }

    impl RawTransactionRpc for Client {
        type Error = rpc_types::Error;

        fn send_raw_transaction(&self, tx: String) -> Result<Txid> {
            self.call_command(RpcCommand::SendRawTransaction { hex: tx })
        }

        fn get_raw_transaction(&self, tx_id: Txid, verbosity: Option<u32>) -> Result<RawTxResp> {
            self.call_command(RpcCommand::GetRawTransaction {
                txid: tx_id,
                verbosity,
            })
        }
    }

    impl ControlRpc for Client {
        type Error = rpc_types::Error;

        fn stop(&self) -> Result<String> {
            self.call_command(RpcCommand::Stop)
        }

        fn uptime(&self) -> Result<u64> {
            self.call_command(RpcCommand::Uptime)
        }

        fn get_memory_info(&self, mode: String) -> Result<GetMemInfoRes> {
            self.call_command(RpcCommand::GetMemoryInfo { mode: Some(mode) })
        }

        fn get_rpc_info(&self) -> Result<GetRpcInfoRes> {
            self.call_command(RpcCommand::GetRpcInfo)
        }
    }
}

/// Struct to represent a JSON-RPC response.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse<Res> {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<Res>,
    pub error: Option<serde_json::Value>,
}
