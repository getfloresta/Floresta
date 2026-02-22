//! This module defines error types specific to the blockchain validation and database operations, along with conversion between types.
//!
//! The main error types are:
//! - [BlockchainError]: High-level error type that encapsulates all the error kinds from our node chain backend operation.
//! - [TransactionError]: Represents errors in transaction validation
//! - [BlockValidationErrors]: Errors encountered during block validation that are not tied to any specific transaction
//!
//! Each error type implements `Display` and `Debug` for error reporting.

use core::fmt::Debug;
extern crate alloc;

use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::Txid;
use floresta_common::impl_error_from;
use floresta_common::prelude::*;

use crate::proof_util::UtreexoLeafError;
use crate::pruned_utreexo::chain_state_builder::BlockchainBuilderError;

pub trait DatabaseError: Debug + Send + Sync + 'static {}
#[derive(Debug)]
/// This is the highest level error type in floresta-chain, returned by the [crate::ChainState] methods.
/// It represents errors encountered during blockchain validation.
pub enum BlockchainError {
    BlockNotPresent,
    OrphanOrInvalidBlock,
    Parsing(String),
    BlockValidation(BlockValidationErrors),
    TransactionError(TransactionError),
    InvalidProof,
    UtreexoError(String),
    UtreexoLeaf(UtreexoLeafError),
    Database(Box<dyn DatabaseError>),
    ConsensusDecode(bitcoin::consensus::encode::Error),
    ChainNotInitialized,
    InvalidTip(String),
    Io(ioError),
    UnsupportedNetwork(Network),
    BadValidationIndex,
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
pub enum BlockValidationErrors {
    BlockDoesntExtendTip,
    InvalidCoinbase(String),
    UtxoNotFound(OutPoint),
    ScriptValidationError(String),
    NullPrevOut,
    EmptyInputs,
    EmptyOutputs,
    ScriptError,
    BlockTooBig,
    TooManyCoins,
    NotEnoughPow,
    BadMerkleRoot,
    BadWitnessCommitment,
    NotEnoughMoney,
    FirstTxIsNotCoinbase,
    BadCoinbaseOutValue,
    EmptyBlock,
    BlockExtendsAnOrphanChain,
    BadBip34,
    InvalidProof,
    CoinbaseNotMatured,
    UnspendableUTXO,
    BIP94TimeWarp,
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Transaction {} is invalid: {}", self.txid, self.error)
    }
}

impl Display for BlockValidationErrors {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BlockValidationErrors::BlockDoesntExtendTip => {
                write!(f, "This block doesn't build directly on the tip")
            }
            BlockValidationErrors::ScriptValidationError(e) => {
                write!(f, "{e}")
            }
            BlockValidationErrors::UtxoNotFound(outpoint) => {
                write!(f, "Utxo referenced by {outpoint:?} not found")
            }
            BlockValidationErrors::NullPrevOut => {
                write!(
                    f,
                    "This transaction has a null PrevOut but it's not coinbase"
                )
            }
            BlockValidationErrors::EmptyInputs => {
                write!(f, "This transaction has no inputs")
            }
            BlockValidationErrors::EmptyOutputs => {
                write!(f, "This transaction has no outputs")
            }
            BlockValidationErrors::BlockTooBig => write!(f, "Block too big"),
            BlockValidationErrors::InvalidCoinbase(e) => {
                write!(f, "Invalid coinbase: {e:?}")
            }
            BlockValidationErrors::TooManyCoins => write!(f, "Moving more coins that exists"),
            BlockValidationErrors::ScriptError => {
                write!(
                    f,
                    "Script does not follow size requirements of 2>= and <=520"
                )
            }
            BlockValidationErrors::NotEnoughPow => {
                write!(f, "This block doesn't have enough proof-of-work")
            }
            BlockValidationErrors::BadMerkleRoot => write!(f, "Wrong merkle root"),
            BlockValidationErrors::BadWitnessCommitment => write!(f, "Wrong witness commitment"),
            BlockValidationErrors::NotEnoughMoney => {
                write!(f, "A transaction spends more than it should")
            }
            BlockValidationErrors::FirstTxIsNotCoinbase => {
                write!(f, "The first transaction in a block isn't a coinbase")
            }
            BlockValidationErrors::BadCoinbaseOutValue => {
                write!(f, "Coinbase claims more bitcoins than it should")
            }
            BlockValidationErrors::EmptyBlock => {
                write!(f, "This block is empty (doesn't have a coinbase tx)")
            }
            BlockValidationErrors::BlockExtendsAnOrphanChain => {
                write!(f, "This block extends a chain we don't have the ancestors")
            }
            BlockValidationErrors::BadBip34 => write!(f, "BIP34 commitment mismatch"),
            BlockValidationErrors::InvalidProof => write!(f, "Invalid proof"),
            BlockValidationErrors::CoinbaseNotMatured => {
                write!(f, "Coinbase not matured yet")
            }
            BlockValidationErrors::UnspendableUTXO => {
                write!(f, "Attempts to spend unspendable UTXO that was overwritten by the historical BIP30 violation")
            }
            BlockValidationErrors::BIP94TimeWarp => {
                write!(f, "BIP94 time warp detected")
            }
        }
    }
}

impl<T: DatabaseError> From<T> for BlockchainError {
    fn from(value: T) -> Self {
        BlockchainError::Database(Box::new(value))
    }
}

impl<T: DatabaseError> From<T> for BlockchainBuilderError {
    fn from(value: T) -> Self {
        BlockchainBuilderError::Database(Box::new(value))
    }
}

impl_error_from!(BlockchainError, ioError, Io);
impl_error_from!(BlockchainError, TransactionError, TransactionError);
impl_error_from!(
    BlockchainError,
    bitcoin::consensus::encode::Error,
    ConsensusDecode
);
impl_error_from!(BlockchainError, BlockValidationErrors, BlockValidation);
impl_error_from!(BlockchainError, String, UtreexoError);

impl Display for BlockchainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for BlockchainError {}
