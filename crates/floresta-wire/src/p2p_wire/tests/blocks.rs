// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use bitcoin::Block;
    use floresta_chain::ChainState;
    use floresta_chain::FlatChainStore;
    use tokio::sync::oneshot;
    use tokio::sync::oneshot::Receiver;

    use crate::node::InflightRequests;
    use crate::node::PeerStatus;
    use crate::node::UtreexoNode;
    use crate::node::sync_ctx::SyncNode;
    use crate::node_handle::NodeResponse;
    use crate::node_handle::UserRequest;
    use crate::p2p_wire::error::WireError;
    use crate::p2p_wire::tests::utils::Mutation;
    use crate::p2p_wire::tests::utils::PEER_TEST;
    use crate::p2p_wire::tests::utils::mutated_block_h7;
    use crate::p2p_wire::tests::utils::setup_unit_node;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;
    use crate::p2p_wire::tests::utils::synthetic_block;

    type TestNode = UtreexoNode<Arc<ChainState<FlatChainStore>>, SyncNode>;

    type TestSetup = (TestNode, Block, Option<Receiver<NodeResponse>>);

    fn setup_test(is_user_request: bool, is_mutated_block: bool) -> TestSetup {
        let mut node = setup_unit_node();
        let blocks = signet_blocks();
        let headers = signet_headers();

        let block = if is_mutated_block {
            mutated_block_h7()
        } else {
            let block_hash = headers[1].block_hash();
            blocks.get(&block_hash).unwrap().clone()
        };

        let block_hash = block.block_hash();

        // Pre-populate inflight
        node.inflight.insert(
            InflightRequests::Blocks(block_hash),
            (PEER_TEST, Instant::now()),
        );

        let mut response = None;
        if is_user_request {
            let (tx, rx) = oneshot::channel::<NodeResponse>();
            node.inflight_user_requests.insert(
                UserRequest::Block(block_hash),
                (PEER_TEST, Instant::now(), tx),
            );

            response = Some(rx);
        }

        (node, block, response)
    }

    #[tokio::test]
    async fn test_check_mutated_block_unmutated_block_is_ok() {
        let mut node: TestNode = setup_unit_node();
        let block = synthetic_block(Mutation::None);

        node.check_mutated_block(&block, PEER_TEST).unwrap();

        // The peer shouldn't be punished
        assert_eq!(node.peers.get(&PEER_TEST).unwrap().state, PeerStatus::Ready);
    }

    #[tokio::test]
    async fn test_check_mutated_block_mutated_block_bans_peer() {
        let mut node: TestNode = setup_unit_node();
        let block = synthetic_block(Mutation::MerkleRoot);
        assert!(!block.check_merkle_root());

        let result = node.check_mutated_block(&block, PEER_TEST).unwrap_err();

        assert!(matches!(result, WireError::PeerMisbehaving));
        assert_eq!(
            node.peers.get(&PEER_TEST).unwrap().state,
            PeerStatus::Banned
        );
    }

    #[tokio::test]
    async fn test_check_mutated_block_bad_witness_commitment_bans_peer() {
        let mut node: TestNode = setup_unit_node();
        let block = synthetic_block(Mutation::WitnessCommitment);

        // The merkle root matches, so only the witness commitment is invalid
        assert!(block.check_merkle_root());
        assert!(!block.check_witness_commitment());

        let result = node.check_mutated_block(&block, PEER_TEST).unwrap_err();

        assert!(matches!(result, WireError::PeerMisbehaving));
        assert_eq!(
            node.peers.get(&PEER_TEST).unwrap().state,
            PeerStatus::Banned
        );
    }

    #[tokio::test]
    async fn test_block_proof_valid_block_stores_and_requests_proof() {
        let (mut node, block, _) = setup_test(false, false);
        let block_hash = block.block_hash();

        node.request_block_proof(block, PEER_TEST).unwrap();

        // The block should be stored
        assert!(node.blocks.contains_key(&block_hash));
        // The Blocks inflight entry should have been removed
        assert!(
            !node
                .inflight
                .contains_key(&InflightRequests::Blocks(block_hash))
        );

        // Coinbase-only: no proof needed, aux_data is ready
        let inflight_block = node.blocks.get(&block_hash).unwrap();
        assert!(inflight_block.aux_data.is_some());
        assert!(
            !node
                .inflight
                .contains_key(&InflightRequests::UtreexoProof(block_hash))
        );
    }

    #[tokio::test]
    async fn test_block_proof_valid_block_user_request_replies_to_user() {
        let (mut node, block, response) = setup_test(true, false);
        let block_hash = block.block_hash();

        node.request_block_proof(block, PEER_TEST).unwrap();

        // The user should have received the block
        let response = response.unwrap().await.unwrap();
        match response {
            NodeResponse::Block(Some(b)) => assert_eq!(b.block_hash(), block_hash),
            _ => panic!("expected NodeResponse::Block(Some(_))"),
        }

        // The block should NOT be stored in self.blocks (consumed by the user reply)
        assert!(!node.blocks.contains_key(&block_hash));

        // The user request should have been removed
        assert!(
            !node
                .inflight_user_requests
                .contains_key(&UserRequest::Block(block_hash))
        );

        // The proof request should NOT happen.
        assert!(
            !node
                .inflight
                .contains_key(&InflightRequests::UtreexoProof(block_hash))
        );
    }

    #[tokio::test]
    async fn test_block_proof_mutated_block_not_user_request_bans_peer() {
        let (mut node, mutated_block, _) = setup_test(false, true);
        let block_hash = mutated_block.block_hash();

        let result = node.request_block_proof(mutated_block, PEER_TEST);

        assert!(matches!(result, Err(WireError::PeerMisbehaving)));

        // Block should NOT be stored
        assert!(!node.blocks.contains_key(&block_hash));
        // Peer should be banned
        assert_eq!(
            node.peers.get(&PEER_TEST).unwrap().state,
            PeerStatus::Banned
        );
        // The proof request should NOT happen.
        assert!(
            !node
                .inflight
                .contains_key(&InflightRequests::UtreexoProof(block_hash))
        );
    }

    #[tokio::test]
    async fn test_block_proof_mutated_block_user_request_retries() {
        let (mut node, mutated_block, _) = setup_test(true, true);
        let block_hash = mutated_block.block_hash();

        node.request_block_proof(mutated_block, PEER_TEST).unwrap();

        assert_eq!(
            node.peers.get(&PEER_TEST).unwrap().state,
            PeerStatus::Banned
        );

        // A new Blocks inflight entry should exist (the retry)
        assert!(
            node.inflight
                .contains_key(&InflightRequests::Blocks(block_hash))
        );

        // The user request should still be open (not removed)
        assert!(
            node.inflight_user_requests
                .contains_key(&UserRequest::Block(block_hash))
        );

        // The proof request should NOT happen.
        assert!(
            !node
                .inflight
                .contains_key(&InflightRequests::UtreexoProof(block_hash))
        );
    }

    #[tokio::test]
    async fn test_check_and_retry_user_request_unmutated_block_returns_false() {
        let mut node: TestNode = setup_unit_node();
        let block = synthetic_block(Mutation::None);
        let block_hash = block.block_hash();

        let is_mutated = node
            .check_mutated_block_and_retry_user_request(&block, PEER_TEST)
            .unwrap();

        assert!(!is_mutated);

        // Nothing should change: no inflight entry and no ban
        assert!(
            !node
                .inflight
                .contains_key(&InflightRequests::Blocks(block_hash))
        );
        assert_eq!(node.peers.get(&PEER_TEST).unwrap().state, PeerStatus::Ready);
    }

    #[tokio::test]
    async fn test_check_and_retry_user_request_mutated_user_request_retries() {
        let mut node: TestNode = setup_unit_node();
        let block = synthetic_block(Mutation::MerkleRoot);
        let block_hash = block.block_hash();

        let (tx, _rx) = oneshot::channel::<NodeResponse>();
        node.inflight_user_requests.insert(
            UserRequest::Block(block_hash),
            (PEER_TEST, Instant::now(), tx),
        );

        let is_mutated = node
            .check_mutated_block_and_retry_user_request(&block, PEER_TEST)
            .unwrap();

        assert!(is_mutated);

        // The peer is banned and the block is retried elsewhere
        assert_eq!(
            node.peers.get(&PEER_TEST).unwrap().state,
            PeerStatus::Banned
        );
        assert!(
            node.inflight
                .contains_key(&InflightRequests::Blocks(block_hash))
        );

        // The user request stays open
        assert!(
            node.inflight_user_requests
                .contains_key(&UserRequest::Block(block_hash))
        );
    }

    #[tokio::test]
    async fn test_check_and_retry_user_request_mutated_no_user_request_errors() {
        let mut node: TestNode = setup_unit_node();
        let block = synthetic_block(Mutation::MerkleRoot);
        let block_hash = block.block_hash();

        let result = node
            .check_mutated_block_and_retry_user_request(&block, PEER_TEST)
            .unwrap_err();

        assert!(matches!(result, WireError::PeerMisbehaving));
        assert_eq!(
            node.peers.get(&PEER_TEST).unwrap().state,
            PeerStatus::Banned
        );

        // No retry should happen
        assert!(
            !node
                .inflight
                .contains_key(&InflightRequests::Blocks(block_hash))
        );
    }
}
