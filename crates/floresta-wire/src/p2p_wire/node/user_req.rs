use std::time::Instant;

use bitcoin::p2p::ServiceFlags;
use bitcoin::Block;
use floresta_chain::ChainBackend;
use tokio::sync::oneshot;
use tracing::debug;
use tracing::info;
use tracing::warn;

use super::try_and_log;
use super::NodeRequest;
use super::UtreexoNode;
use crate::block_proof::Bitmap;
use crate::node::running_ctx::RunningNode;
use crate::node_context::NodeContext;
use crate::node_interface::NodeInterface;
use crate::node_interface::NodeResponse;
use crate::node_interface::UserRequest;
use crate::p2p_wire::error::WireError;

impl<T, Chain> UtreexoNode<Chain, T>
where
    T: 'static + Default + NodeContext,
    Chain: ChainBackend + 'static,
    WireError: From<Chain::Error>,
{
    /// Returns a handle to the node interface that we can use to request data from our
    /// node. This struct is thread safe, so we can use it from multiple threads and have
    /// multiple handles. It also doesn't require a mutable reference to the node, or any
    /// synchronization mechanism.
    pub fn get_handle(&self) -> NodeInterface {
        NodeInterface::new(self.common.node_tx.clone())
    }

    /// Handles getpeerinfo requests, returning a list of all connected peers and some useful
    /// information about it.
    fn handle_get_peer_info(&self, responder: oneshot::Sender<NodeResponse>) {
        let mut peers = Vec::new();
        for peer in self.peer_ids.iter() {
            peers.push(self.get_peer_info(peer));
        }

        let peers = peers.into_iter().flatten().collect();
        try_and_log!(responder.send(NodeResponse::GetPeerInfo(peers)));
    }

    /// Actually perform the user request
    ///
    /// These are requests made by some consumer of `floresta-wire` using the [`NodeInterface`], and may
    /// be a mempool transaction, a block, or a connection request.
    pub(crate) fn perform_user_request(
        &mut self,
        user_req: UserRequest,
        responder: oneshot::Sender<NodeResponse>,
    ) {
        if self.inflight.len() >= RunningNode::MAX_INFLIGHT_REQUESTS {
            return;
        }

        debug!("Performing user request {user_req:?}");

        let req = match user_req {
            UserRequest::Config => {
                let config = self.common.config.clone();
                let _ = responder.send(NodeResponse::Config(config));

                return;
            }
            UserRequest::Ping => {
                self.broadcast_to_peers(NodeRequest::Ping);
                try_and_log!(responder.send(NodeResponse::Ping(true)));

                return;
            }
            UserRequest::Block(block_hash) => NodeRequest::GetBlock(vec![block_hash]),
            UserRequest::UtreexoProof(block_hash) => {
                NodeRequest::GetBlockProof((block_hash, Bitmap::default(), Bitmap::default()))
            }
            UserRequest::MempoolTransaction(txid) => NodeRequest::MempoolTransaction(txid),
            UserRequest::GetPeerInfo => {
                self.handle_get_peer_info(responder);
                return;
            }
            UserRequest::Add((addr, port, v2transport)) => {
                let node_response = match self.handle_addnode_add_peer(addr, port, v2transport) {
                    Ok(_) => {
                        info!("Added peer {addr}:{port}");
                        NodeResponse::Add(true)
                    }
                    Err(err) => {
                        warn!("{err:?}");
                        NodeResponse::Add(false)
                    }
                };

                let _ = responder.send(node_response);
                return;
            }
            UserRequest::Remove((addr, port)) => {
                let node_response = match self.handle_addnode_remove_peer(addr, port) {
                    Ok(_) => {
                        info!("Removed peer {addr}:{port}");
                        NodeResponse::Remove(true)
                    }
                    Err(err) => {
                        warn!("{err:?}");
                        NodeResponse::Remove(false)
                    }
                };

                let _ = responder.send(node_response);
                return;
            }
            UserRequest::Onetry((addr, port, v2transport)) => {
                let node_response = match self.handle_addnode_onetry_peer(addr, port, v2transport) {
                    Ok(_) => {
                        info!("Connected to peer {addr}:{port}");
                        NodeResponse::Onetry(true)
                    }
                    Err(err) => {
                        warn!("{err:?}");
                        NodeResponse::Onetry(false)
                    }
                };

                let _ = responder.send(node_response);
                return;
            }
            UserRequest::Disconnect((addr, port)) => {
                let node_response = match self.handle_disconnect_peer(addr, port) {
                    Ok(_) => {
                        info!("Disconnected from peer {addr}:{port}");
                        NodeResponse::Disconnect(true)
                    }
                    Err(err) => {
                        warn!("Failed to disconnect from peer {addr}:{port}: {err:?}");
                        NodeResponse::Disconnect(false)
                    }
                };

                let _ = responder.send(node_response);
                return;
            }
        };

        let peer = self.send_to_fastest_peer(req, ServiceFlags::NONE);
        if let Ok(peer) = peer {
            self.inflight_user_requests
                .insert(user_req, (peer, Instant::now(), responder));
        }
    }

    /// Check if this block request is made by a user through the user interface and answer it
    /// back to the user if so.
    ///
    /// This function will return the given block if it isn't a user request. This is to avoid cloning
    /// the block.
    pub(crate) fn check_is_user_block_and_reply(
        &mut self,
        block: Block,
    ) -> Result<Option<Block>, WireError> {
        // If this block is a request made through the user interface, send it back to the
        // user.
        if let Some(request) = self
            .inflight_user_requests
            .remove(&UserRequest::Block(block.block_hash()))
        {
            debug!("answering user request for block {}", block.block_hash());
            request
                .2
                .send(NodeResponse::Block(Some(block)))
                .map_err(|_| WireError::ResponseSendError)?;

            return Ok(None);
        }

        Ok(Some(block))
    }
}
