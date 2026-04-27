// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(clippy::unwrap_used)]

use core::fmt::Debug;
use core::fmt::Display;
use core::fmt::Formatter;

use bitcoin::hash_types::Txid;
use bitcoin::hashes::sha256::Hash;
use bitcoin::ScriptBuf;
use bitcoin::Transaction;
use floresta_common::prelude::*;

use crate::merkle::MerkleProof;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "sqlite")]
pub fn new_repository(db_path: &str) -> Result<Box<dyn WalletRepository>, WalletRepositoryError> {
    let repo = sqlite::SqliteRepository::new(db_path)?;

    Ok(Box::new(repo))
}

// Represents a Bitcoin descriptor that can be used to derive addresses.
// A descriptor is associated with a wallet and can be marked as active for transaction generation and
// address derivation.
#[derive(Debug, Clone)]
pub struct DbDescriptor {
    // The wallet that owns this descriptor
    pub wallet: String,

    // Unique identifier for this descriptor within its wallet
    pub id: String,

    // The descriptor string defining how addresses are derived (e.g., "wpkh(...)")
    pub descriptor: String,

    // Optional human-readable label for this descriptor
    pub label: Option<String>,

    // Whether this descriptor is currently active for transaction generation and address derivation
    pub is_active: bool,

    // Whether this is a change address descriptor (used for change outputs)
    pub is_change: bool,
}

// Represents a Bitcoin transaction persisted in the database.
// Includes the transaction data along with confirmation information.
#[derive(Debug, Clone)]
pub struct DbTransaction {
    // The full transaction data
    pub tx: Transaction,

    // Block height at which the transaction was confirmed (None if unconfirmed)
    pub height: Option<u64>,

    // Merkle proof proving inclusion in a block (None if unconfirmed)
    pub merkle_block: Option<MerkleProof>,

    // The transaction ID (hash)
    pub hash: Txid,

    // Position of the transaction within its block (None if unconfirmed)
    pub position: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DbScriptBuffer {
    pub script: ScriptBuf,
    pub hash: Hash,
}

#[derive(Debug)]
pub enum WalletRepositoryError {
    // Error during database initialization or configuration
    SetupError(String),

    // Error when inserting data into the database
    InsertError(String),

    // Error when updating existing data in the database
    UpdateError(String),

    // Error when deleting data from the database
    DeleteError(String),

    // Error when a requested item was not found in the database
    NotFound(String),

    // Generic error for other database operations
    Other(String),
}

impl Display for WalletRepositoryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            WalletRepositoryError::SetupError(msg) => {
                write!(f, "Database setup error: {}", msg)
            }
            WalletRepositoryError::InsertError(msg) => {
                write!(f, "Failed to insert data: {}", msg)
            }
            WalletRepositoryError::UpdateError(msg) => {
                write!(f, "Failed to update data: {}", msg)
            }
            WalletRepositoryError::DeleteError(msg) => {
                write!(f, "Failed to delete data: {}", msg)
            }
            WalletRepositoryError::NotFound(msg) => {
                write!(f, "Data not found: {}", msg)
            }
            WalletRepositoryError::Other(msg) => {
                write!(f, "Database error: {}", msg)
            }
        }
    }
}

pub trait WalletRepository: Send + Sync {
    // Creates a new wallet with the given name and returns its ID
    fn create_wallet(&self, name: &str) -> Result<String, WalletRepositoryError>;

    // Returns a list of all wallet names stored in the database
    fn list_wallets(&self) -> Result<Vec<String>, WalletRepositoryError>;

    // Removes a wallet and all its associated data from the database
    fn delete_wallet(&self, name: &str) -> Result<(), WalletRepositoryError>;

    // Stores a new descriptor in the database or updates it if it already exists
    fn insert_or_update_descriptor(
        &self,
        descriptor: &DbDescriptor,
    ) -> Result<(), WalletRepositoryError>;

    // Retrieves a specific descriptor by ID, optionally filtered by wallet name
    fn get_descriptor(
        &self,
        id: &str,
        wallet: Option<&str>,
    ) -> Result<DbDescriptor, WalletRepositoryError>;

    // Checks whether a descriptor exists, supports flexible filtering:
    // - Both id and wallet: checks if descriptor with specific ID exists in specific wallet
    // - Only id: checks if descriptor with that ID exists in any wallet
    // - Only wallet: checks if wallet has any descriptors
    // - Neither: checks if any descriptor exists in the database
    fn exists_descriptor(
        &self,
        id: Option<&str>,
        wallet: Option<&str>,
    ) -> Result<bool, WalletRepositoryError>;

    // Loads all descriptors associated with a specific wallet
    fn load_wallet(&self, wallet: &str) -> Result<Vec<DbDescriptor>, WalletRepositoryError>;

    // Stores a new transaction in the database or updates it if it already exists
    fn insert_or_update_transaction(
        &self,
        transaction: &DbTransaction,
    ) -> Result<(), WalletRepositoryError>;

    // Retrieves a specific transaction by its transaction ID
    fn get_transaction(&self, txid: &Txid) -> Result<DbTransaction, WalletRepositoryError>;

    // Returns all transactions stored in the database
    fn list_transactions(&self) -> Result<Vec<DbTransaction>, WalletRepositoryError>;

    // Inserts or updates a script buffer in the database
    fn insert_or_update_script_buffer(
        &self,
        script_buffer: &DbScriptBuffer,
    ) -> Result<(), WalletRepositoryError>;

    // Retrieves a script buffer by its hash
    fn get_script_buffer(&self, hash: &Hash) -> Result<DbScriptBuffer, WalletRepositoryError>;

    // Lists all script buffers stored in the database
    fn list_script_buffers(&self) -> Result<Vec<DbScriptBuffer>, WalletRepositoryError>;

    // Deletes a script buffer by its hash
    fn delete_script_buffer(&self, hash: &Hash) -> Result<(), WalletRepositoryError>;
}
