//! Address manager is a module that keeps track of known peer addresses and associated
//! metadata. This module is very important in keeping our node protected against targeted
//! attacks, like eclipse attacks.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::read_to_string;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bitcoin::p2p::address::AddrV2;
use bitcoin::p2p::address::AddrV2Message;
use bitcoin::p2p::ServiceFlags;
use bitcoin::Network;
use floresta_chain::DnsSeed;
use floresta_common::service_flags;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

/// How long we'll wait before trying to connect to a peer that failed
const RETRY_TIME: u64 = 10 * 60; // 10 minutes

/// The minimum amount of addresses we need to have on the [`AddressMan`].
const MIN_ADDRESSES: usize = 15;

/// The minimum amount of CBF-capable addresses we need to have on the [`AddressMan`].
const MIN_ADDRESSES_CBF: usize = 5;

/// The minimum amount of Utreexo-capable addresses we need to have on the [`AddressMan`].
const MIN_ADDRESSES_UTREEXO: usize = 2;

/// If we haven't heard from a peer in this amount of time, we consider its info stale
/// and add it to the NeverTried bucket
const ASSUME_STALE: u64 = 24 * 60 * 60; // 24 hours

/// How many addresses we keep in our address manager
const MAX_ADDRESSES: usize = 50_000;

/// A type alias for a list of addresses to send to our peers
type AddressToSend = Vec<(AddrV2, u64, ServiceFlags, u16)>;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
/// A local state for how we see this peer. It helps us during peer selection,
/// by keeping track of our past encounters with this node (if any),
/// helping us to find live peers more easily, and avoid troublesome peers.
pub enum AddressState {
    /// We never tried this peer before, so we don't know what to expect. This variant
    /// also applies to peers that we tried to connect, but failed or we didn't connect
    /// to for a long time.
    NeverTried,

    /// We tried this peer before, and had success at least once, so we know what to expect
    Tried(u64),

    /// This peer misbehaved and we banned them
    Banned(u64),

    /// We are connected to this peer right now
    Connected,

    /// We tried connecting, but failed
    Failed(u64),
}

/// All the networks we might receive addresses for
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReachableNetworks {
    IPv4,
    IPv6,
    TorV3,
    I2P,
    CJDNS,
}

#[derive(Debug, Clone, PartialEq)]
/// How do we store peers locally
pub struct LocalAddress {
    /// An actual address
    address: AddrV2,
    /// Last time we successfully connected to this peer, only relevant is state == State::Tried
    last_connected: u64,
    /// Our local state for this peer, as defined in AddressState
    state: AddressState,
    /// Network services announced by this peer
    services: ServiceFlags,
    /// Network port this peers listens to
    port: u16,
    /// Random id for this peer
    pub id: usize,
}

impl From<AddrV2> for LocalAddress {
    fn from(value: AddrV2) -> Self {
        LocalAddress {
            address: value,
            last_connected: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            state: AddressState::NeverTried,
            services: ServiceFlags::NONE,
            port: 8333,
            id: rand::random::<usize>(),
        }
    }
}

impl From<AddrV2Message> for LocalAddress {
    fn from(value: AddrV2Message) -> Self {
        LocalAddress {
            address: value.addr,
            last_connected: value.time.into(),
            state: AddressState::NeverTried,
            services: value.services,
            port: value.port,
            id: rand::random::<usize>(),
        }
    }
}

impl FromStr for LocalAddress {
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LocalAddress::try_from(s)
    }
    type Err = std::net::AddrParseError;
}

// Note that, since we can't know the network we are operating in, this code
// can't know what's the default port. Therefore, it will only work if you give
// a SocketAddr, i.e. <IP:PORT>
impl TryFrom<&str> for LocalAddress {
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let address = value.parse::<SocketAddr>()?;
        let ip = match address {
            SocketAddr::V4(ipv4) => AddrV2::Ipv4(*ipv4.ip()),
            SocketAddr::V6(ipv6) => AddrV2::Ipv6(*ipv6.ip()),
        };

        Ok(LocalAddress::new(
            ip,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            super::address_man::AddressState::NeverTried,
            ServiceFlags::NONE,
            address.port(),
            rand::random::<usize>(),
        ))
    }

    type Error = std::net::AddrParseError;
}

impl LocalAddress {
    pub fn new(
        address: AddrV2,
        last_connected: u64,
        state: AddressState,
        services: ServiceFlags,
        port: u16,
        id: usize,
    ) -> LocalAddress {
        LocalAddress {
            address,
            last_connected,
            state,
            services,
            port,
            id,
        }
    }

    /// Sets the port used by this node
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    /// Sets the services advertised by this node
    pub fn set_services(&mut self, services: ServiceFlags) {
        self.services = services;
    }

    /// Returns this address's port
    pub fn get_port(&self) -> u16 {
        self.port
    }

    /// Return an IP address associated with this peer address
    pub fn get_net_address(&self) -> IpAddr {
        match self.address {
            // IPV4
            AddrV2::Ipv4(ipv4) => IpAddr::V4(ipv4),
            // IPV6
            AddrV2::Ipv6(ipv6) => IpAddr::V6(ipv6),
            _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
        }
    }

    /// Returns the actual address, as defined in AddrV2. This is useful
    /// if we are trying a peer that needs a proxy like Tor.
    pub fn get_address(&self) -> AddrV2 {
        self.address.clone()
    }
}

#[derive(Clone)]
/// A module that keeps track of known addresses and chooses addresses that our node can connect
pub struct AddressMan {
    /// A map of all peers we know, mapping the address id to the actual address.
    addresses: HashMap<usize, LocalAddress>,

    /// All indexes of "good" addresses
    ///
    /// Good peers are those which we think are live, and haven't banned yet.
    /// If we try to connect with one peer, and the connection doesn't succeed,
    /// this peer is assumed to be down and removed from good addresses for some time.
    good_addresses: Vec<usize>,

    /// A map of a set of good peers indexes by their [`ServiceFlags`]
    ///
    /// We use this to make peer selection, if we are looking for a specific kind of peer (like utreexo or CBF peers)
    good_peers_by_service: HashMap<ServiceFlags, Vec<usize>>,

    /// A map of a set of peers indexes by their [`ServiceFlags`]
    ///
    /// This works similarly to `good_peers_by_service`. However, we keep all peers here, not only good peers
    peers_by_service: HashMap<ServiceFlags, Vec<usize>>,

    /// The maximum number of entries this address manager can hold
    max_size: usize,

