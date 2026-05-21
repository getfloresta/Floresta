// SPDX-License-Identifier: MIT OR Apache-2.0

use core::error::Error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;
use std::path::Path;

use bitcoin::consensus::deserialize;
use bitcoin::consensus::encode::Error as EncodingError;
use bitcoin::consensus::serialize;
use bitcoin::hashes::Hash;
use bitcoin::Txid;
use floresta_common::impl_error_from;
use floresta_common::prelude::*;
use kv::Bucket;
use kv::Config;
use kv::Store;

use super::AddressCacheDatabase;
use super::CachedAddress;
use super::CachedTransaction;
use super::Stats;

/// A key-value database for the watch-only wallet.
pub struct KvDatabase(Store, Bucket<'static, String, Vec<u8>>);

impl KvDatabase {
    /// Create a new [`KvDatabase`] inside the `datadir`.
    pub fn new(datadir: impl AsRef<Path>) -> Result<KvDatabase> {
        let kv_db_config = Config::new(datadir);

        let kv_db_store = Store::new(kv_db_config)?;
        let kv_db_bucket = kv_db_store.bucket::<String, Vec<u8>>(Some("addresses"))?;

        Ok(KvDatabase(kv_db_store, kv_db_bucket))
    }
}

/// Errors related to the [`KvDatabase`].
#[derive(Debug)]
pub enum KvDatabaseError {
    /// A [`kv`] error.
    KvError(kv::Error),

    /// A [`serde_json`] error.
    SerdeJsonError(serde_json::Error),

    /// The wallet is not initialized.
    WalletNotInitialized,

    /// Failed to deseriliaze data.
    DeserializeError(EncodingError),
}

impl Display for KvDatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            KvDatabaseError::KvError(e) => write!(f, "kv error: {e}"),
            KvDatabaseError::SerdeJsonError(e) => write!(f, "JSON error: {e}"),
            KvDatabaseError::WalletNotInitialized => write!(f, "The wallet is not initialized"),
            KvDatabaseError::DeserializeError(e) => {
                write!(f, "Error whilst deserilializing: {e}")
            }
        }
    }
}

impl Error for KvDatabaseError {}

impl_error_from!(KvDatabaseError, serde_json::Error, SerdeJsonError);
impl_error_from!(KvDatabaseError, kv::Error, KvError);
impl_error_from!(KvDatabaseError, EncodingError, DeserializeError);

type Result<T> = floresta_common::prelude::Result<T, KvDatabaseError>;

impl AddressCacheDatabase for KvDatabase {
    type Error = KvDatabaseError;

    /// Load [`CachedAddress`]es from the [`KvDatabase`].
    fn load(&self) -> Result<Vec<CachedAddress>> {
        let bucket = &self.1;
        let mut addresses = Vec::new();

        for item in bucket.iter() {
            let item = item?;

            let key = item.key::<String>()?;
            if *"height" == key || *"desc" == key {
                continue;
            }
            let value: Vec<u8> = item.value()?;
            let value = serde_json::from_slice(&value)?;
            addresses.push(value);
        }
        Ok(addresses)
    }

    /// Save a [`CachedAddress`] to the [`KvDatabase`].
    fn save(&self, address: &CachedAddress) -> Result<()> {
        let key = address.script_hash.to_string();
        let value = serde_json::to_vec(&address)?;
        self.1.set(&key, &value)?;
        self.1.flush()?;
        Ok(())
    }

    /// Update a [`CachedAddress`] in the [`KvDatabase`].
    fn update(&self, address: &CachedAddress) -> Result<()> {
        self.save(address)
    }

    /// Get the height which [`CachedAddress`]es are cached to.
    fn get_cache_height(&self) -> Result<u32> {
        let bucket = &self.1;

        let cached_height = bucket.get(&String::from("height"))?;
        if let Some(cached_height) = cached_height {
            return Ok(deserialize(&cached_height)?);
        }

        Err(KvDatabaseError::WalletNotInitialized)
    }

