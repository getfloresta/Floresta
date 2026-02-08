#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bitcoin::BlockHash;
    use bitcoin::Network;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use floresta_common::acchashes;
    use floresta_common::prelude::HashMap;
    use rustreexo::accumulator::node_hash::BitcoinNodeHash;
    use rustreexo::accumulator::stump::Stump;

    use crate::p2p_wire::tests::utils::create_false_acc;
    use crate::p2p_wire::tests::utils::setup_sync_node;
    use crate::p2p_wire::tests::utils::signet_blocks;
    use crate::p2p_wire::tests::utils::signet_headers;
    use crate::p2p_wire::tests::utils::signet_roots;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;
    const STARTING_LIE_BLOCK_HEIGHT: usize = 30;

    pub const NUM_BLOCKS: usize = 120;

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
        let args = SetupNodeArgs::new(peers, true, Network::Signet, datadir, NUM_BLOCKS);

        let chain = setup_sync_node(args).await;
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
        let args = SetupNodeArgs::new(peers, true, Network::Signet, datadir, NUM_BLOCKS);

        let chain = setup_sync_node(args).await;
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