    /// The networks we can reach
    reachable_networks: HashSet<ReachableNetworks>,
}

impl AddressMan {
    /// Creates a new address manager
    ///
    /// `max_size` is the maximum number of addresses to keep in memory. If None is provided,
    /// a default of 50,000 addresses is used.
    pub fn new(max_size: Option<usize>, reachable_networks: &[ReachableNetworks]) -> Self {
        let reachable_networks: HashSet<ReachableNetworks> =
            reachable_networks.iter().cloned().collect();

        AddressMan {
            addresses: HashMap::new(),
            good_addresses: Vec::new(),
            good_peers_by_service: HashMap::new(),
            peers_by_service: HashMap::new(),
            max_size: max_size.unwrap_or(MAX_ADDRESSES),
            reachable_networks,
        }
    }

    /// Add a new address to our list of known address
    pub fn push_addresses(&mut self, addresses: &[LocalAddress]) {
        for address in addresses {
            let id = address.id;
            // don't add addresses that don't have the minimum required services
            if !address.services.has(ServiceFlags::WITNESS)
                | !address.services.has(ServiceFlags::NETWORK_LIMITED)
            {
                continue;
            }

            // don't unreachable addresses
            match address.address {
                AddrV2::Ipv4(ipv4) => {
                    if !Self::is_routable_ipv4(&ipv4) {
                        continue;
                    }
                }

                AddrV2::Ipv6(ipv6) => {
                    if !Self::is_routable_ipv6(&ipv6) {
                        continue;
                    }
                }

                _ => {}
            }

            // don't add addresses from networks we can't reach
            if !self.is_net_reachable(address) {
                continue;
            }

            if !address.is_routable() {
                continue;
            }

            // don't add duplicate addresses
            if self
                .addresses
                .values()
                .any(|x| x.address == address.address)
            {
                continue;
            }

            if let std::collections::hash_map::Entry::Vacant(e) = self.addresses.entry(id) {
                e.insert(address.clone());
                if Self::is_good_peer(address) {
                    self.good_addresses.push(id);
                }

                self.push_if_has_service(address, service_flags::UTREEXO.into());
                self.push_if_has_service(address, ServiceFlags::NONE); // this means any peer
                self.push_if_has_service(address, ServiceFlags::COMPACT_FILTERS);
            }
        }

        // Open up space by pruning old addresses
        self.prune_addresses();
    }

    /// Check if we can reach this address based on our reachable networks
    fn is_net_reachable(&self, address: &LocalAddress) -> bool {
        match address.address {
            AddrV2::Ipv4(_) => self.reachable_networks.contains(&ReachableNetworks::IPv4),
            AddrV2::Ipv6(_) => self.reachable_networks.contains(&ReachableNetworks::IPv6),
            AddrV2::TorV3(_) => self.reachable_networks.contains(&ReachableNetworks::TorV3),
            AddrV2::I2p(_) => self.reachable_networks.contains(&ReachableNetworks::I2P),
            _ => false,
        }
    }

    /// Remove addresses that we last heard of, until we are under the limit
    /// of addresses to keep.
    fn prune_addresses(&mut self) {
        let excess = self.addresses.len().saturating_sub(self.max_size);
        if excess == 0 {
            return;
        }

        let mut oldest_ids: Vec<_> = self
            .addresses
            .iter()
            .map(|(&id, addr)| (id, addr.last_connected))
            .collect();

        oldest_ids.sort_by_key(|&(_, last_connected)| last_connected);

        for (oldest_id, _) in oldest_ids.into_iter().take(excess) {
            self.addresses.remove(&oldest_id);
            self.good_addresses.retain(|&x| x != oldest_id);
            for peers in self.good_peers_by_service.values_mut() {
                peers.retain(|&x| x != oldest_id);
            }
            for peers in self.peers_by_service.values_mut() {
                peers.retain(|&x| x != oldest_id);
            }
        }
    }

