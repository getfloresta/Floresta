use core::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io;

use bip324::serde::deserialize as deserialize_v2;
use bip324::serde::serialize as serialize_v2;
use bip324::serde::CommandString;
use bip324::AsyncProtocol;
use bip324::AsyncProtocolReader;
use bip324::AsyncProtocolWriter;
use bip324::ProtocolError;
use bip324::ProtocolFailureSuggestion;
use bip324::Role;
use bitcoin::consensus::deserialize;
use bitcoin::consensus::deserialize_partial;
use bitcoin::consensus::encode;
use bitcoin::consensus::serialize;
use bitcoin::consensus::Decodable;
use bitcoin::hashes::sha256;
use bitcoin::hashes::Hash;
use bitcoin::hashes::HashEngine;
use bitcoin::hex::DisplayHex;
use bitcoin::p2p::address::AddrV2;
use bitcoin::p2p::message::NetworkMessage;
use bitcoin::p2p::message::RawNetworkMessage;
use bitcoin::p2p::message::MAX_MSG_SIZE;
use bitcoin::p2p::Magic;
use bitcoin::Network;
use floresta_common::impl_error_from;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::ReadHalf;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;
use tracing::debug;
use tracing::info;

use super::socks::Socks5Addr;
use super::socks::Socks5Error;
use super::socks::Socks5StreamBuilder;
use crate::address_man::LocalAddress;

type TcpReadTransport = ReadTransport<BufReader<ReadHalf<TcpStream>>>;
type TcpWriteTransport = WriteTransport<WriteHalf<TcpStream>>;
type TransportResult =
    Result<(TcpReadTransport, TcpWriteTransport, TransportProtocol), TransportError>;

#[derive(Copy, Clone, PartialEq, Eq)]
/// A wrapper type for a network checksum
///
/// This checksum accompanies every P2PV1 message to detect corruption.
/// Computed as the first 4 bytes of `SHA-265d(<msg_payload>)`.
pub struct P2PV1MessageChecksum([u8; 4]);

impl Display for P2PV1MessageChecksum {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.as_hex())
    }
}

impl Debug for P2PV1MessageChecksum {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl AsRef<[u8]> for P2PV1MessageChecksum {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl P2PV1MessageChecksum {
    pub fn from_payload(payload: &[u8]) -> Self {
        // Compute the `SHA-256d` digest of the message payload.
        let mut hasher = sha256::Hash::engine();
        hasher.input(payload);
        let digest = sha256::Hash::from_engine(hasher);
        let digest_double = digest.hash_again().to_byte_array();

        // The checksum is the first 4 bytes of the digest.
        let mut checksum = [0; 4];
        checksum.copy_from_slice(&digest_double[0..4]);
        P2PV1MessageChecksum(checksum)
    }
}

#[derive(Debug)]
/// Enum that deals with transport errors
pub enum TransportError {
    /// I/O error
    Io(io::Error),

    /// V2 protocol error
    Protocol(ProtocolError),

    /// V2 serde error
    SerdeV2(bip324::serde::Error),

    /// V1 serde error
    SerdeV1(encode::Error),

    /// Proxy error
    Proxy(Socks5Error),

    /// Message is too big
    OversizedMessage {
        max_size: usize,
        message_size: usize,
    },

    /// Peer sent us a corrupted message
    BadChecksum {
        expected: P2PV1MessageChecksum,
        provided: P2PV1MessageChecksum,
    },

    /// Peer sent us a message with invalid magic bits
    BadMagicBits { expected: Magic, provided: Magic },
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(err) => write!(f, "IO error: {err:?}"),
            TransportError::Protocol(err) => write!(f, "V2 protocol error: {err:?}"),
            TransportError::SerdeV2(err) => write!(f, "V2 serde error: {err:?}"),
            TransportError::SerdeV1(err) => write!(f, "V1 serde error: {err:?}"),
            TransportError::Proxy(err) => write!(f, "Proxy error: {err:?}"),
            TransportError::OversizedMessage { max_size, message_size } => write!(f, "Peer sent us an oversized message: size {message_size} is greater than the max of {max_size}"),
            TransportError::BadChecksum { expected, provided } => write!(f, "Peer sent us a corrupted message: expected {expected}, got {provided}"),
            TransportError::BadMagicBits { expected, provided } => {
                write!(f, "Peer sent us a message with invalid magic bits: expected {expected}, got {provided}")
            }
        }
    }
}

