// SPDX-License-Identifier: MIT OR Apache-2.0

//! This module defines error types specific to the blockchain validation and database operations, along with conversion between types.
//!
//! The main error types are:
//! - [BlockchainError]: High-level error type that encapsulates all the error kinds from our node chain backend operation.
//! - [TransactionError]: Represents errors in transaction validation
//! - [BlockValidationErrors]: Errors encountered during block validation that are not tied to any specific transaction
//!
//! Each error type implements `Display` and `Debug` for error reporting.

extern crate alloc;

use core::error::Error;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Display;
use core::fmt::Formatter;

use bitcoin::OutPoint;
use bitcoin::Txid;
use floresta_common::impl_error_from;
use floresta_common::prelude::*;
use rustreexo::stump::StumpError;

use crate::extensions::ChainWorkOverflow;
use crate::proof_util::UtreexoLeafError;
use crate::pruned_utreexo::chain_state_builder::BlockchainBuilderError;

pub trait DatabaseError: Debug + Display + Send + Sync + 'static {}

#[derive(Debug)]
/// Errors that can happen whilst interacting with the local blockchain.
///
/// It's the highest level error type in [`floresta_chain`](crate),
/// and is returned by [`ChainState`](crate::ChainState) methods.
pub enum BlockchainError {
    /// The block is not present in the [`ChainState`](crate::ChainState).
    BlockNotPresent,

    /// The block is an orphan or is invalid.
    OrphanOrInvalidBlock,

    /// The block failed validation.
    BlockValidation(BlockValidationErrors),

    /// The block contains invalid transaction(s).
    TransactionError(TransactionError),

    /// The Utreexo proof for this block is invalid.
    InvalidUtreexoProof,

    /// Error whilst interacting with the [accumulator](rustreexo::stump::Stump).
    AccumulatorError(StumpError),

    /// Failed to reconstruct a scriptpubkey from a [leaf](crate::pruned_utreexo::udata::CompactLeafData).
    UtreexoLeaf(UtreexoLeafError),

    /// Error whilst interacting with the the [`ChainStore`](crate::ChainStore).
    Database(Box<dyn DatabaseError>),

    /// The [`ChainState`](crate::ChainState) is not initialized.
    ChainNotInitialized,

    /// The [`ChainState`](crate::ChainState)'s tip is invalid.
    InvalidTip(String),

    /// The [`ChainState`](crate::ChainState)'s validation index is invalid.
    BadValidationIndex,

    /// A [`ChainState`](crate::ChainState) operation overflowed.
    OperationOverflow(ChainWorkOverflow),
}

impl_error_from!(BlockchainError, ChainWorkOverflow, OperationOverflow);
impl Display for BlockchainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlockNotPresent => write!(f, "The block is not present in the ChainState"),
            Self::OrphanOrInvalidBlock => write!(f, "The block was orphaned or is invalid"),
            Self::BlockValidation(e) => write!(f, "Failed to validate the block: {e}"),
            Self::TransactionError(e) => {
                write!(f, "The block contains invalid transaction(s): {e}")
            }
            Self::InvalidUtreexoProof => write!(f, "The Utreexo proof for this block is invalid"),
            Self::AccumulatorError(e) => {
                write!(f, "Error whilst interacting with the accumulator: {e:?}")
            }
            Self::UtreexoLeaf(e) => write!(
                f,
                "Failed to reconstruct a scriptpubkey from Compact Leaf Data: {e}"
            ),
            Self::Database(e) => {
                write!(f, "Error whilst interacting with the the ChainState: {e}")
            }
            Self::ChainNotInitialized => write!(f, "The ChainState is not initialized"),
            Self::InvalidTip(e) => write!(f, "The ChainState's tip is invalid: {e}"),
            Self::BadValidationIndex => write!(f, "The ChainState's validation index is invalid"),
            Self::OperationOverflow(_) => write!(f, "A ChainState operation overflowed"),
        }
    }
}

impl Error for BlockchainError {}