    /// Set the height which [`CachedAddress`]es are cached to.
    fn set_cache_height(&self, height: u32) -> Result<()> {
        let bucket = &self.1;

        bucket.set(&String::from("height"), &serialize(&height))?;
        bucket.flush()?;

        Ok(())
    }

    /// Add a new descriptor to the [`KvDatabase`].
    fn save_descriptor(&self, descriptor: &str) -> Result<()> {
        let bucket = &self.1;
        let mut descriptors = self.get_descriptors()?;

        descriptors.push(String::from(descriptor));

        bucket.set(&String::from("desc"), &serde_json::to_vec(&descriptors)?)?;
        bucket.flush()?;

        Ok(())
    }

    /// Get the [`KvDatabase`]'s descriptors.
    fn get_descriptors(&self) -> Result<Vec<String>> {
        let descriptor = self.1.get(&String::from("desc"))?;

        if let Some(descriptor) = descriptor {
            return Ok(serde_json::de::from_slice(&descriptor)?);
        }
        Ok(Vec::new())
    }

    /// Get a [`CachedTransaction`] from the [`KvDatabase`], given its [`Txid`].
    fn get_transaction(&self, txid: &Txid) -> Result<Option<CachedTransaction>> {
        let tx_store = self.0.bucket::<&[u8], Vec<u8>>(Some("transactions"))?;
        let res = tx_store.get(&txid.as_byte_array().to_vec().as_slice())?;
        if let Some(res) = res {
            return Ok(Some(serde_json::de::from_slice(&res)?));
        }
        Ok(None)
    }

    /// Save a [`CachedTransaction`] to the [`KvDatabase`].
    fn save_transaction(&self, tx: &CachedTransaction) -> Result<()> {
        let tx_store = self.0.bucket::<&[u8], Vec<u8>>(Some("transactions"))?;
        let bucket = &self.1;

        let serialized_tx = serde_json::to_vec(&tx)?;
        tx_store.set(
            &tx.tx.compute_txid().as_byte_array().to_vec().as_slice(),
            &serialized_tx,
        )?;
        bucket.flush()?;

        Ok(())
    }

    /// Get [`Stats`] about the [`KvDatabase`].
    fn get_stats(&self) -> Result<Stats> {
        let stats_store = self.0.bucket::<&[u8], Vec<u8>>(Some("stats"))?;

        let stats = stats_store.get(&String::from("stats").as_bytes())?;
        if let Some(stats) = stats {
            return Ok(serde_json::de::from_slice(&stats)?);
        }
        Err(KvDatabaseError::WalletNotInitialized)
    }

    /// Save [`Stats`] to the [`KvDatabase`].
    fn save_stats(&self, stats: &Stats) -> Result<()> {
        let stats_store = self.0.bucket::<&[u8], Vec<u8>>(Some("stats"))?;

        let ser_stats = serde_json::to_vec(&stats)?;
        stats_store.set(&String::from("stats").as_bytes(), &ser_stats)?;
        self.1.flush()?;

        Ok(())
    }

    /// List the [`CachedTransaction`]s [`Txid`]s.
    fn list_transactions(&self) -> Result<Vec<Txid>> {
        let mut transactions = Vec::new();
        let tx_store = self.0.bucket::<&[u8], Vec<u8>>(Some("transactions"))?;

        for item in tx_store.iter() {
            let item = item?;
            let key = item.key::<&[u8]>()?;
            transactions.push(
                Txid::from_slice(key)
                    .expect("keys in the transactions bucket are always 32-byte txids"),
            );
        }
        Ok(transactions)
    }
}

#[cfg(test)]
mod test {
    use core::str::FromStr;

    use bitcoin::address::NetworkChecked;
    use bitcoin::consensus::deserialize;
    use bitcoin::hashes::hex::FromHex;
    use bitcoin::hashes::sha256;
    use bitcoin::Address;
    use bitcoin::Transaction;
    use floresta_common::get_spk_hash;

    use super::KvDatabase;
    use crate::AddressCacheDatabase;
    use crate::CachedAddress;
    use crate::CachedTransaction;
    use crate::Stats;

