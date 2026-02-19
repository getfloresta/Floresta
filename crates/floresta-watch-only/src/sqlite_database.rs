//! A SQLite-based database for the watch-only wallet.
//!
//! This module provides both a persistent and a ephemeral storage implementation using SQLite via rusqlite.
//!
//! It implements the [`AddressCacheDatabase`] trait to store addresses, transactions,
//! descriptors, and wallet statistics.

use std::sync::Mutex;
use std::sync::PoisonError;

use bitcoin::consensus;
use bitcoin::consensus::deserialize;
use bitcoin::consensus::encode;
use bitcoin::hashes::Hash as HashTrait;
use bitcoin::OutPoint;
use bitcoin::Txid;
use floresta_common::impl_error_from;
use floresta_common::prelude;
use floresta_common::prelude::*;
use rusqlite::params;
use rusqlite::types::Type;
use rusqlite::Connection;
use rusqlite::Error as SqliteError;

use super::merkle::MerkleProof;
use super::AddressCacheDatabase;
use super::CachedAddress;
use super::CachedTransaction;
use super::Stats;

/// Addresses table keyed by script hash, storing balance, script, associated transactions and UTXOs.
const CREATE_ADDRESSES: &str = "
    CREATE TABLE IF NOT EXISTS addresses (
        script_hash BLOB PRIMARY KEY,
        balance INTEGER NOT NULL,
        script BLOB NOT NULL,
        transactions BLOB NOT NULL,
        utxos BLOB NOT NULL
    );";

/// Transactions table keyed by txid, storing the raw transaction, block height, merkle proof and position.
const CREATE_TRANSACTIONS: &str = "
    CREATE TABLE IF NOT EXISTS transactions (
        txid BLOB PRIMARY KEY,
        tx BLOB NOT NULL,
        height INTEGER NOT NULL,
        merkle_block BLOB,
        position INTEGER NOT NULL
    );";

/// Descriptors table storing unique output descriptor strings.
const CREATE_DESCRIPTORS: &str = "
    CREATE TABLE IF NOT EXISTS descriptors (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        descriptor TEXT NOT NULL UNIQUE
    );";

/// Singleton stats table (enforced by `CHECK (id = 0)`) holding aggregate wallet statistics.
const CREATE_STATS: &str = "
    CREATE TABLE IF NOT EXISTS stats (
        id INTEGER PRIMARY KEY CHECK (id = 0),
        address_count INTEGER NOT NULL,
        transaction_count INTEGER NOT NULL,
        utxo_count INTEGER NOT NULL,
        cache_height INTEGER NOT NULL,
        txo_count INTEGER NOT NULL,
        balance INTEGER NOT NULL,
        derivation_index INTEGER NOT NULL
    );";

/// Metadata sql table that holds some wallet extra info.
const CREATE_METADATA: &str = "
    CREATE TABLE IF NOT EXISTS metadata (
        key TEXT PRIMARY KEY,
        value BLOB NOT NULL
    );";

/// Holds all table definitions inside an array.
const SQL_TABLES: &[&str] = &[
    CREATE_ADDRESSES,
    CREATE_TRANSACTIONS,
    CREATE_DESCRIPTORS,
    CREATE_STATS,
    CREATE_METADATA,
];

/// A SQLite-backed database for the watch-only wallet.
pub struct SqliteDatabase {
    conn: Mutex<Connection>,
}

impl SqliteDatabase {
    /// Creates a new SQLite database at the given path.
    ///
    /// This will create the database file if it doesn't exist and initialize
    /// all required tables.
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(format!("{path}/wallet.sqlite3"))?;
        let conn = Mutex::new(conn);
        let db = SqliteDatabase { conn };
        db.init_tables()?;
        Ok(db)
    }

    /// Creates a new in-memory SQLite database.
    ///
    /// Useful for testing purposes since all data is kept in memory and
    /// lost after dropping the instance.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_ephemeral() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let conn = Mutex::new(conn);
        let db = SqliteDatabase { conn };
        db.init_tables()?;
        Ok(db)
    }

    /// Initialize all required tables.
    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock()?;
        for table in SQL_TABLES {
            conn.execute_batch(table)?;
        }
        Ok(())
    }

    pub(super) fn to_conv_error(index: usize) -> SqliteError {
        SqliteError::FromSqlConversionFailure(
            index,
            Type::Blob,
            Box::new(SqliteDatabaseError::CorruptedData),
        )
    }

    pub(super) fn hash_from_slice<T: HashTrait>(bytes: &[u8], index: usize) -> rusqlite::Result<T> {
        T::from_slice(bytes).map_err(|_| Self::to_conv_error(index))
    }
}

