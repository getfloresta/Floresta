// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use bitcoin::Network;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use tokio::time::timeout;

    use crate::node::sync_ctx::SyncNode;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;
    use crate::p2p_wire::tests::utils::mutate_block;
    use crate::p2p_wire::tests::utils::setup_node;
    use crate::p2p_wire::tests::utils::setup_sync_node;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;

    const NUM_BLOCKS: usize = 9;

    #[tokio::test]
    async fn test_sync_valid_blocks() {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let headers = signet_headers();
        let blocks = signet_blocks();

        let peer = vec![PeerData::new(Vec::new(), blocks, HashMap::new())];
        let args = SetupNodeArgs::new(peer, false, Network::Signet, datadir, NUM_BLOCKS);

        let chain = setup_sync_node(args).await;

        assert_eq!(chain.get_validation_index().unwrap(), 9);
        assert_eq!(chain.get_best_block().unwrap().1, headers[9].block_hash());
        assert!(!chain.is_in_ibd());
    }

    #[tokio::test]
    async fn test_sync_to_stale_tip_finishes_but_stays_in_ibd() {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let peer = vec![PeerData::new(Vec::new(), signet_blocks(), HashMap::new())];
        let args = SetupNodeArgs::new(peer, false, Network::Signet, datadir, NUM_BLOCKS);

        // The signet fixtures are years old, so a one day limit makes the tip stale.
        let mut node = setup_node::<SyncNode>(args);
        node.config.max_tip_age_secs = 24 * 60 * 60;
        let chain = node.chain.clone();

        // SyncNode can't receive new headers, so it must still hand off instead of waiting.
        timeout(Duration::from_secs(100), node.run(|_| {}))
            .await
            .expect("SyncNode should finish on a stale tip");

        assert_eq!(chain.get_validation_index().unwrap(), 9);
        assert!(chain.is_in_ibd());
    }

    #[tokio::test]
    async fn test_sync_mutated_block() {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let headers = signet_headers();

        let mut blocks = signet_blocks();
        // Replace the height 7 block with an invalid one
        mutate_block(blocks.get_mut(&headers[7].block_hash()).unwrap());

        // We will have 9 peers sending mutated blocks, only one with the original txdata
        let mut peers = vec![PeerData::new(Vec::new(), blocks, HashMap::new()); 9];
        peers.push(PeerData::new(Vec::new(), signet_blocks(), HashMap::new()));

        let args = SetupNodeArgs::new(peers, false, Network::Signet, datadir, NUM_BLOCKS);
        let chain = setup_sync_node(args).await;

        // We were able to find the original block and sync
        assert_eq!(chain.get_validation_index().unwrap(), 9);
        assert_eq!(chain.get_best_block().unwrap().1, headers[9].block_hash());
        assert!(!chain.is_in_ibd());
    }
}