impl_error_from!(TransportError, io::Error, Io);
impl_error_from!(TransportError, ProtocolError, Protocol);
impl_error_from!(TransportError, bip324::serde::Error, SerdeV2);
impl_error_from!(TransportError, encode::Error, SerdeV1);
impl_error_from!(TransportError, Socks5Error, Proxy);

pub enum ReadTransport<R: AsyncRead + Unpin + Send> {
    V2(R, AsyncProtocolReader),
    V1(R, Network),
}

pub enum WriteTransport<W: AsyncWrite + Unpin + Send + Sync> {
    V2(W, AsyncProtocolWriter),
    V1(W, Network),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Bitcoin nodes can communicate using different transport layer protocols.
pub enum TransportProtocol {
    /// Encrypted V2 protocol defined in BIP-324.
    V2,

    /// Original unencrypted V1 protocol.
    V1,
}

struct V1MessageHeader {
    magic: Magic,
    command: [u8; 12],
    length: usize,
    checksum: P2PV1MessageChecksum,
}

impl Decodable for V1MessageHeader {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> std::result::Result<Self, bitcoin::consensus::encode::Error> {
        let magic = Magic::consensus_decode(reader)?;
        let command = <[u8; 12]>::consensus_decode(reader)?;
        let length = u32::consensus_decode(reader)? as usize;
        let checksum = <[u8; 4]>::consensus_decode(reader)?;

        Ok(Self {
            checksum: P2PV1MessageChecksum(checksum),
            command,
            length,
            magic,
        })
    }
}

impl Encodable for V1MessageHeader {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut size = 0;
        size += self.magic.consensus_encode(writer)?;
        size += self.command.consensus_encode(writer)?;
        size += (self.length as u32).consensus_encode(writer)?;
        size += self.checksum.0.consensus_encode(writer)?;

        Ok(size)
    }
}

/// Establishes a TCP connection and negotiates the bitcoin protocol.
///
/// This function tries to connect to the specified address and negotiate the bitcoin protocol
/// with the remote node. It first attempts to use the V2 protocol, and if that fails with a specific
/// error suggesting fallback to V1 protocol (and `allow_v1_fallback` is true), it will retry
/// the connection with the V1 protocol.
///
/// # Arguments
///
/// * `address` - The address of a target node
/// * `network` - The bitcoin network
/// * `allow_v1_fallback` - Whether to allow fallback to V1 protocol if V2 negotiation fails
///
/// # Returns
///
/// Returns a tuple of read and write transports that can be used to communicate with the node.
///
/// # Errors
///
/// Returns a `TransportError` if the connection cannot be established or protocol negotiation fails.
pub async fn connect<A: ToSocketAddrs>(
    address: A,
    network: Network,
    allow_v1_fallback: bool,
) -> TransportResult {
    match try_connection(&address, network, false).await {
        Ok(transport) => Ok(transport),
        Err(TransportError::Protocol(ProtocolError::Io(_, ProtocolFailureSuggestion::RetryV1)))
            if allow_v1_fallback =>
        {
            try_connection(&address, network, true).await
        }
        Err(e) => Err(e),
    }
}

