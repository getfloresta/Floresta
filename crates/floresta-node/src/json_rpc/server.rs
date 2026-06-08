// SPDX-License-Identifier: MIT OR Apache-2.0

use core::net::SocketAddr;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::Method;
use axum::http::Response as HttpResponse;
use axum::http::StatusCode;
use axum::routing::post;
use bitcoin::Network;
use bitcoin::Txid;
use floresta_chain::ThreadSafeChain;
use floresta_compact_filters::flat_filters_store::FlatFiltersStore;
use floresta_compact_filters::network_filters::NetworkFilters;
use floresta_rpc::rpc_interfaces::BlockchainRpc;
use floresta_rpc::rpc_interfaces::ControlRpc;
use floresta_rpc::rpc_interfaces::NetworkRpc;
use floresta_rpc::rpc_interfaces::RawTransactionRpc;
use floresta_rpc::rpc_interfaces::RpcMethods;
use floresta_rpc::rpc_interfaces::WalletRpc;
use floresta_rpc::rpc_types::AddNodeCommand;
use floresta_watch_only::AddressCache;
use floresta_watch_only::kv_database::KvDatabase;
use floresta_wire::node_handle::NodeHandle;
use serde_json::Value;
use serde_json::json;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::debug;
use tracing::error;
use tracing::info;

use super::res::jsonrpc_interface::JsonRpcError;
use crate::json_rpc::request::RpcRequest;
use crate::json_rpc::request::arg_parser::get_at;
use crate::json_rpc::request::arg_parser::get_with_default;
use crate::json_rpc::request::arg_parser::try_into_optional;
use crate::json_rpc::res::jsonrpc_interface::Response;

/// Expect message for `serde_json` serialization of types that implement `Serialize`.
pub(super) const SERIALIZATION_EXPECT_MSG: &str = "types used in RPC responses implement Serialize";

/// Expect message for HTTP response builder with hardcoded valid headers.
pub(super) const HTTP_RESPONSE_EXPECT: &str = "HTTP response built from valid hardcoded headers";

/// The server holds this to tell which rpc method is awaiting to be processed and when the request were made.
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
    pub(super) node: NodeHandle,
    pub(super) kill_signal: Arc<RwLock<bool>>,
    pub(super) inflight: Arc<RwLock<HashMap<Value, InflightRpc>>>,
    pub(super) log_path: PathBuf,
    pub(super) start_time: Instant,
    pub(super) user_agent: String,
    pub(super) proxy: Option<SocketAddr>,
}

type Result<T> = std::result::Result<T, JsonRpcError>;

