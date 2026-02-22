use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bip324::serde::CommandString;
use bitcoin::bip158::BlockFilter;
use bitcoin::block::Header as BlockHeader;
use bitcoin::consensus::deserialize;
use bitcoin::consensus::encode;
use bitcoin::consensus::serialize;
use bitcoin::hashes::Hash;
use bitcoin::p2p::address::AddrV2Message;
use bitcoin::p2p::message::NetworkMessage;
use bitcoin::p2p::message_blockdata::Inventory;
use bitcoin::p2p::message_network::VersionMessage;
use bitcoin::p2p::ServiceFlags;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Transaction;
use floresta_common::impl_error_from;
use floresta_mempool::Mempool;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::spawn;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::error;
use tracing::warn;

use self::peer_utils::make_pong;
use super::node::NodeNotification;
use super::node::NodeRequest;
use super::transport::TransportError;
use super::transport::TransportProtocol;
use super::transport::WriteTransport;
use crate::block_proof::UtreexoProofMask;
use crate::node::ConnectionKind;
use crate::p2p_wire::block_proof::GetUtreexoProof;
use crate::p2p_wire::block_proof::UtreexoProof;
use crate::p2p_wire::transport::ReadTransport;

/// If we send a ping, and our peer takes more than PING_TIMEOUT to
/// reply, disconnect.
const PING_TIMEOUT: u64 = 30;

/// If the last message we've got was more than XX, send out a ping
const SEND_PING_TIMEOUT: u64 = 60;

/// The command string for the "utreexo proof" message
const UTREEXO_PROOF_CMD_STRING: &str = "uproof";

/// The command string for the "get utreexo proof" message
const GET_UTREEXO_PROOF_CMD: &str = "getuproof";

#[derive(Debug, PartialEq)]
enum State {
    None,
    SentVersion(Instant),
    SentVerack,
    Connected,
}

pub struct MessageActor<R: AsyncRead + Unpin + Send> {
    pub transport: ReadTransport<R>,
    pub sender: UnboundedSender<ReaderMessage>,
}

impl<R: AsyncRead + Unpin + Send> MessageActor<R> {
    async fn inner(&mut self) -> std::result::Result<(), PeerError> {
        loop {
            let msg = self.transport.read_message().await?;
            let now = Instant::now();
            self.sender.send(ReaderMessage::Message(msg, now))?;
        }
    }

    pub async fn run(mut self) -> Result<()> {
        if let Err(err) = self.inner().await {
            self.sender.send(ReaderMessage::Error(err))?;
        }
        Ok(())
    }
}

pub fn create_actors<R: AsyncRead + Unpin + Send>(
    transport: ReadTransport<R>,
) -> (UnboundedReceiver<ReaderMessage>, MessageActor<R>) {
    let (actor_sender, actor_receiver) = unbounded_channel();
    let actor = MessageActor {
        transport,
        sender: actor_sender,
    };
    (actor_receiver, actor)
}

pub struct Peer<T: AsyncWrite + Unpin + Send + Sync> {
    mempool: Arc<Mutex<Mempool>>,
    blocks_only: bool,
    services: ServiceFlags,
    user_agent: String,
    messages: u64,
    start_time: Instant,
    last_message: Instant,
    current_best_block: i32,
    last_ping: Option<Instant>,
    id: u32,
    node_tx: UnboundedSender<NodeNotification>,
    state: State,
    send_headers: bool,
    node_requests: UnboundedReceiver<NodeRequest>,
    address_id: usize,
    kind: ConnectionKind,
    wants_addrv2: bool,
    shutdown: bool,
    actor_receiver: UnboundedReceiver<ReaderMessage>, // Add the receiver for messages from TcpStreamActor
    writer: WriteTransport<T>,
    our_user_agent: String,
    cancellation_sender: tokio::sync::oneshot::Sender<()>,
    transport_protocol: TransportProtocol,
}

#[derive(Debug)]
/// Enum for diverse variants of errors when dealing with a [`Peer`]
pub enum PeerError {
    /// Error while sending data to a peer
    Send,

    /// Error while reading data from a peer
    Read(std::io::Error),

