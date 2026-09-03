// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pruned utreexo module handles the full blockchain logic: validation, state tracking and
//! interfacing. This blockchain backend does not store the historical blocks, it's pruned.
//!
//! This module file defines the main traits for an utreexo-enabled chain backend:
//!
//! - [BlockchainInterface]: The main interface for interacting with the backend
//! - [UpdatableChainstate]: Trait defining methods for updating the chain state

extern crate alloc;

pub mod chain_state;
pub mod chain_state_builder;
pub mod chainparams;
pub mod chainstore;
#[macro_use]
pub mod error;
pub mod consensus;
#[cfg(feature = "flat-chainstore")]
pub mod flat_chain_store;
pub mod merkle;
pub mod partial_chain;
pub mod udata;

use alloc::sync::Arc;
use core::error::Error;

use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::OutPoint;
use bitcoin::Work;
use bitcoin::block::Header as BlockHeader;
use bitcoin::hashes::sha256;
use rustreexo::node_hash::BitcoinNodeHash;
use rustreexo::proof::Proof;
use rustreexo::stump::Stump;

use self::chainstore::ChainStoreWarning;
use self::partial_chain::PartialChainState;
use crate::BlockConsumer;
use crate::BlockchainError;
use crate::prelude::*;
use crate::pruned_utreexo::utxo_data::UtxoData;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
/// Our current IBD state, meaning which startup phase are we, if any.
///
/// During startup, our node will go through a bootstrap process called Initial Block Download,
/// where it will catch up with the network. This enum is a simple state machine that represents
/// which state we are currently in.
pub enum IBDState {
    #[default]
    /// Downloading headers to establish which is the most-work chain.
    ///
    /// During this phase, we only download and check headers. We will finish when we are convinced
    /// this is the most work chain available.
    HeadersSync,

    /// Downloading and checking blocks.
    ///
    /// After we find the most work chain, we start downloading blocks and connecting them to our
    /// chain. This step usually takes the longest time.
    DownloadingBlocks,

    /// We've finished IBD and are now listening for new blocks as they are found.
    Done,
}

/// This trait is the main interface between our blockchain backend and other services.
/// It'll be useful for transitioning from rpc to a p2p based node
pub trait BlockchainInterface {
    type Error: Error + Send + Sync + 'static;

    /// Returns the block with a given height in our current tip.
    fn get_block_hash(&self, height: u32) -> Result<bitcoin::BlockHash, Self::Error>;

    /// Returns a bitcoin [`bitcoin::Transaction`] given its txid.
    fn get_tx(&self, txid: &bitcoin::Txid) -> Result<Option<bitcoin::Transaction>, Self::Error>;

    /// Get the height of our best know chain.
    fn get_height(&self) -> Result<u32, Self::Error>;

    /// Returns fee estimation for inclusion in `target` blocks.
    fn estimate_fee(&self, target: usize) -> Result<f64, Self::Error>;

    /// Returns a block with a given `hash` if any.
    fn get_block(&self, hash: &BlockHash) -> Result<Block, Self::Error>;

    /// Returns the best known block
    fn get_best_block(&self) -> Result<(u32, BlockHash), Self::Error>;

    /// Returns associated header for block with `hash`
    fn get_block_header(&self, hash: &BlockHash) -> Result<BlockHeader, Self::Error>;

    /// Register for receiving notifications for some event. Right now it only works for
    /// new blocks, but may work with transactions in the future too.
    /// if a module performs some heavy-lifting on the block's data, it should pass in a
    /// vector or a channel where data can be transferred to the actual worker, otherwise
    /// chainstate will be stuck for as long as you have work to do.
    fn subscribe(&self, tx: Arc<dyn BlockConsumer>);

    /// Tells whether or not we are on IBD
    fn is_in_ibd(&self) -> bool;

    /// Checks if a coinbase is mature
    fn is_coinbase_mature(&self, height: u32, block: BlockHash) -> Result<bool, Self::Error>;

    /// Returns a block locator
    fn get_block_locator(&self) -> Result<Vec<BlockHash>, Self::Error>;

    /// Returns a block locator from a given tip
    ///
    /// This method may be used to get the locator from a tip that's not the best one
    fn get_block_locator_for_tip(&self, tip: BlockHash) -> Result<Vec<BlockHash>, BlockchainError>;

    /// Returns the last block we validated
    fn get_validation_index(&self) -> Result<u32, Self::Error>;

    /// Returns the height of a block, given it's hash
    fn get_block_height(&self, hash: &BlockHash) -> Result<Option<u32>, Self::Error>;

    /// Applies a changeset to an accumulator and returns the resulting one
    fn update_acc(
        &self,
        acc: Stump,
        block: &Block,
        height: u32,
        proof: Proof,
        del_hashes: Vec<sha256::Hash>,
    ) -> Result<Stump, Self::Error>;

    /// Returns all known chain tips, including the best one and forks
    fn get_chain_tips(&self) -> Result<Vec<BlockHash>, Self::Error>;

    /// Validates a block according to Bitcoin's rules, without modifying our chain
    fn validate_block(
        &self,
        block: &Block,
        proof: Proof,
        inputs: HashMap<OutPoint, UtxoData>,
        del_hashes: Vec<sha256::Hash>,
        acc: Stump,
    ) -> Result<(), Self::Error>;

    /// Find the last common ancestor between the current best chain and `block`
    fn get_fork_point(&self, block: BlockHash) -> Result<BlockHash, Self::Error>;

    /// Returns this chain's params
    fn get_params(&self) -> bitcoin::params::Params;

    /// Returns our current acc
    fn acc(&self) -> Stump;

    /// Returns the amount of [`Work`] associated with a given chain tip
    fn get_work(&self, tip: BlockHash) -> Result<Work, Self::Error>;

    /// Returns the total size on disk, in bytes, of the chain data persisted by this backend.
    fn size_on_disk(&self) -> Result<u64, Self::Error>;

    /// Returns the current state of our chain.
    fn ibd_state(&self) -> IBDState;

    /// Returns accumulated chain-store health warnings (e.g. index full).
    ///
    /// Warnings persist for the process lifetime. Mirrors the `warnings` field returned by
    /// Bitcoin Core's `getblockchaininfo`.
    fn get_warnings(&self) -> Vec<ChainStoreWarning> {
        vec![]
    }
}

/// [UpdatableChainstate] is a contract that a is expected from a chainstate
/// implementation, that wishes to be updated. Using those methods, a backend like the p2p-node,
/// can notify new blocks and transactions to a chainstate, allowing it to update it's state.
pub trait UpdatableChainstate {
    /// This is one of the most important methods for a ChainState,
    /// it gets a block and some utreexo data, validates this block and
    /// connects to our chain of blocks. This function is meant to be atomic
    /// and prone of running in parallel.
    fn connect_block(
        &self,
        block: &Block,
        proof: Proof,
        inputs: HashMap<OutPoint, UtxoData>,
        del_hashes: Vec<sha256::Hash>,
    ) -> Result<u32, BlockchainError>;

