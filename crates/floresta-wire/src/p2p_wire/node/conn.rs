use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bitcoin::p2p::address::AddrV2;
use bitcoin::p2p::ServiceFlags;
use bitcoin::Network;
use floresta_chain::ChainBackend;
use floresta_common::service_flags;
use floresta_common::service_flags::UTREEXO;
use floresta_common::Ema;
use floresta_mempool::Mempool;
use tokio::net::tcp::WriteHalf;
use tokio::spawn;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::debug;
use tracing::info;

use super::try_and_log;
use super::ConnectionKind;
use super::InflightRequests;
use super::LocalPeerView;
use super::NodeNotification;
use super::NodeRequest;
use super::PeerStatus;
use super::UtreexoNode;
use crate::address_man::AddressMan;
use crate::address_man::AddressState;
use crate::address_man::LocalAddress;
use crate::node_context::NodeContext;
use crate::p2p_wire::error::AddrParseError;
use crate::p2p_wire::error::WireError;
use crate::p2p_wire::peer::create_actors;
use crate::p2p_wire::peer::Peer;
use crate::p2p_wire::transport;
use crate::TransportProtocol;

/// How long before we consider using alternative ways to find addresses,
/// such as hard-coded peers
const HARDCODED_ADDRESSES_GRACE_PERIOD: Duration = Duration::from_secs(60);

/// The minimum amount of time between address fetching requests from DNS seeds (2 minutes).
const DNS_SEED_REQUEST_INTERVAL: Duration = Duration::from_secs(2 * 60);

