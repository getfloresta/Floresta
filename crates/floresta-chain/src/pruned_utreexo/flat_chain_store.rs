//! A fast database for the chainstore
//!
//! In its infancy, floresta-chain used `kv` as database, since `kv` is a small and efficient
//! embedded database that doesn't require any runtime dependency. However, floresta-chain uses the
//! database in a very unusual way: it downloads a bunch of small chunks of data that needs to be
//! indexed and retrieved, all at once (~800k for mainnet at the time of writing). If we simply
//! keep everything in memory, and then make one big batch, most embedded databases will see a big
//! spike in heap usage. This would be OK for regular desktops, but floresta aims to run in small,
//! lower-power devices too, so we can't just assume we have two gigs of RAM to spare. We could make
//! these writes more commonly, but then we reach an I/O bottleneck in those lower-power systems,
//! where usually we won't see high-quality SSDs that can make billions of transfers per second.
//!
//! This chainstore was designed to reduce the over-usage of both. We do rely on any extra RAM as
//! kernel buffers, but we also do a decent level of I/O. We get a better performance by using
//! an ad-hock storage that exploits the fact that the data we keep is canonical and monotonically
//! increasing, so we keep all headers in a simple flat file, one after the other. So `pos(h) = h *
//! size_of(DiskBlockHeader)`, with an overhead factor of 1. We also need a way to map block hashes
//! into the given header, we do this by keeping a persistent, open-addressing hash map that map block
//! hashes -> heights. Then from the height we can work out the block header in the headers file.
//!
//! ## Calculations
//!
//! We want to keep the load factor for the hash map as low as possible, while also avoiding
//! re-hashing. So we pick a fairly big initial space to it, say 10 million buckets. Each bucket is
//! 4 bytes long, so we have 40 MiB of map. Each `HashedDiskHeader` is 124 bytes long (80 bytes for
//! header + 4 for height + 32 for hash + 8 for the accumulator size and pos), so the maximum size
//! for it, assuming 2.5 million headers (good for the next ~30 years), is 310 MiB.
//! The smallest device I know that can run floresta has ~250 MiB of RAM, so we could
//! fit almost everything in memory. Given that newer blocks are more likely to be accessed,
//! the OS will keep those in RAM.
//!
//! The longest chain we have right now is testnet, with about 3 million blocks. That yields a load
//! factor of 0.3. With that load factor, there's a ~1/3 probability of collision, we are expected
//! to have a worst-case search ~2. So we'll need to fetch two nodes to find the one we want. Since
//! each node is a u32, most of the time we'll pull the second node too (64 bits machines can't
//! pull 32 bits values from memory). But to avoid going into the map every time, we keep a LRU
//! cache of the last n blocks we've touched.
//!
//! We also keep the accumulators for every block in a separate file, so we can load them in case
//! of a reorg. They are stored in a regular file, and we keep the position and length of each one.
//! The forest for mainnet has about 2^31 leaves, if we assume a Hamming weight of 1/2, we have
//! 16 hashes per block, plus a 8-bytes leaves count. At the time of writing, we are approaching
//! 900k blocks on mainnet. So we would have 32 * 16 + 8 = 520 bytes per accumulator.
//! 520 * 900k = 468 MiB. This is the absolute worst case for the almost two decades that Bitcoin
//! existed. However, although this is a pretty manageable number, we can safely get rid of some
//! older roots, only storing the latest ones, and a few old ones for very deep reorgs. This is,
//! however, a TODO.
//!
//! # Good to know
//!
//! A load factor of a hashmap is the relation between empty buckets and buckets that are being used.
//! The load factor is used to express the chance of hash collisions which decreases performance.
//!
//! Buckets are the slots of a hashmap.
//!
//! For more detailed information please refer to [Hash Table](https://en.wikipedia.org/wiki/Hash_table)
//! from wikipedia.
//!
//! # Safety
//!
//! This is completely reckless and bare-bones, so it needs some safety precautions:
//!     (i): we must make sure that the map and flat file are initialized correctly
//!     (ii): we must make sure that the map won't give us any height greater than the size
//!           of the flat file
//!     (iii): we must make sure that the load factor **never** reaches one
//! i and ii will cause a segfault, iii will turn the addition (or search for non-existent values)
//! an infinite loop. If we are about to reach the map's capacity, we should re-hash with a new
//! capacity.

extern crate std;

use core::mem::size_of;
use core::num::NonZeroUsize;
use std::fs::DirBuilder;
use std::fs::File;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::fs::Permissions;
use std::io::Seek;
use std::io::SeekFrom;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;

use bitcoin::hashes::Hash;
use bitcoin::BlockHash;
use floresta_common::impl_error_from;
use floresta_common::prelude::*;
use index_impl::Index;
use lru::LruCache;
use memmap2::MmapMut;
use memmap2::MmapOptions;
use tracing::info;
use twox_hash::XxHash3_64;

use crate::BestChain;
use crate::ChainStore;
use crate::DatabaseError;
use crate::DiskBlockHeader;

/// The magic number we use to make sure we're reading the right file
///
/// This is backwards, because when we look at the hex dump of the file, on little-endian systems,
/// it will show in the correct order.
const FLAT_CHAINSTORE_MAGIC: u32 = 0x74_73_6C_66; // "flst" backwards

/// The version of our flat chain store
const FLAT_CHAINSTORE_VERSION: u32 = 1;

/// We use a LRU cache to keep the last n blocks we've touched, so we don't need to do a map search
/// again. This is the type of our cache
type CacheType = LruCache<BlockHash, DiskBlockHeader>;

/// How long an accumulator is.
///
/// Worst case, we have 64 roots, each with 32 bytes, and a 64 bits integer for the number of
/// leaves. So 32 * 64 + 8 = 2048 + 8 = 2056 bytes
const UTREEXO_ACC_SIZE: usize = 32 * 64 + 8;

#[derive(Clone)]
/// Configuration for our flat chain store. See each field for more information
pub struct FlatChainStoreConfig {
    /// The index map size, in buckets
    ///
    /// This index holds our map from block hashes to block heights. We use an open-addressing hash
    /// map to map block hashes to block heights. Ideally, size should be way bigger than the
    /// number of blocks we expect to have in our chain, therefore reducing the load factor to a
    /// negligible value. The default value is having space for 10 million blocks.
    ///
    /// We compute the actual capacity by rounding the requested size up to the next power of two,
    /// so we can use `hash & (capacity - 1)` instead of `hash % capacity`.
    pub block_index_size: Option<usize>,

    /// The size of the headers file map, in headers
    ///
    /// This is the size of the flat file that holds all of our block headers. We keep all headers
    /// in a simple flat file, one after the other. That file then gets mapped into RAM, so we can
    /// use pointer arithmetic to find specific block, since pos(h) = h * size_of(DiskBlockHeader)
    /// The default value is having space for 10 million blocks.
    ///
    /// We compute the actual capacity by rounding the requested size up to the next power of two.
    pub headers_file_size: Option<usize>,

    /// The size of the cache, in blocks
    ///
    /// We keep a LRU cache of the last n blocks we've touched. This is to avoid going into the
    /// map every time we need to find a block. The default value is 1000 blocks.
    pub cache_size: Option<usize>,

    /// The permission for all the files we create
    ///
    /// This is the permission we give to all the files we create. The default value is 0o660
    pub file_permission: Option<u32>,

    /// The size of the fork headers file map, in headers
    ///
    /// This store keeps headers that are not in our main chain, but may be needed sometime. The
    /// default value is having space for 10,000 blocks.
    ///
    /// We compute the actual capacity by rounding the requested size up to the next power of two.
    pub fork_file_size: Option<usize>,

    /// The path where we store our files
    ///
    /// We'll create a few files (namely, the index map, headers file, forks file, and metadata file).
    /// We need a directory where we can read and write, it needs at least 880 MiB of free space.
    /// And have a file system that supports mmap and sparse files (all the default *unix FS do).
    pub path: String,
}

