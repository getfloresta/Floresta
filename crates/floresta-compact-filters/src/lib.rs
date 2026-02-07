// SPDX-License-Identifier: MIT

//! A library for building and querying BIP-158 compact block filters locally
//!
//! This lib implements BIP-158 client-side Galomb-Rice block filters, without
//! relaying on p2p connections to retrieve them. We use this to speedup wallet
//! resyncs and allow arbitrary UTXO retrieving for lightning nodes.
//!
//! This module should receive blocks as we download them, it'll create a filter
//! for it. Therefore, you can't use this to speedup wallet sync **before** IBD,
//! since we wouldn't have the filter for all blocks yet.

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]
#![allow(clippy::manual_is_multiple_of)]

use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;

use bitcoin::bip158::BlockFilter;
use bitcoin::consensus::encode;
use bitcoin::consensus::Decodable;
use bitcoin::consensus::Encodable;
use bitcoin::FilterHeader;
use floresta_common::impl_error_from;

#[derive(Debug)]
pub enum FlatFilterStoreError {
    /// A filter at that height was not found
    NotFound,

    /// An I/O error occurred in the bitcoin library
    BitcoinIo(bitcoin::io::Error),

    /// An I/O error occurred in the standard library
    StdIo(std::io::Error),

    /// A serialization or deserialization error occurred
    Encode(encode::Error),

    /// A poison error, used for mutexes and rwlocks
    Poison,

    /// Our file got corrupted on disk, we need to rebuild
    CorruptedFile,
}

impl PartialEq for FlatFilterStoreError {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (
                FlatFilterStoreError::NotFound,
                FlatFilterStoreError::NotFound
            ) | (
                FlatFilterStoreError::BitcoinIo(_),
                FlatFilterStoreError::BitcoinIo(_)
            ) | (
                FlatFilterStoreError::StdIo(_),
                FlatFilterStoreError::StdIo(_)
            ) | (
                FlatFilterStoreError::Encode(_),
                FlatFilterStoreError::Encode(_)
            )
        )
    }
}

impl Eq for FlatFilterStoreError {}

impl_error_from!(FlatFilterStoreError, bitcoin::io::Error, BitcoinIo);
impl_error_from!(FlatFilterStoreError, encode::Error, Encode);
impl_error_from!(FlatFilterStoreError, std::io::Error, StdIo);

#[derive(Debug, Clone, PartialEq, Eq)]
/// This represents a offset in a filter descriptor.
///
/// It keep track of the actual position of the filter in the data file, as well
/// as if the filter is actually present locally. We won't keep all filters all
/// the time, in order to reclaim space. If the filter is present, we set the MSB
/// of the offset to 1, otherwise it's 0.
pub struct HeaderOffset {
    /// Whether the filter is present locally.
    present: bool,

    /// The offset of the filter in the data file, if present.
    ///
    /// The value of this field is undefined if `present` is false.
    offset: u64,
}

impl HeaderOffset {
    /// Creates a new [`HeaderOffset`].
    pub fn new(present: bool, offset: u64) -> Self {
        Self { present, offset }
    }

    /// Converts the [`HeaderOffset`] to a [`u64`] representation.
    ///
    /// This representation should not be used directly, only for serialization purposes.
    /// It might change in the future without a major version bump.
    /// If you need to parse a [`u64`] representation, use [`HeaderOffset::from_u64`].
    pub fn to_u64(&self) -> u64 {
        if !self.present {
            return 0;
        }

        (1 << 63) | self.offset
    }

    /// Creates a HeaderOffset from a u64 representation.
    ///
    /// This representation should not be used directly, only for serialization purposes.
    /// This function should only be used with values created by `to_u64`.
    pub fn from_u64(value: u64) -> Option<Self> {
        let present = value & (1 << 63) != 0;
        if !present {
            return None;
        }

        let offset = value & !(1 << 63);
        Some(HeaderOffset { present, offset })
    }

    /// Returns whether the filter is present locally.
    pub fn is_present(&self) -> bool {
        self.present
    }

    /// Returns the offset of the filter in the data file, if present.
    pub fn offset(&self) -> Option<u64> {
        if !self.present {
            return None;
        }

        Some(self.offset)
    }
}

impl Encodable for HeaderOffset {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut len = 0;
        len += self.to_u64().consensus_encode(writer)?;
        Ok(len)
    }
}

impl Decodable for HeaderOffset {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let offset = u64::consensus_decode(reader)?;

