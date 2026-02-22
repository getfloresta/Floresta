use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bitcoin::block::Header;
use bitcoin::consensus::encode;
use bitcoin::consensus::encode::deserialize_hex;
use bitcoin::consensus::Decodable;
use bitcoin::hex::FromHex;
use bitcoin::p2p::ServiceFlags;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use derive_more::Constructor;
use floresta_chain::pruned_utreexo::UpdatableChainstate;
use floresta_chain::AssumeValidArg;
use floresta_chain::ChainState;
use floresta_chain::FlatChainStore;
use floresta_chain::FlatChainStoreConfig;
use floresta_common::service_flags;
use floresta_common::service_flags::UTREEXO;
use floresta_common::Ema;
use floresta_mempool::Mempool;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::RngCore;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::task;
use tokio::time::timeout;
use zstd;

use crate::address_man::AddressMan;
use crate::node::running_ctx::RunningNode;
use crate::node::swift_sync_ctx::Hints;
use crate::node::swift_sync_ctx::SwiftSync;
use crate::node::sync_ctx::SyncNode;
use crate::node::ConnectionKind;
use crate::node::InflightRequests;
use crate::node::LocalPeerView;
use crate::node::NodeNotification;
use crate::node::NodeRequest;
use crate::node::PeerStatus;
use crate::node::UtreexoNode;
use crate::node_context::NodeContext;
use crate::p2p_wire::block_proof::UtreexoProof;
use crate::p2p_wire::peer::PeerMessages;
use crate::p2p_wire::peer::Version;
use crate::p2p_wire::transport::TransportProtocol;
use crate::UtreexoNodeConfig;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UtreexoRoots {
    roots: Option<Vec<String>>,
    numleaves: usize,
}

#[derive(Debug, Constructor)]
pub struct SimulatedPeer {
    headers: Vec<Header>,
    blocks: HashMap<BlockHash, Block>,
    accs: HashMap<BlockHash, Vec<u8>>,
    node_tx: UnboundedSender<NodeNotification>,
    node_rx: UnboundedReceiver<NodeRequest>,
    peer_id: u32,
}

impl SimulatedPeer {
    pub async fn run(&mut self) {
        let version = Version {
            user_agent: "node_test".to_string(),
            protocol_version: 0,
            blocks: rand::random::<u32>() % 23,
            id: self.peer_id,
            address_id: rand::random::<usize>(),
            services: ServiceFlags::NETWORK
                | service_flags::UTREEXO.into()
                | ServiceFlags::WITNESS
                | ServiceFlags::COMPACT_FILTERS
                | ServiceFlags::from(1 << 25),
            kind: ConnectionKind::Regular(UTREEXO.into()),
            transport_protocol: TransportProtocol::V2,
        };

        self.node_tx
            .send(NodeNotification::FromPeer(
                self.peer_id,
                PeerMessages::Ready(version),
                Instant::now(),
            ))
            .unwrap();

        loop {
            let req = self.node_rx.recv().await.unwrap();
            let now = Instant::now();

            match req {
                NodeRequest::GetHeaders(hashes) => {
                    let headers = hashes
                        .iter()
                        .filter_map(|h| self.headers.iter().find(|x| x.block_hash() == *h))
                        .copied()
                        .collect();

                    let peer_msg = PeerMessages::Headers(headers);
                    self.node_tx
                        .send(NodeNotification::FromPeer(self.peer_id, peer_msg, now))
                        .unwrap();
                }
                NodeRequest::GetUtreexoState((hash, _)) => {
                    let accs = self.accs.get(&hash).unwrap().clone();

                    let peer_msg = PeerMessages::UtreexoState(accs);
                    self.node_tx
                        .send(NodeNotification::FromPeer(self.peer_id, peer_msg, now))
                        .unwrap();
                }
                NodeRequest::GetBlock(hashes) => {
                    for hash in hashes {
                        let block = self.blocks.get(&hash).unwrap().clone();

                        let peer_msg = PeerMessages::Block(block);
                        self.node_tx
                            .send(NodeNotification::FromPeer(self.peer_id, peer_msg, now))
                            .unwrap();
                    }
                }
                NodeRequest::Shutdown => {
                    break;
                }
                NodeRequest::GetBlockProof((block_hash, _, _)) => {
                    let proof = UtreexoProof {
                        block_hash,
                        leaf_data: vec![],
                        targets: vec![],
                        proof_hashes: vec![],
                    };

                    let peer_msg = PeerMessages::UtreexoProof(proof);
                    self.node_tx
                        .send(NodeNotification::FromPeer(self.peer_id, peer_msg, now))
                        .unwrap();
                }
                _ => {}
            }
        }

        self.node_tx
            .send(NodeNotification::FromPeer(
                self.peer_id,
                PeerMessages::Disconnected(self.peer_id as usize),
                Instant::now(),
            ))
            .unwrap();
    }
}

