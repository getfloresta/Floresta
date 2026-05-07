// SPDX-License-Identifier: MIT OR Apache-2.0

//! A simple mempool that keeps our transactions in memory. It try to rebroadcast
//! our transactions every 1 hour.
//! Once our transaction is included in a block, we remove it from the mempool.

use core::error::Error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;

use bitcoin::block::Header;
use bitcoin::block::Version;
use bitcoin::hashes::Hash;
use bitcoin::Amount;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::CompactTarget;
use bitcoin::OutPoint;
use bitcoin::Transaction;
use bitcoin::TxMerkleNode;
use bitcoin::Txid;
use floresta_chain::pruned_utreexo::consensus::Consensus;
use floresta_chain::BlockchainError;
use tracing::debug;

/// A short transaction id that we use to identify transactions in the mempool.
///
/// We use this to keep track of dependencies between transactions, since keeping the full txid
/// would be too expensive. This value is computed using a keyed hash function, with a local key
/// that only we know. This way, peers can't cause collisions and make our mempool slow.
type ShortTxid = u64;

#[derive(Debug)]
/// A transaction in the mempool.
///
/// This struct holds the transaction itself, the time when we added it to the mempool, the
/// transactions that depend on it, and the transactions that it depends on. We need that extra
/// information to make decisions when to include or not a transaction in mempool or in a block.
struct MempoolTransaction {
    transaction: Transaction,
    /// The fee paid by this transaction in satoshis.
    fee: Amount,
    time: Instant,
    depends: Vec<ShortTxid>,
    children: Vec<ShortTxid>,
}

/// Holds the transactions that we broadcasted and are still in the mempool.
#[derive(Debug)]
pub struct Mempool {
    /// A list of all transactions we currently have in the mempool.
    ///
    /// Transactions are kept as a map of their transaction id to the transaction itself, we
    /// also keep track of when we added the transaction to the mempool to be able to remove
    /// stale transactions.
    transactions: HashMap<ShortTxid, MempoolTransaction>,

    /// Maps every outpoint currently being spent by a mempool transaction to that spender's
    /// [`ShortTxid`].
    spent_outpoints: HashMap<OutPoint, ShortTxid>,

    /// How much memory (in bytes) does the mempool currently use.
    mempool_size: usize,

    /// The maximum size of the mempool in bytes.
    max_mempool_size: usize,

    /// A queue of transaction we know about, but we haven't downloaded yet
    queue: Vec<Txid>,

    /// A hasher that we use to compute the short transaction ids.
    hasher: ahash::RandomState,
}

#[derive(Debug)]
/// Errors that can occur whilst trying to add a transaction to the [`Mempool`].
pub enum MempoolError {
    /// The [`Mempool`] is full and cannot accept more [`Transaction`]s.
    FullMempool,

    /// The [`Transaction`] conflicts with another [`Transaction`] in the [`Mempool`], and the
    /// conflicting transaction does not opt-in to RBF (BIP 125 rule 1).
    ConflictingTransaction,

    /// The [`Transaction`] has duplicate inputs.
    DuplicatedInputs,

    // TODO(davidson): we might want to make an error type specific for consensus,
    // instead of reusing BlockchainError.
    /// The [`Transaction`] failed consensus validation.
    ConsensusValidation(BlockchainError),

    /// The conflicting mempool transaction(s) do not signal opt-in Replace-by-Fee (BIP 125
    /// rule 1).
    RbfNotSignaled,
    InsufficientRbfFee {
        /// Minimum absolute fee the replacement must pay to be accepted.
        required: Amount,
        /// Absolute fee the replacement actually pays.
        provided: Amount,
    },

    /// Evicting the conflicting transactions would require removing more than
    TooManyConflicts(usize),
}

impl Display for MempoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::FullMempool => {
                write!(
                    f,
                    "The mempool is full and cannot accept any more transactions"
                )
            }
            Self::ConflictingTransaction => {
                write!(
                    f,
                    "The transaction conflicts with another transaction in the mempool"
                )
            }
            Self::DuplicatedInputs => {
                write!(f, "The transaction has duplicate inputs")
            }
            Self::ConsensusValidation(e) => {
                write!(f, "The transaction failed consensus validation: {e}")
            }
            Self::RbfNotSignaled => {
                write!(
                    f,
                    "The conflicting mempool transaction does not opt-in to RBF (BIP 125 rule 1)"
                )
            }
            Self::InsufficientRbfFee { required, provided } => {
                write!(
                    f,
                    "Replacement fee {provided} is below the required minimum {required} (BIP 125 rule 3)"
                )
            }
            Self::TooManyConflicts(n) => {
                write!(
                    f,
                    "Evicting the conflicting transactions would remove {n} transactions, \
                     exceeding the BIP 125 rule-5 limit of {}",
                    Mempool::MAX_RBF_CONFLICTS
                )
            }
        }
    }
}