        Ok(HeaderOffset::from_u64(offset).unwrap_or(HeaderOffset {
            present: false,
            offset: 0,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A descriptor for a filter, including its header and an optional offset in the data file.
///
/// By default, we only keep the filter headers, as we may not need to access the full filters
/// often. We may, however, want to store some of the filters for faster access -- for example,
/// we could keep the last N filters in the store. For that reason, we include an optional offset
/// in the descriptor, which points to the location of the full filter in a separate data file.
///
/// If offset is None, the filter is not stored in the data file, and must be downloaded from peers
pub struct FilterDescriptor {
    /// The filter header, as defined in BIP-157.
    header: FilterHeader,

    /// The filter itself, if stored locally.
    offset: HeaderOffset,
}

impl FilterDescriptor {
    /// The size of a serialized [`FilterDescriptor`] in bytes.
    ///
    /// This is calculated as:
    ///  - 32 bytes for the [`FilterHeader`] hash
    ///  - 8 for the offset, with the MSB reserved for presence flag
    pub const FILTER_DESCRIPTOR_SIZE: u32 = 32 + 8;
}

impl Encodable for FilterDescriptor {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut len = 0;
        len += self.header.consensus_encode(writer)?;
        len += self.offset.consensus_encode(writer)?;
        Ok(len)
    }
}

impl Decodable for FilterDescriptor {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let header = FilterHeader::consensus_decode(reader)?;
        let offset = HeaderOffset::consensus_decode(reader)?;

        Ok(FilterDescriptor { header, offset })
    }
}

/// A store for filter headers, allowing insertion and retrieval by block height.
pub trait FilterHeadersStore {
    /// Inserts a new filter header into the store.
    ///
    /// If you have a reorg, you should use `update_filter_header` to overwrite existing headers.
    fn put_filter_header(&mut self, header: FilterHeader) -> Result<(), FlatFilterStoreError>;

    /// Retrieves a filter header by its block height.
    fn get_filter_header(&mut self, height: u32) -> Result<FilterHeader, FlatFilterStoreError>;

    /// Updates a filter header at a specific height.
    fn update_filter_header(
        &mut self,
        height: u32,
        header: FilterHeader,
    ) -> Result<FilterHeader, FlatFilterStoreError>;

    /// Return the height of the filter headers store (number of headers stored).
    fn get_height(&self) -> Result<Option<u32>, FlatFilterStoreError>;

    /// Retrieves a full block filter by its block height, if stored locally.
    ///
    /// If the filter is not stored locally, returns [`None`]. Note however, that
    /// if we don't have a header for that filter, we return an error. [`None`]
    /// means we have the header, but we have not cached it.
    fn get_filter(&mut self, _height: u32) -> Result<Option<BlockFilter>, FlatFilterStoreError> {
        Ok(None)
    }

    /// Flushes any pending writes to the underlying storage. This is a no-op for in-memory stores,
    /// but may be necessary for file-based implementations to ensure data integrity.
    fn flush(&mut self) -> Result<(), FlatFilterStoreError> {
        Ok(())
    }
}

#[derive(Debug)]
/// A flat file implementation of the [`FilterHeadersStore`] trait.
///
/// This will store filter headers in a binary file, appending new headers to the end of the file.
/// You can retrieve headers by their block height, which corresponds to their position in the
/// file. Each header is stored in a fixed-size format, allowing for efficient random access.
pub struct FlatFilterStore {
    /// The file where filter headers are stored.
    reader: BufReader<File>,

    writer: BufWriter<File>,

    /// The current length of the file, used to determine the next write position.
    len: u32,
}

impl FlatFilterStore {
    /// Creates a new [`FlatFilterStore`]
    ///
    /// It assumes that the directory for the file already exists, the file may not
    /// exist or contain valid filter headers.
    pub fn new(file: &Path) -> Result<Self, FlatFilterStoreError> {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file)?;

        let len = file.metadata().map(|m| m.len()).unwrap_or(0) as u32;
        let file_copy = file.try_clone()?;

        let writer = BufWriter::new(file);
        let reader = BufReader::new(file_copy);

        Ok(Self {
            reader,
            writer,
            len,
        })
    }

    fn update_descriptor(
        &mut self,
        height: u32,
        header: FilterDescriptor,
    ) -> Result<FilterDescriptor, FlatFilterStoreError> {
        let offset = height * FilterDescriptor::FILTER_DESCRIPTOR_SIZE;
        if offset >= self.len {
            return Err(FlatFilterStoreError::NotFound);
        }

        let old_descriptor = self.read_descriptor_at(offset)?;
        let writer = &mut self.writer;

        writer.seek(SeekFrom::Start(offset as u64))?;
        header.consensus_encode(&mut *writer)?;
        Ok(old_descriptor)
    }

    /// Reads a filter header from the file at the specified offset.
    fn read_descriptor_at(
        &mut self,
        offset: u32,
    ) -> Result<FilterDescriptor, FlatFilterStoreError> {
        if offset >= self.len {
            return Err(FlatFilterStoreError::NotFound);
        }

        let reader = &mut self.reader;
        reader.seek(SeekFrom::Start(offset as u64))?;
        let header = FilterDescriptor::consensus_decode(reader)?;
        Ok(header)
    }

    /// Reads a filter header by its block height.
    fn read_descriptor_by_height(
        &mut self,
        height: u32,
    ) -> Result<FilterDescriptor, FlatFilterStoreError> {
        if self.len % FilterDescriptor::FILTER_DESCRIPTOR_SIZE != 0 && self.len != 0 {
            return Err(FlatFilterStoreError::CorruptedFile);
        }

        let offset = height * FilterDescriptor::FILTER_DESCRIPTOR_SIZE;

        // will check bounds in read_descriptor_at
        let header = self.read_descriptor_at(offset)?;
        Ok(header)
    }

    /// Appends a new filter header to the end of the file.
    ///
    /// If you have a reorg, you should use `update_descriptor` to overwrite existing headers.
    fn put_descriptor(&mut self, header: FilterDescriptor) -> Result<(), FlatFilterStoreError> {
        let writer = &mut self.writer;
        writer.seek(std::io::SeekFrom::End(0))?;
        header.consensus_encode(writer)?;
        self.len += FilterDescriptor::FILTER_DESCRIPTOR_SIZE;

        Ok(())
    }
}

impl FilterHeadersStore for FlatFilterStore {
    fn put_filter_header(&mut self, header: FilterHeader) -> Result<(), FlatFilterStoreError> {
        let descriptor = FilterDescriptor {
            header,
            offset: HeaderOffset::new(false, 0),
        };
        self.put_descriptor(descriptor)
    }

