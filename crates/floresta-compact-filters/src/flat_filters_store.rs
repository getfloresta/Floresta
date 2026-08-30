// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::TryFrom;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;

use crate::IterableFilterStore;
use crate::IterableFilterStoreError;

/// The maximum size that a block filter can have.
pub const MAX_FILTER_SIZE: u32 = 1_000_000;

const HEADER_SIZE: u64 = 4;

pub struct FiltersIterator {
    reader: BufReader<File>,
}

impl Iterator for FiltersIterator {
    type Item = (u32, crate::bip158::BlockFilter);

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = [0; 4];

        self.reader.read_exact(&mut buf).ok()?;
        let height = u32::from_le_bytes(buf);

        self.reader.read_exact(&mut buf).ok()?;
        let length = u32::from_le_bytes(buf);

        debug_assert!(
            length < 1_000_000,
            "filter for block {} has length {}",
            height,
            length,
        );

        let mut buf = vec![0_u8; length as usize];
        self.reader.read_exact(&mut buf).ok()?;
        let filter = crate::bip158::BlockFilter::new(&buf);

        Some((height, filter))
    }
}

struct FlatFiltersStoreInner {
    file: std::fs::File,
    index: std::fs::File,
    path: PathBuf,
}

impl From<PoisonError<MutexGuard<'_, FlatFiltersStoreInner>>> for IterableFilterStoreError {
    fn from(_: PoisonError<MutexGuard<'_, FlatFiltersStoreInner>>) -> Self {
        Self::PoisonedLock
    }
}

pub struct FlatFiltersStore(Mutex<FlatFiltersStoreInner>);

impl FlatFiltersStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap();

        let mut index_path = path.as_os_str().to_owned();
        index_path.push("-index");
        let mut index = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&index_path)
            .unwrap();

        index.seek(SeekFrom::Start(0)).unwrap();
        index.write_all(&HEADER_SIZE.to_le_bytes()).unwrap();

        Self(Mutex::new(FlatFiltersStoreInner {
            file,
            path: path.into(),
            index,
        }))
    }
}

impl TryFrom<&PathBuf> for FlatFiltersStore {
    type Error = std::io::Error;

    fn try_from(path: &PathBuf) -> Result<Self, Self::Error> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let index = format!("{}-index", path.to_string_lossy());
        let mut index = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(index)?;

        index.seek(SeekFrom::Start(0))?;
        index.write_all(&HEADER_SIZE.to_le_bytes())?;

        Ok(Self(Mutex::new(FlatFiltersStoreInner {
            file,
            index,
            path: path.clone(),
        })))
    }
}

impl IntoIterator for FlatFiltersStore {
    type Item = (u32, crate::bip158::BlockFilter);
    type IntoIter = FiltersIterator;

    fn into_iter(self) -> Self::IntoIter {
        let mut inner = self.0.lock().unwrap();
        inner.file.seek(SeekFrom::Start(HEADER_SIZE)).unwrap();
        let reader = BufReader::new(inner.file.try_clone().unwrap());
        FiltersIterator { reader }
    }
}

impl IterableFilterStore for FlatFiltersStore {
    type I = FiltersIterator;
    fn set_height(&self, height: u32) -> Result<(), IterableFilterStoreError> {
        let mut inner = self.0.lock()?;
        inner.file.seek(SeekFrom::Start(0))?;
        inner.file.write_all(&height.to_le_bytes())?;

        Ok(())
    }

    fn get_height(&self) -> Result<u32, IterableFilterStoreError> {
        let mut inner = self.0.lock()?;

        let mut buf = [0; 4];
        inner.file.seek(SeekFrom::Start(0))?;
        inner.file.read_exact(&mut buf)?;

        Ok(u32::from_le_bytes(buf))
    }

