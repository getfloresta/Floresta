// SPDX-License-Identifier: MIT OR Apache-2.0

use core::net::SocketAddr;
use std::collections::HashMap;
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
use floresta_compact_filters::flat_filters_store::FlatFiltersStore;
use floresta_compact_filters::network_filters::NetworkFilters;
use floresta_watch_only::kv_database::KvDatabase;
use floresta_watch_only::AddressCache;
use floresta_watch_only::CachedTransaction;
use floresta_wire::node_interface::NodeInterface;
use serde_json::json;
use serde_json::Value;
#[cfg(feature = "bitassets")]
use tokio::sync::Mutex as AsyncMutex;
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
#[cfg(feature = "bitassets")]
use crate::bitassets_wallet::{
    parse_asset_id, BitAssetData, DutchAuctionParams, NativeBitAssetsWallet,
};
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
    #[cfg(feature = "bitassets")]
    pub(super) bitassets_wallet: Option<Arc<AsyncMutex<NativeBitAssetsWallet>>>,
}

type Result<T> = std::result::Result<T, JsonRpcError>;

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    fn get_transaction(&self, tx_id: Txid, verbosity: Option<bool>) -> Result<Value> {
        if verbosity == Some(true) {
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
        let addresses = self.wallet.push_descriptor(&descriptor)?;
        info!("Descriptor pushed: {descriptor}");
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
            let verbosity = get_optional_field(&params, 1, "verbosity", get_bool)?.unwrap_or(true);

            state
                .get_block_header(hash, verbosity)
                .await
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
            let verbosity = get_optional_field(&params, 1, "verbosity", get_bool)?;

            state
                .get_transaction(txid, verbosity)
                .map(|v| serde_json::to_value(v).unwrap())
        }

        "getroots" => state.get_roots().map(|v| serde_json::to_value(v).unwrap()),

        #[cfg(feature = "bitassets")]
        "bitassets_getnewaddress" => {
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            let address = wallet
                .lock()
                .await
                .get_new_address()
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))?;
            Ok(json!(address))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_walletinfo" => {
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            Ok(wallet.lock().await.wallet_info())
        }

        #[cfg(feature = "bitassets")]
        "bitassets_sync" => {
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .sync()
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_listutxos" => {
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            Ok(wallet.lock().await.list_utxos())
        }

        #[cfg(feature = "bitassets")]
        "bitassets_getbalance" => {
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            let asset_id = params.first().and_then(Value::as_str);
            Ok(wallet.lock().await.get_balance(asset_id))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_transfer" => {
            let destination = get_string(&params, 0, "destination")?;
            let asset_id = get_string(&params, 1, "asset_id")?;
            let amount = get_numeric(&params, 2, "amount")?;
            let fee_sats = get_numeric(&params, 3, "fee_sats")?;
            let memo = params
                .get(4)
                .and_then(Value::as_str)
                .map(|memo| hex::decode(memo).unwrap_or_else(|_| memo.as_bytes().to_vec()));
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .transfer(&destination, &asset_id, amount, fee_sats, memo)
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_reserve" => {
            let name = get_string(&params, 0, "name")?;
            let fee_sats = get_numeric(&params, 1, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .reserve(&name, fee_sats)
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_register" => {
            let name = get_string(&params, 0, "name")?;
            let initial_supply = get_numeric(&params, 1, "initial_supply")?;
            let bitasset_data: BitAssetData = params
                .get(2)
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))?
                .unwrap_or(BitAssetData {
                    ticker: None,
                    name: None,
                    summary: None,
                });
            let fee_sats = get_numeric(&params, 3, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .register(&name, initial_supply, bitasset_data, fee_sats)
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_amm_mint" => {
            let asset0 = get_string(&params, 0, "asset0")?;
            let asset1 = get_string(&params, 1, "asset1")?;
            let amount0 = get_numeric(&params, 2, "amount0")?;
            let amount1 = get_numeric(&params, 3, "amount1")?;
            let lp_token_mint = get_numeric(&params, 4, "lp_token_mint")?;
            let fee_sats = get_numeric(&params, 5, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .amm_mint(&asset0, &asset1, amount0, amount1, lp_token_mint, fee_sats)
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_amm_swap" => {
            let asset_spend = get_string(&params, 0, "asset_spend")?;
            let asset_receive = get_string(&params, 1, "asset_receive")?;
            let amount_spend = get_numeric(&params, 2, "amount_spend")?;
            let amount_receive = get_numeric(&params, 3, "amount_receive")?;
            let fee_sats = get_numeric(&params, 4, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .amm_swap(
                    &asset_spend,
                    &asset_receive,
                    amount_spend,
                    amount_receive,
                    fee_sats,
                )
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_amm_burn" => {
            let asset0 = get_string(&params, 0, "asset0")?;
            let asset1 = get_string(&params, 1, "asset1")?;
            let amount0 = get_numeric(&params, 2, "amount0")?;
            let amount1 = get_numeric(&params, 3, "amount1")?;
            let lp_token_burn = get_numeric(&params, 4, "lp_token_burn")?;
            let fee_sats = get_numeric(&params, 5, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .amm_burn(&asset0, &asset1, amount0, amount1, lp_token_burn, fee_sats)
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_dutch_auction_create" => {
            let object = params.first().and_then(Value::as_object).ok_or_else(|| {
                JsonRpcError::Wallet("auction params must be an object".to_string())
            })?;
            let string_field = |name: &str| -> Result<String> {
                object
                    .get(name)
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| JsonRpcError::Wallet(format!("auction params missing {name}")))
            };
            let u32_field = |name: &str| -> Result<u32> {
                object
                    .get(name)
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| JsonRpcError::Wallet(format!("auction params missing {name}")))
            };
            let u64_field = |name: &str| -> Result<u64> {
                object
                    .get(name)
                    .and_then(Value::as_u64)
                    .ok_or_else(|| JsonRpcError::Wallet(format!("auction params missing {name}")))
            };
            let auction_params = DutchAuctionParams {
                start_block: u32_field("start_block")?,
                duration: u32_field("duration")?,
                base_asset: parse_asset_id(&string_field("base_asset")?)
                    .map_err(|err| JsonRpcError::Wallet(err.to_string()))?,
                base_amount: u64_field("base_amount")?,
                quote_asset: parse_asset_id(&string_field("quote_asset")?)
                    .map_err(|err| JsonRpcError::Wallet(err.to_string()))?,
                initial_price: u64_field("initial_price")?,
                final_price: u64_field("final_price")?,
            };
            let fee_sats = get_numeric(&params, 1, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .dutch_auction_create(auction_params, fee_sats)
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_dutch_auction_bid" => {
            let auction_id = get_string(&params, 0, "auction_id")?;
            let base_asset = get_string(&params, 1, "base_asset")?;
            let quote_asset = get_string(&params, 2, "quote_asset")?;
            let bid_size = get_numeric(&params, 3, "bid_size")?;
            let receive_quantity = get_numeric(&params, 4, "receive_quantity")?;
            let fee_sats = get_numeric(&params, 5, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .dutch_auction_bid(
                    &auction_id,
                    &base_asset,
                    &quote_asset,
                    bid_size,
                    receive_quantity,
                    fee_sats,
                )
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

        #[cfg(feature = "bitassets")]
        "bitassets_dutch_auction_collect" => {
            let auction_id = get_string(&params, 0, "auction_id")?;
            let base_asset = get_string(&params, 1, "base_asset")?;
            let quote_asset = get_string(&params, 2, "quote_asset")?;
            let amount_base = get_numeric(&params, 3, "amount_base")?;
            let amount_quote = get_numeric(&params, 4, "amount_quote")?;
            let fee_sats = get_numeric(&params, 5, "fee_sats")?;
            let Some(wallet) = state.bitassets_wallet.as_ref() else {
                return Err(JsonRpcError::Wallet(
                    "native BitAssets wallet is not enabled".to_string(),
                ));
            };
            wallet
                .lock()
                .await
                .dutch_auction_collect(
                    &auction_id,
                    &base_asset,
                    &quote_asset,
                    amount_base,
                    amount_quote,
                    fee_sats,
                )
                .map_err(|err| JsonRpcError::Wallet(err.to_string()))
        }

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
        | JsonRpcError::ConversionOverflow(_)
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
        | JsonRpcError::ConversionOverflow(_)
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
        #[cfg(feature = "bitassets")] bitassets_wallet: Option<
            Arc<AsyncMutex<NativeBitAssetsWallet>>,
        >,
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
                #[cfg(feature = "bitassets")]
                bitassets_wallet,
            }));

        axum::serve(listener, router)
            .await
            .expect("failed to start rpc server");
    }
}
