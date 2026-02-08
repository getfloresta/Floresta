//! This module is centered around the `ChainState` type, defining it and providing
//! implementations for the [BlockchainInterface] and [UpdatableChainstate] traits.
//!
//! Consequently, `ChainState` serves as the blockchain backend for our node and is
//! the highest-level type in `floresta-chain`. It is responsible for:
//!
//! - Keeping track of the chain state, and using a [ChainStore] for persisted storage
//! - Correctly updating the state with the help of the `consensus.rs` functions
//! - Interfacing with other components, and providing data about the current view of the chain
//!
//! The primary methods for updating our state are [ChainState::accept_header], which constructs
//! a chain of headers, and [ChainState::connect_block], which verifies the corresponding blocks.
//!
//! Key definitions:
//! - [ChainState]: The high-level chain backend
//! - [BlockConsumer]: Trait for receiving new block notifications
extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::fmt::format;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;

use bitcoin::block::Header as BlockHeader;
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::hashes::sha256;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::Target;
use bitcoin::Transaction;
use bitcoin::Txid;
use bitcoin::Work;
use floresta_common::Channel;
#[cfg(feature = "metrics")]
use metrics;
use rustreexo::accumulator::node_hash::BitcoinNodeHash;
use rustreexo::accumulator::proof::Proof;
use rustreexo::accumulator::stump::Stump;
use spin::RwLock;
use tracing::debug;
use tracing::info;
use tracing::warn;

use super::chain_state_builder::BlockchainBuilderError;
use super::chain_state_builder::ChainStateBuilder;
use super::chainparams::ChainParams;
use super::chainstore::DiskBlockHeader;
use super::consensus::Consensus;
use super::error::BlockValidationErrors;
use super::error::BlockchainError;
use super::partial_chain::PartialChainState;
use super::partial_chain::PartialChainStateInner;
use super::BlockchainInterface;
use super::UpdatableChainstate;
use crate::prelude::*;
use crate::pruned_utreexo::utxo_data::UtxoData;
use crate::read_lock;
use crate::write_lock;
use crate::BestChain;
use crate::ChainStore;

/// Trait for components that need to receive notifications about new blocks.
pub trait BlockConsumer: Sync + Send + 'static {
    /// Return true if this consumer wants the set of spent UTXOs.
    fn wants_spent_utxos(&self) -> bool;

    /// Called whenever a valid block is connected. `spent_utxos` is None unless consumer `wants_spent_utxos()`.
    fn on_block(
        &self,
        block: &Block,
        height: u32,
        spent_utxos: Option<&HashMap<OutPoint, UtxoData>>,
    );
}

impl BlockConsumer for Channel<(Block, u32)> {
    fn wants_spent_utxos(&self) -> bool {
        false
    }

    fn on_block(
        &self,
        block: &Block,
        height: u32,
        _spent_utxos: Option<&HashMap<OutPoint, UtxoData>>,
    ) {
        self.send((block.to_owned(), height));
    }
}

impl BlockConsumer for Channel<(Block, u32, HashMap<OutPoint, UtxoData>)> {
    fn wants_spent_utxos(&self) -> bool {
        true
    }

    fn on_block(
        &self,
        block: &Block,
        height: u32,
        spent_utxos: Option<&HashMap<OutPoint, UtxoData>>,
    ) {
        let spent_utxos = spent_utxos.expect("Safe to unwrap because wants_spent_utxos()");
        self.send((block.to_owned(), height, spent_utxos.to_owned()));
    }
}

/// Internal state of the blockchain managed by `ChainState`.
pub struct ChainStateInner<PersistedState: ChainStore> {
    /// The acc we use for validation.
    acc: Stump,
    /// All data is persisted here.
    chainstore: PersistedState,
    /// Best known block, cached in a specific field to faster access.
    best_block: BestChain,
    /// We may have multiple modules that needs to receive and process blocks as they come, to
    /// be notified of new blocks, a module should implement the [BlockConsumer] trait, and
    /// subscribe by passing an [Arc] of itself to chainstate.
    /// When a new block is accepted (as valid) we call `on_block` from [BlockConsumer].
    /// If a module just wants pass in a channel, `Sender` implements [BlockConsumer], and can
    /// be used during subscription (just keep the `Receiver` side.
    subscribers: Vec<Arc<dyn BlockConsumer>>,
    /// Fee estimation for 1, 10 and 20 blocks
    fee_estimation: (f64, f64, f64),
    /// Are we in Initial Block Download?
    ibd: bool,
    /// Parameters for the chain and functions that verify the chain.
    consensus: Consensus,
    /// Assume valid is a Core-specific config that tells the node to not validate signatures
    /// in blocks before this one. Note that we only skip signature validation, everything else
    /// is still validated.
    assume_valid: Option<BlockHash>,
}

/// The high-level chain backend managing the blockchain state.
///
/// `ChainState` is responsible for:
/// - Keeping track of the chain state with the help of a `ChainStore` for persisted storage.
/// - Correctly updating the state using consensus functions.
/// - Interfacing with other components and providing data about the current view of the chain.
pub struct ChainState<PersistedState: ChainStore> {
    inner: RwLock<ChainStateInner<PersistedState>>,
}

#[derive(Debug, Copy, Clone)]
/// Represents the argument for the assume-valid configuration.
///
/// This enum indicates the state of the assume-valid configuration,
/// which defines whether we should validate the scripts for blocks before this one.
/// You can either disable it, use a value provided by floresta or use your own value.
pub enum AssumeValidArg {
    /// Do not assume any script are valid, check every single one from genesis. This should make IBD considerably slower, but in theory has the best security model
    Disabled,
    /// Use the hard-coded value provided by Floresta. In this case, you trust that the Floresta repository faces enough scrutiny and review, and therefore the value can be trusted.
    Hardcoded,
    /// Provide your own value, moving the trust assumption to your program.
    UserInput(BlockHash),
}

impl<PersistedState: ChainStore> ChainState<PersistedState> {
    fn maybe_reindex(&self, potential_tip: &DiskBlockHeader) -> Result<(), BlockchainError> {
        if let DiskBlockHeader::HeadersOnly(_, height) = potential_tip {
            let best_height = self.get_best_block()?.0;

            if *height > best_height {
                warn!("Found block with height {height}, while the current best chain is at {best_height}. Reindexing.");
                self.reindex_chain()?;
            }
        }

        Ok(())
    }

    /// Just adds headers to the chainstate, without validating them.
    pub fn push_headers(
        &self,
        headers: Vec<BlockHeader>,
        height: u32,
    ) -> Result<(), BlockchainError> {
        for (offset, &header) in headers.iter().enumerate() {
            let disk_height = height + offset as u32;
            let disk_header = DiskBlockHeader::FullyValid(header, disk_height);
            let hash = disk_header.block_hash();

            self.update_header_and_index(&disk_header, hash, disk_height)?;
        }

        Ok(())
    }

    fn update_header(&self, header: &DiskBlockHeader) -> Result<(), BlockchainError> {
        Ok(write_lock!(self).chainstore.save_header(header)?)
    }

    fn update_header_and_index(
        &self,
        header: &DiskBlockHeader,
        hash: BlockHash,
        height: u32,
    ) -> Result<(), BlockchainError> {
        let mut inner = write_lock!(self);

        inner.chainstore.save_header(header)?;
        inner.chainstore.update_block_index(height, hash)?;

        Ok(())
    }