    /// Return addresses from the [`AddressMan`] filtered by their [`ServiceFlags`].
    fn get_addresses_by_service(&self, service: ServiceFlags) -> Vec<LocalAddress> {
        self.good_peers_by_service
            .get(&service)
            .map(|peer_ids| {
                peer_ids
                    .iter()
                    .filter_map(|id| self.addresses.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if we have enough addresses on the address manager.
    #[rustfmt::skip]
    pub fn enough_addresses(&self) -> bool {
        if self.good_addresses.len() < MIN_ADDRESSES{
            return false;
        }

        if self.get_addresses_by_service(ServiceFlags::COMPACT_FILTERS).len() < MIN_ADDRESSES_CBF {
            return false;
        }

        if self.get_addresses_by_service(service_flags::UTREEXO.into()).len() < MIN_ADDRESSES_UTREEXO {
            return false;
        }

        true
    }

    fn is_good_peer(address: &LocalAddress) -> bool {
        if !Self::is_routable(address) {
            return false;
        }

        matches!(address.state, AddressState::Connected)
            || matches!(address.state, AddressState::Tried(_))
    }

    const fn is_routable(address: &LocalAddress) -> bool {
        match address.address {
            AddrV2::Ipv4(ipv4) => Self::is_routable_ipv4(&ipv4),
            AddrV2::Ipv6(ipv6) => Self::is_routable_ipv6(&ipv6),
            AddrV2::Cjdns(address) => {
                let octets = address.octets();
                // CJDNS addresses use a special range for local addresses (FC00::/8)
                // See: https://github.com/cjdelisle/cjdns/tree/master/doc#what-is-notable-about-cjdns-why-should-i-use-it
                if octets[0] == 0xFC {
                    return true;
                }

                false
            }
            _ => true,
        }
    }

    const fn is_routable_ipv4(ip: &Ipv4Addr) -> bool {
        // Code taken from bitcoinfuzz commit: 7619d400bbd8078b8dc51d077c900f0b54f9cfcf/
        let octets = ip.octets();

        // 0.0.0.0/8 - "This" network
        if octets[0] == 0 {
            return false;
        }

        // Loopback, broadcast, private (RFC 1918)
        if ip.is_loopback() || ip.is_broadcast() || ip.is_private() {
            return false;
        }

        // RFC 2544 - Benchmarking - 198.18.0.0/15
        if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
            return false;
        }

        // RFC 3927 - Link-Local - 169.254.0.0/16
        if ip.is_link_local() {
            return false;
        }

        // RFC 6598 - Shared Address Space (CGNAT) - 100.64.0.0/10
        if octets[0] == 100 && (octets[1] >= 64 && octets[1] <= 127) {
            return false;
        }

        // RFC 5737 - Documentation (TEST-NET-1, TEST-NET-2, TEST-NET-3)
        if ip.is_documentation() {
            return false;
        }

        true
    }

    const fn is_routable_ipv6(ip: &Ipv6Addr) -> bool {
        let octets = ip.octets();

        // Unspecified, loopback, unique local (RFC 4193 - fc00::/7)
        if ip.is_unspecified() || ip.is_loopback() || ip.is_unique_local() {
            return false;
        }

        // RFC 4843 - ORCHID - 2001:10::/28
        if octets[0] == 0x20 && octets[1] == 0x01 && octets[2] == 0x00 && (octets[3] & 0xF0) == 0x10
        {
            return false;
        }

        // RFC 4862 - Link-local - fe80::/64
        if octets[0] == 0xFE && (octets[1] & 0xC0) == 0x80 {
            return false;
        }

        // RFC 7343 - ORCHIDv2 - 2001:20::/28
        if octets[0] == 0x20 && octets[1] == 0x01 && octets[2] == 0x00 && (octets[3] & 0xf0) == 0x20
        {
            return false;
        }

        true
    }

    fn push_if_has_service(&mut self, address: &LocalAddress, service: ServiceFlags) {
        if address.services.has(service) {
            if Self::is_good_peer(address) {
                self.good_peers_by_service
                    .entry(service)
                    .or_default()
                    .push(address.id);
            }

            self.peers_by_service
                .entry(service)
                .or_default()
                .push(address.id);
        }
    }

    pub fn get_addresses_to_send(&self) -> AddressToSend {
        let addresses = self
            .good_addresses
            .iter()
            .filter_map(|id| {
                let address = self.addresses.get(id)?;
                Some((
                    address.address.clone(),
                    address.last_connected,
                    address.services,
                    address.port,
                ))
            })
            .collect();

        addresses
    }

    fn do_lookup(host: &str, default_port: u16, socks5: Option<SocketAddr>) -> Vec<LocalAddress> {
        let ips = match socks5 {
            Some(proxy) => {
                debug!("Performing DNS lookup for host: {host}, using SOCKS5 proxy: {proxy}");
                // SOCKS5 proxy lookup (proxied DNS-over-HTTPS).
                dns_proxy::lookup_host_via_proxy(host, proxy).unwrap_or_else(|e| {
                    error!("DNS lookup via SOCKS5 proxy failed: {e}");
                    Vec::new()
                })
            }
            None => {
                debug!("Performing DNS lookup for host: {host}, using the system resolver");
                // System lookup (usually unencrypted, resolver sees both query and our IP).
                dns_lookup::lookup_host(host).unwrap_or_else(|e| {
                    error!("DNS lookup failed: {e}");
                    Vec::new()
                })
            }
        };

        if ips.is_empty() {
            warn!("No peer addresses read from DNS host: {host}");
        } else {
            info!("Fetched {} peer addresses from DNS host: {host}", ips.len());
        }

        let mut addresses = Vec::new();
        for ip in ips {
            if let Ok(ip) = LocalAddress::try_from(format!("{ip}:{default_port}").as_str()) {
                addresses.push(ip);
            }
        }

        addresses
    }

    pub fn get_seeds_from_dns(
        seed: &DnsSeed,
        default_port: u16,
        socks5: Option<SocketAddr>,
    ) -> Result<Vec<LocalAddress>, std::io::Error> {
        let mut addresses = Vec::new();

        // ask for utreexo peers (if filtering is available)
        if seed.filters.has(service_flags::UTREEXO.into()) {
            let host = format!("x1000000.{}", seed.seed);
            let _addresses = Self::do_lookup(&host, default_port, socks5);
            let _addresses = _addresses.into_iter().map(|mut x| {
                x.services =
                    ServiceFlags::NETWORK | service_flags::UTREEXO.into() | ServiceFlags::WITNESS;
                x
            });

            addresses.extend(_addresses);
        }

        // ask for compact filter peers (if filtering is available)
        if seed.filters.has(ServiceFlags::COMPACT_FILTERS) {
            let host = format!("x49.{}", seed.seed);
            let _addresses = Self::do_lookup(&host, default_port, socks5);
            let _addresses = _addresses.into_iter().map(|mut x| {
                x.services =
                    ServiceFlags::COMPACT_FILTERS | ServiceFlags::NETWORK | ServiceFlags::WITNESS;
                x
            });

            addresses.extend(_addresses);
        }

        // ask for any peer (if filtering is available)
        if seed.filters.has(ServiceFlags::WITNESS) {
            let host = format!("x9.{}", seed.seed);
            let _addresses = Self::do_lookup(&host, default_port, socks5);
            let _addresses = _addresses.into_iter().map(|mut x| {
                x.services = ServiceFlags::NETWORK | ServiceFlags::WITNESS;
                x
            });

            addresses.extend(_addresses);
        }

        // ask for any peer (if filtering isn't available)
        if seed.filters == ServiceFlags::NONE {
            let _addresses = Self::do_lookup(seed.seed, default_port, socks5);
            let _addresses = _addresses.into_iter().map(|mut x| {
                x.services = ServiceFlags::NETWORK | ServiceFlags::WITNESS;
                x
            });

            addresses.extend(_addresses);
        }

        Ok(addresses)
    }

    /// Returns a new random address to open a new connection, we try to get addresses with
    /// a set of features supported for our peers
    ///
    /// If no peers are known with the required service bit, we may return a random peer.
    /// Service bits are learned from DNS seeds or peer gossip and may be outdated or
    /// inaccurate, so we sometimes try random peers expecting they might implement the service.
    pub fn get_address_to_connect(
        &mut self,
        required_service: ServiceFlags,
        feeler: bool,
    ) -> Option<(usize, LocalAddress)> {
        if self.addresses.is_empty() {
            return None;
        }

        // Feeler connection are used to test if a peer is still alive, we don't care about
        // the features it supports or even if it's a valid peer. The only thing we care about
        // is that we haven't banned it.
        if feeler {
            let idx = rand::random::<usize>() % self.addresses.len();
            let peer = self.addresses.keys().nth(idx)?;
            let address = self.addresses.get(peer)?.to_owned();

            // don't try to connect to a peer that is banned or already connected
            if matches!(address.state, AddressState::Banned(_))
                | matches!(address.state, AddressState::Connected)
            {
                return None;
            }

            return Some((*peer, address));
        };

        for _ in 0..10 {
            let (id, peer) = self
                .get_address_by_service(required_service)
                .or_else(|| self.get_random_address(required_service))?;

            match peer.state {
                AddressState::NeverTried | AddressState::Tried(_) => {
                    return Some((id, peer));
                }

                AddressState::Connected => {
                    // if we are connected to this peer, don't try to connect again
                    continue;
                }

                AddressState::Banned(when) | AddressState::Failed(when) => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    if when + RETRY_TIME < now {
                        return Some((id, peer));
                    }

                    if let Some(peers) = self.good_peers_by_service.get_mut(&required_service) {
                        peers.retain(|&x| x != id)
                    }

                    self.good_addresses.retain(|&x| x != id);
                }
            }
        }

        None
    }

    pub fn dump_peers(&self, datadir: &str) -> std::io::Result<()> {
        let peers: Vec<_> = self
            .addresses
            .values()
            .cloned()
            .map(Into::<DiskLocalAddress>::into)
            .collect::<Vec<_>>();
        let peers = serde_json::to_string(&peers);
        if let Ok(peers) = peers {
            std::fs::write(datadir.to_owned() + "/peers.json", peers)?;
        }
        Ok(())
    }

    /// Dumps the connected utreexo peers to a file on dir `datadir/anchors.json` in json format `
    /// inputs are the directory to save the file and the list of ids of the connected utreexo peers
    pub fn dump_utreexo_peers(&self, datadir: &str, peers_id: &[usize]) -> std::io::Result<()> {
        let addresses: Vec<DiskLocalAddress> = peers_id
            .iter()
            .filter_map(|id| Some(self.addresses.get(id)?.to_owned().into()))
            .collect();
        let addresses: Result<String, serde_json::Error> = serde_json::to_string(&addresses);
        if let Ok(addresses) = addresses {
            std::fs::write(datadir.to_owned() + "/anchors.json", addresses)?;
        }
        Ok(())
    }

    fn get_address_by_service(&self, service: ServiceFlags) -> Option<(usize, LocalAddress)> {
        let peers = self.good_peers_by_service.get(&service)?;
        if peers.is_empty() {
            return None;
        }

        let idx = rand::random::<usize>() % peers.len();
        let utreexo_peer = peers.get(idx)?;

        Some((*utreexo_peer, self.addresses.get(utreexo_peer)?.to_owned()))
    }

    pub fn start_addr_man(&mut self, datadir: String) -> Vec<LocalAddress> {
        let persisted_peers = read_to_string(format!("{datadir}/peers.json"))
            .map(|seeds| serde_json::from_str::<Vec<DiskLocalAddress>>(&seeds));

        if let Ok(Ok(peers)) = persisted_peers {
            let peers = peers
                .into_iter()
                .map(Into::<LocalAddress>::into)
                .collect::<Vec<_>>();

            self.push_addresses(&peers);
        }

        let anchors = read_to_string(format!("{datadir}/anchors.json")).and_then(|anchors| {
            let anchors = serde_json::from_str::<Vec<DiskLocalAddress>>(&anchors)?;
            Ok(anchors
                .into_iter()
                .map(Into::<LocalAddress>::into)
                .collect::<Vec<_>>())
        });

        if anchors.is_err() {
            warn!("Failed to init Utreexo peers: anchors.json does not exist yet, or is invalid");
        }

        anchors.unwrap_or_default()
    }

    /// This function moves addresses between buckets, like if the ban time of a peer expired,
    /// or if we tried to connect to a peer and it failed in the past, but now it might be online
    /// again.
    pub fn rearrange_buckets(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for (_, address) in self.addresses.iter_mut() {
            match address.state {
                AddressState::Banned(ban_time) => {
                    if ban_time < now {
                        address.state = AddressState::NeverTried;
                    }
                }
                AddressState::Tried(tried_time) => {
                    if tried_time + ASSUME_STALE < now {
                        address.state = AddressState::NeverTried;
                    }
                }
                AddressState::Failed(failed_time) => {
                    if failed_time + RETRY_TIME < now {
                        address.state = AddressState::NeverTried;
                    }
                }
                AddressState::Connected | AddressState::NeverTried => {}
            }
        }
    }

    /// Attempt to find one random peer that advertises the required service
    ///
    /// If we cannot find a peer that advertises the required service, we return any peer
    /// that we have in our list of known peers. Luckily, either we'll connect to a peer that has
    /// this but we didn't know, or one of those peers will give us useful addresses.
    fn try_with_service(&self, service: ServiceFlags) -> Option<(usize, LocalAddress)> {
        if let Some(peers) = self.peers_by_service.get(&service) {
            let peers = peers
                .iter()
                .filter(|&x| {
                    if let Some(address) = self.addresses.get(x) {
                        if let AddressState::Failed(when) = address.state {
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();

                            if (when + RETRY_TIME) < now {
                                return true;
                            }
                        }

                        return matches!(address.state, AddressState::Tried(_))
                            || matches!(address.state, AddressState::NeverTried);
                    }

                    false
                })
                .collect::<Vec<_>>();

            if peers.is_empty() {
                return None;
            }

            let idx = rand::random::<usize>() % peers.len();
            let utreexo_peer = peers.get(idx)?;
            return Some((**utreexo_peer, self.addresses.get(utreexo_peer)?.to_owned()));
        }

        None
    }

    fn get_random_address(&self, service: ServiceFlags) -> Option<(usize, LocalAddress)> {
        if self.addresses.is_empty() {
            return None;
        }

        if let Some(address) = self.try_with_service(service) {
            return Some(address);
        }

        // if we can't find a peer that advertises the required service, get any peer
        let idx = rand::random::<usize>() % self.addresses.len();
        let peer = self.addresses.keys().nth(idx)?;

        Some((*peer, self.addresses.get(peer)?.to_owned()))
    }

    /// Updates the state of an address
    pub fn update_set_state(&mut self, idx: usize, state: AddressState) -> &mut Self {
        match state {
            AddressState::Banned(_) => {
                self.good_addresses.retain(|&x| x != idx);
            }
            AddressState::Tried(_) => {
                if !self.good_addresses.contains(&idx) {
                    self.good_addresses.push(idx);
                }

                if let Some(address) = self.addresses.get(&idx).cloned() {
                    self.push_if_has_service(&address, service_flags::UTREEXO.into());
                    self.push_if_has_service(&address, ServiceFlags::from(1 << 25)); // UTREEXO_FILTER
                    self.push_if_has_service(&address, ServiceFlags::NONE); // this means any peer
                    self.push_if_has_service(&address, ServiceFlags::COMPACT_FILTERS);
                }
            }
            AddressState::NeverTried => {
                self.good_addresses.retain(|&x| x != idx);
            }
            AddressState::Connected => {
                self.addresses.entry(idx).and_modify(|addr| {
                    addr.last_connected = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                });

                if !self.good_addresses.contains(&idx) {
                    self.good_addresses.push(idx);
                }
            }
            AddressState::Failed(_) => {
                self.good_addresses.retain(|&x| x != idx);
                for peers in self.good_peers_by_service.values_mut() {
                    peers.retain(|&x| x != idx);
                }
            }
        }

        if let Some(address) = self.addresses.get_mut(&idx) {
            address.state = state;
        };

        self
    }

    /// Adds a peer to the list of peers known to have some service
    fn add_peer_to_service(&mut self, idx: usize, service: ServiceFlags) {
        if let Some(peers) = self.peers_by_service.get_mut(&service) {
            if peers.contains(&idx) {
                return;
            }

            peers.push(idx);
        } else {
            self.peers_by_service.insert(service, vec![idx]);
        }
    }

    /// Removes a peer from the list of peers known to have some service
    fn remove_peer_from_service(&mut self, idx: usize, service: ServiceFlags) {
        if let Some(peers) = self.peers_by_service.get_mut(&service) {
            peers.retain(|&x| x != idx);
        }
    }

    /// Updates the list of peers that have a service
    ///
    /// If a peer used to advertise a service, but now it doesn't, we remove it from the list
    /// of peers that have that service. If a peer didn't advertise a service, but now it does,
    /// we add it to the list of peers that have that service.
    fn update_peer_for_service(&mut self, id: usize, service: ServiceFlags) {
        let Some(peer) = self.addresses.get(&id) else {
            return;
        };

        match peer.services.has(service) {
            true => self.add_peer_to_service(id, service),
            false => self.remove_peer_from_service(id, service),
        }
    }

    /// Updates `peers_by_service` buckets with the latest service flags info about a peer
    ///
    /// This function is called when we receive a version message from a peer, telling which
    /// services it advertises.
    ///
    /// We only index for Compact Filters and Utreexo. For NODE_NETWORK and NODE_WITNESS we already
    /// filter them out when we add them to the address manager, therefore, all peers in this list
    /// is already known for having those. And we don't care about the rest of the services,
    /// like NODE_BLOOM.
    fn update_peer_services_buckets(&mut self, idx: usize) {
        self.update_peer_for_service(idx, service_flags::UTREEXO.into());
        self.update_peer_for_service(idx, ServiceFlags::COMPACT_FILTERS);
    }

    /// Updates the service flags after we receive a version message
    pub fn update_set_service_flag(&mut self, idx: usize, flags: ServiceFlags) -> &mut Self {
        // if this peer turns out to not have the minimum required services, we remove it
        if !flags.has(ServiceFlags::NETWORK) || !flags.has(ServiceFlags::WITNESS) {
            self.addresses.remove(&idx);
            for peers in self.peers_by_service.values_mut() {
                peers.retain(|&x| x != idx);
            }

            self.good_addresses.retain(|&x| x != idx);
            self.good_peers_by_service
                .values_mut()
                .for_each(|peers| peers.retain(|&x| x != idx));

            return self;
        }

        if let Some(address) = self.addresses.get_mut(&idx) {
            address.services = flags;
        }

        self.update_peer_services_buckets(idx);
        self
    }

    /// Returns the file path to the seeds file for the given network
    fn get_net_seeds(network: Network) -> &'static str {
        match network {
            Network::Bitcoin => include_str!("seeds/mainnet_seeds.json"),
            Network::Signet => include_str!("seeds/signet_seeds.json"),
            Network::Testnet => include_str!("seeds/testnet_seeds.json"),
            Network::Testnet4 => include_str!("seeds/testnet4_seeds.json"),
            Network::Regtest => include_str!("seeds/regtest_seeds.json"),
        }
    }

    /// Reads the hard-coded addresses from the seeds file and adds them to the address manager
    ///
    /// This is a last-resort method to try to connect to a peer, if we don't have any other
    /// addresses to connect to.
    pub(crate) fn add_fixed_addresses(&mut self, network: Network) {
        let addresses = Self::get_net_seeds(network);
        let peers: Vec<DiskLocalAddress> =
            serde_json::from_str(addresses).expect("BUG: fixed peers are invalid");

        let peers = peers
            .into_iter()
            .map(Into::<LocalAddress>::into)
            .collect::<Vec<_>>();

        self.push_addresses(&peers);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskLocalAddress {
    /// An actual address
    address: Address,
    /// Last time we successfully connected to this peer, only relevant is state == State::Tried
    last_connected: u64,
    /// Our local state for this peer, as defined in AddressState
    state: AddressState,
    /// Network services announced by this peer
    services: u64,
    /// Network port this peers listens to
    port: u16,
    /// An id to identify this address
    id: Option<usize>,
}

impl From<LocalAddress> for DiskLocalAddress {
    fn from(value: LocalAddress) -> Self {
        let address = match value.address {
            AddrV2::Ipv4(ip) => Address::V4(ip),
            AddrV2::Ipv6(ip) => Address::V6(ip),
            AddrV2::Cjdns(ip) => Address::Cjdns(ip),
            AddrV2::I2p(ip) => Address::I2p(ip),
            AddrV2::TorV2(ip) => Address::OnionV2(ip),
            AddrV2::TorV3(ip) => Address::OnionV3(ip),
            AddrV2::Unknown(_, _) => Address::V4(Ipv4Addr::LOCALHOST),
        };

        DiskLocalAddress {
            address,
            last_connected: value.last_connected,
            state: if value.state == AddressState::Connected {
                AddressState::Tried(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                )
            } else {
                value.state
            },
            services: value.services.to_u64(),
            port: value.port,
            id: Some(value.id),
        }
    }
}
impl From<DiskLocalAddress> for LocalAddress {
    fn from(value: DiskLocalAddress) -> Self {
        let address = match value.address {
            Address::V4(ip) => AddrV2::Ipv4(ip),
            Address::V6(ip) => AddrV2::Ipv6(ip),
            Address::Cjdns(ip) => AddrV2::Cjdns(ip),
            Address::I2p(ip) => AddrV2::I2p(ip),
            Address::OnionV2(ip) => AddrV2::TorV2(ip),
            Address::OnionV3(ip) => AddrV2::TorV3(ip),
        };
        let services = ServiceFlags::from(value.services);
        LocalAddress {
            address,
            last_connected: value.last_connected,
            state: value.state,
            services,
            port: value.port,
            id: value.id.unwrap_or_else(rand::random::<usize>),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Address {
    /// Regular ipv4 address
    V4(Ipv4Addr),
    /// Regular ipv6 address
    V6(Ipv6Addr),
    /// Tor v2 address, this may never be used, as OnionV2 is deprecated
    /// but we'll keep it here for completeness sake
    OnionV2([u8; 10]),
    /// Tor v3 address. This is the preferred way to connect to a tor node
    OnionV3([u8; 32]),
    /// Cjdns ipv6 address
    Cjdns(Ipv6Addr),
    /// I2p address, a 32 byte node key
    I2p([u8; 32]),
}

/// Simple implementation of a DNS-over-HTTPS (DoH) lookup routed through the SOCKS5 proxy
pub mod dns_proxy {
    use std::net::IpAddr;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use rustls::crypto;
    use serde::Deserialize;
    use ureq::tls::TlsConfig;
    use ureq::tls::TlsProvider;
    use ureq::Agent;
    use ureq::Proxy;

    #[derive(Deserialize)]
    /// JSON format from [Google's DoH API](https://developers.google.com/speed/public-dns/docs/doh/json#dns_response_in_json)
    struct DnsResponse {
        /// We only care about the "Answer" array
        #[serde(rename = "Answer")]
        answers: Option<Vec<AnswerEntry>>,
    }

    #[derive(Deserialize)]
    struct AnswerEntry {
        /// The IP address as a string
        data: String,

        /// Record type; 1=A, 28=AAAA
        #[serde(rename = "type")]
        record_type: u8,
    }

    /// Lookup `host` by DNS-over-HTTPS (DoH) through a SOCKS5 proxy. Returns both A (IPv4)
    /// and AAAA (IPv6) records. Only Google sees the actual DNS query but doesn't learn our IP.
    pub fn lookup_host_via_proxy(
        host: &str,
        proxy_addr: SocketAddr,
    ) -> Result<Vec<IpAddr>, ureq::Error> {
        // Note: ureq does not implement "socks5h://", so this will resolve "dns.google" locally,
        // but the Bitcoin DNS query remains encrypted. Only Google can see the query contents.
        let proxy = Proxy::new(&format!("socks5://{proxy_addr}"))?;

        let crypto = Arc::new(crypto::aws_lc_rs::default_provider());
        let tls_config = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .unversioned_rustls_crypto_provider(crypto)
            .build();

        let agent: Agent = Agent::config_builder()
            .tls_config(tls_config)
            .timeout_global(Some(Duration::from_secs(30)))
            .proxy(Some(proxy))
            .build()
            .into();

        // We will perform two queries in sequence: type=1 (A) and type=28 (AAAA).
        let mut all_ips = Vec::new();
        for record_type in [1u8, 28u8] {
            let mut ips = query(&agent, host, record_type)?;
            all_ips.append(&mut ips);
        }

        Ok(all_ips)
    }

    // Helper function that performs a single DoH query for the given record_type.
    fn query(agent: &Agent, host: &str, record_type: u8) -> Result<Vec<IpAddr>, ureq::Error> {
        // Construct the DoH URL for the JSON API:
        // https://developers.google.com/speed/public-dns/docs/secure-transports
        let url = format!("https://dns.google/resolve?name={host}&type={record_type}");

        // Send a GET over HTTPS. The proxy will only see Google's address and the TLS handshake.
        let mut response = agent.get(&url).call()?;
        let dns_response: DnsResponse = response.body_mut().read_json()?;

        let answers = dns_response.answers.unwrap_or_default();

        // Filter by record_type (sanity) and parse each "data" field into an IpAddr.
        let mut result = Vec::new();
        for entry in answers.into_iter().filter(|e| e.record_type == record_type) {
            if let Ok(ip) = entry.data.parse() {
                result.push(ip);
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use std::fs::File;
    use std::io::Read;
    use std::io::{self};
    use std::net::Ipv4Addr;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use bitcoin::p2p::address::AddrV2;
    use bitcoin::p2p::ServiceFlags;
    use bitcoin::Network;
    use floresta_chain::get_chain_dns_seeds;
    use floresta_common::assert_ok;
    use floresta_common::service_flags;
    use rand::Rng;
    use serde::Deserialize;
    use serde::Serialize;

    use super::AddressState;
    use super::LocalAddress;
    use crate::address_man::AddressMan;
    use crate::address_man::ReachableNetworks;

    /// Seed Data for paesing in tests.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    pub struct SeedData {
        /// An actual address
        address: SeedAddress,
        /// Last time we successfully connected to this peer, only relevant is state == State::Tried
        last_connected: u64,
        /// Our local state for this peer, as defined in AddressState
        state: AddressState,
        /// Network services announced by this peer
        pub services: u64,
        /// Network port this peers listens to
        port: u16,
    }

    #[allow(non_snake_case)]
    #[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
    struct SeedAddress {
        V4: Ipv4Addr,
    }

    fn load_addresses_from_json(file_path: &str) -> io::Result<Vec<LocalAddress>> {
        let mut contents = String::new();
        File::open(file_path)?.read_to_string(&mut contents)?;

        let seeds: Vec<SeedData> =
            serde_json::from_str(&contents).expect("JSON not well-formatted");
        let mut addresses = Vec::new();
        let mut rng = rand::thread_rng();

        for seed in seeds {
            let state = match seed.state {
                AddressState::Tried(time) => AddressState::Tried(time),
                _ => continue,
            };

            let _address = AddrV2::Ipv4(seed.address.V4);

            let local_address = LocalAddress {
                address: _address,
                last_connected: seed.last_connected,
                state,
                services: ServiceFlags::from(seed.services),
                port: seed.port,
                id: rng.gen(),
            };
            addresses.push(local_address);
        }

        Ok(addresses)
    }

    #[test]
    fn test_local_addr_from_str() {
        // v4
        let ips = [
            "127.146.182.45",
            "2.212.31.248",
            "6.108.160.10",
            "151.43.223.99",
            "216.20.167.190",
            "188.33.163.249",
            "227.237.60.84",
            "8.104.121.145",
            "100.119.250.124",
        ];

        for addr_str in ips {
            let local_address = LocalAddress::try_from(format!("{addr_str}:8333").as_str())
                .unwrap_or_else(|_| panic!("failed to parse {addr_str}"));

            assert_eq!(
                local_address.address,
                AddrV2::Ipv4(addr_str.parse().unwrap())
            );
            assert_eq!(local_address.port, 8333);
        }

        // v6
        let ips = [
            "67db:3727:f145:5c59:718f:d3b9:6e56:d937",
            "7813:70c7:ea5d:f78a:7920:33d8:1da0:f9d7",
            "4a08:75e4:893f:d5a1:e2e2:3c99:8d20:22cf",
            "8da0:6b59:1494:bc7f:b217:51eb:c5fb:29c6",
            "cb1a:5104:57a9:0616:f6e0:191f:9224:4f35",
            "259a:ddc7:44a2:b5ec:f1ff:6024:50e8:928d",
            "46eb:cab1:bd48:c461:1775:c64e:c11b:3e77",
            "142b:a452:dff7:a41c:6cc6:317e:cc94:bb10",
            "0f8d:6d08:de58:017a:cd92:c868:023a:86e6",
            "8a80:5cfd:ccac:3e63:a243:d89f:d5e1:8e4c",
        ];

        for addr_str in ips {
            let local_address = LocalAddress::try_from(format!("[{addr_str}]:8333").as_str())
                .unwrap_or_else(|_| panic!("failed to parse {addr_str}"));

            assert_eq!(
                local_address.address,
                AddrV2::Ipv6(addr_str.parse().unwrap())
            );
            assert_eq!(local_address.port, 8333);
        }
    }

    #[test]
    fn test_parse() {
        let signet_address =
            load_addresses_from_json("./src/p2p_wire/seeds/signet_seeds.json").unwrap();

        assert!(!signet_address.is_empty());
        let random = rand::thread_rng().gen_range(1..=13);
        let loc_adr_1 = LocalAddress::from(signet_address[random].address.clone());
        assert_eq!(loc_adr_1.address, signet_address[random].address);
    }

    #[test]
    fn test_fixed_peers() {
        let _ = load_addresses_from_json("./src/p2p_wire/seeds/signet_seeds.json").unwrap();
        let _ = load_addresses_from_json("./src/p2p_wire/seeds/mainnet_seeds.json").unwrap();
        let _ = load_addresses_from_json("./src/p2p_wire/seeds/testnet_seeds.json").unwrap();
        let _ = load_addresses_from_json("./src/p2p_wire/seeds/regtest_seeds.json").unwrap();
    }

    #[test]
    fn test_address_man() {
        let mut address_man =
            AddressMan::new(None, &[ReachableNetworks::IPv4, ReachableNetworks::IPv6]);

        let signet_address =
            load_addresses_from_json("./src/p2p_wire/seeds/signet_seeds.json").unwrap();

        address_man.push_addresses(&signet_address);

        assert!(!address_man.good_addresses.is_empty());

        assert!(!address_man.peers_by_service.is_empty());

        assert!(!address_man.get_addresses_to_send().is_empty());

        assert!(address_man
            .get_address_to_connect(ServiceFlags::default(), true)
            .is_some());

        assert!(address_man
            .get_address_to_connect(ServiceFlags::default(), false)
            .is_some());

        assert!(address_man
            .get_address_to_connect(ServiceFlags::NONE, false)
            .is_some());

        assert!(address_man
            .get_address_to_connect(service_flags::UTREEXO.into(), false)
            .is_some());

        assert!(!AddressMan::get_net_seeds(Network::Signet).is_empty());
        assert!(!AddressMan::get_net_seeds(Network::Bitcoin).is_empty());
        assert!(!AddressMan::get_net_seeds(Network::Regtest).is_empty());
        assert!(!AddressMan::get_net_seeds(Network::Testnet).is_empty());

        assert_ok!(AddressMan::get_seeds_from_dns(
            &get_chain_dns_seeds(Network::Signet)[0],
            8333,
            None, // No proxy
        ));

        address_man.rearrange_buckets();
    }

    #[test]
    fn test_is_routable() {
        // random addresses that are private
        let addresses = vec![
            "10.42.187.23:8333",
            "10.0.254.199:8333",
            "172.16.88.4:8333",
            "172.31.201.77:8333",
            "192.168.1.14:8333",
            "192.168.203.250:8333",
            "[fd3a:9f2b:4c10:1a2b::1]:8333",
            "[fd12:3456:789a:1::dead]:8333",
            "[fdff:ab23:9012:beef::42]:8333",
            "[fd7c:2e91:aa10:ff01:1234:5678:9abc:def0]:8333",
            "[fd00:1111:2222:3333:4444:5555:6666:7777]:8333",
        ]
        .into_iter()
        .map(|s| {
            LocalAddress::try_from(s).unwrap_or_else(|_| panic!("Failed to parse address: {s}"))
        })
        .collect::<Vec<_>>();

        for address in addresses {
            assert!(!AddressMan::is_routable(&address));
        }

        // now load the signet seeds and ensure none are private
        let signet_address =
            load_addresses_from_json("./src/p2p_wire/seeds/signet_seeds.json").unwrap();

        for address in signet_address {
            assert!(AddressMan::is_routable(&address));
        }
    }

    fn get_addresses_and_random_times() -> Vec<LocalAddress> {
        let signet_address =
            load_addresses_from_json("./src/p2p_wire/seeds/signet_seeds.json").unwrap();

        // modify some addresses to have failed connections in the past
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut modified_addresses = signet_address.clone();
        let addresses = modified_addresses.len();
        for (i, item) in modified_addresses.iter_mut().enumerate().take(addresses) {
            if i % 3 == 0 {
                item.last_connected = now - 5000;
            } else if i % 3 == 1 {
                item.last_connected = now - 6000;
            } else {
                item.last_connected = now - 2000;
            }
        }

        modified_addresses
    }

    #[test]
    fn test_rearrange_buckets() {
        let mut address_man = AddressMan::new(None, &[]);
        let addresses = get_addresses_and_random_times();
        address_man.addresses.extend(
            addresses
                .iter()
                .map(|addr| (addr.id, addr.clone()))
                .collect::<std::collections::HashMap<usize, LocalAddress>>(),
        );

        assert_eq!(address_man.addresses.len(), addresses.len());
        address_man.rearrange_buckets();

        assert!(address_man.addresses.iter().all(|(_, addr)| {
            matches!(
                addr.state,
                AddressState::NeverTried | AddressState::Tried(_)
            )
        }));
    }

    #[test]
    fn test_is_reachable() {
        let v4 = "127.146.182.45";
        let v6 = "142b:a452:dff7:a41c:6cc6:317e:cc94:bb10";

        let addr_v4 = AddrV2::Ipv4(v4.parse().unwrap());
        let addr_v6 = AddrV2::Ipv6(v6.parse().unwrap());
        let addr_onionv3 = AddrV2::TorV3([
            0x89, 0x6c, 0x6a, 0x71, 0x70, 0x6b, 0x67, 0x61, 0x62, 0x67, 0x34, 0x68, 0x72, 0x63,
            0x68, 0x62, 0x6f, 0x7a, 0x77, 0x6f, 0x76, 0x66, 0x66, 0x79, 0x6b, 0x37, 0x66, 0x6f,
            0x62, 0x70, 0x6f, 0x76,
        ]);
        let addr_i2p = AddrV2::I2p([
            0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e,
            0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x66, 0x6f,
            0x62, 0x70, 0x6f, 0x76,
        ]);

        let address_man =
            AddressMan::new(None, &[ReachableNetworks::IPv4, ReachableNetworks::IPv6]);
        assert!(address_man.is_net_reachable(&LocalAddress {
            address: addr_v4,
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::default(),
            port: 8333,
            id: 0,
        }));

        assert!(address_man.is_net_reachable(&LocalAddress {
            address: addr_v6,
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::default(),
            port: 8333,
            id: 0,
        }));

        assert!(!address_man.is_net_reachable(&LocalAddress {
            address: addr_onionv3,
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::default(),
            port: 8333,
            id: 0,
        }));

        assert!(!address_man.is_net_reachable(&LocalAddress {
            address: addr_i2p,
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::default(),
            port: 8333,
            id: 0,
        }));
    }

    #[test]
    fn test_push_address() {
        let mut address_man = AddressMan::new(None, &[ReachableNetworks::IPv4]);
        let v4_no_witness = LocalAddress {
            address: AddrV2::Ipv4("12.146.182.45".parse().unwrap()),
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::NETWORK | ServiceFlags::NETWORK_LIMITED,
            port: 8333,
            id: 0,
        };

        let v4_with_witness = LocalAddress {
            address: AddrV2::Ipv4("12.146.182.45".parse().unwrap()),
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::NETWORK | ServiceFlags::NETWORK_LIMITED | ServiceFlags::WITNESS,
            port: 8333,
            id: 1,
        };

        let v6_with_witness = LocalAddress {
            address: AddrV2::Ipv6("fd3a:9f2b:4c10:1a2b::1".parse().unwrap()),
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::NETWORK_LIMITED | ServiceFlags::NETWORK | ServiceFlags::WITNESS,
            port: 8333,
            id: 2,
        };

        let v4_not_routable = LocalAddress {
            address: AddrV2::Ipv4("127.0.0.1".parse().unwrap()),
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::NETWORK_LIMITED | ServiceFlags::NETWORK | ServiceFlags::WITNESS,
            port: 8333,
            id: 3,
        };

        let v6_not_routable = LocalAddress {
            address: AddrV2::Ipv6("::1".parse().unwrap()),
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::NETWORK_LIMITED | ServiceFlags::NETWORK | ServiceFlags::WITNESS,
            port: 8333,
            id: 4,
        };

        let onion = LocalAddress {
            address: AddrV2::TorV3([
                0x89, 0x6c, 0x6a, 0x71, 0x70, 0x6b, 0x67, 0x61, 0x62, 0x67, 0x34, 0x68, 0x72, 0x63,
                0x68, 0x62, 0x6f, 0x7a, 0x77, 0x6f, 0x76, 0x66, 0x66, 0x79, 0x6b, 0x37, 0x66, 0x6f,
                0x62, 0x70, 0x6f, 0x76,
            ]),
            last_connected: 0,
            state: AddressState::NeverTried,
            services: ServiceFlags::NETWORK_LIMITED | ServiceFlags::NETWORK | ServiceFlags::WITNESS,
            port: 8333,
            id: 5,
        };

        let addresses = vec![
            v4_no_witness,
            v4_with_witness.clone(),
            v6_with_witness,
            v4_not_routable,
            v6_not_routable,
            onion,
        ];

        address_man.push_addresses(&addresses);

        // only the v4 with witness
        assert_eq!(address_man.addresses.len(), 1);
        assert_eq!(
            *address_man.addresses.values().next().unwrap(),
            v4_with_witness
        );
    }

    #[test]
    fn test_prune_addresses() {
        let mut address_man = AddressMan::new(Some(10), &[]);
        let addresses = get_addresses_and_random_times();
        address_man.addresses.extend(
            addresses
                .iter()
                .map(|addr| (addr.id, addr.clone()))
                .collect::<std::collections::HashMap<usize, LocalAddress>>(),
        );

        assert_eq!(address_man.addresses.len(), addresses.len(),);

        address_man.prune_addresses();

        assert_ne!(address_man.addresses.len(), addresses.len());
    }

    #[test]
    fn test_update_address_state() {
        let mut address_man = AddressMan::new(None, &[]);
        let addresses = get_addresses_and_random_times();
        address_man.addresses.extend(
            addresses
                .iter()
                .map(|addr| (addr.id, addr.clone()))
                .collect::<std::collections::HashMap<usize, LocalAddress>>(),
        );

        for addr in addresses {
            address_man.update_set_state(addr.id, AddressState::Banned(0));
        }

        assert!(address_man
            .addresses
            .values()
            .all(|addr| matches!(addr.state, AddressState::Banned(_))));
    }

    #[test]
    fn test_update_service_flags() {
        let mut address_man = AddressMan::new(None, &[]);
        let addresses = get_addresses_and_random_times();

        address_man.addresses.extend(
            addresses
                .iter()
                .map(|addr| (addr.id, addr.clone()))
                .collect::<std::collections::HashMap<usize, LocalAddress>>(),
        );

        for addr in addresses {
            address_man.update_set_service_flag(addr.id, service_flags::UTREEXO.into());
        }

        assert!(address_man
            .addresses
            .values()
            .all(|addr| addr.services.has(service_flags::UTREEXO.into())));
    }

    #[test]
    fn test_add_fixed_addresses() {
        let mut address_man =
            AddressMan::new(None, &[ReachableNetworks::IPv4, ReachableNetworks::IPv6]);
        address_man.add_fixed_addresses(Network::Signet);
        assert!(!address_man.addresses.is_empty());
    }
}