async fn try_connection<A: ToSocketAddrs>(
    address: &A,
    network: Network,
    force_v1: bool,
) -> TransportResult {
    let tcp_stream = TcpStream::connect(address).await?;
    // Data is buffered until there is enough to send out
    // thus reducing the amount of packages going through
    // the network.
    tcp_stream.set_nodelay(false)?;

    let peer_addr = match tcp_stream.peer_addr() {
        Ok(addr) => addr.to_string(),
        Err(_) => String::from("unknown peer"),
    };
    let (reader, mut writer) = tokio::io::split(tcp_stream);
    let mut reader = BufReader::new(reader);

    match force_v1 {
        true => {
            debug!("Using V1 protocol for connection to {peer_addr}");
            Ok((
                ReadTransport::V1(reader, network),
                WriteTransport::V1(writer, network),
                TransportProtocol::V1,
            ))
        }
        false => match AsyncProtocol::new(
            network,
            Role::Initiator,
            None,
            None,
            &mut reader,
            &mut writer,
        )
        .await
        {
            Ok(protocol) => {
                debug!("Successfully established V2 protocol connection to {peer_addr}",);
                let (reader_protocol, writer_protocol) = protocol.into_split();
                Ok((
                    ReadTransport::V2(reader, reader_protocol),
                    WriteTransport::V2(writer, writer_protocol),
                    TransportProtocol::V2,
                ))
            }
            Err(e) => {
                debug!("Failed to establish V2 protocol connection to {peer_addr}: {e:?}",);
                Err(TransportError::Protocol(e))
            }
        },
    }
}

/// Establishes a connection through a SOCKS5 proxy and negotiates the bitcoin protocol.
///
/// This function connects to a SOCKS5 proxy, establishes a connection to the target address
/// through the proxy, and then negotiates the bitcoin protocol. Like `connect`, it first tries
/// the V2 protocol and can fall back to V1 if needed and allowed.
///
/// # Arguments
///
/// * `proxy_addr` - The address of the SOCKS5 proxy
/// * `address` - The target address to connect to through the proxy
/// * `port` - The port to connect to on the target
/// * `network` - The bitcoin network
/// * `allow_v1_fallback` - Whether to allow fallback to V1 protocol if V2 negotiation fails
///
/// # Returns
///
/// Returns a tuple of read and write transports that can be used to communicate with the node.
///
/// # Errors
///
/// Returns a `TransportError` if the proxy connection cannot be established, the connection
/// to the target fails, or protocol negotiation fails.
pub async fn connect_proxy<A: ToSocketAddrs>(
    proxy_addr: A,
    address: LocalAddress,
    network: Network,
    allow_v1_fallback: bool,
) -> TransportResult {
    let addr = match address.get_address() {
        AddrV2::Cjdns(addr) => Socks5Addr::Ipv6(addr),
        AddrV2::I2p(addr) => Socks5Addr::Domain(addr.into()),
        AddrV2::Ipv4(addr) => Socks5Addr::Ipv4(addr),
        AddrV2::Ipv6(addr) => Socks5Addr::Ipv6(addr),
        AddrV2::TorV2(addr) => Socks5Addr::Domain(addr.into()),
        AddrV2::TorV3(addr) => Socks5Addr::Domain(addr.into()),
        AddrV2::Unknown(_, _) => {
            return Err(TransportError::Proxy(Socks5Error::InvalidAddress));
        }
    };

    match try_proxy_connection(&proxy_addr, &addr, address.get_port(), network, false).await {
        Ok(transport) => Ok(transport),
        Err(TransportError::Protocol(ProtocolError::Io(_, ProtocolFailureSuggestion::RetryV1)))
            if allow_v1_fallback =>
        {
            try_proxy_connection(&proxy_addr, &addr, address.get_port(), network, true).await
        }
        Err(e) => Err(e),
    }
}

async fn try_proxy_connection<A: ToSocketAddrs>(
    proxy_addr: A,
    target_addr: &Socks5Addr,
    port: u16,
    network: Network,
    force_v1: bool,
) -> TransportResult {
    let proxy = TcpStream::connect(proxy_addr).await?;
    let stream = Socks5StreamBuilder::connect(proxy, target_addr, port).await?;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    match force_v1 {
        true => {
            info!("Using V1 protocol for proxy connection to {target_addr:?}",);
            Ok((
                ReadTransport::V1(reader, network),
                WriteTransport::V1(writer, network),
                TransportProtocol::V1,
            ))
        }
        false => {
            match AsyncProtocol::new(
                network,
                Role::Initiator,
                None,
                None,
                &mut reader,
                &mut writer,
            )
            .await
            {
                Ok(protocol) => {
                    info!(
                        "Successfully established V2 protocol proxy connection to {target_addr:?}",
                    );
                    let (reader_protocol, writer_protocol) = protocol.into_split();
                    Ok((
                        ReadTransport::V2(reader, reader_protocol),
                        WriteTransport::V2(writer, writer_protocol),
                        TransportProtocol::V2,
                    ))
                }
                Err(e) => {
                    debug!(
                        "Failed to establish V2 protocol proxy connection to {target_addr:?}: {e:?}",
                    );
                    Err(TransportError::Protocol(e))
                }
            }
        }
    }
}

