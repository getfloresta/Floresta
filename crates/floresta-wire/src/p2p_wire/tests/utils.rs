use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bitcoin::block::Header;
use bitcoin::consensus::deserialize_partial;
use bitcoin::consensus::encode;
use bitcoin::consensus::Decodable;
use bitcoin::hex::FromHex;
use bitcoin::p2p::ServiceFlags;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use floresta_chain::pruned_utreexo::UpdatableChainstate;
use floresta_chain::AssumeValidArg;
use floresta_chain::ChainState;
use floresta_chain::FlatChainStore;
use floresta_chain::FlatChainStoreConfig;
use floresta_common::service_flags;
use floresta_common::service_flags::UTREEXO;
use floresta_common::FractionAvg;
use rand::rngs::OsRng;
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
use crate::node::ConnectionKind;
use crate::node::LocalPeerView;
use crate::node::NodeNotification;
use crate::node::NodeRequest;
use crate::node::PeerStatus;
use crate::node::UtreexoNode;
use crate::p2p_wire::block_proof::UtreexoProof;
use crate::p2p_wire::mempool::Mempool;
use crate::p2p_wire::peer::PeerMessages;
use crate::p2p_wire::peer::Version;
use crate::p2p_wire::sync_node::SyncNode;
use crate::p2p_wire::transport::TransportProtocol;
use crate::UtreexoNodeConfig;

/// A list of headers, used to represent the collection of headers.
pub type HeaderList = Vec<Header>;

/// A map that associates block hashes with their corresponding `UtreexoBlock` objects.
/// This is useful for efficiently looking up blocks by their hash.
pub type BlockHashMap = HashMap<BlockHash, Block>;

/// A map of block hashes to raw block data (represented as bytes vector).
pub type BlockDataMap = HashMap<BlockHash, Vec<u8>>;

/// A collection of essential data related to blocks and headers.
pub struct Essentials {
    pub headers: HeaderList,
    pub blocks: BlockHashMap,
    pub invalid_block: Block,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UtreexoRoots {
    roots: Option<Vec<String>>,
    numleaves: usize,
}

#[derive(Deserialize, Debug, Clone)]
struct BlockFile {
    block: String,
}

#[derive(Debug)]
pub struct TestPeer {
    headers: Vec<Header>,
    blocks: HashMap<BlockHash, Block>,
    accs: HashMap<BlockHash, Vec<u8>>,
    node_tx: UnboundedSender<NodeNotification>,
    node_rx: UnboundedReceiver<NodeRequest>,
    peer_id: u32,
}

impl TestPeer {
    pub fn new(
        node_tx: UnboundedSender<NodeNotification>,
        headers: Vec<Header>,
        blocks: HashMap<BlockHash, Block>,
        accs: HashMap<BlockHash, Vec<u8>>,
        node_rx: UnboundedReceiver<NodeRequest>,
        peer_id: u32,
    ) -> Self {
        TestPeer {
            headers,
            blocks,
            accs,
            node_tx,
            node_rx,
            peer_id,
        }
    }

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
            ))
            .unwrap();

        loop {
            let req = self.node_rx.recv().await.unwrap();

            match req {
                NodeRequest::GetHeaders(hashes) => {
                    let headers = hashes
                        .iter()
                        .filter_map(|h| self.headers.iter().find(|x| x.block_hash() == *h))
                        .copied()
                        .collect();

                    self.node_tx
                        .send(NodeNotification::FromPeer(
                            self.peer_id,
                            PeerMessages::Headers(headers),
                        ))
                        .unwrap();
                }
                NodeRequest::GetUtreexoState((hash, _)) => {
                    let accs = self.accs.get(&hash).unwrap().clone();
                    self.node_tx
                        .send(NodeNotification::FromPeer(
                            self.peer_id,
                            PeerMessages::UtreexoState(accs),
                        ))
                        .unwrap();
                }
                NodeRequest::GetBlock(hashes) => {
                    for hash in hashes {
                        let block = self.blocks.get(&hash).unwrap().clone();
                        self.node_tx
                            .send(NodeNotification::FromPeer(
                                self.peer_id,
                                PeerMessages::Block(block),
                            ))
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
                    self.node_tx
                        .send(NodeNotification::FromPeer(
                            self.peer_id,
                            PeerMessages::UtreexoProof(proof),
                        ))
                        .unwrap();
                }
                _ => {}
            }
        }

        self.node_tx
            .send(NodeNotification::FromPeer(
                self.peer_id,
                PeerMessages::Disconnected(self.peer_id as usize),
            ))
            .unwrap();
    }
}

