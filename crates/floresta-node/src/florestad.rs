// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(feature = "metrics")]
use core::net::IpAddr;
#[cfg(feature = "metrics")]
use core::net::Ipv4Addr;
use core::net::SocketAddr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(feature = "json-rpc")]
use std::sync::OnceLock;

use bitcoin::p2p::Magic;
use bitcoin::Address;
pub use bitcoin::Network;
use bitcoin::ScriptBuf;
#[cfg(feature = "bitassets")]
use floresta_chain::AssetHistoryEventKind;
#[cfg(feature = "bitassets")]
use floresta_chain::AssetProofRefs;
#[cfg(feature = "bitassets")]
use floresta_chain::AssetProofStatus;
#[cfg(feature = "bitassets")]
use floresta_chain::AssetUtxo;
pub use floresta_chain::AssumeUtreexoValue;
pub use floresta_chain::AssumeValidArg;
#[cfg(feature = "bitassets")]
use floresta_chain::BitAssetIndex;
#[cfg(any(feature = "zmq-server", feature = "bitassets"))]
use floresta_chain::BlockchainInterface;
use floresta_chain::ChainParams;
use floresta_chain::ChainState;
use floresta_chain::FlatChainStore as ChainStore;
use floresta_chain::FlatChainStoreConfig;
#[cfg(feature = "bitassets")]
use floresta_chain::TrustedSidechainAssetUtxo;
#[cfg(feature = "compact-filters")]
use floresta_compact_filters::flat_filters_store::FlatFiltersStore;
#[cfg(feature = "compact-filters")]
use floresta_compact_filters::network_filters::NetworkFilters;
use floresta_electrum::electrum_protocol::client_accept_loop;
use floresta_electrum::electrum_protocol::ElectrumServer;
use floresta_mempool::Mempool;
use floresta_watch_only::kv_database::KvDatabase;
use floresta_watch_only::AddressCache;
use floresta_watch_only::WatchOnlyError;
use floresta_wire::address_man::AddressMan;
use floresta_wire::address_man::SUPPORTED_NETWORKS;
use floresta_wire::node::running_ctx::RunningNode;
use floresta_wire::node::UtreexoNode;
use floresta_wire::UtreexoNodeConfig;
use rcgen::BasicConstraints;
use rcgen::CertificateParams;
use rcgen::IsCa;
use rcgen::KeyPair;
#[cfg(feature = "bitassets")]
use serde::Deserialize;
#[cfg(feature = "bitassets")]
use serde::Serialize;
#[cfg(feature = "bitassets")]
use serde_json::json;
#[cfg(feature = "bitassets")]
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::task;
#[cfg(any(feature = "metrics", feature = "bitassets"))]
use tokio::time::Duration;
#[cfg(feature = "metrics")]
use tokio::time::{self};
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::CertificateDer;
use tokio_rustls::rustls::pki_types::PrivateKeyDer;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

#[cfg(feature = "bitassets")]
use crate::bitassets_wallet::NativeBitAssetsWallet;
use crate::config_file::ConfigFile;
use crate::error::FlorestadError;
use crate::florestad::fs::OpenOptions;
#[cfg(feature = "json-rpc")]
use crate::json_rpc;
#[cfg(feature = "zmq-server")]
use crate::zmq::ZMQServer;

#[cfg(feature = "bitassets")]
const BITASSETS_INDEX_FILE: &str = "bitassets-index.json";
#[cfg(feature = "bitassets")]
const BITASSETS_WALLET_FILE: &str = "bitassets-wallet.json";

#[cfg(feature = "bitassets")]
#[derive(Debug, Deserialize, Serialize)]
struct PersistedBitAssetIndex {
    version: u32,
    #[serde(default)]
    sidechain_height: Option<u32>,
    #[serde(default)]
    sidechain_tip: Option<String>,
    utxos: Vec<PersistedAssetUtxo>,
}

#[cfg(feature = "bitassets")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct BitAssetsSidechainTip {
    height: u32,
    hash: Option<String>,
}

#[cfg(feature = "bitassets")]
#[derive(Debug, Deserialize, Serialize)]
struct PersistedAssetUtxo {
    asset_id: String,
    txid: String,
    vout: u32,
    asset_amount: u64,
    bitcoin_value: u64,
    height: u32,
    #[serde(default)]
    block_hash: Option<String>,
    #[serde(default)]
    event_kind: String,
    #[serde(default)]
    proof_status: String,
    #[serde(default)]
    sidechain_block_height: Option<u32>,
    #[serde(default)]
    bmm_inclusions: Vec<String>,
    #[serde(default)]
    best_main_verification: Option<String>,
}

#[cfg(feature = "bitassets")]
impl From<AssetUtxo> for PersistedAssetUtxo {
    fn from(utxo: AssetUtxo) -> Self {
        Self {
            asset_id: utxo.asset_id.to_string(),
            txid: utxo.outpoint.txid.to_string(),
            vout: utxo.outpoint.vout,
            asset_amount: utxo.asset_amount,
            bitcoin_value: utxo.bitcoin_value,
            height: utxo.height,
            block_hash: utxo.block_hash.map(|hash| hash.to_string()),
            event_kind: AssetHistoryEventKind::SidechainUnspent.as_str().to_string(),
            proof_status: utxo.proof_status.as_str().to_string(),
            sidechain_block_height: utxo.proof_refs.sidechain_block_height,
            bmm_inclusions: utxo.proof_refs.bmm_inclusions,
            best_main_verification: utxo.proof_refs.best_main_verification,
        }
    }
}

/// The default maximum size of the mempool in bytes.
///
/// This is the same default as Bitcoin Core.
const DEFAULT_MEMPOOL_MAX_SIZE_BYTES: usize = 300_000_000; // 300 MiB

#[derive(Clone)]
/// General configuration for the floresta daemon.
///
/// Those configs should be passed in by anyone that wants to start a floresta instance. Some of
/// these are also exposed through the config file.
pub struct Config {
    /// Whether we should disable dns seeds
    pub disable_dns_seeds: bool,

    /// Where we should place our data
    ///
    /// This directory must be readable and writable by our process. We'll use this dir to store
    /// both chain and wallet data, so this should be kept in a non-volatile medium. We are not
    /// particularly aggressive in disk usage, so we don't need a fast disk to work.
    pub data_dir: String,

    /// Assume that all blocks prior to and including this block have valid scripts.
    ///
    /// This is an optimization mirrored from Bitcoin Core: script execution (including signature
    /// checks) is skipped under the assumption that these scripts were correctly validated when
    /// the software was released. Since users already trust the developers and reviewers of the
    /// software, the hardcoded boundary is assumed to be correct.
    pub assume_valid: AssumeValidArg,

    /// A vector of xpubs to cache
    ///
    /// This is a list of SLIP-132-encoded extended public key that we should add to our Watch-only
    /// wallet. A descriptor may be only passed one time, if you call florestad with an already
    /// cached address, that will be a no-op. After a xpub is cached, we derive multiple addresses
    /// from it and try to find transactions involving it.
    pub wallet_xpub: Option<Vec<String>>,

    /// An output descriptor to cache
    ///
    /// This should be a list of output descriptors that we should add to our watch-only wallet.
    /// This works just like wallet_xpub, but with a descriptor.
    pub wallet_descriptor: Option<Vec<String>>,

    /// Where should we read from a config file
    ///
    /// This is a toml-encoded file with floresta's configs. For a sample of how this file looks
    /// like, see config.toml.sample inside floresta's codebase.
    ///
    /// If a setting is modified by the config file and this config struct, the following logic is
    /// used:
    ///     - For vectors, we use the combination of both vectors
    ///     - for mutually exclusive options, this struct has precedence over the config file
    pub config_file: Option<String>,

    /// A proxy that we should use to connect with others
    ///
    /// This should be a socks5 proxy, like Tor's socks. If provided, all our outgoing connections
    /// will be made through this one, except dns seed connections.
    pub proxy: Option<String>,

    /// The network we are running in, it may be one of: bitcoin, signet, regtest or testnet.
    pub network: Network,

    /// Whether we should build and store compact block filters
    ///
    /// Those filters are used for rescanning our wallet for historical transactions. If you don't
    /// have this on, the only way to find historical transactions is to download all blocks, which
    /// is very inefficient and resource/time consuming. But keep in mind that filters will take
    /// up disk space.
    pub cfilters: bool,

