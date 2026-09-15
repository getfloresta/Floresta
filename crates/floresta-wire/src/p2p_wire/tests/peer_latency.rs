// SPDX-License-Identifier: MIT OR Apache-2.0

//! Regression tests for shared peer latency and timeout handling.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use std::time::Instant;

    use bitcoin::Network;
    use bitcoin::bip158::BlockFilter;
    use floresta_chain::ChainState;
    use floresta_chain::FlatChainStore;
    use floresta_chain::pruned_utreexo::BlockchainInterface;

    use crate::node::InflightRequests;
    use crate::node::PeerStatus;
    use crate::node::UtreexoNode;
    use crate::node::running_ctx::RunningNode;
    use crate::node::sync_ctx::SyncNode;
    use crate::node_context::NodeContext;
    use crate::p2p_wire::peer::PeerMessages;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;
    use crate::p2p_wire::tests::utils::setup_node;
    use crate::p2p_wire::tests::utils::signet_blocks;

    const NUM_BLOCKS: usize = 9;

    /// Creates two simulated peers with initial latency samples of 1 ms.
    fn latency_node<T: 'static + Default + NodeContext>()
    -> UtreexoNode<Arc<ChainState<FlatChainStore>>, T> {
        let peer = PeerData::new(Vec::new(), signet_blocks(), HashMap::new());
        let args = SetupNodeArgs::new(
            vec![peer; 2],
            false,
            Network::Signet,
            format!("./tmp-db/{}.sync_latency", rand::random::<u32>()),
            NUM_BLOCKS,
        );
        let mut node = setup_node::<T>(args);
        // Set the initial latency and clear handshake requests ourselves because these
        // tests call node methods directly, without running the node's event loop.
        for peer in node.peers.values_mut() {
            peer.message_times.add(1.0);
        }
        node.inflight.clear();
        node
    }

    #[tokio::test]
    // Verifies that without `Ready` peers we can't retry requests, and we keep penalizing
    // the original peer we had requested stuff from. Our inflight requests don't change.
    async fn test_failed_retries() {
        let mut node = latency_node::<SyncNode>();
        let requests = [
            InflightRequests::Blocks(node.chain.get_block_hash(1).unwrap()),
            InflightRequests::Headers,
            InflightRequests::GetFilters,
            InflightRequests::UtreexoState(0),
        ];

        // No peer can accept a retry, regardless of the requested service
        for peer in node.peers.values_mut() {
            peer.state = PeerStatus::Awaiting;
        }

        let expired_time = Instant::now() - Duration::from_secs(SyncNode::REQUEST_TIMEOUT + 1);
        for req in &requests {
            node.inflight.insert(req.clone(), (0, expired_time));
        }

        // This inflight list must remain untouched since we can't retry requests
        let original_inflight = node.inflight.clone();
        let mut expected_lat = node.peers[&0].message_times.clone();
        assert_eq!(expected_lat.value().unwrap(), 1.0, "initial latency is 1ms");

        for check in 1..=2 {
            assert!(node.check_for_timeout().is_err());
            assert_eq!(node.inflight, original_inflight);

            // Still-expired requests are retried and penalized again on the next check
            for _ in 0..original_inflight.len() {
                expected_lat.add(SyncNode::REQUEST_TIMEOUT as f64 * 1_000.0);
            }
            assert_eq!(node.peers[&0].message_times.value(), expected_lat.value());
            assert_eq!(node.peers[&0].banscore as usize, check * requests.len());
        }
    }

    #[tokio::test]
    // Verifies that replies from the wrong peer or with a stale timestamp don't change
    // latency. A valid reply updates only the peer assigned to the request.
    async fn test_reply_latency() {
        let mut node = latency_node::<SyncNode>();
        let hash = node.chain.get_block_hash(1).unwrap();
        let block = signet_blocks().remove(&hash).unwrap();
        let requests = [
            (InflightRequests::Blocks(hash), PeerMessages::Block(block)),
            (InflightRequests::Headers, PeerMessages::Headers(Vec::new())),
            (
                InflightRequests::GetFilters,
                PeerMessages::BlockFilter((hash, BlockFilter::new(&[]))),
            ),
            (
                InflightRequests::UtreexoState(1),
                PeerMessages::UtreexoState(Vec::new()),
            ),
        ];

        let mut expected_lat = node.peers[&1].message_times.clone();
        assert_eq!(expected_lat.value().unwrap(), 1.0, "initial latency is 1ms");

        for (request, message) in requests {
            // Prepare a request to peer 1 and a valid reply timestamp 5 seconds later
            let sent_at = Instant::now();
            let read_at = sent_at + Duration::from_secs(5);
            node.inflight.insert(request, (1, sent_at));

            // Peer 0 wasn't assigned this request, so its reply must not count
            assert_eq!(node.register_message_time(&message, 0, read_at), None);

            // Peer 1 is correct, but a reply read before `sent_at` is a stale sample
            let stale_time = sent_at - Duration::from_millis(1);
            assert_eq!(node.register_message_time(&message, 1, stale_time), None);

            // Neither rejected reply should change either peer's latency
            assert_eq!(node.peers[&0].message_times.value(), Some(1.0));
            assert_eq!(node.peers[&1].message_times.value(), expected_lat.value());

            // The valid reply adds a 5000ms sample to peer 1, while peer 0 stays at 1ms
            node.register_message_time(&message, 1, read_at).unwrap();

            expected_lat.add(5_000.0);
            assert_eq!(node.peers[&1].message_times.value(), expected_lat.value());
            assert_eq!(node.peers[&0].message_times.value(), Some(1.0));
            node.inflight.clear();
        }
    }

    #[tokio::test]
    /// Verifies that a timed-out request can be retried with the same peer,
    /// and recording its reply latency preserves the earlier timeout penalty.
    async fn test_same_peer_retry() {
        let mut node = latency_node::<RunningNode>();
        let hash = node.chain.get_block_hash(1).unwrap();
        let request = InflightRequests::Blocks(hash);

        // Now we have only one `Ready` peer (i.e., peer 0)
        node.peers.get_mut(&1).unwrap().state = PeerStatus::Awaiting;

        let expired_time = Instant::now() - Duration::from_secs(RunningNode::REQUEST_TIMEOUT + 1);
        node.inflight.insert(request.clone(), (0, expired_time));

        let mut expected_lat = node.peers[&0].message_times.clone();
        assert_eq!(expected_lat.value().unwrap(), 1.0, "initial latency is 1ms");

        // Record a timeout sample before retrying the request
        node.check_for_timeout().unwrap();
        expected_lat.add(RunningNode::REQUEST_TIMEOUT as f64 * 1_000.0);

        let (peer, retried_at) = node.inflight[&request];
        assert_eq!(peer, 0);
        assert!(retried_at > expired_time);
        assert_eq!(node.peers[&0].message_times.value(), expected_lat.value());
        assert_eq!(node.peers[&0].banscore, 1);

        // The retry is fresh, so another timeout check should change nothing
        node.check_for_timeout().unwrap();
        assert_eq!(node.inflight[&request], (peer, retried_at));
        assert_eq!(node.peers[&0].banscore, 1);

        // Recording the reply latency must preserve the earlier timeout penalty
        let message = PeerMessages::Block(signet_blocks().remove(&hash).unwrap());
        node.register_message_time(&message, peer, retried_at + Duration::from_millis(1))
            .unwrap();

        expected_lat.add(1.0);
        assert_eq!(node.peers[&0].message_times.value(), expected_lat.value());
    }
}