pub fn spawn_peer(
    peer_data: PeerData,
    node_sender: UnboundedSender<NodeNotification>,
    peer_id: u32,
) -> LocalPeerView {
    let (sender, node_rcv) = unbounded_channel();
    let PeerData {
        headers,
        blocks,
        accs,
    } = peer_data;

    let mut peer = SimulatedPeer::new(headers, blocks, accs, node_sender, node_rcv, peer_id);
    task::spawn(async move {
        peer.run().await;
    });

    LocalPeerView {
        message_times: Ema::with_half_life_50(),
        address: "127.0.0.1".parse().unwrap(),
        services: service_flags::UTREEXO.into(),
        user_agent: "/utreexo:0.1.0/".to_string(),
        height: 0,
        state: PeerStatus::Ready,
        channel: sender,
        port: 8333,
        kind: ConnectionKind::Regular(UTREEXO.into()),
        banscore: 0,
        address_id: 0,
        _last_message: Instant::now(),
        transport_protocol: TransportProtocol::V2,
    }
}

pub fn get_node_config(
    datadir: String,
    network: Network,
    pow_fraud_proofs: bool,
) -> UtreexoNodeConfig {
    UtreexoNodeConfig {
        network,
        pow_fraud_proofs,
        datadir,
        user_agent: "node_test".to_string(),
        ..Default::default()
    }
}

pub fn serialize(root: UtreexoRoots) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&(root.numleaves as u64).to_le_bytes());

    for root_hash in root.roots.unwrap() {
        let bytes = Vec::from_hex(&root_hash).unwrap();
        buffer.extend_from_slice(&bytes);
    }

    buffer
}

pub fn create_false_acc(tip: usize) -> Vec<u8> {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let node_hash = encode::serialize_hex(&bytes);

    let utreexo_root = UtreexoRoots {
        roots: Some(vec![node_hash]),
        numleaves: tip,
    };

    serialize(utreexo_root)
}

/// Returns the first 2016 signet headers
pub fn signet_headers() -> Vec<Header> {
    let mut headers: Vec<Header> = Vec::new();

    let file = include_bytes!("../../../../floresta-chain/testdata/signet_headers.zst");
    let uncompressed: Vec<u8> = zstd::decode_all(std::io::Cursor::new(file)).unwrap();
    let mut buffer = uncompressed.as_slice();

    while let Ok(header) = Header::consensus_decode(&mut buffer) {
        headers.push(header);
    }

    headers
}

pub fn mainnet_headers() -> Vec<Header> {
    let mut headers: Vec<Header> = Vec::new();

    let file = include_bytes!("../../../../floresta-chain/testdata/headers.zst");
    let uncompressed: Vec<u8> = zstd::decode_all(std::io::Cursor::new(file)).unwrap();
    let mut buffer = uncompressed.as_slice();

    while let Ok(header) = Header::consensus_decode(&mut buffer) {
        headers.push(header);
    }

    headers
}

/// Returns the first 121 signet blocks, including genesis
pub fn signet_blocks() -> HashMap<BlockHash, Block> {
    let file = include_str!("./test_data/blocks.json");
    let entries: Vec<serde_json::Value> = serde_json::from_str(file).unwrap();

    entries
        .iter()
        .map(|e| {
            let str = e["block"].as_str().unwrap();
            let block: Block = deserialize_hex(str).unwrap();
            (block.block_hash(), block)
        })
        .collect()
}

