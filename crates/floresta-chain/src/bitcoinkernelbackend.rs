// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bitcoin Core as a chain backend, so that our tests can ask it what it would have said.
//!
//! # This is an oracle, not a backend anyone should run
//!
//! Nothing here is meant for production, and none of it is written as if it were. It exists so
//! that a test can drive Bitcoin Core through the same trait it drives us through, and compare
//! the two answers. It panics where a real backend would return, it holds Core's chain state
//! on disk in whatever directory it's handed, and most of
//! [`UpdatableChainstate`](crate::pruned_utreexo::UpdatableChainstate) is left
//! unimplemented, because an oracle is asked exactly two things: take this header, take this
//! block.
//!
//! # Reading Core's answer
//!
//! Core says two things about a block it turns down, and they arrive separately:
//!
//! - the **reject reason**, the string its checks are named after, which only reaches us
//!   through the validation log, so the logger is turned up to debug and read back;
//! - the **validation result**, the coarse enum, which comes back with the block itself.
//!   Three checks log something other than their reject reason (`duplicate-invalid`,
//!   `prev-blk-not-found` and `bad-prevblk`), and the result is the only thing that names
//!   them, so it's the fallback.
//!
//! `from_core_reason` turns the reason into one of ours, and is the inverse of the
//! `Bitcoin Core:` lines on [`BlockValidationErrors`](crate::BlockValidationErrors). A reason
//! it doesn't know is a variant there that doesn't name it yet, and it says so.
//!
//! # What Core here can't answer
//!
//! - It is consensus only. `IsBlockMutated` lives in Core's networking layer, so the checks
//!   that are only there never fire and a later consensus check answers in their place.
//! - It holds the whole UTXO set and looks the spent outputs up for itself, so the proof and
//!   the UTXOs that `connect_block` carries are dropped on the floor.
//! - It has no way of being *told* a block is invalid. Hand it one that is, and it finds out.

// This is test scaffolding: it fails by panicking, on purpose and loudly.
#![allow(clippy::unwrap_used)]

extern crate std;

use std::string::String;
use std::string::ToString;
use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::block::Header as BlockHeader;
use bitcoin::consensus::serialize;
use bitcoin::hashes::sha256;
use bitcoinkernel::BlockValidationResult;
use bitcoinkernel::BlockValidationStateRef;
use bitcoinkernel::ChainType;
use bitcoinkernel::ChainstateManager;
use bitcoinkernel::ChainstateManagerBuilder;
use bitcoinkernel::Context;
use bitcoinkernel::ContextBuilder;
use bitcoinkernel::Log;
use bitcoinkernel::LogCategory;
use bitcoinkernel::LogLevel;
use bitcoinkernel::Logger;
use bitcoinkernel::ProcessBlockHeaderResult;
use bitcoinkernel::ProcessBlockResult;
use bitcoinkernel::ValidationMode;
use bitcoinkernel::prelude::BlockValidationStateExt;
use rustreexo::node_hash::BitcoinNodeHash;
use rustreexo::proof::Proof;
use rustreexo::stump::Stump;

use crate::BlockValidationErrors;
use crate::BlockchainError;
use crate::prelude::HashMap;
use crate::pruned_utreexo::IBDState;
use crate::pruned_utreexo::UpdatableChainstate;
use crate::pruned_utreexo::partial_chain::PartialChainState;
use crate::pruned_utreexo::utxo_data::UtxoData;

/// Bitcoin Core, behind [`UpdatableChainstate`].
pub struct KernelBackend {
    // Declared before what it borrows from, so that it's dropped first
    chainman: ChainstateManager,
    _context: Context,
    _logger: Logger,
    log: Arc<Mutex<Vec<String>>>,
    rejection: Arc<Mutex<Option<BlockValidationResult>>>,
}

impl KernelBackend {
    /// Bitcoin Core with an empty chain of its own, under `data_dir`.
    ///
    /// # Panics
    ///
    /// If Core won't start: there's nothing an oracle can do about that.
    pub fn new(network: Network, data_dir: &str) -> Self {
        let log = Arc::new(Mutex::new(Vec::new()));
        let rejection = Arc::new(Mutex::new(None));

        let logger = Logger::new(LogSink(log.clone())).expect("the log connects");
        // Both of these: the reject reasons of the header checks are logged at debug level,
        // under a category that is off by default
        logger.enable_category(LogCategory::Validation);
        logger.set_level_category(LogCategory::Validation, LogLevel::Debug);

        let seen = rejection.clone();
        let context = ContextBuilder::new()
            .chain_type(chain_type(network))
            .with_block_checked_validation(move |_block, state: BlockValidationStateRef<'_>| {
                // Core signals this for every block it connects, so only the ones that didn't
                // are worth keeping
                if state.mode() != ValidationMode::Valid {
                    *seen.lock().unwrap() = Some(state.result());
                }
            })
            .build()
            .expect("the context builds");

