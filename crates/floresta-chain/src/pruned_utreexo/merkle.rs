// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bitcoin transaction Merkle-root calculation and mutation detection.
//!
//! Bitcoin duplicates the final hash when a tree level has an odd length. Consequently,
//! `[1, 2, 3, 4, 5, 6]` and `[1, 2, 3, 4, 5, 6, 5, 6]` produce the same root. This is the
//! source of [CVE-2012-2459](https://www.cve.org/CVERecord?id=CVE-2012-2459).
//!
//! To detect this mutation, equal real siblings are recorded before adding Bitcoin's synthetic
//! odd-tail duplicate. Mutated transaction data must be rejected without permanently invalidating
//! the block header, since an unmutated block can have the same hash.
//!
//! This follows Bitcoin Core's
//! [`ComputeMerkleRoot`](https://github.com/bitcoin/bitcoin/blob/v31.0/src/consensus/merkle.cpp#L46-L63)
//! semantics.

extern crate alloc;

use alloc::vec::Vec;

use bitcoin::Block;
use bitcoin::Transaction;
use bitcoin::TxMerkleNode;
use bitcoin::Txid;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::Hash;
use bitcoin_hashes::HashEngine as _;
use bitcoin_hashes::Sha256d;

use super::consensus::Consensus;

impl Consensus {
    /// Validates a block's transaction Merkle commitment for consensus.
    ///
    /// Returns the computed [`Txid`]s only when the header commits to their Merkle root
    /// and the tree has no duplicate-sibling mutation ([CVE-2012-2459]). Bitcoin Core
    /// performs these checks across [`BlockMerkleRoot`] and [`CheckMerkleRoot`]. This
    /// method combines them and returns the transaction IDs for reuse.
    ///
    /// [`Block::check_merkle_root`] only checks that the root matches and does not
    /// detect mutation. Use this method instead for consensus validation.
    ///
    /// [CVE-2012-2459]: https://www.cve.org/CVERecord?id=CVE-2012-2459
    /// [`BlockMerkleRoot`]: https://github.com/bitcoin/bitcoin/blob/v31.0/src/consensus/merkle.cpp#L66-L74
    /// [`CheckMerkleRoot`]: https://github.com/bitcoin/bitcoin/blob/v31.0/src/validation.cpp#L3869-L3894
    pub fn check_merkle_root(block: &Block) -> Option<Vec<Txid>> {
        let txids: Vec<_> = block.txdata.iter().map(compute_txid).collect();

        // Core defines the root of an empty transaction list as zero.
        let (root, mutated) = calculate_root(&txids).unwrap_or((TxMerkleNode::all_zeros(), false));

        (!mutated && block.header.merkle_root == root).then_some(txids)
    }
}

/// Computes a transaction Merkle root and reports equal real sibling hashes.
///
/// Rust counterpart of Bitcoin Core's [`ComputeMerkleRoot`]. Returns `(root, mutated)`.
/// The root always covers the entire input, including when `mutated` is true.
/// Unlike Core's zero hash for an empty list, this returns `None`.
/// Synthetic siblings added for odd-length levels are not mutations.
///
/// [`ComputeMerkleRoot`]: https://github.com/bitcoin/bitcoin/blob/v31.0/src/consensus/merkle.cpp#L46-L63
pub fn calculate_root(txids: &[Txid]) -> Option<(TxMerkleNode, bool)> {
    let mut level: Vec<[u8; 32]> = txids.iter().map(|txid| txid.to_byte_array()).collect();

    if level.is_empty() {
        return None;
    }

    // Scratch space for each level's packed `left || right` inputs.
    let mut inputs = Vec::<[u8; 64]>::with_capacity(level.len().div_ceil(2));
    let mut mutated = false;

    while level.len() > 1 {
        for pair in level.chunks_exact(2) {
            mutated |= pair[0] == pair[1];
        }

        let parent_count = level.len().div_ceil(2);
        inputs.resize(parent_count, [0; 64]);

        // Pack each pair, duplicating an unpaired final hash.
        for (index, input) in inputs.iter_mut().enumerate() {
            let left = level[index * 2];
            let right = level.get(index * 2 + 1).copied().unwrap_or(left);
            input[..32].copy_from_slice(&left);
            input[32..].copy_from_slice(&right);
        }

        // Hash all parents into the front of `level`.
        Sha256d::hash_64_many(&mut level[..parent_count], &inputs);
        level.truncate(parent_count);
    }

    let root = TxMerkleNode::from_byte_array(level[0]);
    Some((root, mutated))
}