    fn get_filter_header(&mut self, height: u32) -> Result<FilterHeader, FlatFilterStoreError> {
        Ok(self.read_descriptor_by_height(height)?.header)
    }

    fn update_filter_header(
        &mut self,
        height: u32,
        header: FilterHeader,
    ) -> Result<FilterHeader, FlatFilterStoreError> {
        let descriptor = FilterDescriptor {
            header,
            offset: HeaderOffset::new(false, 0),
        };

        self.update_descriptor(height, descriptor).map(|d| d.header)
    }

    fn get_height(&self) -> Result<Option<u32>, FlatFilterStoreError> {
        let count = self.len / FilterDescriptor::FILTER_DESCRIPTOR_SIZE;
        if count == 0 {
            return Ok(None);
        }

        // count is 1-based, since the first header is at offset 0, so we need to subtract 1 to get
        // the height of the last header
        Ok(Some(count - 1))
    }

    fn get_filter(&mut self, _height: u32) -> Result<Option<BlockFilter>, FlatFilterStoreError> {
        Ok(None)
    }

    fn flush(&mut self) -> Result<(), FlatFilterStoreError> {
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use bitcoin::consensus::serialize;
    use bitcoin::hashes::Hash;
    use bitcoin::FilterHeader;

    use super::*;

    fn create_test_header(n: u64) -> FilterHeader {
        let mut hash = [0u8; 32];
        let bytes = n.to_le_bytes();
        hash[0..8].copy_from_slice(&bytes);

        FilterHeader::from_raw_hash(Hash::from_byte_array(hash))
    }

    fn tempdir() -> PathBuf {
        // create ./tmp-db if it doesn't exist
        let tmp_dir = PathBuf::from("./tmp-db");
        if !tmp_dir.exists() {
            fs::create_dir(&tmp_dir).unwrap();
        }
        let test_name = rand::random::<u64>();
        PathBuf::from(format!("./tmp-db/test-{test_name}"))
    }

    #[test]
    fn test_put_and_get_filter_header() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();

        let header1 = create_test_header(1);
        let header2 = create_test_header(2);

        store.put_filter_header(header1).unwrap();
        store.put_filter_header(header2).unwrap();

        let retrieved1 = store.read_descriptor_by_height(0).unwrap();
        let retrieved2 = store.read_descriptor_by_height(1).unwrap();

        assert_eq!(retrieved1.header, header1);
        assert_eq!(retrieved2.header, header2);
        assert_eq!(retrieved1.offset, HeaderOffset::new(false, 0));
        assert_eq!(retrieved2.offset, HeaderOffset::new(false, 0));
    }

    #[test]
    fn test_update_filter_header() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();

        let header1 = create_test_header(0);
        let header2 = create_test_header(1);
        let updated_header = create_test_header(2);