    fn validate_header(&self, block_header: &BlockHeader) -> Result<BlockHash, BlockchainError> {
        let prev_block = self.get_disk_block_header(&block_header.prev_blockhash)?;
        let height = prev_block
            .height()
            .ok_or(BlockValidationErrors::BlockExtendsAnOrphanChain)?
            + 1;

        // Check pow
        let expected_target = self.get_next_required_work(&prev_block, height, block_header)?;

        let actual_target = block_header.target();
        if actual_target > expected_target {
            return Err(BlockValidationErrors::NotEnoughPow)?;
        }

        self.check_bip94_block(block_header, height)?;

        let block_hash = block_header
            .validate_pow(actual_target)
            .map_err(|_| BlockValidationErrors::NotEnoughPow)?;
        Ok(block_hash)
    }

    #[inline]
    /// Whether a node is the genesis block for this net
    fn is_genesis(&self, header: &BlockHeader) -> bool {
        header.block_hash() == self.chain_params().genesis.block_hash()
    }

    #[inline]
    /// Returns the ancestor of a given block
    fn get_ancestor(&self, header: &BlockHeader) -> Result<DiskBlockHeader, BlockchainError> {
        self.get_disk_block_header(&header.prev_blockhash)
    }

    /// Returns the cumulative work in this branch
    fn get_branch_work(&self, header: &BlockHeader) -> Result<Work, BlockchainError> {
        let mut header = *header;
        let mut work = Work::from_be_bytes([0; 32]);
        while !self.is_genesis(&header) {
            work = work + header.work();
            header = *self.get_ancestor(&header)?;
        }

        Ok(work)
    }

    /// Checks if a branch is valid (i.e. all ancestors are known)
    fn check_branch(&self, branch_tip: &BlockHeader) -> Result<(), BlockchainError> {
        let mut header = *branch_tip;

        while !self.is_genesis(&header) {
            let _header = self.get_ancestor(&header)?;
            if let DiskBlockHeader::Orphan(block) = _header {
                return Err(BlockchainError::InvalidTip(format(format_args!(
                    "Block {} doesn't have a known ancestor (i.e an orphan block)",
                    block.block_hash()
                ))));
            }
            header = *_header;
        }

        Ok(())
    }

    /// Returns the depth of a branch (i.e. how many blocks are in the branch)
    fn get_chain_depth(&self, branch_tip: &BlockHeader) -> Result<u32, BlockchainError> {
        let mut header = *branch_tip;

        let mut counter = 0;
        while !self.is_genesis(&header) {
            header = *self.get_ancestor(&header)?;
            counter += 1;
        }

        Ok(counter)
    }

    /// Mark the current index as active, because we are in the middle of a reorg
    fn mark_chain_as_active(
        &self,
        new_tip: &BlockHeader,
        fork_point: BlockHash,
    ) -> Result<(), BlockchainError> {
        let mut height = self.get_chain_depth(new_tip)?;
        let mut header = DiskBlockHeader::HeadersOnly(*new_tip, height);

        while !self.is_genesis(&header) && header.block_hash() != fork_point {
            let disk_header = DiskBlockHeader::HeadersOnly(*header, height);
            let hash = disk_header.block_hash();

            self.update_header_and_index(&disk_header, hash, height)?;

            header = self.get_ancestor(&header)?;
            height -= 1;
        }

        Ok(())
    }

    /// Mark the current index as inactive, either because we found an invalid ancestor,
    /// or we are in the middle of reorg
    fn mark_chain_as_inactive(
        &self,
        old_tip: &BlockHeader,
        fork_point: BlockHash,
    ) -> Result<(), BlockchainError> {
        let mut header = *old_tip;
        let mut height = self.get_chain_depth(old_tip)?;

        while !self.is_genesis(&header) && header.block_hash() != fork_point {
            self.update_header(&DiskBlockHeader::InFork(header, height))?;

            header = *self.get_ancestor(&header)?;
            height -= 1;
        }

        Ok(())
    }

    /// Finds where in the current index, a given branch forks out.
    fn find_fork_point(&self, header: &BlockHeader) -> Result<BlockHeader, BlockchainError> {
        let mut header = *self.get_ancestor(header)?;
        let inner = read_lock!(self);
        while !self.is_genesis(&header) {
            match inner.chainstore.get_header(&header.block_hash())? {
                Some(DiskBlockHeader::HeadersOnly(block, _)) => {
                    return Ok(block);
                }
                Some(DiskBlockHeader::FullyValid(block, _)) => {
                    return Ok(block);
                }
                Some(DiskBlockHeader::InFork(block, _)) => {
                    header = *self.get_ancestor(&block)?;
                    continue;
                }
                Some(DiskBlockHeader::AssumedValid(block, _)) => {
                    return Ok(block);
                }
                Some(DiskBlockHeader::Orphan(header)) => {
                    return Err(BlockchainError::InvalidTip(format(format_args!(
                        "Block {} doesn't have a known ancestor (i.e an orphan block)",
                        header.block_hash()
                    ))));
                }
                Some(DiskBlockHeader::InvalidChain(header)) => {
                    return Err(BlockchainError::InvalidTip(format(format_args!(
                        "Block {} is invalid",
                        header.block_hash()
                    ))));
                }
                None => {
                    return Err(BlockchainError::InvalidTip(format(format_args!(
                        "Block {} isn't in our storage",
                        header.block_hash()
                    ))));
                }
            }
        }
        Err(BlockchainError::InvalidTip(
            "Couldn't find a fork point".to_string(),
        ))
    }

    /// Changes the acc we are using to validate blocks.
    fn reorg_acc(&self, fork_point: &BlockHeader) -> Result<(), BlockchainError> {
        let height = self
            .get_block_height(&fork_point.block_hash())?
            .ok_or(BlockchainError::BlockNotPresent)?;

        let acc = self.get_roots_for_block(height)?.unwrap_or_default();
        let mut inner = write_lock!(self);
        inner.acc = acc;

        Ok(())
    }

    // This method should only be called after we validate the new branch
    fn reorg(&self, new_tip: BlockHeader) -> Result<(), BlockchainError> {
        let current_best_block = self.get_block_header(&self.get_best_block()?.1)?;
        let fork_point = self.find_fork_point(&new_tip)?;

        self.mark_chain_as_inactive(&current_best_block, fork_point.block_hash())?;
        self.mark_chain_as_active(&new_tip, fork_point.block_hash())?;

        let validation_index = self.get_last_valid_block(&new_tip)?;
        let depth = self.get_chain_depth(&new_tip)?;

        self.change_active_chain(&new_tip, validation_index, depth);
        self.reorg_acc(&fork_point)?;

        Ok(())
    }

    /// Changes the active chain to the new branch during a reorg
    fn change_active_chain(&self, new_tip: &BlockHeader, last_valid: BlockHash, depth: u32) {
        let mut inner = self.inner.write();
        inner.best_block.best_block = new_tip.block_hash();
        inner.best_block.validation_index = last_valid;
        inner.best_block.depth = depth;
    }

