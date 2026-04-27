// SPDX-License-Identifier: MIT OR Apache-2.0

use core::net::SocketAddr;
use std::collections::HashMap;
use std::slice;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::Method;
use axum::http::Response;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Json;
use axum::Router;
use bitcoin::consensus::deserialize;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::ecdsa::Signature as EcdsaSignature;
use bitcoin::hashes::hex::FromHex;
use bitcoin::hex::DisplayHex;
use bitcoin::taproot::Signature as TaprootSignature;
use bitcoin::Address;
use bitcoin::Network;
use bitcoin::ScriptBuf;
use bitcoin::Transaction;
use bitcoin::TxIn;
use bitcoin::TxOut;
use bitcoin::Txid;
use floresta_chain::ThreadSafeChain;
use floresta_common::parse_descriptors;
use floresta_compact_filters::flat_filters_store::FlatFiltersStore;
use floresta_compact_filters::network_filters::NetworkFilters;
use floresta_watch_only::kv_database::KvDatabase;
use floresta_watch_only::AddressCache;
use floresta_watch_only::CachedTransaction;
use floresta_wire::node_interface::NodeInterface;
use serde_json::json;
use serde_json::Value;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::debug;
use tracing::error;
use tracing::info;

use super::res::JsonRpcError;
use super::res::RawTxJson;
use super::res::RpcError;
use super::res::ScriptPubKeyJson;
use super::res::ScriptSigJson;
use super::res::TxInJson;
use super::res::TxOutJson;
use crate::json_rpc::request::arg_parser::get_bool;
use crate::json_rpc::request::arg_parser::get_hash;
use crate::json_rpc::request::arg_parser::get_hashes_array;
use crate::json_rpc::request::arg_parser::get_numeric;
use crate::json_rpc::request::arg_parser::get_optional_field;
use crate::json_rpc::request::arg_parser::get_string;
use crate::json_rpc::request::RpcRequest;
use crate::json_rpc::res::RescanConfidence;

pub(super) struct InflightRpc {
    pub method: String,
    pub when: Instant,
}

/// Utility trait to ensure that the chain implements all the necessary traits
///
/// Instead of using this very complex trait bound declaration on every impl block
/// and function, this trait makes sure everything we need is implemented.
pub trait RpcChain: ThreadSafeChain + Clone {}

impl<T> RpcChain for T where T: ThreadSafeChain + Clone {}

pub struct RpcImpl<Blockchain: RpcChain> {
    pub(super) block_filter_storage: Option<Arc<NetworkFilters<FlatFiltersStore>>>,
    pub(super) network: Network,
    pub(super) chain: Blockchain,
    pub(super) wallet: Arc<AddressCache<KvDatabase>>,
    pub(super) node: NodeInterface,
    pub(super) kill_signal: Arc<RwLock<bool>>,
    pub(super) inflight: Arc<RwLock<HashMap<Value, InflightRpc>>>,
    pub(super) log_path: String,
    pub(super) start_time: Instant,
}