async fn handle_json_rpc_request(
    req: RpcRequest,
    state: Arc<RpcImpl<impl RpcChain>>,
) -> Result<Value> {
    let RpcRequest {
        jsonrpc,
        method,
        params,
        id,
    } = req;

    if let Some(version) = jsonrpc {
        if !["1.0", "2.0"].contains(&version.as_str()) {
            return Err(JsonRpcError::InvalidJsonRpcVersion);
        }
    }

    state.inflight.write().await.insert(
        id.clone(),
        InflightRpc {
            method: method.clone(),
            when: Instant::now(),
        },
    );
    let method = RpcMethods::from_str(&method).map_err(|_| JsonRpcError::MethodNotFound)?;
    let params = params.unwrap_or_default();

    match method {
        // Blockchain
        RpcMethods::FindTxOut => {
            let txid = get_at(&params, 0, "txid")?;
            let vout = get_at(&params, 1, "vout")?;
            let script = get_at(&params, 2, "script")?;
            let height = get_at(&params, 3, "height")?;

            state
                .clone()
                .find_tx_out(txid, vout, script, height)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetBestBlockHash => state
            .get_best_block_hash()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::GetBlock => {
            let hash = get_at(&params, 0, "block_hash")?;
            let verbosity = try_into_optional(get_at(&params, 1, "verbosity"))?;

            state
                .get_block(hash, verbosity)
                .await
                .map(|v| serde_json::to_value(v).expect("GetBlockRes implements serde"))
        }
        RpcMethods::GetBlockFromPeer => {
            let hash = get_at(&params, 0, "block_hash")?;

            state.get_block(hash, Some(0)).await?;

            Ok(Value::Null)
        }
        RpcMethods::GetBlockchainInfo => state
            .get_blockchain_info()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::GetBlockCount => state
            .get_block_count()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::GetBlockHash => {
            let height = get_at(&params, 0, "block_height")?;
            state
                .get_block_hash(height)
                .await
                .map(|h| serde_json::to_value(h).expect(SERIALIZATION_EXPECT_MSG))
        }

        RpcMethods::GetBlockHeader => {
            let hash = get_at(&params, 0, "block_hash")?;
            let verbosity = try_into_optional(get_at(&params, 1, "verbosity"))?;

            state
                .get_block_header(hash, verbosity)
                .await
                .map(|h| serde_json::to_value(h).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetDeploymentInfo => {
            let blockhash = try_into_optional(get_at(&params, 0, "blockhash"))?;

            state
                .get_deployment_info(blockhash)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetDifficulty => state
            .get_difficulty()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::GetTxOut => {
            let txid = get_at(&params, 0, "txid")?;
            let vout = get_at(&params, 1, "vout")?;
            let include_mempool = get_with_default(&params, 2, "include_mempool", false)?;

            state
                .get_tx_out(txid, vout, include_mempool)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetTxOutProof => {
            let txids: Vec<Txid> = get_at(&params, 0, "txids")?;
            let block_hash = try_into_optional(get_at(&params, 1, "block_hash"))?;

            state
                .get_txout_proof(&txids, block_hash)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetRoots => state
            .get_roots()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),

        // Wallet
        RpcMethods::LoadDescriptor => {
            let descriptor = get_at(&params, 0, "descriptor")?;

            state
                .load_descriptor(descriptor)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::ListDescriptors => {
            return state
                .list_descriptors()
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG));
        }
        RpcMethods::RescanBlockchain => {
            let start_height = try_into_optional(get_at(&params, 0, "start_height"))?;
            let stop_height = try_into_optional(get_at(&params, 1, "stop_height"))?;
            let use_timestamp = get_with_default(&params, 2, "use_timestamp", false)?;
            let confidence = try_into_optional(get_at(&params, 3, "confidence"))?;

            state
                .rescan_blockchain(start_height, stop_height, use_timestamp, confidence)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }

        // Network
        RpcMethods::AddNode => {
            let node = get_at(&params, 0, "node")?;
            let command: String = get_at(&params, 1, "command")?;
            let command = AddNodeCommand::from_str(&command)
                .map_err(|_| JsonRpcError::InvalidAddnodeCommand)?;
            let v2transport = get_with_default(&params, 2, "V2transport", false)?;

            state
                .add_node(node, command, v2transport)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::DisconnectNode => {
            let node_address = get_at(&params, 0, "node_address")?;
            let node_id = try_into_optional(get_at(&params, 1, "node_id"))?;

            state
                .disconnect_node(node_address, node_id)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetAddrManInfo => state
            .get_addrman_info()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::GetConnectionCount => state
            .get_connection_count()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::GetNetworkInfo => state
            .get_network_info()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::GetPeerInfo => state
            .get_peer_info()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::Ping => {
            state.ping().await?;
            Ok(serde_json::json!(null))
        }

        // RawTransactions
        RpcMethods::SendRawTransaction => {
            let tx = get_at(&params, 0, "hex")?;
            state
                .send_raw_transaction(tx)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetRawTransaction => {
            let txid = get_at(&params, 0, "txid")?;
            let verbosity = try_into_optional(get_at(&params, 1, "verbosity"))?;

            state
                .get_raw_transaction(txid, verbosity)
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        }

        // Control
        RpcMethods::Stop => state
            .stop()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
        RpcMethods::Uptime => {
            Ok(serde_json::to_value(state.uptime().await?).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetMemoryInfo => {
            let mode: String = get_with_default(&params, 0, "mode", "stats".into())?;

            let memory_info = state.get_memory_info(mode).await?;

            Ok(serde_json::to_value(memory_info).expect(SERIALIZATION_EXPECT_MSG))
        }
        RpcMethods::GetRpcInfo => state
            .get_rpc_info()
            .await
            .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG)),
    }
}

async fn json_rpc_request(
    State(state): State<Arc<RpcImpl<impl RpcChain>>>,
    body: Bytes,
) -> HttpResponse<Body> {
    let Ok(req): std::result::Result<RpcRequest, _> = serde_json::from_slice(&body) else {
        let error = JsonRpcError::InvalidRequest;
        let body = Response::error(error.rpc_error(), Value::Null);
        return HttpResponse::builder()
            .status(error.http_code())
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&body).expect(SERIALIZATION_EXPECT_MSG),
            ))
            .expect(HTTP_RESPONSE_EXPECT);
    };

    debug!("Received JSON-RPC request: {req:?}");

    let id = req.id.clone();
    let method_res = handle_json_rpc_request(req, state.clone()).await;

    state.inflight.write().await.remove(&id);

    let response = HttpResponse::builder()
        .status(match &method_res {
            Err(e) => e.http_code(),
            Ok(_) => StatusCode::OK,
        })
        .header("Content-Type", "application/json");

    let body = Response::from_result(method_res, id);

    response
        .body(Body::from(
            serde_json::to_vec(&body).expect(SERIALIZATION_EXPECT_MSG),
        ))
        .expect(HTTP_RESPONSE_EXPECT)
}

async fn cannot_get(_state: State<Arc<RpcImpl<impl RpcChain>>>) -> Json<Value> {
    Json(json!({
        "error": "Cannot get on this route",
    }))
}

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
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
        node: NodeHandle,
        kill_signal: Arc<RwLock<bool>>,
        network: Network,
        block_filter_storage: Option<Arc<NetworkFilters<FlatFiltersStore>>>,
        address: Option<SocketAddr>,
        log_path: impl AsRef<Path>,
        user_agent: String,
        proxy: Option<SocketAddr>,
    ) {
        let address = address.unwrap_or_else(|| {
            format!("127.0.0.1:{}", Self::get_port(&network))
                .parse()
                .expect("hardcoded address is valid")
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
                log_path: log_path.as_ref().into(),
                start_time: Instant::now(),
                user_agent,
                proxy,
            }));

        axum::serve(listener, router)
            .await
            .expect("failed to start rpc server");
    }
}