    fn get_test_db() -> KvDatabase {
        let test_id = rand::random::<u32>();

        KvDatabase::new(format!("./tmp-db/{test_id}.floresta/")).unwrap()
    }
    fn get_test_address() -> (Address<NetworkChecked>, sha256::Hash) {
        let address = Address::from_str("tb1q9d4zjf92nvd3zhg6cvyckzaqumk4zre26x02q9")
            .unwrap()
            .assume_checked();
        let script_hash = get_spk_hash(&address.script_pubkey());
        (address, script_hash)
    }
    #[test]
    fn test_kv_database() {
        let db = get_test_db();
        let (address, script_hash) = get_test_address();

        let cache_address = CachedAddress {
            script_hash,
            balance: 0,
            script: address.script_pubkey(),
            transactions: Vec::new(),
            utxos: Vec::new(),
        };
        let test_height: u32 = rand::random();
        let desc = "wsh(sortedmulti(1,[54ff5a12/48h/1h/0h/2h]tpubDDw6pwZA3hYxcSN32q7a5ynsKmWr4BbkBNHydHPKkM4BZwUfiK7tQ26h7USm8kA1E2FvCy7f7Er7QXKF8RNptATywydARtzgrxuPDwyYv4x/<0;1>/*,[bcf969c0/48h/1h/0h/2h]tpubDEFdgZdCPgQBTNtGj4h6AehK79Jm4LH54JrYBJjAtHMLEAth7LuY87awx9ZMiCURFzFWhxToRJK6xp39aqeJWrG5nuW3eBnXeMJcvDeDxfp/<0;1>/*))#fuw35j0q";

        let transaction = "020000000001017ca523c5e6df0c014e837279ab49be1676a9fe7571c3989aeba1e5d534f4054a0000000000fdffffff01d2410f00000000001600142b6a2924aa9b1b115d1ac3098b0ba0e6ed510f2a02473044022071b8583ba1f10531b68cb5bd269fb0e75714c20c5a8bce49d8a2307d27a082df022069a978dac00dd9d5761aa48c7acc881617fa4d2573476b11685596b17d437595012103b193d06bd0533d053f959b50e3132861527e5a7a49ad59c5e80a265ff6a77605eece0100";
        let transaction = Vec::from_hex(transaction).unwrap();
        let transaction: Transaction = deserialize(&transaction).unwrap();

        let merkle_block = "0100000000000000ea530307089e3e6f6e8997a0ae48e1dc2bee84635bc4e6c6ecdcc7225166b06b010000000000000034086ef398efcdec47b37241221c8f4613e02bc31026cc74d07ddb3092e6d6e7";
        let merkle_block = Vec::from_hex(merkle_block).unwrap();
        let merkle_block = deserialize(&merkle_block).unwrap();

        let cache_tx = CachedTransaction {
            tx: transaction.clone(),
            height: 118511,
            merkle_block: Some(merkle_block),
            hash: transaction.compute_txid(),
            position: 1,
        };

        let stats = Stats {
            address_count: 11,
            transaction_count: 11,
            utxo_count: 11,
            cache_height: 11,
            txo_count: 11,
            balance: 11,
            derivation_index: 11,
        };

        db.save_stats(&stats).unwrap();
        assert_eq!(db.get_stats().unwrap().address_count, 11);

        db.save_transaction(&cache_tx).unwrap();
        assert_eq!(
            db.get_transaction(&cache_tx.hash).unwrap(),
            Some(cache_tx.clone())
        );
        assert_eq!(db.list_transactions().unwrap(), vec![cache_tx.hash]);

        db.set_cache_height(test_height).unwrap();
        assert_eq!(db.get_cache_height().unwrap(), test_height);

        db.save_descriptor(desc).unwrap();
        assert_eq!(db.get_descriptors().unwrap(), vec![desc]);

        db.update(&cache_address).unwrap();
        assert_eq!(db.load().unwrap()[0].script_hash, cache_address.script_hash);
    }
}
