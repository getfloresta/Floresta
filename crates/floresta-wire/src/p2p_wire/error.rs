use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::{self};
use std::io;
use std::net::IpAddr;

use floresta_chain::BlockchainError;
use floresta_common::impl_error_from;
use tokio::sync::mpsc::error::SendError;

use super::peer::PeerError;
use super::transport::TransportError;
use crate::node::NodeRequest;

#[derive(Debug)]
pub enum WireError {
    /// Blockchain-related error.
    ///
    /// This error kind is returned by our `ChainState`.
    Blockchain(BlockchainError),

    /// Error while writing into a channel
    ChannelSend(SendError<NodeRequest>),

    /// Peer error
    PeerError(PeerError),

    /// Coinbase isn't mature
    CoinbaseNotMatured,

    /// Peer not found in our current connections
    PeerNotFound,

    /// We don't have any peers that could fulfill such request.
    NoPeersAvailable,

    /// Our peer is misbehaving
    PeerMisbehaving,

    /// Failed to init Utreexo peers: anchors.json does not exist yet
    AnchorFileNotFound,

    /// Peer already exists in our peers list
    PeerAlreadyExists(IpAddr, u16),

    /// Peer not found with this given address and port, in our peer list
    PeerNotFoundAtAddress(IpAddr, u16),

    /// Generic io error
    Io(std::io::Error),

    /// JSON (de)serialization error
    Serde(serde_json::Error),

    /// Failed to save Utreexo peers: no peers to save to anchors.json
    NoUtreexoPeersAvailable,

    /// We couldn't find a peer to send a request
    NoPeerToSendRequest,

    /// Peer timed out some request
    PeerTimeout,

    /// Poisoned lock
    PoisonedLock,

    /// We couldn't parse the provided address
    InvalidAddress(AddrParseError),

    /// Transport error
    Transport(TransportError),

    /// Can't send back response for user request
    ResponseSendError,

    /// No addresses available to connect to
    NoAddressesAvailable,

    /// We tried to work on a block we don't have. This is a bug!
    BlockNotFound,

    /// We tried to work on a block that we don't have a proof for yet. This is a bug!
    BlockProofNotFound,

    /// Couldn't find the leaf data for a block
    LeafDataNotFound,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Blockchain(err) => write!(f, "Blockchain error: {err:?}"),
            WireError::ChannelSend(err) => write!(f, "Error while writing into channel: {err:?}"),
            WireError::PeerError(err) => write!(f, "Peer error: {err:?}"),
            WireError::CoinbaseNotMatured => write!(f, "Coinbase isn't mature yet"),
            WireError::PeerNotFound => write!(f, "Peer not found in our current connections list"),
            WireError::NoPeersAvailable => write!(f, "We don't have peers to send a given request"),
            WireError::PeerMisbehaving => write!(f, "Our peer is misbehaving"),
            WireError::AnchorFileNotFound => write!(
                f,
                "Failed to init Utreexo peers: anchors.json does not exist yet"
            ),
            WireError::PeerAlreadyExists(ip, port) => write!(f, "Peer {ip}:{port} already exists"),
            WireError::PeerNotFoundAtAddress(ip, port) => write!(f, "Peer {ip}:{port} not found"),
            WireError::Io(err) => write!(f, "Generic IO error: {err:?}"),
            WireError::Serde(err) => write!(f, "Serde error: {err:?}"),
            WireError::NoUtreexoPeersAvailable => write!(
                f,
                "Failed to save Utreexo peers: no peers to save to anchors.json"
            ),
            WireError::NoPeerToSendRequest => {
                write!(f, "We couldn't find a peer to send the request")
            }
            WireError::PeerTimeout => write!(f, "Peer timed out"),
            WireError::PoisonedLock => write!(f, "Poisoned lock"),
            WireError::InvalidAddress(err) => {
                write!(f, "We couldn't parse the provided address due to: {err:?}")
            }
            WireError::Transport(err) => write!(f, "Transport error: {err:?}"),
            WireError::ResponseSendError => write!(f, "Can't send back response for user request"),
            WireError::NoAddressesAvailable => write!(f, "No addresses available to connect to"),
            WireError::BlockNotFound => write!(f, "We tried to work on a block we don't have"),
            WireError::BlockProofNotFound => write!(
                f,
                "We tried to work on a block that we don't have a proof for yet"
            ),
            WireError::LeafDataNotFound => write!(f, "Couldn't find the leaf data for a block"),
        }
    }
}

impl_error_from!(WireError, PeerError, PeerError);
impl_error_from!(WireError, BlockchainError, Blockchain);
impl_error_from!(WireError, AddrParseError, InvalidAddress);
impl_error_from!(WireError, SendError<NodeRequest>, ChannelSend);
impl_error_from!(WireError, serde_json::Error, Serde);
impl_error_from!(WireError, io::Error, Io);

impl From<tokio::sync::oneshot::error::RecvError> for WireError {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        WireError::ResponseSendError
    }
}

impl From<TransportError> for WireError {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::Io(io) => WireError::Io(io),
            other => WireError::Transport(other),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AddrParseError {
    InvalidIpv6,
    InvalidIpv4,
    InvalidHostname,
    InvalidPort,
    Inconclusive,
}

impl Display for AddrParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            AddrParseError::InvalidIpv6 => write!(f, "Invalid ipv6"),
            AddrParseError::InvalidIpv4 => write!(f, "Invalid ipv4"),
            AddrParseError::InvalidHostname => write!(f, "Invalid hostname"),
            AddrParseError::InvalidPort => write!(f, "Invalid port"),
            AddrParseError::Inconclusive => write!(f, "Inconclusive"),
        }
    }
}
