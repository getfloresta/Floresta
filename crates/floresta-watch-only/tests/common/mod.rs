#![cfg(any(feature = "bdk-provider", feature = "sqlite"))]

use std::fs::create_dir_all;

use bitcoin::absolute::LockTime;
use bitcoin::block::Version;
use bitcoin::blockdata::block::Header;
use bitcoin::hashes::Hash;
use bitcoin::transaction::Version as TxVersion;
use bitcoin::Amount;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::CompactTarget;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Sequence;
use bitcoin::Transaction;
use bitcoin::TxIn;
use bitcoin::TxMerkleNode;
use bitcoin::TxOut;
use bitcoin::Txid;
use bitcoin::WPubkeyHash;
use bitcoin::Witness;

pub(crate) const DESCRIPTOR: &str = "wpkh(tpubDDtyive2LqLWKzPZ8LZ9Ebi1JDoLcf1cEpn3Mshp6sxVfCupHZJRPQTozp2EpTF76vJcyQBN7VP7CjUntEJxeADnuTMNTYKoSWNae8soVyv/0/*)#7h6kdtnk";
#[allow(dead_code)]
pub(crate) const DESCRIPTOR_ID: &str =
    "3f3958f4779e4c23273f1821263a7d788efb4c8a7354a5b4accc5cf45040e404";

pub(crate) const DESCRIPTOR_SECOND: &str = "wpkh(tpubDDtyive2LqLWKzPZ8LZ9Ebi1JDoLcf1cEpn3Mshp6sxVfCupHZJRPQTozp2EpTF76vJcyQBN7VP7CjUntEJxeADnuTMNTYKoSWNae8soVyv/1/*)#0rlhs7rw";
#[allow(dead_code)]
pub(crate) const DESCRIPTOR_SECOND_ID: &str =
    "902b63d58c5126027a6709a20c5259f105534c1bafe44445491ec11b0c1708ec";

pub struct TransactionInner {
    pub outpoint: Vec<OutPoint>,
    pub txo: Vec<TxOut>,
}

impl TransactionInner {
    pub fn to_transaction(&self) -> Transaction {
        if self.outpoint.is_empty() && self.txo.is_empty() {
            panic!("Cannot create transaction with empty inputs and outputs");
        }

        let outpoint = if self.outpoint.is_empty() {
            let txid = Txid::all_zeros();
            vec![OutPoint { txid, vout: 0 }]
        } else {
            self.outpoint.clone()
        };

        let txo = if self.txo.is_empty() {
            vec![TxOut {
                value: Amount::from_sat(10_000_000),
                script_pubkey: create_script_buff(),
            }]
        } else {
            self.txo.clone()
        };

        Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            input: outpoint
                .iter()
                .map(|outpoint| TxIn {
                    previous_output: *outpoint,
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    witness: Witness::default(),
                })
                .collect(),
            output: txo,
        }
    }
}

pub fn create_coinbase_transaction(
    script_pubkey: Option<ScriptBuf>,
    value: Option<u64>,
) -> Transaction {
    let script_pubkey = script_pubkey.unwrap_or_else(create_script_buff);
    let value = value.unwrap_or(50 * 100_000_000); // Default to 50 BTC

    let txout = bitcoin::TxOut {
        value: Amount::from_sat(value),
        script_pubkey,
    };

    let tx = Transaction {
        version: TxVersion::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            ..Default::default()
        }],
        output: vec![txout],
    };

    assert!(tx.is_coinbase());

    tx
}

pub fn create_script_buff() -> ScriptBuf {
    ScriptBuf::new_p2wpkh(&WPubkeyHash::all_zeros())
}

pub fn create_block_with_transaction(
    prev_block_hash: Option<BlockHash>,
    transaction: &Transaction,
) -> Block {
    let coinbase_tx = create_coinbase_transaction(None, None);

    create_block(prev_block_hash, vec![coinbase_tx, transaction.clone()])
}

pub fn create_block_with_transactions(
    prev_block_hash: Option<BlockHash>,
    transactions: Vec<Transaction>,
) -> Block {
    let coinbase_tx = create_coinbase_transaction(None, None);
    let mut all_txs = vec![coinbase_tx];
    all_txs.extend(transactions);

    create_block(prev_block_hash, all_txs)
}

#[allow(dead_code)]
pub fn create_block_with_coinbase(
    prev_block_hash: Option<BlockHash>,
    script_pubkey: ScriptBuf,
    value: u64,
) -> Block {
    let coinbase_tx = create_coinbase_transaction(Some(script_pubkey), Some(value));

    create_block(prev_block_hash, vec![coinbase_tx])
}

// pub fn create_block_with_coinbase_and_transaction(
//     prev_block_hash: Option<BlockHash>,
//     script_pubkey: ScriptBuf,
//     value: u64,
//     transaction: &Transaction,
// ) -> Block {
//     let coinbase_tx = create_coinbase_transaction(Some(script_pubkey), Some(value));

//     create_block(prev_block_hash, vec![coinbase_tx, transaction.clone()])
// }

pub fn create_block(prev_block_hash: Option<BlockHash>, transactions: Vec<Transaction>) -> Block {
    let header = Header {
        bits: CompactTarget::default(),
        nonce: 0,
        version: Version::default(),
        prev_blockhash: prev_block_hash.unwrap_or_else(BlockHash::all_zeros),
        merkle_root: TxMerkleNode::all_zeros(),
        time: 0,
    };

    let mut block = Block {
        header,
        txdata: transactions,
    };

    block.header.merkle_root = block.compute_merkle_root().unwrap();

    block
}

pub fn generate_blocks(count: u32, prev_block_hash: Option<BlockHash>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current_prev_hash = prev_block_hash.unwrap_or(BlockHash::all_zeros());

    for _ in 0..count {
        let block = create_block_with_transactions(Some(current_prev_hash), vec![]);
        current_prev_hash = block.header.block_hash();
        blocks.push(block);
    }

    blocks
}

pub fn generate_random_path_tmpdir() -> String {
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let random_suffix = duration.as_nanos();
    let path = format!("/tmp/{}", random_suffix);

    // Create all parent directories before returning the path
    create_dir_all(&path).expect("Failed to create tmp directory");

    path
}