    /// Grabs the last block we validated in this branch. We don't validate a fork, unless it
    /// becomes the best chain. This function technically finds out what is the last common block
    /// between two branches.
    fn get_last_valid_block(&self, header: &BlockHeader) -> Result<BlockHash, BlockchainError> {
        let mut header = *header;

        while !self.is_genesis(&header) {
            let _header = self.get_ancestor(&header)?;

            match _header {
                DiskBlockHeader::FullyValid(_, _) | DiskBlockHeader::AssumedValid(_, _) => {
                    return Ok(_header.block_hash())
                }
                DiskBlockHeader::Orphan(_) => {
                    return Err(BlockchainError::InvalidTip(format(format_args!(
                        "Block {} doesn't have a known ancestor (i.e an orphan block)",
                        _header.block_hash()
                    ))))
                }
                DiskBlockHeader::HeadersOnly(_, _) | DiskBlockHeader::InFork(_, _) => {}
                DiskBlockHeader::InvalidChain(_) => {
                    return Err(BlockchainError::InvalidTip(format(format_args!(
                        "Block {} is in an invalid chain",
                        _header.block_hash()
                    ))))
                }
            }

            header = *_header;
        }

        Ok(header.block_hash())
    }

    /// If we get a header that doesn't build on top of our best chain, it may cause a reorganization.
    /// We check this here.
    fn maybe_reorg(&self, branch_tip: BlockHeader) -> Result<(), BlockchainError> {
        let current_tip = self.get_block_header(&self.get_best_block()?.1)?;
        self.check_branch(&branch_tip)?;

        let current_work = self.get_branch_work(&current_tip)?;
        let new_work = self.get_branch_work(&branch_tip)?;
        // If the new branch has more work, it becomes the new best chain
        if new_work > current_work {
            self.reorg(branch_tip)?;
            return Ok(());
        }
        // If the new branch has less work, we just store it as an alternative branch
        // that might become the best chain in the future.
        self.push_alt_tip(&branch_tip)?;

        let parent_height = self.get_ancestor(&branch_tip)?.try_height()?;

        self.update_header(&DiskBlockHeader::InFork(branch_tip, parent_height + 1))?;

        Ok(())
    }

    /// Stores a new tip for a branch that is not the best one
    fn push_alt_tip(&self, branch_tip: &BlockHeader) -> Result<(), BlockchainError> {
        let ancestor = self.get_ancestor(branch_tip);
        let ancestor = match ancestor {
            Ok(ancestor) => Some(ancestor),
            Err(BlockchainError::BlockNotPresent) => None,
            Err(e) => return Err(e),
        };
        let mut inner = write_lock!(self);
        if let Some(ancestor) = ancestor {
            let ancestor_hash = ancestor.block_hash();
            if let Some(idx) = inner
                .best_block
                .alternative_tips
                .iter()
                .position(|hash| ancestor_hash == *hash)
            {
                inner.best_block.alternative_tips.remove(idx);
            }
        }
        inner
            .best_block
            .alternative_tips
            .push(branch_tip.block_hash());
        Ok(())
    }

    /// Returns the chain_params struct for the current network
    fn chain_params(&self) -> ChainParams {
        let inner = read_lock!(self);
        // We clone the parameters here, because we don't want to hold the lock for too long
        inner.consensus.parameters.clone()
    }

    fn get_header_by_height(&self, height: u32) -> Result<DiskBlockHeader, BlockchainError> {
        read_lock!(self)
            .chainstore
            .get_header_by_height(height)?
            .ok_or(BlockchainError::BlockNotPresent)
    }

    fn notify(&self, block: &Block, height: u32, inputs: Option<&HashMap<OutPoint, UtxoData>>) {
        let inner = self.inner.read();
        for client in &inner.subscribers {
            if client.wants_spent_utxos() {
                client.on_block(block, height, inputs);
            } else {
                client.on_block(block, height, None);
            }
        }
    }

    pub fn new(
        mut chainstore: PersistedState,
        network: Network,
        assume_valid: AssumeValidArg,
    ) -> ChainState<PersistedState> {
        let parameters = network.into();
        let genesis = genesis_block(&parameters);

        chainstore
            .save_header(&DiskBlockHeader::FullyValid(genesis.header, 0))
            .expect("Error while saving genesis");

        chainstore
            .update_block_index(0, genesis.block_hash())
            .expect("Error updating index");

        let assume_valid = ChainParams::get_assume_valid(network, assume_valid);

        ChainState {
            inner: RwLock::new(ChainStateInner {
                chainstore,
                acc: Stump::new(),
                best_block: BestChain {
                    best_block: genesis.block_hash(),
                    depth: 0,
                    validation_index: genesis.block_hash(),
                    alternative_tips: Vec::new(),
                },
                subscribers: Vec::new(),
                fee_estimation: (1_f64, 1_f64, 1_f64),
                ibd: true,
                consensus: Consensus { parameters },
                assume_valid,
            }),
        }
    }

    /// Fetches a `DiskBlockHeader` from the chain store given its block hash. Returns an error if
    /// it's not present or if the database operation failed.
    fn get_disk_block_header(&self, hash: &BlockHash) -> Result<DiskBlockHeader, BlockchainError> {
        read_lock!(self)
            .chainstore
            .get_header(hash)?
            .ok_or(BlockchainError::BlockNotPresent)
    }

    /// Returns the parsed accumulator for a given block height, if they are present.
    fn get_roots_for_block(&self, height: u32) -> Result<Option<Stump>, BlockchainError> {
        let acc = { write_lock!(self).chainstore.load_roots_for_block(height)? };

        let Some(acc) = acc else {
            return Ok(None);
        };

        let mut acc = acc.as_slice();
        let acc = Stump::deserialize(&mut acc).map_err(BlockchainError::UtreexoError)?;
        Ok(Some(acc))
    }

    /// Re-indexes the chain if we find ourselves in an undefined state
    ///
    /// Here, we have to find what's the best chain we have, then figure out
    /// how many blocks we have validated. We then need to take extra care
    /// to align our accumulator with the validation index. If the validation_index
    /// diverges from the inner acc by even one block, all proofs will be invalid.
    ///
    /// We start at the alleged validation index, and then we look for the last
    /// accumulator we have. If we don't have the accumulator for the validation index,
    /// we roll back our chain to the last accumulator we have, and then re-validate
    /// them
    fn reindex_chain(&self) -> Result<(), BlockchainError> {
        // Figure out what's the best chain we have
        let best_chain = self.find_best_chain();

        // This is what we've found as the best chain, now let's figure out
        // what is the most recent accumulator we have
        let validation_index_height = self
            .get_block_height(&best_chain.validation_index)?
            .unwrap_or(0);

        debug!(
            "Re-indexing chain, validation index height: {}, best block: {}, depth: {}",
            validation_index_height, best_chain.best_block, best_chain.depth
        );

        // This will be the height of the last accumulator we have
        // Find the last height <= validation_index_height that has an accumulator
        let mut found = None;

        for h in (1..=validation_index_height).rev() {
            if let Some(acc) = self.get_roots_for_block(h)? {
                found = Some((h, acc));
                break;
            }
        }

        if let Some((last_acc_height, acc)) = found {
            // If the last acc height is lower than the validation index height, roll back our
            // database state
            for height in (last_acc_height + 1)..=validation_index_height {
                let header = self.get_header_by_height(height)?;
                self.update_header(&DiskBlockHeader::HeadersOnly(*header, height))?;
            }

            let last_acc_header = self.get_header_by_height(last_acc_height)?;
            let mut inner = write_lock!(self);

            inner.acc = acc;
            inner.best_block = best_chain.clone();
            inner.best_block.validation_index = last_acc_header.block_hash();
        } else {
            // We may reindex as well during chain selection (no acc nor validation index)
            write_lock!(self).best_block = best_chain.clone();
        }

        debug!(
            "Re-indexing complete, new validation index: {} new tip: {}",
            best_chain.validation_index, best_chain.best_block
        );

        Ok(())
    }