    fn switch_chain(&self, new_tip: BlockHash) -> Result<(), BlockchainError>;
    /// Accepts a new header to our chain. This method is called before connect_block, and
    /// makes some basic checks on a header and saves it on disk. We only accept a block as
    /// valid after calling connect_block.
    ///
    /// This function returns whether this block is on our best-known chain, or in a fork
    fn accept_header(&self, header: BlockHeader) -> Result<(), BlockchainError>;
    /// Not used for now, but in a future blockchain with mempool, we can process transactions
    /// that are not in a block yet.
    fn handle_transaction(&self) -> Result<(), BlockchainError>;
    /// Persists our data. Should be invoked periodically.
    fn flush(&self) -> Result<(), BlockchainError>;
    /// Update IBD state
    fn update_ibd(&self, ibd_state: IBDState);
    /// Tells this blockchain to consider this block invalid, and not build on top of it
    fn invalidate_block(&self, block: BlockHash) -> Result<(), BlockchainError>;
    /// Marks one block as being fully validated, this overrides a block that was explicitly
    /// marked as invalid.
    fn mark_block_as_valid(&self, block: BlockHash) -> Result<(), BlockchainError>;
    /// Returns the root hashes of our utreexo forest
    fn get_root_hashes(&self) -> Vec<BitcoinNodeHash>;
    /// Returns a partial chainstate from a range of blocks.
    ///
    /// [PartialChainState] is a simplified version of `ChainState` that is used during IBD.
    /// It doesn't support reorgs, only hold headers for a subset of blocks and isn't [Sync].
    /// The idea here is that you take a OS thread or some async task that will drive one
    /// [PartialChainState] to completion by downloading blocks inside that chainstate's range.
    /// If all goes right, it'll end without error, and you should mark blocks in this range as
    /// valid.
    /// Since this chainstate may start from a height with an existing UTXO set, you need to
    /// provide a [Stump] for that block.
    fn get_partial_chain(
        &self,
        initial_height: u32,
        final_height: u32,
        acc: Stump,
    ) -> Result<PartialChainState, BlockchainError>;
    /// Marks a chain as fully-valid
    ///
    /// This mimics the behaviour of checking every block before this block, and continues
    /// from this point
    fn mark_chain_as_assumed(&self, acc: Stump, tip: BlockHash) -> Result<bool, BlockchainError>;
    /// Returns the current accumulator
    fn get_acc(&self) -> Stump;
}

#[derive(Debug, Clone)]
/// A notification is a hook that a type implementing [BlockchainInterface] sends each
/// time the given event happens. This is use to notify new blocks to the Electrum server.
/// In the future, it can be expanded to send more data, like transactions.
pub enum Notification {
    NewBlock((Block, u32)),
}

impl<T: UpdatableChainstate> UpdatableChainstate for Arc<T> {
    fn flush(&self) -> Result<(), BlockchainError> {
        T::flush(self)
    }

    fn get_acc(&self) -> Stump {
        T::get_acc(self)
    }

    fn update_ibd(&self, ibd_state: IBDState) {
        T::update_ibd(self, ibd_state)
    }

    fn connect_block(
        &self,
        block: &Block,
        proof: Proof,
        inputs: HashMap<OutPoint, UtxoData>,
        del_hashes: Vec<sha256::Hash>,
    ) -> Result<u32, BlockchainError> {
        T::connect_block(self, block, proof, inputs, del_hashes)
    }

    fn accept_header(&self, header: BlockHeader) -> Result<(), BlockchainError> {
        T::accept_header(self, header)
    }

    fn get_root_hashes(&self) -> Vec<BitcoinNodeHash> {
        T::get_root_hashes(self)
    }

    fn invalidate_block(&self, block: BlockHash) -> Result<(), BlockchainError> {
        T::invalidate_block(self, block)
    }

    fn get_partial_chain(
        &self,
        initial_height: u32,
        final_height: u32,
        acc: Stump,
    ) -> Result<PartialChainState, BlockchainError> {
        T::get_partial_chain(self, initial_height, final_height, acc)
    }

    fn handle_transaction(&self) -> Result<(), BlockchainError> {
        T::handle_transaction(self)
    }

    fn switch_chain(&self, new_tip: BlockHash) -> Result<(), BlockchainError> {
        T::switch_chain(self, new_tip)
    }

    fn mark_block_as_valid(&self, block: BlockHash) -> Result<(), BlockchainError> {
        T::mark_block_as_valid(self, block)
    }

    fn mark_chain_as_assumed(&self, acc: Stump, tip: BlockHash) -> Result<bool, BlockchainError> {
        T::mark_chain_as_assumed(self, acc, tip)
    }
}

impl<T: BlockchainInterface> BlockchainInterface for Arc<T> {
    type Error = <T as BlockchainInterface>::Error;

    fn get_work(&self, tip: BlockHash) -> Result<Work, Self::Error> {
        T::get_work(self, tip)
    }

    fn get_tx(&self, txid: &bitcoin::Txid) -> Result<Option<bitcoin::Transaction>, Self::Error> {
        T::get_tx(self, txid)
    }

    fn get_params(&self) -> bitcoin::params::Params {
        T::get_params(self)
    }

    fn acc(&self) -> Stump {
        T::acc(self)
    }

    fn get_block(&self, hash: &BlockHash) -> Result<Block, Self::Error> {
        T::get_block(self, hash)
    }

    fn subscribe(&self, tx: Arc<dyn BlockConsumer>) {
        T::subscribe(self, tx)
    }

    fn is_in_ibd(&self) -> bool {
        T::is_in_ibd(self)
    }

    fn get_height(&self) -> Result<u32, Self::Error> {
        T::get_height(self)
    }

    fn estimate_fee(&self, target: usize) -> Result<f64, Self::Error> {
        T::estimate_fee(self, target)
    }

    fn get_block_hash(&self, height: u32) -> Result<BlockHash, Self::Error> {
        T::get_block_hash(self, height)
    }

    fn get_best_block(&self) -> Result<(u32, BlockHash), Self::Error> {
        T::get_best_block(self)
    }

    fn get_block_header(&self, hash: &BlockHash) -> Result<BlockHeader, Self::Error> {
        T::get_block_header(self, hash)
    }

    fn get_block_height(&self, hash: &BlockHash) -> Result<Option<u32>, Self::Error> {
        T::get_block_height(self, hash)
    }

    fn get_block_locator(&self) -> Result<Vec<BlockHash>, Self::Error> {
        T::get_block_locator(self)
    }

