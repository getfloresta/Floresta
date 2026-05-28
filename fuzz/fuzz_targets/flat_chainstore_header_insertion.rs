// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::HashMap;

use bitcoin::BlockHash;
use bitcoin::CompactTarget;
use bitcoin::TxMerkleNode;
use bitcoin::block::Header as BlockHeader;
use bitcoin::block::Version;
use bitcoin::hashes::Hash;
use floresta_chain::ChainStore;
use floresta_chain::DiskBlockHeader;
use floresta_chain::InvalidReason;
use floresta_chain::pruned_utreexo::flat_chain_store::FlatChainStore;
use floresta_chain::pruned_utreexo::flat_chain_store::FlatChainStoreConfig;
use libfuzzer_sys::arbitrary::Arbitrary;
use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use tempfile::TempDir;

/// Fuzz input containing a pool of 1-20 block headers and 10-500 operations.
///
/// Headers can be referenced by index in operations, allowing the fuzzer to test
/// scenarios like saving the same header multiple times or in different states.
#[derive(Debug)]
struct FuzzInput {
    headers: Vec<BlockHeader>,
    operations: Vec<Operation>,
}

/// Operations to perform on the FlatChainStore during fuzzing.
///
/// Includes saving headers in various states (fully valid, headers-only, assumed valid,
/// in-fork, orphan, invalid), retrieving by hash or height, and maintenance operations
/// (flush, integrity check).
#[derive(Debug, Clone, Copy)]
enum Operation {
    SaveHeaderFullyValid {
        header_idx: u8,
        height: u32,
    },
    SaveHeaderHeadersOnly {
        header_idx: u8,
        height: u32,
    },
    SaveHeaderAssumedValid {
        header_idx: u8,
        height: u32,
    },
    SaveHeaderInFork {
        header_idx: u8,
        height: u32,
    },
    SaveHeaderOrphan {
        header_idx: u8,
    },
    SaveHeaderInvalidChain {
        header_idx: u8,
        height: u32,
        reason_tag: u8,
    },
    GetHeaderByHash {
        header_idx: u8,
    },
    GetBlockHash {
        height: u32,
    },
    Flush,
    CheckIntegrity,
}

impl<'a> Arbitrary<'a> for FuzzInput {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self, libfuzzer_sys::arbitrary::Error> {
        // Generate 1-20 different headers
        let header_count = u.int_in_range(1..=20)?;
        let mut headers = Vec::with_capacity(header_count);

        for _ in 0..header_count {
            let version = i32::arbitrary(u)?;
            let prev: [u8; 32] = Arbitrary::arbitrary(u)?;
            let merkle: [u8; 32] = Arbitrary::arbitrary(u)?;
            let time = u32::arbitrary(u)?;
            let bits = u32::arbitrary(u)?;
            let nonce = u32::arbitrary(u)?;

            let header = BlockHeader {
                version: Version::from_consensus(version),
                prev_blockhash: BlockHash::from_byte_array(prev),
                merkle_root: TxMerkleNode::from_byte_array(merkle),
                time,
                bits: CompactTarget::from_consensus(bits),
                nonce,
            };
            headers.push(header);
        }

        // Generate 10-500 operations
        let op_count = u.int_in_range(10..=500)?;
        let mut operations = Vec::with_capacity(op_count);

        for _ in 0..op_count {
            let header_idx = u.int_in_range(0..=(header_count - 1))? as u8;

            let op = match u.int_in_range(0u8..=9)? {
                0 => Operation::SaveHeaderFullyValid {
                    header_idx,
                    height: u32::arbitrary(u)?,
                },
                1 => Operation::SaveHeaderHeadersOnly {
                    header_idx,
                    height: u32::arbitrary(u)?,
                },
                2 => Operation::SaveHeaderAssumedValid {
                    header_idx,
                    height: u32::arbitrary(u)?,
                },
                3 => Operation::SaveHeaderInFork {
                    header_idx,
                    height: u32::arbitrary(u)?,
                },
                4 => Operation::SaveHeaderOrphan { header_idx },
                5 => Operation::SaveHeaderInvalidChain {
                    header_idx,
                    height: u32::arbitrary(u)?,
                    reason_tag: u8::arbitrary(u)?,
                },
                6 => Operation::GetHeaderByHash { header_idx },
                7 => Operation::GetBlockHash {
                    height: u32::arbitrary(u)?,
                },
                8 => Operation::Flush,
                _ => Operation::CheckIntegrity,
            };
            operations.push(op);
        }

        Ok(FuzzInput {
            headers,
            operations,
        })
    }
}

/// Oracle tracking expected store state for verification during fuzzing.
///
/// Maintains mappings of hash→header and height→hash, updated only on successful
/// save operations, to verify retrieval correctness.

#[derive(Debug, Clone)]
struct OracleEntry {
    header: BlockHeader,
}

