//! This module holds all RPC server side methods for interacting with our node's network stack.

use std::net::IpAddr;
use std::net::SocketAddr;

use bitcoin::Network;
use floresta_wire::node_interface::PeerInfo;
use serde_json::json;
use serde_json::Value;

use super::res::jsonrpc_interface::JsonRpcError;
use super::server::RpcChain;
use super::server::RpcImpl;

type Result<T> = std::result::Result<T, JsonRpcError>;

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    pub(crate) async fn ping(&self) -> Result<bool> {
        self.node
            .ping()
            .await
            .map_err(|e| JsonRpcError::Node(e.to_string()))
    }

    pub(crate) async fn add_node(
        &self,
        node_address: String,
        command: String,
        v2transport: bool,
    ) -> Result<Value> {
        // Try to parse both IP address and port.
        let (addr, port) = if let Ok(socket_addr) = node_address.parse::<SocketAddr>() {
            (socket_addr.ip(), socket_addr.port())
        // Try to parse the IP address only, and append the default P2P port for the network.
        } else {
            let ip = node_address
                .parse::<IpAddr>()
                .map_err(|_| JsonRpcError::InvalidAddress)?;

            // TODO: use `NetworkExt` to append the correct port once
            // https://github.com/rust-bitcoin/rust-bitcoin/pull/4639 makes it into a release.
            let default_port = match self.network {
                Network::Bitcoin => 8333,
                Network::Signet => 38333,
                Network::Testnet => 18333,
                Network::Testnet4 => 48333,
                Network::Regtest => 18444,
            };

            (ip, default_port)
        };

        let _ = match command.as_str() {
            "add" => self.node.add_peer(addr, port, v2transport).await,
            "remove" => self.node.remove_peer(addr, port).await,
            "onetry" => self.node.onetry_peer(addr, port, v2transport).await,
            _ => return Err(JsonRpcError::InvalidAddnodeCommand),
        };

        Ok(json!(null))
    }

    pub(crate) async fn disconnect_node(
        &self,
        node_address: String,
        node_id: Option<u32>,
    ) -> Result<Value> {
        let (peer_addr, peer_port) = match (node_address.is_empty(), node_id) {
            // Reference the peer by it's IP address and port.
            (false, None) => {
                // Try to parse `node_address` into a `SocketAddr`.
                // This will handle IPv4:port and IPv6:port.
                let socket_addr = node_address
                    .parse::<SocketAddr>()
                    .map_err(|_| JsonRpcError::InvalidAddress)?;

                (socket_addr.ip(), socket_addr.port())
            }
            // Reference the peer by it's ID.
            (true, Some(node_id)) => {
                let peer_info = self
                    .node
                    .get_peer_info()
                    .await
                    .map_err(|e| JsonRpcError::Node(e.to_string()))?;

                let peer = peer_info
                    .iter()
                    .find(|peer| peer.id == node_id)
                    .ok_or(JsonRpcError::PeerNotFound)?;

                (peer.address.ip(), peer.address.port())
            }
            // Both address and ID were provided, or neither was provided.
            _ => {
                return Err(JsonRpcError::InvalidDisconnectNodeCommand);
            }
        };

        let disconnected = self
            .node
            .disconnect_peer(peer_addr, peer_port)
            .await
            .map_err(|e| JsonRpcError::Node(e.to_string()))?;

        if !disconnected {
            return Err(JsonRpcError::PeerNotFound);
        }

        Ok(json!(null))
    }

    pub(crate) async fn get_peer_info(&self) -> Result<Vec<PeerInfo>> {
        self.node
            .get_peer_info()
            .await
            .map_err(|_| JsonRpcError::Node("Failed to get peer information".to_string()))
    }
}
