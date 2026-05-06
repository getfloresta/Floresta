// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Debug;

use floresta_proc_macro::enum_str_map;

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