    /// If we are using block filters, we may not need to download the whole chain of filters, as
    /// our wallets may not have been created at the beginning of the chain. With this option, we
    /// can make a rough estimate of the block height we need to start downloading filters.
    ///
    /// If the value is negative, it's relative to the current tip. For example, if the current tip
    /// is at height 1000, and we set this value to -100, we will start downloading filters from
    /// height 900.
    pub filters_start_height: Option<i32>,

    #[cfg(feature = "zmq-server")]
    /// The address to listen to for our ZMQ server
    ///
    /// We have an (optional) ZMQ server, that pushes new blocks over a PUSH/PULL ZMQ queue, this
    /// is the address that we'll listen for incoming connections.
    pub zmq_address: Option<String>,

    /// A node to connect to
    ///
    /// If this option is provided, we'll connect **only** to this node.
    pub connect: Option<String>,

    #[cfg(feature = "json-rpc")]
    /// The address our json-rpc should listen to
    pub json_rpc_address: Option<String>,

    /// Whether we should write logs to `stdout`.
    pub log_to_stdout: bool,

    /// Whether we should log to a fs file
    pub log_to_file: bool,

    /// Whether we should use assume utreexo
    pub assume_utreexo: bool,

    /// Whether we should post debug information to the console
    pub debug: bool,

    /// The user agent that we will advertise to our peers
    pub user_agent: String,

    /// The value to use for assumeutreexo
    pub assumeutreexo_value: Option<AssumeUtreexoValue>,

    /// Address the Electrum Server will listen to.
    pub electrum_address: Option<String>,

    #[cfg(feature = "bitassets")]
    /// Whether to enable experimental BitAssets indexing and asset Electrum methods.
    pub enable_bitassets: bool,

    #[cfg(feature = "bitassets")]
    /// Optional plain-bitassets JSON-RPC URL used as a trusted sidechain data source.
    pub bitassets_rpc_url: Option<String>,

    #[cfg(feature = "bitassets")]
    /// Optional refresh interval in seconds for the trusted sidechain data source.
    pub bitassets_rpc_refresh_seconds: Option<u64>,

    #[cfg(feature = "bitassets")]
    /// Whether to enable the Floresta-owned native BitAssets wallet.
    pub enable_bitassets_native_wallet: bool,

    #[cfg(feature = "bitassets")]
    /// Allow native BitAssets wallet signing RPC methods on non-loopback JSON-RPC binds.
    pub allow_remote_bitassets_native_wallet_rpc: bool,

    #[cfg(feature = "bitassets")]
    /// Whether to create the native BitAssets wallet if missing.
    pub bitassets_wallet_create: bool,

    #[cfg(feature = "bitassets")]
    /// Optional 64-byte hex seed used when creating the native BitAssets wallet.
    pub bitassets_wallet_seed: Option<String>,

    #[cfg(feature = "bitassets")]
    /// Optional plain-bitassets lite-wallet QUIC endpoint for native wallet subscriptions.
    pub bitassets_lite_wallet_quic_url: Option<String>,

    #[cfg(feature = "bitassets")]
    /// Disable native wallet QUIC subscriptions and use JSON-RPC polling only.
    pub disable_bitassets_lite_wallet_quic: bool,

    /// Whether to enable the Electrum TLS server.
    pub enable_electrum_tls: bool,

    /// Address the Electrum TLS Server will listen to.
    pub electrum_address_tls: Option<String>,

    /// TLS private key path (defaults to `{data_dir}/tls/key.pem`).
    /// It must be PKCS#8-encoded. You can use `openssl` to generate it:
    ///
    /// ```shell
    /// openssl genpkey -algorithm RSA -out key.pem -pkeyopt rsa_keygen_bits:2048
    /// ```
    pub tls_key_path: Option<String>,

    /// TLS certificate path (defaults to `{data_dir}/tls/cert.pem`).
    /// It must be PKCS#8-encoded. You can use `openssl` to generate it from a PKCS#8-encoded private key:
    ///
    /// ```shell
    /// openssl req -x509 -new -key key.pem -out cert.pem -days 365 -subj "/CN=localhost"
    /// ```
    pub tls_cert_path: Option<String>,

    /// Whether to create self signed certificate for `tls_key_path` and `tls_cert_path`.
    pub generate_cert: bool,

    /// Whether to allow fallback to v1 transport if v2 connection fails.
    pub allow_v1_fallback: bool,
    /// Optional P2P message magic override for private signets.
    pub p2p_magic: Option<Magic>,
    /// Whether we should backfill
    ///
    /// If we assumeutreexo or use pow fraud proofs, you have the option to download and validate
    /// the blocks that were skipped. This will take a long time, but will run on the background
    /// and won't affect the node's operation. You may notice that this will take a lot of CPU
    /// and bandwidth to run.
    pub backfill: bool,
}

impl Config {
    pub fn new(network: Network, data_dir: String) -> Self {
        Self {
            disable_dns_seeds: false,
            data_dir,
            assume_valid: AssumeValidArg::Hardcoded,
            wallet_xpub: None,
            wallet_descriptor: None,
            config_file: None,
            proxy: None,
            network,
            cfilters: false,
            filters_start_height: None,
            #[cfg(feature = "zmq-server")]
            zmq_address: None,
            connect: None,
            #[cfg(feature = "json-rpc")]
            json_rpc_address: None,
            log_to_stdout: false,
            log_to_file: false,
            assume_utreexo: false,
            debug: false,
            user_agent: String::new(),
            assumeutreexo_value: None,
            electrum_address: None,
            #[cfg(feature = "bitassets")]
            enable_bitassets: false,
            #[cfg(feature = "bitassets")]
            bitassets_rpc_url: None,
            #[cfg(feature = "bitassets")]
            bitassets_rpc_refresh_seconds: None,
            #[cfg(feature = "bitassets")]
            enable_bitassets_native_wallet: false,
            #[cfg(feature = "bitassets")]
            allow_remote_bitassets_native_wallet_rpc: false,
            #[cfg(feature = "bitassets")]
            bitassets_wallet_create: false,
            #[cfg(feature = "bitassets")]
            bitassets_wallet_seed: None,
            #[cfg(feature = "bitassets")]
            bitassets_lite_wallet_quic_url: None,
            #[cfg(feature = "bitassets")]
            disable_bitassets_lite_wallet_quic: false,
            enable_electrum_tls: false,
            electrum_address_tls: None,
            generate_cert: false,
            tls_key_path: None,
            tls_cert_path: None,
            allow_v1_fallback: false,
            p2p_magic: None,
            backfill: false,
        }
    }
}

pub struct Florestad {
    /// The config used by this node, see [Config] for more details
    config: Config,

    /// A channel that tells others to stop what they are doing because we
    /// are about to die
    stop_signal: Arc<RwLock<bool>>,

    /// A channel that notifies we are done, and it's safe to die now
    stop_notify: Arc<Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>,

    #[cfg(feature = "json-rpc")]
    /// A handle to our json-rpc server
    json_rpc: OnceLock<tokio::task::JoinHandle<()>>,
}

impl Florestad {
    /// Kills a running florestad, this will return as soon as the main node stops.
    ///
    /// It's not safe to stop your program before this thread returns because some
    /// information may not be fully flushed to disk yet, and killing the process
    /// before flushing everything is equivalent to an unclean shutdown.
    pub async fn stop(&self) {
        info!("Stopping node...");
        let mut stop_signal = self.stop_signal.write().await;
        *stop_signal = true;
    }

    pub async fn should_stop(&self) -> bool {
        let stop_signal = self.stop_signal.read().await;
        *stop_signal
    }

    pub fn get_stop_signal(&self) -> Arc<RwLock<bool>> {
        self.stop_signal.clone()
    }