    fn is_coinbase_mature(&self, height: u32, block: BlockHash) -> Result<bool, Self::Error> {
        T::is_coinbase_mature(self, height, block)
    }

    fn get_validation_index(&self) -> Result<u32, Self::Error> {
        T::get_validation_index(self)
    }

    fn get_block_locator_for_tip(&self, tip: BlockHash) -> Result<Vec<BlockHash>, BlockchainError> {
        T::get_block_locator_for_tip(self, tip)
    }

    fn update_acc(
        &self,
        acc: Stump,
        block: &Block,
        height: u32,
        proof: Proof,
        del_hashes: Vec<sha256::Hash>,
    ) -> Result<Stump, Self::Error> {
        T::update_acc(self, acc, block, height, proof, del_hashes)
    }

    fn get_chain_tips(&self) -> Result<Vec<BlockHash>, Self::Error> {
        T::get_chain_tips(self)
    }

    fn validate_block(
        &self,
        block: &Block,
        proof: Proof,
        inputs: HashMap<OutPoint, UtxoData>,
        del_hashes: Vec<sha256::Hash>,
        acc: Stump,
    ) -> Result<(), Self::Error> {
        T::validate_block(self, block, proof, inputs, del_hashes, acc)
    }

    fn get_fork_point(&self, block: BlockHash) -> Result<BlockHash, Self::Error> {
        T::get_fork_point(self, block)
    }

    fn size_on_disk(&self) -> Result<u64, Self::Error> {
        T::size_on_disk(self)
    }

    fn ibd_state(&self) -> IBDState {
        T::ibd_state(self)
    }

    fn get_warnings(&self) -> Vec<ChainStoreWarning> {
        T::get_warnings(self)
    }
}

/// This module defines an [UtxoData] struct, helpful for transaction validation
pub mod utxo_data {
    use bitcoin::TxOut;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(
        any(test, feature = "test-utils"),
        derive(serde::Serialize, serde::Deserialize)
    )]
    /// Represents an unspent transaction output (UTXO) with additional metadata for validation.
    pub struct UtxoData {
        /// The unspent transaction output.
        pub txout: TxOut,
        /// Whether this output was created by a coinbase transaction.
        pub is_coinbase: bool,
        /// The block height at which the UTXO was confirmed.
        pub creation_height: u32,
        /// The creation time of the UTXO, defined by BIP 68 as the median time past (MTP) of the
        /// block preceding the confirming block.
        pub creation_time: u32,
    }
}

/// [`ChainBackend`] is a trait alias for the [`BlockchainInterface`] and [`UpdatableChainstate`] combo meant to be used
/// to specify a generic blockchain backend.
///
/// Useful to avoid trait bounds verbosity.
pub trait ChainBackend: BlockchainInterface + UpdatableChainstate {}

impl<T: BlockchainInterface + UpdatableChainstate> ChainBackend for T {}

/// [`ThreadSafeChain`] is a trait alias for the [`BlockchainInterface`], [`UpdatableChainstate`], [`Sync`] and [`Send`] combo
/// and has a static lifetime. It is meant to be used to specify thread-safe blockchain backends.
///
/// Useful to avoid code verbosity.
pub trait ThreadSafeChain: ChainBackend + Sync + Send + 'static {}

impl<T: ChainBackend + Sync + Send + 'static> ThreadSafeChain for T {}

#[cfg(all(test, feature = "flat-chainstore"))]
mod validation_pipeline_test {
    use bitcoin::Amount;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::CompactTarget;
    use bitcoin::Network;
    use bitcoin::ScriptBuf;
    use bitcoin::Sequence;
    use bitcoin::Transaction;
    use bitcoin::TxMerkleNode;
    use bitcoin::TxOut;
    use bitcoin::Witness;
    use bitcoin::absolute::LockTime;
    use bitcoin::block::Header as BlockHeader;
    use bitcoin::block::Version as HeaderVersion;
    use bitcoin::constants::genesis_block;
    use bitcoin::hashes::Hash;
    use bitcoin::opcodes::OP_TRUE;
    use bitcoin::opcodes::all::OP_CHECKSIG;
    use bitcoin::transaction::Version as TransactionVersion;
    use floresta_common::assert_ok;

    use super::*;
    use crate::AssumeValidArg;
    use crate::ChainState;
    use crate::FlatChainStore;
    use crate::extensions::HeaderExt;
    use crate::pruned_utreexo::chain_state::test::EASIEST_REGTEST_TARGET_BITS;
    use crate::pruned_utreexo::chain_state::test::block_with_transactions;
    use crate::pruned_utreexo::chain_state::test::mine;
    use crate::pruned_utreexo::chain_state::test::setup_test_chain;
    use crate::pruned_utreexo::chain_state::test::test_coinbase;
    use crate::pruned_utreexo::chain_state::test::test_outpoint;
    use crate::pruned_utreexo::chainparams::ChainParams;
    use crate::pruned_utreexo::consensus::Consensus;
    use crate::pruned_utreexo::consensus::UNSPENDABLE_BIP30_UTXO_91722;
    use crate::pruned_utreexo::error::BlockValidationErrors;
    use crate::pruned_utreexo::partial_chain::PartialChainStateInner;
    use crate::txin;
    use crate::txout;

    /// A target that a header we haven't mined won't meet: it asks for roughly 2^47 times the
    /// work of the regtest limit, which is the easiest target there is.
    ///
    /// It is *harder* than the target we require at this height, so the difficulty comparison
    /// lets it through and the header fails on its own hash against its own claimed target,
    /// which is what Bitcoin Core reports as `high-hash`.
    const UNMEETABLE_TARGET_BITS: u32 = 0x1b00_ffff;

    /// A timestamp no clock will accept: the largest one a header can carry.
    ///
    /// A header may not be stamped more than two hours ahead of the clock of whoever is
    /// validating it, and the largest timestamp there is sits decades past that, whatever that
    /// clock says. Which also keeps the walk from depending on the clock of the machine
    /// running it.
    const FAR_FUTURE_TIME: u32 = u32::MAX;

    /// A coinbase value above the 50 BTC the regtest chain pays at the height we walk.
    const EXCESSIVE_COINBASE_VALUE: u64 = 51 * 100_000_000;

    /// More coins than there will ever be, one satoshi over the 21 million limit.
    const MORE_THAN_EVERY_COIN: u64 = 21_000_000 * 100_000_000 + 1;

    /// A script long enough to take a block past the 4,000,000 weight units it's allowed. Bytes
    /// outside a witness weigh four units each, so a megabyte of script is more than enough.
    const OVERSIZED_SCRIPT_LEN: usize = 1_000_001;

    /// An absolute lock time no block of the height we walk can satisfy.
    const LOCK_TIME_IN_THE_FUTURE: u32 = 1_000;