#[rustfmt::skip]
/// Computes a non-witness transaction ID using the newer `bitcoin_hashes` SHA256d engine.
///
/// This mirrors rust-bitcoin's [`Transaction::compute_txid`], but uses `bitcoin_hashes` 1.x
/// with its `cpufeatures`-enabled ARM SHA2 implementation.
///
/// TODO: Remove this helper once our rust-bitcoin dependency provides the same acceleration.
fn compute_txid(tx: &Transaction) -> Txid {
    struct NewTxidEngine(bitcoin_hashes::sha256d::HashEngine);

    impl bitcoin::io::Write for NewTxidEngine {
        fn write(&mut self, buf: &[u8]) -> bitcoin::io::Result<usize> {
            self.0.input(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> bitcoin::io::Result<()> {
            Ok(())
        }
    }

    let mut enc = NewTxidEngine(Sha256d::engine());
    tx.version.consensus_encode(&mut enc).expect("engines don't error");
    tx.input.consensus_encode(&mut enc).expect("engines don't error");
    tx.output.consensus_encode(&mut enc).expect("engines don't error");
    tx.lock_time.consensus_encode(&mut enc).expect("engines don't error");

    Txid::from_byte_array(Sha256d::from_engine(enc.0).to_byte_array())
}

#[cfg(test)]
mod tests {
    use bitcoin::Network;
    use bitcoin::constants::genesis_block;
    use bitcoin::merkle_tree;

    use super::*;

    fn unique_txids(count: u8) -> Vec<Txid> {
        (0..count)
            .map(|value| Txid::from_byte_array([value; 32]))
            .collect()
    }

    #[test]
    fn matches_rust_bitcoin_roots() {
        for count in 0..=16 {
            let txids = unique_txids(count);
            let expected = merkle_tree::calculate_root(txids.iter().map(|txid| txid.to_raw_hash()))
                .map(TxMerkleNode::from);

            let actual = calculate_root(&txids);
            assert_eq!(actual.map(|(root, _)| root), expected);
            assert!(!actual.is_some_and(|(_, mutated)| mutated));
        }
    }

    #[test]
    fn detects_duplicate_subtrees() {
        for count in [3, 6, 12] {
            let txids = unique_txids(count);
            let mut duplicated = txids.clone();
            let duplicate_count = count.next_power_of_two() - count;
            duplicated.extend_from_slice(&txids[usize::from(count - duplicate_count)..]);

            let (root, mutated) = calculate_root(&txids).unwrap();
            let (duplicated_root, duplicated_mutated) = calculate_root(&duplicated).unwrap();

            assert_eq!(root, duplicated_root);
            assert!(!mutated);
            assert!(duplicated_mutated);
        }
    }

    #[test]
    fn empty_block_uses_core_zero_root() {
        let mut block = genesis_block(Network::Regtest);
        block.txdata.clear();

        assert!(Consensus::check_merkle_root(&block).is_none());

        block.header.merkle_root = TxMerkleNode::all_zeros();
        // Such block is invalid, but not considered mutated.
        assert_eq!(Consensus::check_merkle_root(&block), Some(Vec::new()));
    }

    #[test]
    fn compute_txid_matches_rust_bitcoin_and_excludes_witness() {
        let mut tx = genesis_block(Network::Bitcoin).txdata[0].clone();
        let expected = tx.compute_txid();

        assert_eq!(compute_txid(&tx), expected);

        tx.input[0].witness.push([42u8; 32]);

        assert_eq!(tx.compute_txid(), expected);
        assert_eq!(compute_txid(&tx), expected);
    }
}