impl<T: DatabaseError> From<T> for BlockchainError {
    fn from(value: T) -> Self {
        Self::Database(Box::new(value))
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Represents errors encountered during transaction validation.
pub struct TransactionError {
    /// The id of the transaction that caused this error
    pub txid: Txid,

    /// The error we've encountered
    pub error: BlockValidationErrors,
}

#[derive(Clone, Debug, PartialEq)]
/// Represents errors encountered during block validation.
///
/// Every variant names the Bitcoin Core reject reason it stands for, so that the two
/// implementations can be compared check by check. Bitcoin Core splits these checks between
/// the networking layer (`net_processing`) and consensus (`validation.cpp`); we don't have
/// that split, so all of them are raised by `floresta-chain` and bubble up to the caller.
///
/// The variants marked **not raised yet** name something we don't report yet: mostly checks
/// that Bitcoin Core performs and we don't, plus the odd case where we want to tell the caller
/// more than Core does. Nothing constructs them today: they exist so that the gap is written
/// down, and so that the validation-pipeline acceptance test can state the error each of them
/// is expected to produce once it's implemented.
pub enum BlockValidationErrors {
    /// This block is not the next one we have to validate. It says nothing about the block:
    /// it's the caller handing us one we can't do anything with yet.
    ///
    /// Bitcoin Core has no equivalent, and this is a **known divergence**: we only ever connect
    /// the block right after our validation index, on the chain we consider active. A block on
    /// a fork can't be handed to us at all — we take its *header*, keep that branch as headers
    /// alone, and only validate its blocks if the branch wins and we switch to it.
    ///
    /// Core takes any block whose parent it has and writes it to disk, whatever branch it sits
    /// on. We keep no blocks: we validate the transaction data the main chain needs and then
    /// discard it, holding one accumulator, the one at our validation index. Validating a fork
    /// block would leave us nothing to keep, so a fork is worth headers to us and nothing more.
    BlockDoesntExtendTip,

    /// The coinbase transaction is malformed.
    ///
    /// Bitcoin Core: `bad-cb-length` and `bad-txns-prevout-null`.
    InvalidCoinbase(String),

    /// A spent output isn't in the UTXOs we've been given for this block, either because it
    /// doesn't exist or because it was already spent.
    ///
    /// Bitcoin Core: `bad-txns-inputs-missingorspent`.
    UtxoNotFound(OutPoint),

    /// An input script didn't validate against the output it spends.
    ///
    /// Bitcoin Core: `mandatory-script-verify-flag-failed` and
    /// `block-script-verify-flag-failed`.
    ScriptValidationError(String),

    /// A non-coinbase transaction has a null PrevOut.
    ///
    /// Bitcoin Core: `bad-txns-prevout-null`.
    NullPrevOut,

    /// A transaction has no inputs.
    ///
    /// Bitcoin Core: `bad-txns-vin-empty`.
    EmptyInputs,

    /// A transaction has no outputs.
    ///
    /// Bitcoin Core: `bad-txns-vout-empty`.
    EmptyOutputs,

    /// A script is unspendable or has too many sigops.
    ///
    /// Bitcoin Core: `bad-txns-too-many-sigops`.
    ScriptError,

    /// The block weight is above the 4,000,000 WU limit.
    ///
    /// Bitcoin Core: `bad-blk-weight` and `bad-blk-length`.
    BlockTooBig,

    /// An amount is above the 21 million coin limit.
    ///
    /// Bitcoin Core: `bad-txns-vout-toolarge`, `bad-txns-txouttotal-toolarge`,
    /// `bad-txns-inputvalues-outofrange`, `bad-txns-fee-outofrange` and
    /// `bad-txns-accumulated-fee-outofrange`.
    TooManyCoins,

    /// The header hash doesn't meet the target it claims.
    ///
    /// Bitcoin Core: `high-hash`.
    NotEnoughPow,

    /// The header's merkle root doesn't commit to the transactions in this block.
    ///
    /// Bitcoin Core: `bad-txnmrklroot`. This describes the payload we've been given, not the
    /// header, which may still have a valid payload that another peer can send us, so it must
    /// not mark the header invalid.
    BadMerkleRoot,

    /// The witness merkle root doesn't match the coinbase witness commitment.
    ///
    /// Bitcoin Core: `bad-witness-merkle-match`. Like [`BlockValidationErrors::BadMerkleRoot`],
    /// this describes the payload and must not mark the header invalid.
    BadWitnessCommitment,

    /// A transaction spends more than its inputs hold.
    ///
    /// Bitcoin Core: `bad-txns-in-belowout`.
    NotEnoughMoney,

    /// The first transaction in the block isn't a coinbase.
    ///
    /// Bitcoin Core: `bad-cb-missing`.
    FirstTxIsNotCoinbase,

    /// The coinbase claims more than the subsidy plus the block fees.
    ///
    /// Bitcoin Core: `bad-cb-amount`.
    BadCoinbaseOutValue,

    /// The block has no transactions, so it has no coinbase either.
    ///
    /// Bitcoin Core: `bad-blk-length`.
    EmptyBlock,

    /// This block extends a chain whose ancestors we don't have.
    ///
    /// Bitcoin Core has no equivalent: it doesn't validate a block before connecting its
    /// parent. See [`BlockValidationErrors::PrevBlockNotFound`] for the missing-parent case.
    BlockExtendsAnOrphanChain,

    /// The coinbase doesn't commit to the block height, as required after BIP34.
    ///
    /// Bitcoin Core: `bad-cb-height`.
    BadBip34,

    /// The proof doesn't verify the spent UTXOs against our accumulator.
    ///
    /// Bitcoin Core has no equivalent: it holds the whole UTXO set, so it looks the spent
    /// outputs up instead of having them proven.
    ///
    /// Today a proof failure reaches the caller in three shapes: this one, rustreexo's own
    /// error let out as [`BlockchainError::AccumulatorError`], and
    /// [`BlockchainError::InvalidUtreexoProof`], the top-level variant of the same name that a
    /// partial chain answers with. `floresta-wire` has to fold all three back together before
    /// it can decide whom to blame, which is the thing to fix: what went wrong with a proof
    /// should be said here, once.
    InvalidUtreexoProof,

    /// A transaction spends a coinbase output that isn't 100 blocks old yet.
    ///
    /// Bitcoin Core: `bad-txns-premature-spend-of-coinbase`.
    CoinbaseNotMatured,

    /// A transaction's absolute lock time is not final for this block.
    ///
    /// Bitcoin Core: `bad-txns-nonfinal`.
    NonFinalTransaction,

    /// A transaction spends an output that the historical BIP30 violation overwrote.
    ///
    /// Bitcoin Core: `bad-txns-BIP30`. For us it's the other way around: Utreexo commits to the
    /// block hash in the leaf hash, so the output is still provable and we reject it here.
    UnspendableUTXO,

    /// The header timestamp went backwards on a difficulty-adjustment block.
    ///
    /// Bitcoin Core: `time-timewarp-attack`.
    BIP94TimeWarp,

    /// A transaction spends the same output more than once.
    ///
    /// Bitcoin Core: `bad-txns-inputs-duplicate`.
    DuplicateInput,

    /// The proof doesn't line up with the deletions it comes with, claiming a different number
    /// of targets than the leaf hashes handed over with it.
    ///
    /// Bitcoin Core has no equivalent, as with every proof failure. **Not raised yet**: with
    /// nothing to delete, rustreexo returns before it so much as looks at the proof, so a proof
    /// claiming targets that nothing accounts for goes through, and we don't check it either.
    MalformedUtreexoProof,

    /// We already have this header, and nothing is wrong with it as far as we know.
    ///
    /// Bitcoin Core has no equivalent: `AcceptBlockHeader` reports success for a header it
    /// already has. **Not raised yet**: we do the same, so the caller can't tell a header that
    /// advanced our chain from one we had already seen, and has to work that out on its own.
    DuplicateBlock,

    /// We already have this header, and we've already marked it invalid.
    ///
    /// Bitcoin Core: `duplicate-invalid`. **Not raised yet**: we short-circuit every header we
    /// already have, whether we've marked it invalid or not, and report success.
    DuplicateInvalidBlock,

    /// We don't have this block's parent, so we can't validate the header.
    ///
    /// Bitcoin Core: `prev-blk-not-found`. **Not raised yet**: looking the parent up is the
    /// first thing we do, and a miss surfaces as [`BlockchainError::BlockNotPresent`], which
    /// says nothing about why we were looking it up.
    PrevBlockNotFound,

    /// This block's parent is one we've already marked invalid.
    ///
    /// Bitcoin Core: `bad-prevblk`. **Not raised yet**: we report it as
    /// [`BlockValidationErrors::BlockExtendsAnOrphanChain`], which doesn't say that we know
    /// the parent and know it to be invalid.
    BadPrevBlock,

    /// The header claims less work than the one we require for its height.
    ///
    /// Bitcoin Core: `bad-diffbits`. **Not raised yet**: we report it as
    /// [`BlockValidationErrors::NotEnoughPow`], conflating it with Core's `high-hash`. Note
    /// that Core compares the compact encoding for equality, while we only require the claimed
    /// target to be at most the expected one, since the testnet minimum-difficulty rule makes
    /// the expected `nBits` ambiguous.
    BadDifficultyBits,

    /// The header timestamp is not above the Median Time Past of its ancestors.
    ///
    /// Bitcoin Core: `time-too-old`. **Not raised yet**: we compute the MTP only to decide the
    /// lock-time cutoff, and never compare the header timestamp against it.
    TimeTooOld,

    /// The header timestamp is more than 2 hours into the future.
    ///
    /// Bitcoin Core: `time-too-new`. **Not raised yet**. Note this isn't a consensus failure:
    /// the block may become valid as time passes, so the header must not be marked invalid and
    /// the peer must not be punished.
    TimeTooNew,

    /// The header version is below the minimum required at this height by BIP34, BIP66 or
    /// BIP65.
    ///
    /// Bitcoin Core: `bad-version(0x...)`. **Not raised yet**.
    BadBlockVersion,

    /// The signet block solution doesn't sign this block with the network challenge.
    ///
    /// Bitcoin Core: `bad-signet-blksig`. **Not raised yet**: we don't validate the signet
    /// block signature at all.
    BadSignetBlockSignature,

    /// The merkle tree has duplicated real siblings, so a different transaction list yields
    /// the same root ([CVE-2012-2459]).
    ///
    /// Bitcoin Core: `bad-txns-duplicate`. **Not raised yet**: we do detect the mutation, but
    /// report it as [`BlockValidationErrors::BadMerkleRoot`].
    ///
    /// [CVE-2012-2459]: https://www.cve.org/CVERecord?id=CVE-2012-2459
    DuplicateTransactions,

    /// A block without a coinbase carries a transaction that serializes to exactly 64 bytes,
    /// which is indistinguishable from an inner merkle node ([CVE-2017-12842]).
    ///
    /// Bitcoin Core has no reject reason for this: such a block is already invalid, and is only
    /// treated as mutated so that the header isn't marked invalid. **Not raised yet**.
    ///
    /// [CVE-2017-12842]: https://www.cve.org/CVERecord?id=CVE-2017-12842
    SixtyFourByteTransaction,

    /// The witness commitment is present, but the coinbase witness isn't a single 32-byte item.
    ///
    /// Bitcoin Core: `bad-witness-nonce-size`. **Not raised yet**: the
    /// [`check_witness_commitment`](bitcoin::Block::check_witness_commitment) helper we use
    /// folds this into [`BlockValidationErrors::BadWitnessCommitment`].
    BadWitnessNonceSize,

    /// The block has no witness commitment, but some transaction carries witness data.
    ///
    /// Bitcoin Core: `unexpected-witness`. **Not raised yet**: folded into
    /// [`BlockValidationErrors::BadWitnessCommitment`] as well.
    UnexpectedWitness,

    /// A transaction other than the first one is a coinbase.
    ///
    /// Bitcoin Core: `bad-cb-multiple`. **Not raised yet**: such a transaction has a null
    /// PrevOut, so we reject it with [`BlockValidationErrors::NullPrevOut`], which is Core's
    /// `bad-txns-prevout-null`, instead of catching it as a second coinbase.
    MultipleCoinbase,

    /// The block's total sigop cost is above the 80,000 limit.
    ///
    /// Bitcoin Core: `bad-blk-sigops`. **Not raised yet**: we only bound the sigops of each
    /// individual script, never their sum over the block.
    TooManySigOps,

    /// A transaction's BIP68 relative lock times are not satisfied at this height.
    ///
    /// Bitcoin Core reports this as `bad-txns-nonfinal` too, but it's a separate check.
    /// **Not raised yet**: we don't evaluate sequence locks.
    UnsatisfiedSequenceLocks,
}

// Helpful macro for generating a TransactionError
macro_rules! tx_err {
    ($txid_fn:expr, $variant:ident, $msg:expr) => {
        TransactionError {
            txid: ($txid_fn)(),
            error: BlockValidationErrors::$variant($msg.into()),
        }
    };
    ($txid_fn:expr, $variant:ident) => {
        TransactionError {
            txid: ($txid_fn)(),
            error: BlockValidationErrors::$variant,
        }
    };
}

impl Display for TransactionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Transaction {} is invalid: {}", self.txid, self.error)
    }
}

