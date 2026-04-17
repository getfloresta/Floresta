// SPDX-License-Identifier: MIT OR Apache-2.0

//! In-memory [`ChainStore`] implementation for WASM environments where filesystem
//! access is unavailable.

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

use bitcoin::BlockHash;
use floresta_chain::BestChain;
use floresta_chain::ChainStore;
use floresta_chain::DatabaseError;
use floresta_chain::DiskBlockHeader;

#[derive(Debug)]
pub enum MemoryChainStoreError {
    PoisonedLock,
}

impl fmt::Display for MemoryChainStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryChainStoreError::PoisonedLock => write!(f, "poisoned lock"),
        }
    }
}

impl DatabaseError for MemoryChainStoreError {}

struct Inner {
    headers: HashMap<BlockHash, DiskBlockHeader>,
    height_to_hash: HashMap<u32, BlockHash>,
    best_chain: Option<BestChain>,
    roots: HashMap<u32, Vec<u8>>,
}

/// An in-memory chain store suitable for WASM targets.
pub struct MemoryChainStore {
    inner: RwLock<Inner>,
}

impl MemoryChainStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                headers: HashMap::new(),
                height_to_hash: HashMap::new(),
                best_chain: None,
                roots: HashMap::new(),
            }),
        }
    }
}

impl Default for MemoryChainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ChainStore for MemoryChainStore {
    type Error = MemoryChainStoreError;

    fn save_roots_for_block(&mut self, roots: Vec<u8>, height: u32) -> Result<(), Self::Error> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        inner.roots.insert(height, roots);
        Ok(())
    }

    fn load_roots_for_block(&mut self, height: u32) -> Result<Option<Vec<u8>>, Self::Error> {
        let inner = self
            .inner
            .read()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        Ok(inner.roots.get(&height).cloned())
    }

    fn load_height(&self) -> Result<Option<BestChain>, Self::Error> {
        let inner = self
            .inner
            .read()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        Ok(inner.best_chain.clone())
    }

    fn save_height(&mut self, height: &BestChain) -> Result<(), Self::Error> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        inner.best_chain = Some(height.clone());
        Ok(())
    }

    fn get_header(&self, block_hash: &BlockHash) -> Result<Option<DiskBlockHeader>, Self::Error> {
        let inner = self
            .inner
            .read()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        Ok(inner.headers.get(block_hash).copied())
    }

    fn get_header_by_height(&self, height: u32) -> Result<Option<DiskBlockHeader>, Self::Error> {
        let inner = self
            .inner
            .read()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        let hash = match inner.height_to_hash.get(&height) {
            Some(h) => h,
            None => return Ok(None),
        };
        Ok(inner.headers.get(hash).copied())
    }

    fn save_header(&mut self, header: &DiskBlockHeader) -> Result<(), Self::Error> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        inner.headers.insert(header.block_hash(), *header);
        if let Some(height) = header.height() {
            inner.height_to_hash.insert(height, header.block_hash());
        }
        Ok(())
    }

    fn get_block_hash(&self, height: u32) -> Result<Option<BlockHash>, Self::Error> {
        let inner = self
            .inner
            .read()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        Ok(inner.height_to_hash.get(&height).copied())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // No-op for in-memory store
        Ok(())
    }

    fn update_block_index(&mut self, height: u32, hash: BlockHash) -> Result<(), Self::Error> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| MemoryChainStoreError::PoisonedLock)?;
        inner.height_to_hash.insert(height, hash);
        Ok(())
    }

    fn check_integrity(&self) -> Result<(), Self::Error> {
        // Always passes for in-memory store
        Ok(())
    }
}