#[derive(Debug)]
pub enum SqliteDatabaseError {
    /// Error from rusqlite
    Sqlite(rusqlite::Error),

    /// Error deserializing Bitcoin consensus data
    ConsensusEncoding(encode::Error),

    /// Wallet has not been initialized
    WalletNotInitialized,

    /// Transaction was not found in the database
    TransactionNotFound,

    /// Mutex was poisoned (a thread panicked while holding the lock)
    MutexPoisoned,

    /// We tried to parse a `Vec<u8>` into a type and it failed.
    CorruptedData,
}

impl_error_from!(SqliteDatabaseError, rusqlite::Error, Sqlite);
impl_error_from!(SqliteDatabaseError, encode::Error, ConsensusEncoding);

impl Display for SqliteDatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SqliteDatabaseError::Sqlite(e) => write!(f, "SQLite error: {e}"),
            SqliteDatabaseError::ConsensusEncoding(e) => {
                write!(f, "Consensus deserialization error: {e}")
            }
            SqliteDatabaseError::WalletNotInitialized => write!(f, "Wallet not initialized"),
            SqliteDatabaseError::TransactionNotFound => write!(f, "Transaction not found"),
            SqliteDatabaseError::MutexPoisoned => write!(f, "Mutex poisoned"),
            SqliteDatabaseError::CorruptedData => write!(f, "Corrupted Data"),
        }
    }
}

impl<T> From<PoisonError<T>> for SqliteDatabaseError {
    fn from(_: PoisonError<T>) -> Self {
        SqliteDatabaseError::MutexPoisoned
    }
}

impl Error for SqliteDatabaseError {}

type Result<T> = prelude::Result<T, SqliteDatabaseError>;

impl AddressCacheDatabase for SqliteDatabase {
    type Error = SqliteDatabaseError;

    fn load(&self) -> Result<Vec<CachedAddress>> {
        let conn = self.conn.lock()?;
        let mut stmt = conn
            .prepare("SELECT script_hash, balance, script, transactions, utxos FROM addresses")?;

        let rows = stmt.query_map([], |row| {
            let script_hash: Vec<u8> = row.get(0)?;
            let script_hash = Self::hash_from_slice(&script_hash, 0)?;

            let balance: i64 = row.get(1)?;
            let script: Vec<u8> = row.get(2)?;
            let transactions: Vec<u8> = row.get(3)?;
            let transactions: rusqlite::Result<Vec<Txid>, SqliteError> = transactions
                .chunks_exact(32)
                .map(|chunk| Self::hash_from_slice(chunk, 3))
                .collect();

            let transactions = transactions?;

            let utxos: Vec<u8> = row.get(4)?;
            let utxos: rusqlite::Result<Vec<OutPoint>, SqliteError> = utxos
                .chunks_exact(36)
                .map(|chunk| {
                    let txid: Txid = Self::hash_from_slice(&chunk[..32], 4)?;
                    let vout = u32::from_le_bytes(
                        chunk[32..36]
                            .try_into()
                            .map_err(|_| Self::to_conv_error(4))?,
                    );
                    Ok(OutPoint { txid, vout })
                })
                .collect();

            let utxos = utxos?;
            let script = deserialize(&script).map_err(|e| -> SqliteError {
                SqliteError::FromSqlConversionFailure(2, Type::Blob, Box::new(e))
            })?;

            Ok(CachedAddress {
                script_hash,
                balance: balance as u64,
                script,
                transactions,
                utxos,
            })
        })?;

        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn save(&self, address: &CachedAddress) -> Result<()> {
        let script_hash = address.script_hash.as_byte_array().to_vec();
        let balance = address.balance as i64;
        let script = consensus::serialize(&address.script);
        let mut tx_buf = Vec::with_capacity(address.transactions.len() * 32);

        for txid in &address.transactions {
            tx_buf.extend_from_slice(txid.as_byte_array());
        }

        let transactions = tx_buf;

        let mut utxo_buf = Vec::with_capacity(address.utxos.len() * 36);
        for op in &address.utxos {
            utxo_buf.extend_from_slice(op.txid.as_byte_array());

            utxo_buf.extend_from_slice(&op.vout.to_le_bytes());
        }

        let utxos = utxo_buf;

        self.conn.lock()?.execute(
            "INSERT OR REPLACE INTO addresses (script_hash, balance, script, transactions, utxos)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            params![script_hash, balance, script, transactions, utxos],
        )?;

        Ok(())
    }

    fn update(&self, address: &CachedAddress) -> Result<()> {
        self.save(address)
    }

    fn get_cache_height(&self) -> Result<u32> {
        let conn = self.conn.lock()?;
        let result: std::result::Result<i64, _> = conn.query_row(
            "SELECT value FROM metadata WHERE key = 'height'",
            [],
            |row| row.get(0),
        );

        match result {
            Ok(height) => Ok(height as u32),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(SqliteDatabaseError::WalletNotInitialized)
            }
            Err(e) => Err(SqliteDatabaseError::Sqlite(e)),
        }
    }

