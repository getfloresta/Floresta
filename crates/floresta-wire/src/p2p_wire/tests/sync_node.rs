// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bitcoin::Network;
    use floresta_chain::pruned_utreexo::BlockchainInterface;

    use crate::p2p_wire::tests::utils::mutate_block;
    use crate::p2p_wire::tests::utils::setup_sync_node;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;

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
    async fn test_sync_mutated_block() {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let headers = signet_headers();

        let mut blocks = signet_blocks();
        // Replace the height 7 block with an invalid one
        if let Some(block) = blocks.get_mut(&headers[7].block_hash()) {
            mutate_block(block);
        }

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
