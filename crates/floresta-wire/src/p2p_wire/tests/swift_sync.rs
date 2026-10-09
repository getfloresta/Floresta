#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs::File;
    use std::time::Duration;

    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::Network;
    use bitcoin::ScriptBuf;
    use bitcoin::blockdata::constants::genesis_block;
    use bitcoin::consensus::encode::deserialize_hex;
    use floresta_chain::AssumeValidArg;
    use floresta_chain::pruned_utreexo::BlockchainInterface;
    use floresta_chain::pruned_utreexo::IBDState;
    use floresta_chain::pruned_utreexo::UpdatableChainstate;
    use floresta_common::bhash;
    use hintsfile::EliasFano;
    use hintsfile::HintsfileBuilder;
    use rustreexo::proof::Proof;
    use tokio::time::timeout;

    use crate::node::WitnessMode;
    use crate::node::running_ctx::RunningNode;
    use crate::node::swift_sync_ctx::SwiftSync;
    use crate::p2p_wire::tests::utils::PeerData;
    use crate::p2p_wire::tests::utils::SetupNodeArgs;
    use crate::p2p_wire::tests::utils::mainnet_headers;
    use crate::p2p_wire::tests::utils::mutate_block;
    use crate::p2p_wire::tests::utils::setup_node;
    use crate::p2p_wire::tests::utils::setup_node_with_assume_valid;
    use crate::p2p_wire::tests::utils::setup_swiftsync;

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
        let headers = mainnet_headers();

        // AssumeValid may be at or above the hints target. SwiftSync must stop at the hints.
        for assume_valid_height in [NUM_BLOCKS, NUM_BLOCKS + 1] {
            let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
            std::fs::create_dir_all(&datadir).unwrap();
            std::fs::copy(
                "./src/p2p_wire/tests/test_data/bitcoin.hints",
                format!("{datadir}/bitcoin.hints"),
            )
            .unwrap();

            let blocks = read_blocks_txt();
            assert_eq!(blocks.len(), NUM_BLOCKS);
            let peer = vec![PeerData::new(Vec::new(), blocks, HashMap::new())];
            let args =
                SetupNodeArgs::new(peer, false, Network::Bitcoin, datadir, assume_valid_height);
            let hash = headers[assume_valid_height].block_hash();
            let chain = setup_swiftsync(args, AssumeValidArg::UserInput(hash)).await;

            assert_eq!(
                chain.get_validation_index().unwrap(),
                u32::try_from(NUM_BLOCKS).unwrap()
            );
            assert_eq!(
                chain.get_best_block().unwrap(),
                (u32::try_from(assume_valid_height).unwrap(), hash)
            );
            assert_eq!(
                chain.ibd_state(),
                IBDState::SwiftSync {
                    processed_blocks: u32::try_from(NUM_BLOCKS).unwrap(),
                    total_blocks: u32::try_from(NUM_BLOCKS).unwrap(),
                }
            );
        }
    }

    #[tokio::test]
    async fn test_swift_sync_mutated_block() {
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
            mutate_block(block);
        }

        // We will have 9 peers sending mutated blocks, only one with the original txdata
        let mut peers = vec![PeerData::new(Vec::new(), blocks, HashMap::new()); 9];
        peers.push(PeerData::new(Vec::new(), read_blocks_txt(), HashMap::new()));

        let args = SetupNodeArgs::new(peers, false, Network::Bitcoin, datadir, NUM_BLOCKS);
        let assume_valid = AssumeValidArg::UserInput(headers[NUM_BLOCKS].block_hash());
        let chain = setup_swiftsync(args, assume_valid).await;

        assert_eq!(chain.get_validation_index().unwrap(), NUM_BLOCKS as u32);
        let best_block = chain.get_best_block().unwrap();
        let expected = (
            175,
            bhash!("00000000fd4afcc15f0fdda9b24be4c62068d8cf82fe6277730fd096712d9d08"),
        );

        assert_eq!(best_block.1, headers[NUM_BLOCKS].block_hash());
        assert_eq!(best_block, expected);
        assert_eq!(
            chain.ibd_state(),
            IBDState::SwiftSync {
                processed_blocks: NUM_BLOCKS as u32,
                total_blocks: NUM_BLOCKS as u32,
            }
        );
    }

    /// Stopping SwiftSync must return to shutdown, not enter proof sync.
    #[tokio::test]
    async fn test_swift_sync_shutdown_skips_proof_sync() {
        let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
        std::fs::create_dir_all(&datadir).unwrap();
        std::fs::copy(
            "./src/p2p_wire/tests/test_data/bitcoin.hints",
            format!("{datadir}/bitcoin.hints"),
        )
        .unwrap();

        // Eligible hints and AssumeValid let SwiftSync start, but shutdown prevents processing
        let hash = mainnet_headers()[NUM_BLOCKS].block_hash();
        let args = SetupNodeArgs::new(Vec::new(), false, Network::Bitcoin, datadir, NUM_BLOCKS);
        let node =
            setup_node_with_assume_valid::<RunningNode>(args, AssumeValidArg::UserInput(hash));
        *node.kill_signal.write().await = true;

        let node = timeout(Duration::from_secs(1), node.catch_up())
            .await
            .unwrap()
            .unwrap();

        // Proof sync would replace this state with ProofSync, even on an already-stopped node
        assert_eq!(
            node.chain.ibd_state(),
            IBDState::SwiftSync {
                processed_blocks: 0,
                total_blocks: u32::try_from(NUM_BLOCKS).unwrap(),
            }
        );
        assert_eq!(node.chain.get_validation_index().unwrap(), 0);
        assert_eq!(node.chain.get_acc().leaves, 0);
        assert_eq!(node.witness_mode, WitnessMode::Full);
    }

    /// Hints cannot authorize skipping validation beyond the configured AssumeValid block.
    #[tokio::test]
    async fn test_swift_sync_ineligible_assume_valid() {
        let headers = mainnet_headers();
        for assume_valid in [
            AssumeValidArg::Disabled,
            // In this test the node doesn't have block 176 nor the hardcoded mainnet hash
            AssumeValidArg::Hardcoded,
            AssumeValidArg::UserInput(headers[NUM_BLOCKS + 1].block_hash()),
            // We cannot proceed with AV SwiftSync if its stop height is after the AV block
            AssumeValidArg::UserInput(headers[NUM_BLOCKS - 1].block_hash()),
        ] {
            let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
            std::fs::create_dir_all(&datadir).unwrap();
            std::fs::copy(
                "./src/p2p_wire/tests/test_data/bitcoin.hints",
                format!("{datadir}/bitcoin.hints"),
            )
            .unwrap();

            let args = SetupNodeArgs::new(Vec::new(), false, Network::Bitcoin, datadir, NUM_BLOCKS);
            let mut node = setup_node_with_assume_valid::<SwiftSync>(args, assume_valid);
            node.last_block_request = 2;
            let tip = node.chain.get_best_block().unwrap();
            let acc = node.chain.get_acc();
            let ibd = node.chain.ibd_state();

            let node = timeout(Duration::from_secs(1), node.run(|_| {}))
                .await
                .unwrap()
                .unwrap();

            // Fall back to proof sync without entering SwiftSync or changing chainstate
            assert!(!node.was_aborted());
            assert_eq!(node.chain.get_validation_index().unwrap(), 0);
            assert_eq!(node.chain.get_best_block().unwrap(), tip);
            assert_eq!(node.chain.get_acc(), acc);
            assert_eq!(node.chain.ibd_state(), ibd);
            assert_eq!(node.last_block_request, 2);
            assert_eq!(node.witness_mode, WitnessMode::Full);
        }
    }

    /// Unusable hints must skip SwiftSync, leaving the node ready for proof sync.
    #[tokio::test]
    async fn test_swift_sync_unreadable_hints() {
        // Cover a missing file, wrong magic, and valid magic/version with the height missing
        let cases: [Option<&[u8]>; 3] = [None, Some(b"bad!"), Some(b"UTXO\0")];

        for contents in cases {
            let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
            std::fs::create_dir_all(&datadir).unwrap();

            if let Some(contents) = contents {
                std::fs::write(format!("{datadir}/bitcoin.hints"), contents).unwrap();
            }

            let args = SetupNodeArgs::new(Vec::new(), false, Network::Bitcoin, datadir, 0);
            let node = setup_node::<SwiftSync>(args);
            let node = timeout(Duration::from_secs(1), node.run(|_| {}))
                .await
                .unwrap()
                .unwrap();

            // Skipping is not an abort and must not disable witnesses or advance validation
            assert!(!node.was_aborted());
            assert_eq!(node.chain.get_validation_index().unwrap(), 0);
            assert_eq!(node.witness_mode, WitnessMode::Full);
            assert!(!matches!(
                node.chain.ibd_state(),
                IBDState::SwiftSync { .. }
            ));
        }
    }

    /// SwiftSync must preserve progress already made by proof sync.
    #[tokio::test]
    async fn test_swift_sync_skips_partially_validated_chain() {
        let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
        std::fs::create_dir_all(&datadir).unwrap();
        std::fs::copy(
            "./src/p2p_wire/tests/test_data/bitcoin.hints",
            format!("{datadir}/bitcoin.hints"),
        )
        .unwrap();

        let headers = mainnet_headers();
        let blocks = read_blocks_txt();
        let args = SetupNodeArgs::new(Vec::new(), false, Network::Bitcoin, datadir, 2);
        let mut node = setup_node::<SwiftSync>(args);
        // Validate only block 1, keeping the chain below the hints stop height
        node.chain
            .connect_block(
                &blocks[&headers[1].block_hash()],
                Proof::default(),
                HashMap::new(),
                Vec::new(),
            )
            .unwrap();
        node.last_block_request = 2;
        let acc = node.chain.get_acc();

        let node = timeout(Duration::from_secs(1), node.run(|_| {}))
            .await
            .unwrap()
            .unwrap();

        // Skip SwiftSync without changing the validated stump, request cursor, or witness mode
        assert!(!node.was_aborted());
        assert_eq!(node.chain.get_validation_index().unwrap(), 1);
        assert_eq!(node.chain.get_acc(), acc);
        assert_eq!(node.last_block_request, 2);
        assert_eq!(node.witness_mode, WitnessMode::Full);
        assert!(!matches!(
            node.chain.ibd_state(),
            IBDState::SwiftSync { .. }
        ));
    }

    /// A header-committed invalid block must abort SwiftSync, unlike a mutated block.
    #[tokio::test]
    async fn test_swift_sync_invalid_block() {
        let datadir = format!("./tmp-db/{}.swift_sync_node", rand::random::<u32>());
        std::fs::create_dir_all(&datadir).unwrap();

        // Sync only height 1, marking its sole coinbase output as unspent
        let file = File::create(format!("{datadir}/regtest.hints")).unwrap();
        let mut hints = HintsfileBuilder::new(file).initialize(1).unwrap();
        hints.append(EliasFano::compress(&[0])).unwrap();
        hints.finish().unwrap();

        let genesis = genesis_block(Network::Regtest);
        let mut block = genesis.clone();
        // An empty coinbase scriptSig violates consensus
        block.txdata[0].input[0].script_sig = ScriptBuf::new();
        block.header.prev_blockhash = genesis.block_hash();
        block.header.time += 1;

        // A matching Merkle root prevents treating the invalid body as a mutated peer response
        block.header.merkle_root = block.compute_merkle_root().unwrap();

        // Mine the easy Regtest target so the header itself can be accepted
        block.header.nonce = 0;
        while block.header.validate_pow(block.header.target()).is_err() {
            block.header.nonce += 1;
        }
        assert!(block.check_merkle_root());

        let hash = block.block_hash();
        let header = block.header;
        let peer = PeerData::new(Vec::new(), HashMap::from([(hash, block)]), HashMap::new());
        let args = SetupNodeArgs::new(vec![peer], false, Network::Regtest, datadir, 0);
        let node = setup_node_with_assume_valid::<SwiftSync>(args, AssumeValidArg::UserInput(hash));
        // Accept the header first, then let SwiftSync discover the invalid body
        node.chain.accept_header(header).unwrap();

        let node = timeout(Duration::from_secs(10), node.run(|_| {}))
            .await
            .unwrap()
            .unwrap();

        // Abort and invalidate the header without advancing validation or the Utreexo stump
        assert!(node.was_aborted());
        assert_eq!(node.chain.get_validation_index().unwrap(), 0);
        assert_eq!(
            node.chain.get_best_block().unwrap(),
            (0, genesis.block_hash())
        );
        assert_eq!(node.chain.get_block_height(&hash).unwrap(), None);
        assert_eq!(node.chain.get_acc().leaves, 0);
    }
}
