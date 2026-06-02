// SPDX-License-Identifier: MIT OR Apache-2.0

//! Main file for this blockchain. A node is the central task that runs and handles important
//! events, such as new blocks, peer connection/disconnection, new addresses, etc.
//! A node should not care about peer-specific messages, peers'll handle things like pings.

mod blocks;
pub mod chain_selector_ctx;
mod conn;
mod peer_man;
pub mod running_ctx;
pub mod sync_ctx;
mod user_req;

use core::fmt::Debug;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use bitcoin::BlockHash;
use bitcoin::Network;
use bitcoin::Transaction;
use bitcoin::Txid;
use bitcoin::p2p::ServiceFlags;
use bitcoin::p2p::address::AddrV2Message;
pub(crate) use blocks::InflightBlock;
use floresta_chain::ChainBackend;
use floresta_common::Ema;
use floresta_common::try_and_log;
use floresta_common::try_and_warn;
use floresta_compact_filters::flat_filters_store::FlatFiltersStore;
use floresta_compact_filters::network_filters::NetworkFilters;
use floresta_mempool::Mempool;
pub use peer_man::AddedPeerInfo;
use running_ctx::RunningNode;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;
use tracing::info;

use super::UtreexoNodeConfig;
use super::address_man::AddressMan;
use super::address_man::LocalAddress;
use super::block_proof::Bitmap;
use super::error::WireError;
use super::node_context::NodeContext;
use super::node_handle::NodeResponse;
use super::node_handle::UserRequest;
use super::peer::PeerMessages;
use super::socks::Socks5StreamBuilder;
use super::transport::TransportProtocol;
use crate::bitcoin_socket_addr::BitcoinSocketAddr;
use crate::bitcoin_socket_addr::SystemResolver;
use crate::node_context::PeerId;
use crate::private_broadcast::PrivateBroadcast;
use crate::private_broadcast::PrivateBroadcastConnector;

/// As per BIP 155, limit the number of addresses to 1,000
pub const MAX_ADDRV2_ADDRESSES: usize = 1_000;

#[derive(Debug)]
pub enum NodeNotification {
    DnsSeedAddresses(Vec<LocalAddress>),
    FromPeer(u32, PeerMessages, Instant),
    FromUser(UserRequest, oneshot::Sender<NodeResponse>),
}

#[derive(Debug, Clone, PartialEq, Hash)]
/// Sent from node to peers, usually to request something
pub enum NodeRequest {
    /// Request the full block data for one or more blocks
    GetBlock(Vec<BlockHash>),

    /// Asks peer for headers
    GetHeaders(Vec<BlockHash>),

    /// Ask for other peers addresses
    GetAddresses,

    /// Asks this peer to shutdown
    Shutdown,

    /// Sends a transaction to peers
    BroadcastTransaction(Txid),

    /// Announce a queued transaction on a Tor private-broadcast outbound.
    ///
    /// Sent to a [`ConnectionKind::PrivateBroadcast`] peer after verack, when the node
    /// has assigned a transaction from [`PrivateBroadcast`] to that connection. The peer
    /// task keeps the full [`Transaction`] so it can answer the recipient's `getdata` without
    /// relying on the mempool, and writes a normal Bitcoin `inv` with the txid (same wire
    /// shape as [`NodeRequest::BroadcastTransaction`], but on a short-lived anonymous handshake).
    /// See [`crate::private_broadcast`] for the full inv → getdata → tx → ping flow.
    PrivateBroadcastInv(Transaction),

    /// Deliver the full transaction and probe for receipt on a private-broadcast peer.
    ///
    /// On the wire this is `tx` followed by `ping`; the recipient's `pong` is treated
    /// as confirmation that the transaction was received, after which the node shuts
    /// the connection down. Only used on [`ConnectionKind::PrivateBroadcast`] peers.
    /// The [`Transaction`] must match the txid previously announced with
    /// [`NodeRequest::PrivateBroadcastInv`]. In practice the peer task usually performs this step
    /// when it handles an incoming `getdata` for that txid rather than receiving this
    /// request from the node.
    PrivateBroadcastSendTx(Transaction),

    /// Ask for an unconfirmed transaction
    MempoolTransaction(Txid),

    /// Sends know addresses to our peers
    SendAddresses(Vec<AddrV2Message>),

    /// Requests the peer to send us the utreexo state for a given block
    GetUtreexoState((BlockHash, u32)),

    /// Requests the peer to send us the compact block filters for blocks
    /// starting at a given block hash and height.
    GetFilter((BlockHash, u32)),