    /// Reconstructs the `BestChain` data based on the database indexes and headers. This is useful
    /// to recover from an invalid state or data corruption
    fn find_best_chain(&self) -> BestChain {
        let get_disk_block_hash =
            |height: u32| -> Result<Option<BlockHash>, PersistedState::Error> {
                read_lock!(self).chainstore.get_block_hash(height)
            };

        let mut best_block = get_disk_block_hash(0)
            // TODO: handle possible Err
            .expect("TODO handle chainstore error")
            .expect("expected genesis block, found None");

        let mut depth = 0;
        let mut validation_index = best_block;
        let mut next_height = depth + 1;

        // Iteratively fetch the disk header given the next height
        while let Ok(Some(block_hash)) = get_disk_block_hash(next_height) {
            match self.get_disk_block_header(&block_hash) {
                Ok(DiskBlockHeader::FullyValid(_, height)) => {
                    assert_eq!(height, next_height);
                    validation_index = block_hash;
                }
                Ok(DiskBlockHeader::HeadersOnly(_, height))
                | Ok(DiskBlockHeader::AssumedValid(_, height)) => {
                    assert_eq!(height, next_height);
                }
                _ => break,
            }

            best_block = block_hash;
            depth = next_height;
            next_height += 1;
        }

        BestChain {
            best_block,
            depth,
            validation_index,
            alternative_tips: Vec::new(),
        }
    }

    pub fn load_chain_state(
        mut chainstore: PersistedState,
        network: Network,
        assume_valid: AssumeValidArg,
    ) -> Result<ChainState<PersistedState>, BlockchainError> {
        let best_block = chainstore
            .load_height()?
            .ok_or(BlockchainError::ChainNotInitialized)?;

        debug!("Loaded best block: {best_block:#?}");

        let validation_index_height = chainstore
            .get_header(&best_block.validation_index)?
            .ok_or(BlockchainError::BlockNotPresent)?
            .try_height()?;

        let loaded_acc = chainstore.load_roots_for_block(validation_index_height)?;
        let acc = Self::deserialize_accumulator(loaded_acc)?;

        let inner = ChainStateInner {
            acc,
            best_block,
            chainstore,
            fee_estimation: (1_f64, 1_f64, 1_f64),
            subscribers: Vec::new(),
            ibd: true,
            consensus: Consensus {
                parameters: network.into(),
            },
            assume_valid: ChainParams::get_assume_valid(network, assume_valid),
        };

        info!(
            "ChainState loaded at height: {}, checking if we have all blocks",
            inner.best_block.depth,
        );

        let chainstate = ChainState {
            inner: RwLock::new(inner),
        };

        // Check the integrity of our chain
        chainstate.check_chain_integrity()?;
        Ok(chainstate)
    }

    /// Checks whether our database got a file-level corruption, and if so, reindex.
    ///
    /// This protects us from fs corruption, like random bit-flips or power loss.
    fn check_db_integrity(&self) -> Result<(), BlockchainError> {
        let res = {
            let inner = read_lock!(self);
            inner.chainstore.check_integrity()
        };

        if res.is_err() {
            warn!("We had a data corruption in our database: {res:?}. Reindexing.");
            self.reindex_chain()?;
        }

        Ok(())
    }

    fn check_chain_integrity(&self) -> Result<(), BlockchainError> {
        self.check_db_integrity()?;
        let (best_height, best_hash) = self.get_best_block()?;

        // make sure our index is right for the latest block
        let best_disk_height = self.get_disk_block_header(&best_hash)?.try_height()?;

        if best_height != best_disk_height {
            warn!(
                "Best height {best_height} != disk header height {best_disk_height}. Reindexing."
            );
            self.reindex_chain()?;
            return Ok(());
        }

        // make sure our validation index is pointing to a valid block
        let validation_index = match self.get_validation_index() {
            Ok(idx) => idx,
            Err(e) => {
                warn!("Error with our validation index: {e}. Reindexing.");
                self.reindex_chain()?;
                return Ok(());
            }
        };

        let last_valid_block = self.get_block_hash(validation_index)?;
        let last_valid_header = self.get_disk_block_header(&last_valid_block)?;

        if !matches!(last_valid_header, DiskBlockHeader::FullyValid(_, _)) {
            warn!("The validation index header is not `FullyValid`: {last_valid_header:?}, fetched with validation index {validation_index}. Reindexing.");
            self.reindex_chain()?;
            return Ok(());
        }

        // make sure we don't have valid blocks that are after our validation index
        let next_height = validation_index + 1;
        if next_height > best_height {
            // We don't have a next block, so we are good
            return Ok(());
        }

        let next_block_hash = self.get_block_hash(next_height)?;
        let next_header = self.get_disk_block_header(&next_block_hash)?;

        if matches!(next_header, DiskBlockHeader::FullyValid(_, _)) {
            warn!(
                "We have a valid block at height {next_height}, but our validation index is {validation_index}",
            );
            self.reindex_chain()?;
        }

        Ok(())
    }

    /// Tries to deserialize an accumulator returning an empty [`Stump`] `acc` is None
    fn deserialize_accumulator(acc: Option<Vec<u8>>) -> Result<Stump, BlockchainError> {
        let acc = match acc {
            Some(acc) => acc,
            None => return Ok(Stump::new()),
        };

        let mut acc = acc.as_slice();
        Stump::deserialize(&mut acc).map_err(BlockchainError::UtreexoError)
    }

    fn update_view(
        &self,
        height: u32,
        block: &BlockHeader,
        acc: Stump,
    ) -> Result<(), BlockchainError> {
        let mut inner = write_lock!(self);

        inner
            .chainstore
            .save_header(&DiskBlockHeader::FullyValid(*block, height))?;
        inner
            .chainstore
            .update_block_index(height, block.block_hash())?;

        // save roots for this block
        let mut roots = Vec::new();
        acc.serialize(&mut roots)?;

        inner.chainstore.save_roots_for_block(roots, height)?;

        // Updates our local view of the network
        inner.acc = acc;
        inner.best_block.valid_block(block.block_hash());

        Ok(())
    }

    fn update_tip(&self, best_block: BlockHash, height: u32) {
        let mut inner = write_lock!(self);
        inner.best_block.best_block = best_block;
        inner.best_block.depth = height;
    }