    /// A sequence claiming a relative lock time of ten blocks, which also makes the input it
    /// sits in non-final for an absolute lock time.
    const RELATIVE_LOCK_SEQUENCE: u32 = 10;

    /// The leaf our bogus proof claims to prove the deletion of, and the position it claims
    /// for it. Any preimage and any position would do: what's hashed here was never in the
    /// accumulator, which is the whole point.
    const UNPROVEN_LEAF: &[u8] = b"not in the accumulator";
    const UNPROVEN_LEAF_POSITION: u64 = 0;

    /// The number of sigops that takes a block over its budget. Legacy sigops count four times
    /// towards the limit of 80,000, so 20,000 of them is exactly the budget, and one more is
    /// over it.
    const SIGOPS_OVER_THE_BLOCK_LIMIT: usize = 20_001;

    /// The size of a transaction that can be read as two concatenated merkle hashes instead,
    /// which is what makes a block carrying one ambiguous ([CVE-2017-12842]).
    ///
    /// [CVE-2017-12842]: https://www.cve.org/CVERecord?id=CVE-2017-12842
    const MERKLE_NODE_TX_SIZE: usize = 64;

    /// The six bytes that mark a coinbase output as the SegWit commitment: `OP_RETURN`,
    /// `OP_PUSHBYTES_36` and the `aa21a9ed` tag defined by [BIP141]. The 32 committed bytes
    /// follow them.
    ///
    /// [BIP141]: https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki#commitment-structure
    const WITNESS_COMMITMENT_HEADER: [u8; 6] = [0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];

    /// What the walk below needs from a chain backend, on top of [`UpdatableChainstate`].
    ///
    /// Deliberately as small as it can be: anything a backend gets to answer for itself is a
    /// divergence the walk stops covering. So this only says how to build the backend, whether
    /// it validates headers at all, and how to hand it a block — never which error a check
    /// reports, which is what the walk is there to pin down.
    trait PipelineHarness: UpdatableChainstate + BlockchainInterface<Error = BlockchainError> {
        /// A backend that knows the headers of `blocks` and hasn't validated any of them.
        fn setup(blocks: &[Block]) -> Self;

        /// Whether this backend validates headers. A [`PartialChainState`] is handed a range
        /// of headers that the full chainstate has already accepted, so it skips step 1.
        fn validates_headers() -> bool;

        /// Feeds `block` to the backend and returns what the pipeline reports.
        fn feed(
            &self,
            block: &Block,
            proof: Proof,
            inputs: HashMap<OutPoint, UtxoData>,
            del_hashes: Vec<sha256::Hash>,
        ) -> Result<u32, BlockchainError>;
    }

    impl PipelineHarness for ChainState<FlatChainStore> {
        fn setup(blocks: &[Block]) -> Self {
            let chain = setup_test_chain(Network::Regtest, AssumeValidArg::Disabled, None);

            for block in blocks {
                chain.accept_header(block.header).unwrap();
            }

            chain
        }

        fn validates_headers() -> bool {
            true
        }

        /// Puts this block's header in the place of whatever we have at its height, and
        /// connects the block.
        ///
        /// Replacing the header is what lets one walk carry a single block through the whole
        /// pipeline: repairing the payload changes the merkle root, and with it the header
        /// that commits to it.
        fn feed(
            &self,
            block: &Block,
            proof: Proof,
            inputs: HashMap<OutPoint, UtxoData>,
            del_hashes: Vec<sha256::Hash>,
        ) -> Result<u32, BlockchainError> {
            let parent_height = self
                .get_block_height(&block.header.prev_blockhash)
                .unwrap()
                .expect("the parent is in our chain");

            // Drop the variant we fed last time, so this one can extend the tip in its place
            if let Ok(hash) = self.get_block_hash(parent_height + 1) {
                let known = self.get_block_height(&hash);
                let is_valid_tip = matches!(known, Ok(Some(_)));

                if hash != block.block_hash() && is_valid_tip {
                    self.invalidate_block(hash).unwrap();
                }
            }

            self.accept_header(block.header)?;
            self.connect_block(block, proof, inputs, del_hashes)
        }
    }

    impl PipelineHarness for PartialChainState {
        fn setup(blocks: &[Block]) -> Self {
            // The headers at heights 0 to 11: the genesis, and the eleven blocks that the
            // block we walk through the pipeline builds on
            let headers = core::iter::once(genesis_block(Network::Regtest).header)
                .chain(blocks.iter().take(11).map(|block| block.header))
                .collect();

            PartialChainStateInner {
                assume_valid: true,
                consensus: Consensus {
                    parameters: ChainParams::from(Network::Regtest),
                },
                current_height: 0,
                current_acc: Stump::default(),
                final_height: 12,
                blocks: headers,
                error: None,
            }
            .into()
        }

        fn validates_headers() -> bool {
            false
        }

        fn feed(
            &self,
            block: &Block,
            proof: Proof,
            inputs: HashMap<OutPoint, UtxoData>,
            del_hashes: Vec<sha256::Hash>,
        ) -> Result<u32, BlockchainError> {
            // A partial chain holds no headers of its own to replace
            self.connect_block(block, proof, inputs, del_hashes)
        }
    }

    #[test]
    fn chain_state_validation_pipeline() {
        walk_the_validation_pipeline::<ChainState<FlatChainStore>>();
    }

    #[test]
    fn partial_chain_state_validation_pipeline() {
        walk_the_validation_pipeline::<PartialChainState>();
    }

