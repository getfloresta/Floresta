// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(clippy::unwrap_used)]

use core::error::Error;
use core::fmt;
use std::collections::HashSet;

#[cfg(feature = "bdk-provider")]
use bdk_wallet::rusqlite::Connection;
use bitcoin::amount::Amount;
use bitcoin::Address;
use bitcoin::Block;
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Transaction;
use bitcoin::TxOut;
use bitcoin::Txid;

use crate::models::Balance;
use crate::models::GetBalanceParams;
use crate::models::LastProcessedBlock;
use crate::models::LocalOutput;

#[cfg(feature = "bdk-provider")]
pub mod bdk_provider;

// For now we only have one provider, so we can just return it directly.
// In the future, we may want to support multiple providers and select them based on configuration.
#[cfg(feature = "bdk-provider")]
pub fn new_provider(
    db_path: &str,
    network: Network,
    is_initialized: bool,
) -> Result<Box<dyn WalletProvider>, WalletProviderError> {
    let provider = bdk_provider::BdkWalletProvider::<Connection, bdk_provider::KeyId>::new(
        db_path,
        network,
        is_initialized,
    )?;

    Ok(Box::new(provider))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletProviderEvent {
    UpdateTransaction { tx: Transaction, output: TxOut },
    UnconfirmedTransactionInBlock { tx: Transaction },
    ConfirmedTransaction { tx: Transaction },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletProviderError {
    // Persistência
    PersistenceError(String),

    // Wallet Management
    WalletCreationError(String),

    WalletLoadError(String),

    WalletNotInitialized,

    // Keyring & Descriptors
    InvalidDescriptor(String),

    DescriptorAlreadyExists(String),

    MissingDescriptor,

    MismatchedDescriptor(String),

    WalletAlreadyExists(String),

    MissingWallet(String),

    // Block Processing
    BlockProcessingError(String),

    TransactionNotFoundInBlock(Txid),

    // Address Management
    NoAddressAvailable { keychain: String },

    InvalidKeychain(String),

    // Synchronization
    LockPoisoned(String),

    // Transactions
    TransactionNotFound(Txid),

    NetworkMismatch { expected: Network, found: Network },

    NetworkMissing,

    WalletError(String),

    AddressError(String),

    // Generic
    Other(String),
}

impl fmt::Display for WalletProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WalletProviderError::PersistenceError(e) => {
                write!(f, "Persistence error: {e}")
            }
            WalletProviderError::WalletCreationError(e) => {
                write!(f, "Failed to create wallet: {e}")
            }
            WalletProviderError::WalletLoadError(e) => {
                write!(f, "Failed to load wallet: {e}")
            }
            WalletProviderError::WalletNotInitialized => {
                write!(f, "Wallet not initialized")
            }
            WalletProviderError::InvalidDescriptor(e) => {
                write!(f, "Invalid descriptor: {e}")
            }
            WalletProviderError::DescriptorAlreadyExists(e) => {
                write!(f, "Descriptor already exists: {e}")
            }
            WalletProviderError::MissingDescriptor => {
                write!(f, "Missing descriptor")
            }
            WalletProviderError::MismatchedDescriptor(e) => {
                write!(f, "Mismatched descriptor: {e}")
            }
            WalletProviderError::WalletAlreadyExists(e) => {
                write!(f, "Wallet already exists: {e}")
            }
            WalletProviderError::MissingWallet(e) => {
                write!(f, "Missing wallet: {e}")
            }
            WalletProviderError::BlockProcessingError(e) => {
                write!(f, "Block processing error: {e}")
            }
            WalletProviderError::TransactionNotFoundInBlock(txid) => {
                write!(f, "Transaction {txid} not found in block")
            }
            WalletProviderError::NoAddressAvailable { keychain } => {
                write!(f, "No address available for keychain: {keychain}")
            }
            WalletProviderError::InvalidKeychain(e) => {
                write!(f, "Invalid keychain: {e}")
            }
            WalletProviderError::LockPoisoned(e) => {
                write!(f, "Lock poisoned: {e}")
            }
            WalletProviderError::TransactionNotFound(txid) => {
                write!(f, "Transaction {txid} not found")
            }
            WalletProviderError::NetworkMismatch { expected, found } => {
                write!(f, "Network mismatch: expected {expected} but found {found}")
            }
            WalletProviderError::NetworkMissing => {
                write!(f, "Network not specified")
            }
            WalletProviderError::WalletError(e) => {
                write!(f, "Wallet error: {e}")
            }
            WalletProviderError::AddressError(e) => {
                write!(f, "Address error: {e}")
            }
            WalletProviderError::Other(e) => {
                write!(f, "Error: {e}")
            }
        }
    }
}

impl Error for WalletProviderError {}

pub trait WalletProvider: Send + Sync {
    fn persist_descriptor(&mut self, id: &str, descriptor: &str)
        -> Result<(), WalletProviderError>;

    fn block_process(
        &self,
        block: &Block,
        height: u32,
    ) -> Result<Vec<WalletProviderEvent>, WalletProviderError>;

    fn get_transaction(&self, txid: &Txid) -> Result<Transaction, WalletProviderError>;

    fn get_transactions(&self) -> Result<Vec<Transaction>, WalletProviderError>;

    fn get_transaction_by_wallet(
        &self,
        ids: HashSet<String>,
        txid: &Txid,
    ) -> Result<Transaction, WalletProviderError>;

    fn get_transactions_by_wallet(
        &self,
        ids: HashSet<String>,
    ) -> Result<Vec<Transaction>, WalletProviderError>;

    /// Returns the total available balance.
    ///
    /// The available balance is what the wallet considers currently spendable,
    /// and is thus affected by options which limit spendability such as avoid_reuse.
    fn get_balance(
        &self,
        ids: HashSet<String>,
        params: GetBalanceParams,
    ) -> Result<Amount, WalletProviderError>;

    fn get_balances(&self, ids: HashSet<String>) -> Result<Balance, WalletProviderError>;

    fn create_transaction(
        &self,
        ids: HashSet<String>,
        address: &str,
    ) -> Result<(), WalletProviderError>;

    fn new_address(&self, id: &str) -> Result<Address, WalletProviderError>;

    fn sent_and_received(
        &self,
        ids: HashSet<String>,
        txid: &Txid,
    ) -> Result<(u64, u64), WalletProviderError>;

    fn process_mempool_transactions(
        &self,
        transactions: Vec<&Transaction>,
    ) -> Result<Vec<WalletProviderEvent>, WalletProviderError>;

    fn get_txo(
        &self,
        outpoint: &OutPoint,
        is_spent: Option<bool>,
    ) -> Result<Option<TxOut>, WalletProviderError>;

    fn get_local_output_by_script(
        &self,
        script_hash: ScriptBuf,
        is_spent: Option<bool>,
    ) -> Result<Vec<LocalOutput>, WalletProviderError>;

    fn list_script_buff(
        &self,
        ids: Option<HashSet<String>>,
    ) -> Result<Vec<ScriptBuf>, WalletProviderError>;

    fn get_last_processed_block(&self) -> Result<LastProcessedBlock, WalletProviderError>;

    fn get_descriptor(&self, id: &str) -> Result<String, WalletProviderError>;
}
