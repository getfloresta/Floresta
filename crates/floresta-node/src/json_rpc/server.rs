use std::collections::HashMap;
use std::net::SocketAddr;
use std::slice;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::Method;
use axum::http::Response as HttpResponse;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Json;
use axum::Router;
use bitcoin::consensus::deserialize;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hashes::hex::FromHex;
use bitcoin::hashes::Hash;
use bitcoin::hex::DisplayHex;
use bitcoin::Address;
use bitcoin::BlockHash;
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

use super::res::jsonrpc_interface::JsonRpcError;
use super::res::RawTxJson;
use super::res::ScriptPubKeyJson;
use super::res::ScriptSigJson;
use super::res::TxInJson;
use super::res::TxOutJson;
use crate::json_rpc::request::arg_parser::get_arr_at;
use crate::json_rpc::request::arg_parser::get_at;
use crate::json_rpc::request::arg_parser::get_with_default;
use crate::json_rpc::request::arg_parser::optional;
use crate::json_rpc::request::RpcRequest;
use crate::json_rpc::res::jsonrpc_interface::Response;
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
    fn get_transaction(&self, tx_id: Txid, verbosity: bool) -> Result<Value> {
        if verbosity {
            let tx = self
                .wallet
                .get_transaction(&tx_id)
                .ok_or(JsonRpcError::TxNotFound);
            return tx.map(|tx| serde_json::to_value(self.make_raw_transaction(tx)).unwrap());
        }

        self.wallet
            .get_transaction(&tx_id)
            .and_then(|tx| serde_json::to_value(self.make_raw_transaction(tx)).ok())
            .ok_or(JsonRpcError::TxNotFound)
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
        start: u32,
        stop: u32,
        use_timestamp: bool,
        confidence: RescanConfidence,
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

    // Methods that don't require params
    match method.as_str() {
        "getbestblockhash" => {
            return state
                .get_best_block_hash()
                .map(|v| serde_json::to_value(v).unwrap())
        }
        "getblockchaininfo" => {
            return state
                .get_blockchain_info()
                .map(|v| serde_json::to_value(v).unwrap())
        }
        "getblockcount" => {
            return state
                .get_block_count()
                .map(|v| serde_json::to_value(v).unwrap())
        }
        "getroots" => return state.get_roots().map(|v| serde_json::to_value(v).unwrap()),
        "getrpcinfo" => {
            return state
                .get_rpc_info()
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }
        "stop" => return state.stop().await.map(|v| serde_json::to_value(v).unwrap()),
        "uptime" => return Ok(serde_json::to_value(state.uptime()).unwrap()),
        "getpeerinfo" => {
            return state
                .get_peer_info()
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }
        "ping" => {
            state.ping().await?;
            return Ok(serde_json::json!(null));
        }
        "listdescriptors" => {
            return state
                .list_descriptors()
                .map(|v| serde_json::to_value(v).unwrap())
        }
        _ => {}
    }

    // Methods that require params
    //
    // We default here because theres some cases we have only optionals for a call, so even if omitted, is
    // nice for params to be Some(Value) even if this value is Null.
    let params = params.unwrap_or_default();

    match method.as_str() {
        "getblock" => {
            let hash = get_at(&params, 0, "block_hash")?;
            let verbosity = get_with_default(&params, 1, "verbosity", 1)?;

            state
                .get_block(hash, verbosity)
                .await
                .map(|v| serde_json::to_value(v).expect("GetBlockRes implements serde"))
        }

        "getblockfrompeer" => {
            let hash = get_at(&params, 0, "block_hash")?;

            state.get_block(hash, 0).await?;

            Ok(Value::Null)
        }

        "getblockhash" => {
            let height = get_at(&params, 0, "block_height")?;
            state
                .get_block_hash(height)
                .map(|h| serde_json::to_value(h).unwrap())
        }

        "getblockheader" => {
            let hash = get_at(&params, 0, "block_hash")?;
            state
                .get_block_header(hash)
                .map(|h| serde_json::to_value(h).unwrap())
        }

        "gettxout" => {
            let txid = get_at(&params, 0, "txid")?;
            let vout = get_at(&params, 1, "vout")?;
            let include_mempool = get_with_default(&params, 2, "include_mempool", false)?;

            state
                .get_tx_out(txid, vout, include_mempool)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "gettxoutproof" => {
            let txids = get_arr_at(&params, 0, "txids")?;
            let block_hash = optional(get_at(&params, 1, "block_hash"))?;

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
            let txid = get_at(&params, 0, "txid")?;
            let verbosity = get_with_default(&params, 1, "verbosity", false)?;

            state
                .get_transaction(txid, verbosity)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "findtxout" => {
            let txid = get_at(&params, 0, "txid")?;
            let vout = get_at(&params, 1, "vout")?;
            let script: String = get_at(&params, 2, "script")?;
            let script = ScriptBuf::from_hex(&script).map_err(|_| JsonRpcError::InvalidScript)?;
            let height = get_at(&params, 3, "height")?;

            state.clone().find_tx_out(txid, vout, script, height).await
        }

        "getmemoryinfo" => {
            let mode: String = get_with_default(&params, 0, "mode", "stats".into())?;

            state
                .get_memory_info(&mode)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "addnode" => {
            let node = get_at(&params, 0, "node")?;
            let command = get_at(&params, 1, "command")?;
            let v2transport = get_with_default(&params, 2, "V2transport", false)?;

            state
                .add_node(node, command, v2transport)
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "disconnectnode" => {
            let node_address = get_at(&params, 0, "node_address")?;

            let node_id = optional(get_at(&params, 1, "node_id"))?;

            state
                .disconnect_node(node_address, node_id)
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "loaddescriptor" => {
            let descriptor = get_at(&params, 0, "descriptor")?;

            state
                .load_descriptor(descriptor)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "rescanblockchain" => {
            let start_height = get_with_default(&params, 0, "start_height", 0)?;
            let stop_height = get_with_default(&params, 1, "stop_height", 0)?;
            let use_timestamp = get_with_default(&params, 2, "use_timestamp", false)?;
            let confidence = get_with_default(&params, 3, "confidence", RescanConfidence::Medium)?;

            state
                .rescan_blockchain(start_height, stop_height, use_timestamp, confidence)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "sendrawtransaction" => {
            let tx = get_at(&params, 0, "hex")?;
            state
                .send_raw_transaction(tx)
                .await
                .map(|v| serde_json::to_value(v).unwrap())
        }

        _ => Err(JsonRpcError::MethodNotFound),
    }
}

async fn json_rpc_request(
    State(state): State<Arc<RpcImpl<impl RpcChain>>>,
    body: Bytes,
) -> HttpResponse<Body> {
    let Ok(req): std::result::Result<RpcRequest, _> = serde_json::from_slice(&body) else {
        return JsonRpcError::InvalidRequest.into_response();
    };

    debug!("Received JSON-RPC request: {req:?}");

    let id = req.id.clone();
    let method_res = handle_json_rpc_request(req, state.clone()).await;

    let response = axum::http::Response::builder()
        .status(match &method_res {
            Err(e) => e.http_code(),
            Ok(_) => StatusCode::OK.as_u16(),
        })
        .header("Content-Type", "application/json");

    let body = Response::from_result(method_res, id);

    response
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
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

    fn make_vin(&self, input: TxIn) -> TxInJson {
        let txid = serialize_hex(&input.previous_output.txid);
        let vout = input.previous_output.vout;
        let sequence = input.sequence.0;
        TxInJson {
            txid,
            vout,
            script_sig: ScriptSigJson {
                asm: input.script_sig.to_asm_string(),
                hex: input.script_sig.to_hex_string(),
            },
            witness: input
                .witness
                .iter()
                .map(|w| w.to_hex_string(bitcoin::hex::Case::Upper))
                .collect(),
            sequence,
        }
    }

    fn get_script_type(script: ScriptBuf) -> Option<&'static str> {
        if script.is_p2pkh() {
            return Some("p2pkh");
        }
        if script.is_p2sh() {
            return Some("p2sh");
        }
        if script.is_p2wpkh() {
            return Some("v0_p2wpkh");
        }
        if script.is_p2wsh() {
            return Some("v0_p2wsh");
        }
        None
    }

    fn make_vout(&self, output: TxOut, n: u32) -> TxOutJson {
        let value = output.value;
        TxOutJson {
            value: value.to_sat(),
            n,
            script_pub_key: ScriptPubKeyJson {
                asm: output.script_pubkey.to_asm_string(),
                hex: output.script_pubkey.to_hex_string(),
                req_sigs: 0, // This field is deprecated
                address: Address::from_script(&output.script_pubkey, self.network)
                    .map(|a| a.to_string())
                    .unwrap(),
                type_: Self::get_script_type(output.script_pubkey)
                    .unwrap_or("nonstandard")
                    .to_string(),
            },
        }
    }

    fn make_raw_transaction(&self, tx: CachedTransaction) -> RawTxJson {
        let raw_tx = tx.tx;
        let in_active_chain = tx.height != 0;
        let hex = serialize_hex(&raw_tx);
        let txid = serialize_hex(&raw_tx.compute_txid());
        let block_hash = self
            .chain
            .get_block_hash(tx.height)
            .unwrap_or(BlockHash::all_zeros());
        let tip = self.chain.get_height().unwrap();
        let confirmations = if in_active_chain {
            tip - tx.height + 1
        } else {
            0
        };

        RawTxJson {
            in_active_chain,
            hex,
            txid,
            hash: serialize_hex(&raw_tx.compute_wtxid()),
            size: raw_tx.total_size() as u32,
            vsize: raw_tx.vsize() as u32,
            weight: raw_tx.weight().to_wu() as u32,
            version: raw_tx.version.0 as u32,
            locktime: raw_tx.lock_time.to_consensus_u32(),
            vin: raw_tx
                .input
                .iter()
                .map(|input| self.make_vin(input.clone()))
                .collect(),
            vout: raw_tx
                .output
                .into_iter()
                .enumerate()
                .map(|(i, output)| self.make_vout(output, i as u32))
                .collect(),
            blockhash: serialize_hex(&block_hash),
            confirmations,
            blocktime: self
                .chain
                .get_block_header(&block_hash)
                .map(|h| h.time)
                .unwrap_or(0),
            time: self
                .chain
                .get_block_header(&block_hash)
                .map(|h| h.time)
                .unwrap_or(0),
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