    fn verify_script(&self, height: u32) -> Result<bool, PersistedState::Error> {
        let inner = self.inner.read();
        match inner.assume_valid {
            Some(hash) => {
                match inner.chainstore.get_header(&hash)? {
                    // If the assume-valid block is in the best chain, only verify scripts if we are higher
                    Some(DiskBlockHeader::HeadersOnly(_, assume_h))
                    | Some(DiskBlockHeader::FullyValid(_, assume_h)) => Ok(height > assume_h),
                    // Assume-valid is not in the best chain, so verify all the scripts
                    _ => Ok(true),
                }
            }
            None => Ok(true),
        }
    }
    pub fn acc(&self) -> Stump {
        read_lock!(self).acc.to_owned()
    }
    /// Returns the next required work for the next block, usually it's just the last block's target
    /// but if we are in a retarget period, it's calculated from the last 2016 blocks.
    fn get_next_required_work(
        &self,
        last_block: &BlockHeader,
        next_height: u32,
        next_header: &BlockHeader,
    ) -> Result<Target, BlockchainError> {
        let params: ChainParams = self.chain_params();
        // Special testnet rule, if a block takes more than 20 minutes to mine, we can
        // mine a block with diff 1
        if params.params.allow_min_difficulty_blocks
            && last_block.time + params.params.pow_target_spacing as u32 * 2 < next_header.time
        {
            return Ok(params.params.max_attainable_target);
        }

        // Regtest don't have retarget
        if !params.params.no_pow_retargeting && (next_height) % 2016 == 0 {
            // First block in this epoch
            let first_block = self.get_header_by_height(next_height - 2016)?;
            let last_block = self.get_header_by_height(next_height - 1)?;

            let target =
                Consensus::calc_next_work_required(&last_block, &first_block, self.chain_params());

            if target < params.params.max_attainable_target {
                return Ok(target);
            }

            return Ok(params.params.max_attainable_target);
        }

        Ok(last_block.target())
    }

    /// Check timestamp against prev for difficulty-adjustment blocks to prevent timewarp attacks.
    /// Must only be called for non-genesis block headers.
    fn check_bip94_block(&self, block: &BlockHeader, height: u32) -> Result<(), BlockchainError> {
        let params = self.chain_params();
        // Difficulty adjustment window
        let window = params.params.miner_confirmation_window;

        if !params.enforce_bip94 || height % window != 0 {
            return Ok(());
        }

        let prev_header = self.get_header_by_height(height - 1)?;
        Ok(Consensus::check_bip94_time(block, &prev_header)?)
    }

    /// Validates the block without checking whether the inputs are present in the UTXO set. This
    /// function contains the core validation logic.
    ///
    /// The methods `BlockchainInterface::validate_block` and `UpdatableChainstate::connect_block`
    /// call this and additionally verify the inclusion proof (i.e., they perform full validation).
    pub fn validate_block_no_acc(
        &self,
        block: &Block,
        height: u32,
        inputs: HashMap<OutPoint, UtxoData>,
    ) -> Result<(), BlockchainError> {
        read_lock!(self).consensus.check_block(block, height)?;

        // Validate block transactions
        let subsidy = read_lock!(self).consensus.get_subsidy(height);
        let verify_script = self.verify_script(height)?;
        #[cfg(feature = "bitcoinkernel")]
        let flags = self
            .chain_params()
            .get_validation_flags(height, block.block_hash());
        #[cfg(not(feature = "bitcoinkernel"))]
        let flags = 0;

        Consensus::verify_block_transactions(
            height,
            inputs,
            &block.txdata,
            subsidy,
            verify_script,
            flags,
        )?;
        Ok(())
    }
}

impl<PersistedState: ChainStore> BlockchainInterface for ChainState<PersistedState> {
    type Error = BlockchainError;

    fn get_params(&self) -> bitcoin::params::Params {
        self.chain_params().params
    }

    fn acc(&self) -> Stump {
        read_lock!(self).acc.to_owned()
    }

    fn get_fork_point(&self, block: BlockHash) -> Result<BlockHash, Self::Error> {
        let fork_point = self.find_fork_point(&self.get_block_header(&block)?)?;
        Ok(fork_point.block_hash())
    }

    fn update_acc(
        &self,
        acc: Stump,
        block: &Block,
        height: u32,
        proof: Proof,
        del_hashes: Vec<sha256::Hash>,
    ) -> Result<Stump, Self::Error> {
        Consensus::update_acc(&acc, block, height, proof, del_hashes)
    }

    fn get_chain_tips(&self) -> Result<Vec<BlockHash>, Self::Error> {
        let inner = read_lock!(self);
        let mut tips = Vec::new();

        tips.push(inner.best_block.best_block);
        tips.extend(inner.best_block.alternative_tips.iter());

        Ok(tips)
    }

    fn validate_block(
        &self,
        block: &Block,
        proof: Proof,
        inputs: HashMap<OutPoint, UtxoData>,
        del_hashes: Vec<sha256::Hash>,
        acc: Stump,
    ) -> Result<(), Self::Error> {
        // Convert to BitcoinNodeHashes, from rustreexo
        let del_hashes: Vec<_> = del_hashes.into_iter().map(Into::into).collect();

        if !acc.verify(&proof, &del_hashes)? {
            return Err(BlockValidationErrors::InvalidProof)?;
        }

        let height = self
            .get_disk_block_header(&block.block_hash())?
            .try_height()?;

        self.validate_block_no_acc(block, height, inputs)
    }

    fn get_block_locator_for_tip(&self, tip: BlockHash) -> Result<Vec<BlockHash>, BlockchainError> {
        let mut hashes = Vec::new();
        let height = self.get_disk_block_header(&tip)?.try_height()?;

        let mut index = height;
        let mut current_height = height;
        let mut current_header = self.get_disk_block_header(&tip)?;
        let mut step = 1;

        while index > 0 {
            while current_height > index {
                current_header = self.get_ancestor(&current_header)?;
                current_height -= 1;
            }

            if hashes.len() >= 10 {
                step *= 2;
            }

            hashes.push(current_header.block_hash());

            if index > step {
                index -= step;
            } else {
                break;
            }
        }

        // genesis
        hashes.push(self.get_block_hash(0)?);
        Ok(hashes)
    }

    fn is_in_ibd(&self) -> bool {
        self.inner.read().ibd
    }

    fn get_block_height(&self, hash: &BlockHash) -> Result<Option<u32>, Self::Error> {
        self.get_disk_block_header(hash)
            .map(|header| header.height())
    }

    fn get_block_hash(&self, height: u32) -> Result<BlockHash, Self::Error> {
        read_lock!(self)
            .chainstore
            .get_block_hash(height)?
            .ok_or(BlockchainError::BlockNotPresent)
    }

    fn get_tx(&self, _txid: &Txid) -> Result<Option<Transaction>, Self::Error> {
        unimplemented!("This chainstate doesn't hold any tx")
    }

    fn get_height(&self) -> Result<u32, Self::Error> {
        let inner = read_lock!(self);
        Ok(inner.best_block.depth)
    }

    fn estimate_fee(&self, target: usize) -> Result<f64, Self::Error> {
        let inner = read_lock!(self);
        if target == 1 {
            Ok(inner.fee_estimation.0)
        } else if target == 10 {
            Ok(inner.fee_estimation.1)
        } else {
            Ok(inner.fee_estimation.2)
        }
    }

    fn get_block(&self, _hash: &BlockHash) -> Result<Block, Self::Error> {
        unimplemented!("This chainstate doesn't hold full blocks")
    }

    fn get_best_block(&self) -> Result<(u32, BlockHash), Self::Error> {
        let inner = read_lock!(self);
        Ok((inner.best_block.depth, inner.best_block.best_block))
    }

    fn get_block_header(&self, hash: &BlockHash) -> Result<BlockHeader, Self::Error> {
        let inner = read_lock!(self);
        if let Some(header) = inner.chainstore.get_header(hash)? {
            return Ok(*header);
        }
        Err(BlockchainError::BlockNotPresent)
    }

    fn subscribe(&self, tx: Arc<dyn BlockConsumer>) {
        let mut inner = self.inner.write();
        inner.subscribers.push(tx);
    }