impl Error for MempoolError {}

impl Mempool {
    /// BIP 125 rule 5: maximum number of transactions that may be evicted by a single RBF.
    pub const MAX_RBF_CONFLICTS: usize = 100;

    /// Creates a new mempool with a given maximum size
    pub fn new(max_mempool_size: usize) -> Mempool {
        let a = rand::random();
        let b = rand::random();
        let c = rand::random();
        let d = rand::random();

        let hasher = ahash::RandomState::with_seeds(a, b, c, d);

        Mempool {
            transactions: HashMap::new(),
            spent_outpoints: HashMap::new(),
            queue: Vec::new(),
            mempool_size: 0,
            max_mempool_size,
            hasher,
        }
    }

    /// List transactions we are pending to process.
    pub fn list_unprocessed(&self) -> Vec<Txid> {
        self.queue.clone()
    }

    /// List all transactions we've accepted to the mempool.
    ///
    /// This won't count transactions that are still in the queue.
    pub fn list_mempool(&self) -> Vec<Txid> {
        self.transactions
            .keys()
            .map(|id| self.transactions[id].transaction.compute_txid())
            .collect()
    }

    /// Returns an unsolved block (with nonce 0) with as many transactions as we can fit
    /// into a block (up to max_block_weight).
    pub fn get_block_template(
        &self,
        version: Version,
        prev_blockhash: BlockHash,
        time: u32,
        bits: CompactTarget,
        max_block_weight: u64,
    ) -> Block {
        // add transactions until we reach the block limit
        let mut size = 0;

        let mut txs = Vec::new();
        for tx in self.transactions.values() {
            let tx_size = tx.transaction.weight().to_wu();
            if size + tx_size > max_block_weight {
                break;
            }

            if txs.contains(&tx.transaction) {
                continue;
            }

            size += tx_size;
            let short_txid = self.hasher.hash_one(tx.transaction.compute_txid());
            self.add_transaction_to_block(&mut txs, short_txid);
        }

        let mut block = Block {
            header: Header {
                version,
                prev_blockhash,
                merkle_root: TxMerkleNode::all_zeros(),
                time,
                bits,
                nonce: 0,
            },
            txdata: txs,
        };

        block.header.merkle_root = block.compute_merkle_root().unwrap();
        block
    }

    /// Utility method that grabs one transaction and all its dependencies, then adds them to a tx
    /// list.
    fn add_transaction_to_block(
        &self,
        block_transactions: &mut Vec<Transaction>,
        short_txid: ShortTxid,
    ) {
        let transaction = self.transactions.get(&short_txid).unwrap();
        if block_transactions.contains(&transaction.transaction) {
            return;
        }

        let depends_on = transaction.depends.clone();

        for depend in depends_on {
            self.add_transaction_to_block(block_transactions, depend);
        }

        block_transactions.push(transaction.transaction.clone());
    }

    /// Consume a block and remove all transactions that were included in it.
    pub fn consume_block(&mut self, block: &Block) -> Vec<Txid> {
        block
            .txdata
            .iter()
            .map(|tx| {
                let short_txid = self.hasher.hash_one(tx.compute_txid());

                // Remove this transaction from the mempool, and also remove it from the depends list of all
                // its children, since they don't depend on it anymore.
                if let Some(removed) = self.transactions.remove(&short_txid) {
                    self.mempool_size -= removed.transaction.total_size();

                    // Remove all spent-outpoint index entries for this transaction.
                    for input in &removed.transaction.input {
                        self.spent_outpoints.remove(&input.previous_output);
                    }

                    for child in &removed.children {
                        if let Some(child_tx) = self.transactions.get_mut(child) {
                            child_tx.depends.retain(|depend| *depend != short_txid);
                        }
                    }
                }
                tx.compute_txid()
            })
            .collect()
    }

