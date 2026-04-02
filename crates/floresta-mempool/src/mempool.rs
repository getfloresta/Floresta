// SPDX-License-Identifier: MIT OR Apache-2.0

//! A simple mempool that keeps our transactions in memory. It try to rebroadcast
//! our transactions every 1 hour.
//! Once our transaction is included in a block, we remove it from the mempool.

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
use floresta_chain::pruned_utreexo::error::BlockValidationErrors;
use floresta_chain::pruned_utreexo::error::TransactionError;
use floresta_chain::pruned_utreexo::utxo_data::UtxoData;
use floresta_chain::BlockchainError;
use tracing::debug;

use crate::MempoolError;
use crate::MempoolPolicy;

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

    /// Relay policy enforced during admission.
    policy: MempoolPolicy,

    /// A queue of transaction we know about, but we haven't downloaded yet
    queue: Vec<Txid>,

    /// A hasher that we use to compute the short transaction ids.
    hasher: ahash::RandomState,
}

impl Mempool {
    /// Creates a new mempool with the default relay policy.
    pub fn new(max_mempool_size: usize) -> Mempool {
        Self::with_policy(max_mempool_size, MempoolPolicy::default())
    }

    /// Creates a new mempool with a given maximum size and relay policy.
    pub fn with_policy(max_mempool_size: usize, policy: MempoolPolicy) -> Mempool {
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
            policy,
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
                if let Some(tx) = self.transactions.remove(&short_txid) {
                    self.mempool_size = self
                        .mempool_size
                        .saturating_sub(tx.transaction.total_size());
                }

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
    fn check_for_conflicts(&self, transaction: &Transaction) -> Result<(), MempoolError> {
        let inputs = transaction
            .input
            .iter()
            .map(|input| input.previous_output)
            .collect::<BTreeSet<_>>();

        if inputs.len() != transaction.input.len() {
            return Err(MempoolError::DuplicatedInputs);
        }

        // TODO(davidson): RBF
        for input in &transaction.input {
            if self.is_already_spent(&input.previous_output) {
                return Err(MempoolError::ConflictingTransaction);
            }
        }

        Ok(())
    }

    fn check_fee_floor(
        &self,
        transaction: &Transaction,
        spent_utxos: &HashMap<OutPoint, UtxoData>,
    ) -> Result<(), MempoolError> {
        let fee = self.get_fee(transaction, spent_utxos)?;
        let min_fee =
            Amount::from_sat(self.policy.min_relay_fee_sat_per_vbyte * transaction.vsize() as u64);

        if fee < min_fee {
            return Err(MempoolError::FeeTooLow);
        }

        Ok(())
    }

    fn get_fee(
        &self,
        transaction: &Transaction,
        spent_utxos: &HashMap<OutPoint, UtxoData>,
    ) -> Result<Amount, MempoolError> {
        let txid = transaction.compute_txid();
        let mut in_value = Amount::ZERO;
        let out_value = transaction
            .output
            .iter()
            .try_fold(Amount::ZERO, |acc, output| acc.checked_add(output.value))
            .ok_or_else(Self::too_many_coins_error)?;

        if out_value > Amount::MAX_MONEY {
            return Err(Self::too_many_coins_error());
        }

        for input in &transaction.input {
            let txout = &spent_utxos
                .get(&input.previous_output)
                .ok_or(MempoolError::MissingPrevoutContext)?
                .txout;

            in_value = in_value
                .checked_add(txout.value)
                .ok_or_else(Self::too_many_coins_error)?;
        }

        if in_value > Amount::MAX_MONEY {
            return Err(Self::too_many_coins_error());
        }

        in_value
            .checked_sub(out_value)
            .ok_or_else(|| Self::not_enough_money_error(txid))
    }

    fn too_many_coins_error() -> MempoolError {
        MempoolError::Consensus(BlockchainError::BlockValidation(
            BlockValidationErrors::TooManyCoins,
        ))
    }

    fn not_enough_money_error(txid: Txid) -> MempoolError {
        MempoolError::Consensus(BlockchainError::TransactionError(TransactionError {
            txid,
            error: BlockValidationErrors::NotEnoughMoney,
        }))
    }

    fn check_weight(&self, transaction: &Transaction) -> Result<(), MempoolError> {
        if transaction.weight().to_wu() > self.policy.max_standard_tx_weight {
            return Err(MempoolError::ExceedsMaxWeight);
        }

        Ok(())
    }

    fn check_script_sig_size(&self, transaction: &Transaction) -> Result<(), MempoolError> {
        if transaction
            .input
            .iter()
            .any(|input| input.script_sig.len() > self.policy.max_standard_script_sig_size)
        {
            return Err(MempoolError::ExceedsScriptSigSize);
        }

        Ok(())
    }

    fn check_witness_standardness(&self, transaction: &Transaction) -> Result<(), MempoolError> {
        for input in &transaction.input {
            let witness_len = input.witness.len();
            if witness_len > self.policy.max_standard_witness_stack_items {
                return Err(MempoolError::NonStandard);
            }

            if witness_len == 0 {
                continue;
            }

            for item in input.witness.iter().take(witness_len.saturating_sub(1)) {
                if item.len() > self.policy.max_standard_witness_stack_item_size {
                    return Err(MempoolError::NonStandard);
                }
            }

            if let Some(last) = input.witness.iter().last() {
                if last.len() > self.policy.max_standard_witness_script_size {
                    return Err(MempoolError::NonStandard);
                }
            }
        }

        Ok(())
    }

    fn check_standard_outputs(&self, transaction: &Transaction) -> Result<(), MempoolError> {
        if transaction.output.iter().all(|output| {
            output.script_pubkey.is_p2pkh()
                || output.script_pubkey.is_p2sh()
                || output.script_pubkey.is_p2wpkh()
                || output.script_pubkey.is_p2wsh()
        }) {
            return Ok(());
        }

        Err(MempoolError::NonStandard)
    }

    fn resolve_mempool_prevout(&self, outpoint: &OutPoint) -> Option<UtxoData> {
        let short_txid = self.hasher.hash_one(outpoint.txid);
        let tx = self.transactions.get(&short_txid)?;
        let txout = tx.transaction.output.get(outpoint.vout as usize)?.clone();

        Some(UtxoData {
            txout,
            is_coinbase: false,
            creation_height: 0,
            creation_time: 0,
        })
    }

    fn resolve_spent_utxos(
        &self,
        transaction: &Transaction,
        spent_utxos: &HashMap<OutPoint, UtxoData>,
    ) -> Result<HashMap<OutPoint, UtxoData>, MempoolError> {
        let mut resolved = HashMap::with_capacity(transaction.input.len());

        for input in &transaction.input {
            let outpoint = input.previous_output;
            if let Some(utxo) = spent_utxos.get(&outpoint) {
                resolved.insert(outpoint, utxo.clone());
                continue;
            }

            if let Some(utxo) = self.resolve_mempool_prevout(&outpoint) {
                resolved.insert(outpoint, utxo);
                continue;
            }

            return Err(MempoolError::MissingPrevoutContext);
        }

        Ok(resolved)
    }

    /// Accepts a transaction to mempool
    ///
    /// This method will perform context-free consensus checks and relay-policy checks before
    /// accepting a transaction to the mempool. The caller must provide prevout metadata for any
    /// input that is not already backed by a mempool parent transaction.
    pub fn accept_to_mempool(
        &mut self,
        transaction: Transaction,
        spent_utxos: HashMap<OutPoint, UtxoData>,
    ) -> Result<(), MempoolError> {
        debug!(
            "Accepting {} to mempool {:?}",
            transaction.compute_txid(),
            self.transactions
        );

        let short_txid = self.hasher.hash_one(transaction.compute_txid());

        if self.transactions.contains_key(&short_txid) {
            return Err(MempoolError::AlreadyKnown);
        }

        let tx_size = transaction.total_size();
        if self.mempool_size + tx_size > self.max_mempool_size {
            return Err(MempoolError::MemoryUsageTooHigh);
        }

        Consensus::check_transaction_context_free(&transaction).map_err(MempoolError::Consensus)?;
        self.check_for_conflicts(&transaction)?;

        let spent_utxos = self.resolve_spent_utxos(&transaction, &spent_utxos)?;
        self.check_fee_floor(&transaction, &spent_utxos)?;
        self.check_weight(&transaction)?;
        self.check_script_sig_size(&transaction)?;
        self.check_witness_standardness(&transaction)?;
        self.check_standard_outputs(&transaction)?;

        let depends = self.find_mempool_depends(&transaction);
        for depend in &depends {
            let tx = self.transactions.get_mut(depend).unwrap();
            tx.children.push(short_txid);
        }

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
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Get a transaction from the mempool.
    pub fn get_from_mempool(&self, id: &Txid) -> Option<&Transaction> {
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
    use std::collections::HashMap;
    use std::collections::HashSet;

    use bitcoin::absolute;
    use bitcoin::block;
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version;
    use bitcoin::Amount;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::OutPoint;
    use bitcoin::ScriptBuf;
    use bitcoin::Sequence;
    use bitcoin::Target;
    use bitcoin::Transaction;
    use bitcoin::TxIn;
    use bitcoin::TxOut;
    use bitcoin::Txid;
    use bitcoin::Witness;
    use floresta_chain::pruned_utreexo::error::BlockValidationErrors;
    use floresta_chain::pruned_utreexo::error::TransactionError;
    use floresta_chain::BlockchainError;
    use floresta_common::bhash;
    use rand::Rng;
    use rand::SeedableRng;

    use super::Mempool;
    use crate::MempoolError;
    use crate::MempoolPolicy;
    use crate::UtxoData;

    fn p2pkh_script(tag: u8) -> ScriptBuf {
        let mut bytes = Vec::with_capacity(25);
        bytes.extend_from_slice(&[0x76, 0xa9, 0x14]);
        bytes.extend_from_slice(&[tag; 20]);
        bytes.extend_from_slice(&[0x88, 0xac]);
        ScriptBuf::from_bytes(bytes)
    }

    fn p2sh_script(tag: u8) -> ScriptBuf {
        let mut bytes = Vec::with_capacity(23);
        bytes.extend_from_slice(&[0xa9, 0x14]);
        bytes.extend_from_slice(&[tag; 20]);
        bytes.push(0x87);
        ScriptBuf::from_bytes(bytes)
    }

    fn p2wpkh_script(tag: u8) -> ScriptBuf {
        let mut bytes = Vec::with_capacity(22);
        bytes.extend_from_slice(&[0x00, 0x14]);
        bytes.extend_from_slice(&[tag; 20]);
        ScriptBuf::from_bytes(bytes)
    }

    fn p2wsh_script(tag: u8) -> ScriptBuf {
        let mut bytes = Vec::with_capacity(34);
        bytes.extend_from_slice(&[0x00, 0x20]);
        bytes.extend_from_slice(&[tag; 32]);
        ScriptBuf::from_bytes(bytes)
    }

    fn non_standard_script() -> ScriptBuf {
        ScriptBuf::from_bytes(vec![0x51])
    }

    fn make_utxo(value: u64, script_pubkey: ScriptBuf) -> UtxoData {
        UtxoData {
            txout: TxOut {
                value: Amount::from_sat(value),
                script_pubkey,
            },
            is_coinbase: false,
            creation_height: 0,
            creation_time: 0,
        }
    }

    fn input(previous_output: OutPoint) -> TxIn {
        TxIn {
            previous_output,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }
    }

    fn prevout(tag: u8) -> OutPoint {
        OutPoint {
            txid: Txid::from_slice(&[tag; 32]).unwrap(),
            vout: 0,
        }
    }

    fn single_input_tx(
        tag: u8,
        input_value: u64,
        output_value: u64,
        output_script: ScriptBuf,
    ) -> (Transaction, HashMap<OutPoint, UtxoData>) {
        let previous_output = prevout(tag);
        let mut context = HashMap::new();
        context.insert(previous_output, make_utxo(input_value, p2wpkh_script(tag)));

        let tx = Transaction {
            version: Version::TWO,
            lock_time: absolute::LockTime::from_consensus(0),
            input: vec![input(previous_output)],
            output: vec![TxOut {
                value: Amount::from_sat(output_value),
                script_pubkey: output_script,
            }],
        };

        (tx, context)
    }

    fn standard_tx(tag: u8) -> (Transaction, HashMap<OutPoint, UtxoData>) {
        single_input_tx(tag, 100_000, 98_000, p2wpkh_script(tag.wrapping_add(1)))
    }

    fn make_weight_tunable_tx(
        tag: u8,
        script_sig_len: usize,
    ) -> (Transaction, HashMap<OutPoint, UtxoData>) {
        let (mut tx, context) = single_input_tx(tag, 500_000, 100_000, p2wpkh_script(tag));
        tx.input[0].script_sig = ScriptBuf::from_bytes(vec![1; script_sig_len]);
        (tx, context)
    }

    fn make_witness_tunable_tx(
        tag: u8,
        witness_item_len: usize,
        witness_script_len: usize,
    ) -> (Transaction, HashMap<OutPoint, UtxoData>) {
        let (mut tx, context) = single_input_tx(tag, 500_000, 100_000, p2wsh_script(tag));
        tx.input[0].witness =
            vec![vec![0x30; witness_item_len], vec![0x51; witness_script_len]].into();
        (tx, context)
    }

    /// builds a list of transactions in a pseudo-random way
    ///
    /// We use those transactions in mempool tests
    fn build_transactions(
        seed: u64,
        conflict: bool,
    ) -> Vec<(Transaction, HashMap<OutPoint, UtxoData>)> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut transactions = Vec::new();
        let mut available = vec![(prevout(0), make_utxo(1_000_000, p2wpkh_script(0)))];
        let n = rng.gen_range(1..10);

        for idx in 0..n {
            let inputs = rng.gen_range(1..=available.len().min(3));
            let mut tx_inputs = Vec::new();
            let mut context = HashMap::new();
            let mut input_value = 0_u64;

            for _ in 0..inputs {
                let choice = rng.gen_range(0..available.len());
                let (outpoint, utxo) = if conflict {
                    available[choice].clone()
                } else {
                    available.remove(choice)
                };

                input_value += utxo.txout.value.to_sat();
                context.insert(outpoint, utxo);
                tx_inputs.push(input(outpoint));
            }

            let fee = 500 + idx as u64;
            let output_value = input_value.saturating_sub(fee);
            let tx = Transaction {
                version: Version::TWO,
                lock_time: absolute::LockTime::from_consensus(0),
                input: tx_inputs,
                output: vec![TxOut {
                    value: Amount::from_sat(output_value),
                    script_pubkey: p2wpkh_script(idx as u8 + 1),
                }],
            };

            available.push((
                OutPoint {
                    txid: tx.compute_txid(),
                    vout: 0,
                },
                make_utxo(output_value, tx.output[0].script_pubkey.clone()),
            ));

            transactions.push((tx, context));
        }

        transactions
    }

    fn expect_variant(
        result: Result<(), MempoolError>,
        expected: fn(&MempoolError) -> bool,
        label: &str,
    ) {
        match result {
            Err(error) if expected(&error) => {}
            other => panic!("expected {label}, got {other:?}"),
        }
    }

    #[test]
    fn test_random() {
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

        for (tx, context) in transactions {
            mempool
                .accept_to_mempool(tx, context)
                .expect("failed to accept to mempool");
        }

        assert_eq!(mempool.transactions.len(), len);
    }

    #[test]
    fn test_gbt_with_conflict() {
        let mut mempool = Mempool::new(10_000_000);
        let transactions = build_transactions(21, true);
        let mut did_conflict = false;

        for (tx, context) in transactions {
            match mempool.accept_to_mempool(tx, context) {
                Ok(()) => {}
                Err(MempoolError::DuplicatedInputs) | Err(MempoolError::ConflictingTransaction) => {
                    did_conflict = true;
                }
                Err(error) => panic!("unexpected error: {error:?}"),
            }
        }

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
    }

    fn check_block_transactions(block: Block) {
        let mut outputs = HashSet::new();
        let dummy_input = prevout(0);
        outputs.insert(dummy_input);

        for tx in &block.txdata {
            for input in &tx.input {
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
                outputs.insert(OutPoint {
                    txid,
                    vout: vout as u32,
                });
            }
        }
    }

    #[test]
    fn test_gbt_first_transaction() {
        let mut mempool = Mempool::new(10_000_000);
        let (tx, context) = single_input_tx(42, 100_000, 98_000, p2pkh_script(43));

        mempool
            .accept_to_mempool(tx, context)
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

        for (tx, context) in transactions {
            mempool
                .accept_to_mempool(tx, context)
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
    fn test_error_variants_are_reachable() {
        let policy = MempoolPolicy {
            min_relay_fee_sat_per_vbyte: 5,
            max_standard_tx_weight: 400,
            max_standard_script_sig_size: 32,
            max_standard_witness_stack_item_size: 16,
            max_standard_witness_script_size: 32,
            max_standard_witness_stack_items: 4,
        };

        let (tx, context) = standard_tx(1);
        let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
        mempool
            .accept_to_mempool(tx.clone(), context.clone())
            .unwrap();
        expect_variant(
            mempool.accept_to_mempool(tx, context),
            |error| matches!(error, MempoolError::AlreadyKnown),
            "AlreadyKnown",
        );

        let (tx, context) = standard_tx(2);
        let mut mempool = Mempool::with_policy(1, policy.clone());
        expect_variant(
            mempool.accept_to_mempool(tx, context),
            |error| matches!(error, MempoolError::MemoryUsageTooHigh),
            "MemoryUsageTooHigh",
        );

        let duplicate = prevout(3);
        let tx = Transaction {
            version: Version::TWO,
            lock_time: absolute::LockTime::from_consensus(0),
            input: vec![input(duplicate), input(duplicate)],
            output: vec![TxOut {
                value: Amount::from_sat(10_000),
                script_pubkey: p2wpkh_script(3),
            }],
        };
        let mut context = HashMap::new();
        context.insert(duplicate, make_utxo(30_000, p2wpkh_script(3)));
        let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
        expect_variant(
            mempool.accept_to_mempool(tx, context),
            |error| matches!(error, MempoolError::DuplicatedInputs),
            "DuplicatedInputs",
        );

        let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
        let (parent, parent_context) = standard_tx(4);
        let parent_outpoint = OutPoint {
            txid: parent.compute_txid(),
            vout: 0,
        };
        mempool
            .accept_to_mempool(parent.clone(), parent_context)
            .unwrap();
        let mut first_spend_context = HashMap::new();
        first_spend_context.insert(
            parent_outpoint,
            make_utxo(
                parent.output[0].value.to_sat(),
                parent.output[0].script_pubkey.clone(),
            ),
        );
        let (mut first_spend, _) = standard_tx(5);
        first_spend.input[0].previous_output = parent_outpoint;
        first_spend.output[0].value = Amount::from_sat(50_000);
        mempool
            .accept_to_mempool(first_spend, first_spend_context.clone())
            .unwrap();

        let (mut conflict, _) = standard_tx(6);
        conflict.input[0].previous_output = parent_outpoint;
        conflict.output[0].value = Amount::from_sat(40_000);
        expect_variant(
            mempool.accept_to_mempool(conflict, first_spend_context),
            |error| matches!(error, MempoolError::ConflictingTransaction),
            "ConflictingTransaction",
        );

        let (tx, context) = standard_tx(6);
        let mut low_fee = tx.clone();
        low_fee.output[0].value = Amount::from_sat(99_999);
        let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
        expect_variant(
            mempool.accept_to_mempool(low_fee, context),
            |error| matches!(error, MempoolError::FeeTooLow),
            "FeeTooLow",
        );

        let (tx, context) = make_weight_tunable_tx(7, 80);
        let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
        expect_variant(
            mempool.accept_to_mempool(tx, context),
            |error| matches!(error, MempoolError::ExceedsMaxWeight),
            "ExceedsMaxWeight",
        );

        let (tx, context) = make_weight_tunable_tx(8, 40);
        let mut too_large_script_sig = tx.clone();
        too_large_script_sig.input[0].script_sig = ScriptBuf::from_bytes(vec![1; 33]);
        let mut mempool = Mempool::with_policy(
            10_000_000,
            MempoolPolicy {
                max_standard_tx_weight: u64::MAX,
                ..policy.clone()
            },
        );
        expect_variant(
            mempool.accept_to_mempool(too_large_script_sig, context),
            |error| matches!(error, MempoolError::ExceedsScriptSigSize),
            "ExceedsScriptSigSize",
        );

        let (tx, context) = single_input_tx(9, 100_000, 98_000, non_standard_script());
        let mut mempool = Mempool::with_policy(
            10_000_000,
            MempoolPolicy {
                max_standard_tx_weight: u64::MAX,
                min_relay_fee_sat_per_vbyte: 1,
                ..policy.clone()
            },
        );
        expect_variant(
            mempool.accept_to_mempool(tx, context),
            |error| matches!(error, MempoolError::NonStandard),
            "NonStandard",
        );

        let (tx, _) = standard_tx(10);
        let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
        expect_variant(
            mempool.accept_to_mempool(tx, HashMap::new()),
            |error| matches!(error, MempoolError::MissingPrevoutContext),
            "MissingPrevoutContext",
        );

        let (tx, context) = single_input_tx(11, 50_000, 60_000, p2wpkh_script(11));
        let mut mempool = Mempool::with_policy(
            10_000_000,
            MempoolPolicy {
                min_relay_fee_sat_per_vbyte: 0,
                max_standard_tx_weight: u64::MAX,
                ..policy
            },
        );
        expect_variant(
            mempool.accept_to_mempool(tx, context),
            |error| {
                matches!(
                    error,
                    MempoolError::Consensus(BlockchainError::TransactionError(TransactionError {
                        error: BlockValidationErrors::NotEnoughMoney,
                        ..
                    }))
                )
            },
            "Consensus",
        );
    }

    #[test]
    fn test_standard_output_families_are_accepted() {
        let scripts = [
            p2pkh_script(1),
            p2sh_script(2),
            p2wpkh_script(3),
            p2wsh_script(4),
        ];

        for (index, script) in scripts.into_iter().enumerate() {
            let (tx, context) = single_input_tx(index as u8 + 20, 100_000, 98_000, script);
            let mut mempool = Mempool::new(10_000_000);
            mempool
                .accept_to_mempool(tx, context)
                .expect("standard output should be accepted");
        }
    }

    #[test]
    fn test_fee_floor_boundaries() {
        let (base_tx, base_context) = standard_tx(30);
        let vsize = base_tx.vsize() as u64;
        let floor = 3;
        let min_fee = floor * vsize;

        for (fee, should_accept) in [(min_fee - 1, false), (min_fee, true), (min_fee + 1, true)] {
            let mut tx = base_tx.clone();
            tx.output[0].value = Amount::from_sat(100_000 - fee);
            let mut mempool = Mempool::with_policy(
                10_000_000,
                MempoolPolicy {
                    min_relay_fee_sat_per_vbyte: floor,
                    ..MempoolPolicy::default()
                },
            );
            let result = mempool.accept_to_mempool(tx, base_context.clone());
            assert_eq!(
                result.is_ok(),
                should_accept,
                "unexpected result for fee {fee}"
            );
        }
    }

    #[test]
    fn test_weight_boundaries() {
        let (exact_tx, context) = make_weight_tunable_tx(31, 20);
        let exact_weight = exact_tx.weight().to_wu();
        let (below_tx, below_context) = make_weight_tunable_tx(32, 19);
        let (above_tx, above_context) = make_weight_tunable_tx(33, 21);

        for (tx, context, should_accept) in [
            (below_tx, below_context, true),
            (exact_tx, context, true),
            (above_tx, above_context, false),
        ] {
            let mut mempool = Mempool::with_policy(
                10_000_000,
                MempoolPolicy {
                    min_relay_fee_sat_per_vbyte: 0,
                    max_standard_tx_weight: exact_weight,
                    max_standard_script_sig_size: usize::MAX / 2,
                    ..MempoolPolicy::default()
                },
            );
            let result = mempool.accept_to_mempool(tx, context);
            assert_eq!(result.is_ok(), should_accept);
        }
    }

    #[test]
    fn test_script_sig_boundaries() {
        let max_script_sig_size = 32;
        for (size, should_accept) in [(31, true), (32, true), (33, false)] {
            let (tx, context) = make_weight_tunable_tx(size as u8 + 40, size);
            let mut mempool = Mempool::with_policy(
                10_000_000,
                MempoolPolicy {
                    min_relay_fee_sat_per_vbyte: 0,
                    max_standard_tx_weight: u64::MAX,
                    max_standard_script_sig_size: max_script_sig_size,
                    ..MempoolPolicy::default()
                },
            );
            let result = mempool.accept_to_mempool(tx, context);
            assert_eq!(result.is_ok(), should_accept);
        }
    }

    #[test]
    fn test_witness_boundaries() {
        let policy = MempoolPolicy {
            min_relay_fee_sat_per_vbyte: 0,
            max_standard_tx_weight: u64::MAX,
            max_standard_witness_stack_item_size: 16,
            max_standard_witness_script_size: 24,
            ..MempoolPolicy::default()
        };

        for (item_len, script_len, should_accept) in
            [(16, 24, true), (17, 24, false), (16, 25, false)]
        {
            let (tx, context) = make_witness_tunable_tx(60 + item_len as u8, item_len, script_len);
            let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
            let result = mempool.accept_to_mempool(tx, context);
            assert_eq!(result.is_ok(), should_accept);
        }
    }

    #[test]
    fn test_fee_rate_boundary_sweep() {
        let policy = MempoolPolicy {
            min_relay_fee_sat_per_vbyte: 2,
            ..MempoolPolicy::default()
        };

        for seed in 0..16_u8 {
            let (tx, context) = standard_tx(seed + 80);
            let vsize = tx.vsize() as u64;
            let threshold = policy.min_relay_fee_sat_per_vbyte * vsize;

            for delta in [-1_i64, 0, 1] {
                let fee = (threshold as i64 + delta) as u64;
                let mut tx = tx.clone();
                tx.output[0].value = Amount::from_sat(100_000 - fee);
                let mut mempool = Mempool::with_policy(10_000_000, policy.clone());
                let accepted = mempool.accept_to_mempool(tx, context.clone()).is_ok();
                assert_eq!(accepted, delta >= 0, "seed={seed}, delta={delta}");
            }
        }
    }

    #[test]
    fn test_weight_boundary_sweep() {
        for threshold in [300_u64, 320, 340, 360, 380] {
            for script_sig_len in 10..30_usize {
                let (tx, context) =
                    make_weight_tunable_tx(script_sig_len as u8 + 100, script_sig_len);
                let mut mempool = Mempool::with_policy(
                    10_000_000,
                    MempoolPolicy {
                        min_relay_fee_sat_per_vbyte: 0,
                        max_standard_tx_weight: threshold,
                        max_standard_script_sig_size: usize::MAX / 2,
                        ..MempoolPolicy::default()
                    },
                );
                let accepted = mempool.accept_to_mempool(tx.clone(), context).is_ok();
                assert_eq!(accepted, tx.weight().to_wu() <= threshold);
            }
        }
    }

    #[test]
    fn test_mempool_parents_supply_missing_context() {
        let mut mempool = Mempool::new(10_000_000);
        let (parent, parent_context) = standard_tx(120);
        let parent_outpoint = OutPoint {
            txid: parent.compute_txid(),
            vout: 0,
        };

        mempool
            .accept_to_mempool(parent.clone(), parent_context)
            .unwrap();

        let child = Transaction {
            version: Version::TWO,
            lock_time: absolute::LockTime::from_consensus(0),
            input: vec![input(parent_outpoint)],
            output: vec![TxOut {
                value: Amount::from_sat(parent.output[0].value.to_sat() - 1_000),
                script_pubkey: p2wpkh_script(121),
            }],
        };

        mempool
            .accept_to_mempool(child, HashMap::new())
            .expect("mempool parent should satisfy prevout context");
    }
}