        let blocks_dir = std::format!("{data_dir}/blocks");
        std::fs::create_dir_all(&blocks_dir).expect("the data directory is ours to make");

        let chainman = ChainstateManagerBuilder::new(&context, data_dir, &blocks_dir)
            .expect("the chainstate options build")
            .build()
            .expect("the chainstate builds");

        Self {
            chainman,
            _context: context,
            _logger: logger,
            log,
            rejection,
        }
    }

    /// The height of the chain Core considers best.
    pub fn height(&self) -> u32 {
        self.chainman
            .active_chain()
            .height()
            .try_into()
            .expect("the chain is at the genesis or past it")
    }

    /// A block as Core takes them, or the reason it can't be handed this one at all.
    ///
    /// A transaction with no inputs has a zero where the input count goes, and that zero is the
    /// SegWit marker, so a block carrying one doesn't decode. No peer can send one either.
    fn to_kernel(block: &Block) -> Result<bitcoinkernel::Block, BlockchainError> {
        bitcoinkernel::Block::try_from(serialize(block).as_slice()).map_err(|_| {
            BlockchainError::InvalidTip("core can't be handed this block: it doesn't decode".into())
        })
    }

    /// What we would have said for the check that turned the last block or header down.
    fn last_rejection(&self) -> BlockchainError {
        // Copied out, so that nothing below panics holding a lock that Core's own threads are
        // going to take
        let log = self.log.lock().unwrap().clone();
        let rejection = self.rejection.lock().unwrap().take();

        if let Some(reason) = first_reject_reason(&log) {
            return BlockchainError::BlockValidation(from_core_reason(reason));
        }

        // The checks that log something other than their reject reason
        let error = match rejection {
            Some(BlockValidationResult::CachedInvalid) => {
                BlockValidationErrors::DuplicateInvalidBlock
            }
            Some(BlockValidationResult::MissingPrev) => BlockValidationErrors::PrevBlockNotFound,
            Some(BlockValidationResult::InvalidPrev) => BlockValidationErrors::BadPrevBlock,
            Some(BlockValidationResult::TimeFuture) => BlockValidationErrors::TimeTooNew,
            other => panic!("core turned a block down without saying why: {other:?}\n{log:#?}"),
        };

        BlockchainError::BlockValidation(error)
    }
}

impl UpdatableChainstate for KernelBackend {
    /// The proof, the UTXOs and the deletion hashes are ours, not Core's: it holds the whole
    /// UTXO set and looks the spent outputs up for itself.
    fn connect_block(
        &self,
        block: &Block,
        _proof: Proof,
        _inputs: HashMap<OutPoint, UtxoData>,
        _del_hashes: Vec<sha256::Hash>,
    ) -> Result<u32, BlockchainError> {
        self.log.lock().unwrap().clear();
        *self.rejection.lock().unwrap() = None;

        let processed = self.chainman.process_block(&Self::to_kernel(block)?);

        // A block that fails to connect is processed all the same: Core marks it invalid, keeps
        // the tip it had, and says so through the validation state rather than the outcome
        let connected = self.rejection.lock().unwrap().is_none();

        match processed {
            ProcessBlockResult::NewBlock | ProcessBlockResult::Duplicate if connected => {
                Ok(self.height())
            }
            _ => Err(self.last_rejection()),
        }
    }

    fn accept_header(&self, header: BlockHeader) -> Result<(), BlockchainError> {
        self.log.lock().unwrap().clear();

        let header = bitcoinkernel::BlockHeader::try_from(serialize(&header).as_slice())
            .expect("a header is 80 bytes, whatever is in it");

        match self.chainman.process_block_header(&header) {
            Ok(ProcessBlockHeaderResult::Valid) => Ok(()),
            Ok(ProcessBlockHeaderResult::Invalid(state)) => {
                *self.rejection.lock().unwrap() = Some(state.result());
                Err(self.last_rejection())
            }
            Err(e) => panic!("the kernel failed to process a header: {e:?}"),
        }
    }