impl Display for BlockValidationErrors {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlockDoesntExtendTip => {
                write!(f, "This block doesn't build directly on the tip")
            }
            Self::ScriptValidationError(e) => {
                write!(f, "{e}")
            }
            Self::UtxoNotFound(outpoint) => {
                write!(f, "Utxo referenced by {outpoint:?} not found")
            }
            Self::NullPrevOut => {
                write!(
                    f,
                    "This transaction has a null PrevOut but it's not coinbase"
                )
            }
            Self::EmptyInputs => {
                write!(f, "This transaction has no inputs")
            }
            Self::EmptyOutputs => {
                write!(f, "This transaction has no outputs")
            }
            Self::BlockTooBig => write!(f, "Block too big"),
            Self::InvalidCoinbase(e) => {
                write!(f, "Invalid coinbase: {e:?}")
            }
            Self::TooManyCoins => write!(f, "Moving more coins that exists"),
            Self::ScriptError => {
                write!(
                    f,
                    "Script does not follow size requirements of 2>= and <=520"
                )
            }
            Self::NotEnoughPow => {
                write!(f, "This block doesn't have enough proof-of-work")
            }
            Self::BadMerkleRoot => write!(f, "Wrong merkle root"),
            Self::BadWitnessCommitment => write!(f, "Wrong witness commitment"),
            Self::NotEnoughMoney => {
                write!(f, "A transaction spends more than it should")
            }
            Self::FirstTxIsNotCoinbase => {
                write!(f, "The first transaction in a block isn't a coinbase")
            }
            Self::BadCoinbaseOutValue => {
                write!(f, "Coinbase claims more bitcoins than it should")
            }
            Self::EmptyBlock => {
                write!(f, "This block is empty (doesn't have a coinbase tx)")
            }
            Self::BlockExtendsAnOrphanChain => {
                write!(f, "This block extends a chain we don't have the ancestors")
            }
            Self::BadBip34 => write!(f, "BIP34 commitment mismatch"),
            Self::InvalidUtreexoProof => write!(f, "Invalid proof"),
            Self::CoinbaseNotMatured => {
                write!(f, "Coinbase not matured yet")
            }
            Self::NonFinalTransaction => {
                write!(f, "Block contains a non-final transaction")
            }
            Self::UnspendableUTXO => {
                write!(
                    f,
                    "Attempts to spend unspendable UTXO that was overwritten by the historical BIP30 violation"
                )
            }
            Self::BIP94TimeWarp => {
                write!(f, "BIP94 time warp detected")
            }
            Self::DuplicateInput => {
                write!(f, "This transaction has duplicate inputs")
            }
            Self::MalformedUtreexoProof => {
                write!(f, "The proof doesn't match the deletions it comes with")
            }
            Self::DuplicateBlock => {
                write!(f, "We already know this block")
            }
            Self::DuplicateInvalidBlock => {
                write!(f, "We already know this block, and it's invalid")
            }
            Self::PrevBlockNotFound => {
                write!(f, "We don't have this block's parent")
            }
            Self::BadPrevBlock => {
                write!(f, "This block's parent is invalid")
            }
            Self::BadDifficultyBits => {
                write!(f, "This block claims less work than we require")
            }
            Self::TimeTooOld => {
                write!(f, "This block is older than the Median Time Past")
            }
            Self::TimeTooNew => {
                write!(f, "This block is too far into the future")
            }
            Self::BadBlockVersion => {
                write!(f, "This block's version is below the required one")
            }
            Self::BadSignetBlockSignature => {
                write!(f, "This block's signet solution is invalid")
            }
            Self::DuplicateTransactions => {
                write!(f, "This block's merkle tree has duplicate transactions")
            }
            Self::SixtyFourByteTransaction => {
                write!(f, "This block has a transaction of exactly 64 bytes")
            }
            Self::BadWitnessNonceSize => {
                write!(f, "The witness reserved value isn't a single 32-byte item")
            }
            Self::UnexpectedWitness => {
                write!(f, "There's witness data but no witness commitment")
            }
            Self::MultipleCoinbase => {
                write!(f, "This block has more than one coinbase transaction")
            }
            Self::TooManySigOps => {
                write!(f, "This block has too many sigops")
            }
            Self::UnsatisfiedSequenceLocks => {
                write!(f, "A transaction's relative lock times aren't satisfied")
            }
        }
    }
}

impl<T: DatabaseError> From<T> for BlockchainBuilderError {
    fn from(value: T) -> Self {
        Self::Database(Box::new(value))
    }
}

impl_error_from!(BlockchainError, TransactionError, TransactionError);
impl_error_from!(BlockchainError, BlockValidationErrors, BlockValidation);
impl_error_from!(BlockchainError, StumpError, AccumulatorError);