    pub async fn wait_shutdown(&self) {
        let chan = {
            let mut guard = self.stop_notify.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        if let Some(chan) = chan {
            if let Err(e) = chan.await {
                error!("POSSIBLE BUG: unexpected error while shutting down {e:?}");
            }
        }
    }

    /// Parses an address in the format `<hostname>[<:port>]` and returns a
    /// `SocketAddr` with the resolved IP address. If a hostname is provided,
    /// it will be resolved using the system's DNS resolver. This function will
    /// propagate a [FlorestadError] if it fails to resolve the hostname or the
    /// provided address is invalid.
    fn resolve_hostname(hostname: &str, default_port: u16) -> Result<SocketAddr, FlorestadError> {
        if !hostname.contains(':') {
            return hostname
                .parse()
                .map(|ip| SocketAddr::new(ip, default_port))
                .map_err(FlorestadError::InvalidIpAddress);
        }

        let ip = hostname.parse();
        let sock = match ip {
            Ok(ip) => ip,
            Err(_) => {
                let mut split = hostname.split(':');
                let hostname = split
                    .next()
                    .expect("First element of the iterator is `Some`");

                debug!("Resolving hostname: {hostname}");

                let ips: Vec<_> = match dns_lookup::lookup_host(hostname) {
                    Ok(ips) => ips,
                    Err(e) => {
                        return Err(FlorestadError::CouldNotResolveHostname(e));
                    }
                };

                if ips.is_empty() {
                    return Err(FlorestadError::NoIPAddressesFound(hostname.to_string()));
                }

                let port = split
                    .next()
                    .map(|x| x.parse().unwrap_or(default_port))
                    .unwrap_or(default_port);

                SocketAddr::new(ips[0], port)
            }
        };

        Ok(sock)
    }

    // TODO(@luisschwab): update `datadir.to_str().expect("infallible")` when modifying `floresta-node`'s methods
    /// Actually runs florestad, spawning all modules and waiting until
    /// someone asks to stop.
    ///
    /// This function will return an error if the configured data directory path is not an
    /// **existing and writable directory**, or cannot be validated as such.
    pub async fn start(&self) -> Result<(), FlorestadError> {
        let datadir = PathBuf::from(&self.config.data_dir);

        // Check that the directory exists and is writable
        Florestad::validate_data_dir(datadir.to_str().expect("infallible"))?;

        info!("Loading watch-only wallet");
        let wallet = self.setup_wallet()?;

        info!("Loading blockchain database");
        let blockchain_state = Arc::new(Self::load_chain_state(
            datadir.to_str().expect("infallible").to_owned(),
            self.config.network,
            self.config.assume_valid,
        )?);

        #[cfg(feature = "compact-filters")]
        let cfilters = if self.config.cfilters {
            let filter_store = FlatFiltersStore::new(datadir.join("cfilters"));
            let cfilters = Arc::new(NetworkFilters::new(filter_store));

            let height = cfilters
                .get_height()
                .map_err(FlorestadError::CouldNotLoadCompactFiltersStore)?;

            info!("Loaded compact filters store at height {height}");
            Some(cfilters)
        } else {
            None
        };

        #[cfg(not(feature = "compact-filters"))]
        let cfilters = None;

        // If this network already allows pow fraud proofs, we should use it instead of assumeutreexo
        let assume_utreexo = match self.config.assume_utreexo {
            true => Some(ChainParams::get_assume_utreexo(self.config.network)),

            _ => None,
        };

        let proxy = self
            .config
            .proxy
            .as_ref()
            .map(|addr| Self::resolve_hostname(addr, 9050))
            .transpose()?;

        let config = UtreexoNodeConfig {
            disable_dns_seeds: self.config.disable_dns_seeds,
            network: self.config.network,
            pow_fraud_proofs: false,
            proxy,
            datadir: datadir.clone(),
            fixed_peer: self.config.connect.clone(),
            network_magic: self.config.p2p_magic,
            compact_filters: self.config.cfilters,
            assume_utreexo: self.config.assumeutreexo_value.clone().or(assume_utreexo),
            backfill: self.config.backfill,
            filter_start_height: self.config.filters_start_height,
            user_agent: self.config.user_agent.clone(),
            allow_v1_fallback: self.config.allow_v1_fallback,
            ..Default::default()
        };

        let kill_signal = self.stop_signal.clone();

        // Chain Provider (p2p)
        let chain_provider = UtreexoNode::<_, RunningNode>::new(
            config,
            blockchain_state.clone(),
            Arc::new(tokio::sync::Mutex::new(Mempool::new(
                DEFAULT_MEMPOOL_MAX_SIZE_BYTES,
            ))),
            cfilters.clone(),
            kill_signal.clone(),
            AddressMan::new(None, SUPPORTED_NETWORKS),
        )
        .map_err(|e| FlorestadError::CouldNotCreateChainProvider(format!("{e}")))?;

        // ZMQ
        #[cfg(feature = "zmq-server")]
        {
            info!("Starting ZMQ server");
            if let Ok(zserver) = ZMQServer::new(
                self.config
                    .zmq_address
                    .as_ref()
                    .unwrap_or(&"tcp://127.0.0.1:5150".to_string()),
            ) {
                blockchain_state.subscribe(Arc::new(zserver));
                info!("Done!");
            } else {
                error!("Could not create zmq server, skipping");
            };
        }

        info!("Starting server");
        let wallet = Arc::new(wallet);
        #[cfg(feature = "bitassets")]
        let native_bitassets_wallet = if self.config.enable_bitassets_native_wallet {
            let Some(rpc_url) = self.config.bitassets_rpc_url.clone() else {
                return Err(FlorestadError::CouldNotSetupBitAssetsWallet(
                    "--enable-bitassets-native-wallet requires --bitassets-rpc-url".to_string(),
                ));
            };
            let wallet_path = Self::bitassets_wallet_path(&self.config.data_dir);
            Some(Arc::new(tokio::sync::Mutex::new(
                NativeBitAssetsWallet::open(
                    wallet_path,
                    rpc_url,
                    self.config.bitassets_wallet_seed.as_deref(),
                    self.config.bitassets_wallet_create,
                )
                .map_err(|err| FlorestadError::CouldNotSetupBitAssetsWallet(err.to_string()))?,
            )))
        } else {
            None
        };
        #[cfg(feature = "bitassets")]
        if let Some(wallet) = native_bitassets_wallet.as_ref() {
            if self.config.disable_bitassets_lite_wallet_quic {
                wallet.lock().await.set_quic_enabled(false);
            } else {
                wallet.lock().await.set_quic_enabled(true);
                let quic_url = self
                    .config
                    .bitassets_lite_wallet_quic_url
                    .clone()
                    .or_else(|| {
                        self.config
                            .bitassets_rpc_url
                            .as_deref()
                            .and_then(Self::default_bitassets_quic_url)
                    });
                if let Some(quic_url) = quic_url {
                    Self::spawn_native_bitassets_quic_loop(wallet.clone(), quic_url);
                } else {
                    wallet
                        .lock()
                        .await
                        .set_quic_error("missing --bitassets-lite-wallet-quic-url");
                }
            }
        }

        // JSON-RPC
        #[cfg(feature = "json-rpc")]
        {
            let json_rpc_address = self
                .config
                .json_rpc_address
                .as_ref()
                .map(|x| Self::resolve_hostname(x, 8332))
                .transpose()?;
            #[cfg(feature = "bitassets")]
            if self.config.enable_bitassets_native_wallet
                && !native_wallet_json_rpc_exposure_allowed(
                    json_rpc_address.as_ref(),
                    self.config.allow_remote_bitassets_native_wallet_rpc,
                )
            {
                return Err(FlorestadError::CouldNotSetupBitAssetsWallet(
                    "--enable-bitassets-native-wallet exposes signing RPC methods; bind JSON-RPC to loopback or pass --allow-remote-bitassets-native-wallet-rpc".to_string(),
                ));
            }
            let server = tokio::spawn(json_rpc::server::RpcImpl::create(
                blockchain_state.clone(),
                wallet.clone(),
                chain_provider.get_handle(),
                self.stop_signal.clone(),
                self.config.network,
                cfilters.clone(),
                json_rpc_address,
                datadir
                    .join("debug.log")
                    .to_str()
                    .expect("infallible")
                    .to_owned(),
                #[cfg(feature = "bitassets")]
                native_bitassets_wallet.clone(),
            ));

            if self.json_rpc.set(server).is_err() {
                core::panic!("We should be the first one setting this");
            }
        }

        // Electrum Server configuration.
        #[cfg(feature = "bitassets")]
        let bitasset_index = if self.config.enable_bitassets {
            let bitasset_index = Arc::new(BitAssetIndex::new());
            blockchain_state.subscribe(bitasset_index.clone());
            info!("BitAssets index enabled for Electrum asset methods");
            let persistence_path = Self::bitassets_index_path(&self.config.data_dir);

            match Self::load_persisted_bitassets_index(&persistence_path, &bitasset_index) {
                Ok(0) => {}
                Ok(indexed) => info!(
                    "Loaded {indexed} persisted plain-bitassets sidechain UTXO(s) from {persistence_path}"
                ),
                Err(err) => warn!(
                    "Could not load persisted plain-bitassets sidechain UTXOs from {persistence_path}: {err}"
                ),
            }

            if let Some(rpc_url) = self.config.bitassets_rpc_url.as_ref() {
                match Self::sync_trusted_bitassets_utxos(rpc_url, &bitasset_index) {
                    Ok((indexed, tip)) => {
                        info!(
                            "Indexed {indexed} trusted plain-bitassets sidechain UTXO(s) from {rpc_url} at sidechain height {} tip {:?}",
                            tip.height,
                            tip.hash
                        );
                        match Self::persist_bitassets_index(
                            &persistence_path,
                            &bitasset_index,
                            Some(&tip),
                        ) {
                            Ok(persisted) => info!(
                                "Persisted {persisted} plain-bitassets sidechain UTXO(s) to {persistence_path}"
                            ),
                            Err(err) => warn!(
                                "Could not persist plain-bitassets sidechain UTXOs to {persistence_path}: {err}"
                            ),
                        }
                    }
                    Err(err) => warn!(
                        "Could not index trusted plain-bitassets sidechain UTXOs from {rpc_url}: {err}"
                    ),
                }
                if let Some(refresh_seconds) = self.config.bitassets_rpc_refresh_seconds {
                    if refresh_seconds > 0 {
                        Self::spawn_bitassets_refresh_loop(
                            rpc_url.clone(),
                            bitasset_index.clone(),
                            persistence_path.clone(),
                            refresh_seconds,
                        );
                    }
                }
            }
            Some(bitasset_index)
        } else {
            None
        };

        // Instantiate the Electrum Server.
        let mut electrum_server = ElectrumServer::new(
            wallet,
            blockchain_state,
            cfilters,
            chain_provider.get_handle(),
        )
        .map_err(FlorestadError::CouldNotCreateElectrumServer)?;

        #[cfg(feature = "bitassets")]
        if let Some(bitasset_index) = bitasset_index {
            electrum_server = electrum_server.with_bitasset_index(bitasset_index);
        }
        #[cfg(feature = "bitassets")]
        if let Some(rpc_url) = self.config.bitassets_rpc_url.clone() {
            electrum_server = electrum_server.with_bitassets_rpc_url(rpc_url);
        }

        // Default Electrum Server port.
        let default_electrum_port: u16 =
            Self::get_default_electrum_port(self.config.network, false);

        // Electrum Server address.
        let electrum_addr: SocketAddr = self
            .config
            .electrum_address
            .as_ref()
            .map(|addr| Self::resolve_hostname(addr, default_electrum_port))
            .transpose()?
            .unwrap_or(
                format!("127.0.0.1:{default_electrum_port}")
                    .parse()
                    .expect("Hardcoded address"),
            );
        // sans-TLS Electrum listener.
        let non_tls_listener = TcpListener::bind(electrum_addr)
            .await
            .map(Arc::new)
            .map_err(FlorestadError::FailedToBindElectrumServer)?;

        task::spawn(client_accept_loop(
            non_tls_listener,
            electrum_server.get_notifier(),
            None,
        ));
        info!("Electrum Server is running at {electrum_addr}");

        // with-TLS Electrum listener.
        if self.config.enable_electrum_tls {
            // Default Electrum TLS port.
            let default_electrum_port_tls: u16 =
                Self::get_default_electrum_port(self.config.network, true);

            let electrum_addr_tls = self
                .config
                .electrum_address_tls
                .as_ref()
                .map(|addr| Self::resolve_hostname(addr, default_electrum_port_tls))
                .transpose()?
                .unwrap_or(
                    format!("127.0.0.1:{default_electrum_port_tls}")
                        .parse()
                        .expect("Hardcoded address"),
                );

            // Generate self-signed TLS certificate, if enabled.
            if self.config.generate_cert {
                // Create TLS directory, if it does not exist.
                let tls_dir = datadir.join("tls").to_str().expect("infallible").to_owned();
                if !Path::new(&tls_dir).exists() {
                    fs::create_dir_all(&tls_dir).map_err(|e| {
                        FlorestadError::CouldNotCreateTLSDataDir(tls_dir.clone(), e)
                    })?;
                    info!("Created TLS directory at {tls_dir}");
                }

                // Create information for the self-signed certificate about the current node.
                let subject_alt_names = vec!["localhost".to_string()];

                // Define file paths
                let tls_key_path = datadir
                    .join("tls/key.pem")
                    .to_str()
                    .expect("infallible")
                    .to_owned();
                let tls_cert_path = datadir
                    .join("tls/cert.pem")
                    .to_str()
                    .expect("infallible")
                    .to_owned();

                // Create the certificate.
                Self::generate_self_signed_certificate(
                    tls_key_path.clone(),
                    tls_cert_path.clone(),
                    subject_alt_names,
                )?;

                info!("TLS private key saved to {tls_key_path}");
                info!("TLS certificate saved to {tls_cert_path}");
            }

            // Assemble TLS configuration from file.
            let tls_config = self.create_tls_config(datadir.to_str().expect("infallible"))?;

            // Electrum TLS accept loop.
            let tls_listener = TcpListener::bind(electrum_addr_tls)
                .await
                .map(Arc::new)
                .map_err(FlorestadError::FailedToBindElectrumServer)?;

            // TLS Acceptor.
            let tls_acceptor: TlsAcceptor = TlsAcceptor::from(tls_config);
            task::spawn(client_accept_loop(
                tls_listener,
                electrum_server.get_notifier(),
                Some(tls_acceptor),
            ));
            info!("Electrum TLS Server is running at {electrum_addr_tls}");
        }

        // Electrum Server's main loop.
        task::spawn(electrum_server.main_loop());

        // Chain provider
        let (sender, receiver) = tokio::sync::oneshot::channel();

        let mut recv = self.stop_notify.lock().unwrap();
        *recv = Some(receiver);

        task::spawn(chain_provider.run(sender));

        // Metrics
        #[cfg(feature = "metrics")]
        {
            let metrics_server_address =
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3333);

            task::spawn(metrics::metrics_server(metrics_server_address));
            info!("Started metrics server on: {metrics_server_address}",);

            // Periodically update memory usage
            tokio::spawn(async {
                let interval = Duration::from_secs(5);
                let mut ticker = time::interval(interval);

                loop {
                    ticker.tick().await;
                    metrics::get_metrics().update_memory_usage();
                }
            });
        }

        // All done, return Ok
        Ok(())
    }

