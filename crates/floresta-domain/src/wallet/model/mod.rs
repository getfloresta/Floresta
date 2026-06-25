use core::cmp::Ordering;
use core::fmt::Debug;

use bitcoin::OutPoint;
use bitcoin::Transaction;
use bitcoin::TxOut;
use bitcoin::consensus::deserialize;
use bitcoin::hash_types::Txid;
use bitcoin::hashes::Hash as HashTrait;
use bitcoin::hashes::hex::FromHex;
use serde::Deserialize;
use serde::Serialize;

mod merkle;

pub use merkle::MerkleProof;

/// Every address contains zero or more associated transactions, this struct defines what
/// data we store for those.
#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct CachedTransaction {
    pub tx: Transaction,
    pub height: u32,
    pub merkle_block: Option<MerkleProof>,
    pub hash: Txid,
    pub position: u32,
}

impl Ord for CachedTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.height.cmp(&other.height)
    }
}

impl PartialOrd for CachedTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for CachedTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.height == other.height
    }
}

impl Default for CachedTransaction {
    fn default() -> Self {
        Self {
            // A placeholder transaction with no input and no outputs, the bare-minimum to be
            // serializable
            tx: deserialize(&Vec::from_hex("010000000000ffffffff").unwrap()).unwrap(),
            height: 0,
            merkle_block: None,
            hash: Txid::all_zeros(),
            position: 0,
        }
    }
}

/// Holds some useful data about our wallet, like how many addresses we have, how many
/// transactions we have, etc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub address_count: usize,
    pub transaction_count: usize,
    pub utxo_count: usize,
    pub cache_height: u32,
    pub txo_count: usize,
    pub balance: u64,
    pub derivation_index: u32,
}

/// Type alias for UTXO information
pub type AddressUtxos = Vec<(TxOut, OutPoint)>;
