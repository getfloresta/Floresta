// SPDX-License-Identifier: MIT OR Apache-2.0

//! This module holds all RPC server side methods for interacting with our node's network stack.

use floresta_wire::bitcoin_socket_addr::BitcoinSocketAddr;
use floresta_wire::bitcoin_socket_addr::SystemResolver;
use floresta_wire::node_interface::PeerInfo;
use serde_json::json;
use serde_json::Value;

use super::res::JsonRpcError;
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
        address: String,
        command: String,
        v2transport: bool,
    ) -> Result<Value> {
        let address =
            BitcoinSocketAddr::parse_address(&address, Some(self.network), SystemResolver)?;

        let _ = match command.as_str() {
            "add" => self.node.add_peer(address, v2transport).await,
            "remove" => self.node.remove_peer(address).await,
            "onetry" => self.node.onetry_peer(address, v2transport).await,
            _ => return Err(JsonRpcError::InvalidAddnodeCommand),
        };

        Ok(json!(null))
    }

    pub(crate) async fn disconnect_node(
        &self,
        node_address: String,
        node_id: Option<u32>,
    ) -> Result<Value> {
        let peer_addr = match (node_address.is_empty(), node_id) {
            // Reference the peer by it's IP address and port.
            (false, None) => {
                BitcoinSocketAddr::parse_address(&node_address, Some(self.network), SystemResolver)?
            }

            // Reference the peer by it's ID.
            (true, Some(node_id)) => {
                let peer_info = self
                    .node
                    .get_peer_info()
                    .await
                    .map_err(|e| JsonRpcError::Node(e.to_string()))?;

                let peer = peer_info
                    .into_iter()
                    .find(|peer| peer.id == node_id)
                    .ok_or(JsonRpcError::PeerNotFound)?;

                peer.address
            }

            // Both address and ID were provided, or neither was provided.
            _ => {
                return Err(JsonRpcError::InvalidDisconnectNodeCommand);
            }
        };

        let disconnected = self
            .node
            .disconnect_peer(peer_addr)
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

    pub(crate) async fn get_connection_count(&self) -> Result<usize> {
        self.node
            .get_connection_count()
            .await
            .map_err(|_| JsonRpcError::Node("Failed to get connection count".to_string()))
    }
}