    /// Error while parsing message
    Parse(encode::Error),

    /// Peer sent us a message that we aren't expecting
    UnexpectedMessage,

    /// Peer sent us a message that is too big
    MessageTooBig,

    /// Peer sent us a message with the wrong magic bits
    MagicBitsMismatch,

    /// Peer sent us too many messages in a short period of time
    TooManyMessages,

    /// Peer timed out a ping message
    PingTimeout,

    /// Channel error
    Channel,

    /// Transport error
    Transport(TransportError),
}

impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerError::Send => write!(f, "Error while sending to peer"),
            PeerError::Read(err) => write!(f, "Error while reading from peer: {err:?}"),
            PeerError::Parse(err) => write!(f, "Error while parsing message: {err:?}"),
            PeerError::UnexpectedMessage => {
                write!(f, "Peer sent us a message that we aren't expecting")
            }
            PeerError::MessageTooBig => write!(f, "Peer sent us a message that is too big"),
            PeerError::MagicBitsMismatch => {
                write!(f, "Peer sent us a message with the wrong magic bits")
            }
            PeerError::TooManyMessages => {
                write!(
                    f,
                    "Peer sent us too many messages in a short period of time"
                )
            }
            PeerError::PingTimeout => write!(f, "Peer timed out a ping"),
            PeerError::Channel => write!(f, "Channel error with empty data"),
            PeerError::Transport(err) => write!(f, "Transport error: {err:?}"),
        }
    }
}

impl_error_from!(PeerError, TransportError, Transport);
impl_error_from!(PeerError, std::io::Error, Read);
impl_error_from!(PeerError, encode::Error, Parse);

impl From<SendError<ReaderMessage>> for PeerError {
    fn from(_: SendError<ReaderMessage>) -> Self {
        PeerError::Channel
    }
}

pub enum ReaderMessage {
    Message(NetworkMessage, Instant),
    Error(PeerError),
}

impl<T: AsyncWrite + Unpin + Send + Sync> Debug for Peer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)?;
        Ok(())
    }
}

type Result<T> = std::result::Result<T, PeerError>;

impl<T: AsyncWrite + Unpin + Send + Sync> Peer<T> {
    pub async fn read_loop(mut self) -> Result<()> {
        let err = self.peer_loop_inner().await;
        if err.is_err() {
            debug!("Peer {} connection loop closed: {err:?}", self.id);
        }

        let now = Instant::now();
        self.send_to_node(PeerMessages::Disconnected(self.address_id), now);

        // force the stream to shutdown to prevent leaking resources
        if let Err(shutdown_err) = self.writer.shutdown().await {
            debug!(
                "Failed to shutdown writer for Peer {}: {shutdown_err:?}",
                self.id
            );
        }

        if let Err(cancellation_err) = self.cancellation_sender.send(()) {
            debug!(
                "Failed to propagate cancellation signal for Peer {}: {cancellation_err:?}",
                self.id
            );
        }

        if let Err(err) = err {
            debug!("Peer {} connection loop closed: {err:?}", self.id);
        }

        Ok(())
    }

