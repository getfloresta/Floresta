// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests for blocks requested by users through the node handle.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;

    use bitcoin::Network;
    use floresta_chain::ChainState;
    use floresta_chain::FlatChainStore;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::sync::oneshot;

    use crate::node::NodeRequest;
    use crate::node::PeerStatus;
    use crate::node::UtreexoNode;
    use crate::node::running_ctx::RunningNode;
    use crate::node_handle::NodeResponse;
    use crate::node_handle::UserRequest;
    use crate::p2p_wire::error::WireError;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;
    use crate::p2p_wire::tests::utils::mutate_block;
    use crate::p2p_wire::tests::utils::setup_node;
    use crate::p2p_wire::tests::utils::signet_blocks;

    const NUM_BLOCKS: usize = 9;

    fn user_request_node() -> UtreexoNode<Arc<ChainState<FlatChainStore>>, RunningNode> {
        let peer = PeerData::new(Vec::new(), signet_blocks(), HashMap::new());
        let args = SetupNodeArgs::new(
            vec![peer],
            false,
            Network::Signet,
            format!("./tmp-db/{}.user_request", rand::random::<u32>()),
            NUM_BLOCKS,
        );
        setup_node::<RunningNode>(args)
    }

    #[tokio::test]
    // A requested block whose txdata matches the header is delivered to the user.
    async fn test_user_block_reply() {
        let mut node = user_request_node();
        let hash = node.chain.get_block_hash(1).unwrap();
        let block = signet_blocks().remove(&hash).unwrap();

        let (sender, mut user_reply) = oneshot::channel();
        node.inflight_user_requests
            .insert(UserRequest::Block(hash), (0, Instant::now(), sender));

        let unhandled = node.check_is_user_block_and_reply(block.clone()).unwrap();
        assert!(unhandled.is_none(), "user blocks are consumed by the reply");
        assert!(matches!(
            user_reply.try_recv().unwrap(),
            NodeResponse::Block(Some(received)) if received == block
        ));
        assert_eq!(node.peers[&0].state, PeerStatus::Ready);
    }

    #[tokio::test]
    // A block with the right header but tampered txdata bans the peer and is never
    // delivered to the user.
    async fn test_user_block_mutated() {
        let mut node = user_request_node();
        let (sender, mut messages) = unbounded_channel();
        node.peers.get_mut(&0).unwrap().channel = sender;

        let hash = node.chain.get_block_hash(1).unwrap();
        let mut block = signet_blocks().remove(&hash).unwrap();
        mutate_block(&mut block);
        assert_eq!(block.block_hash(), hash, "header hash survives tampering");

        let (sender, mut user_reply) = oneshot::channel();
        node.inflight_user_requests
            .insert(UserRequest::Block(hash), (0, Instant::now(), sender));

        let err = node.check_is_user_block_and_reply(block).unwrap_err();
        assert!(matches!(err, WireError::PeerMisbehaving));
        assert_eq!(node.peers[&0].state, PeerStatus::Banned);
        assert!(matches!(
            messages.try_recv().unwrap(),
            NodeRequest::Shutdown
        ));
        assert_eq!(
            user_reply.try_recv().unwrap_err(),
            oneshot::error::TryRecvError::Closed
        );
    }

    #[tokio::test]
    // Blocks nobody asked for through the handle are returned untouched for chain handling.
    async fn test_non_user_block_passthrough() {
        let mut node = user_request_node();
        let hash = node.chain.get_block_hash(1).unwrap();
        let block = signet_blocks().remove(&hash).unwrap();

        let unhandled = node.check_is_user_block_and_reply(block.clone()).unwrap();
        assert_eq!(unhandled, Some(block));
    }
}