    pub fn from_config(config: Config) -> Self {
        Self::from(config)
    }

    pub fn new(network: Network, data_dir: String) -> Self {
        Self::from_config(Config::new(network, data_dir))
    }

    fn validate_data_dir(path: &str) -> Result<(), FlorestadError> {
        let p = Path::new(path);

        let md = fs::metadata(p).map_err(|_| FlorestadError::InvalidDataDir(path.into()))?;
        if !md.is_dir() {
            return Err(FlorestadError::InvalidDataDir(path.into()));
        }

        // Reliable cross-platform writability test:
        let probe = p.join(".perm_probe");
        if OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&probe)
            .is_err()
        {
            return Err(FlorestadError::InvalidDataDir(path.into()));
        }
        let _ = fs::remove_file(probe);

        Ok(())
    }

    /// Load config from disk; prefer explicit `config_file`, otherwise use `{data_dir}/config.toml`.
    /// Returns default if it cannot load it
    fn get_config_file(&self) -> ConfigFile {
        let path = match &self.config.config_file {
            Some(path) => path.clone(),
            None => format!("{}/config.toml", self.config.data_dir),
        };

        let data = ConfigFile::from_file(&path);

        if let Ok(data) = data {
            data
        } else {
            match data.unwrap_err() {
                FlorestadError::TomlParsing(e) => {
                    warn!("Could not parse config file, ignoring it");
                    debug!("{e}");
                    ConfigFile::default()
                }
                FlorestadError::Io(e) => {
                    warn!("Could not read config file, ignoring it");
                    debug!("{e}");
                    ConfigFile::default()
                }
                // Shouldn't be any other error
                _ => unreachable!(),
            }
        }
    }

    fn get_key_from_env() -> Option<String> {
        let xpub = std::env::var("WALLET_XPUB");
        match xpub {
            Ok(key) => return Some(key),
            Err(e) => match e {
                std::env::VarError::NotPresent => {}
                std::env::VarError::NotUnicode(xpub) => error!("Invalid xpub {xpub:?}"),
            },
        }
        None
    }

    fn load_chain_state(
        data_dir: String,
        network: Network,
        assume_valid: AssumeValidArg,
    ) -> Result<ChainState<ChainStore>, FlorestadError> {
        let config = FlatChainStoreConfig::new(data_dir + "/chaindata");
        let store = ChainStore::new(config)
            .map_err(|e| FlorestadError::CouldNotLoadFlatChainStore(e.into()))?;
        ChainState::open(store, network, assume_valid)
            .map_err(FlorestadError::CouldNotLoadFlatChainStore)
    }

    #[cfg(feature = "bitassets")]
    fn sync_trusted_bitassets_utxos(
        rpc_url: &str,
        index: &BitAssetIndex,
    ) -> Result<(usize, BitAssetsSidechainTip), String> {
        let height = Self::bitassets_rpc_call(rpc_url, "getblockcount")?
            .as_u64()
            .ok_or_else(|| "getblockcount result was not a u64".to_string())?
            as u32;
        let hash = Self::bitassets_rpc_call(rpc_url, "get_best_sidechain_block_hash")?
            .as_str()
            .map(ToOwned::to_owned);
        let tip = BitAssetsSidechainTip { height, hash };
        let utxos = Self::bitassets_rpc_call(rpc_url, "list_utxos")?;
        let utxos = utxos
            .as_array()
            .ok_or_else(|| "list_utxos result was not an array".to_string())?;

        let mut records = Vec::new();
        for utxo in utxos {
            let Some(bitasset) = utxo
                .pointer("/output/content/BitAsset")
                .and_then(Value::as_array)
            else {
                continue;
            };

            let asset_id = bitasset
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| "BitAsset content missing asset id".to_string())?
                .parse()
                .map_err(|err| format!("invalid BitAsset asset id: {err}"))?;
            let amount = bitasset
                .get(1)
                .and_then(Value::as_u64)
                .ok_or_else(|| "BitAsset content missing amount".to_string())?;

            let regular = utxo
                .pointer("/outpoint/Regular")
                .ok_or_else(|| "BitAsset UTXO did not have a regular outpoint".to_string())?;
            let txid: bitcoin::Txid = regular
                .get("txid")
                .and_then(Value::as_str)
                .ok_or_else(|| "regular outpoint missing txid".to_string())?
                .parse()
                .map_err(|err| format!("invalid regular outpoint txid: {err}"))?;
            let vout = regular
                .get("vout")
                .and_then(Value::as_u64)
                .ok_or_else(|| "regular outpoint missing vout".to_string())?;
            let vout = u32::try_from(vout)
                .map_err(|err| format!("regular outpoint vout overflow: {err}"))?;
            let (block_hash, proof_status, proof_refs) =
                match Self::bitassets_tx_proof_metadata(rpc_url, &txid.to_string()) {
                    Ok(metadata) => metadata,
                    Err(_) => Self::bitassets_tx_block_hash(rpc_url, &txid.to_string())
                        .map(|block_hash| {
                            (
                                block_hash,
                                AssetProofStatus::TrustedSnapshot,
                                AssetProofRefs::default(),
                            )
                        })
                        .map_err(|err| {
                            format!("could not get transaction info for {txid}: {err}")
                        })?,
                };

            records.push(TrustedSidechainAssetUtxo {
                asset_id,
                outpoint: bitcoin::OutPoint { txid, vout },
                amount,
                height,
                block_hash,
                event_kind: AssetHistoryEventKind::SidechainUnspent,
                proof_status,
                proof_refs,
            });
        }

        let indexed = index.replace_with_trusted_sidechain_utxos(records);
        Ok((indexed, tip))
    }

    #[cfg(feature = "bitassets")]
    fn bitassets_index_path(data_dir: &str) -> String {
        format!("{data_dir}/{BITASSETS_INDEX_FILE}")
    }

    #[cfg(feature = "bitassets")]
    fn bitassets_wallet_path(data_dir: &str) -> String {
        format!("{data_dir}/{BITASSETS_WALLET_FILE}")
    }

    #[cfg(feature = "bitassets")]
    fn load_persisted_bitassets_index(path: &str, index: &BitAssetIndex) -> Result<usize, String> {
        if !Path::new(path).exists() {
            return Ok(0);
        }

        let bytes = fs::read(path).map_err(|err| format!("read failed: {err}"))?;
        let persisted: PersistedBitAssetIndex =
            serde_json::from_slice(&bytes).map_err(|err| format!("decode failed: {err}"))?;
        if persisted.version != 1 {
            return Err(format!(
                "unsupported persisted bitassets index version {}",
                persisted.version
            ));
        }

        let mut records = Vec::with_capacity(persisted.utxos.len());
        for utxo in persisted.utxos {
            records.push(TrustedSidechainAssetUtxo {
                asset_id: utxo
                    .asset_id
                    .parse()
                    .map_err(|err| format!("invalid persisted asset id: {err}"))?,
                outpoint: bitcoin::OutPoint {
                    txid: utxo
                        .txid
                        .parse()
                        .map_err(|err| format!("invalid persisted txid: {err}"))?,
                    vout: utxo.vout,
                },
                amount: utxo.asset_amount,
                height: utxo.height,
                block_hash: utxo
                    .block_hash
                    .map(|hash| {
                        hash.parse()
                            .map_err(|err| format!("invalid persisted block hash: {err}"))
                    })
                    .transpose()?,
                event_kind: match utxo.event_kind.as_str() {
                    "sidechain_unspent" => AssetHistoryEventKind::SidechainUnspent,
                    _ => AssetHistoryEventKind::SidechainUnspent,
                },
                proof_status: match utxo.proof_status.as_str() {
                    "sidechain_rpc_proof" => AssetProofStatus::SidechainRpcProof,
                    _ => AssetProofStatus::TrustedSnapshot,
                },
                proof_refs: AssetProofRefs {
                    sidechain_block_height: utxo.sidechain_block_height,
                    bmm_inclusions: utxo.bmm_inclusions,
                    best_main_verification: utxo.best_main_verification,
                },
            });
        }

        Ok(index.replace_with_trusted_sidechain_utxos(records))
    }

    #[cfg(feature = "bitassets")]
    fn persist_bitassets_index(
        path: &str,
        index: &BitAssetIndex,
        tip: Option<&BitAssetsSidechainTip>,
    ) -> Result<usize, String> {
        let utxos = index
            .get_all_asset_utxo_records()
            .into_iter()
            .map(PersistedAssetUtxo::from)
            .collect::<Vec<_>>();
        let persisted_count = utxos.len();
        let persisted = PersistedBitAssetIndex {
            version: 1,
            sidechain_height: tip.map(|tip| tip.height),
            sidechain_tip: tip.and_then(|tip| tip.hash.clone()),
            utxos,
        };
        let bytes =
            serde_json::to_vec_pretty(&persisted).map_err(|err| format!("encode failed: {err}"))?;
        let tmp_path = format!("{path}.tmp");
        fs::write(&tmp_path, bytes).map_err(|err| format!("write failed: {err}"))?;
        fs::rename(&tmp_path, path).map_err(|err| format!("rename failed: {err}"))?;

        Ok(persisted_count)
    }

    #[cfg(feature = "bitassets")]
    fn bitassets_rpc_call(rpc_url: &str, method: &str) -> Result<Value, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "florestad-bitassets-sync",
            "method": method,
            "params": []
        });
        let mut response = ureq::post(rpc_url)
            .header("content-type", "application/json")
            .send_json(body)
            .map_err(|err| format!("request failed for {method}: {err}"))?;
        let value = response
            .body_mut()
            .read_json::<Value>()
            .map_err(|err| format!("invalid JSON response for {method}: {err}"))?;

        if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
            return Err(format!("RPC error for {method}: {error}"));
        }

        value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("RPC response for {method} did not include result"))
    }

    #[cfg(feature = "bitassets")]
    fn bitassets_rpc_call_with_params(
        rpc_url: &str,
        method: &str,
        params: Vec<Value>,
    ) -> Result<Value, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "florestad-bitassets-sync",
            "method": method,
            "params": params
        });
        let mut response = ureq::post(rpc_url)
            .header("content-type", "application/json")
            .send_json(body)
            .map_err(|err| format!("request failed for {method}: {err}"))?;
        let value = response
            .body_mut()
            .read_json::<Value>()
            .map_err(|err| format!("invalid JSON response for {method}: {err}"))?;

        if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
            return Err(format!("RPC error for {method}: {error}"));
        }

        value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("RPC response for {method} did not include result"))
    }

    #[cfg(feature = "bitassets")]
    fn bitassets_tx_block_hash(rpc_url: &str, txid: &str) -> Result<Option<bitcoin::Txid>, String> {
        let Some(block_hash) = Self::bitassets_rpc_call_with_params(
            rpc_url,
            "get_transaction_info",
            vec![json!(txid)],
        )?
        .pointer("/txin/block_hash")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned) else {
            return Ok(None);
        };

        block_hash
            .parse()
            .map(Some)
            .map_err(|err| format!("invalid transaction block hash: {err}"))
    }

    #[cfg(feature = "bitassets")]
    fn bitassets_tx_proof_metadata(
        rpc_url: &str,
        txid: &str,
    ) -> Result<(Option<bitcoin::Txid>, AssetProofStatus, AssetProofRefs), String> {
        let proof = Self::bitassets_rpc_call_with_params(
            rpc_url,
            "get_transaction_proof",
            vec![json!(txid)],
        )?;
        if proof.is_null() {
            return Ok((
                None,
                AssetProofStatus::TrustedSnapshot,
                AssetProofRefs::default(),
            ));
        }

        let block_hash = proof
            .pointer("/txin/block_hash")
            .and_then(Value::as_str)
            .map(str::parse)
            .transpose()
            .map_err(|err| format!("invalid transaction proof block hash: {err}"))?;
        let sidechain_block_height = proof
            .get("sidechain_block_height")
            .and_then(Value::as_u64)
            .map(u32::try_from)
            .transpose()
            .map_err(|err| format!("invalid transaction proof sidechain height: {err}"))?;
        let bmm_inclusions = proof
            .get("bmm_inclusions")
            .and_then(Value::as_array)
            .map(|inclusions| {
                inclusions
                    .iter()
                    .map(|inclusion| {
                        inclusion
                            .as_str()
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| "BMM inclusion was not a string".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let best_main_verification = proof
            .get("best_main_verification")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let has_sidechain_archive = block_hash.is_some()
            && proof.get("block").is_some_and(|block| !block.is_null())
            && !bmm_inclusions.is_empty()
            && best_main_verification.is_some();
        let proof_status = if has_sidechain_archive {
            AssetProofStatus::SidechainRpcProof
        } else {
            AssetProofStatus::TrustedSnapshot
        };

        Ok((
            block_hash,
            proof_status,
            AssetProofRefs {
                sidechain_block_height,
                bmm_inclusions,
                best_main_verification,
            },
        ))
    }

    #[cfg(feature = "bitassets")]
    fn spawn_bitassets_refresh_loop(
        rpc_url: String,
        bitasset_index: Arc<BitAssetIndex>,
        persistence_path: String,
        refresh_seconds: u64,
    ) {
        info!(
            "Refreshing trusted plain-bitassets sidechain UTXOs every {refresh_seconds} second(s)"
        );
        task::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(refresh_seconds));
            loop {
                interval.tick().await;
                let rpc_url = rpc_url.clone();
                let bitasset_index = bitasset_index.clone();
                let persistence_path = persistence_path.clone();
                let sync_result = task::spawn_blocking(move || {
                    let (indexed, tip) =
                        Self::sync_trusted_bitassets_utxos(&rpc_url, &bitasset_index)?;
                    let persisted = Self::persist_bitassets_index(
                        &persistence_path,
                        &bitasset_index,
                        Some(&tip),
                    )?;
                    Ok::<_, String>((indexed, persisted, tip))
                })
                .await;

                match sync_result {
                    Ok(Ok((indexed, persisted, tip))) => {
                        info!(
                            "Refreshed {indexed} trusted plain-bitassets sidechain UTXO(s), persisted {persisted}, sidechain height {} tip {:?}",
                            tip.height,
                            tip.hash
                        )
                    }
                    Ok(Err(err)) => {
                        warn!("Could not refresh trusted plain-bitassets sidechain UTXOs: {err}")
                    }
                    Err(err) => warn!("Trusted plain-bitassets refresh task failed: {err}"),
                }
            }
        });
    }

    #[cfg(feature = "bitassets")]
    fn default_bitassets_quic_url(rpc_url: &str) -> Option<String> {
        let without_scheme = rpc_url.split_once("://").map_or(rpc_url, |(_, rest)| rest);
        let authority = without_scheme.split('/').next()?;
        let (host, port) = authority.rsplit_once(':')?;
        let port = port.parse::<u16>().ok()?.checked_add(100)?;
        Some(format!("{host}:{port}"))
    }

    #[cfg(feature = "bitassets")]
    fn spawn_native_bitassets_quic_loop(
        wallet: Arc<tokio::sync::Mutex<NativeBitAssetsWallet>>,
        quic_url: String,
    ) {
        info!("Starting native BitAssets lite-wallet QUIC subscription to {quic_url}");
        task::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            loop {
                let result = Self::run_native_bitassets_quic_once(wallet.clone(), &quic_url).await;
                if let Err(err) = result {
                    warn!("Native BitAssets lite-wallet QUIC disconnected: {err}");
                    let should_reconnect_immediately = err.contains("wallet script hashes changed")
                        || wallet
                            .lock()
                            .await
                            .wallet_info()
                            .get("quic")
                            .and_then(|quic| quic.get("last_message_unix_ms"))
                            .is_some_and(|value| !value.is_null());
                    {
                        let mut wallet = wallet.lock().await;
                        wallet.set_quic_error(err.clone());
                    }
                    let wallet_for_sync = wallet.clone();
                    let _ = task::spawn_blocking(move || {
                        let mut wallet = wallet_for_sync.blocking_lock();
                        wallet.sync()
                    })
                    .await;
                    if should_reconnect_immediately {
                        backoff = Duration::from_secs(1);
                    }
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        });
    }

    #[cfg(feature = "bitassets")]
    async fn run_native_bitassets_quic_once(
        wallet: Arc<tokio::sync::Mutex<NativeBitAssetsWallet>>,
        quic_url: &str,
    ) -> Result<(), String> {
        let remote: SocketAddr = quic_url
            .parse()
            .map_err(|err| format!("invalid QUIC address {quic_url}: {err}"))?;
        let endpoint = Self::native_bitassets_quic_endpoint(remote)?;
        let connection = endpoint
            .connect(remote, "localhost")
            .map_err(|err| format!("QUIC connect setup failed: {err}"))?
            .await
            .map_err(|err| format!("QUIC connect failed: {err}"))?;
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|err| format!("QUIC stream open failed: {err}"))?;
        let (script_hashes, from_block_hash, subscription_generation) = {
            let mut wallet = wallet.lock().await;
            wallet.set_quic_connected(true);
            (
                wallet.script_hashes().map_err(|err| err.to_string())?,
                wallet.last_tip_hash(),
                wallet.subscription_generation(),
            )
        };
        if script_hashes.is_empty() {
            return Err("native BitAssets wallet has no script hashes to subscribe".to_string());
        }
        let request = json!({
            "type": "subscribe",
            "script_hashes": script_hashes,
            "from_block_hash": from_block_hash,
        });
        let request = serde_json::to_vec(&request).map_err(|err| err.to_string())?;
        send.write_all(&request)
            .await
            .map_err(|err| format!("QUIC subscribe write failed: {err}"))?;
        send.finish()
            .map_err(|err| format!("QUIC subscribe finish failed: {err}"))?;

        let mut buffer = Vec::<u8>::new();
        loop {
            let chunk = match tokio::time::timeout(
                Duration::from_secs(2),
                recv.read_chunk(64 * 1024, true),
            )
            .await
            {
                Ok(result) => result.map_err(|err| format!("QUIC read failed: {err}"))?,
                Err(_) => {
                    if wallet.lock().await.subscription_generation() != subscription_generation {
                        return Err("wallet script hashes changed; resubscribing".to_string());
                    }
                    continue;
                }
            };
            let Some(chunk) = chunk else {
                break;
            };
            buffer.extend_from_slice(&chunk.bytes);
            while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                let line = buffer.drain(..=newline).collect::<Vec<_>>();
                let line = &line[..line.len().saturating_sub(1)];
                if line.is_empty() {
                    continue;
                }
                if wallet.lock().await.subscription_generation() != subscription_generation {
                    return Err("wallet script hashes changed; resubscribing".to_string());
                }
                Self::apply_native_bitassets_quic_message(wallet.clone(), line).await?;
            }
        }
        Err("QUIC stream closed".to_string())
    }

    #[cfg(feature = "bitassets")]
    async fn apply_native_bitassets_quic_message(
        wallet: Arc<tokio::sync::Mutex<NativeBitAssetsWallet>>,
        line: &[u8],
    ) -> Result<(), String> {
        let message: Value = serde_json::from_slice(line)
            .map_err(|err| format!("invalid QUIC JSON message: {err}"))?;
        let message_type = message
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "QUIC message missing type".to_string())?;
        if message_type == "error" {
            let message = message
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("plain-bitassets lite-wallet error");
            return Err(message.to_string());
        }
        let update = match message_type {
            "snapshot" | "mempool" | "confirmed" => message
                .get("update")
                .ok_or_else(|| format!("{message_type} message missing update"))?,
            other => return Err(format!("unknown QUIC message type {other}")),
        };
        let mut wallet = wallet.lock().await;
        wallet
            .apply_quic_update(update)
            .map_err(|err| format!("could not apply {message_type} update: {err}"))?;
        Ok(())
    }

    #[cfg(feature = "bitassets")]
    fn native_bitassets_quic_endpoint(remote: SocketAddr) -> Result<quinn::Endpoint, String> {
        let bind_addr: SocketAddr = if remote.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        }
        .parse()
        .map_err(|err| format!("invalid QUIC bind address: {err}"))?;
        let mut endpoint = quinn::Endpoint::client(bind_addr)
            .map_err(|err| format!("could not create QUIC endpoint: {err}"))?;
        endpoint.set_default_client_config(Self::native_bitassets_quic_client_config()?);
        Ok(endpoint)
    }

    #[cfg(feature = "bitassets")]
    fn native_bitassets_quic_client_config() -> Result<quinn::ClientConfig, String> {
        #[derive(Debug)]
        struct SkipServerVerification;

        impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
            fn verify_server_cert(
                &self,
                _end_entity: &rustls::pki_types::CertificateDer,
                _intermediates: &[rustls::pki_types::CertificateDer],
                _server_name: &rustls::pki_types::ServerName,
                _ocsp_response: &[u8],
                _now: rustls::pki_types::UnixTime,
            ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                message: &[u8],
                cert: &rustls::pki_types::CertificateDer<'_>,
                dss: &rustls::DigitallySignedStruct,
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
                rustls::crypto::verify_tls12_signature(
                    message,
                    cert,
                    dss,
                    &rustls::crypto::ring::default_provider().signature_verification_algorithms,
                )
            }

            fn verify_tls13_signature(
                &self,
                message: &[u8],
                cert: &rustls::pki_types::CertificateDer<'_>,
                dss: &rustls::DigitallySignedStruct,
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
                rustls::crypto::verify_tls13_signature(
                    message,
                    cert,
                    dss,
                    &rustls::crypto::ring::default_provider().signature_verification_algorithms,
                )
            }

            fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                rustls::crypto::ring::default_provider()
                    .signature_verification_algorithms
                    .supported_schemes()
            }
        }

        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();
        let client_config = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|err| format!("could not create QUIC rustls client config: {err}"))?;
        Ok(quinn::ClientConfig::new(Arc::new(client_config)))
    }

    /// Setup the wallet by initializing the database and adding descriptors, xpubs, and addresses.
    fn setup_wallet(&self) -> Result<AddressCache<KvDatabase>, FlorestadError> {
        let database = KvDatabase::new(self.config.data_dir.clone())
            .map_err(FlorestadError::CouldNotOpenKvDatabase)?;

        let wallet = AddressCache::new(database);

        wallet
            .setup()
            .map_err(FlorestadError::CouldNotInitializeWallet)?;

        // Add the configured descriptors and addresses to the wallet
        for descriptor in self.get_descriptors() {
            match wallet.push_descriptor(&descriptor) {
                Ok(_) => info!("Added descriptor to wallet: {descriptor}"),
                Err(WatchOnlyError::DuplicateDescriptor(_)) => {
                    warn!("Descriptor already exists in wallet, skipping: {descriptor}");
                }
                Err(e) => {
                    return Err(FlorestadError::from(e));
                }
            }
        }

        for xpub in self.get_xpubs() {
            match wallet.push_xpub(&xpub, self.config.network) {
                Ok(()) => info!("Added xpubs to wallet: {xpub}"),
                Err(WatchOnlyError::DuplicateDescriptor(_)) =>
                    warn!("Descriptor for the provided XPUB already exists in the wallet. Skipping: {xpub}"),
                Err(e) => return Err(FlorestadError::from(e))
            }
        }

        for address in self.get_addresses()? {
            wallet.cache_address(address);
        }

        info!("Wallet setup completed!");
        Ok(wallet)
    }

    /// Get the wallet descriptors from the config file
    fn get_descriptors(&self) -> Vec<String> {
        self.config
            .wallet_descriptor
            .iter()
            .flatten()
            .chain(self.get_config_file().wallet.descriptors.iter().flatten())
            .cloned()
            .collect()
    }

    /// Get the wallet xpubs from the config file and the environment
    fn get_xpubs(&self) -> Vec<String> {
        self.config
            .wallet_xpub
            .iter()
            .flatten()
            .chain(self.get_config_file().wallet.xpubs.iter().flatten())
            .chain(Self::get_key_from_env().iter())
            .cloned()
            .collect()
    }

    /// Get the wallet addresses from the config file
    fn get_addresses(&self) -> Result<Vec<ScriptBuf>, FlorestadError> {
        self.get_config_file()
            .wallet
            .addresses
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|addr_str| {
                Address::from_str(addr_str)
                    .map(|addr| addr.assume_checked().script_pubkey())
                    .map_err(|e| {
                        error!("Invalid address provided: {addr_str} \nReason: {e:?}");
                        FlorestadError::from(e)
                    })
            })
            .collect()
    }

    /// Get the default Electrum port for the Network and TLS combination.
    ///
    /// Bitcoin  => 50001 (50002 TLS)
    /// Signet   => 60001 (60002 TLS)
    /// Testnet4 => 40001 (40003 TLS)
    /// Testnet3 => 30001 (30002 TLS)
    /// Regtest  => 20001 (20002 TLS)
    fn get_default_electrum_port(network: Network, enable_electrum_tls: bool) -> u16 {
        let mut electrum_port = match network {
            Network::Bitcoin => 50001,
            Network::Signet => 60001,
            Network::Testnet4 => 40001,
            Network::Testnet => 30001,
            Network::Regtest => 20001,
        };

        if enable_electrum_tls {
            electrum_port += 1;
        }

        electrum_port
    }

    /// Generate a self-signed TLS certificate from a random private key.
    pub fn generate_self_signed_certificate(
        tls_key_path: String,
        tls_cert_path: String,
        subject_alt_names: Vec<String>,
    ) -> Result<(), FlorestadError> {
        // Generate a key pair
        let tls_key_pair = KeyPair::generate().map_err(FlorestadError::CouldNotGenerateKeypair)?;

        // Generate self-signed certificate
        let mut cert_params = CertificateParams::new(subject_alt_names)
            .map_err(FlorestadError::CouldNotGenerateCertParam)?;

        cert_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let certificate = cert_params
            .self_signed(&tls_key_pair)
            .map_err(FlorestadError::CouldNotGenerateSelfSignedCert)?;

        // Create files
        fs::write(&tls_key_path, tls_key_pair.serialize_pem())
            .map_err(|err| FlorestadError::CouldNotWriteFile(tls_key_path, err))?;

        fs::write(&tls_cert_path, certificate.pem())
            .map_err(|err| FlorestadError::CouldNotWriteFile(tls_cert_path, err))?;

        Ok(())
    }

    /// Create the TLS configuration from a PKCS#8 private key and certificate.
    fn create_tls_config(&self, data_dir: &str) -> Result<Arc<ServerConfig>, FlorestadError> {
        // Use an agnostic way to build paths for platforms and fix the differences
        // in how Unix and Windows represent strings, maybe a user could use a weird
        // string on his/her path.
        //
        // See more at https://doc.rust-lang.org/std/ffi/struct.OsStr.html#method.to_string_lossy
        let tls_cert_path = self.config.tls_cert_path.clone().unwrap_or_else(|| {
            PathBuf::from(&data_dir)
                .join("tls")
                .join("cert.pem")
                .to_string_lossy()
                .into_owned()
        });

        let tls_key_path = self.config.tls_key_path.clone().unwrap_or_else(|| {
            PathBuf::from(&data_dir)
                .join("tls")
                .join("key.pem")
                .to_string_lossy()
                .into_owned()
        });

        // Convert paths to a [`Path`] for system-agnostic handling.
        let tls_cert_path = Path::new(&tls_cert_path);
        let tls_key_path = Path::new(&tls_key_path);

        // Parse the certificate's chain from the file.
        let tls_cert_chain =
            CertificateDer::from_pem_file(tls_cert_path).map_err(FlorestadError::InvalidCert)?;

        // Parse the private key from the file.
        let tls_key =
            PrivateKeyDer::from_pem_file(tls_key_path).map_err(FlorestadError::InvalidPrivKey)?;

        // Assemble the TLS configuration.
        let tls_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![tls_cert_chain], tls_key)
            .map_err(FlorestadError::CouldNotConfigureTLS)?;

        Ok(Arc::new(tls_config))
    }
}