/// Returns the first 120 signet accumulators. The genesis hash doesn't have a value since those
/// coinbase coins are unspendable.
pub fn signet_roots() -> HashMap<BlockHash, Vec<u8>> {
    let file = include_str!("./test_data/roots.json");
    let roots: Vec<UtreexoRoots> = serde_json::from_str(file).unwrap();

    let headers = signet_headers();
    let mut accs = HashMap::new();

    for root in roots.into_iter() {
        // For empty signet blocks numleaves equals the height; the genesis coins are unspendable,
        // so at height 1 we have one leaf, and so on as long as blocks have only one coinbase UTXO
        let height = root.numleaves;

        accs.insert(headers[height].block_hash(), serialize(root));
    }
    accs
}

/// Modifies a block to have an invalid output script (txdata is tampered with)
pub fn make_block_invalid(block: &mut Block) {
    let mut rng = rand::thread_rng();

    let tx = block.txdata.choose_mut(&mut rng).unwrap();
    let out = tx.output.choose_mut(&mut rng).unwrap();
    let spk = out.script_pubkey.as_mut_bytes();
    let byte = spk.choose_mut(&mut rng).unwrap();

    *byte += 1;
}

#[derive(Constructor)]
/// The chain data that our simulated peer will have
pub struct PeerData {
    headers: Vec<Header>,
    blocks: HashMap<BlockHash, Block>,
    accs: HashMap<BlockHash, Vec<u8>>,
}

#[derive(Constructor)]
/// The arguments needed to set up the test `UtreexoNode`
pub struct SetupNodeArgs {
    peers: Vec<PeerData>,
    pow_fraud_proofs: bool,
    network: Network,
    datadir: String,
    num_blocks: usize,
}

type Chain = Arc<ChainState<FlatChainStore>>;

pub fn setup_node<T>(args: SetupNodeArgs) -> UtreexoNode<Chain, T>
where
    T: 'static + Default + NodeContext,
{
    let net = args.network;
    let datadir = args.datadir;

    // Create `ChainState` and add headers to it
    let chainstore = FlatChainStore::new(FlatChainStoreConfig::new(datadir.clone())).unwrap();
    let chain = Arc::new(ChainState::new(chainstore, net, AssumeValidArg::Disabled));

    let headers = match net {
        Network::Signet => signet_headers(),
        Network::Bitcoin => mainnet_headers(),
        _ => panic!("unavailable headers for net: {net}"),
    };
    for header in headers.into_iter().skip(1).take(args.num_blocks) {
        chain.accept_header(header).unwrap();
    }

    // Create `UtreexoNode` and spawn the simulated peers
    let config = get_node_config(datadir, net, args.pow_fraud_proofs);
    let mempool = Arc::new(Mutex::new(Mempool::new(1000)));
    let kill_signal = Arc::new(RwLock::new(false));
    let addr_man = AddressMan::default();
    let mut node = UtreexoNode::new(config, chain, mempool, None, kill_signal, addr_man).unwrap();

    for (i, peer_data) in args.peers.into_iter().enumerate() {
        let peer_id = i as u32;
        let peer = spawn_peer(peer_data, node.node_tx.clone(), peer_id);

        node.peers.insert(peer_id, peer);
        // This allows the node to properly assign a message time for the peer
        node.inflight.insert(
            InflightRequests::Connect(peer_id),
            (peer_id, Instant::now()),
        );
    }

    node
}

const NODE_TIMEOUT: Duration = Duration::from_secs(100);

pub async fn setup_sync_node(args: SetupNodeArgs) -> Arc<ChainState<FlatChainStore>> {
    let node = setup_node::<SyncNode>(args);
    let chain = node.chain.clone();

    timeout(NODE_TIMEOUT, node.run(|_| {})).await.unwrap();

    chain
}

pub async fn setup_swiftsync(args: SetupNodeArgs) -> Arc<ChainState<FlatChainStore>> {
    let node = setup_node::<SwiftSync>(args);
    let chain = node.chain.clone();

    timeout(NODE_TIMEOUT, node.run(|_| {})).await.unwrap();

    chain
}