        store.put_filter_header(header1).unwrap();
        store.put_filter_header(header2).unwrap();

        store.update_filter_header(0, updated_header).unwrap();

        let retrieved = store.read_descriptor_by_height(0).unwrap();
        assert_eq!(retrieved.header, updated_header);
        assert_eq!(retrieved.offset, HeaderOffset::new(false, 0));
    }

    #[test]
    fn test_get_height() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();

        assert_eq!(store.get_height().unwrap(), None);

        let header1 = create_test_header(1); // block 0
        let header2 = create_test_header(2); // block 1

        store.put_filter_header(header1).unwrap();
        store.put_filter_header(header2).unwrap();
        assert_eq!(store.get_height().unwrap(), Some(1));
    }

    #[test]
    fn test_not_found() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();

        let result = store.get_filter_header(0);
        assert_eq!(result, Err(FlatFilterStoreError::NotFound));

        let result = store.update_filter_header(0, create_test_header(1));
        assert_eq!(result, Err(FlatFilterStoreError::NotFound));
    }

    #[test]
    fn test_persistence() {
        let file_path = tempdir();

        {
            let mut store = FlatFilterStore::new(&file_path).unwrap();
            let header1 = create_test_header(1);
            let header2 = create_test_header(2);
            store.put_filter_header(header1).unwrap();
            store.put_filter_header(header2).unwrap();
            assert_eq!(store.get_height().unwrap(), Some(1));
        }

        {
            let mut store = FlatFilterStore::new(&file_path).unwrap();
            assert_eq!(store.get_height().unwrap(), Some(1));

            let retrieved1 = store.read_descriptor_by_height(0).unwrap();
            let retrieved2 = store.read_descriptor_by_height(1).unwrap();

            assert_eq!(retrieved1.header, create_test_header(1));
            assert_eq!(retrieved2.header, create_test_header(2));
            assert_eq!(retrieved1.offset, HeaderOffset::new(false, 0));
            assert_eq!(retrieved2.offset, HeaderOffset::new(false, 0));
        }
    }

    #[test]
    fn test_cleanup() {
        let file_path = tempdir();
        {
            let mut store = FlatFilterStore::new(&file_path).unwrap();
            let header1 = create_test_header(1);
            store.put_filter_header(header1).unwrap();
        }
        // Ensure the file is deleted after the test
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_empty_store() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();
        assert_eq!(store.get_height().unwrap(), None);
        let result = store.get_filter_header(0);
        assert_eq!(result, Err(FlatFilterStoreError::NotFound));
    }

    #[test]
    fn test_large_number_of_headers() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();

        let num_headers = 1000;
        for i in 0..num_headers {
            let header = create_test_header(i as u64);
            store.put_filter_header(header).unwrap();
        }

        assert_eq!(store.get_height().unwrap(), Some(num_headers - 1));

        for i in 0..num_headers {
            let retrieved = store.get_filter_header(i).unwrap();
            assert_eq!(retrieved, create_test_header(i as u64));
        }
    }

    #[test]
    fn test_partial_read() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();

        let header1 = create_test_header(1);
        store.put_filter_header(header1).unwrap();

        // Manually truncate the file to simulate a partial write

        let file = store.reader.into_inner();
        file.set_len(FilterDescriptor::FILTER_DESCRIPTOR_SIZE as u64 - 1)
            .unwrap();
        store.reader = BufReader::new(file);
        store.len = FilterDescriptor::FILTER_DESCRIPTOR_SIZE - 1;

        let res = store.get_filter_header(0).unwrap_err();
        assert!(matches!(res, FlatFilterStoreError::CorruptedFile));
    }

    #[test]
    fn test_no_offset() {
        let file_path = tempdir();
        let mut store = FlatFilterStore::new(&file_path).unwrap();

        let header1 = FilterDescriptor {
            header: create_test_header(1),
            offset: HeaderOffset::new(false, 0),
        };
        store.put_filter_header(header1.header).unwrap();

        let retrieved = store.read_descriptor_by_height(0).unwrap();
        assert_eq!(retrieved.header, header1.header);
        assert_eq!(retrieved.offset, HeaderOffset::new(false, 0));
    }

    #[test]
    fn assert_descriptor_size() {
        let desc = FilterDescriptor {
            header: FilterHeader::all_zeros(),
            offset: HeaderOffset::new(false, 0),
        };

        let ser_descriptor_size = serialize(&desc).len() as u32;
        assert_eq!(
            FilterDescriptor::FILTER_DESCRIPTOR_SIZE,
            ser_descriptor_size
        );
    }
}