impl From<Config> for Florestad {
    fn from(config: Config) -> Self {
        Self {
            config,
            stop_signal: Arc::new(RwLock::new(false)),
            stop_notify: Arc::new(Mutex::new(None)),
            #[cfg(feature = "json-rpc")]
            json_rpc: OnceLock::new(),
        }
    }
}

#[cfg(all(feature = "bitassets", feature = "json-rpc"))]
fn native_wallet_json_rpc_exposure_allowed(
    bind_address: Option<&SocketAddr>,
    allow_remote: bool,
) -> bool {
    allow_remote
        || bind_address
            .map(|address| address.ip().is_loopback())
            .unwrap_or(true)
}

#[cfg(all(test, feature = "bitassets"))]
mod tests {
    use super::*;
    use bitcoin::Txid;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_cache_path(name: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("florestad-bitassets-{name}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(BITASSETS_INDEX_FILE)
            .to_string_lossy()
            .into_owned()
    }

    fn sample_txid(byte: u8) -> Txid {
        let hex = format!("{byte:02x}").repeat(32);
        hex.parse().expect("valid txid")
    }

    #[cfg(feature = "bitassets")]
    fn test_native_bitassets_wallet() -> Arc<tokio::sync::Mutex<NativeBitAssetsWallet>> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("florestad-native-bitassets-wallet-{nanos}"));
        fs::create_dir_all(&dir).expect("create wallet temp dir");
        let wallet_path = dir.join(BITASSETS_WALLET_FILE);
        fs::write(&wallet_path, "").ok();
        fs::remove_file(&wallet_path).ok();
        Arc::new(tokio::sync::Mutex::new(
            NativeBitAssetsWallet::open_with_seed_storage(
                wallet_path,
                "http://127.0.0.1:6004".to_string(),
                Some(concat!(
                    "0000000000000000000000000000000000000000000000000000000000000000",
                    "0000000000000000000000000000000000000000000000000000000000000000"
                )),
                true,
                false,
            )
            .expect("open native BitAssets wallet"),
        ))
    }

    #[cfg(feature = "bitassets")]
    #[test]
    fn native_bitassets_quic_url_defaults_to_rpc_port_plus_100() {
        assert_eq!(
            Florestad::default_bitassets_quic_url("http://127.0.0.1:6004"),
            Some("127.0.0.1:6104".to_string())
        );
        assert_eq!(
            Florestad::default_bitassets_quic_url("https://bitassets.local:18443/rpc"),
            Some("bitassets.local:18543".to_string())
        );
        assert_eq!(Florestad::default_bitassets_quic_url("unix:/tmp/rpc"), None);
    }

    #[cfg(feature = "bitassets")]
    #[tokio::test]
    async fn native_bitassets_quic_message_surfaces_server_errors() {
        let err = Florestad::apply_native_bitassets_quic_message(
            test_native_bitassets_wallet(),
            br#"{"type":"error","message":"resync from snapshot"}"#,
        )
        .await
        .expect_err("server error message must be surfaced");

        assert_eq!(err, "resync from snapshot");
    }

    #[cfg(feature = "bitassets")]
    #[tokio::test]
    async fn native_bitassets_quic_message_rejects_unknown_or_incomplete_messages() {
        let err = Florestad::apply_native_bitassets_quic_message(
            test_native_bitassets_wallet(),
            br#"{"type":"wat"}"#,
        )
        .await
        .expect_err("unknown message types must be rejected");
        assert!(err.contains("unknown QUIC message type"));

        let err = Florestad::apply_native_bitassets_quic_message(
            test_native_bitassets_wallet(),
            br#"{"type":"confirmed"}"#,
        )
        .await
        .expect_err("confirmed messages must include update payloads");
        assert!(err.contains("confirmed message missing update"));
    }

    #[cfg(feature = "json-rpc")]
    #[test]
    fn native_wallet_json_rpc_requires_loopback_unless_explicitly_allowed() {
        let loopback = "127.0.0.1:8332".parse().unwrap();
        let wildcard = "0.0.0.0:8332".parse().unwrap();
        let remote = "192.168.1.10:8332".parse().unwrap();

        assert!(native_wallet_json_rpc_exposure_allowed(None, false));
        assert!(native_wallet_json_rpc_exposure_allowed(
            Some(&loopback),
            false
        ));
        assert!(!native_wallet_json_rpc_exposure_allowed(
            Some(&wildcard),
            false
        ));
        assert!(!native_wallet_json_rpc_exposure_allowed(
            Some(&remote),
            false
        ));
        assert!(native_wallet_json_rpc_exposure_allowed(Some(&remote), true));
    }

    #[test]
    fn persisted_bitassets_index_loads_legacy_cache_as_trusted_snapshot() {
        let path = unique_cache_path("legacy");
        let asset_id = sample_txid(0x11);
        let txid = sample_txid(0x22);
        let cache = json!({
            "version": 1,
            "sidechain_height": 42,
            "sidechain_tip": sample_txid(0x33).to_string(),
            "utxos": [{
                "asset_id": asset_id.to_string(),
                "txid": txid.to_string(),
                "vout": 0,
                "asset_amount": 100,
                "bitcoin_value": 0,
                "height": 42,
                "block_hash": sample_txid(0x44).to_string(),
                "event_kind": "sidechain_unspent"
            }]
        });
        fs::write(&path, serde_json::to_vec_pretty(&cache).unwrap()).unwrap();

        let index = BitAssetIndex::new();
        assert_eq!(
            Florestad::load_persisted_bitassets_index(&path, &index),
            Ok(1)
        );

        let records = index.get_asset_utxo_records(&asset_id);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].asset_amount, 100);
        assert_eq!(records[0].proof_status, AssetProofStatus::TrustedSnapshot);
        assert_eq!(records[0].proof_refs, AssetProofRefs::default());
    }

    #[test]
    fn persisted_bitassets_index_roundtrips_compact_proof_refs() {
        let path = unique_cache_path("proof-refs");
        let asset_id = sample_txid(0x55);
        let txid = sample_txid(0x66);
        let block_hash = sample_txid(0x77);
        let bmm_inclusion = sample_txid(0x88).to_string();
        let best_main_verification = sample_txid(0x99).to_string();
        let index = BitAssetIndex::new();
        index.replace_with_trusted_sidechain_utxos([TrustedSidechainAssetUtxo {
            asset_id,
            outpoint: bitcoin::OutPoint { txid, vout: 1 },
            amount: 900,
            height: 77,
            block_hash: Some(block_hash),
            event_kind: AssetHistoryEventKind::SidechainUnspent,
            proof_status: AssetProofStatus::SidechainRpcProof,
            proof_refs: AssetProofRefs {
                sidechain_block_height: Some(77),
                bmm_inclusions: vec![bmm_inclusion.clone()],
                best_main_verification: Some(best_main_verification.clone()),
            },
        }]);

        let tip = BitAssetsSidechainTip {
            height: 77,
            hash: Some(block_hash.to_string()),
        };
        assert_eq!(
            Florestad::persist_bitassets_index(&path, &index, Some(&tip)),
            Ok(1)
        );

        let reloaded = BitAssetIndex::new();
        assert_eq!(
            Florestad::load_persisted_bitassets_index(&path, &reloaded),
            Ok(1)
        );
        let records = reloaded.get_asset_utxo_records(&asset_id);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].proof_status, AssetProofStatus::SidechainRpcProof);
        assert_eq!(records[0].proof_refs.sidechain_block_height, Some(77));
        assert_eq!(records[0].proof_refs.bmm_inclusions, vec![bmm_inclusion]);
        assert_eq!(
            records[0].proof_refs.best_main_verification.as_deref(),
            Some(best_main_verification.as_str())
        );
    }

    #[test]
    fn persisted_bitassets_index_rejects_malformed_cache() {
        let path = unique_cache_path("malformed");
        fs::write(&path, b"{not-json").unwrap();

        let index = BitAssetIndex::new();
        let err = Florestad::load_persisted_bitassets_index(&path, &index)
            .expect_err("malformed cache should fail");
        assert!(err.contains("decode failed"));
        assert!(index.list_assets().is_empty());
    }
}
