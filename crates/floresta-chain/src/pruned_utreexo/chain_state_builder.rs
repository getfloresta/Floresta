//! This module provides a builder pattern for constructing ChainState instances with various
//! optional configurations.
//!
//! It includes:
//! - Chain parameters
//! - Chainstore backend
//! - Initial block download status
//! - Assumed valid blocks for validation optimization
//! - UTREEXO accumulator state
//! - Current chain tip and header
use bitcoin::block::Header as BlockHeader;
use bitcoin::BlockHash;
use bitcoin::Network;
use rustreexo::accumulator::stump::Stump;

use super::chain_state::ChainState;
use super::chainparams::ChainParams;
use crate::prelude::Vec;
use crate::pruned_utreexo::Box;
use crate::AssumeValidArg;
use crate::BestChain;
use crate::ChainStore;
use crate::DatabaseError;
use crate::DiskBlockHeader;

#[derive(Debug)]
/// Represents errors that can occur during the construction of a ChainState instance.
pub enum BlockchainBuilderError {
    /// Indicates that the chainstore is missing.
    MissingChainstore,

    /// Indicates that the chain parameters are missing.
    MissingChainParams,

    /// Indicates that the `tip` and `tip_header` parameters were not provided together.
    IncompleteTip,

    /// Error while trying to save initial data.
    Database(Box<dyn DatabaseError>),
}

impl core::fmt::Display for BlockchainBuilderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BlockchainBuilderError::MissingChainstore => write!(f, "chainstore is missing"),
            BlockchainBuilderError::MissingChainParams => write!(f, "chain parameters are missing"),
            BlockchainBuilderError::IncompleteTip => {
                write!(f, "tip and tip_header must be provided together")
            }
            BlockchainBuilderError::Database(e) => write!(f, "database error: {:?}", e),
        }
    }
}

#[derive(Clone, Debug, Default)]
/// A builder for configuring and creating a `ChainState`.
///
/// It implements a few methods to access and modify the settings. Call `.build()` to consume the
/// builder and produce the `ChainState`.
pub struct ChainStateBuilder<PersistedState: ChainStore> {
    /// The accumulator stump.
    acc: Option<Stump>,

    /// The chain store.
    chainstore: Option<PersistedState>,

    /// Indicates whether the builder is in initial block download mode.
    ibd: bool,

    /// The chain parameters.
    chain_params: Option<ChainParams>,

    /// The block hash that is assumed to be valid.
    assume_valid: Option<BlockHash>,

    /// The current chain tip.
    tip: Option<(BlockHash, u32)>,

    /// The first block header.
    tip_header: Option<BlockHeader>,
}

impl<T: ChainStore> ChainStateBuilder<T> {
    /// Creates a new instance of ChainStateBuilder.
    pub fn new() -> Self {
        ChainStateBuilder {
            acc: None,
            chainstore: None,
            ibd: true,
            chain_params: None,
            assume_valid: None,
            tip: None,
            tip_header: None,
        }
    }

    /// Builds the chain state. Returns error if the `chainstore` or `chain_params` are missing, or
    /// if only one of `tip_header` and `tip` is set (either set both or none).
    pub fn build(mut self) -> Result<ChainState<T>, BlockchainBuilderError> {
        let chainstore = self
            .chainstore
            .as_mut()
            .ok_or(BlockchainBuilderError::MissingChainstore)?;

        // Tip header and tip tuple must come as a pair (both Some or both None)
        match (self.tip_header, self.tip) {
            // Persist both values
            (Some(first_header), Some((height, block_hash))) => {
                chainstore.save_header(&DiskBlockHeader::FullyValid(first_header, block_hash))?;
                chainstore.update_block_index(block_hash, height)?;
            }
            // Do nothing
            (None, None) => {}
            // One was Some and one None, return error
            _ => return Err(BlockchainBuilderError::IncompleteTip),
        }

        ChainState::try_from(self)
    }

    /// Set the chainstore backend, implementing [ChainStore]. **Always required**.
    pub fn with_chainstore(mut self, chainstore: T) -> Self {
        self.chainstore = Some(chainstore);
        self
    }

    /// Enable or disable Initial Block Download (IBD) mode.
    pub fn toggle_ibd(mut self, ibd: bool) -> Self {
        self.ibd = ibd;
        self
    }

    /// Sets the chain parameters. **Always required**.
    pub fn with_chain_params(mut self, chain_params: ChainParams) -> Self {
        self.chain_params = Some(chain_params);
        self
    }

    /// Sets the assume-valid argument, which can be `Disabled`, `Hardcoded` or `UserInput`. This
    /// option is used to skip script validation up to the specified block, speeding up IBD.
    pub fn with_assume_valid(mut self, arg: AssumeValidArg, network: Network) -> Self {
        // TODO: handle possible Err
        self.assume_valid = ChainParams::get_assume_valid(network, arg);
        self
    }

    /// Sets the utreexo accumulator, assumed as the initial state.
    pub fn assume_utreexo(mut self, acc: Stump) -> Self {
        self.acc = Some(acc);
        self
    }

    /// Sets the tip block data, assumed as the initial state.
    pub fn with_tip(mut self, tip: (BlockHash, u32), header: BlockHeader) -> Self {
        self.tip = Some(tip);
        self.tip_header = Some(header);
        self
    }

    /// Returns the utreexo accumulator that was set or None if empty.
    pub(super) fn acc(&self) -> Option<Stump> {
        self.acc.clone()
    }

    /// Take the chainstore out of the builder, returning it or an error if missing.
    pub(super) fn chainstore(&mut self) -> Result<T, BlockchainBuilderError> {
        self.chainstore
            .take()
            .ok_or(BlockchainBuilderError::MissingChainstore)
    }

    /// Returns whether Initial Block Download (IBD) mode is enabled.
    pub(super) fn ibd(&self) -> bool {
        self.ibd
    }

    /// Get the chain parameters, returning an error if they haven't been set.
    pub(super) fn chain_params(&self) -> Result<ChainParams, BlockchainBuilderError> {
        self.chain_params
            .clone()
            .ok_or(BlockchainBuilderError::MissingChainParams)
    }

    /// Get the specified best tip as a `BestChain`, or fall back to the genesis block if unset.
    /// The returned validation index is always the genesis block (to skip script validation, you
    /// must specify an `assume_valid` hash).
    ///
    /// Returns an error if the chain parameters are missing.
    pub(super) fn best_block(&self) -> Result<BestChain, BlockchainBuilderError> {
        let genesis_hash = self.chain_params()?.genesis.header.block_hash();

        let (best_block, depth) = match self.tip {
            Some(value) => value,
            None => (genesis_hash, 0),
        };

        Ok(BestChain {
            best_block,
            depth,
            validation_index: genesis_hash,
            alternative_tips: Vec::new(),
        })
    }

    /// Returns the block hash of the assume-valid option, if enabled.
    pub(super) fn assume_valid(&self) -> Option<BlockHash> {
        self.assume_valid
    }
}
