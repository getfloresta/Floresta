use bitcoin::block::Version;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::CompactTarget;
use bitcoin::Transaction;
use bitcoin::Txid;

use crate::mempool::error::MempoolError;

/// An abstract interface for interacting with a Mempool.
///
/// This trait decouples consumers from a concrete Mempool implementation,
/// allowing other crates to depend on this domain-level abstraction
/// rather than on the `floresta-mempool` crate directly.
///
/// `: Send` means any implementor can be wrapped in an envelope,
/// and its ownership handed off among threads.
pub trait MempoolBackend: Send {
    /// Returns all transactions accepted to the Mempool.
    fn list_mempool(&self) -> Vec<Txid>;

    fn get_block_template(
        &self,
        version: Version,
        prev_blockhash: BlockHash,
        time: u32,
        bits: CompactTarget,
        max_block_weight: u64,
    ) -> Block;

    fn get_from_mempool<'a>(&'a self, id: &Txid) -> Option<&'a Transaction>;

    fn get_stale(&mut self) -> Vec<Txid>;

    fn consume_block(&mut self, block: &Block) -> Vec<Txid>;

    fn accept_to_mempool(&mut self, transaction: Transaction) -> Result<(), MempoolError>;
}