impl FlatChainStoreConfig {
    /// Creates a new configuration with the default values
    pub fn new(path: String) -> Self {
        FlatChainStoreConfig {
            file_permission: Some(0o666),
            fork_file_size: Some(10_000),
            path,
            headers_file_size: Some(10_000_000),
            block_index_size: Some(10_000_000),
            cache_size: Some(10_000),
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileChecksum(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The current checksum of our database
pub struct DbCheckSum {
    /// The checksum of the headers file
    headers_checksum: FileChecksum,

    /// The checksum of the index map
    index_checksum: FileChecksum,

    /// The checksum of the fork headers file
    fork_headers_checksum: FileChecksum,
}

/// A bucket in our index map, holding a pointer to the index.
///
/// This enum indicates whether a given bucket is occupied, and if it is, it holds the respective
/// block header as well.
enum IndexBucket {
    /// This bucket is empty
    ///
    /// If this is a search, this means the entry isn't in the map, and this is where it would be
    Empty { ptr: *mut Index },

    /// This bucket is occupied. We can read or overwrite the index value from the pointer.
    Occupied {
        ptr: *mut Index,
        header: DiskBlockHeader,
    },
}

/// A simple index implementation with safe API
mod index_impl {
    use super::FlatChainstoreError;

    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    /// A tagged block index (MSB = chain status, lower 31 bits = position in the header file).
    ///
    /// - If MSB = 0, the block is in the **main chain**. In this case, the index is equal to the
    ///   block height, as we store mainchain headers with canonical order.
    /// - If MSB = 1, the block is in a **fork chain**. In this case, the index is different from
    ///   the height, as we store fork headers (in the fork file) in the order they are found.
    pub struct Index(u32);

    impl Index {
        /// Mask for the MSB
        const FORK_BIT: u32 = 0x8000_0000;

        /// Mask for the 31 lower bits
        const INDEX_MASK: u32 = 0x7FFF_FFFF;

        /// Create a new mainchain entry (MSB is zero)
        pub fn new(index: u32) -> Result<Self, FlatChainstoreError> {
            if index >= Self::FORK_BIT {
                // Index value is out of bounds for our 31-bit indexes
                return Err(FlatChainstoreError::IndexTooBig);
            }

            Ok(Index(index))
        }

        /// Create a new fork entry (MSB is set)
        pub fn new_fork(index: u32) -> Result<Self, FlatChainstoreError> {
            if index >= Self::FORK_BIT {
                // Index value is out of bounds for our 31-bit indexes
                return Err(FlatChainstoreError::IndexTooBig);
            }

            Ok(Index(index | Self::FORK_BIT))
        }

        /// Tells if this is a block in our main chain
        pub fn is_main_chain(&self) -> bool {
            self.0 & Self::FORK_BIT == 0
        }

        /// Takes only the integer height of the block, without the tag
        pub fn index(&self) -> u32 {
            self.0 & Self::INDEX_MASK
        }

        /// Tells if this is an empty position (i.e., we haven't written anything here yet, or this
        /// is the mainchain genesis index)
        pub fn is_empty(&self) -> bool {
            self.0 == 0
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
/// To avoid having to sha256 headers every time we retrieve them, we store the hash along with
/// the header, so we just need to compare the hash to know if we have the right header
struct HashedDiskHeader {
    /// The actual header with contextually relevant information
    header: DiskBlockHeader,

    /// The hash of the header
    hash: BlockHash,

    /// Where in the accumulator file this block's accumulator is
    acc_pos: u32,

    /// The length of the block's accumulator
    acc_len: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
/// Metadata about our chainstate and the blocks we have. We need this to keep track of the
/// network state, our local validation state and accumulator state
struct Metadata {
    /// A magic number to make sure we're reading the right file and it was initialized correctly
    magic: u32,

    /// The version of our flat chain store
    version: u32,

    /// Hash of the last block in the chain we believe has more work on
    best_block: BlockHash,

    /// How many blocks are pilled on this chain
    depth: u32,

    /// We actually validated blocks up to this point
    validation_index: BlockHash,

    /// Blockchains are not fast-forward only, they might have "forks", sometimes it's useful
    /// to keep track of them, in case they become the best one. This keeps track of some
    /// tips we know about, but are not the best one. We don't keep tips that are too deep
    /// or has too little work if compared to our best one
    alternative_tips: [BlockHash; 64], // we hope to never have more than 64 alt-chains

    /// How many blocks we have that are not in our main chain
    fork_count: u32,

    /// The size of the headers file map, in headers
    headers_file_size: usize,

    /// The size of the fork headers file map, in headers
    fork_file_size: usize,

    /// This holds how much of the index is occupied
    block_index_occupancy: usize,

    /// The capacity of the index, in buckets
    index_capacity: usize,

    /// The checksum of our database, as it was in the last time we've flushed our data
    /// We can use this to check if our database is corrupted
    checksum: DbCheckSum,
}

#[derive(Debug)]
/// An error that can happen when we're dealing with our flat chain store
pub enum FlatChainstoreError {
    /// An I/O error happened
    ///
    /// Check the inner error for more information
    Io(std::io::Error),

    /// We couldn't find the block we were looking for
    BlockNotFound,

    /// The index is full, we can't add more blocks to it
    IndexIsFull,

    /// Tried to open a database with an unsupported version number
    DbTooNew(u32),

    /// Our cache lock is poisoned
    Poisoned,

    /// We encountered an invalid magic value, possibly database corruption
    InvalidMagic(u32),

    /// The provided accumulator is too big
    AccumulatorTooBig,

    /// Tried to create an index more than 31 bits long
    IndexTooBig,

    /// Something wrong happened with the metadata file mmap
    InvalidMetadataPointer,

    /// The database is corrupted
    DbCorrupted,

    /// The validation index doesn't have a height. This probably means it is in
    /// a fork or invalid chain
    InvalidValidationIndex,
}

impl core::fmt::Display for FlatChainstoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FlatChainstoreError::Io(e) => write!(f, "io error: {}", e),
            FlatChainstoreError::BlockNotFound => write!(f, "block not found"),
            FlatChainstoreError::IndexIsFull => write!(f, "index is full"),
            FlatChainstoreError::DbTooNew(v) => write!(f, "database version {} is too new", v),
            FlatChainstoreError::Poisoned => write!(f, "lock is poisoned"),
            FlatChainstoreError::InvalidMagic(m) => write!(f, "invalid magic: {:#x}", m),
            FlatChainstoreError::AccumulatorTooBig => write!(f, "accumulator is too big"),
            FlatChainstoreError::IndexTooBig => write!(f, "index is too big"),
            FlatChainstoreError::InvalidMetadataPointer => write!(f, "invalid metadata pointer"),
            FlatChainstoreError::DbCorrupted => write!(f, "database is corrupted"),
            FlatChainstoreError::InvalidValidationIndex => write!(f, "invalid validation index"),
        }
    }
}

/// Need this to use [FlatChainstoreError] as a [DatabaseError] in [ChainStore]
impl DatabaseError for FlatChainstoreError {}

impl_error_from!(FlatChainstoreError, std::io::Error, Io);

impl From<PoisonError<MutexGuard<'_, CacheType>>> for FlatChainstoreError {
    fn from(_: PoisonError<MutexGuard<'_, CacheType>>) -> Self {
        FlatChainstoreError::Poisoned
    }
}

/// A hash map implementation that maps block hashes to u32 indexes. Indexes are stored scattered
/// across the memory-mapped file and accessed via `hash_map_find_pos`. We keep track of how many
/// buckets are occupied in the metadata file (so we can re-hash the map when needed).
struct BlockIndex {
    /// The memory map for the block indexes
    index_map: MmapMut,

    /// The maximum size of the index map, in buckets
    index_size: usize,
}

impl BlockIndex {
    /// Creates a new block index
    ///
    /// This function should only be called by [FlatChainStore::new], and it should never be called
    /// directly. It creates a new block index, given a mutable memory-mapped buffer for the index
    /// map and its maximum size in buckets.
    fn new(index_map: MmapMut, index_size: usize) -> Self {
        Self {
            index_map,
            index_size,
        }
    }

    /// Flushes the index map to disk
    ///
    /// If we have enough changes that we don't want to lose, we should flush the index map to disk.
    /// This makes sure the indexes are persisted, and we can recover them in case of a crash.
    fn flush(&self) -> Result<(), FlatChainstoreError> {
        self.index_map.flush()?;

        Ok(())
    }

    /// Updates our index to map a block hash to an index
    ///
    /// After accepting a new block, this should be updated to record its position in the chain.
    /// Returns true if the position was empty, and false if it was occupied (meaning we are
    /// rewriting an existing entry).
    unsafe fn set_index_for_hash(
        &self,
        hash: BlockHash,
        index: Index,
        get_header_by_index: impl Fn(Index) -> Result<HashedDiskHeader, FlatChainstoreError>,
    ) -> Result<bool, FlatChainstoreError> {
        let pos = self.hash_map_find_pos(hash, get_header_by_index)?;

        match pos {
            IndexBucket::Empty { ptr } => {
                ptr.write(index);
                Ok(true)
            }

            // A position may be re-written if we happen to have a reorg.
            // If this is the case, we should update the fork block to make it into the main chain,
            // and mark the old main chain block as a fork.
            IndexBucket::Occupied { ptr, .. } => {
                ptr.write(index);
                Ok(false)
            }
        }
    }

    /// Returns the block index for a given block hash and its fetched header, if present
    unsafe fn get_index_for_hash(
        &self,
        hash: BlockHash,
        get_header_by_index: impl Fn(Index) -> Result<HashedDiskHeader, FlatChainstoreError>,
    ) -> Result<Option<(Index, DiskBlockHeader)>, FlatChainstoreError> {
        match self.hash_map_find_pos(hash, get_header_by_index)? {
            IndexBucket::Empty { .. } => Ok(None),
            IndexBucket::Occupied { ptr, header } => Ok(Some((*ptr, header))),
        }
    }

    /// Returns the position inside the hash map where a given hash should be
    ///
    /// This function computes the short hash for the block hash and looks up the position inside
    /// the index map. If the found index fetches the header we are looking for, return this bucket.
    /// Otherwise, we continue incrementing the short hash until we either find the record or a
    /// vacant position. If you're adding a new entry, call this function (it will return a vacant
    /// position) and write the height there.
    unsafe fn hash_map_find_pos(
        &self,
        block_hash: BlockHash,
        get_header_by_index: impl Fn(Index) -> Result<HashedDiskHeader, FlatChainstoreError>,
    ) -> Result<IndexBucket, FlatChainstoreError> {
        let mut hash = Self::index_hash_fn(block_hash) as usize;

        // Retrieve the base pointer to the start of the memory-mapped index
        let base_ptr = self.index_map.as_ptr() as *mut Index;

        // Since the size is a power of two `2^k`, subtracting one gives a 0b111...1 k-bit mask
        let mask = self.index_size - 1;

        for _ in 0..self.index_size {
            // Obtain the bucket's address by adding the masked hash to the base pointer
            // SAFETY: the masked hash is lower than the `index_size`
            let entry_ptr = base_ptr.add(hash & mask);

            // If this is the first time we've accessed this pointer, this candidate index is 0
            let candidate_index = *entry_ptr;

            // If the header at `candidate_index` matches `block_hash`, this is the target bucket
            let file_header = get_header_by_index(candidate_index)?;
            if file_header.hash == block_hash {
                return Ok(IndexBucket::Occupied {
                    ptr: entry_ptr,
                    header: file_header.header,
                });
            }

            // If we find an empty index, this bucket is where the entry would be added
            // Note: The genesis block doesn't reach this point, as its header hash is matched
            if candidate_index.is_empty() {
                return Ok(IndexBucket::Empty { ptr: entry_ptr });
            }

            // If no match and bucket is occupied, continue probing the next bucket
            hash = hash.wrapping_add(1);
        }

        // If we reach here, it means the index is full. We should re-hash the map
        Err(FlatChainstoreError::IndexIsFull)
    }

    /// The (short) hash function we use to compute where in the map a given index should be
    ///
    /// In our normal operation, we sometime need to retrieve a header based on a block hash,
    /// rather than height. Block hashes are 256 bits long, so we can't really use them to index
    /// here. Truncating the sha256 is one option, but this short hash function will give us better
    /// randomization over the data, and it's super easy to compute anyway.
    ///
    /// This hash function is based on the Jenkins hash function with non-zero seed.
    fn index_hash_fn(block_hash: BlockHash) -> u32 {
        let mut hash: u32 = (1 << 16) - 1;

        for i in block_hash.to_byte_array() {
            hash = hash.wrapping_add(i as u32);
            hash = hash.wrapping_add(hash << 10);
            hash ^= hash >> 6;
        }

        hash = hash.wrapping_add(hash << 3);
        hash ^= hash >> 11;
        hash = hash.wrapping_add(hash << 15);

        hash
    }
}

/// The main struct that holds all the context for our flat chain store
///
/// This struct is kept in memory, and it holds multiple memory maps that may or may not be
/// in RAM right now. All functions in the impl block are inherently unsafe, since we're dealing
/// with raw pointers and memory maps. We need to be very careful with them. All methods should be
/// carefully tested and reviewed. This struct is not thread-safe, and it's not meant to be used
/// in multi-threaded environments without proper synchronization.
///
/// We only ever expect one chainstate to hold a [FlatChainStore] at a time. You can then use that
/// chainstate to interact with the chainstore, even in a multi-threaded environment.
pub struct FlatChainStore {
    /// The memory map for our headers
    headers: MmapMut,

    /// The memory map for our metadata
    metadata: MmapMut,

    /// The memory map for our block index
    block_index: BlockIndex,

    /// The memory map for our fork files
    fork_headers: MmapMut,

    /// The file containing the accumulators for each blocks
    accumulator_file: File,

    /// A LRU cache for the last n blocks we've touched
    cache: Mutex<LruCache<BlockHash, DiskBlockHeader>>,
}

impl FlatChainStore {
    /// Creates a new storage, given a configuration
    ///
    /// If any of the I/O operations fail, this function should return an error
    fn create_chain_store(config: FlatChainStoreConfig) -> Result<Self, FlatChainstoreError> {
        let file_mode = config.file_permission.unwrap_or(0o600);
        let dir = &config.path;

        DirBuilder::new().recursive(true).create(dir)?;

        let index_size = config
            .block_index_size
            .map(Self::truncate_to_pow2)
            .unwrap_or(Self::truncate_to_pow2(10_000_000));

        let headers_size = config
            .headers_file_size
            .map(Self::truncate_to_pow2)
            .unwrap_or(Self::truncate_to_pow2(10_000_000));

        let fork_size = config
            .fork_file_size
            .map(Self::truncate_to_pow2)
            .unwrap_or(Self::truncate_to_pow2(10_000));

        let index_path = format!("{dir}/blocks_index.bin");
        let headers_path = format!("{dir}/headers.bin");
        let metadata_path = format!("{dir}/metadata.bin");
        let fork_headers_path = format!("{dir}/fork_headers.bin");
        let accumulator_file_path = format!("{dir}/accumulators.bin");

        let index_map_file_size = index_size * size_of::<u32>();
        let index_map = unsafe { Self::init_file(&index_path, index_map_file_size, file_mode)? };

        let headers_file_size = headers_size * size_of::<HashedDiskHeader>();
        let headers = unsafe { Self::init_file(&headers_path, headers_file_size, file_mode)? };

        let metadata =
            unsafe { Self::init_file(&metadata_path, size_of::<Metadata>(), file_mode)? };

        let fork_headers_file_size = fork_size * size_of::<HashedDiskHeader>();
        let fork_headers =
            unsafe { Self::init_file(&fork_headers_path, fork_headers_file_size, file_mode)? };

        let _metadata = metadata.as_ptr() as *mut Metadata;
        let _metadata = unsafe { &mut *_metadata };

        // init the metadata file
        _metadata.magic = FLAT_CHAINSTORE_MAGIC;
        _metadata.version = FLAT_CHAINSTORE_VERSION;
        _metadata.headers_file_size = headers_size;
        _metadata.fork_file_size = fork_size;
        _metadata.index_capacity = index_size;
        _metadata.block_index_occupancy = 0;
        _metadata.best_block = BlockHash::all_zeros();
        _metadata.depth = 0;
        _metadata.validation_index = BlockHash::all_zeros();
        _metadata.fork_count = 0;
        _metadata
            .alternative_tips
            .copy_from_slice(&[BlockHash::all_zeros(); 64]);

        _metadata.checksum = DbCheckSum {
            headers_checksum: FileChecksum(0),
            index_checksum: FileChecksum(0),
            fork_headers_checksum: FileChecksum(0),
        };

        let cache_size = config.cache_size.and_then(NonZeroUsize::new).unwrap_or(
            NonZeroUsize::new(1000).expect("Infallible: Hard-coded default is always non-zero"),
        );

        let accumulator_file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(accumulator_file_path)?;

        Ok(Self {
            headers,
            accumulator_file,
            metadata,
            block_index: BlockIndex::new(index_map, index_size),
            fork_headers,
            cache: LruCache::new(cache_size).into(),
        })
    }

    /// Opens a new storage. If it already exists, just load. If not, create a new one
    pub fn new(config: FlatChainStoreConfig) -> Result<Self, FlatChainstoreError> {
        let dir = &config.path;
        let metadata_path = format!("{dir}/metadata.bin");
        let file_mode = config.file_permission.unwrap_or(0o600);

        // Maybe migrate our database if it's the old version 0
        migrate_v0_to_v1::maybe_migrate(&metadata_path, file_mode)?;

        // Check if metadata file exists before attempting to load
        let metadata_exists = Path::new(&metadata_path).metadata().is_ok();

        if !metadata_exists {
            // Metadata doesn't exist, create a new chain store
            let mut store = Self::create_chain_store(config)?;
            store.flush()?;
            return Ok(store);
        }

        let metadata_file =
            unsafe { Self::init_file(&metadata_path, size_of::<Metadata>(), file_mode) }?;

        let metadata = metadata_file.as_ptr() as *const Metadata;
        let metadata = unsafe {
            metadata
                .as_ref()
                .ok_or(FlatChainstoreError::InvalidMetadataPointer)?
        };

        // check the magic number and version
        if metadata.version > FLAT_CHAINSTORE_VERSION {
            return Err(FlatChainstoreError::DbTooNew(metadata.version));
        }

        if metadata.magic != FLAT_CHAINSTORE_MAGIC {
            return Err(FlatChainstoreError::InvalidMagic(metadata.magic));
        }

        let index_path = format!("{}/blocks_index.bin", config.path);
        let headers_file_path = format!("{}/headers.bin", config.path);
        let fork_file_path = format!("{}/fork_headers.bin", config.path);
        let accumulator_file_path = format!("{}/accumulators.bin", config.path);

        let index_file_size = metadata.index_capacity * size_of::<u32>();
        let headers_file_size = metadata.headers_file_size * size_of::<HashedDiskHeader>();
        let fork_file_size = metadata.fork_file_size * size_of::<HashedDiskHeader>();

        let index_map = unsafe { Self::init_file(&index_path, index_file_size, file_mode)? };
        let headers = unsafe { Self::init_file(&headers_file_path, headers_file_size, file_mode)? };
        let fork_headers = unsafe { Self::init_file(&fork_file_path, fork_file_size, file_mode)? };
        let cache_size = config.cache_size.and_then(NonZeroUsize::new).unwrap_or(
            NonZeroUsize::new(1000).expect("Infallible: Hard-coded default is always non-zero"),
        );

        let accumulator_file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(accumulator_file_path)?;

        Ok(Self {
            headers,
            accumulator_file,
            metadata: metadata_file,
            block_index: BlockIndex::new(index_map, metadata.index_capacity),
            fork_headers,
            cache: LruCache::new(cache_size).into(),
        })
    }

    /// Adds a new entry into the block index, given a block hash and its `Index`
    ///
    /// This is the only place where we should call `BlockIndex.set_index_for_hash`. Increments
    /// occupancy only if the entry is new; errors if the index map is full.
    unsafe fn add_index_entry(
        &mut self,
        hash: BlockHash,
        index: Index,
    ) -> Result<(), FlatChainstoreError> {
        let metadata = self.get_metadata()?;
        let next_occupancy = metadata.block_index_occupancy + 1;

        if next_occupancy >= metadata.index_capacity {
            return Err(FlatChainstoreError::IndexIsFull);
        }

        let is_new = self
            .block_index
            .set_index_for_hash(hash, index, |index| self.get_disk_header(index).copied())?;

        // Only increment the index occupancy if this is a new entry, i.e., a new block. Otherwise,
        // if this is a reorg, the occupancy is kept the same as we just overwrite indexes.
        if is_new {
            self.get_metadata_mut()?.block_index_occupancy = next_occupancy;
        }

        Ok(())
    }

    /// Checks the integrity of our database
    ///
    /// This function will check the integrity of our database by comparing the checksum of the
    /// headers file, index map, and fork headers file with the checksum stored in the metadata.
    ///
    /// As checksum, the [xxHash] of the memory-mapped region is used. This is a fast hash function that
    /// is very good at detecting errors in memory. It is not cryptographically secure, but it is
    /// enough for random errors in a file.
    ///
    /// [xxHash]: https://github.com/Cyan4973/xxHash
    fn check_integrity(&self) -> Result<(), FlatChainstoreError> {
        let computed_checksum = self.compute_checksum();
        let metadata = unsafe { self.get_metadata()? };

        if metadata.checksum != computed_checksum {
            return Err(FlatChainstoreError::DbCorrupted);
        }

        Ok(())
    }

    /// Computes the XXH3-64 checksum for our database
    pub fn compute_checksum(&self) -> DbCheckSum {
        // a function that computes the xxHash of a memory map
        let checksum_fn = |mmap: &MmapMut| {
            let mmap_as_slice = mmap.iter().as_slice();
            let hash = XxHash3_64::oneshot(mmap_as_slice);

            FileChecksum(hash)
        };

        let headers_checksum = checksum_fn(&self.headers);
        let index_checksum = checksum_fn(&self.block_index.index_map);
        let fork_headers_checksum = checksum_fn(&self.fork_headers);

        DbCheckSum {
            headers_checksum,
            index_checksum,
            fork_headers_checksum,
        }
    }

    /// Truncates a number to the nearest power of 2
    fn truncate_to_pow2(mut n: usize) -> usize {
        if n == 0 {
            return 0;
        }

        n -= 1;
        n |= n >> 1;
        n |= n >> 2;
        n |= n >> 4;
        n |= n >> 8;
        n |= n >> 16;
        n += 1;

        n
    }

    /// Initializes a memory-mapped file with the specified byte size and permissions (mode).
    /// If the underlying file does not exist, it will be created.
    unsafe fn init_file(
        path: &str,
        size: usize,
        _mode: u32,
    ) -> Result<MmapMut, FlatChainstoreError> {
        let file = OpenOptions::new()
            // Set read and write access
            .read(true)
            .write(true)
            // Create the file if it doesn't exist
            .create(true)
            .truncate(false)
            .open(path)?;

        #[cfg(unix)]
        {
            let perm = Permissions::from_mode(_mode);
            file.set_permissions(perm)?;
        }

        file.set_len(size as u64)?;

        // Return the `MmapMut` instance that represents the file
        Ok(MmapOptions::default().len(size).map_mut(&file)?)
    }

    /// Returns a reference to the respective disk header from the file. Errors if nothing is found.
    unsafe fn get_disk_header(
        &self,
        index: Index,
    ) -> Result<&HashedDiskHeader, FlatChainstoreError> {
        let metadata = self.get_metadata()?;
        let (max_size, base_ptr) = match index.is_main_chain() {
            true => (metadata.headers_file_size, self.headers.as_ptr()),
            false => (metadata.fork_file_size, self.fork_headers.as_ptr()),
        };

        let index = index.index() as usize;
        if index >= max_size {
            return Err(FlatChainstoreError::IndexIsFull);
        }

        // SAFETY: we've checked index < max_size
        let ptr = (base_ptr as *const HashedDiskHeader).add(index);
        let header = &*ptr;

        // Uninitialized memory means we haven't written anything here yet
        if header.hash == BlockHash::all_zeros() {
            return Err(FlatChainstoreError::BlockNotFound);
        }

        Ok(header)
    }

    /// Returns a mutable reference to the respective disk header from the file, which may be
    /// uninitialized. This method must only be used for recording a new header or mutating one.
    unsafe fn get_disk_header_mut(
        &mut self,
        index: Index,
    ) -> Result<&mut HashedDiskHeader, FlatChainstoreError> {
        let metadata = self.get_metadata()?;
        let (max_size, base_ptr) = match index.is_main_chain() {
            true => (metadata.headers_file_size, self.headers.as_ptr()),
            false => (metadata.fork_file_size, self.fork_headers.as_ptr()),
        };

        let index = index.index() as usize;
        if index >= max_size {
            return Err(FlatChainstoreError::IndexIsFull);
        }

        // SAFETY: we've checked index < max_size
        let ptr = (base_ptr as *mut HashedDiskHeader).add(index);

        Ok(&mut *ptr)
    }

    unsafe fn do_save_height(&mut self, best_block: &BestChain) -> Result<(), FlatChainstoreError> {
        let metadata = self.get_metadata_mut()?;

        metadata.best_block = best_block.best_block;
        metadata.depth = best_block.depth;
        metadata.validation_index = best_block.validation_index;

        assert!(best_block.alternative_tips.len() <= 64);

        metadata
            .alternative_tips
            .as_mut_ptr()
            .copy_from_nonoverlapping(
                best_block.alternative_tips.as_ptr(),
                best_block.alternative_tips.len(),
            );

        Ok(())
    }

    unsafe fn get_best_chain(&self) -> Result<BestChain, FlatChainstoreError> {
        let metadata = self.get_metadata()?;

        Ok(BestChain {
            best_block: metadata.best_block,
            depth: metadata.depth,
            validation_index: metadata.validation_index,
            alternative_tips: metadata
                .alternative_tips
                .into_iter()
                .take_while(|tip| *tip != BlockHash::all_zeros())
                .collect(),
        })
    }

    /// Returns the block header, given a block hash
    ///
    /// If the header doesn't exist in our index, it'll return an error
    unsafe fn get_header_by_hash(
        &self,
        hash: BlockHash,
    ) -> Result<Option<DiskBlockHeader>, FlatChainstoreError> {
        let result = self
            .block_index
            .get_index_for_hash(hash, |height| self.get_disk_header(height).copied())?
            .map(|idx_and_header| idx_and_header.1);

        Ok(result)
    }

    unsafe fn get_metadata(&self) -> Result<&Metadata, FlatChainstoreError> {
        let ptr = self.metadata.as_ptr() as *const Metadata;

        Ok(ptr
            .as_ref()
            .expect("Infallible: we already validated this pointer"))
    }

    unsafe fn get_metadata_mut(&mut self) -> Result<&mut Metadata, FlatChainstoreError> {
        let ptr = self.metadata.as_ptr() as *mut Metadata;

        Ok(ptr
            .as_mut()
            .expect("Infallible: we already validated this pointer"))
    }

    /// Writes a block header in our storage
    ///
    /// This function will allocate size_of(DiskBlockHeader) bytes in our file and write the raw
    /// header there. May return an error if we can't grow the file
    unsafe fn write_header_to_storage(
        &mut self,
        header: DiskBlockHeader,
    ) -> Result<(), FlatChainstoreError> {
        let height = header
            .try_height()
            .expect("Infallible: this function is only called for best chain blocks");
        let index = Index::new(height)?;

        let pos = self.get_disk_header_mut(index)?;
        *pos = HashedDiskHeader {
            header,
            hash: header.block_hash(),
            // We write the actual values after calling `save_roots_for_block`
            acc_pos: 0,
            acc_len: 0,
        };

        Ok(())
    }

    /// Saves a block that is not in our main chain
    ///
    /// If called for a reorg, we must make sure that the chain is marked as inactive **before**
    /// marking the new chain as active. This happens because we'll write over the old chain inside
    /// the headers file.
    ///
    /// If we mark a chain as Inactive, when we call get_header_by_index it will return the
    /// main chain index. If we override this position in the headers file, we get a different hash.
    /// The algorithm will think that position is occupied with a different header and therefore
    /// keep looking for a vacant position. Therefore, we'll have one stale index that will never
    /// be used.
    ///
    /// When marking a chain active, because we don't overwrite the fork block (we actually call
    /// update_index before saving the actual header), even if we get an Index to the fork block,
    /// it'll return the same hash. Therefore, our find method will return the right entry that
    /// will be overwritten with the new position.
    ///
    /// Here's an example:
    ///
    /// Say we have the following chain:
    ///
    /// ```text
    /// 1 -> 2 -> 3 -> 4 -> 5
    ///      \ -> 3' -> 4'
    /// ```
    ///
    /// If we want to reorg to the fork chain, we must:
    /// 1. Mark the chain [3, 4, 5] as inactive
    /// 2. Mark the chain [3', 4'] as active
    ///
    /// If we do this in the wrong order, when we try to save, e.g. 3. The index will find a
    /// position for 3 in the main chain, and return the position of 3'. Since 3' is different, the
    /// find algorithm will think this position doesn't exist, returning the next vacant position.
    ///
    /// We will write 3 in a new position, and it should work fine. However, we now have a stale 3
    /// that points to the main chain position where it originally was. This will never be used
    /// again, but will occupy a position in the index. Increasing the load factor for no reason.
    unsafe fn save_fork_block(
        &mut self,
        header: DiskBlockHeader,
    ) -> Result<(), FlatChainstoreError> {
        let fork_blocks = self.get_metadata()?.fork_count;
        let index = Index::new_fork(fork_blocks)?;

        let pos = self.get_disk_header_mut(index)?;
        let block_hash = header.block_hash();

        *pos = HashedDiskHeader {
            header,
            hash: block_hash,
            // Fork blocks don't have accumulators, so we set them to 0
            acc_len: 0,
            acc_pos: 0,
        };

        let index = Index::new_fork(fork_blocks)?;
        self.add_index_entry(block_hash, index)?;

        self.get_metadata_mut()?.fork_count += 1;

        Ok(())
    }

    unsafe fn do_flush(&mut self) -> Result<(), FlatChainstoreError> {
        self.headers.flush()?;
        self.block_index.flush()?;
        self.fork_headers.flush()?;

        let checksum = self.compute_checksum();
        let metadata = self.get_metadata_mut()?;

        metadata.checksum = checksum;
        self.metadata.flush()?;
        Ok(())
    }

    #[inline(always)]
    #[doc(hidden)]
    fn get_cache_mut(
        &self,
    ) -> Result<MutexGuard<'_, CacheType>, PoisonError<MutexGuard<'_, CacheType>>> {
        self.cache.lock()
    }
}

impl ChainStore for FlatChainStore {
    type Error = FlatChainstoreError;

    fn check_integrity(&self) -> Result<(), Self::Error> {
        self.check_integrity()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        unsafe { self.do_flush() }
    }

    fn save_roots_for_block(&mut self, roots: Vec<u8>, height: u32) -> Result<(), Self::Error> {
        let index = Index::new(height)?;

        let metadata = unsafe { self.get_metadata()? };
        let validation_index = self
            .get_header(&metadata.validation_index)?
            .map(|h| {
                h.try_height()
                    .map_err(|_| FlatChainstoreError::InvalidValidationIndex)
            })
            .transpose()?
            .unwrap_or(0);

        if height <= validation_index {
            // this is probably a reorg, truncate the file up to the previous block height
            let header = unsafe { self.get_disk_header(index)? };

            // this is where the new acc starts, truncating the file to this position
            let pos = header.acc_pos as u64;

            self.accumulator_file
                .set_len(pos)
                .map_err(FlatChainstoreError::Io)?;
        }

        let pos = self.accumulator_file.seek(SeekFrom::End(0))?;
        let size = roots.len();

        if size > UTREEXO_ACC_SIZE {
            return Err(FlatChainstoreError::AccumulatorTooBig);
        }

        let header = unsafe { self.get_disk_header_mut(index)? };
        // Only write to this header if we actually have it in our store
        if header.hash == BlockHash::all_zeros() {
            return Err(FlatChainstoreError::BlockNotFound);
        }

        header.acc_pos = pos as u32;
        header.acc_len = size as u32;

        self.accumulator_file.write_all(&roots)?;
        self.accumulator_file.flush()?;

        Ok(())
    }

    fn load_roots_for_block(&mut self, height: u32) -> Result<Option<Vec<u8>>, Self::Error> {
        let index = Index::new(height)?;

        let header = unsafe { self.get_disk_header(index)? };
        let size = header.acc_len as usize;

        if size == 0 {
            return Ok(None);
        }

        let mut roots = vec![0; size];

        // move the reading position to a specific position in the file, where
        // that block's accumulator roots are stored
        self.accumulator_file
            .seek(SeekFrom::Start(header.acc_pos as u64))?;

        self.accumulator_file.read_exact(&mut roots)?;

        Ok(Some(roots))
    }

    fn get_header(&self, block_hash: &BlockHash) -> Result<Option<DiskBlockHeader>, Self::Error> {
        let mut cache = self.get_cache_mut()?;

        if let Some(header) = cache.get(block_hash) {
            return Ok(Some(*header));
        }

        let header = unsafe { self.get_header_by_hash(*block_hash)? };
        if let Some(header) = header {
            cache.put(*block_hash, header);
        }

        Ok(header)
    }

    fn get_header_by_height(&self, height: u32) -> Result<Option<DiskBlockHeader>, Self::Error> {
        let index = Index::new(height)?;

        unsafe {
            match self.get_disk_header(index) {
                Ok(header) => Ok(Some(header.header)),
                Err(FlatChainstoreError::BlockNotFound) => Ok(None),
                Err(e) => Err(e),
            }
        }
    }

    fn load_height(&self) -> Result<Option<crate::BestChain>, Self::Error> {
        unsafe {
            self.get_best_chain()
                .map(|x| if x.depth == 0 { None } else { Some(x) })
        }
    }

    fn save_height(&mut self, height: &crate::BestChain) -> Result<(), Self::Error> {
        unsafe { self.do_save_height(height) }
    }

    fn save_header(&mut self, header: &DiskBlockHeader) -> Result<(), Self::Error> {
        let cache = self.get_cache_mut();
        cache?.put(header.block_hash(), *header);

        match header {
            DiskBlockHeader::FullyValid(_, _)
            | DiskBlockHeader::HeadersOnly(_, _)
            | DiskBlockHeader::AssumedValid(_, _) => unsafe {
                self.write_header_to_storage(*header)
            },
            DiskBlockHeader::InFork(_, _)
            | DiskBlockHeader::Orphan(_)
            | DiskBlockHeader::InvalidChain(_) => unsafe { self.save_fork_block(*header) },
        }
    }

    fn get_block_hash(&self, height: u32) -> Result<Option<BlockHash>, Self::Error> {
        let index = Index::new(height)?;

        unsafe {
            match self.get_disk_header(index) {
                Ok(header) => Ok(Some(header.hash)),
                Err(FlatChainstoreError::BlockNotFound) => Ok(None),
                Err(e) => Err(e),
            }
        }
    }

    fn update_block_index(&mut self, height: u32, hash: BlockHash) -> Result<(), Self::Error> {
        let index = Index::new(height)?;

        unsafe { self.add_index_entry(hash, index) }
    }
}

pub mod migrate_v0_to_v1 {
    use std::fs;
    use std::fs::OpenOptions;
    use std::io;
    use std::path::Path;

    use memmap2::Mmap;
    use memmap2::MmapOptions;

    use super::*;

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct MetadataV0 {
        magic: u32,
        version: u32,
        best_block: BlockHash,
        depth: u32,
        validation_index: BlockHash,
        alternative_tips: [BlockHash; 64],
        assume_valid_index: u32, // DEPRECATED FIELD
        fork_count: u32,
        headers_file_size: usize,
        fork_file_size: usize,
        block_index_occupancy: usize,
        index_capacity: usize,
        checksum: DbCheckSum,
    }

    impl From<MetadataV0> for Metadata {
        fn from(value: MetadataV0) -> Self {
            Metadata {
                magic: value.magic,
                version: FLAT_CHAINSTORE_VERSION, // bump version
                best_block: value.best_block,
                depth: value.depth,
                validation_index: value.validation_index,
                alternative_tips: value.alternative_tips,
                // assume_valid_index is skipped
                fork_count: value.fork_count,
                headers_file_size: value.headers_file_size,
                fork_file_size: value.fork_file_size,
                block_index_occupancy: value.block_index_occupancy,
                index_capacity: value.index_capacity,
                checksum: value.checksum,
            }
        }
    }

    /// If `metadata.bin` is exactly the old size, rename to `.bin.old`, mmap it as `MetadataV0`,
    /// then create a fresh v1 file and copy all fields except the deprecated u32. Returns a bool
    /// indicating whether a migration was performed or not.
    pub fn maybe_migrate(path: &str, mode: u32) -> Result<bool, FlatChainstoreError> {
        match fs::metadata(path) {
            // No db found, nothing to migrate from
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            // Propagate the rest of errors
            Err(e) => return Err(e.into()),
            // Found but doesn't have the v0 size
            Ok(meta) if meta.len() != size_of::<MetadataV0>() as u64 => return Ok(false),
            Ok(_) => {}
        }

        // 1) back up
        let backup = Path::new(path).with_extension("bin.old");
        fs::rename(path, &backup)?;

        // 2) read old struct
        let mmap = init_mmap(&backup, size_of::<MetadataV0>())?;
        let old_meta = unsafe { &*(mmap.as_ptr() as *const MetadataV0) };

        // 3) create new v1 mmap
        let mut new_mmap = unsafe { FlatChainStore::init_file(path, size_of::<Metadata>(), mode)? };
        let new_meta = unsafe { &mut *(new_mmap.as_mut_ptr() as *mut Metadata) };

        // 4) copy everything but the deprecated u32, bump version
        *new_meta = Metadata::from(*old_meta);
        info!("Migrated FlatChainStore from v0 to v1!");

        Ok(true)
    }

    /// Initialize a read-only mmap
    pub(super) fn init_mmap(file_path: &Path, size: usize) -> Result<Mmap, FlatChainstoreError> {
        let file = OpenOptions::new().read(true).open(file_path)?;
        let mmap = unsafe { MmapOptions::new().len(size).map(&file)? };
        Ok(mmap)
    }
}

#[cfg(test)]
mod tests {
    use core::mem::size_of;
    use std::fs;
    use std::str::FromStr;

    use bitcoin::block::Header;
    use bitcoin::consensus::deserialize;
    use bitcoin::consensus::Decodable;
    use bitcoin::constants::genesis_block;
    use bitcoin::hashes::Hash;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::Network;
    use floresta_common::bhash;
    use tempfile::TempDir;
    use twox_hash::XxHash3_64;

    use super::FlatChainStore;
    use super::FlatChainStoreConfig;
    use super::FlatChainstoreError;
    use super::Index;
    use super::FLAT_CHAINSTORE_MAGIC;
    use super::FLAT_CHAINSTORE_VERSION;
    use crate::migrate_v0_to_v1::init_mmap;
    use crate::migrate_v0_to_v1::maybe_migrate;
    use crate::pruned_utreexo::flat_chain_store::FileChecksum;
    use crate::pruned_utreexo::flat_chain_store::Metadata;
    use crate::pruned_utreexo::UpdatableChainstate;
    use crate::AssumeValidArg;
    use crate::BestChain;
    use crate::ChainState;
    use crate::ChainStore;
    use crate::DbCheckSum;
    use crate::DiskBlockHeader;

    #[test]
    fn test_truncate_pow2() {
        assert_eq!(FlatChainStore::truncate_to_pow2(1), 1);
        assert_eq!(FlatChainStore::truncate_to_pow2(2), 2);
        assert_eq!(FlatChainStore::truncate_to_pow2(3), 4);
        assert_eq!(FlatChainStore::truncate_to_pow2(4), 4);
        assert_eq!(FlatChainStore::truncate_to_pow2(5), 8);
        assert_eq!(FlatChainStore::truncate_to_pow2(1023), 1024);
        assert_eq!(FlatChainStore::truncate_to_pow2(1024), 1024);
        assert_eq!(FlatChainStore::truncate_to_pow2(1025), 2048);
        assert_eq!(FlatChainStore::truncate_to_pow2(10_000), 16_384);
        assert_eq!(FlatChainStore::truncate_to_pow2(1_000_000), 1_048_576);
        assert_eq!(FlatChainStore::truncate_to_pow2(1_048_576), 1_048_576);
        assert_eq!(FlatChainStore::truncate_to_pow2(1_048_577), 2_097_152);
        assert_eq!(FlatChainStore::truncate_to_pow2(10_000_000), 16_777_216);
        assert_eq!(
            FlatChainStore::truncate_to_pow2(1_000_000_000),
            1_073_741_824
        );
        assert_eq!(
            FlatChainStore::truncate_to_pow2(1_073_741_824),
            1_073_741_824
        );
        assert_eq!(
            FlatChainStore::truncate_to_pow2(1_073_741_825),
            2_147_483_648
        );
    }

    fn get_test_chainstore(id: Option<u64>) -> Result<FlatChainStore, FlatChainstoreError> {
        let test_id = id.unwrap_or_else(rand::random::<u64>);

        let config = FlatChainStoreConfig {
            block_index_size: Some(32_768),
            headers_file_size: Some(32_768),
            fork_file_size: Some(10_000), // Will be rounded up to 16,384
            cache_size: Some(10),
            file_permission: Some(0o660),
            path: format!("./tmp-db/{test_id}/"),
        };

        FlatChainStore::new(config)
    }

    #[test]
    fn test_create_chainstore_from_existing_dir() {
        let id = rand::random::<u64>();
        let path = format!("./tmp-db/{id}/");
        fs::create_dir_all(&path).expect("Failed to create temp dir");
        let _ = get_test_chainstore(Some(id)).expect("Should create a chainstore");
        fs::remove_dir_all(&path).expect("Failed to remove temp dir");
    }

    #[test]
    fn test_create_chainstore() {
        // Helper to tweak the database identifiers while persisting the changes
        fn tweak_version_and_magic(version: u32, magic: u32) -> u64 {
            let store_id = rand::random();
            let mut store = get_test_chainstore(Some(store_id)).unwrap();

            let metadata = unsafe { store.get_metadata_mut().unwrap() };
            metadata.version = version;
            metadata.magic = magic;

            store.flush().unwrap();
            store_id
        }

        {
            let store = get_test_chainstore(None).expect("Should create a chainstore");
            store.check_integrity().unwrap();
        }

        // Tweak the version and check we get the expected error while loading the db
        for version in (FLAT_CHAINSTORE_VERSION + 1)..(FLAT_CHAINSTORE_VERSION + 32) {
            let store_id = tweak_version_and_magic(version, FLAT_CHAINSTORE_MAGIC);

            match get_test_chainstore(Some(store_id)) {
                Err(FlatChainstoreError::DbTooNew(v)) if v == version => {},
                Err(e) => panic!("Should have failed with `FlatChainstoreError::DbTooNew({version})`, instead we got {e:?}"),
                Ok(_) => panic!("Should have failed with `FlatChainstoreError::DbTooNew({version})`, instead we got `Ok`"),
            }
        }

        // Tweak the magic number and check we get the expected error while loading the db
        for magic in std::iter::repeat_with(rand::random)
            .filter(|m| *m != FLAT_CHAINSTORE_MAGIC)
            .take(32)
        {
            let store_id = tweak_version_and_magic(FLAT_CHAINSTORE_VERSION, magic);

            match get_test_chainstore(Some(store_id)) {
                Err(FlatChainstoreError::InvalidMagic(m)) if m == magic => {},
                Err(e) => panic!("Should have failed with `FlatChainstoreError::InvalidMagic({magic})`, instead we got {e:?}"),
                Ok(_) => panic!("Should have failed with `FlatChainstoreError::InvalidMagic({magic})`, instead we got `Ok`"),
            }
        }
    }

    #[test]
    fn test_migrate_v0_to_v1() {
        let tmp = TempDir::new().expect("Failed to create temp dir");
        // Sanity check
        let did_migrate = maybe_migrate(tmp.path().to_str().unwrap(), 0o600).unwrap();
        assert!(!did_migrate, "Should not migrate: empty directory");

        // Copy the v0 metadata file to the temporal directory (we will rename it to .bin.old)
        let src = "./testdata/v0_flat_metadata.bin";
        let dst = tmp.path().join("metadata.bin");
        fs::copy(src, &dst).unwrap();

        let did_migrate = maybe_migrate(dst.to_str().unwrap(), 0o600).unwrap();
        assert!(did_migrate, "Expected a migration to happen");

        // Check that the length is now the expected one
        let new_len = fs::metadata(&dst).unwrap().len();
        assert_eq!(new_len, size_of::<Metadata>() as u64);

        // Mmap the file and read the metadata
        let mmap = init_mmap(&dst, size_of::<Metadata>()).unwrap();
        let metadata = unsafe { &*(mmap.as_ptr() as *const Metadata) };

        let expected = Metadata {
            magic: FLAT_CHAINSTORE_MAGIC,
            version: FLAT_CHAINSTORE_VERSION,
            best_block: bhash!("000000004f74d42205b9d7ae1cb5c1591e723894a71358fce73b7e0919628161"),
            depth: 10_236,
            validation_index: bhash!(
                "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
            ),
            alternative_tips: [BlockHash::all_zeros(); 64],
            fork_count: 0,
            headers_file_size: 32_768,
            fork_file_size: 16_384,
            block_index_occupancy: 10_236,
            index_capacity: 32_768,
            checksum: DbCheckSum {
                headers_checksum: FileChecksum(4430534975455841125),
                index_checksum: FileChecksum(9017076313313378046),
                fork_headers_checksum: FileChecksum(10728257719073392722),
            },
        };

        assert_eq!(metadata, &expected);
    }

    #[test]
    // Sanity check
    fn test_checksum() {
        assert_eq!(XxHash3_64::oneshot("a".as_bytes()), 0xe6c632b61e964e1f);
        assert_eq!(XxHash3_64::oneshot("abc1".as_bytes()), 0xec035b7226cacedf);
        assert_eq!(XxHash3_64::oneshot("abc 1".as_bytes()), 0x5740573263e9d84d);
        assert_eq!(
            XxHash3_64::oneshot("Floresta".as_bytes()),
            0x066d384879d98e84
        );
        assert_eq!(
            XxHash3_64::oneshot("floresta".as_bytes()),
            0x58d9f8aa416ed680
        );
        assert_eq!(
            XxHash3_64::oneshot("floresta-chain".as_bytes()),
            0x066540290fdae363
        );
    }

    #[test]
    fn test_save_and_retrieve_headers() {
        let mut store = get_test_chainstore(None).unwrap();
        let blocks = include_str!("../../testdata/blocks.txt");

        for (i, line) in blocks.lines().enumerate() {
            let block = hex::decode(line).unwrap();
            let block: Block = deserialize(&block).unwrap();

            store
                .save_header(&DiskBlockHeader::FullyValid(block.header, i as u32))
                .unwrap();
            // Map hashes to block indices such that we can later fetch headers from hashes
            // (hash -> index -> header)
            store
                .update_block_index(i as u32, block.block_hash())
                .unwrap();
        }

        for i in 0..151 {
            // Reads the disk header at index `i` directly from the file
            let header = store.get_header_by_height(i).unwrap().unwrap();
            // Reads the header hash at index `i` directly from the file
            let hash = store.get_block_hash(i).unwrap().unwrap();

            // Gets the header via the LRU cache, or else via hash -> index -> header
            let header_by_hash_cached = store.get_header(&hash).unwrap().unwrap();
            // Gets the header via hash -> index -> header
            let header_by_hash = unsafe { store.get_header_by_hash(hash).unwrap().unwrap() };

            assert_eq!(header, header_by_hash);
            assert_eq!(header, header_by_hash_cached);
            assert_eq!(
                header.block_hash(),
                hash,
                "Returned header matches the hash"
            );
            assert_eq!(
                i,
                header.try_height().unwrap(),
                "Returned header has the correct height"
            );

            if i == 0 {
                // Must be the regtest genesis header
                let regtest_genesis = genesis_block(Network::Regtest);
                assert_eq!(regtest_genesis.block_hash(), hash);
                assert_eq!(regtest_genesis.header, *header);
            }
        }

        match store.get_block_hash(151).unwrap() {
            None => (),
            Some(hash) => panic!("Should not have found a block at height 151, hash: {hash}"),
        }
        match store.get_header_by_height(151).unwrap() {
            None => (),
            Some(header) => panic!("Should not have found a header at height 151: {header:?}"),
        }
        match store.get_header(&BlockHash::all_zeros()).unwrap() {
            None => (),
            Some(header) => {
                panic!("Should not have found a header with hash all_zeros: {header:?}")
            }
        }

        // Test that the inner header-fetching function returns the proper error for mainnet indices
        unsafe {
            match store.get_disk_header(Index::new(151).unwrap()) {
                Err(FlatChainstoreError::BlockNotFound) => (),
                Err(e) => panic!("Unexpected err: {e:?}"),
                Ok(val) => panic!("Should not have found a header at height 151: {val:?}"),
            }
            // Last available position
            match store.get_disk_header(Index::new(32_767).unwrap()) {
                Err(FlatChainstoreError::BlockNotFound) => (),
                Err(e) => panic!("Unexpected err: {e:?}"),
                Ok(val) => panic!("Should not have found a header at height 32767: {val:?}"),
            }
            // Exceeds header file capacity
            match store.get_disk_header(Index::new(32_768).unwrap()) {
                Err(FlatChainstoreError::IndexIsFull) => (),
                Err(e) => panic!("Unexpected err: {e:?}"),
                Ok(val) => {
                    panic!("Should not have found a header exceeding file capacity: {val:?}")
                }
            }
        }

        // Test that the inner header-fetching function returns the proper error for fork indices
        unsafe {
            match store.get_disk_header(Index::new_fork(0).unwrap()) {
                Err(FlatChainstoreError::BlockNotFound) => (),
                Err(e) => panic!("Unexpected err: {e:?}"),
                Ok(val) => panic!("Should not have found any fork header: {val:?}"),
            }
            // Last available position
            match store.get_disk_header(Index::new_fork(16_383).unwrap()) {
                Err(FlatChainstoreError::BlockNotFound) => (),
                Err(e) => panic!("Unexpected err: {e:?}"),
                Ok(val) => panic!("Should not have found any fork header: {val:?}"),
            }
            // Exceeds fork file capacity
            match store.get_disk_header(Index::new_fork(16_384).unwrap()) {
                Err(FlatChainstoreError::IndexIsFull) => (),
                Err(e) => panic!("Unexpected err: {e:?}"),
                Ok(val) => {
                    panic!("Should not have found a header exceeding file capacity: {val:?}")
                }
            }
        }
    }

    #[test]
    fn test_save_height() {
        let mut store = get_test_chainstore(None).unwrap();
        let height = BestChain {
            alternative_tips: Vec::new(),
            validation_index: genesis_block(Network::Signet).block_hash(),
            depth: 1,
            best_block: genesis_block(Network::Signet).block_hash(),
        };

        store.save_height(&height).unwrap();

        let recovered = store.load_height().unwrap().unwrap();
        assert_eq!(recovered, height);
    }

    #[test]
    fn test_index() {
        let mut store = get_test_chainstore(None).unwrap();
        let mut hashes = Vec::new();
        let blocks = include_str!("../../testdata/blocks.txt");

        for (i, line) in blocks.lines().enumerate() {
            let block = hex::decode(line).unwrap();
            let block: Block = deserialize(&block).unwrap();
            hashes.push(block.block_hash());

            store
                .save_header(&DiskBlockHeader::FullyValid(block.header, i as u32))
                .unwrap();

            store
                .update_block_index(i as u32, block.block_hash())
                .unwrap();
        }

        for hash in hashes {
            if hash == genesis_block(Network::Regtest).block_hash() {
                continue;
            }
            let header = store.get_header(&hash).unwrap().unwrap();

            assert_eq!(header.block_hash(), hash);
        }
    }

    #[test]
    fn accept_mainnet_headers() {
        // Accepts the first 10235 mainnet headers
        let file = include_bytes!("../../testdata/headers.zst");
        let uncompressed: Vec<u8> = zstd::decode_all(std::io::Cursor::new(file)).unwrap();
        let store = get_test_chainstore(None).unwrap();
        let chain = ChainState::new(store, Network::Bitcoin, AssumeValidArg::Hardcoded);
        let mut buffer = uncompressed.as_slice();

        while let Ok(header) = Header::consensus_decode(&mut buffer) {
            chain.accept_header(header).unwrap();
        }
    }

    #[test]
    fn accept_first_signet_headers() {
        // Accepts the first 2016 signet headers
        let file = include_bytes!("../../testdata/signet_headers.zst");
        let uncompressed: Vec<u8> = zstd::decode_all(std::io::Cursor::new(file)).unwrap();
        let store = get_test_chainstore(None).unwrap();
        let chain = ChainState::new(store, Network::Signet, AssumeValidArg::Hardcoded);
        let mut buffer = uncompressed.as_slice();

        while let Ok(header) = Header::consensus_decode(&mut buffer) {
            chain.accept_header(header).unwrap();
        }
    }

    #[test]
    fn test_fork_blocks() {
        let mut store = get_test_chainstore(None).unwrap();
        let file = include_str!("../../testdata/blocks.txt");
        let headers = file
            .lines()
            .map(|x| hex::decode(x).unwrap())
            .collect::<Vec<_>>();

        let (blocks, forks) = headers.split_at(headers.len() / 2);
        for (i, block) in blocks.iter().enumerate() {
            let block: Block = deserialize(block).unwrap();

            store
                .save_header(&DiskBlockHeader::FullyValid(block.header, i as u32))
                .unwrap();

            store
                .update_block_index(i as u32, block.block_hash())
                .unwrap();
        }

        let mut hashes = Vec::new();
        for (i, block) in forks.iter().enumerate() {
            let block: Block = deserialize(block).unwrap();

            hashes.push(block.block_hash());

            store
                .save_header(&DiskBlockHeader::InFork(block.header, i as u32))
                .unwrap();
        }

        for hash in hashes {
            let header = store.get_header(&hash).unwrap().unwrap();

            assert_eq!(header.block_hash(), hash);
        }
    }

    #[test]
    fn test_recover_acc() {
        let test_id = rand::random::<u64>();
        let mut store = get_test_chainstore(Some(test_id)).unwrap();

        // Save the genesis block and the best chain data
        let genesis = genesis_block(Network::Regtest);
        store
            .save_header(&DiskBlockHeader::FullyValid(genesis.header, 0))
            .unwrap();
        store.update_block_index(0, genesis.block_hash()).unwrap();

        store
            .save_height(&BestChain {
                best_block: genesis.block_hash(),
                depth: 0,
                validation_index: genesis.block_hash(),
                alternative_tips: vec![],
            })
            .unwrap();

        let acc = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        // Test saving and recovering the dummy accumulator for the genesis block
        store.save_roots_for_block(acc.clone(), 0).unwrap();
        store.flush().unwrap();

        let recovered = store.load_roots_for_block(0).unwrap().unwrap();
        assert_eq!(recovered, acc);

        drop(store);

        let mut store = get_test_chainstore(Some(test_id)).unwrap();

        let recovered = store.load_roots_for_block(0).unwrap().unwrap();
        assert_eq!(recovered, acc);

        // If we try to save the accumulator for a block that we don't have, it fails
        let result = store.save_roots_for_block(acc.clone(), 10);

        match result {
            Err(FlatChainstoreError::BlockNotFound) => (),
            Err(e) => panic!("Unexpected err: {e:?}"),
            Ok(_) => panic!("Should not have been able to save roots for a block we don't have"),
        }
    }
}