    /// Sends a ping to the peer to check if it's alive
    Ping,

    /// Ask for the peer to send us the block proof for a given block
    ///
    /// The first bitmap tells which proof hashes do we want, and the second
    /// which leaf data the peer should send us.
    ///
    /// Proof hashes are the hashes needed to reconstruct the proof, while
    /// leaf data are the actual data of the leaves (i.e., the txouts).
    GetBlockProof((BlockHash, Bitmap, Bitmap)),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub(crate) enum InflightRequests {
    /// Requests the peer to send us the next block headers in their main chain
    Headers,

    /// Requests the peer to send us the utreexo state for a given peer
    UtreexoState(PeerId),

    /// Requests the peer to send us the block data for a given block hash
    Blocks(BlockHash),

    /// We've opened a connection with a peer, and are waiting for them to complete the handshake.
    Connect(PeerId),

    /// Requests the peer to send us the compact filters for blocks
    GetFilters,

    /// Requests the peer to send us the utreexo proof for a given block
    UtreexoProof(BlockHash),

    /// We've requested addresses from a peer
    GetAddresses,
}

#[derive(Debug, PartialEq, Clone, Copy)]
/// The kind of connection we see this peer as.
///
/// Core's counterpart: <https://github.com/bitcoin/bitcoin/blob/bf9ef4f0433551e850a11c2da8baae0ec6439a99/src/node/connection_types.h#L18>.
pub enum ConnectionKind {
    /// A feeler connection is a short-lived connection used to check whether this peer is alive.
    ///
    /// After handshake, we ask for addresses and when we receive an answer we just disconnect,
    /// marking this peer as alive in our address manager.
    Feeler,

    /// A regular peer, used to send requests to and learn about transactions and blocks.
    Regular(ServiceFlags),

    /// An extra peer specially created if our tip hasn't moved for too long.
    ///
    /// If more than [`NodeContext::ASSUME_STALE`] seconds have passed since the
    /// last processed block, we use this to make sure we are not in a partitioned subnet,
    /// unable to learn about new blocks.
    Extra,

    /// A connection that was manually requested by our user. This type of peer won't be banned on
    /// misbehaving, and won't respect the [`ServiceFlags`] requirements when creating a
    /// connection.
    Manual,

    /// A short-lived outbound Tor peer used to announce one locally submitted transaction.
    ///
    /// Opened when [`UtreexoNodeConfig::private_broadcast`] is enabled and a SOCKS5 proxy is
    /// configured. The node picks tx-recipient onion addresses from the address manager,
    /// completes a minimal handshake, relays the transaction with the usual inv/getdata/tx
    /// flow, and disconnects once the peer acknowledges receipt or the connection ages out.
    /// These peers do not participate in block sync or the public mempool relay path.
    PrivateBroadcast,
}

impl Serialize for ConnectionKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ConnectionKind::Feeler => serializer.serialize_str("feeler"),
            ConnectionKind::Regular(_) => serializer.serialize_str("regular"),
            ConnectionKind::Extra => serializer.serialize_str("extra"),
            ConnectionKind::Manual => serializer.serialize_str("manual"),
            ConnectionKind::PrivateBroadcast => serializer.serialize_str("private_broadcast"),
        }
    }
}

#[derive(Debug, Clone)]
/// Local information kept about each peer
pub struct LocalPeerView {
    /// Average message times from this peer
    ///
    /// This is measured in milliseconds, and it's recorded every time we get
    /// a response from a peer
    pub(crate) message_times: Ema,

    /// The state in which this peer is, e.g., awaiting handshake, ready, banned, etc.
    pub(crate) state: PeerStatus,

    /// A channel used to send requests to this peer
    pub(crate) channel: UnboundedSender<NodeRequest>,

    /// Services this peer claims to support
    pub(crate) services: ServiceFlags,

    /// A version string that identifies which software this peer is running
    pub(crate) user_agent: String,

    /// This peer's IP address
    pub(crate) address: LocalAddress,

    /// The last time we received a message from this peer
    pub(crate) _last_message: Instant,

    /// The kind of connection we have with this peer
    ///
    /// We use different connections with different goals, e.g., feeler connections,
    /// regular connections, extra connections to find about new tips, etc.
    pub(crate) kind: ConnectionKind,

    /// The latest height this peer has announced to us
    pub(crate) height: u32,

    /// The banscore of this peer
    ///
    /// This is a score kept for each peer, every time this peer misbehaves, we
    /// increase this score. If the score reaches a certain threshold, we ban
    /// the peer.
    pub(crate) banscore: u32,