    async fn peer_loop_inner(&mut self) -> Result<()> {
        // send a version
        let version = peer_utils::build_version_message(self.our_user_agent.clone());
        self.write(version).await?;
        self.state = State::SentVersion(Instant::now());
        loop {
            tokio::select! {
                request = tokio::time::timeout(Duration::from_secs(2), self.node_requests.recv()) => {
                    match request {
                        Ok(None) => {
                            return Err(PeerError::Channel);
                        },
                        Ok(Some(request)) => {
                            self.handle_node_request(request).await?;
                        },
                        Err(_) => {
                            // Timeout, do nothing
                        }
                    }
                },
                message = self.actor_receiver.recv() => {
                    match message {
                        None => {
                            return Err(PeerError::Channel);
                        }
                        Some(ReaderMessage::Error(e)) => {
                            return Err(e);
                        }
                        Some(ReaderMessage::Message(msg, time)) => {
                            self.handle_peer_message(msg, time).await?;
                        }
                    }
                }
            }

            if self.shutdown {
                return Ok(());
            }

            // If we send a ping and our peer doesn't respond in time, disconnect
            if let Some(when) = self.last_ping {
                if when.elapsed().as_secs() > PING_TIMEOUT {
                    return Err(PeerError::PingTimeout);
                }
            }

            // Send a ping to check if this peer is still good
            let last_message = self.last_message.elapsed().as_secs();
            if last_message > SEND_PING_TIMEOUT {
                if self.last_ping.is_some() {
                    continue;
                }
                let nonce = rand::random();
                self.last_ping = Some(Instant::now());
                self.write(NetworkMessage::Ping(nonce)).await?;
            }

            // divide the number of messages by the number of seconds we've been connected,
            // if it's more than 10 msg/sec, this peer is sending us too many messages, and we should
            // disconnect.
            let msg_sec = self
                .messages
                .checked_div(Instant::now().duration_since(self.start_time).as_secs())
                .unwrap_or(0);

            if msg_sec > 10 {
                error!(
                    "Peer {} is sending us too many messages, disconnecting",
                    self.id
                );
                return Err(PeerError::TooManyMessages);
            }

            if let State::SentVersion(when) = self.state {
                if Instant::now().duration_since(when) > Duration::from_secs(10) {
                    return Err(PeerError::UnexpectedMessage);
                }
            }
        }
    }

    pub async fn handle_node_request(&mut self, request: NodeRequest) -> Result<()> {
        assert_eq!(self.state, State::Connected);
        debug!("Handling node request: {request:?}");
        match request {
            NodeRequest::GetBlock(block_hashes) => {
                let inv = block_hashes
                    .iter()
                    .map(|block| Inventory::Block(*block))
                    .collect();

                let _ = self.write(NetworkMessage::GetData(inv)).await;
            }
            NodeRequest::GetUtreexoState((block_hash, height)) => {
                let get_filter = bitcoin::p2p::message_filter::GetCFilters {
                    filter_type: 1,
                    start_height: height,
                    stop_hash: block_hash,
                };

                let _ = self.write(NetworkMessage::GetCFilters(get_filter)).await;
            }
            NodeRequest::GetHeaders(locator) => {
                let _ = self
                    .write(NetworkMessage::GetHeaders(
                        bitcoin::p2p::message_blockdata::GetHeadersMessage {
                            version: 0,
                            locator_hashes: locator,
                            stop_hash: BlockHash::all_zeros(),
                        },
                    ))
                    .await;
            }
            NodeRequest::Shutdown => {
                self.shutdown = true;
                self.writer.shutdown().await?;
            }
            NodeRequest::GetAddresses => {
                self.write(NetworkMessage::GetAddr).await?;
            }
            NodeRequest::BroadcastTransaction(tx) => {
                self.write(NetworkMessage::Inv(vec![Inventory::Transaction(tx)]))
                    .await?;
            }
            NodeRequest::MempoolTransaction(txid) => {
                self.write(NetworkMessage::GetData(vec![Inventory::Transaction(txid)]))
                    .await?;
            }
            NodeRequest::SendAddresses(addresses) => {
                self.write(NetworkMessage::AddrV2(addresses)).await?;
            }
            NodeRequest::GetFilter((stop_hash, start_height)) => {
                let get_filter = bitcoin::p2p::message_filter::GetCFilters {
                    filter_type: 0,
                    start_height,
                    stop_hash,
                };

                self.write(NetworkMessage::GetCFilters(get_filter)).await?;
            }
            NodeRequest::Ping => {
                let nonce = rand::random();
                self.last_ping = Some(Instant::now());
                self.write(NetworkMessage::Ping(nonce)).await?;
            }
            NodeRequest::GetBlockProof((block_hash, proof_hashes_bitmap, leaf_index_bitmap)) => {
                let get_block_proof = GetUtreexoProof {
                    block_hash,
                    request_bitmap: UtreexoProofMask::request_all(),
                    proof_hashes_bitmap,
                    leaf_index_bitmap,
                };

                self.write(NetworkMessage::Unknown {
                    command: CommandString::try_from_static(GET_UTREEXO_PROOF_CMD)
                        .expect("Invalid command string"),
                    payload: serialize(&get_block_proof),
                })
                .await?;
            }
        }
        Ok(())
    }