pub fn create_peer(
    headers: Vec<Header>,
    blocks: HashMap<BlockHash, Block>,
    accs: HashMap<BlockHash, Vec<u8>>,
    node_sender: UnboundedSender<NodeNotification>,
    sender: UnboundedSender<NodeRequest>,
    node_rcv: UnboundedReceiver<NodeRequest>,
    peer_id: u32,
) -> LocalPeerView {
    let mut peer = TestPeer::new(node_sender, headers, blocks, accs, node_rcv, peer_id);
    task::spawn(async move {
        peer.run().await;
    });

    LocalPeerView {
        message_times: FractionAvg::new(0, 0),
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
        disable_dns_seeds: false,
        network,
        pow_fraud_proofs,
        compact_filters: false,
        fixed_peer: None,
        max_banscore: 100,
        max_outbound: 8,
        max_inflight: 10,
        datadir,
        proxy: None,
        assume_utreexo: None,
        backfill: false,
        filter_start_height: None,
        user_agent: "node_test".to_string(),
        allow_v1_fallback: true,
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

pub fn get_test_headers() -> Vec<Header> {
    let mut headers: Vec<Header> = Vec::new();

    let file = include_bytes!("../../../../floresta-chain/testdata/signet_headers.zst");
    let uncompressed: Vec<u8> = zstd::decode_all(std::io::Cursor::new(file)).unwrap();
    let mut buffer = uncompressed.as_slice();

    while let Ok(header) = Header::consensus_decode(&mut buffer) {
        headers.push(header);
    }

    headers
}

pub fn get_test_blocks() -> io::Result<HashMap<BlockHash, Block>> {
    let dir = "./src/p2p_wire/tests/test_data/blocks.json";
    let mut contents = String::new();
    File::open(dir)?.read_to_string(&mut contents)?;

    let blocks: Vec<BlockFile> = serde_json::from_str(&contents).unwrap();
    let mut u_blocks = HashMap::new();

    for block_str in blocks {
        let ser_block = Vec::from_hex(&block_str.block).unwrap();
        let block: Block = deserialize_partial(&ser_block).unwrap().0;
        u_blocks.insert(block.block_hash(), block);
    }

    Ok(u_blocks)
}

pub fn get_test_accs() -> io::Result<HashMap<BlockHash, Vec<u8>>> {
    let mut contents = String::new();
    File::open("./src/p2p_wire/tests/test_data/roots.json")
        .unwrap()
        .read_to_string(&mut contents)
        .unwrap();
    let roots: Vec<UtreexoRoots> = serde_json::from_str(&contents).unwrap();
    let headers = get_test_headers();
    let mut accs = HashMap::new();

    for root in roots.into_iter() {
        let buffer = serialize(root.clone());

        // Insert the serialised Utreexo-Root along with its corresponding BlockHash in the HashMap
        accs.insert(headers[root.numleaves].block_hash(), buffer);
    }
    Ok(accs)
}

pub fn generate_invalid_block() -> Block {
    let invalid_block_str = "00000020daf3b60d374b19476461f97540498dcfa2eb7016238ec6b1d022f82fb60100007a7ae65b53cb988c2ec92d2384996713821d5645ffe61c9acea60da75cd5edfa1a944d5fae77031e9dbb050001010000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff025751feffffff0200f2052a01000000160014ef2dceae02e35f8137de76768ae3345d99ca68860000000000000000776a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf94c4fecc7daa2490047304402202b3f946d6447f9bf17d00f3696cede7ee70b785495e5498274ee682a493befd5022045fc0bcf9331073168b5d35507175f9f374a8eba2336873885d12aada67ea5f60100012000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    let block = Vec::from_hex(invalid_block_str).unwrap();
    let block: Block = deserialize_partial(&block).unwrap().0;

    block
}

pub fn get_essentials() -> Essentials {
    let headers = get_test_headers();
    let blocks = get_test_blocks().unwrap();
    let invalid_block = generate_invalid_block();

    Essentials {
        headers,
        blocks,
        invalid_block,
    }
}

type PeerData = (HeaderList, BlockHashMap, BlockDataMap);

pub async fn setup_node(
    peers: Vec<PeerData>,
    pow_fraud_proofs: bool,
    network: Network,
    datadir: &str,
    num_blocks: usize,
) -> Arc<ChainState<FlatChainStore>> {
    let config = FlatChainStoreConfig::new(datadir.into());

    let chainstore = FlatChainStore::new(config).unwrap();
    let mempool = Arc::new(Mutex::new(Mempool::new(1000)));
    let chain = ChainState::new(chainstore, network, AssumeValidArg::Disabled);
    let chain = Arc::new(chain);

    let mut headers = get_test_headers();
    headers.remove(0);
    headers.truncate(num_blocks);
    for header in headers {
        chain.accept_header(header).unwrap();
    }

    let config = get_node_config(datadir.into(), network, pow_fraud_proofs);
    let kill_signal = Arc::new(RwLock::new(false));
    let mut node = UtreexoNode::<Arc<ChainState<FlatChainStore>>, SyncNode>::new(
        config,
        chain.clone(),
        mempool,
        None,
        kill_signal.clone(),
        AddressMan::default(),
    )
    .unwrap();

    for (i, peer) in peers.into_iter().enumerate() {
        let (sender, receiver) = unbounded_channel();
        let peer = create_peer(
            peer.0,
            peer.1,
            peer.2,
            node.node_tx.clone(),
            sender.clone(),
            receiver,
            i as u32,
        );

        node.peers.insert(i as u32, peer);
    }

    timeout(Duration::from_secs(100), node.run(|_| {}))
        .await
        .unwrap();

    chain
}
