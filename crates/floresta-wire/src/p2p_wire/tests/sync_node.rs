// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bitcoin::Network;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use floresta_chain::pruned_utreexo::UpdatableChainstate;
    use rustreexo::stump::Stump;

    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;
    use crate::p2p_wire::tests::utils::mutate_block;
    use crate::p2p_wire::tests::utils::run_backfill;
    use crate::p2p_wire::tests::utils::setup_sync_node;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;

    const NUM_BLOCKS: usize = 9;

    /// Accumulator after syncing the pristine signet blocks with the `ChainState` backend,
    /// used as the expected result of the `PartialChainState` (backfill) path.
    async fn honest_acc() -> Stump {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let peer = vec![PeerData::new(Vec::new(), signet_blocks(), HashMap::new())];
        let args = SetupNodeArgs::new(peer, false, Network::Signet, datadir, NUM_BLOCKS);
        let chain = setup_sync_node(args).await;

        assert_eq!(chain.get_validation_index().unwrap(), 9);
        chain.get_acc()
    }

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

    /// Runs a backfill node, which syncs over a `PartialChainState`, and checks it reaches the
    /// same state as the `ChainState` path. If `mutated`, nine extra peers send a mutated block
    /// at height 7, and only one peer has the original txdata.
    async fn check_backfill(mutated: bool) {
        let datadir = format!("./tmp-db/{}.backfill", rand::random::<u32>());
        let honest_peer = PeerData::new(Vec::new(), signet_blocks(), HashMap::new());

        let mut peers = Vec::new();
        if mutated {
            let headers = signet_headers();
            let mut blocks = signet_blocks();
            mutate_block(blocks.get_mut(&headers[7].block_hash()).unwrap());

            peers = vec![PeerData::new(Vec::new(), blocks, HashMap::new()); 9];
        }
        peers.push(honest_peer);

        let args = SetupNodeArgs::new(peers, false, Network::Signet, datadir, NUM_BLOCKS);
        let outcome = run_backfill(args).await;

        assert_eq!(outcome.height, 9);
        assert!(!outcome.is_in_ibd);
        assert!(!outcome.has_invalid_blocks);

        // PartialChainState doesn't check the block structure, so a mutated block that reached
        // it would be connected with a wrong accumulator, without any error.
        assert_eq!(outcome.acc, honest_acc().await);
    }

    #[tokio::test]
    async fn test_backfill_valid_blocks() {
        check_backfill(false).await;
    }

    #[tokio::test]
    async fn test_backfill_mutated_block() {
        check_backfill(true).await;
    }
}