    /// Collects `roots` and every descendant reachable through the `children` graph.
    fn collect_all_descendants(&self, roots: &[ShortTxid]) -> Vec<ShortTxid> {
        let mut all = roots.to_vec();
        let mut i = 0;
        while i < all.len() {
            let id = all[i];
            if let Some(tx) = self.transactions.get(&id) {
                for &child in &tx.children {
                    if !all.contains(&child) {
                        all.push(child);
                    }
                }
            }
            i += 1;
        }
        all
    }

    /// Evicts a set of transactions (and their full descendant subtrees) from the mempool,
    fn evict_with_descendants(&mut self, roots: &[ShortTxid]) {
        let to_evict = self.collect_all_descendants(roots);

        for id in &to_evict {
            if let Some(removed) = self.transactions.remove(id) {
                self.mempool_size -= removed.transaction.total_size();

                // Clean spent-outpoint index.
                for input in &removed.transaction.input {
                    self.spent_outpoints.remove(&input.previous_output);
                }

                // Remove this tx from its mempool parents' children lists.
                for parent_id in &removed.depends {
                    if let Some(parent) = self.transactions.get_mut(parent_id) {
                        parent.children.retain(|c| c != id);
                    }
                }
            }
        }
    }

    /// Validates `transaction` against the current mempool state and returns the set of
    /// [`ShortTxid`]s that must be evicted to make room for it (empty when there are no
    /// conflicts).
    fn check_for_conflicts(
        &self,
        transaction: &Transaction,
        fee: Amount,
    ) -> Result<Vec<ShortTxid>, MempoolError> {
        // Reject transactions with duplicate inputs.
        let unique_inputs = transaction
            .input
            .iter()
            .map(|i| i.previous_output)
            .collect::<BTreeSet<_>>();
        if unique_inputs.len() != transaction.input.len() {
            return Err(MempoolError::DuplicatedInputs);
        }

        // Collect the set of directly-conflicting transactions.
        let mut direct_conflicts: Vec<ShortTxid> = Vec::new();
        for input in &transaction.input {
            if let Some(&conflict_id) = self.spent_outpoints.get(&input.previous_output) {
                if !direct_conflicts.contains(&conflict_id) {
                    direct_conflicts.push(conflict_id);
                }
            }
        }

        if direct_conflicts.is_empty() {
            return Ok(Vec::new());
        }

        // BIP 125 rule 1: every directly-conflicting transaction must opt-in to RBF.
        for &id in &direct_conflicts {
            let conflict = self
                .transactions
                .get(&id)
                .expect("spent_outpoints entry must correspond to a mempool transaction");
            if !conflict.transaction.is_explicitly_rbf() {
                return Err(MempoolError::RbfNotSignaled);
            }
        }

        // Collect the full eviction set (direct conflicts + all their descendants).
        let to_evict = self.collect_all_descendants(&direct_conflicts);

        // BIP 125 rule 5: limit the eviction blast radius.
        if to_evict.len() > Self::MAX_RBF_CONFLICTS {
            return Err(MempoolError::TooManyConflicts(to_evict.len()));
        }

        // BIP 125 rule 3: replacement must pay more than the directly conflicting transactions.
        let conflicting_fee_total: Amount = direct_conflicts
            .iter()
            .filter_map(|id| self.transactions.get(id))
            .try_fold(Amount::ZERO, |acc, tx| acc.checked_add(tx.fee))
            .ok_or(MempoolError::InsufficientRbfFee {
                required: Amount::MAX,
                provided: fee,
            })?;

        let fee_check_required = fee > Amount::ZERO || conflicting_fee_total > Amount::ZERO;
        if fee_check_required && fee <= conflicting_fee_total {
            return Err(MempoolError::InsufficientRbfFee {
                required: conflicting_fee_total
                    .checked_add(Amount::from_sat(1))
                    .unwrap_or(Amount::MAX),
                provided: fee,
            });
        }

        Ok(to_evict)
    }