    fn get_block_locator(&self) -> Result<Vec<BlockHash>, BlockchainError> {
        let top_height = self.get_height()?;
        let mut indexes = Vec::new();
        let mut step = 1;
        let mut index = top_height;
        while index > 0 {
            if indexes.len() >= 10 {
                step *= 2;
            }
            indexes.push(index);
            if index > step {
                index -= step;
            } else {
                break;
            }
        }
        indexes.push(0);
        let hashes = indexes
            .iter()
            .flat_map(|idx| self.get_block_hash(*idx))
            .collect();

        Ok(hashes)
    }

    fn get_validation_index(&self) -> Result<u32, Self::Error> {
        let inner = self.inner.read();
        let validation = inner.best_block.validation_index;
        let header = self.get_disk_block_header(&validation)?;
        // The last validated disk header can only be FullyValid
        if let DiskBlockHeader::FullyValid(_, height) = header {
            return Ok(height);
        }

        Err(BlockchainError::BadValidationIndex)
    }

    fn is_coinbase_mature(&self, height: u32, block: BlockHash) -> Result<bool, Self::Error> {
        let chain_params = self.chain_params();
        let current_height = self.get_disk_block_header(&block)?.try_height()?;

        Ok(height + chain_params.coinbase_maturity <= current_height)
    }
}

impl<PersistedState: ChainStore> UpdatableChainstate for ChainState<PersistedState> {
    fn switch_chain(&self, new_tip: BlockHash) -> Result<(), BlockchainError> {
        let new_tip = self.get_block_header(&new_tip)?;
        self.reorg(new_tip)
    }

    fn get_acc(&self) -> Stump {
        self.acc()
    }

    fn mark_block_as_valid(&self, block: BlockHash) -> Result<(), BlockchainError> {
        let header = self.get_disk_block_header(&block)?;
        let height = header.try_height()?;

        let new_header = DiskBlockHeader::FullyValid(*header, height);
        self.update_header(&new_header)
    }

    fn mark_chain_as_assumed(
        &self,
        acc: Stump,
        assumed_hash: BlockHash,
    ) -> Result<bool, BlockchainError> {
        let assumed_header = self.get_disk_block_header(&assumed_hash)?;

        let mut header = assumed_header;
        let mut hash = assumed_hash;
        loop {
            if self.is_genesis(&header) {
                break;
            }

            let height = header.try_height()?;
            self.update_header_and_index(
                &DiskBlockHeader::FullyValid(*header, height),
                hash,
                height,
            )?;

            // Move to the previous block
            header = self.get_ancestor(&header)?;
            hash = header.block_hash();
        }

        // Update the tip and accumulator data with our assumed tip
        self.update_view(assumed_header.try_height()?, &assumed_header, acc)?;

        Ok(true)
    }

    fn invalidate_block(&self, block: BlockHash) -> Result<(), BlockchainError> {
        let height = self.get_disk_block_header(&block)?.try_height()?;
        let current_height = self.get_height()?;

        // Mark all blocks after this one as invalid
        for h in height..=current_height {
            let hash = self.get_block_hash(h)?;
            let header = self.get_block_header(&hash)?;
            let new_header = DiskBlockHeader::InvalidChain(header);
            self.update_header(&new_header)?;
        }
        // Row back to our previous state. Note that acc doesn't actually change in this case
        // only the currently best known block.
        self.update_tip(
            self.get_ancestor(&self.get_block_header(&block)?)?
                .block_hash(),
            height - 1,
        );
        Ok(())
    }

    fn toggle_ibd(&self, is_ibd: bool) {
        let mut inner = write_lock!(self);
        inner.ibd = is_ibd;
    }

    fn connect_block(
        &self,
        block: &Block,
        proof: Proof,
        inputs: HashMap<OutPoint, UtxoData>,
        del_hashes: Vec<sha256::Hash>,
    ) -> Result<u32, BlockchainError> {
        let header = self.get_disk_block_header(&block.block_hash())?;
        let height = match header {
            DiskBlockHeader::FullyValid(_, height) => {
                let validation_index = self.get_validation_index()?;

                // If this block is not our validation index, but the caller is trying to connect
                // it, this is a logical error, and we will have spurious errors, specially with
                // invalid proof. They don't mean the block is invalid, just that we are using the
                // wrong accumulator, since we are not processing the right block.
                if height != validation_index {
                    return Err(BlockValidationErrors::BlockDoesntExtendTip)?;
                }

                // If this block is our validation index, but it's fully valid, this clearly means
                // there was some corruption of our state. If we don't process this block, we will
                // be stuck forever.
                //
                // Note: You may think "just kick the validation index one block further and we are
                // good". But this is not the case, because we still need to update our
                // accumulator. Otherwise, the next block will always have an invalid proof
                // (because the accumulator is not updated).
                height
            },

            // Our called tried to connect_block on a block that is not the next one in our chain
            DiskBlockHeader::Orphan(_)
            | DiskBlockHeader::AssumedValid(_, _) // this will be validated by a partial chain
            | DiskBlockHeader::InFork(_, _)
            | DiskBlockHeader::InvalidChain(_) => return Err(BlockValidationErrors::BlockExtendsAnOrphanChain)?,

            DiskBlockHeader::HeadersOnly(_, height) => {
                let validation_index = self.get_validation_index()?;

                // In case of a `HeadersOnly` block, we need to check if the height is
                // the next one after the validation index. If not, we would be trying to
                // connect a block where our accumulator isn't the right one. So the proof will
                // always be invalid.
                if height != validation_index + 1 {
                    return Err(BlockValidationErrors::BlockDoesntExtendTip)?;
                }

                height
            }
        };

        // Clone inputs only if a subscriber wants spent utxos
        let inputs_for_notifications = self
            .inner
            .read()
            .subscribers
            .iter()
            .any(|subscriber| subscriber.wants_spent_utxos())
            .then(|| inputs.clone());

        self.validate_block_no_acc(block, height, inputs)?;
        let acc = Consensus::update_acc(&self.acc(), block, height, proof, del_hashes)?;

        self.update_view(height, &block.header, acc)?;

        info!(
            "New tip! hash={} height={height} tx_count={}",
            block.block_hash(),
            block.txdata.len()
        );

        #[cfg(feature = "metrics")]
        metrics::get_metrics().block_height.set(height.into());

        if !self.is_in_ibd() || height % 100_000 == 0 {
            self.flush()?;
        }

        // Notify others we have a new block
        self.notify(block, height, inputs_for_notifications.as_ref());
        Ok(height)
    }

    fn handle_transaction(&self) -> Result<(), BlockchainError> {
        unimplemented!("This chain_state has no mempool")
    }

    fn flush(&self) -> Result<(), BlockchainError> {
        let mut inner = write_lock!(self);
        let best_block = inner.best_block.clone();

        inner.chainstore.save_height(&best_block)?;
        inner.chainstore.flush()?;

        Ok(())
    }

