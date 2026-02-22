use std::fs;
#[cfg(feature = "metrics")]
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(feature = "json-rpc")]
use std::sync::OnceLock;

pub use bitcoin::Network;
#[cfg(feature = "zmq-server")]
use floresta_chain::pruned_utreexo::BlockchainInterface;
pub use floresta_chain::AssumeUtreexoValue;
pub use floresta_chain::AssumeValidArg;
use floresta_chain::BlockchainError;
use floresta_chain::ChainState;
use floresta_chain::FlatChainStore as ChainStore;
use floresta_chain::FlatChainStoreConfig;
#[cfg(feature = "compact-filters")]
use floresta_compact_filters::flat_filters_store::FlatFiltersStore;
#[cfg(feature = "compact-filters")]
use floresta_compact_filters::network_filters::NetworkFilters;
use floresta_electrum::electrum_protocol::client_accept_loop;
use floresta_electrum::electrum_protocol::ElectrumServer;
use floresta_mempool::Mempool;
use floresta_watch_only::kv_database::KvDatabase;
use floresta_watch_only::AddressCache;
use floresta_wire::address_man::AddressMan;
use floresta_wire::address_man::ReachableNetworks;
use floresta_wire::node::running_ctx::RunningNode;
use floresta_wire::node::UtreexoNode;
use floresta_wire::UtreexoNodeConfig;
use rcgen::BasicConstraints;
use rcgen::CertificateParams;
use rcgen::IsCa;
use rcgen::KeyPair;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::task;
#[cfg(feature = "metrics")]
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

