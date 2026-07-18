// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bitcoin::Network;
    use floresta_chain::pruned_utreexo::BlockchainInterface;

    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::mutated_block_h7;
    use crate::p2p_wire::tests::utils::setup_node;
    use crate::p2p_wire::tests::utils::setup_node_with_tip_age;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;

    const NUM_BLOCKS: usize = 9;
    /// `max_tip_age_secs = 0`: every tip timestamp predates "now", so the guard always fires.
    const TIP_AGE_ALWAYS_STALE: u32 = 0;
    /// `max_tip_age_secs = u32::MAX`: no timestamp can exceed this, so the guard never fires.
    const TIP_AGE_NEVER_STALE: u32 = u32::MAX;

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
    async fn test_sync_mutated_block() {
        let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
        let headers = signet_headers();

        let mut blocks = signet_blocks();
        // Replace the height 7 block with a mutated block
        blocks.insert(headers[7].block_hash(), mutated_block_h7());

        // We will have 9 peers sending mutated blocks, only one with the original txdata
        let mut peers = vec![PeerData::new(Vec::new(), blocks, HashMap::new()); 9];
        peers.push(PeerData::new(Vec::new(), signet_blocks(), HashMap::new()));

        let chain = setup_node(peers, false, Network::Signet, &datadir, NUM_BLOCKS).await;

        // We were able to find the original block and sync
        assert_eq!(chain.get_validation_index().unwrap(), 9);
        assert_eq!(chain.get_best_block().unwrap().1, headers[9].block_hash());
        assert!(!chain.is_in_ibd());
    }

    // When validation catches up to a stale tip the node
    // must stay in IBD rather than falsely reporting initialblockdownload=false.
    #[tokio::test]
    async fn test_ibd_stays_active_on_stale_tip() {
        let datadir = format!("./tmp-db/{}.sync_node_stale", rand::random::<u32>());
        let blocks = signet_blocks();

        // TIP_AGE_ALWAYS_STALE: every block timestamp is considered stale.
        let peer = vec![PeerData::new(Vec::new(), blocks, HashMap::new())];
        let chain = setup_node_with_tip_age(
            peer,
            false,
            Network::Signet,
            &datadir,
            NUM_BLOCKS,
            TIP_AGE_ALWAYS_STALE,
        )
        .await;

        // Confirm validation caught up fully -- only then does is_in_ibd() prove
        // the stale guard fired rather than the timeout simply expiring mid-sync.
        assert_eq!(chain.get_validation_index().unwrap(), NUM_BLOCKS as u32);
        assert_eq!(chain.get_best_block().unwrap().0, NUM_BLOCKS as u32);
        assert!(chain.is_in_ibd());
    }

    #[tokio::test]
    async fn test_ibd_completes_on_fresh_tip() {
        let datadir = format!("./tmp-db/{}.sync_node_fresh", rand::random::<u32>());
        let headers = signet_headers();
        let blocks = signet_blocks();

        // TIP_AGE_NEVER_STALE: no tip is ever considered stale.
        let peer = vec![PeerData::new(Vec::new(), blocks, HashMap::new())];
        let chain = setup_node_with_tip_age(
            peer,
            false,
            Network::Signet,
            &datadir,
            NUM_BLOCKS,
            TIP_AGE_NEVER_STALE,
        )
        .await;

        assert_eq!(chain.get_validation_index().unwrap(), NUM_BLOCKS as u32);
        assert_eq!(
            chain.get_best_block().unwrap().1,
            headers[NUM_BLOCKS].block_hash()
        );
        assert!(!chain.is_in_ibd());
    }
}