    pub async fn handle_peer_message(
        &mut self,
        message: NetworkMessage,
        time: Instant,
    ) -> Result<()> {
        self.last_message = time;

        debug!("Received {} from peer {}", message.command(), self.id);
        match self.state {
            State::Connected => match message {
                NetworkMessage::Inv(inv) => {
                    for inv_entry in inv {
                        match inv_entry {
                            Inventory::Error => {}
                            Inventory::Transaction(_) => {}
                            Inventory::Block(block_hash)
                            | Inventory::WitnessBlock(block_hash)
                            | Inventory::CompactBlock(block_hash) => {
                                self.send_to_node(PeerMessages::NewBlock(block_hash), time);
                            }
                            _ => {}
                        }
                    }
                }
                NetworkMessage::GetHeaders(_) => {
                    self.write(NetworkMessage::Headers(Vec::new())).await?;
                }
                NetworkMessage::Headers(headers) => {
                    self.send_to_node(PeerMessages::Headers(headers), time);
                }
                NetworkMessage::SendHeaders => {
                    self.send_headers = true;
                    self.write(NetworkMessage::SendHeaders).await?;
                }
                NetworkMessage::Ping(nonce) => {
                    self.handle_ping(nonce).await?;
                }
                NetworkMessage::FeeFilter(_) => {
                    self.write(NetworkMessage::FeeFilter(1000)).await?;
                }
                NetworkMessage::AddrV2(addresses) => {
                    self.send_to_node(PeerMessages::Addr(addresses), time);
                }
                NetworkMessage::GetBlocks(_) => {
                    self.write(NetworkMessage::Inv(Vec::new())).await?;
                }
                NetworkMessage::GetAddr => {
                    self.write(NetworkMessage::AddrV2(Vec::new())).await?;
                }
                NetworkMessage::GetData(inv) => {
                    for inv_el in inv {
                        self.handle_get_data(inv_el).await?;
                    }
                }
                NetworkMessage::Tx(tx) => {
                    self.send_to_node(PeerMessages::Transaction(tx), time);
                }
                NetworkMessage::NotFound(inv) => {
                    for inv_el in inv {
                        self.send_to_node(PeerMessages::NotFound(inv_el), time);
                    }
                }
                NetworkMessage::SendAddrV2 => {
                    self.wants_addrv2 = true;
                    self.write(NetworkMessage::SendAddrV2).await?;
                }
                NetworkMessage::Pong(_) => {
                    self.last_ping = None;
                }
                NetworkMessage::Unknown { command, payload } => {
                    let utreexo_proof_cmd =
                        CommandString::try_from_static(UTREEXO_PROOF_CMD_STRING)
                            .expect("Invalid command string");

                    if command != utreexo_proof_cmd {
                        warn!("Unknown command string: {command}");
                        return Ok(());
                    }

                    let utreexo_proof: UtreexoProof = deserialize(&payload)?;
                    self.send_to_node(PeerMessages::UtreexoProof(utreexo_proof), time);

                    return Ok(());
                }
                NetworkMessage::Block(block) => {
                    self.send_to_node(PeerMessages::Block(block), time);
                }
                NetworkMessage::CFilter(filter_msg) => match filter_msg.filter_type {
                    0 => {
                        let filter = BlockFilter::new(&filter_msg.filter);

                        self.send_to_node(
                            PeerMessages::BlockFilter((filter_msg.block_hash, filter)),
                            time,
                        );
                    }
                    1 => {
                        self.send_to_node(PeerMessages::UtreexoState(filter_msg.filter), time);
                    }
                    _ => {}
                },
                // Explicitly ignore these messages, if something changes in the future
                // this would cause a compile error.
                NetworkMessage::Verack
                | NetworkMessage::Version(_)
                | NetworkMessage::WtxidRelay
                | NetworkMessage::Reject(_)
                | NetworkMessage::Alert(_)
                | NetworkMessage::BlockTxn(_)
                | NetworkMessage::CFCheckpt(_)
                | NetworkMessage::CFHeaders(_)
                | NetworkMessage::CmpctBlock(_)
                | NetworkMessage::FilterAdd(_)
                | NetworkMessage::FilterClear
                | NetworkMessage::FilterLoad(_)
                | NetworkMessage::GetBlockTxn(_)
                | NetworkMessage::GetCFCheckpt(_)
                | NetworkMessage::GetCFHeaders(_)
                | NetworkMessage::Addr(_)
                | NetworkMessage::GetCFilters(_)
                | NetworkMessage::MemPool
                | NetworkMessage::MerkleBlock(_)
                | NetworkMessage::SendCmpct(_) => {}
            },
            State::None | State::SentVersion(_) => match message {
                bitcoin::p2p::message::NetworkMessage::Version(version) => {
                    self.handle_version(version).await?;
                }
                _ => {
                    warn!("unexpected message: {:?} from peer {}", message, self.id);
                    return Err(PeerError::UnexpectedMessage);
                }
            },
            State::SentVerack => match message {
                bitcoin::p2p::message::NetworkMessage::Verack => {
                    self.state = State::Connected;
                    self.send_to_node(
                        PeerMessages::Ready(Version {
                            user_agent: self.user_agent.clone(),
                            protocol_version: 0,
                            id: self.id,
                            blocks: self.current_best_block.unsigned_abs(),
                            address_id: self.address_id,
                            services: self.services,
                            kind: self.kind,
                            transport_protocol: self.transport_protocol,
                        }),
                        time,
                    );
                }
                bitcoin::p2p::message::NetworkMessage::SendAddrV2 => {
                    self.wants_addrv2 = true;
                }
                bitcoin::p2p::message::NetworkMessage::SendHeaders => {
                    self.send_headers = true;
                }
                bitcoin::p2p::message::NetworkMessage::WtxidRelay => {}
                _ => {
                    warn!("unexpected message: {:?} from peer {}", message, self.id);
                    return Err(PeerError::UnexpectedMessage);
                }
            },
        }
        Ok(())
    }
}

