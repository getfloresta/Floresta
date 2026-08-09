// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bitcoin::BlockHash;
    use bitcoin::Network;
    use bitcoin::block::Header;
    use bitcoin::constants::genesis_block;
    use bitcoin::p2p::ServiceFlags;
    use floresta_chain::AssumeValidArg;
    use floresta_chain::ChainState;
    use floresta_chain::FlatChainStore;
    use floresta_chain::FlatChainStoreConfig;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use floresta_common::acchashes;
    use floresta_common::prelude::HashMap;
    use floresta_common::service_flags;
    use floresta_mempool::Mempool;
    use rustreexo::node_hash::BitcoinNodeHash;
    use rustreexo::stump::Stump;
    use tokio::sync::Mutex;
    use tokio::sync::RwLock;
    use tokio::sync::mpsc::unbounded_channel;

    use crate::address_man::AddressMan;
    use crate::error::WireError;
    use crate::node::PeerStatus;
    use crate::node::UtreexoNode;
    use crate::node::chain_selector_ctx::ChainSelector;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::create_false_acc;
    use crate::p2p_wire::tests::utils::create_peer;
    use crate::p2p_wire::tests::utils::get_node_config;
    use crate::p2p_wire::tests::utils::setup_node;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;
    use crate::p2p_wire::tests::utils::signet_roots;
    const STARTING_LIE_BLOCK_HEIGHT: usize = 30;

    pub const NUM_BLOCKS: usize = 120;

    /// Grinds the header's nonce until its PoW is valid for its own target
    fn grind_pow(mut header: Header) -> Header {
        while header.validate_pow(header.target()).is_err() {
            header.nonce += 1;
        }
        header
    }

    /// A regtest header with the given time, extending `prev_blockhash`, with valid PoW
    fn regtest_header(prev_blockhash: BlockHash, time: u32) -> Header {
        let genesis = genesis_block(Network::Regtest).header;
        grind_pow(Header {
            prev_blockhash,
            time,
            nonce: 0,
            ..genesis
        })
    }

    type TestChain = Arc<ChainState<FlatChainStore>>;
    type TestNode = UtreexoNode<TestChain, ChainSelector>;

    /// A regtest chain-selector node with a single simulated peer (id 0), so we can call
    /// `handle_headers` directly and inspect how the peer is treated afterwards
    fn setup_chain_selector_node(datadir: &str) -> (TestNode, TestChain) {
        let config = FlatChainStoreConfig::new(datadir);
        let chainstore = FlatChainStore::new(config).unwrap();
        let chain = Arc::new(
            ChainState::open(chainstore, Network::Regtest, AssumeValidArg::Disabled).unwrap(),
        );
        let mempool = Arc::new(Mutex::new(Mempool::new(1000)));
        let kill_signal = Arc::new(RwLock::new(false));

        let mut node = TestNode::new(
            get_node_config(datadir, Network::Regtest, false),
            chain.clone(),
            mempool,
            None,
            kill_signal,
            AddressMan::new(None, &[]),
        )
        .unwrap();

        let (sender, receiver) = unbounded_channel();
        let mut peer = create_peer(
            Vec::new(),
            HashMap::new(),
            HashMap::new(),
            node.node_tx.clone(),
            sender,
            receiver,
            0,
        );
        // The node needs at least one latency sample to pick this peer for requests
        peer.message_times.add(0.1);
        node.peers.insert(0, peer);
        node.peer_ids.push(0);

        // Register the peer services, so the node can pick it for requests
        for service in [
            service_flags::UTREEXO.into(),
            ServiceFlags::COMPACT_FILTERS,
            ServiceFlags::NETWORK,
        ] {
            node.peer_by_service.entry(service).or_default().push(0);
        }

        (node, chain)
    }

    #[tokio::test]
    async fn time_too_new_header_does_not_ban_peer() {
        let datadir = format!("./tmp-db/{}.chain_selector", rand::random::<u32>());
        let (mut node, chain) = setup_chain_selector_node(&datadir);

        let genesis = genesis_block(Network::Regtest).header;

        // Two valid headers, then one too far in the future. `handle_headers` checks the
        // header times against the node's own clock, so any timestamp far beyond any
        // realistic clock (here, mid 2094) triggers the rule
        let header1 = regtest_header(genesis.block_hash(), genesis.time + 600);
        let header2 = regtest_header(header1.block_hash(), genesis.time + 1200);
        let too_new = regtest_header(header2.block_hash(), 3_930_000_000);

        node.handle_headers(0, vec![header1, header2, too_new])
            .await
            .unwrap();

        // The valid prefix of the batch is accepted, and the peer is not punished: the
        // too-new header may become valid later, or our own clock may be at fault
        assert_eq!(
            chain.get_best_block().unwrap(),
            (2, header2.block_hash()),
            "should accept the valid headers before the too-new one"
        );
        assert_eq!(
            node.peers.get(&0).unwrap().state,
            PeerStatus::Ready,
            "a too-new header must not ban the peer"
        );
    }

    #[tokio::test]
    async fn time_too_old_header_bans_peer() {
        let datadir = format!("./tmp-db/{}.chain_selector", rand::random::<u32>());
        let (mut node, chain) = setup_chain_selector_node(&datadir);

        // A timestamp equal to the previous MTP (the genesis time here) is a permanent
        // consensus failure, so the peer serving it gets banned
        let genesis = genesis_block(Network::Regtest).header;
        let too_old = regtest_header(genesis.block_hash(), genesis.time);

        // Banning our only peer leaves no one to send the follow-up headers request to
        let result = node.handle_headers(0, vec![too_old]).await;
        assert!(matches!(result, Err(WireError::NoPeersAvailable)));

        assert_eq!(chain.get_best_block().unwrap().0, 0, "tip must not move");
        assert_eq!(
            node.peers.get(&0).unwrap().state,
            PeerStatus::Banned,
            "a too-old header must ban the peer"
        );
    }

    #[tokio::test]
    async fn two_peers_one_lying() {
        let datadir = format!("./tmp-db/{}.chain_selector", rand::random::<u32>());
        let headers = signet_headers();
        let blocks = signet_blocks();
        let true_accs = signet_roots();

        let mut false_accs = true_accs.clone();

        // We will invalidate headers in the range `STARTING_LIE_BLOCK_HEIGHT..NUM_BLOCKS`
        let invalid_accs_iter = headers
            .iter()
            .enumerate()
            .take(NUM_BLOCKS)
            .skip(STARTING_LIE_BLOCK_HEIGHT);

        for (i, header) in invalid_accs_iter {
            false_accs.insert(header.block_hash(), create_false_acc(i));
        }

        let peers = vec![
            PeerData::new(headers.clone(), blocks.clone(), true_accs),
            PeerData::new(headers.clone(), blocks, false_accs),
        ];

        let chain = setup_node(peers, true, Network::Signet, &datadir, NUM_BLOCKS).await;
        let best_block = chain.get_best_block().unwrap();
        assert_eq!(best_block.1, headers[NUM_BLOCKS].block_hash());

        // The data for this accumulator is taken from the signet
        // files. Leaves are the utxos in the set, but here it only has
        // coinbase transactions, thus the leaves and the `num_blocks`
        // are equal.
        let expected_acc = Stump {
            leaves: 120,
            roots: acchashes![
                "fbbff1a533f80135a0cb222859297792d5c9d1cec801a2793ac15184905e672c",
                "42554b3aab845bf18397188fc21f1f39cfc742f36bdb1aae70dd60a39c1fd9b9",
                "2782a7bd0f93d57efb8611c90d41a94d520bceded1fc6c0050b4133db24a15d0",
                "d86dbb6f4c3c258e6a83ae0f349cbee695b10b2b677a02f12e5aefac04d368c9"
            ]
            .to_vec(),
        };
        assert_eq!(chain.acc(), expected_acc);
    }

    #[tokio::test]
    async fn ten_peers_one_honest() {
        let datadir = format!("./tmp-db/{}.chain_selector", rand::random::<u32>());
        let headers = signet_headers();
        let blocks = signet_blocks();
        let true_accs = signet_roots();
        let mut false_accs_array: Vec<HashMap<BlockHash, Vec<u8>>> = Vec::new();

        for i in 0..9 {
            let mut false_accs = true_accs.clone();
            for (j, header) in headers.iter().enumerate().take(NUM_BLOCKS).skip(i * 2) {
                false_accs.insert(header.block_hash(), create_false_acc(j));
            }
            false_accs_array.push(false_accs);
        }

        let mut peers = Vec::new();
        for _ in 0..9 {
            let peer = PeerData::new(
                headers.clone(),
                blocks.clone(),
                false_accs_array.pop().unwrap(),
            );
            peers.push(peer);
        }

        peers.push(PeerData::new(headers.clone(), blocks, true_accs));

        let chain = setup_node(peers, true, Network::Signet, &datadir, NUM_BLOCKS).await;
        let best_block = chain.get_best_block().unwrap();
        assert_eq!(best_block.1, headers[NUM_BLOCKS].block_hash());

        // The data for this accumulator is taken from the signet
        // files. Leaves are the utxos in the set, but here it only has
        // coinbase transactions, thus the leaves and the `num_blocks`
        // are equal.
        let expected_acc = Stump {
            leaves: 120,
            roots: acchashes![
                "fbbff1a533f80135a0cb222859297792d5c9d1cec801a2793ac15184905e672c",
                "42554b3aab845bf18397188fc21f1f39cfc742f36bdb1aae70dd60a39c1fd9b9",
                "2782a7bd0f93d57efb8611c90d41a94d520bceded1fc6c0050b4133db24a15d0",
                "d86dbb6f4c3c258e6a83ae0f349cbee695b10b2b677a02f12e5aefac04d368c9"
            ]
            .to_vec(),
        };
        assert_eq!(chain.acc(), expected_acc);
    }
}