    fn flush(&self) -> Result<(), BlockchainError> {
        // Core writes as it goes
        Ok(())
    }

    fn update_ibd(&self, _ibd_state: IBDState) {
        // Core keeps its own idea of whether it's caught up
    }

    fn invalidate_block(&self, _block: BlockHash) -> Result<(), BlockchainError> {
        unimplemented!("core can't be told a block is invalid: hand it one that is")
    }

    fn get_acc(&self) -> Stump {
        unimplemented!("core holds the UTXO set itself, there's no accumulator to speak of")
    }

    fn get_root_hashes(&self) -> Vec<BitcoinNodeHash> {
        unimplemented!("core holds the UTXO set itself, there's no accumulator to speak of")
    }

    fn switch_chain(&self, _new_tip: BlockHash) -> Result<(), BlockchainError> {
        unimplemented!("core reorgs on its own, as blocks arrive")
    }

    fn mark_block_as_valid(&self, _block: BlockHash) -> Result<(), BlockchainError> {
        unimplemented!("core makes up its own mind about a block")
    }

    fn mark_chain_as_assumed(&self, _acc: Stump, _tip: BlockHash) -> Result<bool, BlockchainError> {
        unimplemented!("core makes up its own mind about a chain")
    }

    fn get_partial_chain(
        &self,
        _initial_height: u32,
        _final_height: u32,
        _acc: Stump,
    ) -> Result<PartialChainState, BlockchainError> {
        unimplemented!("a partial chain is ours, core has no such thing")
    }

    fn handle_transaction(&self) -> Result<(), BlockchainError> {
        unimplemented!("this oracle is asked about blocks")
    }
}

/// Collects everything Core logs, so that the reject reasons can be read back.
struct LogSink(Arc<Mutex<Vec<String>>>);

impl Log for LogSink {
    fn log(&self, message: &str) {
        // Core calls this from its own threads, across an `extern "C"` boundary that a panic
        // would abort the process on, so a lock that a panicking test left poisoned has to be
        // taken all the same
        let mut log = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        log.push(message.to_string());
    }
}

fn chain_type(network: Network) -> ChainType {
    match network {
        Network::Bitcoin => ChainType::Mainnet,
        Network::Testnet => ChainType::Testnet,
        Network::Testnet4 => ChainType::Testnet4,
        Network::Signet => ChainType::Signet,
        Network::Regtest => ChainType::Regtest,
    }
}

