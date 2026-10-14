// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use bitcoin::Network;
    use floresta_chain::AssumeValidArg;
    use floresta_chain::ChainState;
    use floresta_chain::FlatChainStore;
    use floresta_chain::FlatChainStoreConfig;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use floresta_mempool::Mempool;
    use tokio::sync::Mutex;
    use tokio::sync::RwLock;

    use crate::p2p_wire::UtreexoNodeConfig;
    use crate::p2p_wire::address_man::AddressMan;
    use crate::p2p_wire::node::UtreexoNode;
    use crate::p2p_wire::node::running_ctx::RunningNode;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;
    use crate::p2p_wire::tests::utils::mutate_block;
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

    #[tokio::test]
    async fn test_addnode_config() {
        let datadir = format!("./tmp-db/{}.addnode_config", rand::random::<u32>());
        let config = UtreexoNodeConfig {
            network: Network::Signet,
            datadir: datadir.clone().into(),
            add_node: vec!["127.0.0.1:38333".to_string()],
            ..Default::default()
        };

        let chainstore = FlatChainStore::new(FlatChainStoreConfig {
            block_index_size: Some(10),
            headers_file_size: Some(10),
            cache_size: Some(10),
            ..FlatChainStoreConfig::new(&datadir)
        })
        .unwrap();
        let chain = Arc::new(
            ChainState::open(chainstore, Network::Signet, AssumeValidArg::Disabled).unwrap(),
        );
        let mempool = Arc::new(Mutex::new(Mempool::new(1000)));
        let kill_signal = Arc::new(RwLock::new(false));
        let addr_man = AddressMan::new(None, &[]);
        let node: UtreexoNode<_, RunningNode> =
            UtreexoNode::new(config, chain, mempool, None, kill_signal, addr_man).unwrap();

        assert_eq!(node.added_peers.len(), 1);
        assert_eq!(node.added_peers[0].address.to_string(), "127.0.0.1:38333");
        let _ = std::fs::remove_dir_all(&datadir);
    }
}
