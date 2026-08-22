// SPDX-License-Identifier: MIT OR Apache-2.0

use alloc::vec::Vec;

use bitcoin::hashes::sha256d;

// TODO This trait should be moved to floresta-domain (see https://github.com/getfloresta/Floresta/issues/1270).
/// Backend for constructing Bitcoin transaction Merkle branches.
pub trait MerkleBackend: Send + Sync {
    /// Returns the sibling hashes for `position`, or `None` when it is out of bounds.
    fn calculate_branch(
        &self,
        leaves: &[sha256d::Hash],
        position: usize,
    ) -> Option<Vec<sha256d::Hash>>;
}