/// What we would have said for a check Bitcoin Core names `reason`.
///
/// The inverse of the `Bitcoin Core:` lines on [`BlockValidationErrors`], so a reason missing
/// here is a variant there that doesn't name it yet.
///
/// # Panics
///
/// On a reason no variant names.
fn from_core_reason(reason: &str) -> BlockValidationErrors {
    match reason {
        "high-hash" => BlockValidationErrors::NotEnoughPow,
        "bad-diffbits" => BlockValidationErrors::BadDifficultyBits,
        "time-too-old" => BlockValidationErrors::TimeTooOld,
        "time-too-new" => BlockValidationErrors::TimeTooNew,
        "time-timewarp-attack" => BlockValidationErrors::BIP94TimeWarp,
        "bad-version" => BlockValidationErrors::BadBlockVersion,
        "prev-blk-not-found" => BlockValidationErrors::PrevBlockNotFound,
        "bad-prevblk" => BlockValidationErrors::BadPrevBlock,
        "duplicate-invalid" => BlockValidationErrors::DuplicateInvalidBlock,
        "bad-signet-blksig" => BlockValidationErrors::BadSignetBlockSignature,
        "bad-txnmrklroot" => BlockValidationErrors::BadMerkleRoot,
        "bad-txns-duplicate" => BlockValidationErrors::DuplicateTransactions,
        "bad-witness-nonce-size" => BlockValidationErrors::BadWitnessNonceSize,
        "bad-witness-merkle-match" => BlockValidationErrors::BadWitnessCommitment,
        "unexpected-witness" => BlockValidationErrors::UnexpectedWitness,
        // Core says this for a payload with no transactions and for one that's over the size
        // limits alike. We split the two, so one of them can't line up.
        "bad-blk-length" => BlockValidationErrors::EmptyBlock,
        "bad-blk-weight" => BlockValidationErrors::BlockTooBig,
        "bad-cb-missing" => BlockValidationErrors::FirstTxIsNotCoinbase,
        "bad-cb-multiple" => BlockValidationErrors::MultipleCoinbase,
        // Core names the check, we name what's wrong with the coinbase, and the only thing this
        // check turns down is a scriptsig of the wrong size
        "bad-cb-length" => BlockValidationErrors::InvalidCoinbase("Invalid ScriptSig size".into()),
        "bad-cb-amount" => BlockValidationErrors::BadCoinbaseOutValue,
        "bad-cb-height" => BlockValidationErrors::BadBip34,
        "bad-txns-vin-empty" => BlockValidationErrors::EmptyInputs,
        "bad-txns-vout-empty" => BlockValidationErrors::EmptyOutputs,
        "bad-txns-vout-negative"
        | "bad-txns-vout-toolarge"
        | "bad-txns-txouttotal-toolarge"
        | "bad-txns-inputvalues-outofrange"
        | "bad-txns-fee-outofrange"
        | "bad-txns-accumulated-fee-outofrange" => BlockValidationErrors::TooManyCoins,
        "bad-txns-inputs-duplicate" => BlockValidationErrors::DuplicateInput,
        "bad-txns-prevout-null" => BlockValidationErrors::NullPrevOut,
        // Core says this for an unsatisfied absolute lock time and for an unsatisfied relative
        // one alike, which we tell apart
        "bad-txns-nonfinal" => BlockValidationErrors::NonFinalTransaction,
        // Core names the transaction, not the output, so the outpoint we'd name is one it never
        // tells us
        "bad-txns-inputs-missingorspent" => BlockValidationErrors::UtxoNotFound(OutPoint::null()),
        "bad-txns-premature-spend-of-coinbase" => BlockValidationErrors::CoinbaseNotMatured,
        "bad-txns-in-belowout" => BlockValidationErrors::NotEnoughMoney,
        "bad-txns-BIP30" => BlockValidationErrors::UnspendableUTXO,
        "bad-blk-sigops" => BlockValidationErrors::TooManySigOps,
        "bad-txns-too-many-sigops" => BlockValidationErrors::ScriptError,
        "mandatory-script-verify-flag-failed" | "block-script-verify-flag-failed" => {
            BlockValidationErrors::ScriptValidationError(reason.into())
        }
        other => panic!("no variant of BlockValidationErrors names `{other}` yet"),
    }
}

/// Every reason [`from_core_reason`] knows, longest first so that one containing another is
/// matched whole.
fn core_reject_reasons() -> Vec<&'static str> {
    let mut reasons = std::vec![
        "high-hash",
        "bad-diffbits",
        "time-too-old",
        "time-too-new",
        "time-timewarp-attack",
        "bad-version",
        "prev-blk-not-found",
        "bad-prevblk",
        "duplicate-invalid",
        "bad-signet-blksig",
        "bad-txnmrklroot",
        "bad-txns-duplicate",
        "bad-witness-nonce-size",
        "bad-witness-merkle-match",
        "unexpected-witness",
        "bad-blk-length",
        "bad-blk-weight",
        "bad-cb-missing",
        "bad-cb-multiple",
        "bad-cb-length",
        "bad-cb-amount",
        "bad-cb-height",
        "bad-txns-vin-empty",
        "bad-txns-vout-empty",
        "bad-txns-vout-negative",
        "bad-txns-vout-toolarge",
        "bad-txns-txouttotal-toolarge",
        "bad-txns-inputvalues-outofrange",
        "bad-txns-fee-outofrange",
        "bad-txns-accumulated-fee-outofrange",
        "bad-txns-inputs-duplicate",
        "bad-txns-prevout-null",
        "bad-txns-nonfinal",
        "bad-txns-inputs-missingorspent",
        "bad-txns-premature-spend-of-coinbase",
        "bad-txns-in-belowout",
        "bad-txns-BIP30",
        "bad-blk-sigops",
        "bad-txns-too-many-sigops",
        "mandatory-script-verify-flag-failed",
        "block-script-verify-flag-failed",
    ];
    reasons.sort_by_key(|reason| core::cmp::Reverse(reason.len()));

    reasons
}

/// The reason of the first check that turned the block down, which is the earliest one Core
/// logged.
fn first_reject_reason(log: &[String]) -> Option<&'static str> {
    let reasons = core_reject_reasons();

    log.iter().find_map(|line| {
        reasons
            .iter()
            .filter_map(|reason| line.find(reason).map(|at| (at, *reason)))
            .min_by_key(|(at, reason)| (*at, core::cmp::Reverse(reason.len())))
            .map(|(_, reason)| reason)
    })
}