    /// Accepts a transaction to mempool.
    ///
    /// This method will perform some context-less validations on a transaction,
    /// and then accept it to our mempool. It assumes that we have validated this transaction's
    /// proof.
    ///
    /// # Errors
    ///  - If we don't have space left in our mempool
    ///  - If the transaction conflicts with another mempool transaction that does not opt-in
    ///    to RBF, or does not pay a higher fee ([`MempoolError::RbfNotSignaled`] /
    ///    [`MempoolError::InsufficientRbfFee`])
    ///  - If it spends the same input twice
    ///  - If any amount check fails: if input amounts are less than output amounts or if it
    ///    spends more than the theoretical maximum amount of Bitcoins
    ///  - If either vIn or vOut are empty
    ///  - If any script is larger than the maximum allowed size
    pub fn accept_to_mempool(
        &mut self,
        transaction: Transaction,
        fee: Amount,
    ) -> Result<(), MempoolError> {
        debug!(
            "Accepting {} to mempool {:?}",
            transaction.compute_txid(),
            self.transactions
        );

        // Make sure our mempool has space
        let tx_size = transaction.total_size();
        if self.mempool_size + tx_size > self.max_mempool_size {
            return Err(MempoolError::FullMempool);
        }

        let short_txid = self.hasher.hash_one(transaction.compute_txid());

        // Checks if we don't have this tx already
        if self.transactions.contains_key(&short_txid) {
            return Ok(());
        }

        // Perform context-free consensus checks
        Consensus::check_transaction_context_free(&transaction)
            .map_err(MempoolError::ConsensusValidation)?;

        // Check for conflicts and compute the eviction set (may be empty if no conflicts).
        let to_evict = self.check_for_conflicts(&transaction, fee)?;

        // Evict conflicting transactions (and their descendants) before inserting the
        // replacement.
        if !to_evict.is_empty() {
            self.evict_with_descendants(&to_evict);
        }
        let depends = self.find_mempool_depends(&transaction);
        for depend in depends.iter() {
            let tx = self.transactions.get_mut(depend).unwrap();
            tx.children.push(short_txid);
        }

        // Register all inputs in the spent-outpoints index.
        for input in &transaction.input {
            self.spent_outpoints.insert(input.previous_output, short_txid);
        }

        // Insert it into our mempool.
        self.transactions.insert(
            short_txid,
            MempoolTransaction {
                time: Instant::now(),
                fee,
                depends,
                transaction,
                children: Vec::new(),
            },
        );
        self.mempool_size += tx_size;

        Ok(())
    }

    /// From a transaction that is already in the mempool, computes which transaction it depends.
    fn find_mempool_depends(&self, tx: &Transaction) -> Vec<ShortTxid> {
        tx.input
            .iter()
            .filter_map(|input| {
                let short_txid = self.hasher.hash_one(input.previous_output.txid);
                self.transactions.get(&short_txid).map(|_| short_txid)
            })
            .collect()
    }