    fn accept_header(&self, header: BlockHeader) -> Result<(), BlockchainError> {
        let disk_header = self.get_disk_block_header(&header.block_hash());

        match disk_header {
            Err(e @ BlockchainError::Database(_)) => {
                // If there's a database error we don't know if we already
                // have the header or not
                return Err(e);
            }
            Ok(found) => {
                // Possibly reindex to recompute the best_block field
                self.maybe_reindex(&found)?;
                // We already have this header
                return Ok(());
            }
            _ => (),
        }
        // The best block we know of
        let best_block = self.get_best_block()?;

        // Do validation in this header
        let block_hash = self.validate_header(&header)?;

        // Update our current tip
        if header.prev_blockhash == best_block.1 {
            let height = best_block.0 + 1;
            debug!("Header builds on top of our best chain");

            write_lock!(self).best_block.new_block(block_hash, height);
            let disk_header = DiskBlockHeader::HeadersOnly(header, height);

            self.update_header_and_index(&disk_header, block_hash, height)?;
        } else {
            debug!("Header not in the best chain");

            self.maybe_reorg(header)?;
        }

        Ok(())
    }

    fn get_root_hashes(&self) -> Vec<BitcoinNodeHash> {
        let inner = read_lock!(self);
        inner.acc.roots.clone()
    }

    fn get_partial_chain(
        &self,
        initial_height: u32,
        final_height: u32,
        acc: Stump,
    ) -> Result<super::partial_chain::PartialChainState, BlockchainError> {
        let blocks = (0..=final_height)
            .map(|height| {
                let hash = self
                    .get_block_hash(height)
                    .expect("Block should be present");
                *self
                    .get_disk_block_header(&hash)
                    .expect("Block should be present")
            })
            .collect();

        let inner = PartialChainStateInner {
            error: None,
            blocks,
            consensus: Consensus {
                parameters: self.chain_params(),
            },
            current_acc: acc,
            final_height,
            assume_valid: false,
            current_height: initial_height,
        };

        Ok(PartialChainState(UnsafeCell::new(inner)))
    }
}

impl<T: ChainStore> TryFrom<ChainStateBuilder<T>> for ChainState<T> {
    type Error = BlockchainBuilderError;

    fn try_from(mut builder: ChainStateBuilder<T>) -> Result<Self, Self::Error> {
        let inner = ChainStateInner {
            acc: builder.acc().unwrap_or_default(),
            chainstore: builder.chainstore()?,
            best_block: builder.best_block()?,
            assume_valid: builder.assume_valid(),
            ibd: builder.ibd(),
            subscribers: Vec::new(),
            fee_estimation: (1_f64, 1_f64, 1_f64),
            consensus: Consensus {
                parameters: builder.chain_params()?,
            },
        };

        let inner = RwLock::new(inner);
        Ok(Self { inner })
    }
}

#[macro_export]
/// Grabs a RwLock for reading
macro_rules! read_lock {
    ($obj:ident) => {
        $obj.inner.read()
    };
}
#[macro_export]
/// Grabs a RwLock for writing
macro_rules! write_lock {
    ($obj:ident) => {
        $obj.inner.write()
    };
}

#[cfg(all(test, feature = "flat-chainstore"))]
mod test {
    use core::str::FromStr;
    use std::format;
    use std::fs::File;
    use std::io::Cursor;
    use std::vec::Vec;

    use bitcoin::block::Header as BlockHeader;
    use bitcoin::consensus::deserialize;
    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::consensus::Decodable;
    use bitcoin::constants::genesis_block;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::Network;
    use bitcoin::OutPoint;
    use floresta_common::assert_ok;
    use floresta_common::bhash;
    use rand::Rng;
    use rustreexo::accumulator::proof::Proof;
    use rustreexo::accumulator::stump::Stump;

    use super::BlockchainInterface;
    use super::ChainParams;
    use super::ChainState;
    use super::DiskBlockHeader;
    use super::UpdatableChainstate;
    use crate::prelude::HashMap;
    use crate::pruned_utreexo::consensus::Consensus;
    use crate::pruned_utreexo::utxo_data::UtxoData;
    use crate::AssumeValidArg;
    use crate::BlockchainError;
    use crate::FlatChainStore;

    fn setup_test_chain(
        network: Network,
        assume_valid_arg: AssumeValidArg,
    ) -> ChainState<FlatChainStore> {
        let test_id = rand::random::<u64>();
        let config = crate::FlatChainStoreConfig {
            block_index_size: Some(32_768),
            headers_file_size: Some(32_768),
            fork_file_size: Some(10_000), // Will be rounded up to 16,384
            cache_size: Some(10),
            file_permission: Some(0o660),
            path: format!("./tmp-db/{test_id}/"),
        };

        let chainstore = FlatChainStore::new(config).unwrap();
        ChainState::new(chainstore, network, assume_valid_arg)
    }