type Result<T> = std::result::Result<T, JsonRpcError>;

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    fn get_raw_transaction(&self, tx_id: Txid, verbosity: u32) -> Result<Value> {
        if verbosity > 1 {
            return Err(JsonRpcError::InvalidVerbosityLevel);
        }

        let tx = self
            .wallet
            .get_transaction(&tx_id)
            .ok_or(JsonRpcError::TxNotFound)?;

        match verbosity {
            0 => serde_json::to_value(serialize_hex(&tx.tx))
                .map_err(|e| JsonRpcError::Decode(e.to_string())),
            1 => serde_json::to_value(self.make_raw_transaction(tx))
                .map_err(|e| JsonRpcError::Decode(e.to_string())),
            _ => return Err(JsonRpcError::InvalidVerbosityLevel),
        }
    }

    fn load_descriptor(&self, descriptor: String) -> Result<bool> {
        let desc = slice::from_ref(&descriptor);
        let mut parsed = parse_descriptors(desc)?;

        // It's ok to unwrap because we know there is at least one element in the vector
        let addresses = parsed.pop().unwrap();
        let addresses = (0..100)
            .map(|index| {
                let address = addresses
                    .at_derivation_index(index)
                    .unwrap()
                    .script_pubkey();
                self.wallet.cache_address(address.clone());
                address
            })
            .collect::<Vec<_>>();

        debug!("Rescanning with block filters for addresses: {addresses:?}");

        let addresses = self.wallet.get_cached_addresses();
        let wallet = self.wallet.clone();
        if self.block_filter_storage.is_none() {
            return Err(JsonRpcError::InInitialBlockDownload);
        };

        let cfilters = self.block_filter_storage.as_ref().unwrap().clone();
        let node = self.node.clone();
        let chain = self.chain.clone();

        tokio::task::spawn(Self::rescan_with_block_filters(
            addresses, chain, wallet, cfilters, node, None, None,
        ));

        self.wallet.push_descriptor(&descriptor)?;
        debug!("Descriptor pushed: {descriptor}");

        Ok(true)
    }

    fn rescan_blockchain(
        &self,
        start: Option<u32>,
        stop: Option<u32>,
        use_timestamp: bool,
        confidence: Option<RescanConfidence>,
    ) -> Result<bool> {
        let (start_height, stop_height) =
            self.get_rescan_interval(use_timestamp, start, stop, confidence)?;

        if stop_height != 0 && start_height >= stop_height {
            // When stop height is a non zero value it needs atleast to be greater than start_height.
            return Err(JsonRpcError::InvalidRescanVal);
        }

        // if we are on ibd, we don't have any filters to rescan
        if self.chain.is_in_ibd() {
            return Err(JsonRpcError::InInitialBlockDownload);
        }

        let addresses = self.wallet.get_cached_addresses();

        if addresses.is_empty() {
            return Err(JsonRpcError::NoAddressesToRescan);
        }

        let wallet = self.wallet.clone();

        if self.block_filter_storage.is_none() {
            return Err(JsonRpcError::NoBlockFilters);
        };

        let cfilters = self.block_filter_storage.as_ref().unwrap().clone();

        let node = self.node.clone();

        let chain = self.chain.clone();

        tokio::task::spawn(Self::rescan_with_block_filters(
            addresses,
            chain,
            wallet,
            cfilters,
            node,
            (start_height != 0).then_some(start_height), // Its ugly but to maintain the API here its necessary to recast to a Option.
            (stop_height != 0).then_some(stop_height),
        ));
        Ok(true)
    }

    async fn send_raw_transaction(&self, tx: String) -> Result<Txid> {
        let tx_hex = Vec::from_hex(&tx).map_err(|_| JsonRpcError::InvalidHex)?;
        let tx: Transaction =
            deserialize(&tx_hex).map_err(|e| JsonRpcError::Decode(e.to_string()))?;

        Ok(self
            .node
            .broadcast_transaction(tx)
            .await
            .map_err(|e| JsonRpcError::Node(e.to_string()))??)
    }
}