    fn iter(&self, start_height: Option<usize>) -> Result<Self::I, IterableFilterStoreError> {
        let mut inner = self.0.lock()?;
        let new_file = File::open(inner.path.clone())?;
        let mut reader = BufReader::new(new_file);

        let start_height = start_height.unwrap_or(0) as u32;

        // take the index by dividing by 50_000
        let index = start_height as usize / 50_000;

        // read the whole index, it's just one position every 50_000 blocks
        let mut buf = Vec::new();
        inner.index.seek(SeekFrom::Start(0))?;
        inner.index.read_to_end(&mut buf)?;

        // the positions for blocks we never had a filter for are still zero, and zero
        // isn't a valid one, so keep the last position we actually wrote
        let mut pos = HEADER_SIZE;
        for entry in buf.chunks_exact(8).take(index + 1) {
            let offset = u64::from_le_bytes(entry.try_into().unwrap());
            if offset != 0 {
                pos = offset;
            }
        }

        // seek to the position
        reader.seek(SeekFrom::Start(pos))?;

        // we may be up to 50_000 blocks behind, so walk over the filters we don't want
        loop {
            let mut buf = [0; 4];
            if reader.read_exact(&mut buf).is_err() {
                break;
            }
            let height = u32::from_le_bytes(buf);

            if reader.read_exact(&mut buf).is_err() {
                break;
            }
            let length = u32::from_le_bytes(buf);

            if height >= start_height {
                reader.seek_relative(-8)?;
                break;
            }

            reader.seek_relative(length as i64)?;
        }

        Ok(FiltersIterator { reader })
    }

    fn put_filter(
        &self,
        block_filter: crate::bip158::BlockFilter,
        height: u32,
    ) -> Result<(), IterableFilterStoreError> {
        let length = block_filter.content.len() as u32;

        if length > MAX_FILTER_SIZE {
            return Err(IterableFilterStoreError::OversizedBlockFilter);
        }

        let mut inner = self.0.lock()?;

        let offset = inner.file.seek(SeekFrom::End(0))?;
        // save the position of the file for every 50_000 blocks, so we can
        // start the rescan from a given height
        if height % 50_000 == 0 {
            let index_offset = height / 50_000;
            inner
                .index
                .seek(SeekFrom::Start((index_offset * 8) as u64))?;
            inner.index.write_all(&offset.to_le_bytes())?;
        }

        inner.file.write_all(&height.to_le_bytes())?;
        inner.file.write_all(&length.to_le_bytes())?;
        inner.file.write_all(&block_filter.content)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::remove_file;

    use super::FlatFiltersStore;
    use crate::IterableFilterStore;
    use crate::bip158::BlockFilter;

    #[test]
    fn test_filter_store() {
        let path = "test_filter_store";
        let store = FlatFiltersStore::new(path);

        let res = store.get_height().unwrap_err();
        assert!(matches!(res, crate::IterableFilterStoreError::Io(_)));
        store.set_height(1).expect("could not set height");
        assert_eq!(store.get_height().unwrap(), 1);

        let filter = BlockFilter::new(&[10, 11, 12, 13]);
        store
            .put_filter(filter.clone(), 1)
            .expect("could not put filter");

        let mut iter = store.iter(Some(0)).expect("could not get iterator");
        assert_eq!((1, filter), iter.next().unwrap());

        assert_eq!(iter.next(), None);
        remove_file(path).expect("could not remove file after test");
        remove_file(format!("{path}-index")).expect("could not remove index after test");
    }

    #[test]
    fn test_iter_start_height() {
        let path = "test_iter_start_height";
        let store = FlatFiltersStore::new(path);
        store.set_height(0).expect("could not set height");

        let filter = BlockFilter::new(&[10, 11, 12, 13]);
        for height in [50_000, 99_999, 100_000] {
            store
                .put_filter(filter.clone(), height)
                .expect("could not put filter");
        }

        // 60_000 sits between two index entries, we shouldn't fall back to 50_000
        let heights: Vec<u32> = store
            .iter(Some(60_000))
            .expect("could not get iterator")
            .map(|(height, _)| height)
            .collect();

        assert_eq!(heights, vec![99_999, 100_000]);

        // past every filter and every index entry we have
        let mut iter = store.iter(Some(900_000)).expect("could not get iterator");
        assert_eq!(iter.next(), None);

        remove_file(path).expect("could not remove file after test");
        remove_file(format!("{path}-index")).expect("could not remove index after test");
    }

    #[test]
    fn test_iter_with_a_gap_in_the_index() {
        let path = "test_iter_index_gap";
        let store = FlatFiltersStore::new(path);
        store.set_height(0).expect("could not set height");

        // if we only got filters from a given height, every index entry below it is
        // still zeroed
        let filter = BlockFilter::new(&[10, 11, 12, 13]);
        for height in 700_000..700_010 {
            store
                .put_filter(filter.clone(), height)
                .expect("could not put filter");
        }

        let heights: Vec<u32> = store
            .iter(Some(300_000))
            .expect("could not get iterator")
            .map(|(height, _)| height)
            .collect();

        assert_eq!(heights, (700_000..700_010).collect::<Vec<u32>>());

        remove_file(path).expect("could not remove file after test");
        remove_file(format!("{path}-index")).expect("could not remove index after test");
    }
}
