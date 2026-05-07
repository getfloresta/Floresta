// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;
use core::fmt::Display;

use bitcoin::BlockHash;
use bitcoin::Txid;
use floresta_proc_macro::enum_str_map;

use super::rpc_types::*;

#[floresta_proc_macro::maybe_async]
pub trait BlockchainRpc {
    type Error: Display + Debug;

    /// Finds an specific utxo in the chain
    ///
    /// You can use this to look for a utxo. If it exists, it will return the amount and
    /// scriptPubKey of this utxo. It returns an empty object if the utxo doesn't exist.
    /// You must have enabled block filters by setting the `blockfilters=1` option.
    fn find_tx_out(
        &self,
        txid: Txid,
        vout: u32,
        script: String,
        height_hint: u32,
    ) -> Result<Option<GetTxOut>, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getbestblockhash.md")]
    fn get_best_block_hash(&self) -> Result<BlockHash, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getblock.md")]
    fn get_block(
        &self,
        hash: BlockHash,
        verbosity: Option<u32>,
    ) -> Result<GetBlockRes, Self::Error>;

    /// Returns general information about the chain we are on
    ///
    /// This method returns a bunch of information about the chain we are on, including
    /// the current height, the best block hash, the difficulty, and whether we are
    /// currently in IBD (Initial Block Download) mode.
    fn get_blockchain_info(&self) -> Result<GetBlockchainInfo, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getblockcount.md")]
    fn get_block_count(&self) -> Result<u32, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getblockhash.md")]
    fn get_block_hash(&self, height: u32) -> Result<BlockHash, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getdeploymentinfo.md")]
    fn get_deployment_info(
        &self,
        blockhash: Option<BlockHash>,
    ) -> Result<GetDeploymentInfo, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getdifficulty.md")]
    fn get_difficulty(&self) -> Result<f64, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/gettxout.md")]
    fn get_tx_out(
        &self,
        txid: Txid,
        outpoint: u32,
        _include_mempool: bool,
    ) -> Result<Option<GetTxOut>, Self::Error>;

    /// Returns the proof that one or more transactions were included in a block
    ///
    /// This method returns the Merkle proof, showing that a transaction was included in a block.
    /// The proof is returned as a vector hexadecimal string.
    fn get_txout_proof(
        &self,
        tx_ids: &[Txid],
        blockhash: Option<BlockHash>,
    ) -> Result<String, Self::Error>;

    /// Gets the current accumulator for the chain we're on
    ///
    /// This method returns the current accumulator for the chain we're on. The accumulator is
    /// a set of roots, that let's us prove that a UTXO exists in the chain. This method returns
    /// a vector of hexadecimal strings, each of which is a root in the accumulator.
    fn get_roots(&self) -> Result<Vec<String>, Self::Error>;

    /// Returns the block header for the given block hash
    ///
    /// This method returns the block header for the given block hash, as defined
    /// in the Bitcoin protocol specification. A header contains the block's version,
    /// the previous block hash, the merkle root, the timestamp, the difficulty target,
    /// and the nonce.
    fn get_block_header(
        &self,
        hash: BlockHash,
        verbosity: Option<bool>,
    ) -> Result<GetBlockHeaderRes, Self::Error>;
}

#[floresta_proc_macro::maybe_async]
pub trait WalletRpc {
    type Error: Display + Debug;

    /// Loads up a descriptor into the wallet
    ///
    /// This method loads up a descriptor into the wallet. If the rescan option is not None,
    /// the wallet will be rescanned for transactions matching the descriptor. If you have
    /// compact block filters enabled, this process will be much faster and use less bandwidth.
    /// The rescan parameter is the height at which to start the rescan, and should be at least
    /// as old as the oldest transaction this descriptor could have been used in.
    fn load_descriptor(&self, descriptor: String) -> Result<bool, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/listdescriptors.md")]
    /// Returns a list of all descriptors currently loaded in the wallet
    fn list_descriptors(&self) -> Result<Vec<String>, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/rescanblockchain.md")]
    fn rescan_blockchain(
        &self,
        start: Option<u32>,
        stop: Option<u32>,
        use_timestamp: bool,
        confidence: Option<RescanConfidence>,
    ) -> Result<bool, Self::Error>;
}

#[floresta_proc_macro::maybe_async]
pub trait NetworkRpc {
    type Error: Display + Debug;