use crate::config_file::ConfigFile;
use crate::error::FlorestadError;
use crate::florestad::fs::OpenOptions;
#[cfg(feature = "json-rpc")]
use crate::json_rpc;
use crate::wallet_input::InitialWalletSetup;
#[cfg(feature = "zmq-server")]
use crate::zmq::ZMQServer;

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
            enable_electrum_tls: false,
            electrum_address_tls: None,
            generate_cert: false,
            tls_key_path: None,
            tls_cert_path: None,
            allow_v1_fallback: false,
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

    /// Actually runs florestad, spawning all modules and waiting until
    /// someone asks to stop.
    ///
    /// This function will return an error if the configured data directory path is not an
    /// **existing and writable directory**, or cannot be validated as such.
    pub async fn start(&self) -> Result<(), FlorestadError> {
        let data_dir = &self.config.data_dir;

        // Check that the directory exists and is writable
        Florestad::validate_data_dir(data_dir)?;

        info!("Loading watch-only wallet");
        let mut wallet = Self::load_wallet(data_dir)?;
        wallet
            .setup()
            .map_err(FlorestadError::CouldNotInitializeWallet)?;

        // Try to add more wallets to watch if needed
        self.setup_wallet(data_dir, &mut wallet)?;

        info!("Loading blockchain database");
        let blockchain_state = Arc::new(Self::load_chain_state(
            data_dir.clone(),
            self.config.network,
            self.config.assume_valid,
        )?);

        #[cfg(feature = "compact-filters")]
        let cfilters = if self.config.cfilters {
            // Block Filters
            let filter_store = FlatFiltersStore::new((data_dir.clone() + "/cfilters").into());
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

        // For now, we only have compatible bridges on signet
        let pow_fraud_proofs = match self.config.network {
            Network::Bitcoin => false,
            Network::Signet => true,
            Network::Testnet => false,
            Network::Testnet4 => false,
            Network::Regtest => false,
        };

        // If this network already allows pow fraud proofs, we should use it instead of assumeutreexo
        let assume_utreexo = match (pow_fraud_proofs, self.config.assume_utreexo) {
            (false, true) => Some(floresta_chain::ChainParams::get_assume_utreexo(
                self.config.network,
            )),
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
            pow_fraud_proofs,
            proxy,
            datadir: data_dir.clone(),
            fixed_peer: self.config.connect.clone(),
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
            AddressMan::new(None, &[ReachableNetworks::IPv4, ReachableNetworks::IPv6]),
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

        // JSON-RPC
        #[cfg(feature = "json-rpc")]
        {
            let server = tokio::spawn(json_rpc::server::RpcImpl::create(
                blockchain_state.clone(),
                wallet.clone(),
                chain_provider.get_handle(),
                self.stop_signal.clone(),
                self.config.network,
                cfilters.clone(),
                self.config
                    .json_rpc_address
                    .as_ref()
                    .map(|x| Self::resolve_hostname(x, 8332))
                    .transpose()?,
                format!("{data_dir}/debug.log"),
            ));

            if self.json_rpc.set(server).is_err() {
                core::panic!("We should be the first one setting this");
            }
        }

        // Electrum Server configuration.

        // Instantiate the Electrum Server.
        let electrum_server = ElectrumServer::new(
            wallet,
            blockchain_state,
            cfilters,
            chain_provider.get_handle(),
        )
        .map_err(FlorestadError::CouldNotCreateElectrumServer)?;

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
                let tls_dir = format!("{data_dir}/tls");
                if !Path::new(&tls_dir).exists() {
                    fs::create_dir_all(&tls_dir).map_err(|e| {
                        FlorestadError::CouldNotCreateTLSDataDir(tls_dir.clone(), e)
                    })?;
                    info!("Created TLS directory at {tls_dir}");
                }

                // Create information for the self-signed certificate about the current node.
                let subject_alt_names = vec!["localhost".to_string()];

                // Define file paths
                let tls_key_path = format!("{data_dir}/tls/key.pem");
                let tls_cert_path = format!("{data_dir}/tls/cert.pem");

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
            let tls_config = self.create_tls_config(data_dir)?;

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
                SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3333);

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

    /// Loads a config file from disk, returns default if it cannot load it
    fn get_config_file(path: &str) -> ConfigFile {
        let data = ConfigFile::from_file(path);

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

    fn load_chain_store(data_dir: String) -> Result<ChainStore, FlorestadError> {
        let config = FlatChainStoreConfig::new(data_dir + "/chaindata");
        ChainStore::new(config).map_err(FlorestadError::CouldNotCreateFlatChainStore)
    }

    fn load_chain_state(
        data_dir: String,
        network: Network,
        assume_valid: AssumeValidArg,
    ) -> Result<ChainState<ChainStore>, FlorestadError> {
        let db = Self::load_chain_store(data_dir.clone())?;

        ChainState::<ChainStore>::load_chain_state(db, network, assume_valid).or_else(|e| match e {
            BlockchainError::ChainNotInitialized => {
                let db = Self::load_chain_store(data_dir)?;
                Ok(ChainState::new(db, network, assume_valid))
            }
            anyerr => Err(FlorestadError::CouldNotLoadFlatChainStore(anyerr)),
        })
    }

    fn load_wallet(data_dir: &String) -> Result<AddressCache<KvDatabase>, FlorestadError> {
        let database =
            KvDatabase::new(data_dir.to_owned()).map_err(FlorestadError::CouldNotOpenKvDatabase)?;
        Ok(AddressCache::new(database))
    }

    fn setup_wallet(
        &self,
        data_dir: &str,
        wallet: &mut AddressCache<KvDatabase>,
    ) -> Result<(), FlorestadError> {
        // The config file inside our data directory or inside the specified directory
        let config_file = match self.config.config_file {
            Some(ref path) => Self::get_config_file(path),
            None => {
                let default_path = format!("{data_dir}/config.toml");
                Self::get_config_file(&default_path)
            }
        };
        let setup = self.prepare_wallet_setup(config_file)?;

        // Add the configured descriptors and addresses to the wallet
        for descriptor in setup.descriptors {
            let descriptor = descriptor.to_string();
            let is_cached = wallet.is_cached(&descriptor)?;

            if !is_cached {
                wallet.push_descriptor(&descriptor)?;
            }
        }
        for addresses in setup.addresses {
            wallet.cache_address(addresses.script_pubkey());
        }

        info!("Wallet setup completed!");
        Ok(())
    }

    /// Parses the configured list of xpubs, output descriptors and addresses to watch for, and
    /// returns the constructed `InitialWalletSetup`.
    fn prepare_wallet_setup(
        &self,
        config_file: ConfigFile,
    ) -> Result<InitialWalletSetup, FlorestadError> {
        let config = &self.config;

        let mut xpubs = Vec::new();
        xpubs.extend(config.wallet_xpub.clone().unwrap_or_default());
        xpubs.extend(config_file.wallet.xpubs.unwrap_or_default());
        xpubs.extend(Self::get_key_from_env());

        let mut descriptors = Vec::new();
        descriptors.extend(config.wallet_descriptor.clone().unwrap_or_default());
        descriptors.extend(config_file.wallet.descriptors.unwrap_or_default());

        let addresses = config_file.wallet.addresses.unwrap_or_default();

        InitialWalletSetup::build(&xpubs, &descriptors, &addresses, config.network, 100)
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
