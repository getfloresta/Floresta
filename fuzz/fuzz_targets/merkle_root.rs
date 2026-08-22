// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use bitcoin::TxMerkleNode;
use bitcoin::Txid;
use bitcoin::hashes::Hash;
use bitcoin::hashes::sha256d;
use bitcoin::merkle_tree;
use floresta_chain::pruned_utreexo::merkle::ConsensusMerkle;
use libfuzzer_sys::fuzz_target;

const MAX_LEAVES: usize = 256;

/// Asserts our merkle root result matches that of `rust-bitcoin`.
fn assert_root_matches(txids: &[Txid]) -> Option<(TxMerkleNode, bool)> {
    let hashes = txids.iter().map(|txid| txid.to_raw_hash());
    let expected = merkle_tree::calculate_root(hashes).map(TxMerkleNode::from);
    let actual = ConsensusMerkle::calculate_root(txids);

    assert_eq!(actual.as_ref().map(|(root, _)| *root), expected);
    actual
}

fuzz_target!(|data: &[u8]| {
    let depth = data.first().copied().unwrap_or_default();
    let data = data.get(1..).unwrap_or_default();

    // Hash short chunks into varied leaves
    let txids: Vec<_> = data
        .chunks(4)
        .take(MAX_LEAVES)
        .enumerate()
        .map(|(index, chunk)| {
            let mut txid = sha256d::Hash::hash(chunk).to_byte_array();
            txid[0] = index as u8; // keeps every leaf distinct
            Txid::from_byte_array(txid)
        })
        .collect();

    assert_root_matches(&txids);

    let available = txids.len() / 3;
    if available == 0 {
        return;
    }

    // Choose a fuzz-controlled power-of-two size so each group is a complete subtree.
    let subtree_size = 1 << (u32::from(depth) % (available.ilog2() + 1));
    let base = &txids[..3 * subtree_size]; // [A, B, C]
    let mut duplicated = base.to_vec();
    duplicated.extend_from_slice(&base[2 * subtree_size..]); // [A, B, C, C]

    // [A, B, C] hashes as if C was repeated, but that internal copy is not a mutation.
    let (root, base_mutated) = assert_root_matches(base).expect("non-empty");

    // [A, B, C, C] hashes to the same root, but we detect the illegal mutation.
    let (duplicated_root, duplicated_mutated) =
        assert_root_matches(&duplicated).expect("non-empty");

    assert!(!base_mutated);
    assert!(duplicated_mutated);
    assert_eq!(root, duplicated_root);
});