    /// Tells florestad to connect with a peer
    ///
    /// You can use this to connect with a given node, providing it's IP address and port.
    /// If the `v2transport` option is set, we won't retry connecting using the old, unencrypted
    /// P2P protocol.
    #[doc = include_str!("../../../doc/rpc/addnode.md")]
    fn add_node(
        &self,
        node: String,
        command: AddNodeCommand,
        v2transport: bool,
    ) -> Result<(), Self::Error>;

    /// Immediately disconnect from a peer.
    ///
    /// The peer can be referenced either by node_address or node_id.
    /// If referencing by node_id, an empty string must be passed as the node_address.
    fn disconnect_node(
        &self,
        node_address: String,
        node_id: Option<u32>,
    ) -> Result<(), Self::Error>;

    /// Gets information about the peers we're connected with
    ///
    /// This method returns information about the peers we're connected with. This includes
    /// the peer's IP address, the peer's version, the peer's user agent, the transport protocol
    /// and the peer's current height.
    fn get_peer_info(&self) -> Result<Vec<PeerInfo>, Self::Error>;

    /// Returns the number of peers currently connected to the node.
    fn get_connection_count(&self) -> Result<usize, Self::Error>;

    /// Returns information about the network we're connected to
    fn get_network_info(&self) -> Result<GetNetworkInfo, Self::Error>;

    /// Returns address manager statistics broken down by network.
    #[doc = include_str!("../../../doc/rpc/getaddrmaninfo.md")]
    fn get_addrman_info(&self) -> Result<GetAddrManInfo, Self::Error>;

    /// Sends a ping to all peers, checking if they are still alive
    fn ping(&self) -> Result<bool, Self::Error>;
}

pub trait RawTransactionRpc {
    type Error: Display + Debug;

    /// Sends a hex-encoded transaction to the network
    ///
    /// This method sends a transaction to the network. The transaction should be encoded as a
    /// hexadecimal string. If the transaction is valid, it will be broadcast to the network, and
    /// return the transaction id. If the transaction is invalid, an error will be returned.
    fn send_raw_transaction(&self, tx: String) -> Result<Txid, Self::Error>;

    /// Gets a transaction from the blockchain
    ///
    /// This method returns a transaction that's cached in our wallet. If the verbosity flag is
    /// set to false, the transaction is returned as a hexadecimal string. If the verbosity
    /// flag is set to true, the transaction is returned as a json object.
    fn get_raw_transaction(
        &self,
        tx_id: Txid,
        verbosity: Option<u8>,
    ) -> Result<GetRawTransactionRes, Self::Error>;
}

#[floresta_proc_macro::maybe_async]
pub trait ControlRpc {
    type Error: Display + Debug;

    /// Stops the florestad process
    ///
    /// This can be used to gracefully stop the florestad process.
    fn stop(&self) -> Result<String, Self::Error>;

    /// Returns for how long florestad has been running, in seconds
    fn uptime(&self) -> Result<u64, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getmemoryinfo.md")]
    /// Returns statistics about Floresta's memory usage.
    ///
    /// Returns zeroed values for all runtimes that are not *-gnu or MacOS.
    fn get_memory_info(&self, mode: String) -> Result<GetMemInfoRes, Self::Error>;

    #[doc = include_str!("../../../doc/rpc/getrpcinfo.md")]
    /// Returns stats about our RPC server
    fn get_rpc_info(&self) -> Result<GetRpcInfoRes, Self::Error>;
}

#[enum_str_map(case = "lower", separator = "")]
#[derive(Debug, Clone, PartialEq, Eq)]
/// Defines all available RPC methods for the Floresta JSON-RPC interface.
///
/// This enum is annotated with `#[enum_str_map]`, which automatically generates
/// bidirectional conversion between enum variants and their string representations
/// (e.g., `GetBestBlockHash` <-> `"getbestblockhash"`).
pub enum RpcMethods {
    // Blockchain
    FindTxOut,
    GetBestBlockHash,
    GetBlock,
    GetBlockFromPeer,
    GetBlockchainInfo,
    GetBlockCount,
    GetBlockHash,
    GetDeploymentInfo,
    GetDifficulty,
    GetTxOut,
    GetTxOutProof,
    GetRoots,
    GetBlockHeader,

    // Wallet
    LoadDescriptor,
    ListDescriptors,
    RescanBlockchain,

    // Network
    AddNode,
    DisconnectNode,
    GetAddrManInfo,
    GetConnectionCount,
    GetNetworkInfo,
    GetPeerInfo,
    Ping,

    // RawTransactions
    SendRawTransaction,
    GetRawTransaction,

    // Control
    Stop,
    Uptime,
    GetMemoryInfo,
    GetRpcInfo,
}
