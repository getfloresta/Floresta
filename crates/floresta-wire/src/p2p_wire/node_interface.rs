//! node_interface, which holds [`NodeInterface`] and related methods
//! that define the API to interact with the floresta node

use std::net::IpAddr;
use std::net::SocketAddr;
use std::time::Instant;

use bitcoin::p2p::ServiceFlags;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Transaction;
use bitcoin::Txid;
use rustreexo::accumulator::proof::Proof;
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use super::node::ConnectionKind;
use super::node::NodeNotification;
use super::node::PeerStatus;
use super::transport::TransportProtocol;
use super::UtreexoNodeConfig;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// A request to addnode that can be made to the node.
///
/// This enum represents all the possible requests that can be made to the node to add, remove
/// or just try to connect to a peer, following the same pattern as the `addnode` command in [Bitcoin Core].
///
/// [Bitcoin Core]: (https://bitcoincore.org/en/doc/29.0.0/rpc/network/addnode/)
pub enum AddNode {
    /// The `Add` variant is used to add a peer to the node's peer list
    Add((IpAddr, u16)),

    /// The `Remove` variant is used to remove a peer from the node's peer list
    Remove((IpAddr, u16)),

    /// The `Onetry` variant is used to try a connection to the peer once, but not add it to the peer list.
    Onetry((IpAddr, u16)),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// A request that can be made to the node.
///
/// While the node is running, consumers may want to request some useful data, like block data,
/// mempool transactions or tell the node to connect with some given peers. This struct represents
/// all the possible requests that can be made to the node as well as the data that needs to be
/// sent along with the request.
pub enum UserRequest {
    /// Request the [`UtreexoNodeConfig`] of the node.
    Config,

    /// Get a block by its hash.
    ///
    /// This will cause network requests to be made to fetch the block data.
    Block(BlockHash),

    /// Get the Utreexo proof and LeafData for a block by its hash.
    UtreexoProof(BlockHash),

    /// Get an unconfirmed transaction from the mempool by its ID.
    MempoolTransaction(Txid),

    /// Return information about all connected peers.
    GetPeerInfo,

    /// Add a peer to the node's peer list.
    ///
    /// This function will add this peer to a special list of peers such that, if we lose the
    /// connection, we will keep trying to connect to it until we succeed.
    Add((IpAddr, u16, bool)),

    /// Removes a node from the node's peer list.
    ///
    /// This function will remove a node that was added with [`AddNode::Add`]. This will **not**
    /// disconnect the peer, but if it disconnects, it will not be reconnected again.
    Remove((IpAddr, u16)),

    /// Attempts to connect to a peer once.
    ///
    /// Different from [`AddNode::Add`], this function will try to connect to the peer once, but
    /// will not add it to the node's added peers list.
    Onetry((IpAddr, u16, bool)),

    /// Attempt to disconnect from a peer.
    Disconnect((IpAddr, u16)),

    /// Ping all connected peers to check if they are alive.
    Ping,
}

#[derive(Debug, Clone, Serialize)]
/// A struct representing a peer connected to the node.
///
/// This struct contains information about a peer connected to the node, like its address, the
/// services it provides, the user agent it's using, the height of the blockchain it's currently
/// at, its state and the kind of connection it has with the node.
pub struct PeerInfo {
    pub id: u32,
    pub address: SocketAddr,
    #[serde(serialize_with = "serialize_service_flags")]
    pub services: ServiceFlags,
    pub user_agent: String,
    pub initial_height: u32,
    pub state: PeerStatus,
    pub kind: ConnectionKind,
    pub transport_protocol: TransportProtocol,
}

#[derive(Debug, Clone)]
/// A response that can be sent back to the user.
///
/// When the user makes a request to the node, the node will respond with some data. This enum
/// represents all the possible responses that the node can send back to the user.
pub enum NodeResponse {
    /// The [`UtreexoNodeConfig`] of the node.
    Config(UtreexoNodeConfig),

    /// A response containing a block, if we could fetch it.
    Block(Option<Block>),

    /// A response containing a Utreexo proof, if we could fetch it.
    UtreexoProof(Option<Proof>),

    /// A response containing a transaction from the mempool, if we could fetch it.
    MempoolTransaction(Option<Transaction>),

    /// A response containing a list of peer information.
    GetPeerInfo(Vec<PeerInfo>),

    /// A response indicating whether a peer was successfully added.
    Add(bool),

    /// A response indicating whether a peer was successfully removed.
    Remove(bool),

    // A response indicating whether a peer was successfully disconnected from.
    Disconnect(bool),

    /// A response indicating whether a peer was successfully connected once.
    Onetry(bool),