    /// The transport protocol this peer is using (v1 or v2)
    pub(crate) transport_protocol: TransportProtocol,

    /// When this peer entered [`PeerStatus::Ready`], if ever.
    ///
    /// `None` while the handshake is incomplete ([`PeerStatus::Awaiting`]). Set when verack
    /// completes. Used with [`PeerStatus::Ready`] to disconnect stale
    /// [`ConnectionKind::PrivateBroadcast`] peers that stay ready too long without confirming
    /// transaction receipt.
    pub(crate) ready_since: Option<Instant>,

    /// Outbound dial targets Tor v3 via SOCKS5 (set in [`UtreexoNode::open_connection`]).
    pub(crate) is_outbound_tor_v3: bool,
}

impl LocalPeerView {
    /// Whether this peer advertises any service in `services`.
    pub(crate) fn has_any_service(&self, services: &[ServiceFlags]) -> bool {
        services.iter().any(|service| self.services.has(*service))
    }

    /// Whether this is a manually added peer
    pub(crate) const fn is_manual_peer(&self) -> bool {
        matches!(self.kind, ConnectionKind::Manual)
    }

    /// Whether this is a regular peer
    pub(crate) const fn is_regular_peer(&self) -> bool {
        matches!(self.kind, ConnectionKind::Regular(_))
    }

    // Connections expected to remain open if the peer doesn't die
    pub(crate) const fn is_long_lived(&self) -> bool {
        self.is_manual_peer() || self.is_regular_peer()
    }
}

pub struct NodeCommon<Chain: ChainBackend> {
    // 1. Core Blockchain and Transient Data
    pub(crate) chain: Chain,
    pub(crate) blocks: HashMap<BlockHash, InflightBlock>,
    pub(crate) mempool: Arc<tokio::sync::Mutex<Mempool>>,
    pub(crate) block_filters: Option<Arc<NetworkFilters<FlatFiltersStore>>>,
    pub(crate) last_filter: BlockHash,

    // 2. Peer Management
    pub(crate) peer_id_count: u32,
    pub(crate) peer_ids: Vec<u32>,
    pub(crate) peers: HashMap<u32, LocalPeerView>,
    pub(crate) peer_by_service: HashMap<ServiceFlags, Vec<u32>>,
    pub(crate) max_banscore: u32,
    pub(crate) address_man: AddressMan,
    pub(crate) added_peers: Vec<AddedPeerInfo>,
    pub(crate) tx_for_private_broadcast: PrivateBroadcast,
    pub(crate) private_broadcast_connector: PrivateBroadcastConnector,

    // 3. Internal Communication
    pub(crate) node_rx: UnboundedReceiver<NodeNotification>,
    pub(crate) node_tx: UnboundedSender<NodeNotification>,

    // 4. Networking Configuration
    pub(crate) socks5: Option<Socks5StreamBuilder>,
    pub(crate) fixed_peers: Vec<LocalAddress>,
    /// Outbound Tor v3 handshake succeeded at least once this process (via SOCKS5).
    pub(crate) is_outbound_tor_proven: bool,

    // 5. Time and Event Tracking
    pub(crate) inflight: HashMap<InflightRequests, (u32, Instant)>,
    pub(crate) inflight_user_requests:
        HashMap<UserRequest, (u32, Instant, oneshot::Sender<NodeResponse>)>,
    pub(crate) last_tip_update: Instant,
    pub(crate) last_connection: Instant,
    pub(crate) last_peer_db_dump: Instant,
    pub(crate) last_block_request: u32,
    pub(crate) last_get_address_request: Instant,
    pub(crate) last_send_addresses: Instant,
    pub(crate) block_sync_avg: Ema,
    pub(crate) last_feeler: Instant,
    pub(crate) startup_time: Instant,
    pub(crate) last_dns_seed_call: Instant,
    pub(crate) used_fixed_addresses: bool,
    pub(crate) last_private_broadcast_reattempt: Instant,

    // 6. Configuration and Metadata
    pub(crate) config: UtreexoNodeConfig,
    pub(crate) datadir: PathBuf,
    pub(crate) network: Network,
    pub(crate) kill_signal: Arc<tokio::sync::RwLock<bool>>,
}