impl<T: AsyncWrite + Unpin + Send + Sync> Peer<T> {
    pub async fn write(&mut self, msg: NetworkMessage) -> Result<()> {
        debug!("Writing {} to peer {}", msg.command(), self.id);
        self.writer.write_message(msg).await?;
        Ok(())
    }

    pub async fn handle_get_data(&mut self, inv: Inventory) -> Result<()> {
        match inv {
            Inventory::WitnessTransaction(txid) => {
                let tx = self.mempool.lock().await.get_from_mempool(&txid).cloned();
                if let Some(tx) = tx {
                    self.write(NetworkMessage::Tx(tx)).await?;
                }
            }
            Inventory::Transaction(txid) => {
                let tx = self.mempool.lock().await.get_from_mempool(&txid).cloned();
                if let Some(tx) = tx {
                    self.write(NetworkMessage::Tx(tx)).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_peer<W: AsyncWrite + Unpin + Send + Sync + 'static>(
        id: u32,
        mempool: Arc<Mutex<Mempool>>,
        node_tx: UnboundedSender<NodeNotification>,
        node_requests: UnboundedReceiver<NodeRequest>,
        address_id: usize,
        kind: ConnectionKind,
        actor_receiver: UnboundedReceiver<ReaderMessage>,
        writer: WriteTransport<W>,
        our_user_agent: String,
        cancellation_sender: tokio::sync::oneshot::Sender<()>,
        transport_protocol: TransportProtocol,
    ) {
        let peer = Peer {
            address_id,
            blocks_only: false,
            current_best_block: -1,
            id,
            mempool,
            last_ping: None,
            last_message: Instant::now(),
            node_tx,
            services: ServiceFlags::NONE,
            messages: 0,
            start_time: Instant::now(),
            user_agent: "".into(),
            state: State::None,
            send_headers: false,
            node_requests,
            kind,
            wants_addrv2: false,
            shutdown: false,
            actor_receiver, // Add the receiver for messages from TcpStreamActor
            writer,
            our_user_agent,
            cancellation_sender,
            transport_protocol,
        };

        spawn(peer.read_loop());
    }

    async fn handle_ping(&mut self, nonce: u64) -> Result<()> {
        let pong = make_pong(nonce);
        self.write(pong).await
    }

    async fn handle_version(&mut self, version: VersionMessage) -> Result<()> {
        self.user_agent = version.user_agent;
        self.blocks_only = !version.relay;
        self.current_best_block = version.start_height;
        self.services = version.services;
        if version.version >= 70016 {
            self.write(NetworkMessage::SendAddrV2).await?;
        }
        self.state = State::SentVerack;
        let verack = NetworkMessage::Verack;
        self.state = State::SentVerack;
        self.write(verack).await
    }

    fn send_to_node(&self, message: PeerMessages, time: Instant) {
        let message = NodeNotification::FromPeer(self.id, message, time);
        let _ = self.node_tx.send(message);
    }
}

pub(super) mod peer_utils {
    use std::net::IpAddr;
    use std::net::Ipv4Addr;
    use std::net::SocketAddr;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use bitcoin::p2p::address;
    use bitcoin::p2p::message::NetworkMessage;
    use bitcoin::p2p::message::{self};
    use bitcoin::p2p::message_network;

    /// Protocol version we speak
    pub const PROTOCOL_VERSION: u32 = 70016;

    pub(super) fn make_pong(nonce: u64) -> NetworkMessage {
        NetworkMessage::Pong(nonce)
    }

    pub(crate) fn build_version_message(user_agent: String) -> message::NetworkMessage {
        use bitcoin::p2p::ServiceFlags;

        // Building version message, see https://en.bitcoin.it/wiki/Protocol_documentation#version
        let my_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38332);

        // "bitfield of features to be enabled for this connection"
        let services = ServiceFlags::NETWORK | ServiceFlags::from(1 << 24) | ServiceFlags::WITNESS;

        // "standard UNIX timestamp in seconds"
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time error")
            .as_secs();

        // "The network address of the node receiving this message"
        let addr_recv = address::Address::new(&my_address, ServiceFlags::NONE);

        // "The network address of the node emitting this message"
        let addr_from = address::Address::new(&my_address, ServiceFlags::NONE);

        // "Node random nonce, randomly generated every time a version packet is sent. This nonce is used to detect connections to self."
        let nonce: u64 = 1;

        // "The last block received by the emitting node"
        let start_height: i32 = 0;

        // Construct the message
        message::NetworkMessage::Version(message_network::VersionMessage {
            services,
            timestamp: timestamp as i64,
            receiver: addr_recv,
            sender: addr_from,
            nonce,
            user_agent,
            start_height,
            relay: false,
            version: PROTOCOL_VERSION,
        })
    }
}

#[derive(Debug)]
pub struct Version {
    pub user_agent: String,
    pub protocol_version: u32,
    pub blocks: u32,
    pub id: u32,
    pub address_id: usize,
    pub services: ServiceFlags,
    pub kind: ConnectionKind,
    pub transport_protocol: TransportProtocol,
}

/// Messages passed from different modules to the main node to process. They should minimal
/// and only if it requires global states, everything else should be handled by the module
/// itself.
#[derive(Debug)]
pub enum PeerMessages {
    /// A new block just arrived, we should ask for it and update our chain
    NewBlock(BlockHash),

    /// We got a full block from our peer, presumptively we asked for it
    Block(Block),

    /// A response to a `getheaders` request
    Headers(Vec<BlockHeader>),

    /// We got some p2p addresses, add this to our local database
    Addr(Vec<AddrV2Message>),

    /// Peer notify its readiness
    Ready(Version),

    /// Remote peer disconnected
    Disconnected(usize),

    /// Remote peer doesn't know the data we asked for
    NotFound(Inventory),

    /// Remote peer sent us a transaction
    Transaction(Transaction),

    /// Remote peer sent us a Utreexo state
    UtreexoState(Vec<u8>),

    /// Remote peer sent us a compact block filter
    BlockFilter((BlockHash, BlockFilter)),

    /// Remote peer sent us a Utreexo proof,
    UtreexoProof(UtreexoProof),
}