    /// A response indicating whether the ping was successful.
    Ping(bool),
}

#[derive(Debug, Clone)]
/// A struct representing the interface to the node.
///
/// This struct will be used by consumers to interact with the node. You may have as many of it as
/// you need, and you can use it to send requests to the node and get responses back.
pub struct NodeInterface {
    node_sender: UnboundedSender<NodeNotification>,
}

#[derive(Debug)]
pub struct RequestData {
    pub time: Instant,
    pub resolve: oneshot::Sender<Option<NodeResponse>>,
    pub req: UserRequest,
}

impl NodeInterface {
    pub fn new(node_sender: UnboundedSender<NodeNotification>) -> Self {
        NodeInterface { node_sender }
    }

    /// Sends a request to the node.
    ///
    /// This is an internal utility function that will be used to send requests to the node. It will
    /// send the request to the node and return a oneshot receiver that will be used to get the
    /// response back.
    async fn send_request(
        &self,
        request: UserRequest,
    ) -> Result<NodeResponse, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .node_sender
            .send(NodeNotification::FromUser(request, tx)); // Send the request to the node

        rx.await
    }

    /// Get the current [`UtreexoNodeConfig`] from the running node.
    pub async fn get_config(&self) -> Result<UtreexoNodeConfig, oneshot::error::RecvError> {
        let config = self.send_request(UserRequest::Config).await?;

        extract_variant!(Config, config);
    }

    /// Connects to a specified address and port.
    /// This function will return a boolean indicating whether the connection was successful. It
    /// may be called multiple times, and may use hostnames or IP addresses.
    pub async fn add_peer(
        &self,
        addr: IpAddr,
        port: u16,
        v2transport: bool,
    ) -> Result<bool, oneshot::error::RecvError> {
        let val = self
            .send_request(UserRequest::Add((addr, port, v2transport)))
            .await?;

        extract_variant!(Add, val);
    }

    /// Removes a peer from the node's peer list.
    /// This function will return a boolean indicating whether the peer was successfully removed.
    /// It may be called multiple times, and may use hostnames or IP addresses.
    pub async fn remove_peer(
        &self,
        addr: IpAddr,
        port: u16,
    ) -> Result<bool, oneshot::error::RecvError> {
        let val = self.send_request(UserRequest::Remove((addr, port))).await?;
        extract_variant!(Remove, val);
    }

    /// Immediately disconnect from a peer.
    ///
    /// Returns a bool indicating whether the disconnection was successful.
    pub async fn disconnect_peer(
        &self,
        addr: IpAddr,
        port: u16,
    ) -> Result<bool, oneshot::error::RecvError> {
        let val = self
            .send_request(UserRequest::Disconnect((addr, port)))
            .await?;

        extract_variant!(Disconnect, val);
    }

    /// Attempts to connect to a peer once.
    ///
    /// This function will try to connect to the peer once, but will not add it to the node's
    /// peer list. It will return a boolean indicating whether the connection was successful.
    /// It may be called multiple times, and may use hostnames or IP addresses.
    pub async fn onetry_peer(
        &self,
        addr: IpAddr,
        port: u16,
        v2transport: bool,
    ) -> Result<bool, oneshot::error::RecvError> {
        let val = self
            .send_request(UserRequest::Onetry((addr, port, v2transport)))
            .await?;
        extract_variant!(Onetry, val);
    }

    /// Gets a block by its hash.
    ///
    /// This function will try to get a block from the network and return it. Note that we don't
    /// keep a local copy of the blockchain, so this function will always make a network request.
    pub async fn get_block(
        &self,
        block: BlockHash,
    ) -> Result<Option<Block>, oneshot::error::RecvError> {
        let val = self.send_request(UserRequest::Block(block)).await?;

        extract_variant!(Block, val);
    }

    /// Gets a transaction from the mempool by its ID.
    ///
    /// This function will return a transaction from the mempool if it exists. If the transaction
    /// is not in the mempool (because it doesn't exist or because it's already been mined), this
    /// function will return `None`.
    pub async fn get_mempool_transaction(
        &self,
        txid: Txid,
    ) -> Result<Option<Transaction>, oneshot::error::RecvError> {
        let val = self
            .send_request(UserRequest::MempoolTransaction(txid))
            .await?;

        extract_variant!(MempoolTransaction, val);
    }

    /// Gets information about all connected peers.
    ///
    /// This function will return a list of `PeerInfo` structs, each of which contains information
    /// about a single peer.
    pub async fn get_peer_info(&self) -> Result<Vec<PeerInfo>, oneshot::error::RecvError> {
        let val = self.send_request(UserRequest::GetPeerInfo).await?;

        extract_variant!(GetPeerInfo, val);
    }

    /// Pings all connected peers to check if they are alive.
    pub async fn ping(&self) -> Result<bool, oneshot::error::RecvError> {
        let val = self.send_request(UserRequest::Ping).await?;

        extract_variant!(Ping, val)
    }
}

fn serialize_service_flags<S>(flags: &ServiceFlags, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&flags.to_string())
}

macro_rules! extract_variant {
    ($variant:ident, $var:ident) => {
        if let NodeResponse::$variant(val) = $var {
            return Ok(val);
        } else {
            panic!("Unexpected variant");
        }
    };
}

use extract_variant;