    /// Acceptance test for the whole validation pipeline: which check answers for a block, in
    /// which order, and with which error.
    ///
    /// One chain, and one block that starts out broken at every level we know how to check.
    /// Each step feeds it to the backend, asserts the error Bitcoin Core reports for the check
    /// that has to come first, repairs *only* that one thing and feeds it back, so that the
    /// next step sees the next check fire. A step can fail on two counts, and the walk covers
    /// both:
    ///
    /// - **ordering**: a check that runs out of turn answers for a step that isn't its own,
    ///   and a check that doesn't run at all lets a later one answer in its place
    /// - **parity**: the check runs where it should, but reports something other than what
    ///   Bitcoin Core reports for it, so whoever reads the error can't tell which check failed
    ///
    /// ```text
    /// 1. accept_header  a header we already have ──────────────── DuplicateBlock
    ///                   ↓ mark block 12 invalid
    ///                   a header we know to be invalid ────────── DuplicateInvalidBlock
    ///                   ↓ repair: nothing (the same header)
    ///                   pow against its own claimed target ────── NotEnoughPow
    ///                   ↓ repair: claim the regtest target, and meet it
    ///                   unknown parent ────────────────────────── PrevBlockNotFound
    ///                   ↓ repair: build on block 12 (invalid)
    ///                   invalid parent ────────────────────────── BadPrevBlock
    ///                   ↓ repair: build on block 6
    ///                   timestamp at the MTP ──────────────────── TimeTooOld
    ///                   ↓ repair: a timestamp above it
    ///                   timestamp in the future ───────────────── TimeTooNew
    ///                   ↓ repair: a sane timestamp
    ///                   header accepted ───────────────────────── ok
    /// 2. connect_block  not the next block to validate ────────── BlockDoesntExtendTip
    ///                   ↓ repair: connect blocks 1 to 11
    /// 3. block          no transactions at all ────────────────── EmptyBlock
    ///                   ↓ repair: the transactions
    ///                   over the weight limit ─────────────────── BlockTooBig
    ///                   ↓ repair: drop the padding
    /// 4. mutation       payload ≠ merkle root ─────────────────── BadMerkleRoot
    ///                   ↓ repair: the committed payload, last transaction duplicated
    ///                   [a,b,c] against [a,b,c,c] ─────────────── DuplicateTransactions
    ///                   ↓ repair: drop the duplicate
    ///                   64-byte transaction, no coinbase ──────── SixtyFourByteTransaction
    ///                   ↓ repair: make it 65 bytes long
    ///                   no coinbase ───────────────────────────── FirstTxIsNotCoinbase
    ///                   ↓ repair: a coinbase, with a commitment and a 1-byte nonce
    ///                   reserved value isn't 32 bytes ─────────── BadWitnessNonceSize
    ///                   ↓ repair: a 32-byte reserved value
    ///                   the commitment doesn't match ──────────── BadWitnessCommitment
    ///                   ↓ repair: drop the commitment
    ///                   witness data with no commitment ───────── UnexpectedWitness
    /// 5. transactions   ↓ repair: drop the witness data
    ///                   a second coinbase ─────────────────────── MultipleCoinbase
    ///                   ↓ repair: drop the second coinbase
    ///                   a coinbase scriptsig too short ────────── InvalidCoinbase
    ///                   ↓ repair: a scriptsig of the size one has to be
    ///                   no inputs ─────────────────────────────── EmptyInputs
    ///                   ↓ repair: two inputs, the same one twice
    ///                   no outputs ────────────────────────────── EmptyOutputs
    ///                   ↓ repair: an output
    ///                   more coins than there are ─────────────── TooManyCoins
    ///                   ↓ repair: a value that exists
    ///                   the same input twice ──────────────────── DuplicateInput
    ///                   ↓ repair: a second input of its own
    ///                   a null prevout ────────────────────────── NullPrevOut
    ///                   ↓ repair: an output for it to spend
    ///                   sigops over the block limit ───────────── TooManySigOps
    ///                   ↓ repair: drop the sigop script
    ///                   lock time in the future ───────────────── NonFinalTransaction
    ///                   ↓ repair: no lock time
    /// 6. proof          a proof with no deletion hash ─────────── MalformedUtreexoProof
    ///                   ↓ repair: the hash of the deleted leaf
    ///                   a leaf we never had ───────────────────── InvalidUtreexoProof
    ///                   ↓ repair: the hash of a leaf we do hold
    ///                   a leaf the BIP30 violation overwrote ──── UnspendableUTXO
    ///                   ↓ repair: a proof of nothing, which verifies
    /// 7. the UTXOs      the spent UTXOs are missing ───────────── UtxoNotFound(the first)
    ///                   ↓ repair: hand the UTXOs over
    ///                   an immature coinbase ──────────────────── CoinbaseNotMatured
    ///                   ↓ repair: not a coinbase
    ///                   an unspendable script ─────────────────── ScriptError
    ///                   ↓ repair: a script anyone can spend
    ///                   spending more than it holds ───────────── NotEnoughMoney
    ///                   ↓ repair: enough value
    ///                   a relative lock time ──────────────────── UnsatisfiedSequenceLocks
    ///                   ↓ repair: final sequences
    ///                   the coinbase claims too much ──────────── BadCoinbaseOutValue
    ///                   ↓ repair: what the block pays it
    ///                   BLOCK CONNECTED ───────────────────────── height 12
    /// ```
    ///
    /// A backend that doesn't validate headers skips step 1, which is what [`PipelineHarness`]
    /// describes. Nothing else about the walk is per-backend: it's one pipeline, so the errors
    /// have to be the same too.
    fn walk_the_validation_pipeline<T: PipelineHarness>() {
        let mut gaps = Vec::new();

        // Twelve blocks to work against. Block 12 is about to become our invalid block, and
        // block 11 the parent of everything we feed below.
        let blocks = mine_chain(12);
        let chain = T::setup(&blocks);
        let parent = &blocks[10];

        // The block we walk through the pipeline, broken at every level we know how to check.
        // Its payload is, in this order: a transaction of exactly 64 bytes where the coinbase
        // should be, a second coinbase padding the block past the weight limit, and the
        // transaction the block spends with, which starts out with no inputs and no outputs.
        let mut coinbase = test_coinbase(12, Sequence::MAX, LockTime::ZERO);
        coinbase.input[0].script_sig = ScriptBuf::from_bytes(vec![OP_TRUE.to_u8()]);
        coinbase.output = vec![txout!(EXCESSIVE_COINBASE_VALUE, ScriptBuf::new())];
        coinbase.output.push(txout!(0, sigop_script()));
        coinbase.output.push(witness_commitment_output([0u8; 32]));
        coinbase.input[0].witness = Witness::from_slice(&[[0u8; 1]]);

        let mut second_coinbase = test_coinbase(999, Sequence::MAX, LockTime::ZERO);
        second_coinbase
            .output
            .push(txout!(0, anyone_can_spend_bytes(OVERSIZED_SCRIPT_LEN)));

        let spend = Transaction {
            version: TransactionVersion::TWO,
            lock_time: LockTime::from_consensus(LOCK_TIME_IN_THE_FUTURE),
            input: Vec::new(),
            output: Vec::new(),
        };
        let mut txdata = vec![sixty_four_byte_transaction(), second_coinbase, spend];
        let mut block = rebuild(
            &txdata,
            BlockHeader {
                version: HeaderVersion::TWO,
                prev_blockhash: parent.block_hash(),
                merkle_root: TxMerkleNode::all_zeros(),
                time: parent.header.time + 1,
                bits: CompactTarget::from_consensus(EASIEST_REGTEST_TARGET_BITS),
                nonce: 0,
            },
        );

        // ---- 1. Header validation ----
        if T::validates_headers() {
            // A header we already have short-circuits every check below it, and we say so
            // rather than quietly reporting success: it's the caller who knows what a header
            // it has already given us means
            gaps.extend(compare(
                "known header",
                BlockValidationErrors::DuplicateBlock,
                chain.accept_header(blocks[11].header),
            ));

            // One we've marked invalid is rejected outright, still without being revalidated.
            // This also leaves block 11 as our tip.
            chain.invalidate_block(blocks[11].block_hash()).unwrap();
            gaps.extend(compare(
                "known invalid header",
                BlockValidationErrors::DuplicateInvalidBlock,
                chain.accept_header(blocks[11].header),
            ));

            // Break the header: it claims a target it doesn't meet, builds on a parent we
            // don't have, and is stamped at the Median Time Past of the chain we do have. The
            // steps below build on block 6, so a header we wrongly accept lands on a fork and
            // leaves our tip alone.
            let fork_parent = &blocks[5];
            block.header.prev_blockhash = BlockHash::all_zeros();
            block.header.bits = CompactTarget::from_consensus(UNMEETABLE_TARGET_BITS);
            block.header.time = median_time_past(&blocks[..6]);

            // The proof of work is checked against the target the header itself claims, so it
            // needs no context at all and is reported before we even look for the parent
            gaps.extend(compare(
                "header proof of work",
                BlockValidationErrors::NotEnoughPow,
                chain.accept_header(block.header),
            ));

            // Repair: claim the regtest target, and meet it
            block.header.bits = CompactTarget::from_consensus(EASIEST_REGTEST_TARGET_BITS);
            block = mine(block);

            // Now the parent lookup, which is reported before any check against that parent
            gaps.extend(compare(
                "missing parent",
                BlockValidationErrors::PrevBlockNotFound,
                chain.accept_header(block.header),
            ));

            // Repair: build on a parent we do have. It's the one we've just marked invalid.
            block.header.prev_blockhash = blocks[11].block_hash();
            block = mine(block);

            // Knowing the parent and knowing it to be invalid is a different answer from not
            // having it at all, and it too comes before the checks against the parent
            gaps.extend(compare(
                "invalid parent",
                BlockValidationErrors::BadPrevBlock,
                chain.accept_header(block.header),
            ));

            // Repair: build on a valid parent
            block.header.prev_blockhash = fork_parent.block_hash();
            block = mine(block);

            // Contextual: the timestamp has to be above the Median Time Past of the ancestors
            gaps.extend(compare(
                "timestamp at the median time past",
                BlockValidationErrors::TimeTooOld,
                chain.accept_header(block.header),
            ));

            // Repair: a timestamp above the Median Time Past ... and hours ahead of our clock
            block.header.time = FAR_FUTURE_TIME;
            block = mine(block);

            // ... which can't be more than two hours
            gaps.extend(compare(
                "timestamp in the future",
                BlockValidationErrors::TimeTooNew,
                chain.accept_header(block.header),
            ));

            // Repair: a sane timestamp. There's nothing left to say about this header, and
            // it's accepted, as a fork of the chain we're building on.
            block.header.time = fork_parent.header.time + 1;
            block = mine(block);
            assert_ok!(chain.accept_header(block.header));

            // Move the walk back to the tip. A fork is headers alone as far as we're
            // concerned, so a block building on one can't be connected however valid it is,
            // which is why the header steps above could afford to live on one and these can't.
            block.header.prev_blockhash = parent.block_hash();
            block.header.time = parent.header.time + 1;
            block = mine(block);
        }

        // ---- 2. The tip check ----
        // The emptiest payload there is, which is also where the block stage starts below. A
        // header that gets replaced can't come back, so this step uses the one it goes on to
        // use next.
        let empty = rebuild(&[], block.header);

        // A known divergence: Core takes any block whose parent it has, on whatever branch. We
        // keep no blocks — only the transaction data the main chain needs, and only while we
        // validate it — so a fork is worth headers to us and nothing more. See
        // [`BlockValidationErrors::BlockDoesntExtendTip`].
        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "tip check",
            BlockValidationErrors::BlockDoesntExtendTip,
            chain.feed(&empty, proof, HashMap::new(), del_hashes),
        ));

        // Repair: connect everything below this block, so that it's the next one to validate
        for connected in blocks.iter().take(11) {
            chain
                .connect_block(connected, Proof::default(), HashMap::new(), Vec::new())
                .unwrap();
        }

        // ---- 3. The block ----
        // Nothing at all: no transactions, which the header commits to with the zero merkle
        // root Bitcoin Core defines for an empty list. There isn't even a coinbase to look at.
        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "empty block",
            BlockValidationErrors::EmptyBlock,
            chain.feed(&empty, proof, HashMap::new(), del_hashes),
        ));

        // Repair: the transactions this block carries. The second coinbase pads it past the
        // four million weight units a block is allowed.
        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "block weight",
            BlockValidationErrors::BlockTooBig,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: drop the padding
        txdata[1].output.pop();
        block = rebuild(&txdata, block.header);

        // ---- 4. Mutation detection ----
        // The header commits to a payload, and this isn't it. Everything the payload is broken
        // in below is still there, and none of it is reported instead.
        let mut mutated = block.clone();
        mutated.txdata = vec![test_coinbase(998, Sequence::MAX, LockTime::ZERO)];
        assert_eq!(mutated.block_hash(), block.block_hash());

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "merkle root",
            BlockValidationErrors::BadMerkleRoot,
            chain.feed(&mutated, proof, HashMap::new(), del_hashes),
        ));

        // Repair: the payload the header commits to, with the last transaction duplicated.
        // `[a, b, c]` and `[a, b, c, c]` have the same merkle root, so the header commits to
        // this payload just as much.
        let mut duplicated = block.clone();
        duplicated.txdata.push(txdata[2].clone());
        assert_eq!(duplicated.block_hash(), block.block_hash());

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "duplicate transactions",
            BlockValidationErrors::DuplicateTransactions,
            chain.feed(&duplicated, proof, HashMap::new(), del_hashes),
        ));

        // Repair: drop the duplicate. What's left starts with a 64-byte transaction, which can
        // be read as a pair of merkle hashes instead.
        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "64-byte transaction",
            BlockValidationErrors::SixtyFourByteTransaction,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: one more byte of script, so the transaction can't be mistaken for a pair of
        // hashes anymore. It still isn't a coinbase.
        let padding = txdata[0].output[0].script_pubkey.len() + 1;
        txdata[0].output[0].script_pubkey = anyone_can_spend_bytes(padding);
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "missing coinbase",
            BlockValidationErrors::FirstTxIsNotCoinbase,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: a coinbase, which is what commits to the witness data of the whole block. It
        // claims a commitment that doesn't match, and its reserved value isn't 32 bytes long.
        txdata[0] = coinbase;
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "witness reserved value",
            BlockValidationErrors::BadWitnessNonceSize,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: a 32-byte reserved value. Witness data doesn't change any txid, so the header
        // still commits to this payload, and now the commitment itself is what's wrong.
        block.txdata[0].input[0].witness = Witness::from_slice(&[[0u8; 32]]);
        txdata[0] = block.txdata[0].clone();

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "witness commitment",
            BlockValidationErrors::BadWitnessCommitment,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: drop the commitment. Now nothing commits to the witness data that's still in
        // there, which is room for anything.
        txdata[0].output.pop();
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "unexpected witness",
            BlockValidationErrors::UnexpectedWitness,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // ---- 5. The transactions, on their own ----
        // Repair: drop the witness data. The payload is committed to and unmutated now, so
        // what's wrong with it is the block's own business: it has a second coinbase.
        block.txdata[0].input[0].witness = Witness::default();
        txdata[0] = block.txdata[0].clone();

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "second coinbase",
            BlockValidationErrors::MultipleCoinbase,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: drop the second coinbase. The one that's left carries a scriptsig too short
        // to be a coinbase's.
        txdata.remove(1);
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "coinbase scriptsig",
            BlockValidationErrors::InvalidCoinbase("Invalid ScriptSig size".into()),
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: a scriptsig of the size a coinbase's has to be. From here on it's the
        // transaction the block spends with, which has nothing in it at all.
        txdata[0].input[0].script_sig = test_coinbase(12, Sequence::MAX, LockTime::ZERO).input[0]
            .script_sig
            .clone();
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "transaction without inputs",
            BlockValidationErrors::EmptyInputs,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: inputs, both of them spending the same output
        txdata[1].input = vec![
            txin!(
                test_outpoint(0),
                ScriptBuf::new(),
                Sequence::from_consensus(RELATIVE_LOCK_SEQUENCE)
            ),
            txin!(
                test_outpoint(0),
                ScriptBuf::new(),
                Sequence::from_consensus(RELATIVE_LOCK_SEQUENCE)
            ),
        ];
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "transaction without outputs",
            BlockValidationErrors::EmptyOutputs,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: an output, claiming more coins than there will ever be
        txdata[1].output = vec![txout!(MORE_THAN_EVERY_COIN, anyone_can_spend_bytes(1))];
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "more coins than there are",
            BlockValidationErrors::TooManyCoins,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: a value that exists. The two inputs still spend the same output twice.
        txdata[1].output[0].value = Amount::ONE_SAT;
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "the same input twice",
            BlockValidationErrors::DuplicateInput,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: a second input of its own, except it spends nothing at all, which only a
        // coinbase may do
        txdata[1].input[1].previous_output = OutPoint::null();
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "a null prevout",
            BlockValidationErrors::NullPrevOut,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: an output for it to spend. What's left is the coinbase, which spends the
        // block's whole sigop budget four times over.
        txdata[1].input[1].previous_output = test_outpoint(1);
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "block sigops",
            BlockValidationErrors::TooManySigOps,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: drop the sigop script. The spend still claims a lock time this block is too
        // early for.
        txdata[0].output.remove(1);
        block = rebuild(&txdata, block.header);

        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "lock time in the future",
            BlockValidationErrors::NonFinalTransaction,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: no lock time at all
        txdata[1].lock_time = LockTime::ZERO;
        block = rebuild(&txdata, block.header);

        // ---- 6. The utreexo proof ----
        // Nothing is left to say about this block without looking at what it spends, and the
        // proof is what ties those UTXOs to our accumulator, so it has to be verified before
        // anything looks at them. Each way a proof can fail is its own answer: whoever gets it
        // shouldn't have to guess which one it got.
        //
        // A proof that claims a deletion without carrying the hash of what it deletes. With
        // nothing to delete, rustreexo returns before it so much as looks at the proof, so the
        // targets it claims go unchecked unless we check them ourselves.
        let (proof, _) = bogus_proof();
        gaps.extend(compare(
            "proof with no deletion hash",
            BlockValidationErrors::MalformedUtreexoProof,
            chain.feed(&block, proof, HashMap::new(), Vec::new()),
        ));

        // Repair: the hash of the leaf it deletes. The proof is well formed now, and proves the
        // deletion of a leaf that was never in our accumulator.
        let (proof, del_hashes) = bogus_proof();
        gaps.extend(compare(
            "proof of a leaf we don't have",
            BlockValidationErrors::InvalidUtreexoProof,
            chain.feed(&block, proof, HashMap::new(), del_hashes),
        ));

        // Repair: the hash of a leaf we do hold. It's one of the two the BIP30 violation
        // overwrote, which a Utreexo node can still prove and still may not spend.
        let (proof, _) = bogus_proof();
        gaps.extend(compare(
            "spending a BIP30 leftover",
            BlockValidationErrors::UnspendableUTXO,
            chain.feed(
                &block,
                proof,
                HashMap::new(),
                vec![sha256::Hash::from_byte_array(UNSPENDABLE_BIP30_UTXO_91722)],
            ),
        ));

        // ---- 7. The transactions, against the UTXOs they spend ----
        // Repair: a proof of nothing, which verifies. The UTXOs this block spends are missing,
        // and the first input of the first transaction that spends anything is the one we hear
        // about.
        gaps.extend(compare(
            "the spent UTXOs are missing",
            BlockValidationErrors::UtxoNotFound(test_outpoint(0)),
            chain.feed(&block, Proof::default(), HashMap::new(), Vec::new()),
        ));

        // Repair: hand the UTXOs over. They come from a coinbase of this very block's height,
        // which can't be spent for another hundred blocks.
        let mut utxo = UtxoData {
            txout: txout!(0, ScriptBuf::new_op_return([0x0, 0x1])),
            is_coinbase: true,
            creation_height: 12,
            creation_time: 0,
        };
        gaps.extend(compare(
            "an immature coinbase",
            BlockValidationErrors::CoinbaseNotMatured,
            chain.feed(&block, Proof::default(), spent_utxos(&utxo), Vec::new()),
        ));

        // Repair: UTXOs that aren't a coinbase's. Their script can't be spent by anyone.
        utxo.is_coinbase = false;
        gaps.extend(compare(
            "an unspendable script",
            BlockValidationErrors::ScriptError,
            chain.feed(&block, Proof::default(), spent_utxos(&utxo), Vec::new()),
        ));

        // Repair: a script anyone can spend. They hold nothing, and the spend pays out a coin.
        utxo.txout.script_pubkey = anyone_can_spend_bytes(1);
        gaps.extend(compare(
            "spending more than it holds",
            BlockValidationErrors::NotEnoughMoney,
            chain.feed(&block, Proof::default(), spent_utxos(&utxo), Vec::new()),
        ));

        // Repair: enough value to cover what it pays out. The inputs still claim a relative
        // lock time that this height doesn't satisfy.
        utxo.txout.value = Amount::ONE_SAT;
        gaps.extend(compare(
            "a relative lock time",
            BlockValidationErrors::UnsatisfiedSequenceLocks,
            chain.feed(&block, Proof::default(), spent_utxos(&utxo), Vec::new()),
        ));

        // Repair: inputs that claim no lock time. The coinbase still claims more than the
        // block pays it.
        for input in txdata[1].input.iter_mut() {
            input.sequence = Sequence::MAX;
        }
        block = rebuild(&txdata, block.header);

        gaps.extend(compare(
            "the coinbase claims too much",
            BlockValidationErrors::BadCoinbaseOutValue,
            chain.feed(&block, Proof::default(), spent_utxos(&utxo), Vec::new()),
        ));

        // Repair: a coinbase that claims what the block pays it. There's nothing wrong with
        // this block anymore, so it connects and becomes our tip.
        txdata[0].output[0].value = Amount::ZERO;
        block = rebuild(&txdata, block.header);

        match chain.feed(&block, Proof::default(), spent_utxos(&utxo), Vec::new()) {
            Ok(12) => assert_eq!(chain.get_height().unwrap(), 12),
            Ok(height) => gaps.push(format!("valid block: connected at height {height}, not 12")),
            Err(e) => gaps.push(format!("valid block: expected it to connect, got {e:?}")),
        }

        assert!(
            gaps.is_empty(),
            "the validation pipeline diverges from Bitcoin Core:\n{}",
            gaps.join("\n")
        );
    }

    /// Mines `count` valid regtest blocks on top of the genesis. Their timestamps grow by one
    /// second per block, so the chain has a Median Time Past to check candidates against.
    fn mine_chain(count: u32) -> Vec<Block> {
        let mut blocks: Vec<Block> = Vec::new();

        for height in 1..=count {
            let coinbase = test_coinbase(height, Sequence::MAX, LockTime::ZERO);
            let mut block = block_with_transactions(height, vec![coinbase]);

            if let Some(parent) = blocks.last() {
                block.header.prev_blockhash = parent.block_hash();
            }

            blocks.push(mine(block));
        }

        blocks
    }

    /// Rebuilds a block from `txdata`, keeping `header`'s fields, recomputing the merkle root
    /// and mining it: what a miner does after changing what goes into the block.
    fn rebuild(txdata: &[Transaction], header: BlockHeader) -> Block {
        let mut block = Block {
            header,
            txdata: txdata.to_vec(),
        };
        // Bitcoin Core defines the merkle root of an empty payload as zero
        block.header.merkle_root = block
            .compute_merkle_root()
            .unwrap_or(TxMerkleNode::all_zeros());

        mine(block)
    }

    /// The Median Time Past that a header building on the last of `blocks` has to beat.
    ///
    /// This is [`HeaderExt::median_time_past_with`], the same code the chain runs against its
    /// own headers, so the walk asks for exactly the timestamp the chain will compare against,
    /// down to how far back it looks and where it stops.
    fn median_time_past(blocks: &[Block]) -> u32 {
        let genesis = genesis_block(Network::Regtest).header;
        let mut ancestors = blocks
            .iter()
            .rev()
            .skip(1)
            .map(|block| block.header)
            .chain(core::iter::once(genesis));

        blocks
            .last()
            .expect("the walk always has a parent to build on")
            .header
            .median_time_past_with(|_| ancestors.next().ok_or(()))
            .expect("the ancestors reach the genesis, where the walk stops")
    }

    /// A proof and a deletion hash that don't verify against any accumulator we could have.
    fn bogus_proof() -> (Proof, Vec<sha256::Hash>) {
        let proof = Proof {
            targets: vec![UNPROVEN_LEAF_POSITION],
            hashes: Vec::new(),
        };

        (proof, vec![sha256::Hash::hash(UNPROVEN_LEAF)])
    }

    /// An output script that spends more than the whole block's sigop budget.
    fn sigop_script() -> ScriptBuf {
        let mut script = ScriptBuf::new();
        for _ in 0..SIGOPS_OVER_THE_BLOCK_LIMIT {
            script.push_opcode(OP_CHECKSIG);
        }

        script
    }

    /// A transaction that serializes to exactly [`MERKLE_NODE_TX_SIZE`] bytes. It spends an
    /// outpoint of its own, so that it isn't confused with what the block's real spend uses.
    fn sixty_four_byte_transaction() -> Transaction {
        let mut transaction = Transaction {
            version: TransactionVersion::TWO,
            lock_time: LockTime::ZERO,
            input: vec![txin!(test_outpoint(9), ScriptBuf::new(), Sequence::MAX)],
            output: vec![txout!(1, ScriptBuf::new())],
        };

        // Everything but the output script is fixed, so that's what we pad with
        let padding = MERKLE_NODE_TX_SIZE - transaction.base_size();
        transaction.output[0].script_pubkey = anyone_can_spend_bytes(padding);
        assert_eq!(transaction.base_size(), MERKLE_NODE_TX_SIZE);

        transaction
    }

    /// The UTXOs the block's spend claims, both of them `utxo`.
    fn spent_utxos(utxo: &UtxoData) -> HashMap<OutPoint, UtxoData> {
        HashMap::from([
            (test_outpoint(0), utxo.clone()),
            (test_outpoint(1), utxo.clone()),
        ])
    }

    /// A script of `len` bytes that anyone can spend, for padding a transaction to a size.
    fn anyone_can_spend_bytes(len: usize) -> ScriptBuf {
        ScriptBuf::from_bytes(vec![OP_TRUE.to_u8(); len])
    }

    /// A coinbase output carrying `commitment` as the BIP141 witness commitment: the
    /// `OP_RETURN OP_PUSHBYTES_36 aa21a9ed` header followed by the 32 committed bytes.
    fn witness_commitment_output(commitment: [u8; 32]) -> TxOut {
        let mut script = WITNESS_COMMITMENT_HEADER.to_vec();
        script.extend_from_slice(&commitment);

        txout!(0, ScriptBuf::from_bytes(script))
    }

    /// Compares one step of the walk against the error Bitcoin Core reports for it: this is
    /// where both the ordering and the parity of that step are decided, and it describes the
    /// mismatch when there is one.
    ///
    /// A [`crate::TransactionError`] is a [`BlockValidationErrors`] with the txid of whoever
    /// caused it attached, so a step states the error it expects without caring which of the
    /// two it comes back in.
    fn compare<T: core::fmt::Debug>(
        step: &str,
        expected: BlockValidationErrors,
        got: Result<T, BlockchainError>,
    ) -> Option<String> {
        let actual = match &got {
            Err(BlockchainError::BlockValidation(error)) => Some(error),
            Err(BlockchainError::TransactionError(error)) => Some(&error.error),
            _ => None,
        };

        match actual {
            Some(actual) if *actual == expected => None,
            _ => Some(format!(
                "{step}: expected the validation error {expected:?}, got {got:?}"
            )),
        }
    }
}
