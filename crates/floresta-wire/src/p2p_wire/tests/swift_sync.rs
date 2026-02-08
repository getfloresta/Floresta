#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::Network;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use floresta_common::bhash;

    use crate::node::swift_sync_ctx::SwiftSync;
    use crate::p2p_wire::node_context::NodeContext;
    use crate::p2p_wire::tests::utils::mainnet_headers;
    use crate::p2p_wire::tests::utils::make_block_invalid;
    use crate::p2p_wire::tests::utils::setup_running_node;
    use crate::p2p_wire::tests::utils::setup_swiftsync;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;

    const NUM_BLOCKS: usize = 175;

    fn read_blocks_txt() -> HashMap<BlockHash, Block> {
        include_str!("../../../../floresta-chain/testdata/mainnet_blocks.txt")
            .lines()
            .skip(1)
            .map(|b| deserialize_hex(b).unwrap())
            .map(|b: Block| (b.block_hash(), b))
            .collect()
    }

    #[tokio::test]
    async fn test_swift_sync_valid_blocks() {
        let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
        std::fs::create_dir_all(&datadir).unwrap();
        // We need the hints in the datadir
        std::fs::copy(
            "./src/p2p_wire/tests/test_data/bitcoin.hints",
            format!("{datadir}/bitcoin.hints"),
        )
        .unwrap();

        let headers = mainnet_headers();
        let blocks = read_blocks_txt();
        assert_eq!(blocks.len(), NUM_BLOCKS);

        let peer = vec![PeerData::new(Vec::new(), blocks, HashMap::new())];
        let args = SetupNodeArgs::new(peer, false, Network::Bitcoin, datadir, NUM_BLOCKS);

        let chain = setup_swiftsync(args).await;

        assert_eq!(chain.get_validation_index().unwrap(), NUM_BLOCKS as u32);
        let best_block = chain.get_best_block().unwrap();
        let expected = (
            175,
            bhash!("00000000fd4afcc15f0fdda9b24be4c62068d8cf82fe6277730fd096712d9d08"),
        );

        assert_eq!(best_block.1, headers[NUM_BLOCKS].block_hash());
        assert_eq!(best_block, expected);
        assert!(!chain.is_in_ibd());
    }

    #[tokio::test]
    async fn test_swift_sync_invalid_block() {
        let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
        std::fs::create_dir_all(&datadir).unwrap();
        // We need the hints in the datadir
        std::fs::copy(
            "./src/p2p_wire/tests/test_data/bitcoin.hints",
            format!("{datadir}/bitcoin.hints"),
        )
        .unwrap();

        let headers = mainnet_headers();
        let mut blocks = read_blocks_txt();
        assert_eq!(blocks.len(), NUM_BLOCKS);

        // Replace the height 151 block with an invalid one
        if let Some(block) = blocks.get_mut(&headers[151].block_hash()) {
            make_block_invalid(block);
        }

        // The first peer to send the invalid block is banned, then we switch to `SyncNode`, we
        // advance the validation index up to block 150, and finally ban the peer sending us the
        // invalid block again.
        //
        // NOTE: we need `MAX_OUTGOING_PEERS` for `ChainSelector` to start and move to `SwiftSync`.
        let peers: Vec<_> = (0..SwiftSync::MAX_OUTGOING_PEERS)
            .map(|_| PeerData::new(Vec::new(), blocks.clone(), HashMap::new()))
            .collect();

        let args = SetupNodeArgs::new(peers, false, Network::Bitcoin, datadir, NUM_BLOCKS);

        // Running node ensures we switch from `SwiftSync` to `SyncNode`, as we can't verify the
        // SwiftSync hints since the chain is invalid.
        let chain = setup_running_node(args).await;

        // Block at height 151 was invalidated when connecting it to the chain
        assert_eq!(chain.get_validation_index().unwrap(), 150);
        assert_eq!(chain.get_best_block().unwrap().1, headers[150].block_hash());
        assert!(!chain.is_in_ibd());
    }
}