fuzz_target!(|input: FuzzInput| {
    let temp_dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(_) => return,
    };

    let config = FlatChainStoreConfig {
        block_index_size: Some(128),
        headers_file_size: Some(128),
        fork_file_size: Some(64),
        cache_size: Some(15),
        file_permission: Some(0o666),
        path: temp_dir.path().into(),
    };

    let mut store = match FlatChainStore::new(config) {
        Ok(store) => store,
        Err(_) => return,
    };

    // Oracle: track what we've stored
    let mut hash_to_entry: HashMap<BlockHash, OracleEntry> = HashMap::new();
    let mut height_to_hash: HashMap<u32, BlockHash> = HashMap::new();

    for operation in input.operations {
        match operation {
            Operation::SaveHeaderFullyValid { header_idx, height } => {
                let header = input.headers[header_idx as usize % input.headers.len()];
                let variant = DiskBlockHeader::FullyValid(header, height);

                if store.save_header(&variant).is_ok() {
                    height_to_hash.insert(height, header.block_hash());
                    if store
                        .update_block_index(height, header.block_hash())
                        .is_ok()
                    {
                        hash_to_entry.insert(header.block_hash(), OracleEntry { header });
                    }
                }
            }
            Operation::SaveHeaderHeadersOnly { header_idx, height } => {
                let header = input.headers[header_idx as usize % input.headers.len()];
                let variant = DiskBlockHeader::HeadersOnly(header, height);

                if store.save_header(&variant).is_ok() {
                    height_to_hash.insert(height, header.block_hash());
                    if store
                        .update_block_index(height, header.block_hash())
                        .is_ok()
                    {
                        hash_to_entry.insert(header.block_hash(), OracleEntry { header });
                    }
                }
            }
            Operation::SaveHeaderAssumedValid { header_idx, height } => {
                let header = input.headers[header_idx as usize % input.headers.len()];
                let variant = DiskBlockHeader::AssumedValid(header, height);

                if store.save_header(&variant).is_ok() {
                    height_to_hash.insert(height, header.block_hash());
                    if store
                        .update_block_index(height, header.block_hash())
                        .is_ok()
                    {
                        hash_to_entry.insert(header.block_hash(), OracleEntry { header });
                    }
                }
            }
            Operation::SaveHeaderInFork { header_idx, height } => {
                let header = input.headers[header_idx as usize % input.headers.len()];
                let variant = DiskBlockHeader::InFork(header, height);

                if store.save_header(&variant).is_ok()
                    && store
                        .update_block_index(height, header.block_hash())
                        .is_ok()
                {
                    hash_to_entry.insert(header.block_hash(), OracleEntry { header });
                }
            }
            Operation::SaveHeaderOrphan { header_idx } => {
                let header = input.headers[header_idx as usize % input.headers.len()];
                let variant = DiskBlockHeader::Orphan(header);

                if store.save_header(&variant).is_ok() {
                    hash_to_entry.insert(header.block_hash(), OracleEntry { header });
                }
            }
            Operation::SaveHeaderInvalidChain {
                header_idx,
                height,
                reason_tag,
            } => {
                let header = input.headers[header_idx as usize % input.headers.len()];
                let reason = match reason_tag % 3 {
                    0 => InvalidReason::ValidationFailed,
                    1 => InvalidReason::Invalidated,
                    _ => InvalidReason::DescendsFromInvalid,
                };
                let variant = DiskBlockHeader::InvalidChain(header, height, reason);

                if store.save_header(&variant).is_ok() {
                    hash_to_entry.insert(header.block_hash(), OracleEntry { header });
                }
            }
            Operation::GetHeaderByHash { header_idx } => {
                let header = input.headers[header_idx as usize % input.headers.len()];
                let hash = header.block_hash();

                if let Ok(Some(retrieved)) = store.get_header(&hash) {
                    assert_eq!(
                        retrieved.block_hash(),
                        hash,
                        "Retrieved header hash mismatch"
                    );

                    // If we have oracle data, verify consistency
                    if let Some(oracle_entry) = hash_to_entry.get(&hash) {
                        assert_eq!(
                            retrieved.block_hash(),
                            oracle_entry.header.block_hash(),
                            "Oracle hash mismatch"
                        );
                    }
                }
            }
            Operation::GetBlockHash { height } => {
                if let Ok(Some(retrieved_hash)) = store.get_block_hash(height) {
                    // Verify against oracle
                    if let Some(expected_hash) = height_to_hash.get(&height) {
                        assert_eq!(
                            retrieved_hash, *expected_hash,
                            "Height {height} returned wrong hash. Expected {expected_hash:?}, got {retrieved_hash:?}",
                        );
                    }
                }
            }
            Operation::Flush => {
                store.flush().unwrap();
            }
            Operation::CheckIntegrity => {
                store.flush().unwrap();
                store.check_integrity().unwrap();
            }
        }
    }

    // Final flush and integrity check
    store.flush().unwrap();
    store.check_integrity().unwrap()
});