pub async fn setup_running_node(args: SetupNodeArgs) -> Arc<ChainState<FlatChainStore>> {
    let node = setup_node::<RunningNode>(args);
    let kill_signal = node.kill_signal.clone();
    let chain = node.chain.clone();

    // Sends a kill signal to the `RunningNode` after 20 seconds
    let killer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        *kill_signal.write().await = true;
    });

    let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
    timeout(NODE_TIMEOUT, node.run(sender)).await.unwrap();

    receiver.await.unwrap();
    killer.abort();

    chain
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::str::FromStr;

    use bitcoin::consensus::deserialize;
    use bitcoin::hashes::Hash;
    use bitcoin::BlockHash;
    use floresta_common::bhash;

    use super::make_block_invalid;
    use super::signet_blocks;
    use super::signet_headers;
    use super::signet_roots;
    use crate::p2p_wire::tests::utils::Hints;

    fn load_test_hints() -> Hints {
        let file = File::open("./src/p2p_wire/tests/test_data/bitcoin.hints").unwrap();
        Hints::from_file(file)
    }

    #[test]
    #[should_panic]
    fn test_hints_file_genesis() {
        let mut hints = load_test_hints();
        let _ = hints.get_indexes(0);
    }

    #[test]
    #[should_panic]
    fn test_hints_file_after_stop_height() {
        let mut hints = load_test_hints();
        let _ = hints.get_indexes(176);
    }

    #[test]
    fn test_hints_file_shape() {
        let mut hints = load_test_hints();
        assert_eq!(hints.stop_height, 175);

        for height in 1..=175 {
            let unspent_indices = match height {
                9 => Vec::new(),      // The single UTXO in this block is spent later
                170 => vec![0, 1, 2], // Contains the transaction spending the height-9 UTXO
                _ => vec![0],         // Other blocks have just a coinbase output (here unspent)
            };

            assert_eq!(hints.get_indexes(height), unspent_indices);
        }
    }

    #[test]
    fn test_get_headers_and_blocks() {
        let headers = signet_headers();
        let blocks = signet_blocks();

        assert_eq!(headers.len(), 2016);
        assert_eq!(blocks.len(), 121); // including genesis, up to height 120

        // Sanity check
        let mut prev_hash = BlockHash::all_zeros();
        for (i, header) in headers.iter().enumerate() {
            let hash = header.block_hash();

            let Some(block) = blocks.get(&hash) else {
                if i < 121 {
                    panic!("We should have a block at height {i}");
                }
                break;
            };

            assert_eq!(*header, block.header, "hashmap links to the correct block");
            assert!(block.check_merkle_root(), "valid txdata");
            assert_eq!(header.prev_blockhash, prev_hash, "valid hash chain");
            prev_hash = hash;
        }
    }

    #[test]
    fn test_make_block_invalid() {
        let hash = bhash!("000002c45c8ea9e553d4b0ee5d50324e56fc76f13019873fe707ff44fc56183f");
        let blocks = signet_blocks();

        let mut block_25 = blocks.get(&hash).unwrap().clone();
        make_block_invalid(&mut block_25);

        assert!(!block_25.txdata.is_empty(), "at least one tx");
        assert!(
            !block_25.check_merkle_root(),
            "invalid merkle root (txdata was tampered with)",
        );

        let headers = signet_headers();
        assert_eq!(
            block_25.header.prev_blockhash,
            headers[24].block_hash(),
            "block is at height 25",
        );
    }

    #[test]
    fn test_get_accs() {
        let accs = signet_roots();
        assert_eq!(accs.len(), 120, "we have roots starting from height 1");

        for (i, header) in signet_headers().iter().enumerate().skip(1).take(120) {
            let acc = accs.get(&header.block_hash()).unwrap();

            let leaves: u64 = deserialize(acc.clone().drain(0..8).as_slice()).unwrap();
            assert_eq!(i as u64, leaves, "one leaf added per block");
        }
    }
}