/// The main node that operates while florestad is up.
///
/// [`UtreexoNode`] aims to be modular where `Chain` can be any implementation
/// of a [`ChainBackend`].
///
/// `Context` refers to which state the [`UtreexoNode`] is on, being
/// [`RunningNode`], [`SyncNode`], and [`ChainSelector`]. Defaults to
/// [`RunningNode`] which automatically transitions between contexts.
///
/// [`SyncNode`]: sync_ctx::SyncNode
/// [`ChainSelector`]: chain_selector_ctx::ChainSelector
pub struct UtreexoNode<Chain: ChainBackend, Context = RunningNode> {
    pub(crate) common: NodeCommon<Chain>,
    pub(crate) context: Context,
}

impl<Chain: ChainBackend, T> Deref for UtreexoNode<Chain, T> {
    fn deref(&self) -> &Self::Target {
        &self.common
    }
    type Target = NodeCommon<Chain>;
}

impl<T, Chain: ChainBackend> DerefMut for UtreexoNode<Chain, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.common
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Deserialize, Serialize)]
pub enum PeerStatus {
    Awaiting,
    Ready,
    Banned,
}

impl<T, Chain> UtreexoNode<Chain, T>
where
    T: 'static + Default + NodeContext,
    Chain: ChainBackend + 'static,
    WireError: From<Chain::Error>,
{
    pub fn new(
        config: UtreexoNodeConfig,
        chain: Chain,
        mempool: Arc<Mutex<Mempool>>,
        block_filters: Option<Arc<NetworkFilters<FlatFiltersStore>>>,
        kill_signal: Arc<tokio::sync::RwLock<bool>>,
        address_man: AddressMan,
    ) -> Result<Self, WireError> {
        let (node_tx, node_rx) = unbounded_channel();
        let socks5 = config.proxy.map(Socks5StreamBuilder::new);

        // Dedup the resolved fixed peers so we don't open multiple connections to the same host.
        let mut seen = HashSet::new();
        let mut fixed_peers = Vec::with_capacity(config.fixed_peers.len());
        for address in &config.fixed_peers {
            let resolved =
                BitcoinSocketAddr::parse_address(address, Some(config.network), SystemResolver)?;
            if seen.insert(resolved.clone()) {
                fixed_peers.push(LocalAddress::from(resolved));
            }
        }

        Ok(UtreexoNode {
            common: NodeCommon {
                last_dns_seed_call: Instant::now(),
                startup_time: Instant::now(),
                // The last 1k blocks account for 50% of the EMA weight, the last 2k for 75%, etc.
                block_sync_avg: Ema::with_half_life_1000(),
                last_filter: chain.get_block_hash(0).unwrap(),
                block_filters,
                inflight: HashMap::new(),
                inflight_user_requests: HashMap::new(),
                peer_id_count: 0,
                peers: HashMap::new(),
                last_block_request: chain.get_validation_index().expect("Invalid chain"),
                chain,
                peer_ids: Vec::new(),
                peer_by_service: HashMap::new(),
                mempool,
                network: config.network,
                node_rx,
                node_tx,
                address_man,
                last_tip_update: Instant::now(),
                last_connection: Instant::now(),
                last_peer_db_dump: Instant::now(),
                last_feeler: Instant::now(),
                blocks: HashMap::new(),
                last_get_address_request: Instant::now(),
                last_send_addresses: Instant::now(),
                used_fixed_addresses: false,
                last_private_broadcast_reattempt: Instant::now(),
                datadir: config.datadir.clone(),
                max_banscore: config.max_banscore,
                socks5,
                fixed_peers,
                is_outbound_tor_proven: false,
                config,
                kill_signal,
                added_peers: Vec::new(),
                tx_for_private_broadcast: PrivateBroadcast::default(),
                private_broadcast_connector: PrivateBroadcastConnector::default(),
            },
            context: T::default(),
        })
    }

    pub(crate) fn shutdown(&mut self) {
        info!("Shutting down node...");
        try_and_warn!(self.save_utreexo_peers());
        for peer in self.peer_ids.iter() {
            try_and_log!(self.send_to_peer(*peer, NodeRequest::Shutdown));
        }
        try_and_log!(self.save_peers());
        try_and_log!(self.chain.flush());
    }
}

/// If `$interval_secs` has passed since `$timer`, run `$what` and reset `$timer`.
macro_rules! periodic_job {
    ($timer:expr => $what:expr, $interval_secs:path $(,)?) => {{
        if $timer.elapsed() > Duration::from_secs($interval_secs) {
            try_and_log!($what);
            $timer = Instant::now();
        }
    }};

    ($timer:expr => $what:expr, $interval_secs:path,no_log $(,)?) => {{
        if $timer.elapsed() > Duration::from_secs($interval_secs) {
            let _ = $what;
            $timer = Instant::now();
        }
    }};
}

pub(crate) use periodic_job;