async fn handle_json_rpc_request(
    req: RpcRequest,
    state: Arc<RpcImpl<impl RpcChain>>,
) -> Result<serde_json::Value> {
    let RpcRequest {
        jsonrpc,
        method,
        params,
        id,
    } = req;

    if let Some(version) = jsonrpc {
        if !["1.0", "2.0"].contains(&version.as_str()) {
            return Err(JsonRpcError::InvalidRequest);
        }
    }

    state.inflight.write().await.insert(
        id.clone(),
        InflightRpc {
            method: method.clone(),
            when: Instant::now(),
        },
    );

    match method.as_str() {
        // blockchain
        "getbestblockhash" => {
            let hash = state.get_best_block_hash()?;
            Ok(serde_json::to_value(hash).unwrap())
        }

        "getblock" => {
            let hash = get_hash(&params, 0, "block_hash")?;
            // Default value in case of missing parameter is 1
            let verbosity: u8 =
                get_optional_field(&params, 1, "verbosity", get_numeric)?.unwrap_or(1);

            state
                .get_block(hash, verbosity)
                .await
                .map(|v| serde_json::to_value(v).expect("GetBlockRes implements serde"))
        }

        "getblockchaininfo" => state
            .get_blockchain_info()
            .map(|v| serde_json::to_value(v).unwrap()),

        "getblockcount" => state
            .get_block_count()
            .map(|v| serde_json::to_value(v).unwrap()),

        "getblockfrompeer" => {
            let hash = get_hash(&params, 0, "block_hash")?;

            state.get_block(hash, 0).await?;

            Ok(Value::Null)
        }

        "getblockhash" => {
            let height = get_numeric(&params, 0, "block_height")?;
            state
                .get_block_hash(height)
                .map(|h| serde_json::to_value(h).unwrap())
        }

        "getblockheader" => {
            let hash = get_hash(&params, 0, "block_hash")?;
            state
                .get_block_header(hash)
                .map(|h| serde_json::to_value(h).unwrap())
        }

        "gettxout" => {
            let txid = get_hash(&params, 0, "txid")?;
            let vout = get_numeric(&params, 1, "vout")?;
            let include_mempool =
                get_optional_field(&params, 2, "include_mempool", get_bool)?.unwrap_or(false);

            state
                .get_tx_out(txid, vout, include_mempool)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "gettxoutproof" => {
            let txids = get_hashes_array(&params, 0, "txids")?;
            let block_hash = get_optional_field(&params, 1, "block_hash", get_hash)?;

            Ok(serde_json::to_value(
                state
                    .get_txout_proof(&txids, block_hash)
                    .await?
                    .0
                    .to_lower_hex_string(),
            )
            .expect("GetTxOutProof implements serde"))
        }

        "getrawtransaction" => {
            let txid = get_hash(&params, 0, "txid")?;
            let verbosity = get_optional_field(&params, 1, "verbosity", get_numeric)?.unwrap_or(0);

            state
                .get_raw_transaction(txid, verbosity)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "getroots" => state.get_roots().map(|v| serde_json::to_value(v).unwrap()),

        "findtxout" => {
            let txid = get_hash(&params, 0, "txid")?;
            let vout = get_numeric(&params, 1, "vout")?;
            let script = get_string(&params, 2, "script")?;
            let script = ScriptBuf::from_hex(&script).map_err(|_| JsonRpcError::InvalidScript)?;
            let height = get_numeric(&params, 3, "height")?;

            let state = state.clone();
            state.find_tx_out(txid, vout, script, height).await
        }

        // control
        "getmemoryinfo" => {
            let mode =
                get_optional_field(&params, 0, "mode", get_string)?.unwrap_or("stats".into());

            state
                .get_memory_info(&mode)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "getrpcinfo" => state
            .get_rpc_info()
            .await
            .map(|v| serde_json::to_value(v).unwrap()),

        // help
        // logging
        "stop" => state.stop().await.map(|v| serde_json::to_value(v).unwrap()),

        "uptime" => {
            let uptime = state.uptime();
            Ok(serde_json::to_value(uptime).unwrap())
        }

        // network
        "getpeerinfo" => state
            .get_peer_info()
            .await
            .map(|v| serde_json::to_value(v).unwrap()),

        "getconnectioncount" => state
            .get_connection_count()
            .await
            .map(|v| serde_json::to_value(v).unwrap()),

        "addnode" => {
            let node = get_string(&params, 0, "node")?;
            let command = get_string(&params, 1, "command")?;
            let v2transport =
                get_optional_field(&params, 2, "V2transport", get_bool)?.unwrap_or(false);

            state
                .add_node(node, command, v2transport)
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "disconnectnode" => {
            let node_address = get_string(&params, 0, "node_address")?;
            let node_id = get_optional_field(&params, 1, "node_id", get_numeric)?;

            state
                .disconnect_node(node_address, node_id)
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "ping" => {
            state.ping().await?;

            Ok(serde_json::json!(null))
        }

        // wallet
        "loaddescriptor" => {
            let descriptor = get_string(&params, 0, "descriptor")?;

            state
                .load_descriptor(descriptor)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "rescanblockchain" => {
            let start_height = get_optional_field(&params, 0, "start_height", get_numeric)?;
            let stop_height = get_optional_field(&params, 1, "stop_height", get_numeric)?;
            let use_timestamp =
                get_optional_field(&params, 2, "use_timestamp", get_bool)?.unwrap_or(false);
            let confidence_str = get_optional_field(&params, 3, "confidence", get_string)?
                .unwrap_or("medium".into());

            let confidence = match confidence_str.as_str() {
                "low" => RescanConfidence::Low,
                "medium" => RescanConfidence::Medium,
                "high" => RescanConfidence::High,
                "exact" => RescanConfidence::Exact,
                _ => return Err(JsonRpcError::InvalidRescanVal),
            };

            state
                .rescan_blockchain(start_height, stop_height, use_timestamp, Some(confidence))
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "sendrawtransaction" => {
            let tx = get_string(&params, 0, "hex")?;
            state
                .send_raw_transaction(tx)
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "listdescriptors" => state
            .list_descriptors()
            .map(|v| serde_json::to_value(v).unwrap()),

        _ => {
            let error = JsonRpcError::MethodNotFound;
            Err(error)
        }
    }
}

fn get_http_error_code(err: &JsonRpcError) -> u16 {
    match err {
        // you messed up
        JsonRpcError::InvalidHex
        | JsonRpcError::InvalidAddress
        | JsonRpcError::InvalidScript
        | JsonRpcError::InvalidRequest
        | JsonRpcError::InvalidDescriptor(_)
        | JsonRpcError::InvalidVerbosityLevel
        | JsonRpcError::Decode(_)
        | JsonRpcError::NoBlockFilters
        | JsonRpcError::InvalidMemInfoMode
        | JsonRpcError::InvalidAddnodeCommand
        | JsonRpcError::InvalidDisconnectNodeCommand
        | JsonRpcError::PeerNotFound
        | JsonRpcError::InvalidTimestamp
        | JsonRpcError::InvalidRescanVal
        | JsonRpcError::NoAddressesToRescan
        | JsonRpcError::InvalidParameterType(_)
        | JsonRpcError::MissingParameter(_)
        | JsonRpcError::ChainWorkOverflow
        | JsonRpcError::MempoolAccept(_)
        | JsonRpcError::Wallet(_) => 400,

        // idunnolol
        JsonRpcError::MethodNotFound | JsonRpcError::BlockNotFound | JsonRpcError::TxNotFound => {
            404
        }

        // we messed up, sowwy
        JsonRpcError::InInitialBlockDownload
        | JsonRpcError::Node(_)
        | JsonRpcError::Chain
        | JsonRpcError::Filters(_) => 503,
    }
}

fn get_json_rpc_error_code(err: &JsonRpcError) -> i32 {
    match err {
        // Parse Error
        JsonRpcError::Decode(_) | JsonRpcError::InvalidParameterType(_) => -32700,

        // Invalid Request
        JsonRpcError::InvalidHex
        | JsonRpcError::MissingParameter(_)
        | JsonRpcError::InvalidAddress
        | JsonRpcError::InvalidScript
        | JsonRpcError::MethodNotFound
        | JsonRpcError::InvalidRequest
        | JsonRpcError::InvalidDescriptor(_)
        | JsonRpcError::InvalidVerbosityLevel
        | JsonRpcError::TxNotFound
        | JsonRpcError::BlockNotFound
        | JsonRpcError::InvalidTimestamp
        | JsonRpcError::InvalidMemInfoMode
        | JsonRpcError::InvalidAddnodeCommand
        | JsonRpcError::InvalidDisconnectNodeCommand
        | JsonRpcError::PeerNotFound
        | JsonRpcError::InvalidRescanVal
        | JsonRpcError::NoAddressesToRescan
        | JsonRpcError::ChainWorkOverflow
        | JsonRpcError::Wallet(_)
        | JsonRpcError::MempoolAccept(_) => -32600,

        // server error
        JsonRpcError::InInitialBlockDownload
        | JsonRpcError::Node(_)
        | JsonRpcError::Chain
        | JsonRpcError::NoBlockFilters
        | JsonRpcError::Filters(_) => -32603,
    }
}

async fn json_rpc_request(
    State(state): State<Arc<RpcImpl<impl RpcChain>>>,
    body: Bytes,
) -> Response<Body> {
    let req: RpcRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            let error = RpcError {
                code: -32700,
                message: format!("Parse error: {e}"),
                data: None,
            };
            let body = json!({
                "error": error,
                "id": Value::Null,
            });
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap();
        }
    };

    debug!("Received JSON-RPC request: {req:?}");

    let id = req.id.clone();
    let res = handle_json_rpc_request(req, state.clone()).await;

    state.inflight.write().await.remove(&id);

    match res {
        Ok(res) => {
            let body = serde_json::json!({
                "result": res,
                "id": id,
            });

            axum::http::Response::builder()
                .status(axum::http::StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap()
        }

        Err(e) => {
            let http_error_code = get_http_error_code(&e);
            let json_rpc_error_code = get_json_rpc_error_code(&e);
            let error = RpcError {
                code: json_rpc_error_code,
                message: e.to_string(),
                data: None,
            };

            let body = serde_json::json!({
                "error": error,
                "id": id,
            });

            axum::http::Response::builder()
                .status(axum::http::StatusCode::from_u16(http_error_code).unwrap())
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap()
        }
    }
}

async fn cannot_get(_state: State<Arc<RpcImpl<impl RpcChain>>>) -> Json<serde_json::Value> {
    Json(json!({
        "error": "Cannot get on this route",
    }))
}

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    async fn rescan_with_block_filters(
        addresses: Vec<ScriptBuf>,
        chain: Blockchain,
        wallet: Arc<AddressCache<KvDatabase>>,
        cfilters: Arc<NetworkFilters<FlatFiltersStore>>,
        node: NodeInterface,
        start_height: Option<u32>,
        stop_height: Option<u32>,
    ) -> Result<()> {
        let blocks = cfilters
            .match_any(
                addresses.iter().map(|a| a.as_bytes()).collect(),
                start_height,
                stop_height,
                chain.clone(),
            )
            .unwrap();

        info!("rescan filter hits: {blocks:?}");

        for block in blocks {
            if let Ok(Some(block)) = node.get_block(block).await {
                let height = chain
                    .get_block_height(&block.block_hash())
                    .unwrap()
                    .unwrap();

                wallet.block_process(&block, height);
            }
        }

        Ok(())
    }

    fn make_vin(&self, input: TxIn, is_coinbase: bool) -> TxInJson {
        let sequence = input.sequence.0;
        let witness = (!input.witness.is_empty()).then_some(
            input
                .witness
                .iter()
                .map(|w| w.to_hex_string(bitcoin::hex::Case::Lower))
                .collect(),
        );

        if is_coinbase {
            return TxInJson {
                coinbase: Some(input.script_sig.to_hex_string()),
                sequence,
                witness,
                ..Default::default()
            };
        }

        let txid = Some(input.previous_output.txid.to_string());
        let vout = Some(input.previous_output.vout);
        let script_sig = ScriptSigJson {
            asm: converter_script_into_asm(&input.script_sig, true),
            hex: input.script_sig.to_hex_string(),
        };

        TxInJson {
            coinbase: None,
            txid,
            vout,
            script_sig: Some(script_sig),
            witness,
            sequence,
        }
    }

    fn get_script_type(script: ScriptBuf) -> &'static str {
        match () {
            _ if script.is_op_return() => "nulldata",
            _ if script.is_p2wpkh() => "witness_v0_keyhash",
            _ if script.is_p2wsh() => "witness_v0_scripthash",
            _ if script.is_p2tr() => "witness_v1_taproot",
            _ if script.to_bytes() == [0x51, 0x02, 0x4e, 0x73] => "anchor",
            _ if script.is_witness_program() => "witness_unknown",
            _ if script.is_p2pkh() => "pubkeyhash",
            _ if script.is_p2sh() => "scripthash",
            _ if script.is_p2pk() => "pubkey",
            _ if script.is_multisig() => "multisig",
            _ => "nonstandard",
        }
    }

    fn make_vout(&self, output: TxOut, n: u32) -> TxOutJson {
        let value = output.value;
        TxOutJson {
            value: value.to_btc(),
            n,
            script_pub_key: ScriptPubKeyJson {
                asm: converter_script_into_asm(&output.script_pubkey, false),
                hex: output.script_pubkey.to_hex_string(),
                address: Address::from_script(&output.script_pubkey, self.network)
                    .map(|a| a.to_string())
                    .ok(),
                type_: Self::get_script_type(output.script_pubkey).to_string(),
            },
        }
    }

    fn make_raw_transaction(&self, tx: CachedTransaction) -> RawTxJson {
        let raw_tx = tx.tx;
        let in_active_chain = tx.height != 0;
        let hex = serialize_hex(&raw_tx);
        let txid = raw_tx.compute_txid().to_string();

        let mut blockhash = None;
        let mut blocktime = None;
        let mut time = None;
        let mut confirmations = Some(0);
        if in_active_chain {
            confirmations = self.chain.get_height().ok().and_then(|tip| {
                if tip >= tx.height {
                    Some(tip - tx.height + 1)
                } else {
                    None
                }
            });

            match self.chain.get_block_hash(tx.height) {
                Ok(hash) => {
                    if let Ok(header) = self.chain.get_block_header(&hash) {
                        blockhash = Some(header.block_hash().to_string());
                        blocktime = Some(header.time);
                        time = Some(header.time);
                    }
                }
                Err(_) => {}
            }
        }

        RawTxJson {
            in_active_chain,
            hex,
            txid,
            hash: raw_tx.compute_wtxid().to_string(),
            size: raw_tx.total_size() as u32,
            vsize: raw_tx.vsize() as u32,
            weight: raw_tx.weight().to_wu() as u32,
            version: raw_tx.version.0 as u32,
            locktime: raw_tx.lock_time.to_consensus_u32(),
            vin: raw_tx
                .input
                .iter()
                .map(|input| self.make_vin(input.clone(), raw_tx.is_coinbase()))
                .collect(),
            vout: raw_tx
                .output
                .into_iter()
                .enumerate()
                .map(|(i, output)| self.make_vout(output, i as u32))
                .collect::<Vec<_>>(),
            blockhash,
            confirmations,
            blocktime,
            time,
        }
    }

    // TODO(@luisschwab): get rid of this once
    // https://github.com/rust-bitcoin/rust-bitcoin/pull/4639 makes it into a release.
    fn get_port(net: &Network) -> u16 {
        match net {
            Network::Bitcoin => 8332,
            Network::Signet => 38332,
            Network::Testnet => 18332,
            Network::Testnet4 => 48332,
            Network::Regtest => 18442,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        chain: Blockchain,
        wallet: Arc<AddressCache<KvDatabase>>,
        node: NodeInterface,
        kill_signal: Arc<RwLock<bool>>,
        network: Network,
        block_filter_storage: Option<Arc<NetworkFilters<FlatFiltersStore>>>,
        address: Option<SocketAddr>,
        log_path: String,
    ) {
        let address = address.unwrap_or_else(|| {
            format!("127.0.0.1:{}", Self::get_port(&network))
                .parse()
                .unwrap()
        });

        let listener = match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => {
                let local_addr = listener
                    .local_addr()
                    .expect("Infallible: listener binding was `Ok`");
                info!("RPC server is running at {local_addr}");
                listener
            }
            Err(_) => {
                error!(
                    "Failed to bind to address {address}. Floresta is probably already running.",
                );
                std::process::exit(-1);
            }
        };

        let router = Router::new()
            .route("/", post(json_rpc_request).get(cannot_get))
            .layer(
                CorsLayer::new()
                    .allow_private_network(true)
                    .allow_methods([Method::POST, Method::HEAD]),
            )
            .with_state(Arc::new(RpcImpl {
                chain,
                wallet,
                node,
                kill_signal,
                network,
                block_filter_storage,
                inflight: Arc::new(RwLock::new(HashMap::new())),
                log_path,
                start_time: Instant::now(),
            }));

        axum::serve(listener, router)
            .await
            .expect("failed to start rpc server");
    }
}

/// Converts a script to ASM (assembly) format, displaying the script's operations
/// in a format similar to Bitcoin Core.
///
/// This function performs the following transformations:
/// 1. Removes OP_PUSHBYTES and OP_PUSHDATA opcodes (these are unnecessary in ASM output)
/// 2. Converts leading OP_0 to "0" and OP_PUSHNUM_1 to "1" (these represent witness versions)
/// 3. If `attempt_sighash_decode` is true, attempts to decode hexadecimal data as signatures
///    and appends their sighash type (useful for analyzing scripts in scriptSig)
///
/// # Arguments
/// * `script` - The script buffer to convert
/// * `attempt_sighash_decode` - If true, tries to parse data elements as signatures and format them
fn converter_script_into_asm(script: &ScriptBuf, attempt_sighash_decode: bool) -> String {
    let mut script_asm = script.to_asm_string();
    if !script_asm.contains(' ') {
        return script_asm;
    }

    // Remove OP_PUSHBYTES_X opcodes (these are only metadata for script serialization)
    for i in 0..=75 {
        script_asm = script_asm.replace(&format!("OP_PUSHBYTES_{} ", i), "");
    }

    // Remove OP_PUSHDATA1/2/4 opcodes (these are only metadata for script serialization)
    for i in 1..=4 {
        script_asm = script_asm.replace(&format!("OP_PUSHDATA{} ", i), "");
    }

    let mut array_script_asm: Vec<String> = script_asm.split(' ').map(String::from).collect();

    // Convert leading OP_0 to "0" - represents witness version 0
    if array_script_asm[0] == "OP_0" {
        array_script_asm[0] = "0".to_string();
    }

    // Convert leading OP_PUSHNUM_1 to "1" - represents witness version 1 (Taproot)
    if array_script_asm[0] == "OP_PUSHNUM_1" {
        array_script_asm[0] = "1".to_string();
    }

    // If enabled, attempt to decode data elements as signatures and format them
    // This is particularly useful for scriptSig analysis, where signatures are wrapped with their sighash type
    if attempt_sighash_decode {
        for word in array_script_asm.iter_mut() {
            // Skip OP codes and small words that are unlikely to be signatures
            if word.contains("OP") || word.len() <= 8 {
                continue;
            }

            if let Some(decoded) =
                try_parse_and_format_signature(&Vec::from_hex(word).unwrap_or_default())
            {
                *word = decoded;
            }
        }
    }

    let result = array_script_asm.join(" ");

    result
}

/// Attempts to decode a byte slice as a valid signature (ECDSA or Taproot).
/// If the bytes represent a valid signature, returns the signature with the sighash type appended.
fn try_parse_and_format_signature(signature_bytes: &[u8]) -> Option<String> {
    macro_rules! try_decode_signature {
        ($sig_type:ty) => {
            if let Ok(signature) = <$sig_type>::from_slice(signature_bytes) {
                // Extract the sighash type and remove the "SIGHASH_" prefix
                // The rust-bitcoin library prefixes "SIGHASH_" to the type name, but Bitcoin Core
                // does not include this prefix in the output
                let label = signature.sighash_type.to_string().replace("SIGHASH_", "");
                return Some(format!("{}[{}]", signature.signature, label));
            }
        };
    }

    // Attempt to parse as ECDSA signature
    try_decode_signature!(EcdsaSignature);

    // Attempt to parse as Taproot signature
    try_decode_signature!(TaprootSignature);

    // If the bytes don't match any known signature format, return None
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_converter_script_into_asm_not_attempt_sighash_decode() {
        let test_cases = [
            // P2WPKH
            (
                "0014aabc2cd363103811113b040c541afe3759489c96",
                "0 aabc2cd363103811113b040c541afe3759489c96",
            ),
            // BECH32
            (
                "0014251619c32f6500664e71a6d0393ec4b5f6da549c",
                "0 251619c32f6500664e71a6d0393ec4b5f6da549c",
            ),
            (
                "0014aa138477d24cb7b7a84160ef55af14b7bfb98143",
                "0 aa138477d24cb7b7a84160ef55af14b7bfb98143",
            ),
            // P2PKH
            (
                "76a914e7d68c17e6275b2e5c1da053ef648c676c38962488ac",
                "OP_DUP OP_HASH160 e7d68c17e6275b2e5c1da053ef648c676c389624 OP_EQUALVERIFY OP_CHECKSIG",
            ),
            (
                "76a9144eb2df72d9befff81b6dd985044d2d1b3ed4de4188ac",
                "OP_DUP OP_HASH160 4eb2df72d9befff81b6dd985044d2d1b3ed4de41 OP_EQUALVERIFY OP_CHECKSIG",
            ),
            // P2SH
            (
                "a914fae946075d1f629d35ed4067eca928c1632f4fef87",
                "OP_HASH160 fae946075d1f629d35ed4067eca928c1632f4fef OP_EQUAL",
            ),
            // P2TR (Taproot)
            (
                "51209ec7be23a1ec17cd9c4b621d899eec02bacde1d754ab080f9e1ac8445820014e",
                "1 9ec7be23a1ec17cd9c4b621d899eec02bacde1d754ab080f9e1ac8445820014e",
            ),
        ];

        for (script_hex, expected_asm) in test_cases.iter() {
            let script = ScriptBuf::from_hex(script_hex).unwrap();
            let asm = converter_script_into_asm(&script, false);

            assert_eq!(asm, *expected_asm);
        }
    }

    #[test]
    fn test_converter_script_into_asm_attempt_sighash_decode() {
        let test_cases = [
            // scriptSig with ECDSA signature and pubkey
            (
                "47304402205a9b7c4432f9d895cbf4ac78519ae4e9776d47776078521b93e06beda560dd9a02202b1afbda3c917c2698b38f78203e03d2743069939e3ce2b6a3a153e148502f19012103fde976887234670c672e33a4707356997df737f3e7ac6de809164b5a606b8bad",
                "304402205a9b7c4432f9d895cbf4ac78519ae4e9776d47776078521b93e06beda560dd9a02202b1afbda3c917c2698b38f78203e03d2743069939e3ce2b6a3a153e148502f19[ALL] 03fde976887234670c672e33a4707356997df737f3e7ac6de809164b5a606b8bad",
            ),
            (
                "47304402204ab6753b249205b01d938826189cefaa4176e32ca5aa64fc6fd51891fb78fed2022065b7ba08d8739884ba232f5f7bf6efbb36b2cf98917630c64343cad2fe9db3a2012102ecf8dfb67cae8fe66d700cb13c458e5cc59be2a1c5f3ca3c5a54745259cbe45c",
                "304402204ab6753b249205b01d938826189cefaa4176e32ca5aa64fc6fd51891fb78fed2022065b7ba08d8739884ba232f5f7bf6efbb36b2cf98917630c64343cad2fe9db3a2[ALL] 02ecf8dfb67cae8fe66d700cb13c458e5cc59be2a1c5f3ca3c5a54745259cbe45c",
            ),
            // P2WPKH
            (
                "160014bb180b7bf33f066f7b557c09a0bd3b6accc84fcf",
                "0014bb180b7bf33f066f7b557c09a0bd3b6accc84fcf",
            ),
        ];

        for (script_hex, expected_asm) in test_cases.iter() {
            let script = ScriptBuf::from_hex(script_hex).unwrap();
            let asm = converter_script_into_asm(&script, true);

            assert_eq!(asm, *expected_asm);
        }
    }
}
