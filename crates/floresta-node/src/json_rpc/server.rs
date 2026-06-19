// SPDX-License-Identifier: MIT OR Apache-2.0

use core::net::SocketAddr;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
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
use floresta_chain::ThreadSafeChain;
use floresta_compact_filters::flat_filters_store::FlatFiltersStore;
use floresta_compact_filters::network_filters::NetworkFilters;
use floresta_rpc::rpc_interfaces::BlockchainRpc;
use floresta_rpc::rpc_interfaces::ControlRpc;
use floresta_rpc::rpc_interfaces::JsonRpcEnvelope;
use floresta_rpc::rpc_interfaces::JsonRpcVersion;
use floresta_rpc::rpc_interfaces::NetworkRpc;
use floresta_rpc::rpc_interfaces::RawTransactionRpc;
use floresta_rpc::rpc_interfaces::RpcCommand;
use floresta_rpc::rpc_interfaces::WalletRpc;
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
use super::res::jsonrpc_interface::Response;

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
    command: RpcCommand,
    state: Arc<RpcImpl<impl RpcChain>>,
) -> Result<Value> {
    /// Awaits an RPC call and serializes its result to a JSON [`Value`].
    macro_rules! dispatch {
        ($call:expr) => {
            $call
                .await
                .map(|v| serde_json::to_value(v).expect(SERIALIZATION_EXPECT_MSG))
        };
    }

    match command {
        // Blockchain
        RpcCommand::FindTxOut {
            txid,
            vout,
            script,
            height,
        } => dispatch!(state.find_tx_out(txid, vout, script, height)),
        RpcCommand::GetBestBlockHash => dispatch!(state.get_best_block_hash()),
        RpcCommand::GetBlock {
            block_hash,
            verbosity,
        } => dispatch!(state.get_block(block_hash, verbosity)),
        RpcCommand::GetBlockFromPeer { block_hash } => {
            state.get_block(block_hash, Some(0)).await?;
            Ok(Value::Null)
        }
        RpcCommand::GetBlockchainInfo => dispatch!(state.get_blockchain_info()),
        RpcCommand::GetBlockCount => dispatch!(state.get_block_count()),
        RpcCommand::GetBlockHash { block_height } => {
            dispatch!(state.get_block_hash(block_height))
        }
        RpcCommand::GetBlockHeader {
            block_hash,
            verbosity,
        } => dispatch!(state.get_block_header(block_hash, verbosity)),
        RpcCommand::GetDeploymentInfo { blockhash } => {
            dispatch!(state.get_deployment_info(blockhash))
        }
        RpcCommand::GetDifficulty => dispatch!(state.get_difficulty()),
        RpcCommand::GetTxOut {
            txid,
            vout,
            include_mempool,
        } => dispatch!(state.get_tx_out(txid, vout, include_mempool.unwrap_or(false))),
        RpcCommand::GetTxOutProof { txids, block_hash } => {
            dispatch!(state.get_txout_proof(&txids, block_hash))
        }
        RpcCommand::GetRoots => dispatch!(state.get_roots()),

        // Wallet
        RpcCommand::LoadDescriptor { descriptor } => {
            dispatch!(state.load_descriptor(descriptor))
        }
        RpcCommand::ListDescriptors => dispatch!(state.list_descriptors()),
        RpcCommand::RescanBlockchain {
            start_height,
            stop_height,
            use_timestamp,
            confidence,
        } => dispatch!(state.rescan_blockchain(
            start_height,
            stop_height,
            use_timestamp.unwrap_or(false),
            confidence,
        )),

        // Network
        RpcCommand::AddNode {
            node,
            command,
            v2transport,
        } => dispatch!(state.add_node(node, command, v2transport.unwrap_or(false))),
        RpcCommand::DisconnectNode {
            node_address,
            node_id,
        } => dispatch!(state.disconnect_node(node_address, node_id)),
        RpcCommand::GetAddrManInfo => dispatch!(state.get_addrman_info()),
        RpcCommand::GetConnectionCount => dispatch!(state.get_connection_count()),
        RpcCommand::GetNetworkInfo => dispatch!(state.get_network_info()),
        RpcCommand::GetPeerInfo => dispatch!(state.get_peer_info()),
        RpcCommand::Ping => {
            state.ping().await?;
            Ok(Value::Null)
        }

        // RawTransaction
        RpcCommand::SendRawTransaction { hex } => {
            dispatch!(state.send_raw_transaction(hex))
        }
        RpcCommand::GetRawTransaction { txid, verbosity } => {
            dispatch!(state.get_raw_transaction(txid, verbosity))
        }

        // Control
        RpcCommand::Stop => dispatch!(state.stop()),
        RpcCommand::Uptime => dispatch!(state.uptime()),
        RpcCommand::GetMemoryInfo { mode } => {
            let mode = mode.unwrap_or_else(|| "stats".to_string());
            dispatch!(state.get_memory_info(mode))
        }
        RpcCommand::GetRpcInfo => dispatch!(state.get_rpc_info()),
    }
}

async fn json_rpc_request(
    State(state): State<Arc<RpcImpl<impl RpcChain>>>,
    body: Bytes,
) -> HttpResponse<Body> {
    let JsonRpcEnvelope {
        command,
        jsonrpc,
        id,
    } = match serde_json::from_slice(&body) {
        Ok(env) => env,
        Err(err) => {
            let error: JsonRpcError = err.into();
            let id = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|v| v.get("id").cloned())
                .unwrap_or(Value::Null);
            let body = Response::error(error.rpc_error(), id);
            return HttpResponse::builder()
                .status(error.http_code())
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&body).expect(SERIALIZATION_EXPECT_MSG),
                ))
                .expect(HTTP_RESPONSE_EXPECT);
        }
    };

    if let Some(JsonRpcVersion::Unknown(_)) = jsonrpc {
        let error = JsonRpcError::InvalidJsonRpcVersion;
        let body = Response::error(error.rpc_error(), id);
        return HttpResponse::builder()
            .status(error.http_code())
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&body).expect(SERIALIZATION_EXPECT_MSG),
            ))
            .expect(HTTP_RESPONSE_EXPECT);
    }

    let method_name = command.method_name();
    debug!("Received JSON-RPC request: method={method_name} id={id}");

    state.inflight.write().await.insert(
        id.clone(),
        InflightRpc {
            method: method_name,
            when: Instant::now(),
        },
    );

    let method_res = handle_json_rpc_request(command, state.clone()).await;

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
            .with_state(Arc::new(Self {
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
