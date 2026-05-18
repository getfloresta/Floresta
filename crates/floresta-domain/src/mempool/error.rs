// SPDX-License-Identifier: MIT OR Apache-2.0

use core::error::Error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;

// TODO(csgui) This is a know violation of the layering architecture.
// It was added here to make a previous task simpler to review and merge.
// It should be removed in a follow-up PR.
use floresta_chain::BlockchainError;

#[derive(Debug)]
/// Errors that can occur whilst trying to add a transaction to the `Mempool`.
pub enum MempoolError {
    /// The `Mempool` is full and cannot accept more [`Transaction`](bitcoin::Transaction)
    FullMempool,

    /// The `Transaction` conflicts with another `Transaction` in the `Mempool`.
    ConflictingTransaction,

    /// The `Transaction` has duplicate inputs.
    DuplicatedInputs,

    // TODO(davidson): we might want to make an error type specific for consensus,
    // instead of reusing BlockchainError.
    /// The `Transaction` failed consensus validation.
    ConsensusValidation(BlockchainError),
}

impl Display for MempoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::FullMempool => {
                write!(
                    f,
                    "The mempool is full and cannot accept any more transactions"
                )
            }
            Self::ConflictingTransaction => {
                write!(
                    f,
                    "The transaction conflicts with another transaction in the mempool"
                )
            }
            Self::DuplicatedInputs => {
                write!(f, "The transaction has duplicate inputs")
            }
            Self::ConsensusValidation(e) => {
                write!(f, "The transaction failed consensus validation: {e}")
            }
        }
    }
}

impl Error for MempoolError {}