    fn decode_block_and_inputs(
        block_file: File,
        stxos_file: File,
    ) -> (Block, HashMap<OutPoint, UtxoData>) {
        let block_bytes = zstd::decode_all(block_file).unwrap();
        let block: Block = deserialize(&block_bytes).unwrap();

        // Get utxos spent in the block
        let stxos_bytes = zstd::decode_all(stxos_file).unwrap();
        let mut stxos: Vec<UtxoData> =
            serde_json::from_slice(&stxos_bytes).expect("Failed to deserialize JSON");

        let inputs = block
            .txdata
            .iter()
            .skip(1) // Skip the coinbase transaction
            .flat_map(|tx| &tx.input)
            .map(|txin| (txin.previous_output, stxos.remove(0)))
            .collect();

        assert!(stxos.is_empty(), "Moved all stxos to the inputs map");

        (block, inputs)
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore = "this test is very slow in debug mode")]
    fn test_validate_many_inputs_block() {
        let block_file = File::open("./testdata/block_367891/raw.zst").unwrap();
        let stxos_file = File::open("./testdata/block_367891/spent_utxos.zst").unwrap();
        let (block, inputs) = decode_block_and_inputs(block_file, stxos_file);

        assert_eq!(
            block.block_hash(),
            bhash!("000000000000000012ea0ca9579299ec120e3f57e7c309216884872592b29970"),
        );

        // Check whether the block validation passes or not
        let chain = setup_test_chain(Network::Bitcoin, AssumeValidArg::Disabled);
        chain
            .validate_block_no_acc(&block, 367891, inputs)
            .expect("Block must be valid");
    }

    #[test]
    fn test_validate_full_block() {
        let block_file = File::open("./testdata/block_866342/raw.zst").unwrap();
        let stxos_file = File::open("./testdata/block_866342/spent_utxos.zst").unwrap();
        let (block, inputs) = decode_block_and_inputs(block_file, stxos_file);

        assert_eq!(
            block.block_hash(),
            bhash!("000000000000000000014ce9ba7c6760053c3c82ce6ab43d60afb101d3c8f1f1"),
        );

        // Check whether the block validation passes or not
        let chain = setup_test_chain(Network::Bitcoin, AssumeValidArg::Disabled);
        chain
            .validate_block_no_acc(&block, 866342, inputs)
            .expect("Block must be valid");
    }

    #[test]
    fn accept_mainnet_headers() {
        // Accepts the first 10,237 mainnet headers
        let file = include_bytes!("../../testdata/headers.zst");
        let uncompressed: Vec<u8> = zstd::decode_all(Cursor::new(file)).unwrap();
        let mut buffer = uncompressed.as_slice();

        let mut headers = Vec::new();
        let chain = setup_test_chain(Network::Bitcoin, AssumeValidArg::Hardcoded);

        while let Ok(header) = BlockHeader::consensus_decode(&mut buffer) {
            chain.accept_header(header).unwrap();
            headers.push(header);
        }
        assert_eq!(headers.len(), 10_237);

        // Check that our `BestChain` is correct
        assert_eq!(
            chain.get_best_block().unwrap(),
            (
                10_236,
                bhash!("000000004f74d42205b9d7ae1cb5c1591e723894a71358fce73b7e0919628161")
            ),
        );
        assert_eq!(chain.get_validation_index().unwrap(), 0);

        // Check all headers that we have
        for i in 0..=10_236 {
            let hash = chain.get_block_hash(i).unwrap();
            let header = chain.get_disk_block_header(&hash).unwrap();

            match header {
                DiskBlockHeader::FullyValid(h, height) if i == 0 => {
                    assert_eq!(height, 0);
                    assert_eq!(h.block_hash(), hash);
                    assert_eq!(
                        hash,
                        bhash!("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f")
                    );
                }
                // Non-genesis blocks have not been validated, only accepted in the best chain
                DiskBlockHeader::HeadersOnly(h, height) => {
                    assert_eq!(height, i);
                    assert_eq!(h.block_hash(), hash);
                }
                _ => panic!("Header at height {i} should be `HeadersOnly`: {header:?}"),
            }
            assert_eq!(&*header, headers.get(i as usize).unwrap());
        }

        // Check that we don't have more headers (nor indexed hashes)
        for i in 10_237..11_000 {
            match chain.get_block_hash(i) {
                Err(BlockchainError::BlockNotPresent) => {}
                _ => panic!("Should not have found a hash for height {i}"),
            }
            match chain.get_header_by_height(i) {
                Err(BlockchainError::BlockNotPresent) => {}
                _ => panic!("Should not have found a header at height {i}"),
            }
        }
    }

    #[test]
    fn accept_first_signet_headers() {
        // Accepts the first 2016 signet headers
        let file = include_bytes!("../../testdata/signet_headers.zst");
        let uncompressed: Vec<u8> = zstd::decode_all(Cursor::new(file)).unwrap();
        let mut buffer = uncompressed.as_slice();

        let chain = setup_test_chain(Network::Signet, AssumeValidArg::Hardcoded);
        while let Ok(header) = BlockHeader::consensus_decode(&mut buffer) {
            chain.accept_header(header).unwrap();
        }
    }

    #[test]
    fn test_calc_next_work_required() {
        let first_block: BlockHeader = deserialize_hex("0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a008f4d5fae77031e8ad22203").unwrap();
        let last_block: BlockHeader = deserialize_hex("00000020dec6741f7dc5df6661bcb2d3ec2fceb14bd0e6def3db80da904ed1eeb8000000d1f308132e6a72852c04b059e92928ea891ae6d513cd3e67436f908c804ec7be51df535fae77031e4d00f800").unwrap();

        let next_target = Consensus::calc_next_work_required(
            &last_block,
            &first_block,
            ChainParams::from(Network::Signet),
        );

        assert_eq!(0x1e012fa7, next_target.to_compact_lossy().to_consensus());
    }

    #[test]
    fn test_reorg() {
        let chain = setup_test_chain(Network::Regtest, AssumeValidArg::Hardcoded);
        let json_blocks = include_str!("../../testdata/test_reorg.json");
        let blocks: Vec<Vec<&str>> = serde_json::from_str(json_blocks).unwrap();
        let mut fork_acc = Stump::default();

        let parse_blocks = |blocks: &[&str]| {
            blocks
                .iter()
                .map(|s| deserialize_hex(s).unwrap())
                .collect::<Vec<Block>>()
        };

        let short_chain = parse_blocks(&blocks[0]);
        let long_chain = parse_blocks(&blocks[1]);

        assert_eq!(
            short_chain[0].header.prev_blockhash,
            genesis_block(Network::Regtest).block_hash(),
            "The first block we have is the next after genesis",
        );
        assert_eq!(short_chain.len(), 10, "First 10 blocks");
        assert_eq!(long_chain.len(), 11, "Next 11 fork blocks after the fifth");
        assert_eq!(
            long_chain[0].header.prev_blockhash,
            short_chain[4].block_hash(),
        );

        // Connect the first 10 blocks after genesis
        for block in short_chain {
            chain.accept_header(block.header).unwrap();
            chain
                .connect_block(&block, Proof::default(), HashMap::new(), Vec::new())
                .unwrap();

            if block.block_hash()
                == bhash!("45c74beefa2a110715377e023d4260168b4cafbb0891f3b0869aea30867acc87")
            {
                // This is the block we will reorg to
                fork_acc = chain.acc();
            }
        }

        let expected = (
            10,
            bhash!("6e9c49a19038f7db8d13f6c2e70566385536ea11975528b557799e08a014e784"),
        );

        assert_eq!(chain.get_best_block().unwrap(), expected);

        // Then accept a fork chain with 11 new blocks, building on the previous height 5 block
        for fork_block in long_chain.iter() {
            chain.accept_header(fork_block.header).unwrap();
        }

        let expected = (
            16,
            bhash!("4572ac401b94915dde6c4957b706abdb13b5824b000cad7f6065ebd9aea6dad1"),
        );

        assert_eq!(chain.get_best_block().unwrap(), expected);
        assert_eq!(
            chain.acc(),
            fork_acc,
            "The accumulator should not change when accepting headers only",
        );

        // Actually connect the fork chain
        for fork in long_chain {
            chain
                .connect_block(&fork, Proof::default(), HashMap::new(), Vec::new())
                .unwrap();
        }

        for i in 1..=chain.get_height().unwrap() {
            let hash = chain.get_block_hash(i).unwrap();
            let header = chain.get_disk_block_header(&hash).unwrap();
            let header_by_height = chain.get_header_by_height(i).unwrap();

            assert_eq!(header, header_by_height);
            assert_eq!(header.prev_blockhash, chain.get_block_hash(i - 1).unwrap());

            if let DiskBlockHeader::FullyValid(..) = header {
                continue;
            } else {
                panic!("Expected block at height {i} to be FullyValid, got: {header:?}");
            }
        }
    }

    #[test]
    fn test_chainstate_functions() {
        let file = include_bytes!("../../testdata/signet_headers.zst");
        let uncompressed: Vec<u8> = zstd::decode_all(Cursor::new(file)).unwrap();
        let mut buffer = uncompressed.as_slice();

        let chain = setup_test_chain(Network::Signet, AssumeValidArg::Hardcoded);
        let mut headers: Vec<BlockHeader> = Vec::new();
        while let Ok(header) = BlockHeader::consensus_decode(&mut buffer) {
            headers.push(header);
        }

        headers.remove(0);

        // push_headers
        assert_ok!(chain.push_headers(headers.clone(), 1));

        // get_header_by_height
        assert_eq!(*chain.get_header_by_height(1).unwrap(), headers[0]);

        // find_best_chain
        assert_eq!(chain.find_best_chain().depth, 2015);

        // get_block_locator_for_tip
        assert!(!chain
            .get_block_locator_for_tip(read_lock!(chain).best_block.best_block)
            .unwrap()
            .is_empty());

        // get_block_locator
        assert!(!chain.get_block_locator().unwrap().is_empty());

        // invalidate_block
        let random_height = rand::thread_rng().gen_range(1..=2014);

        chain
            .invalidate_block(headers[random_height].prev_blockhash)
            .unwrap();

        assert_eq!(chain.get_height().unwrap() as usize, random_height - 1);

        // update_tip
        chain.update_tip(headers[1].prev_blockhash, 1);
        assert_eq!(
            read_lock!(chain).best_block.best_block,
            headers[1].prev_blockhash
        );
    }
}
