// SPDX-License-Identifier: MIT OR Apache-2.0

use bitcoin::block::Version;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::CompactTarget;
use bitcoin::Transaction;
use bitcoin::Txid;

use super::error::MempoolError;

/// An abstract interface for interacting with a Mempool.
///
/// This trait decouples consumers from a concrete Mempool implementation,
/// allowing other crates to depend on this domain-level abstraction
/// rather than on the `floresta-mempool` crate directly.
///
/// `: Send` means any implementor can be wrapped in an envelope,
/// and its ownership handed off among threads.
pub trait MempoolBase: Send {
    /// Returns all transactions accepted to the Mempool.
    fn list_mempool(&self) -> Vec<Txid>;

    /// Returns an unsolved block (with nonce 0) with as many transactions as we can fit
    /// into a block (up to max_block_weight).
    fn get_block_template(
        &self,
        version: Version,
        prev_blockhash: BlockHash,
        time: u32,
        bits: CompactTarget,
        max_block_weight: u64,
    ) -> Block;

    /// Get a transaction from the mempool.
    fn get_from_mempool<'a>(&'a self, id: &Txid) -> Option<&'a Transaction>;

    /// Get all transactions that were in the mempool for more than 1 hour, if any
    fn get_stale(&mut self) -> Vec<Txid>;

    /// Consume a block and remove all transactions that were included in it.
    fn consume_block(&mut self, block: &Block) -> Vec<Txid>;

    /// Accepts a transaction to mempool
    ///
    /// This method will perform some context-less validations on a transaction,
    /// and then accept to our mempool. It assumes that we have validated this transaction's
    /// proof.
    ///
    /// # Errors
    ///  - If we don't have space left in our mempool
    ///  - If the transaction conflicts with another mempool transaction
    ///  - If it sepends the same input twice
    ///  - If any amount check fails: if input amounts are less than output amounts or if it spends more than
    ///    the theoretical maximum amount of Bitcoins
    ///  - If either vIn or vOut are empty
    ///  - If any script is larger than the maximum allowed size
    fn accept_to_mempool(&mut self, transaction: Transaction) -> Result<(), MempoolError>;
}
