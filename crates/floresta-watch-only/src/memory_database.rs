// SPDX-License-Identifier: MIT OR Apache-2.0

//! An in-memory database to store addresses data. Being in-memory means this database is
//! volatile, and all data is lost after the database is dropped or the process is terminated.
//! It's not meant to use in production, but for the integrated testing framework
//!
//! For actual databases that can be used for production code, see [KvDatabase](crate::kv_database::KvDatabase).
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;

use bitcoin::Txid;
use bitcoin::hashes::sha256;
use floresta_common::prelude::sync::RwLock;
use floresta_common::prelude::*;

use super::AddressCacheDatabase;
use super::CachedAddress;
use super::CachedTransaction;
use super::Stats;

#[derive(Debug, Default)]
struct Inner {
    addresses: HashMap<sha256::Hash, CachedAddress>,
    transactions: HashMap<Txid, CachedTransaction>,
    stats: Stats,
    height: u32,
    descriptors: Vec<String>,
}

#[derive(Debug)]
/// Errors related to the [`MemoryDatabase`].
pub enum MemoryDatabaseError {
    /// The lock is poisoned.
    PoisonedLock,
}

#[derive(Debug, Default)]
/// An in-memory database for the watch-only wallet.
pub struct MemoryDatabase {
    inner: RwLock<Inner>,
}

type Result<T> = floresta_common::prelude::Result<T, MemoryDatabaseError>;

impl Display for MemoryDatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PoisonedLock => write!(f, "Poisoned lock"),
        }
    }
}

impl MemoryDatabase {
    /// Create a new [`MemoryDatabase`].
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
        }
    }

    /// Get the [`MemoryDatabase`]'s [`Inner`] for read operations.
    fn get_inner(&self) -> Result<sync::RwLockReadGuard<'_, Inner>> {
        self.inner
            .read()
            .map_err(|_| MemoryDatabaseError::PoisonedLock)
    }

    /// Get the [`MemoryDatabase`]'s [`Inner`] for write operations.
    fn get_inner_mut(&self) -> Result<sync::RwLockWriteGuard<'_, Inner>> {
        self.inner
            .write()
            .map_err(|_| MemoryDatabaseError::PoisonedLock)
    }
}

impl AddressCacheDatabase for MemoryDatabase {
    type Error = MemoryDatabaseError;

    /// Load [`CachedAddress`]es from the [`MemoryDatabase`].
    fn load(&self) -> Result<Vec<CachedAddress>> {
        Ok(self.get_inner()?.addresses.values().cloned().collect())
    }

    /// Save a [`CachedAddress`] to the [`MemoryDatabase`].
    fn save(&self, address: &CachedAddress) -> Result<()> {
        self.get_inner_mut().map(|mut inner| {
            inner
                .addresses
                .insert(address.script_hash, address.to_owned());
        })
    }

    /// Update a [`CachedAddress`] in the [`MemoryDatabase`].
    fn update(&self, address: &CachedAddress) -> Result<()> {
        self.get_inner_mut().map(|mut inner| {
            inner
                .addresses
                .entry(address.script_hash)
                .and_modify(|addr| addr.clone_from(address));
        })
    }

    /// Get the height which [`CachedAddress`]es are cached to.
    fn get_cache_height(&self) -> Result<u32> {
        Ok(self.get_inner()?.height)
    }

    /// Set the height which [`CachedAddress`]es are cached to.
    fn set_cache_height(&self, height: u32) -> Result<()> {
        self.get_inner_mut()?.height = height;
        Ok(())
    }

    /// Add a new descriptor to the [`MemoryDatabase`].
    fn save_descriptor(&self, descriptor: &str) -> Result<()> {
        self.get_inner_mut().map(|mut inner| {
            inner.descriptors.push(descriptor.into());
        })
    }

    /// Get the [`MemoryDatabase`]'s descriptors.
    fn get_descriptors(&self) -> Result<Vec<String>> {
        Ok(self.get_inner()?.descriptors.to_owned())
    }

    /// Get a [`CachedTransaction`] from the [`MemoryDatabase`].
    fn get_transaction(&self, txid: &bitcoin::Txid) -> Result<Option<CachedTransaction>> {
        Ok(self.get_inner()?.transactions.get(txid).cloned())
    }

    /// Save a [`CachedTransaction`] to the [`MemoryDatabase`].
    fn save_transaction(&self, tx: &CachedTransaction) -> Result<()> {
        self.get_inner_mut()?
            .transactions
            .insert(tx.hash, tx.to_owned());
        Ok(())
    }

    /// List the [`CachedTransaction`]s [`Txid`]s.
    fn list_transactions(&self) -> Result<Vec<Txid>> {
        Ok(self.get_inner()?.transactions.keys().copied().collect())
    }

    /// Get [`Stats`] about the [`MemoryDatabase`].
    fn get_stats(&self) -> Result<Stats> {
        Ok(self.get_inner()?.stats.to_owned())
    }

    /// Save [`Stats`] to the [`MemoryDatabase`].
    fn save_stats(&self, stats: &Stats) -> Result<()> {
        self.get_inner_mut().map(|mut inner| {
            inner.stats.clone_from(stats);
        })?;
        Ok(())
    }
}