impl<T, Chain> UtreexoNode<Chain, T>
where
    T: 'static + Default + NodeContext,
    Chain: ChainBackend + 'static,
    WireError: From<Chain::Error>,
{
    // === CONNECTION CREATION ===

    pub(crate) fn create_connection(&mut self, kind: ConnectionKind) -> Result<(), WireError> {
        let required_services = match kind {
            ConnectionKind::Regular(services) => services,
            _ => ServiceFlags::NONE,
        };

        let address = self
            .fixed_peer
            .as_ref()
            .map(|addr| (0, addr.clone()))
            .or_else(|| {
                self.address_man.get_address_to_connect(
                    required_services,
                    matches!(kind, ConnectionKind::Feeler),
                )
            });

        let Some((peer_id, address)) = address else {
            // No peers with the desired services are known, load hardcoded addresses
            let net = self.network;
            self.address_man.add_fixed_addresses(net);

            return Err(WireError::NoAddressesAvailable);
        };

        debug!("attempting connection with address={address:?} kind={kind:?}",);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Defaults to failed, if the connection is successful, we'll update the state
        self.address_man
            .update_set_state(peer_id, AddressState::Failed(now));

        // Don't connect to the same peer twice
        let is_connected = |(_, peer_addr): (_, &LocalPeerView)| {
            peer_addr.address == address.get_net_address() && peer_addr.port == address.get_port()
        };

        if self.common.peers.iter().any(is_connected) {
            return Err(WireError::PeerAlreadyExists(
                address.get_net_address(),
                address.get_port(),
            ));
        }

        // We allow V1 fallback only if the cli option was set, it's a --connect peer
        // or if we are connecting to a utreexo peer, since utreexod doesn't support V2 yet.
        let is_fixed = self.fixed_peer.is_some();
        let allow_v1 = self.config.allow_v1_fallback
            || kind == ConnectionKind::Regular(UTREEXO.into())
            || is_fixed;

        self.open_connection(kind, peer_id, address, allow_v1)?;

        Ok(())
    }

    pub(crate) fn open_feeler_connection(&mut self) -> Result<(), WireError> {
        // No feeler if `-connect` is set
        if self.fixed_peer.is_some() {
            return Ok(());
        }
        self.create_connection(ConnectionKind::Feeler)?;
        Ok(())
    }

    /// Creates a new outgoing connection with `address`.
    ///
    /// `kind` may or may not be a [`ConnectionKind::Feeler`], a special connection type
    /// that is used to learn about good peers, but are not kept after handshake
    /// (others are [`ConnectionKind::Regular`] and [`ConnectionKind::Extra`]).
    ///
    /// We will always try to open a V2 connection first. If the `allow_v1_fallback` is set,
    /// we may retry the connection with the old V1 protocol if the V2 connection fails.
    /// We don't open the connection here, we create a [`Peer`] actor that will try to open
    /// a connection with the given address and kind. If it succeeds, it will send a
    /// [`PeerMessages::Ready`](crate::p2p_wire::peer::PeerMessages) to the node after handshaking.
    pub(crate) fn open_connection(
        &mut self,
        kind: ConnectionKind,
        peer_id: usize,
        address: LocalAddress,
        allow_v1_fallback: bool,
    ) -> Result<(), WireError> {
        let (requests_tx, requests_rx) = unbounded_channel();
        if let Some(ref proxy) = self.socks5 {
            spawn(timeout(
                Duration::from_secs(10),
                Self::open_proxy_connection(
                    proxy.address,
                    kind,
                    self.mempool.clone(),
                    self.network,
                    self.node_tx.clone(),
                    peer_id,
                    address.clone(),
                    requests_rx,
                    self.peer_id_count,
                    self.config.user_agent.clone(),
                    allow_v1_fallback,
                ),
            ));
        } else {
            spawn(timeout(
                Duration::from_secs(10),
                Self::open_non_proxy_connection(
                    kind,
                    peer_id,
                    address.clone(),
                    requests_rx,
                    self.peer_id_count,
                    self.mempool.clone(),
                    self.network,
                    self.node_tx.clone(),
                    self.config.user_agent.clone(),
                    allow_v1_fallback,
                ),
            ));
        }

        let peer_count: u32 = self.peer_id_count;

        self.inflight.insert(
            InflightRequests::Connect(peer_count),
            (peer_count, Instant::now()),
        );

        self.peers.insert(
            peer_count,
            LocalPeerView {
                message_times: Ema::with_half_life_50(),
                address: address.get_net_address(),
                port: address.get_port(),
                user_agent: "".to_string(),
                state: PeerStatus::Awaiting,
                channel: requests_tx,
                services: ServiceFlags::NONE,
                _last_message: Instant::now(),
                kind,
                address_id: peer_id as u32,
                height: 0,
                banscore: 0,
                // Will be downgraded to V1 if the V2 handshake fails, and we allow fallback
                transport_protocol: TransportProtocol::V2,
            },
        );

        match kind {
            ConnectionKind::Feeler => self.last_feeler = Instant::now(),
            ConnectionKind::Regular(_) => self.last_connection = Instant::now(),
            _ => {}
        }

        // Increment peer_id count and the list of peer ids
        // so we can get information about connected or
        // added peers when requesting with getpeerinfo command
        self.peer_id_count += 1;
        Ok(())
    }

    /// Opens a new connection that doesn't require a proxy and includes the functionalities of create_outbound_connection.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn open_non_proxy_connection(
        kind: ConnectionKind,
        peer_id: usize,
        address: LocalAddress,
        requests_rx: UnboundedReceiver<NodeRequest>,
        peer_id_count: u32,
        mempool: Arc<Mutex<Mempool>>,
        network: Network,
        node_tx: UnboundedSender<NodeNotification>,
        user_agent: String,
        allow_v1_fallback: bool,
    ) -> Result<(), WireError> {
        let address = (address.get_net_address(), address.get_port());

        let (transport_reader, transport_writer, transport_protocol) =
            transport::connect(address, network, allow_v1_fallback).await?;

        let (cancellation_sender, cancellation_receiver) = oneshot::channel();
        let (actor_receiver, actor) = create_actors(transport_reader);
        tokio::spawn(async move {
            tokio::select! {
                _ = cancellation_receiver => {}
                _ = actor.run() => {}
            }
        });

        // Use create_peer function instead of manually creating the peer
        Peer::<WriteHalf>::create_peer(
            peer_id_count,
            mempool,
            node_tx.clone(),
            requests_rx,
            peer_id,
            kind,
            actor_receiver,
            transport_writer,
            user_agent,
            cancellation_sender,
            transport_protocol,
        );

        Ok(())
    }

    /// Opens a connection through a socks5 interface
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn open_proxy_connection(
        proxy: SocketAddr,
        kind: ConnectionKind,
        mempool: Arc<Mutex<Mempool>>,
        network: Network,
        node_tx: UnboundedSender<NodeNotification>,
        peer_id: usize,
        address: LocalAddress,
        requests_rx: UnboundedReceiver<NodeRequest>,
        peer_id_count: u32,
        user_agent: String,
        allow_v1_fallback: bool,
    ) -> Result<(), WireError> {
        let (transport_reader, transport_writer, transport_protocol) =
            transport::connect_proxy(proxy, address, network, allow_v1_fallback).await?;

        let (cancellation_sender, cancellation_receiver) = oneshot::channel();
        let (actor_receiver, actor) = create_actors(transport_reader);
        tokio::spawn(async move {
            tokio::select! {
                _ = cancellation_receiver => {}
                _ = actor.run() => {}
            }
        });

        Peer::<WriteHalf>::create_peer(
            peer_id_count,
            mempool,
            node_tx,
            requests_rx,
            peer_id,
            kind,
            actor_receiver,
            transport_writer,
            user_agent,
            cancellation_sender,
            transport_protocol,
        );
        Ok(())
    }

    // === BOOTSTRAPPING ===

    /// Resolves a string address into a LocalAddress
    ///
    /// This function should get an address in the format `<address>[<:port>]` and return a
    /// usable [`LocalAddress`]. It can be an ipv4, ipv6 or a hostname. In case of hostnames,
    /// we resolve them using the system's DNS resolver and return an ip address. Errors if
    /// the provided address is invalid, or we can't resolve it.
    ///
    /// TODO: Allow for non-clearnet addresses like onion services and i2p.
    pub(crate) fn resolve_connect_host(
        address: &str,
        default_port: u16,
    ) -> Result<LocalAddress, AddrParseError> {
        // ipv6
        if address.starts_with('[') {
            if !address.contains(']') {
                return Err(AddrParseError::InvalidIpv6);
            }

            let mut split = address.trim_end().split(']');
            let hostname = split.next().ok_or(AddrParseError::InvalidIpv6)?;
            let port = split
                .next()
                .filter(|x| !x.is_empty())
                .map(|port| {
                    port.trim_start_matches(':')
                        .parse()
                        .map_err(|_e| AddrParseError::InvalidPort)
                })
                .transpose()?
                .unwrap_or(default_port);

            let hostname = hostname.trim_start_matches('[');
            let ip = hostname.parse().map_err(|_e| AddrParseError::InvalidIpv6)?;
            return Ok(LocalAddress::new(
                AddrV2::Ipv6(ip),
                0,
                AddressState::NeverTried,
                ServiceFlags::NONE,
                port,
                rand::random(),
            ));
        }

        // ipv4 - it's hard to differentiate between ipv4 and hostname without an actual regex
        // simply try to parse it as an ip address and if it fails, assume it's a hostname

        // this breaks the necessity of feature gate on windows
        let mut address = address;
        if address.is_empty() {
            address = "127.0.0.1"
        }

        let mut split = address.split(':');
        let ip = split
            .next()
            .ok_or(AddrParseError::InvalidIpv4)?
            .parse()
            .map_err(|_e| AddrParseError::InvalidIpv4);

        match ip {
            Ok(ip) => {
                let port = split
                    .next()
                    .map(|port| port.parse().map_err(|_e| AddrParseError::InvalidPort))
                    .transpose()?
                    .unwrap_or(default_port);

                if split.next().is_some() {
                    return Err(AddrParseError::Inconclusive);
                }

                let id = rand::random();
                Ok(LocalAddress::new(
                    AddrV2::Ipv4(ip),
                    0,
                    AddressState::NeverTried,
                    ServiceFlags::NONE,
                    port,
                    id,
                ))
            }

            Err(_) => {
                let mut split = address.split(':');
                let hostname = split.next().ok_or(AddrParseError::InvalidHostname)?;
                let port = split
                    .next()
                    .map(|port| port.parse().map_err(|_e| AddrParseError::InvalidPort))
                    .transpose()?
                    .unwrap_or(default_port);

                if split.next().is_some() {
                    return Err(AddrParseError::Inconclusive);
                }

                let ip = dns_lookup::lookup_host(hostname)
                    .map_err(|_e| AddrParseError::InvalidHostname)?;
                let id = rand::random();
                let ip = match ip[0] {
                    std::net::IpAddr::V4(ip) => AddrV2::Ipv4(ip),
                    std::net::IpAddr::V6(ip) => AddrV2::Ipv6(ip),
                };

                Ok(LocalAddress::new(
                    ip,
                    0,
                    AddressState::NeverTried,
                    ServiceFlags::NONE,
                    port,
                    id,
                ))
            }
        }
    }

    // TODO(@luisschwab): get rid of this once
    // https://github.com/rust-bitcoin/rust-bitcoin/pull/4639 makes it into a release.
    pub(crate) fn get_port(network: Network) -> u16 {
        match network {
            Network::Bitcoin => 8333,
            Network::Signet => 38333,
            Network::Testnet => 18333,
            Network::Testnet4 => 48333,
            Network::Regtest => 18444,
        }
    }

    /// Fetch peers from DNS seeds, sending a `NodeNotification` with found ones. Returns
    /// immediately after spawning a background blocking task that performs the work.
    pub(crate) fn get_peers_from_dns(&self) -> Result<(), WireError> {
        let node_sender = self.node_tx.clone();
        let network = self.network;

        let proxy_addr = self.socks5.as_ref().map(|proxy| {
            let addr = proxy.address;
            info!("Asking for DNS peers via the SOCKS5 proxy: {addr}");
            addr
        });

        tokio::task::spawn_blocking(move || {
            let dns_seeds = floresta_chain::get_chain_dns_seeds(network);
            let mut addresses = Vec::new();

            let default_port = Self::get_port(network);
            for seed in dns_seeds {
                let _addresses = AddressMan::get_seeds_from_dns(&seed, default_port, proxy_addr);

                if let Ok(_addresses) = _addresses {
                    addresses.extend(_addresses);
                }
            }

            info!(
                "Fetched {} peer addresses from all DNS seeds",
                addresses.len()
            );
            node_sender
                .send(NodeNotification::DnsSeedAddresses(addresses))
                .unwrap();
        });

        Ok(())
    }

    /// Check whether it's necessary to request more addresses from DNS seeds.
    ///
    /// Perform another address request from DNS seeds if we still don't have enough addresses
    /// on the [`AddressMan`] and the last address request from DNS seeds was over 2 minutes ago.
    fn maybe_ask_dns_seed_for_addresses(&mut self) {
        let enough_addresses = self.address_man.enough_addresses();

        // Skip if address fetching from DNS seeds is disabled,
        // or if the [`AddressMan`] has enough addresses in its database.
        if self.config.disable_dns_seeds || enough_addresses {
            return;
        }

        // Don't ask for peers too often.
        let last_dns_request = self.last_dns_seed_call.elapsed();
        if last_dns_request < DNS_SEED_REQUEST_INTERVAL {
            return;
        }

        self.last_dns_seed_call = Instant::now();

        info!("Floresta has been running for a while without enough addresses, requesting more from DNS seeds");
        try_and_log!(self.get_peers_from_dns());
    }

    /// If we don't have any peers, we use the hardcoded addresses.
    ///
    ///
    /// This is only done if we don't have any peers for a long time, or we
    /// can't find a Utreexo peer in a context we need them. This function
    /// won't do anything if `--connect` was used
    fn maybe_use_hardcoded_addresses(&mut self, needs_utreexo: bool) {
        if self.fixed_peer.is_some() {
            return;
        }

        let has_peers = !self.peers.is_empty();
        // Return if we have peers and utreexo isn't needed OR we have utreexo peers
        if has_peers && (!needs_utreexo || self.has_utreexo_peers()) {
            return;
        }

        let mut wait = HARDCODED_ADDRESSES_GRACE_PERIOD;
        if needs_utreexo {
            // This gives some extra time for the node to try connections after chain selection
            wait += Duration::from_secs(60);
        }

        if self.startup_time.elapsed() < wait {
            return;
        }

        info!("No peers found, using hardcoded addresses");
        let net = self.network;
        self.address_man.add_fixed_addresses(net);
    }

    pub(crate) fn init_peers(&mut self) -> Result<(), WireError> {
        let anchors = self.common.address_man.start_addr_man(self.datadir.clone());
        let enough_addresses = self.common.address_man.enough_addresses();

        if !self.config.disable_dns_seeds && !enough_addresses {
            self.get_peers_from_dns()?;
            self.last_dns_seed_call = Instant::now();
        }

        for address in anchors {
            self.open_connection(
                ConnectionKind::Regular(UTREEXO.into()),
                address.id,
                address,
                // Using V1 transport fallback as utreexo nodes have limited support
                true,
            )?;
        }

        Ok(())
    }

    // === MAINTENANCE ===

    pub(crate) fn maybe_open_connection(
        &mut self,
        required_service: ServiceFlags,
    ) -> Result<(), WireError> {
        // try to connect with manually added peers
        self.maybe_open_connection_with_added_peers()?;

        // If the user passes in a `--connect` cli argument, we only connect with
        // that particular peer.
        if self.fixed_peer.is_some() && !self.peers.is_empty() {
            return Ok(());
        }

        // If we've tried getting some connections, but the addresses we have are not
        // working. Try getting some more addresses from DNS
        self.maybe_ask_dns_seed_for_addresses();
        let needs_utreexo = required_service.has(service_flags::UTREEXO.into());
        self.maybe_use_hardcoded_addresses(needs_utreexo);

        let connection_kind = ConnectionKind::Regular(required_service);
        if self.peers.len() < T::MAX_OUTGOING_PEERS {
            self.create_connection(connection_kind)?;
        }

        Ok(())
    }

    pub(crate) fn maybe_open_connection_with_added_peers(&mut self) -> Result<(), WireError> {
        if self.added_peers.is_empty() {
            return Ok(());
        }
        let peers_count = self.peer_id_count;
        for added_peer in self.added_peers.clone() {
            let matching_peer = self.peers.values().find(|peer| {
                self.to_addr_v2(peer.address) == added_peer.address && peer.port == added_peer.port
            });

            if matching_peer.is_none() {
                let address = LocalAddress::new(
                    added_peer.address.clone(),
                    0,
                    AddressState::Tried(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    ),
                    ServiceFlags::NONE,
                    added_peer.port,
                    peers_count as usize,
                );

                // Finally, open the connection with the node
                self.open_connection(
                    ConnectionKind::Regular(ServiceFlags::NONE),
                    peers_count as usize,
                    address,
                    added_peer.v1_fallback,
                )?
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use floresta_chain::pruned_utreexo::partial_chain::PartialChainState;

    use crate::node::running_ctx::RunningNode;
    use crate::node::UtreexoNode;

    fn check_address_resolving(address: &str, port: u16, should_succeed: bool, description: &str) {
        let result =
            UtreexoNode::<PartialChainState, RunningNode>::resolve_connect_host(address, port);
        if should_succeed {
            assert!(result.is_ok(), "Failed: {description}");
        } else {
            assert!(result.is_err(), "Unexpected success: {description}");
        }
    }

    #[test]
    fn test_parse_address() {
        // IPv6 Tests
        check_address_resolving("[::1]", 8333, true, "Valid IPv6 without port");
        check_address_resolving("[::1", 8333, false, "Invalid IPv6 format");
        check_address_resolving("[::1]:8333", 8333, true, "Valid IPv6 with port");
        check_address_resolving(
            "[::1]:8333:8333",
            8333,
            false,
            "Invalid IPv6 with multiple ports",
        );

        // IPv4 Tests
        check_address_resolving("127.0.0.1", 8333, true, "Valid IPv4 without port");
        check_address_resolving("321.321.321.321", 8333, false, "Invalid IPv4 format");
        check_address_resolving("127.0.0.1:8333", 8333, true, "Valid IPv4 with port");
        check_address_resolving(
            "127.0.0.1:8333:8333",
            8333,
            false,
            "Invalid IPv4 with multiple ports",
        );

        // Hostname Tests
        check_address_resolving("example.com", 8333, true, "Valid hostname without port");
        check_address_resolving("example", 8333, false, "Invalid hostname");
        check_address_resolving("example.com:8333", 8333, true, "Valid hostname with port");
        check_address_resolving(
            "example.com:8333:8333",
            8333,
            false,
            "Invalid hostname with multiple ports",
        );

        // Edge Cases
        // This could fail on windows but doesnt since inside `resolve_connect_host` we specificate empty addresses as localhost for all OS`s.
        check_address_resolving("", 8333, true, "Empty string address");
        check_address_resolving(
            " 127.0.0.1:8333 ",
            8333,
            false,
            "Address with leading/trailing spaces",
        );
        check_address_resolving("127.0.0.1:0", 0, true, "Valid address with port 0");
        check_address_resolving(
            "127.0.0.1:65535",
            65535,
            true,
            "Valid address with maximum port",
        );
        check_address_resolving(
            "127.0.0.1:65536",
            65535,
            false,
            "Valid address with out-of-range port",
        )
    }
}