    /// Get a transaction from the mempool.
    pub fn get_from_mempool<'a>(&'a self, id: &Txid) -> Option<&'a Transaction> {
        let id = self.hasher.hash_one(id);
        self.transactions.get(&id).map(|tx| &tx.transaction)
    }

    /// Get all transactions that were in the mempool for more than 1 hour, if any
    pub fn get_stale(&mut self) -> Vec<Txid> {
        self.transactions
            .values()
            .filter_map(|tx| {
                let txid = tx.transaction.compute_txid();
                match tx.time.elapsed() > Duration::from_secs(3600) {
                    true => Some(txid),
                    false => None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use bitcoin::absolute;
    use bitcoin::block::Header;
    use bitcoin::block::{self};
    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version;
    use bitcoin::Amount;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::OutPoint;
    use bitcoin::Script;
    use bitcoin::Sequence;
    use bitcoin::Target;
    use bitcoin::Transaction;
    use bitcoin::TxIn;
    use bitcoin::TxMerkleNode;
    use bitcoin::TxOut;
    use bitcoin::Txid;
    use bitcoin::Witness;
    use floresta_common::bhash;
    use rand::Rng;
    use rand::SeedableRng;

    use super::Mempool;
    use crate::mempool::MempoolError;

    /// Build a simple transaction spending a single outpoint and producing a single output.
    fn make_tx(input: OutPoint, output_value: Amount, sequence: Sequence) -> Transaction {
        Transaction {
            version: Version::ONE,
            lock_time: absolute::LockTime::from_consensus(0),
            input: vec![TxIn {
                previous_output: input,
                script_sig: Script::new().into(),
                sequence,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: output_value,
                script_pubkey: Script::from_bytes(&[]).into(),
            }],
        }
    }

    /// builds a list of transactions in a pseudo-random way
    ///
    /// We use those transactions in mempool tests
    fn build_transactions(seed: u64, conflict: bool) -> Vec<Transaction> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut transactions = Vec::new();

        let n = rng.gen_range(1..10);
        let mut outputs = Vec::new();

        // This output is used as a dummy input for the first transactions, since
        // we are not allowed to have coinbase transactions in our mempool-created blocks.
        let dummy_input = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };

        outputs.push(dummy_input);

        for _ in 0..n {
            let mut tx = bitcoin::Transaction {
                version: Version::ONE,
                lock_time: absolute::LockTime::from_consensus(0),
                input: Vec::new(),
                output: Vec::new(),
            };

            let inputs = rng.gen_range(1..10);
            for _ in 0..inputs {
                if outputs.is_empty() {
                    break;
                }

                let index = rng.gen_range(0..outputs.len());
                let previous_output: OutPoint = match conflict {
                    false => outputs.remove(index),
                    true => *outputs.get(index).unwrap(),
                };

                let input = bitcoin::TxIn {
                    previous_output,
                    script_sig: bitcoin::Script::new().into(),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                };

                tx.input.push(input);
            }

            let n = rng.gen_range(1..10);

            for _ in 0..n {
                let script = rng.gen::<[u8; 32]>();
                let output = bitcoin::TxOut {
                    value: bitcoin::Amount::from_sat(rng.gen_range(0..100_000_000)),
                    script_pubkey: bitcoin::Script::from_bytes(&script).into(),
                };

                tx.output.push(output);
            }

            outputs.extend(tx.output.iter().enumerate().map(|(vout, _)| OutPoint {
                txid: tx.compute_txid(),
                vout: vout as u32,
            }));

            transactions.push(tx);
        }

        transactions
    }

    #[test]
    fn test_random() {
        // just sanity check for build_transactions
        let transactions = build_transactions(42, true);
        assert!(!transactions.is_empty());

        let transactions2 = build_transactions(42, true);
        assert!(!transactions2.is_empty());
        assert_eq!(transactions, transactions2);

        let transactions3 = build_transactions(43, true);
        assert!(!transactions3.is_empty());
        assert_ne!(transactions, transactions3);
    }

    #[test]
    fn test_mempool_accept() {
        let mut mempool = Mempool::new(10_000_000);

        let transactions = build_transactions(42, false);
        let len = transactions.len();

        for tx in transactions {
            mempool
                .accept_to_mempool(tx, Amount::ZERO)
                .expect("failed to accept to mempool");
        }

        assert_eq!(mempool.transactions.len(), len);
    }

    #[test]
    fn test_gbt_with_conflict() {
        let mut mempool = Mempool::new(10_000_000);
        let transactions = build_transactions(21, true);
        let mut did_conflict = false;

        for tx in transactions {
            match mempool.accept_to_mempool(tx, Amount::ZERO) {
                Ok(_) => {}
                // Both intra-transaction duplicates and cross-transaction conflicts on
                // transactions that don't signal RBF are expected.
                Err(MempoolError::DuplicatedInputs) | Err(MempoolError::RbfNotSignaled) => {
                    did_conflict = true;
                }
                Err(e) => {
                    panic!("unexpected error: {:?}", e);
                }
            }
        }

        // we expect at least one conflict
        assert!(did_conflict);

        let target = Target::MAX_ATTAINABLE_REGTEST;
        let block = mempool.get_block_template(
            block::Version::ONE,
            bitcoin::BlockHash::all_zeros(),
            0,
            target.to_compact_lossy(),
            4_000_000,
        );

        assert!(block.check_merkle_root());

        // we can't really call check_block_transactions here, because the conflict logic only
        // looks for inputs that are presently on mempool.
        //
        // To fix this, we need to add proof verification to mempool acceptance, so that we can
        // know which inputs are actually valid.
    }

    fn check_block_transactions(block: Block) {
        // make sure that all outputs are spent after being created, and only once
        let mut outputs = HashSet::new();

        // This output is used as a dummy input for the first transactions, since
        // we are not allowed to have coinbase transactions in our mempool-created blocks.
        let dummy_input = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };
        outputs.insert(dummy_input);

        for tx in block.txdata.iter() {
            for input in tx.input.iter() {
                if input.previous_output.txid == bitcoin::Txid::all_zeros() {
                    continue;
                }

                assert!(
                    outputs.remove(&input.previous_output),
                    "input {input:?} missing or double spent"
                );
            }

            let txid = tx.compute_txid();
            for (vout, _) in tx.output.iter().enumerate() {
                let output = OutPoint {
                    txid,
                    vout: vout as u32,
                };
                outputs.insert(output);
            }
        }
    }

    #[test]
    fn test_gbt_first_transaction() {
        // this test will recreate the network state on block 269, and then submit the famous
        // first non-coinbase transaction to mempool. Then create a block template,
        // "mines" it, and then consumes the block. After that, we'll have a network at
        // block 270, with the transaction confirmed.

        let mut mempool = Mempool::new(10_000_000);
        let tx_hex = "0100000001c997a5e56e104102fa209c6a852dd90660a20b2d9c352423edce25857fcd3704000000004847304402204e45e16932b8af514961a1d3a1a25fdf3f4f7732e9d624c6c61548ab5fb8cd410220181522ec8eca07de4860a4acdd12909d831cc56cbbac4622082221a8768d1d0901ffffffff0200ca9a3b00000000434104ae1a62fe09c5f51b13905f07f06b99a2f7159b2225f374cd378d71302fa28414e7aab37397f554a7df5f142c21c1b7303b8a0626f1baded5c72a704f7e6cd84cac00286bee0000000043410411db93e1dcdb8a016b49840f8c53bc1eb68a382e97b1482ecad7b148a6909a5cb2e0eaddfb84ccf9744464f82e160bfa9b8b64f9d4c03f999b8643f656b412a3ac00000000";
        let tx: Transaction = deserialize_hex(tx_hex).unwrap();

        mempool
            .accept_to_mempool(tx, Amount::ZERO)
            .expect("failed to accept to mempool");

        let block = mempool.get_block_template(
            block::Version::ONE,
            bhash!("000000002a22cfee1f2c846adbd12b3e183d4f97683f85dad08a79780a84bd55"),
            1231731025,
            Target::MAX_ATTAINABLE_MAINNET.to_compact_lossy(),
            4_000_000,
        );

        assert_eq!(block.txdata.len(), 1);
        assert!(block.check_merkle_root());
    }

    #[test]
    fn test_gbt() {
        let mut mempool = Mempool::new(10_000_000);

        let transactions = build_transactions(42, false);
        let len = transactions.len();

        for tx in transactions {
            mempool
                .accept_to_mempool(tx, Amount::ZERO)
                .expect("failed to accept to mempool");
        }

        let target = Target::MAX_ATTAINABLE_REGTEST;
        let block = mempool.get_block_template(
            block::Version::ONE,
            bitcoin::BlockHash::all_zeros(),
            0,
            target.to_compact_lossy(),
            4_000_000,
        );

        assert_eq!(block.txdata.len(), len);
        assert!(block.check_merkle_root());

        check_block_transactions(block);
    }

    #[test]
    fn test_consume_block_updates_mempool_size() {
        let mut mempool = Mempool::new(10_000_000);

        let transactions = build_transactions(15, false);
        for tx in transactions {
            mempool
                .accept_to_mempool(tx, Amount::ZERO)
                .expect("failed to accept to mempool");
        }

        let size_before_consume = mempool.mempool_size;

        let target = Target::MAX_ATTAINABLE_REGTEST;
        let block = mempool.get_block_template(
            block::Version::ONE,
            BlockHash::all_zeros(),
            0,
            target.to_compact_lossy(),
            4_000_000,
        );

        mempool.consume_block(&block);

        assert_eq!(
            mempool.mempool_size, 0,
            "mempool_size was {} before consume_block and it is {} after but it should be 0",
            size_before_consume, mempool.mempool_size
        );
    }

    #[test]
    // Tests that when we consume a block, transactions that depended on the transactions
    // included in the block should no longer reference them, since they are now confirmed.
    fn test_consume_block_removes_depends() {
        let mut mempool = Mempool::new(10_000_000);

        let parent = Transaction {
            version: Version::ONE,
            lock_time: absolute::LockTime::from_consensus(0),
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::all_zeros(),
                    vout: 0,
                },
                script_sig: Script::new().into(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: Script::from_bytes(&[]).into(),
            }],
        };
        let parent_txid = parent.compute_txid();

        let child = Transaction {
            version: Version::ONE,
            lock_time: absolute::LockTime::from_consensus(0),
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: parent_txid,
                    vout: 0,
                },
                script_sig: Script::new().into(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(49_000),
                script_pubkey: Script::from_bytes(&[]).into(),
            }],
        };
        let child_txid = child.compute_txid();

        mempool.accept_to_mempool(parent.clone(), Amount::ZERO).unwrap();
        mempool.accept_to_mempool(child, Amount::ZERO).unwrap();

        // Sanity check: child currently depends on parent
        let parent_short_txid = mempool.hasher.hash_one(parent_txid);
        let child_short_txid = mempool.hasher.hash_one(child_txid);
        assert!(mempool.transactions[&child_short_txid]
            .depends
            .contains(&parent_short_txid));

        let block = Block {
            header: Header {
                version: block::Version::ONE,
                prev_blockhash: BlockHash::all_zeros(),
                merkle_root: TxMerkleNode::all_zeros(),
                time: 0,
                bits: Target::MAX_ATTAINABLE_REGTEST.to_compact_lossy(),
                nonce: 0,
            },
            txdata: vec![parent],
        };
        mempool.consume_block(&block);

        assert!(!mempool.transactions.contains_key(&parent_short_txid));
        assert!(!mempool.transactions[&child_short_txid]
            .depends
            .contains(&parent_short_txid));
    }

    /// A confirmed UTXO that both the original and replacement transactions will spend.
    fn confirmed_utxo() -> OutPoint {
        OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        }
    }

    #[test]
    fn test_rbf_rejected_when_not_signaled() {
        // The original transaction uses Sequence::MAX (no RBF signal).
        let mut mempool = Mempool::new(10_000_000);

        let utxo = confirmed_utxo();
        let original = make_tx(utxo, Amount::from_sat(49_000), Sequence::MAX);
        mempool
            .accept_to_mempool(original, Amount::from_sat(1_000))
            .expect("original accepted");

        let replacement = make_tx(utxo, Amount::from_sat(48_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let err = mempool
            .accept_to_mempool(replacement, Amount::from_sat(2_000))
            .expect_err("replacement should be rejected");

        assert!(
            matches!(err, MempoolError::RbfNotSignaled),
            "expected RbfNotSignaled, got {err:?}"
        );
    }

    #[test]
    fn test_rbf_accepted_with_higher_fee() {
        // The original transaction opts in to RBF via Sequence::ENABLE_RBF_NO_LOCKTIME.
        let mut mempool = Mempool::new(10_000_000);

        let utxo = confirmed_utxo();
        let original = make_tx(utxo, Amount::from_sat(49_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let original_txid = original.compute_txid();
        mempool
            .accept_to_mempool(original, Amount::from_sat(1_000))
            .expect("original accepted");

        let replacement =
            make_tx(utxo, Amount::from_sat(47_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let replacement_txid = replacement.compute_txid();
        mempool
            .accept_to_mempool(replacement, Amount::from_sat(3_000))
            .expect("replacement should be accepted");

        // Original must be gone; replacement must be present.
        assert!(
            mempool.get_from_mempool(&original_txid).is_none(),
            "original should have been evicted"
        );
        assert!(
            mempool.get_from_mempool(&replacement_txid).is_some(),
            "replacement should be in mempool"
        );
        // Spent-outpoint index must point to the replacement.
        assert_eq!(
            mempool.spent_outpoints.get(&utxo).copied(),
            Some(mempool.hasher.hash_one(replacement_txid))
        );
    }

    #[test]
    fn test_rbf_rejected_with_insufficient_fee() {
        // The replacement is a distinct transaction (different output value) but declares
        // the same fee as the original; it must be rejected.
        let mut mempool = Mempool::new(10_000_000);

        let utxo = confirmed_utxo();
        let original = make_tx(utxo, Amount::from_sat(49_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        mempool
            .accept_to_mempool(original, Amount::from_sat(1_000))
            .expect("original accepted");

        // Different output value so the txid differs from the original, but same declared fee.
        let replacement =
            make_tx(utxo, Amount::from_sat(48_500), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let err = mempool
            .accept_to_mempool(replacement, Amount::from_sat(1_000))
            .expect_err("replacement with equal fee should be rejected");

        assert!(
            matches!(err, MempoolError::InsufficientRbfFee { .. }),
            "expected InsufficientRbfFee, got {err:?}"
        );
    }

    #[test]
    fn test_rbf_evicts_descendants() {
        // TX A (RBF-signaling) → TX B (child of A).
        let mut mempool = Mempool::new(10_000_000);

        let utxo = confirmed_utxo();
        let tx_a = make_tx(utxo, Amount::from_sat(49_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let tx_a_txid = tx_a.compute_txid();
        let tx_a_out = OutPoint {
            txid: tx_a_txid,
            vout: 0,
        };
        mempool
            .accept_to_mempool(tx_a, Amount::from_sat(1_000))
            .expect("TX A accepted");

        let tx_b = make_tx(tx_a_out, Amount::from_sat(48_000), Sequence::MAX);
        let tx_b_txid = tx_b.compute_txid();
        mempool
            .accept_to_mempool(tx_b, Amount::from_sat(1_000))
            .expect("TX B accepted");

        // Replace A with a higher-fee transaction.
        let replacement = make_tx(utxo, Amount::from_sat(46_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let replacement_txid = replacement.compute_txid();
        mempool
            .accept_to_mempool(replacement, Amount::from_sat(4_000))
            .expect("replacement accepted");

        assert!(
            mempool.get_from_mempool(&tx_a_txid).is_none(),
            "TX A should be evicted"
        );
        assert!(
            mempool.get_from_mempool(&tx_b_txid).is_none(),
            "TX B (child of A) should also be evicted"
        );
        assert!(
            mempool.get_from_mempool(&replacement_txid).is_some(),
            "replacement should be in mempool"
        );
        // The original UTXOs should no longer be registered as spent by A or B.
        assert!(!mempool.spent_outpoints.contains_key(&tx_a_out));
    }

    #[test]
    fn test_rbf_zero_fee_fallback() {
        // When neither side provides fee information (both Amount::ZERO), RBF is still
        // allowed if the conflicting transaction opts in.
        let mut mempool = Mempool::new(10_000_000);

        let utxo = confirmed_utxo();
        let original = make_tx(utxo, Amount::from_sat(49_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let original_txid = original.compute_txid();
        mempool
            .accept_to_mempool(original, Amount::ZERO)
            .expect("original accepted with no fee info");

        let replacement =
            make_tx(utxo, Amount::from_sat(48_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let replacement_txid = replacement.compute_txid();
        mempool
            .accept_to_mempool(replacement, Amount::ZERO)
            .expect("replacement accepted via zero-fee fallback");

        assert!(mempool.get_from_mempool(&original_txid).is_none());
        assert!(mempool.get_from_mempool(&replacement_txid).is_some());
    }

    #[test]
    fn test_rbf_conflict_detected_for_confirmed_input() {
        // Two mempool transactions spending the same *confirmed* UTXO must conflict.
        let mut mempool = Mempool::new(10_000_000);

        let utxo = confirmed_utxo();

        // TX1 uses Sequence::MAX (no RBF opt-in).
        let tx1 = make_tx(utxo, Amount::from_sat(49_000), Sequence::MAX);
        mempool
            .accept_to_mempool(tx1, Amount::from_sat(1_000))
            .expect("TX1 accepted");

        // TX2 attempts to spend the same confirmed UTXO — must be rejected.
        let tx2 = make_tx(utxo, Amount::from_sat(48_000), Sequence::ENABLE_RBF_NO_LOCKTIME);
        let err = mempool
            .accept_to_mempool(tx2, Amount::from_sat(2_000))
            .expect_err("TX2 must conflict with TX1");

        // TX1 doesn't signal RBF, so we expect RbfNotSignaled.
        assert!(
            matches!(err, MempoolError::RbfNotSignaled),
            "expected RbfNotSignaled, got {err:?}"
        );
    }

    #[test]
    fn test_spent_outpoints_cleared_after_consume_block() {
        // After a block confirms the mempool transaction, spent_outpoints must be empty.
        let mut mempool = Mempool::new(10_000_000);

        let utxo = confirmed_utxo();
        let tx = make_tx(utxo, Amount::from_sat(49_000), Sequence::MAX);
        let txid = tx.compute_txid();
        mempool
            .accept_to_mempool(tx.clone(), Amount::ZERO)
            .expect("tx accepted");

        assert!(mempool.spent_outpoints.contains_key(&utxo));

        let block = Block {
            header: Header {
                version: block::Version::ONE,
                prev_blockhash: BlockHash::all_zeros(),
                merkle_root: TxMerkleNode::all_zeros(),
                time: 0,
                bits: Target::MAX_ATTAINABLE_REGTEST.to_compact_lossy(),
                nonce: 0,
            },
            txdata: vec![tx],
        };
        mempool.consume_block(&block);

        assert!(mempool.get_from_mempool(&txid).is_none());
        assert!(!mempool.spent_outpoints.contains_key(&utxo));
    }
}
