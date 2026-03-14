// SPDX-License-Identifier: MIT OR Apache-2.0

use bitcoin::bip158::BlockFilter;
use bitcoin::BlockHash;
use floresta_chain::pruned_utreexo::BlockchainInterface;

use crate::IterableFilterStore;
use crate::IterableFilterStoreError;

#[derive(Debug)]
pub struct NetworkFilters<Storage: IterableFilterStore> {
    filters: Storage,
}

impl<Storage: IterableFilterStore> NetworkFilters<Storage> {
    pub fn new(filters: Storage) -> Self {
        if filters.get_height().is_err() {
            filters.set_height(0).unwrap();
        }

        Self { filters }
    }

    pub fn match_any(
        &self,
        query: Vec<&[u8]>,
        start_height: Option<u32>,
        stop_height: Option<u32>,
        chain: impl BlockchainInterface,
    ) -> Result<Vec<BlockHash>, IterableFilterStoreError> {
        let mut blocks = Vec::new();
        let iter = query.into_iter();

        let start_height = start_height.map(|n| n as usize);

        for (height, filter) in self.filters.iter(start_height)? {
            let hash = chain.get_block_hash(height).unwrap();

            if filter.match_any(&hash, &mut iter.clone()).unwrap() {
                blocks.push(hash);
            }

            if let Some(stop_at) = stop_height {
                if height >= stop_at {
                    break;
                };
            }
        }
        Ok(blocks)
    }

    pub fn push_filter(
        &self,
        filter: BlockFilter,
        height: u32,
    ) -> Result<(), IterableFilterStoreError> {
        self.filters.put_filter(filter, height)
    }

    pub fn get_height(&self) -> Result<u32, IterableFilterStoreError> {
        self.filters.get_height()
    }

    pub fn save_height(&self, height: u32) -> Result<(), IterableFilterStoreError> {
        self.filters.set_height(height)
    }
}