    fn set_cache_height(&self, height: u32) -> Result<()> {
        self.conn.lock()?.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('height', ?1)",
            params![height as i64],
        )?;
        Ok(())
    }

    fn desc_save(&self, descriptor: &str) -> Result<()> {
        self.conn.lock()?.execute(
            "INSERT OR IGNORE INTO descriptors (descriptor) VALUES (?1)",
            params![descriptor],
        )?;
        Ok(())
    }

    fn descs_get(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock()?;

        let mut stmt = conn.prepare("SELECT descriptor FROM descriptors")?;

        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn get_transaction(&self, txid: &Txid) -> Result<CachedTransaction> {
        let txid_bytes = txid.as_byte_array().to_vec();

        let conn = self.conn.lock()?;

        let result = conn.query_row(
            "SELECT txid, tx, height, merkle_block, position FROM transactions WHERE txid = ?1",
            params![txid_bytes],
            |row| {
                let hash: Vec<u8> = row.get(0)?;
                let tx: Vec<u8> = row.get(1)?;
                let height: i64 = row.get(2)?;
                let merkle_block: Option<Vec<u8>> = row.get(3)?;
                let position: i64 = row.get(4)?;
                Ok((hash, tx, height, merkle_block, position))
            },
        );

        match result {
            Ok((hash, tx, height, merkle_block, position)) => Ok(CachedTransaction {
                hash: Txid::from_byte_array(
                    hash.try_into()
                        .map_err(|_| SqliteDatabaseError::CorruptedData)?,
                ),
                tx: consensus::deserialize(&tx)?,
                height: height as u32,
                merkle_block: merkle_block
                    .map(|mb| consensus::deserialize::<MerkleProof>(&mb))
                    .transpose()?,
                position: position as u32,
            }),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(SqliteDatabaseError::TransactionNotFound)
            }
            Err(e) => Err(SqliteDatabaseError::Sqlite(e)),
        }
    }

    fn save_transaction(&self, tx: &CachedTransaction) -> Result<()> {
        let txid_bytes = tx.hash.as_byte_array().to_vec();

        let tx_bytes = consensus::serialize(&tx.tx);

        let height = tx.height as i64;

        let merkle_block = tx.merkle_block.as_ref().map(consensus::serialize);

        let position = tx.position as i64;

        self.conn.lock()?.execute(
            "INSERT OR REPLACE INTO transactions (txid, tx, height, merkle_block, position)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![txid_bytes, tx_bytes, height, merkle_block, position],
        )?;
        Ok(())
    }

    fn get_stats(&self) -> Result<Stats> {
        let conn = self.conn.lock()?;
        let result = conn.query_row(
            "SELECT address_count, transaction_count, utxo_count, cache_height,
                    txo_count, balance, derivation_index
             FROM stats WHERE id = 0",
            [],
            |row| {
                Ok(Stats {
                    address_count: row.get::<_, i64>(0)? as usize,
                    transaction_count: row.get::<_, i64>(1)? as usize,
                    utxo_count: row.get::<_, i64>(2)? as usize,
                    cache_height: row.get::<_, i64>(3)? as u32,
                    txo_count: row.get::<_, i64>(4)? as usize,
                    balance: row.get::<_, i64>(5)? as u64,
                    derivation_index: row.get::<_, i64>(6)? as u32,
                })
            },
        );

        match result {
            Ok(stats) => Ok(stats),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(SqliteDatabaseError::WalletNotInitialized)
            }
            Err(e) => Err(SqliteDatabaseError::Sqlite(e)),
        }
    }

    fn save_stats(&self, stats: &Stats) -> Result<()> {
        self.conn.lock()?.execute(
            "INSERT OR REPLACE INTO stats
             (id, address_count, transaction_count, utxo_count, cache_height,
              txo_count, balance, derivation_index)
             VALUES (0, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                stats.address_count as i64,
                stats.transaction_count as i64,
                stats.utxo_count as i64,
                stats.cache_height as i64,
                stats.txo_count as i64,
                stats.balance as i64,
                stats.derivation_index as i64,
            ],
        )?;
        Ok(())
    }

    fn list_transactions(&self) -> Result<Vec<Txid>> {
        let conn = self.conn.lock()?;
        let mut stmt = conn.prepare("SELECT txid FROM transactions")?;

        let rows = stmt.query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            Self::hash_from_slice(&bytes, 0)
        })?;

        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod test {
    use core::str::FromStr;
    use std::fs::create_dir;
    use std::path::PathBuf;

    use bitcoin::address::NetworkChecked;
    use bitcoin::consensus::deserialize;
    use bitcoin::consensus::Decodable;
    use bitcoin::hashes::hex::FromHex;
    use bitcoin::hashes::sha256;
    use bitcoin::hashes::Hash;
    use bitcoin::Address;
    use bitcoin::OutPoint;
    use bitcoin::Transaction;
    use bitcoin::Txid;
    use floresta_common::get_spk_hash;

    use super::SqliteDatabase;
    use crate::merkle::MerkleProof;
    use crate::AddressCacheDatabase;
    use crate::CachedAddress;
    use crate::CachedTransaction;
    use crate::Stats;

    fn deserialize_from_str<T: Decodable>(thing: &str) -> T {
        let hex = Vec::from_hex(thing).unwrap();
        deserialize(&hex).unwrap()
    }

    fn get_test_db() -> SqliteDatabase {
        SqliteDatabase::new_ephemeral().unwrap()
    }

    fn get_test_address() -> (Address<NetworkChecked>, sha256::Hash) {
        let address = Address::from_str("tb1q9d4zjf92nvd3zhg6cvyckzaqumk4zre26x02q9")
            .unwrap()
            .assume_checked();
        let script_hash = get_spk_hash(&address.script_pubkey());
        (address, script_hash)
    }

    #[test]
    fn test_meta_getset() {
        let db = get_test_db();

        // Test stats
        let stats = Stats {
            address_count: 12,
            transaction_count: 21,
            utxo_count: 12,
            cache_height: 21,
            txo_count: 12,
            balance: 21,
            derivation_index: 12,
        };

        db.save_stats(&stats).unwrap();
        assert_eq!(db.get_stats().unwrap(), stats);

        // Test cache height
        let test_height: u32 = rand::random();

        db.set_cache_height(test_height).unwrap();
        assert_eq!(db.get_cache_height().unwrap(), test_height);
    }

    #[test]
    fn test_persistence() {
        let path = env!("CARGO_MANIFEST_DIR").to_owned() + "/tmp-db";
        if !PathBuf::from(&path).exists() {
            create_dir(&path).unwrap()
        };

        let db = SqliteDatabase::new(&path).unwrap();
        let descriptor_string = "wsh(sortedmulti(1,[54ff5a12/48h/1h/0h/2h]tpubDDw6pwZA3hYxcSN32q7a5ynsKmWr4BbkBNHydHPKkM4BZwUfiK7tQ26h7USm8kA1E2FvCy7f7Er7QXKF8RNptATywydARtzgrxuPDwyYv4x/<0;1>/*,[bcf969c0/48h/1h/0h/2h]tpubDEFdgZdCPgQBTNtGj4h6AehK79Jm4LH54JrYBJjAtHMLEAth7LuY87awx9ZMiCURFzFWhxToRJK6xp39aqeJWrG5nuW3eBnXeMJcvDeDxfp/<0;1>/*))#fuw35j0q".to_owned();

        db.desc_save(&descriptor_string).unwrap();
        // Kill db
        drop(db);

        let db_new = SqliteDatabase::new(&path).unwrap();

        assert_eq!(db_new.descs_get().unwrap(), vec![descriptor_string]);

        drop(db_new);
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn test_desc_getsetlist() {
        let db = get_test_db();

        // Test descriptors
        let desc = "wsh(sortedmulti(1,[54ff5a12/48h/1h/0h/2h]tpubDDw6pwZA3hYxcSN32q7a5ynsKmWr4BbkBNHydHPKkM4BZwUfiK7tQ26h7USm8kA1E2FvCy7f7Er7QXKF8RNptATywydARtzgrxuPDwyYv4x/<0;1>/*,[bcf969c0/48h/1h/0h/2h]tpubDEFdgZdCPgQBTNtGj4h6AehK79Jm4LH54JrYBJjAtHMLEAth7LuY87awx9ZMiCURFzFWhxToRJK6xp39aqeJWrG5nuW3eBnXeMJcvDeDxfp/<0;1>/*))#fuw35j0q";

        db.desc_save(desc).unwrap();
        assert_eq!(db.descs_get().unwrap(), vec![desc]);

        // Should ignore desc, we should already have it.
        db.desc_save(desc).unwrap();
        assert_eq!(db.descs_get().unwrap().len(), 1);
    }

    #[test]
    fn test_addr_getset() {
        let db = get_test_db();

        // Test addresses
        let (address, script_hash) = get_test_address();

        let cache_address = CachedAddress {
            script_hash,
            balance: 21,
            script: address.script_pubkey(),
            transactions: vec![Txid::all_zeros()],
            utxos: vec![OutPoint::new(Txid::all_zeros(), 42)],
        };

        db.save(&cache_address).unwrap();

        let load = db.load().unwrap();
        assert_eq!(load[0], cache_address);

        let mut updated_address = cache_address.clone();
        updated_address.balance = 1000;

        db.update(&updated_address).unwrap();

        let new_load = db.load().unwrap();

        assert_eq!(new_load[0], updated_address);
    }

    #[test]
    /// Test asserting transactiong get, set
    fn test_transaction_getsetlist() {
        let db = get_test_db();

        // Test transactions
        let transaction = "020000000001017ca523c5e6df0c014e837279ab49be1676a9fe7571c3989aeba1e5d534f4054a0000000000fdffffff01d2410f00000000001600142b6a2924aa9b1b115d1ac3098b0ba0e6ed510f2a02473044022071b8583ba1f10531b68cb5bd269fb0e75714c20c5a8bce49d8a2307d27a082df022069a978dac00dd9d5761aa48c7acc881617fa4d2573476b11685596b17d437595012103b193d06bd0533d053f959b50e3132861527e5a7a49ad59c5e80a265ff6a77605eece0100";
        let transaction: Transaction = deserialize_from_str(transaction);

        let merkle_block = "0100000000000000ea530307089e3e6f6e8997a0ae48e1dc2bee84635bc4e6c6ecdcc7225166b06b010000000000000034086ef398efcdec47b37241221c8f4613e02bc31026cc74d07ddb3092e6d6e7";
        let merkle_block: MerkleProof = deserialize_from_str(merkle_block);

        let cache_tx = CachedTransaction {
            tx: transaction.clone(),
            height: 118511,
            merkle_block: Some(merkle_block),
            hash: transaction.compute_txid(),
            position: 1,
        };

        db.save_transaction(&cache_tx).unwrap();

        let wrong_txid = bitcoin::Txid::all_zeros();
        assert!(db.get_transaction(&wrong_txid).is_err());

        let correct_txid = cache_tx.tx.compute_txid();
        assert_eq!(db.get_transaction(&correct_txid).unwrap(), cache_tx);

        assert_eq!(db.list_transactions().unwrap(), vec![correct_txid]);
    }

    #[test]
    /// Test call behavior when called on a freshly created wallet
    fn test_wallet_not_initialized() {
        let db = get_test_db();
        let expected_err = "Wallet not initialized";

        assert_eq!(
            db.get_cache_height().err().unwrap().to_string(),
            expected_err
        );

        assert_eq!(db.get_stats().err().unwrap().to_string(), expected_err);

        assert_eq!(db.descs_get().unwrap(), Vec::<String>::new());

        assert_eq!(db.list_transactions().unwrap(), Vec::<Txid>::new());
    }
}
