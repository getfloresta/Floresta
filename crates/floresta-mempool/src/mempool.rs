//! A simple mempool that keeps our transactions in memory. It try to rebroadcast
//! our transactions every 1 hour.
//! Once our transaction is included in a block, we remove it from the mempool.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::time::Duration;
use std::time::Instant;

use bitcoin::block::Header;
use bitcoin::block::Version;
use bitcoin::hashes::Hash;
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
/// An error returned when we try to add a transaction to the mempool.
pub enum AcceptToMempoolError {
    /// Memory usage is too high.
    MemoryUsageTooHigh,

    /// The transaction is conflicting with another transaction in the mempool.
    ConflictingTransaction,

    /// This transaction has duplicated inputs
    DuplicatedInputs,

    /// A validation error happened while consensus checking a transaction
    // TODO(davidson): we might want to make an error type specific for consensus,
    // instead of reusing BlockchainError.
    Consensus(BlockchainError),
}

impl Display for AcceptToMempoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            AcceptToMempoolError::MemoryUsageTooHigh => write!(f, "we are running out of memory"),
            AcceptToMempoolError::ConflictingTransaction => {
                write!(f, "we have another transaction that spends the same input")
            }
            AcceptToMempoolError::DuplicatedInputs => {
                write!(f, "this transaction has duplicated inputs")
            }
            AcceptToMempoolError::Consensus(e) => {
                write!(f, "the transaction failed consensus validation: {e}")
            }
        }
    }
}

impl Error for AcceptToMempoolError {}

impl Mempool {
    /// Creates a new mempool with a given maximum size
    pub fn new(max_mempool_size: usize) -> Mempool {
        let a = rand::random();
        let b = rand::random();
        let c = rand::random();
        let d = rand::random();

        let hasher = ahash::RandomState::with_seeds(a, b, c, d);

        Mempool {
            transactions: HashMap::new(),
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
        for (_, tx) in self.transactions.iter() {
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
                self.transactions
                    .remove(&short_txid)
                    .map(|tx| tx.transaction);

                tx.compute_txid()
            })
            .collect()
    }

    /// Checks if an outpoint is already spent in the mempool.
    ///
    /// This can be used to find conflicts before adding a transaction to the mempool.
    fn is_already_spent(&self, outpoint: &OutPoint) -> bool {
        let short_txid = self.hasher.hash_one(outpoint.txid);
        let Some(tx) = self.transactions.get(&short_txid) else {
            return false;
        };

        tx.children.iter().any(|child| {
            let Some(child_tx) = self.transactions.get(child) else {
                return false;
            };

            child_tx.transaction.input.iter().any(|input| {
                input.previous_output.txid == outpoint.txid
                    && input.previous_output.vout == outpoint.vout
            })
        })
    }

    /// Checks if the transaction doesn't have conflicting inputs or spends the same input twice.
    fn check_for_conflicts(&self, transaction: &Transaction) -> Result<(), AcceptToMempoolError> {
        // check for duplicate inputs
        let inputs = transaction
            .input
            .iter()
            .map(|input| input.previous_output)
            .collect::<BTreeSet<_>>();

        if inputs.len() != transaction.input.len() {
            return Err(AcceptToMempoolError::DuplicatedInputs);
        }

        // Check this transaction doesn't conflict with another transaction in the mempool
        // TODO(davidson): RBF
        for input in transaction.input.iter() {
            if self.is_already_spent(&input.previous_output) {
                return Err(AcceptToMempoolError::ConflictingTransaction);
            }
        }

        Ok(())
    }

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
    pub fn accept_to_mempool(
        &mut self,
        transaction: Transaction,
    ) -> Result<(), AcceptToMempoolError> {
        debug!(
            "Accepting {} to mempool {:?}",
            transaction.compute_txid(),
            self.transactions
        );

        // Make sure our mempool has space
        let tx_size = transaction.total_size();
        if self.mempool_size + tx_size > self.max_mempool_size {
            return Err(AcceptToMempoolError::MemoryUsageTooHigh);
        }

        let short_txid = self.hasher.hash_one(transaction.compute_txid());

        // Checks if we don't have this tx already
        if self.transactions.contains_key(&short_txid) {
            return Ok(());
        }

        // Perform context-free consensus checks
        Consensus::check_transaction_context_free(&transaction)
            .map_err(AcceptToMempoolError::Consensus)?;

        // Make sure transaction won't conflict with other mempool transaction
        self.check_for_conflicts(&transaction)?;

        // List dependants for this transaction
        let depends = self.find_mempool_depends(&transaction);
        for depend in depends.iter() {
            let tx = self.transactions.get_mut(depend).unwrap();
            tx.children.push(short_txid);
        }

        // Insert it into our mempool
        self.transactions.insert(
            short_txid,
            MempoolTransaction {
                time: Instant::now(),
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
    use std::str::FromStr;

    use bitcoin::absolute;
    use bitcoin::block;
    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::OutPoint;
    use bitcoin::Sequence;
    use bitcoin::Target;
    use bitcoin::Transaction;
    use bitcoin::Txid;
    use bitcoin::Witness;
    use floresta_common::bhash;
    use rand::Rng;
    use rand::SeedableRng;

    use super::Mempool;
    use crate::mempool::AcceptToMempoolError;

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
    fn test_mepool_accept() {
        let mut mempool = Mempool::new(10_000_000);

        let transactions = build_transactions(42, false);
        let len = transactions.len();

        for tx in transactions {
            mempool
                .accept_to_mempool(tx)
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
            match mempool.accept_to_mempool(tx) {
                Ok(_) => {}
                Err(AcceptToMempoolError::DuplicatedInputs) => {
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
            .accept_to_mempool(tx)
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
                .accept_to_mempool(tx)
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
}
