// SPDX-License-Identifier: MIT OR Apache-2.0

use core::error::Error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;

#[derive(PartialEq, Debug)]
pub enum WatchOnlyError {
    WalletNotInitialized,
    TransactionNotFound,
    DatabaseError(String),
    DuplicateDescriptor(String),
    InvalidDescriptor(String),
    LockPoisoned(String),
}

impl Display for WatchOnlyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalletNotInitialized => {
                write!(f, "Wallet isn't initialized")
            }
            Self::TransactionNotFound => {
                write!(f, "Transaction not found")
            }
            Self::DatabaseError(e) => {
                write!(f, "Database error: {e}")
            }
            Self::DuplicateDescriptor(desc) => {
                write!(f, "Descriptor is already cached: {desc}")
            }
            Self::InvalidDescriptor(e) => {
                write!(f, "Invalid descriptor: {e}")
            }
            Self::LockPoisoned(e) => write!(f, "Lock poisoned: {e}"),
        }
    }
}

impl Error for WatchOnlyError {}