impl<R> ReadTransport<R>
where
    R: AsyncRead + Unpin + Send,
{
    /// Read the next message from the transport.
    pub async fn read_message(&mut self) -> Result<NetworkMessage, TransportError> {
        match self {
            ReadTransport::V2(reader, protocol) => {
                let payload = protocol.read_and_decrypt(reader).await?;
                let contents = payload.contents();
                let msg = deserialize_v2(contents)?;

                Ok(msg)
            }
            ReadTransport::V1(reader, network) => {
                let mut data: Vec<u8> = vec![0; 24];
                reader.read_exact(&mut data).await?;

                let header: V1MessageHeader = deserialize_partial(&data)?.0;
                if header.length > MAX_MSG_SIZE {
                    return Err(TransportError::OversizedMessage {
                        max_size: MAX_MSG_SIZE,
                        message_size: header.length,
                    });
                }
                if header.magic != network.magic() {
                    return Err(TransportError::BadMagicBits {
                        provided: header.magic,
                        expected: network.magic(),
                    });
                }

                data.resize(24 + header.length as usize, 0);
                reader.read_exact(&mut data[24..]).await?;

                let checksum = P2PV1MessageChecksum::from_payload(&data[24..]);
                if header.checksum != checksum {
                    return Err(TransportError::BadChecksum {
                        expected: checksum,
                        provided: header.checksum,
                    });
                }

                let msg: RawNetworkMessage = deserialize(&data)?;
                Ok(msg.into_payload())
            }
        }
    }
}

fn sha256d_payload(payload: &[u8]) -> [u8; 32] {
    let mut sha = sha256::Hash::engine();
    sha.input(payload);
    let hash = sha256::Hash::from_engine(sha);
    hash.hash_again().to_byte_array()
}

impl<W> WriteTransport<W>
where
    W: AsyncWrite + Unpin + Send + Sync,
{
    /// Write a message to the transport.
    pub async fn write_message(&mut self, message: NetworkMessage) -> Result<(), TransportError> {
        match self {
            WriteTransport::V2(writer, protocol) => {
                let data = serialize_v2(message)?;
                protocol.encrypt_and_write(&data, writer).await?;
            }
            WriteTransport::V1(writer, network) => {
                if let NetworkMessage::Unknown { payload, command } = message {
                    let expected_cmd = CommandString::try_from_static("getuproof").unwrap();
                    assert_eq!(
                        command, expected_cmd,
                        "Only getuproof is supported as unknown message"
                    );

                    // FIXME: This little bit of ugliness is due to https://github.com/rust-bitcoin/rust-bitcoin/issues/4413
                    // Once that is solved upstream (or utreexo messages are added to
                    // rust-bitcoin), this can be removed.
                    let checksum = &sha256d_payload(&payload)[0..4];

                    let mut message_header = [0u8; 24];
                    message_header[0..4].copy_from_slice(&network.magic().to_bytes());
                    message_header[4..13].copy_from_slice("getuproof".as_bytes());
                    message_header[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
                    message_header[20..24].copy_from_slice(checksum);

                    writer.write_all(&message_header).await?;
                    writer.write_all(&payload).await?;
                    writer.flush().await?;
                    return Ok(());
                }

                let data = &mut RawNetworkMessage::new(network.magic(), message);
                let data = serialize(&data);
                writer.write_all(&data).await?;
                writer.flush().await?;
            }
        }
        Ok(())
    }

    /// Shutdown the transport.
    pub async fn shutdown(&mut self) -> Result<(), TransportError> {
        match self {
            WriteTransport::V2(writer, _) => {
                writer.shutdown().await?;
            }
            WriteTransport::V1(writer, _) => {
                writer.shutdown().await?;
            }
        }
        Ok(())
    }
}
