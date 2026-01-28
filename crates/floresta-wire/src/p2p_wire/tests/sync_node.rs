#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bitcoin::Network;
    use floresta_chain::pruned_utreexo::BlockchainInterface;

    use crate::p2p_wire::tests::utils::invalid_block_h7;
    use crate::p2p_wire::tests::utils::setup_node;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;
    use crate::p2p_wire::tests::utils::PeerData;

    const NUM_BLOCKS: usize = 9;

    #[tokio::test]
    async fn test_sync_valid_blocks() {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let headers = signet_headers();
        let blocks = signet_blocks();

        let peer = vec![PeerData::new(Vec::new(), blocks, HashMap::new())];
        let chain = setup_node(peer, false, Network::Signet, &datadir, NUM_BLOCKS).await;

        assert_eq!(chain.get_validation_index().unwrap(), 9);
        assert_eq!(chain.get_best_block().unwrap().1, headers[9].block_hash());
        assert!(!chain.is_in_ibd());
    }

    #[tokio::test]
    async fn test_sync_invalid_block() {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let headers = signet_headers();

        let mut blocks = signet_blocks();
        // Replace the height 7 block with an invalid one
        blocks.insert(headers[7].block_hash(), invalid_block_h7());

        let peer = vec![PeerData::new(Vec::new(), blocks, HashMap::new())];
        let chain = setup_node(peer, false, Network::Signet, &datadir, NUM_BLOCKS).await;

        // Block at height 7 was invalidated when connecting it to the chain
        assert_eq!(chain.get_validation_index().unwrap(), 6);
        assert_eq!(chain.get_best_block().unwrap().1, headers[6].block_hash());
        assert!(!chain.is_in_ibd());
    }
}
