// SPDX-License-Identifier: MIT OR Apache-2.0

//! A Utreexo-based Bitcoin transaction mempool.
//!
//! This crate provides a transaction mempool implementation specifically designed for
//! [Utreexo](https://eprint.iacr.org/2019/611.pdf) nodes. Unlike traditional Bitcoin nodes
//! that maintain a complete UTXO set, Utreexo nodes use a compact cryptographic accumulator
//! to verify transaction validity, significantly reducing storage requirements.
//!
//! # Overview
//!
//! The mempool serves as a holding area for unconfirmed transactions, performing several
//! critical functions:
//!
//! - **Transaction validation**: Verifies Utreexo inclusion proofs for transaction inputs
//! - **Proof management**: Maintains a local accumulator to generate proofs for relay and mining
//! - **Block template construction**: Assembles candidate blocks for miners
//! - **Transaction relay**: Tracks which transactions to broadcast to peers

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]

use core::error::Error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;

use floresta_chain::BlockchainError;

pub mod mempool;

pub use floresta_chain::pruned_utreexo::utxo_data::UtxoData;
pub use mempool::Mempool;

/// Relay-policy settings applied during mempool admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolPolicy {
    /// Minimum relay feerate in satoshis per virtual byte.
    pub min_relay_fee_sat_per_vbyte: u64,
    /// Maximum standard transaction weight in weight units.
    pub max_standard_tx_weight: u64,
    /// Maximum standard `scriptSig` length in bytes.
    pub max_standard_script_sig_size: usize,
    /// Maximum standard witness stack item length in bytes, excluding the final witness script.
    pub max_standard_witness_stack_item_size: usize,
    /// Maximum standard final witness element length in bytes.
    pub max_standard_witness_script_size: usize,
    /// Maximum standard number of witness stack items.
    pub max_standard_witness_stack_items: usize,
}

impl Default for MempoolPolicy {
    fn default() -> Self {
        Self {
            min_relay_fee_sat_per_vbyte: 1,
            max_standard_tx_weight: 400_000,
            max_standard_script_sig_size: 1_650,
            max_standard_witness_stack_item_size: 80,
            max_standard_witness_script_size: 3_600,
            max_standard_witness_stack_items: 100,
        }
    }
}

/// A typed mempool-admission error.
#[derive(Debug)]
pub enum MempoolError {
    /// Memory usage is too high.
    MemoryUsageTooHigh,
    /// The transaction is already present in the mempool.
    AlreadyKnown,
    /// The transaction conflicts with another transaction in the mempool.
    ConflictingTransaction,
    /// The transaction has duplicated inputs.
    DuplicatedInputs,
    /// The transaction feerate is below the relay floor.
    FeeTooLow,
    /// The transaction exceeds the configured standard weight.
    ExceedsMaxWeight,
    /// The transaction is non-standard under relay policy.
    NonStandard,
    /// The transaction exceeds the configured `scriptSig` size.
    ExceedsScriptSigSize,
    /// The caller didn't provide enough prevout metadata to evaluate policy.
    MissingPrevoutContext,
    /// A consensus validation error happened while checking the transaction.
    Consensus(BlockchainError),
}

impl Display for MempoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            MempoolError::MemoryUsageTooHigh => write!(f, "we are running out of memory"),
            MempoolError::AlreadyKnown => write!(f, "this transaction is already in the mempool"),
            MempoolError::ConflictingTransaction => {
                write!(f, "we have another transaction that spends the same input")
            }
            MempoolError::DuplicatedInputs => write!(f, "this transaction has duplicated inputs"),
            MempoolError::FeeTooLow => {
                write!(f, "this transaction does not meet the relay fee floor")
            }
            MempoolError::ExceedsMaxWeight => {
                write!(f, "this transaction exceeds the maximum standard weight")
            }
            MempoolError::NonStandard => write!(f, "this transaction is non-standard"),
            MempoolError::ExceedsScriptSigSize => {
                write!(
                    f,
                    "this transaction exceeds the maximum standard scriptSig size"
                )
            }
            MempoolError::MissingPrevoutContext => {
                write!(
                    f,
                    "missing prevout metadata required for relay policy checks"
                )
            }
            MempoolError::Consensus(error) => {
                write!(f, "the transaction failed consensus validation: {error}")
            }
        }
    }
}

impl Error for MempoolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MempoolError::Consensus(error) => Some(error),
            _ => None,
        }
    }
}
