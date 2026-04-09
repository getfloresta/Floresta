// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(clippy::unwrap_used)]

use std::str::FromStr;
use std::sync::Mutex;

use bitcoin::consensus::deserialize;
use bitcoin::consensus::serialize;
use bitcoin::hash_types::Txid;
use bitcoin::hashes::sha256::Hash;
use refinery::embed_migrations;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::Result as SqliteResult;

use super::DbDescriptor;
use super::DbScriptBuffer;
use super::DbTransaction;
use super::WalletRepository;
use super::WalletRepositoryError;

embed_migrations!("migrations");

pub struct SqliteRepository {
    conn: Mutex<Connection>,
}

impl SqliteRepository {
    /// Creates a new SQLite persister, initializing database schema if needed
    pub fn new(db_path: &str) -> Result<Self, WalletRepositoryError> {
        let conn = Connection::open(db_path)
            .map_err(|e| WalletRepositoryError::SetupError(e.to_string()))?;

        Self::setup(conn)
    }

    /// In-memory SQLite for testing
    #[cfg(test)]
    pub fn in_memory() -> Result<Self, WalletRepositoryError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| WalletRepositoryError::SetupError(e.to_string()))?;

        Self::setup(conn)
    }

    fn setup(conn: Connection) -> Result<Self, WalletRepositoryError> {
        // Enable foreign key constraints
        conn.execute("PRAGMA foreign_keys = ON", [])
            .map_err(|e| WalletRepositoryError::SetupError(e.to_string()))?;

        let persister = SqliteRepository {
            conn: Mutex::new(conn),
        };
        persister.run_migrations()?;
        Ok(persister)
    }

    /// Acquires a lock on the database connection
    fn get_connection(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Connection>, WalletRepositoryError> {
        self.conn.lock().map_err(|e| {
            WalletRepositoryError::Other(format!("Failed to acquire database lock: {}", e))
        })
    }

    fn run_migrations(&self) -> Result<(), WalletRepositoryError> {
        let mut conn = self.get_connection()?;

        migrations::runner().run(&mut *conn).map_err(|e| {
            WalletRepositoryError::SetupError(format!("Failed to run migrations: {}", e))
        })?;

        Ok(())
    }
}

impl WalletRepository for SqliteRepository {
    fn create_wallet(&self, name: &str) -> Result<String, WalletRepositoryError> {
        let conn = self.get_connection()?;

        conn.execute("INSERT INTO wallets (name) VALUES (?1)", params![name])
            .map_err(|e| {
                WalletRepositoryError::InsertError(format!("Failed to create wallet: {}", e))
            })?;

        Ok(name.to_string())
    }

    fn list_wallets(&self) -> Result<Vec<String>, WalletRepositoryError> {
        let conn = self.get_connection()?;

        let mut stmt = conn
            .prepare("SELECT name FROM wallets")
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        let wallets = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        Ok(wallets)
    }

    fn insert_or_update_descriptor(
        &self,
        descriptor: &DbDescriptor,
    ) -> Result<(), WalletRepositoryError> {
        let conn = self.get_connection()?;

        conn.execute(
            "INSERT OR REPLACE INTO descriptors (wallet_id, id, descriptor, label, is_active, is_change)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &descriptor.wallet,
                &descriptor.id,
                &descriptor.descriptor,
                &descriptor.label,
                descriptor.is_active,
                descriptor.is_change
            ],
        )
        .map_err(|e| {
            WalletRepositoryError::InsertError(format!(
                "Failed to insert or update descriptor: {}",
                e
            ))
        })?;

        Ok(())
    }

    fn get_descriptor(
        &self,
        id: &str,
        wallet: Option<&str>,
    ) -> Result<DbDescriptor, WalletRepositoryError> {
        let conn = self.get_connection()?;

        let query = if let Some(wallet_name) = wallet {
            // Specific descriptor in specific wallet
            let mut stmt = conn
                .prepare(
                    "SELECT wallet_id, id, descriptor, label, is_active, is_change
                FROM descriptors WHERE id = ?1 AND wallet_id = ?2",
                )
                .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

            stmt.query_row(params![id, wallet_name], |row| {
                Ok(DbDescriptor {
                    wallet: row.get(0)?,
                    id: row.get(1)?,
                    descriptor: row.get(2)?,
                    label: row.get(3)?,
                    is_active: row.get(4)?,
                    is_change: row.get(5)?,
                })
            })
            .map_err(|e| {
                WalletRepositoryError::NotFound(format!(
                    "Descriptor {} not found in wallet {}: {}",
                    id, wallet_name, e
                ))
            })?
        } else {
            // First descriptor with this id across all wallets
            let mut stmt = conn
                .prepare(
                    "SELECT wallet_id, id, descriptor, label, is_active, is_change
                FROM descriptors WHERE id = ?1 LIMIT 1",
                )
                .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

            stmt.query_row(params![id], |row| {
                Ok(DbDescriptor {
                    wallet: row.get(0)?,
                    id: row.get(1)?,
                    descriptor: row.get(2)?,
                    label: row.get(3)?,
                    is_active: row.get(4)?,
                    is_change: row.get(5)?,
                })
            })
            .map_err(|e| {
                WalletRepositoryError::NotFound(format!("Descriptor {} not found: {}", id, e))
            })?
        };

        Ok(query)
    }

    fn load_wallet(&self, name: &str) -> Result<Vec<DbDescriptor>, WalletRepositoryError> {
        let conn = self.get_connection()?;

        let mut stmt = conn
            .prepare(
                "SELECT wallet_id, id, descriptor, label, is_active, is_change
                 FROM descriptors WHERE wallet_id = ?1",
            )
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        let descriptors = stmt
            .query_map(params![name], |row| {
                Ok(DbDescriptor {
                    wallet: row.get(0)?,
                    id: row.get(1)?,
                    descriptor: row.get(2)?,
                    label: row.get(3)?,
                    is_active: row.get(4)?,
                    is_change: row.get(5)?,
                })
            })
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        Ok(descriptors)
    }

    fn exists_descriptor(
        &self,
        id: Option<&str>,
        wallet: Option<&str>,
    ) -> Result<bool, WalletRepositoryError> {
        let conn = self.get_connection()?;

        match (id, wallet) {
            // Both id and wallet provided: check for specific descriptor in specific wallet
            (Some(desc_id), Some(wallet_name)) => {
                let mut stmt = conn
                    .prepare("SELECT COUNT(*) FROM descriptors WHERE id = ?1 AND wallet_id = ?2")
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                let count: i64 = stmt
                    .query_row(params![desc_id, wallet_name], |row| row.get(0))
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                Ok(count > 0)
            }
            // Only id provided: check if descriptor exists with that id across all wallets
            (Some(desc_id), None) => {
                let mut stmt = conn
                    .prepare("SELECT COUNT(*) FROM descriptors WHERE id = ?1")
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                let count: i64 = stmt
                    .query_row(params![desc_id], |row| row.get(0))
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                Ok(count > 0)
            }
            // Only wallet provided: check if wallet has any descriptors
            (None, Some(wallet_name)) => {
                let mut stmt = conn
                    .prepare("SELECT COUNT(*) FROM descriptors WHERE wallet_id = ?1")
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                let count: i64 = stmt
                    .query_row(params![wallet_name], |row| row.get(0))
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                Ok(count > 0)
            }
            // Neither provided: check if any descriptor exists in database
            (None, None) => {
                let mut stmt = conn
                    .prepare("SELECT COUNT(*) FROM descriptors")
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                let count: i64 = stmt
                    .query_row([], |row| row.get(0))
                    .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

                Ok(count > 0)
            }
        }
    }

    fn insert_or_update_transaction(
        &self,
        transaction: &DbTransaction,
    ) -> Result<(), WalletRepositoryError> {
        let conn = self.get_connection()?;

        let tx_bytes = serialize(&transaction.tx);
        let hash_bytes = transaction.hash.to_string();
        let merkle_bytes = transaction
            .merkle_block
            .as_ref()
            .and_then(|m| serde_json::to_vec(m).ok());

        conn.execute(
            "INSERT OR REPLACE INTO transactions (hash, tx, height, merkle_block, position)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                hash_bytes,
                tx_bytes,
                transaction.height,
                merkle_bytes,
                transaction.position
            ],
        )
        .map_err(|e| {
            WalletRepositoryError::InsertError(format!(
                "Failed to insert or update transaction: {}",
                e
            ))
        })?;

        Ok(())
    }

    fn get_transaction(&self, txid: &Txid) -> Result<DbTransaction, WalletRepositoryError> {
        let conn = self.get_connection()?;

        let hash_bytes = txid.to_string();
        let mut stmt = conn
            .prepare("SELECT tx, height, merkle_block, position FROM transactions WHERE hash = ?1")
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        let transaction = stmt
            .query_row(params![hash_bytes], |row| {
                let tx_bytes: Vec<u8> = row.get(0)?;
                let height: Option<u64> = row.get(1)?;
                let merkle_bytes: Option<Vec<u8>> = row.get(2)?;
                let position: Option<u64> = row.get(3)?;

                let tx = deserialize(&tx_bytes).map_err(|_| rusqlite::Error::InvalidQuery)?;

                let merkle_block = merkle_bytes.and_then(|b| serde_json::from_slice(&b).ok());

                Ok(DbTransaction {
                    tx,
                    height,
                    merkle_block,
                    hash: *txid,
                    position,
                })
            })
            .map_err(|e| {
                WalletRepositoryError::NotFound(format!("Transaction {} not found: {}", txid, e))
            })?;

        Ok(transaction)
    }

    fn list_transactions(&self) -> Result<Vec<DbTransaction>, WalletRepositoryError> {
        let conn = self.get_connection()?;

        let mut stmt = conn
            .prepare("SELECT hash, tx, height, merkle_block, position FROM transactions")
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        let transactions = stmt
            .query_map([], |row| {
                let hash_string: String = row.get(0)?;
                let tx_bytes: Vec<u8> = row.get(1)?;
                let height: Option<u64> = row.get(2)?;
                let merkle_bytes: Option<Vec<u8>> = row.get(3)?;
                let position: Option<u64> = row.get(4)?;

                let tx = deserialize(&tx_bytes).map_err(|_| rusqlite::Error::InvalidQuery)?;

                let hash =
                    Txid::from_str(&hash_string).map_err(|_| rusqlite::Error::InvalidQuery)?;

                let merkle_block = merkle_bytes.and_then(|b| serde_json::from_slice(&b).ok());

                Ok(DbTransaction {
                    tx,
                    height,
                    merkle_block,
                    hash,
                    position,
                })
            })
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        Ok(transactions)
    }

    fn delete_wallet(&self, name: &str) -> Result<(), WalletRepositoryError> {
        let conn = self.get_connection()?;

        let rows_affected = conn
            .execute("DELETE FROM wallets WHERE name = ?1", params![name])
            .map_err(|e| {
                WalletRepositoryError::DeleteError(format!("Failed to delete wallet: {}", e))
            })?;

        if rows_affected == 0 {
            return Err(WalletRepositoryError::NotFound(format!(
                "Wallet {} not found",
                name
            )));
        }

        Ok(())
    }

    fn insert_or_update_script_buffer(
        &self,
        script_buffer: &DbScriptBuffer,
    ) -> Result<(), WalletRepositoryError> {
        let conn = self.get_connection()?;

        let script_bytes = serialize(&script_buffer.script);
        let hash_string = script_buffer.hash.to_string();

        conn.execute(
            "INSERT OR REPLACE INTO script_buffers (hash, script) VALUES (?1, ?2)",
            params![hash_string, script_bytes],
        )
        .map_err(|e| {
            WalletRepositoryError::InsertError(format!(
                "Failed to insert or update script buffer: {}",
                e
            ))
        })?;

        Ok(())
    }

    fn get_script_buffer(&self, hash: &Hash) -> Result<DbScriptBuffer, WalletRepositoryError> {
        let conn = self.get_connection()?;

        let hash_string = hash.to_string();
        let mut stmt = conn
            .prepare("SELECT script FROM script_buffers WHERE hash = ?1")
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        let script_buffer = stmt
            .query_row(params![hash_string], |row| {
                let script_bytes: Vec<u8> = row.get(0)?;
                let script =
                    deserialize(&script_bytes).map_err(|_| rusqlite::Error::InvalidQuery)?;

                Ok(DbScriptBuffer {
                    script,
                    hash: *hash,
                })
            })
            .map_err(|e| {
                WalletRepositoryError::NotFound(format!("Script buffer {} not found: {}", hash, e))
            })?;

        Ok(script_buffer)
    }

    fn list_script_buffers(&self) -> Result<Vec<DbScriptBuffer>, WalletRepositoryError> {
        let conn = self.get_connection()?;

        let mut stmt = conn
            .prepare("SELECT hash, script FROM script_buffers")
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        let script_buffers = stmt
            .query_map([], |row| {
                let hash_string: String = row.get(0)?;
                let script_bytes: Vec<u8> = row.get(1)?;

                let script =
                    deserialize(&script_bytes).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let hash =
                    Hash::from_str(&hash_string).map_err(|_| rusqlite::Error::InvalidQuery)?;

                Ok(DbScriptBuffer { script, hash })
            })
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| WalletRepositoryError::Other(e.to_string()))?;

        Ok(script_buffers)
    }

    fn delete_script_buffer(&self, hash: &Hash) -> Result<(), WalletRepositoryError> {
        let conn = self.get_connection()?;

        let hash_string = hash.to_string();
        let rows_affected = conn
            .execute(
                "DELETE FROM script_buffers WHERE hash = ?1",
                params![hash_string],
            )
            .map_err(|e| {
                WalletRepositoryError::DeleteError(format!("Failed to delete script buffer: {}", e))
            })?;

        if rows_affected == 0 {
            return Err(WalletRepositoryError::NotFound(format!(
                "Script buffer {} not found",
                hash
            )));
        }

        Ok(())
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {

    use bitcoin::hashes::Hash as HashTrait;
    use bitcoin::script::Builder;
    use bitcoin::ScriptBuf;

    use super::*;
    use crate::utils::create_test_transaction;
    use crate::utils::create_test_transaction_with_seed;

    fn create_test_repo() -> SqliteRepository {
        SqliteRepository::in_memory().unwrap()
    }

    fn create_descriptor_default(wallet: &str, id: u64) -> DbDescriptor {
        create_descriptor_info(wallet, id, false, true, false)
    }

    fn create_descriptor_info(
        wallet: &str,
        id: u64,
        label: bool,
        is_active: bool,
        is_change: bool,
    ) -> DbDescriptor {
        DbDescriptor {
            wallet: wallet.to_string(),
            id: id.to_string(),
            descriptor: id.to_string(),
            label: if label {
                Some(format!("Descriptor {}", id))
            } else {
                None
            },
            is_active,
            is_change,
        }
    }

    fn setup_wallet_one_descriptor(wallet: &str) -> (SqliteRepository, DbDescriptor) {
        let persister = create_test_repo();

        let descriptor = setup_wallet(wallet, &persister, 1).first().unwrap().clone();

        (persister, descriptor)
    }

    fn setup_wallet(
        wallet: &str,
        persister: &SqliteRepository,
        quantity: u64,
    ) -> Vec<DbDescriptor> {
        persister.create_wallet(wallet).unwrap();

        let mut descriptors = Vec::new();
        for i in 0..quantity {
            let label = i % 2 == 0;
            let is_active = i % 3 == 0;
            let is_change = i % 5 == 0;

            let descriptor = create_descriptor_info(wallet, i, label, is_active, is_change);
            persister.insert_or_update_descriptor(&descriptor).unwrap();
            descriptors.push(descriptor);
        }
        descriptors
    }

    fn check_descriptor_equality(d1: &DbDescriptor, d2: &DbDescriptor) {
        assert_eq!(d1.id, d2.id);
        assert_eq!(d1.wallet, d2.wallet);
        assert_eq!(d1.descriptor, d2.descriptor);
        assert_eq!(d1.label, d2.label);
        assert_eq!(d1.is_active, d2.is_active);
        assert_eq!(d1.is_change, d2.is_change);
    }

    fn check_transaction_equality(t1: &DbTransaction, t2: &DbTransaction) {
        assert_eq!(t1.hash, t2.hash);
        assert_eq!(t1.height, t2.height);
        assert_eq!(t1.position, t2.position);
        assert_eq!(t1.merkle_block, t2.merkle_block);
        assert_eq!(t1.tx, t2.tx);
    }

    #[test]
    fn test_create_and_list_wallets() {
        let persister = create_test_repo();
        let wallet = "my_wallet";

        let wallet_id = persister.create_wallet(wallet).unwrap();
        assert_eq!(wallet_id, wallet);

        let wallets = persister.list_wallets().unwrap();
        assert!(wallets.contains(&wallet.to_string()));
    }

    #[test]
    fn test_delete_wallet() {
        let persister = create_test_repo();
        let wallet = "wallet_to_delete";
        persister.create_wallet(wallet).unwrap();

        let wallets = persister.list_wallets().unwrap();
        assert!(wallets.contains(&wallet.to_string()));

        persister.delete_wallet(wallet).unwrap();

        let wallets = persister.list_wallets().unwrap();
        assert!(!wallets.contains(&wallet.to_string()));
    }

    #[test]
    fn test_insert_descriptor() {
        let (persister, descriptor) = setup_wallet_one_descriptor("wallet1");

        let loaded = persister
            .get_descriptor(&descriptor.id, Some(&descriptor.wallet))
            .unwrap();

        check_descriptor_equality(&loaded, &descriptor);
    }

    #[test]
    fn test_descriptor_operations() {
        let wallet = "wallet1";
        let (persister, descriptor) = setup_wallet_one_descriptor(wallet);

        let mut updated = descriptor.clone();
        updated.label = Some("Updated Label".to_string());
        updated.is_active = false;
        persister.insert_or_update_descriptor(&updated).unwrap();

        let reloaded = persister
            .get_descriptor(&updated.id, Some(&updated.wallet))
            .unwrap();
        check_descriptor_equality(&reloaded, &updated);
    }

    #[test]
    fn test_load_multiple_descriptors() {
        let persister = create_test_repo();
        let wallet = "wallet1";
        let descriptors1 = setup_wallet(wallet, &persister, 5);

        let wallet2 = "wallet2";
        let descriptors2 = setup_wallet(wallet2, &persister, 3);

        let loaded1 = persister.load_wallet(wallet).unwrap();
        for desc in &descriptors1 {
            let loaded_desc = loaded1
                .iter()
                .find(|d| d.id == desc.id)
                .expect("Descriptor not found in loaded wallet");
            check_descriptor_equality(loaded_desc, desc);
        }

        let loaded2 = persister.load_wallet(wallet2).unwrap();
        for desc in &descriptors2 {
            let loaded_desc = loaded2
                .iter()
                .find(|d| d.id == desc.id)
                .expect("Descriptor not found in loaded wallet");
            check_descriptor_equality(loaded_desc, desc);
        }
    }

    #[test]
    fn test_insert_and_get_transaction() {
        let persister = create_test_repo();
        let tx = create_test_transaction();
        let txid = tx.compute_txid();

        let transaction = DbTransaction {
            tx: tx.clone(),
            height: Some(100),
            merkle_block: None,
            hash: txid,
            position: Some(0),
        };

        persister
            .insert_or_update_transaction(&transaction)
            .unwrap();

        let loaded = persister.get_transaction(&txid).unwrap();
        check_transaction_equality(&loaded, &transaction);
    }

    #[test]
    fn test_update_transaction() {
        let persister = create_test_repo();
        let tx = create_test_transaction();
        let txid = tx.compute_txid();

        let mut transaction = DbTransaction {
            tx: tx.clone(),
            height: Some(100),
            merkle_block: None,
            hash: txid,
            position: Some(0),
        };

        persister
            .insert_or_update_transaction(&transaction)
            .unwrap();

        // Update height and position
        transaction.height = Some(101);
        transaction.position = Some(5);
        persister
            .insert_or_update_transaction(&transaction)
            .unwrap();

        let loaded = persister.get_transaction(&txid).unwrap();

        check_transaction_equality(&loaded, &transaction);
    }

    #[test]
    fn test_list_transactions() {
        let persister = create_test_repo();

        let tx1 = create_test_transaction();
        let tx2 = create_test_transaction_with_seed(21);

        let txid1 = tx1.compute_txid();
        let txid2 = tx2.compute_txid();

        let transaction1 = DbTransaction {
            tx: tx1,
            height: Some(100),
            merkle_block: None,
            hash: txid1,
            position: Some(0),
        };

        let transaction2 = DbTransaction {
            tx: tx2,
            height: Some(101),
            merkle_block: None,
            hash: txid2,
            position: Some(1),
        };

        persister
            .insert_or_update_transaction(&transaction1)
            .unwrap();
        persister
            .insert_or_update_transaction(&transaction2)
            .unwrap();

        let loaded = persister.list_transactions().unwrap();
        assert_eq!(loaded.len(), 2);

        for tx in [transaction1, transaction2] {
            let loaded_tx = loaded
                .iter()
                .find(|t| t.hash == tx.hash)
                .expect("Transaction not found in list");
            check_transaction_equality(loaded_tx, &tx);
        }
    }

    #[test]
    fn test_transaction_not_found() {
        let persister = create_test_repo();
        let tx = create_test_transaction();
        let txid = tx.compute_txid();

        let result = persister.get_transaction(&txid);
        assert!(result.is_err());
    }

    #[test]
    fn test_exists_descriptor_no_params() {
        let persister = create_test_repo();
        let wallet = "wallet1";
        persister.create_wallet(wallet).unwrap();

        // Should return false when no descriptors exist
        let exists = persister.exists_descriptor(None, None).unwrap();
        assert!(!exists);

        // Add a descriptor
        let descriptor = create_descriptor_default(wallet, 1);
        persister.insert_or_update_descriptor(&descriptor).unwrap();

        // Should return true when descriptor exists
        let exists = persister.exists_descriptor(None, None).unwrap();
        assert!(exists);
    }

    #[test]
    fn test_exists_descriptor_by_id_only() {
        let persister = create_test_repo();
        let wallet = "wallet1";
        persister.create_wallet(wallet).unwrap();

        let id = 2;

        // Should return false when descriptor doesn't exist
        let exists = persister
            .exists_descriptor(Some(&id.to_string()), None)
            .unwrap();
        assert!(!exists);

        // Add descriptor
        let descriptor = create_descriptor_default(wallet, id);
        persister.insert_or_update_descriptor(&descriptor).unwrap();

        // Should return true when descriptor with that id exists
        let exists = persister
            .exists_descriptor(Some(&id.to_string()), None)
            .unwrap();
        assert!(exists);

        // Should return false for non-existent id
        let exists = persister
            .exists_descriptor(Some(&(id + 1).to_string()), None)
            .unwrap();
        assert!(!exists);
    }

    #[test]
    fn test_exists_descriptor_by_wallet_only() {
        let persister = create_test_repo();
        let wallet1 = "wallet1";
        persister.create_wallet(wallet1).unwrap();

        let wallet2 = "wallet2";
        persister.create_wallet(wallet2).unwrap();

        // Should return false when wallet has no descriptors
        let exists = persister.exists_descriptor(None, Some(wallet1)).unwrap();
        assert!(!exists);

        // Add descriptor to wallet1
        let descriptor = create_descriptor_default(wallet1, 1);
        persister.insert_or_update_descriptor(&descriptor).unwrap();

        // Should return true for wallet1
        let exists = persister.exists_descriptor(None, Some(wallet1)).unwrap();
        assert!(exists);

        // Should still return false for wallet2
        let exists = persister.exists_descriptor(None, Some(wallet2)).unwrap();
        assert!(!exists);
    }

    #[test]
    fn test_exists_descriptor_by_id_and_wallet() {
        let persister = create_test_repo();
        let wallet1 = "wallet1";
        persister.create_wallet(wallet1).unwrap();
        let wallet2 = "wallet2";
        persister.create_wallet(wallet2).unwrap();

        let id1 = 1;
        let id2 = 2;

        // Add descriptor to wallet1
        let descriptor1 = create_descriptor_default(wallet1, id1);
        persister.insert_or_update_descriptor(&descriptor1).unwrap();

        // Add different descriptor to wallet2
        let descriptor2 = create_descriptor_default(wallet2, id2);
        persister.insert_or_update_descriptor(&descriptor2).unwrap();

        // Should return true for desc1 in wallet1
        let exists = persister
            .exists_descriptor(Some(&id1.to_string()), Some(wallet1))
            .unwrap();
        assert!(exists);

        // Should return false for desc1 in wallet2
        let exists = persister
            .exists_descriptor(Some(&id1.to_string()), Some(wallet2))
            .unwrap();
        assert!(!exists);

        // Should return false for desc2 in wallet1
        let exists = persister
            .exists_descriptor(Some(&id2.to_string()), Some(wallet1))
            .unwrap();
        assert!(!exists);

        // Should return true for desc2 in wallet2
        let exists = persister
            .exists_descriptor(Some(&id2.to_string()), Some(wallet2))
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn test_exists_descriptor_across_wallets() {
        let persister = create_test_repo();
        let wallet1 = "wallet1";
        persister.create_wallet(wallet1).unwrap();
        let wallet2 = "wallet2";
        persister.create_wallet(wallet2).unwrap();

        let id = 1;

        // Add same id to different wallets (should be possible as primary key includes wallet)
        let descriptor1 = create_descriptor_default(wallet1, id);
        let descriptor2 = create_descriptor_default(wallet2, id);

        persister.insert_or_update_descriptor(&descriptor1).unwrap();
        persister.insert_or_update_descriptor(&descriptor2).unwrap();

        // When querying by id only, should find it
        let exists = persister
            .exists_descriptor(Some(&id.to_string()), None)
            .unwrap();
        assert!(exists);

        // Should find in both wallets specifically
        let exists = persister
            .exists_descriptor(Some(&id.to_string()), Some(wallet1))
            .unwrap();
        assert!(exists);

        let exists = persister
            .exists_descriptor(Some(&id.to_string()), Some(wallet2))
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn test_descriptor_update_across_wallets() {
        let persister = create_test_repo();
        let wallet1 = "wallet1";
        persister.create_wallet(wallet1).unwrap();
        let wallet2 = "wallet2";
        persister.create_wallet(wallet2).unwrap();

        let id1 = 1;
        let id2 = 2;
        let descriptor1 = create_descriptor_default(wallet1, id1);
        persister.insert_or_update_descriptor(&descriptor1).unwrap();
        let descriptor2 = create_descriptor_default(wallet2, id2);
        persister.insert_or_update_descriptor(&descriptor2).unwrap();

        let mut descriptors = vec![descriptor1, descriptor2];

        // Update each descriptor and verify changes are saved correctly without affecting the other wallet's descriptor
        for d in &mut descriptors {
            let loaded = persister
                .get_descriptor(&d.id, Some(&d.wallet))
                .expect("Descriptor should exist");

            check_descriptor_equality(d, &loaded);

            d.is_active = !d.is_active;
            d.is_change = !d.is_change;
            d.label = d
                .label
                .as_ref()
                .map(|l| format!("{} Updated", l))
                .or_else(|| Some("Updated Label".to_string()));

            persister.insert_or_update_descriptor(d).unwrap();
            let updated = persister
                .get_descriptor(&d.id, Some(&d.wallet))
                .expect("Descriptor should exist after update");

            check_descriptor_equality(d, &updated);
        }
    }

    #[test]
    fn test_insert_and_get_script_buffer() {
        let persister = create_test_repo();

        let script = ScriptBuf::new();
        let hash = Hash::hash(b"test script");

        let script_buffer = DbScriptBuffer {
            script: script.clone(),
            hash,
        };

        persister
            .insert_or_update_script_buffer(&script_buffer)
            .unwrap();

        let loaded = persister.get_script_buffer(&hash).unwrap();
        assert_eq!(loaded.script, script);
        assert_eq!(loaded.hash, hash);
    }

    #[test]
    fn test_script_buffer_not_found() {
        let persister = create_test_repo();
        let hash = Hash::hash(b"non-existent");

        let result = persister.get_script_buffer(&hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_script_buffer() {
        let persister = create_test_repo();

        let hash = Hash::hash(b"test");
        let script1 = ScriptBuf::new();

        let script_buffer = DbScriptBuffer {
            script: script1,
            hash,
        };

        persister
            .insert_or_update_script_buffer(&script_buffer)
            .unwrap();

        // Update with new script
        let mut updated = script_buffer;
        updated.script = Builder::new().push_int(1).into_script();

        persister.insert_or_update_script_buffer(&updated).unwrap();

        let loaded = persister.get_script_buffer(&hash).unwrap();
        assert_eq!(loaded.script, updated.script);
    }

    #[test]
    fn test_list_script_buffers() {
        let persister = create_test_repo();

        let hash1 = Hash::hash(b"script1");
        let hash2 = Hash::hash(b"script2");
        let hash3 = Hash::hash(b"script3");

        let script1 = ScriptBuf::new();
        let script2 = Builder::new().push_int(1).into_script();
        let script3 = Builder::new().push_int(2).into_script();

        persister
            .insert_or_update_script_buffer(&DbScriptBuffer {
                script: script1,
                hash: hash1,
            })
            .unwrap();
        persister
            .insert_or_update_script_buffer(&DbScriptBuffer {
                script: script2,
                hash: hash2,
            })
            .unwrap();
        persister
            .insert_or_update_script_buffer(&DbScriptBuffer {
                script: script3,
                hash: hash3,
            })
            .unwrap();

        let loaded = persister.list_script_buffers().unwrap();
        assert_eq!(loaded.len(), 3);
        assert!(loaded.iter().any(|sb| sb.hash == hash1));
        assert!(loaded.iter().any(|sb| sb.hash == hash2));
        assert!(loaded.iter().any(|sb| sb.hash == hash3));
    }

    #[test]
    fn test_list_script_buffers_empty() {
        let persister = create_test_repo();

        let loaded = persister.list_script_buffers().unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_delete_script_buffer() {
        let persister = create_test_repo();

        let hash = Hash::hash(b"to delete");
        let script_buffer = DbScriptBuffer {
            script: ScriptBuf::new(),
            hash,
        };

        persister
            .insert_or_update_script_buffer(&script_buffer)
            .unwrap();

        // Verify it exists
        let loaded = persister.get_script_buffer(&hash).unwrap();
        assert_eq!(loaded.hash, hash);

        // Delete it
        persister.delete_script_buffer(&hash).unwrap();

        // Verify it's gone
        let result = persister.get_script_buffer(&hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_non_existent_script_buffer() {
        let persister = create_test_repo();
        let hash = Hash::hash(b"non-existent");

        let result = persister.delete_script_buffer(&hash);
        assert!(result.is_err());
    }
}
