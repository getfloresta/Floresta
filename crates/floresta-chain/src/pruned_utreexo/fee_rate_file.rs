// SPDX-License-Identifier: MIT OR Apache-2.0

//! File-backed storage adapter for per-block fee rates.
//!
//! `FeeRateFile` is a fixed-size circular buffer of [`FeeRateEntry`] slots, keyed
//! by `height % capacity`. It implements the narrow [`FeeRateStore`] port so the
//! fee estimation use-case never has to know about the generic `ChainStore`.

extern crate std;

use core::mem::size_of;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use bitcoin::FeeRate;
use zerocopy::FromBytes;
use zerocopy::Immutable;
use zerocopy::IntoBytes;
use zerocopy::KnownLayout;
use zerocopy::Unaligned;
use zerocopy::byteorder::LE;
use zerocopy::byteorder::U32;
use zerocopy::byteorder::U64;

use crate::pruned_utreexo::fee_estimation::FeeRateEntry;
use crate::pruned_utreexo::fee_estimation::FeeRateStore;
use crate::pruned_utreexo::flat_chain_store::FlatChainstoreError;

#[cfg(feature = "flat-chainstore")]
#[derive(Clone, Copy, Debug, IntoBytes, FromBytes, Immutable, KnownLayout, Unaligned)]
#[repr(C)]
pub(crate) struct DiskFeeRateEntry {
    /// height + 1, 0 = empty
    height_tag: U32<LE>,
    /// sat/kwu
    fee_rate: U64<LE>,
}

#[cfg(feature = "flat-chainstore")]
const _: () = assert!(FeeRateEntry::ENCODED_SIZE == 12);
#[cfg(feature = "flat-chainstore")]
const _: () = assert!(size_of::<U32<LE>>() == 4);
#[cfg(feature = "flat-chainstore")]
const _: () = assert!(size_of::<U64<LE>>() == 8);

/// File-backed implementation of [`FeeRateStore`].
///
/// The backing file holds `capacity` fixed-size slots; `height % capacity`
/// selects a slot. A stale slot (from `capacity` blocks earlier) is rejected on
/// read because its height tag no longer matches.
pub struct FeeRateFile {
    file: Mutex<File>,
    capacity: u32,
}

impl FeeRateEntry {
    /// On-disk record size, in bytes. Single source of truth for the slot layout.
    #[cfg(feature = "flat-chainstore")]
    pub const ENCODED_SIZE: usize = size_of::<DiskFeeRateEntry>();

    #[cfg(feature = "flat-chainstore")]
    #[inline]
    /// Slot height tag: stored as `height + 1`, so the zero-initialized value 0
    /// means "empty". `saturating_add` avoids wrapping to 0 at `u32::MAX`.
    fn height_tag(height: u32) -> u32 {
        // Keep `u32::MAX` from wrapping into the 0 empty sentinel. probably no reach in my life
        height.saturating_add(1)
    }

    #[cfg(feature = "flat-chainstore")]
    pub(crate) fn encode(self) -> DiskFeeRateEntry {
        DiskFeeRateEntry {
            height_tag: U32::new(Self::height_tag(self.height)),
            fee_rate: U64::new(self.fee_rate.to_sat_per_kwu()),
        }
    }

    #[cfg(feature = "flat-chainstore")]
    pub(crate) fn decode(rec: DiskFeeRateEntry, height: u32) -> Option<Self> {
        if rec.height_tag.get() != Self::height_tag(height) {
            return None;
        }
        Some(Self {
            height,
            fee_rate: FeeRate::from_sat_per_kwu(rec.fee_rate.get()),
        })
    }
}

impl FeeRateFile {
    pub fn open(path: impl AsRef<Path>, capacity: u32) -> std::io::Result<Self> {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.set_len(capacity as u64 * FeeRateEntry::ENCODED_SIZE as u64)?;
        Ok(Self {
            file: Mutex::new(file),
            capacity,
        })
    }

    /// Number of slots in the ring buffer.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

impl FeeRateStore for FeeRateFile {
    type Error = FlatChainstoreError;

    fn save(&self, entry: FeeRateEntry) -> Result<(), Self::Error> {
        let pos = (entry.height % self.capacity) as u64 * FeeRateEntry::ENCODED_SIZE as u64;
        let mut f = self.file.lock().expect("fee rate file lock poisoned");
        f.seek(SeekFrom::Start(pos))?;
        f.write_all(entry.encode().as_bytes())?;
        Ok(())
    }

    fn get(&self, height: u32) -> Result<Option<FeeRateEntry>, Self::Error> {
        let pos = (height % self.capacity) as u64 * FeeRateEntry::ENCODED_SIZE as u64;
        let mut f = self.file.lock().expect("fee rate file lock poisoned");

        let file_len = f.metadata()?.len();
        if pos + FeeRateEntry::ENCODED_SIZE as u64 > file_len {
            return Ok(None);
        }

        let mut buf = [0u8; FeeRateEntry::ENCODED_SIZE];
        f.seek(SeekFrom::Start(pos))?;
        f.read_exact(&mut buf)?;

        let rec = DiskFeeRateEntry::read_from_bytes(&buf).expect("buffer is exactly ENCODED_SIZE");
        Ok(FeeRateEntry::decode(rec, height))
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::FeeRate;
    use tempfile::TempDir;
    use zerocopy::FromBytes;

    use super::DiskFeeRateEntry;
    use super::FeeRateEntry;
    use super::FeeRateFile;
    use super::FeeRateStore;

    /// Slot count used by the tests.
    const W: u32 = 1008;

    fn fr(n: u64) -> FeeRate {
        FeeRate::from_sat_per_kwu(n)
    }

    fn fee_entry(height: u32, sat_per_kwu: u64) -> FeeRateEntry {
        FeeRateEntry {
            height,
            fee_rate: fr(sat_per_kwu),
        }
    }

    /// Opens a store backed by a fresh temp dir. The dir handle must outlive it.
    fn open_store() -> (FeeRateFile, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = FeeRateFile::open(dir.path().join("fee_rates.bin"), W).unwrap();
        (store, dir)
    }

    #[test]
    fn test_fee_rate_save_and_get() {
        let (store, _dir) = open_store();

        store.save(fee_entry(0, 12345)).unwrap();
        store.save(fee_entry(1, 67890)).unwrap();
        store.save(fee_entry(100, u64::MAX)).unwrap();

        assert_eq!(store.get(0).unwrap(), Some(fee_entry(0, 12345)));
        assert_eq!(store.get(1).unwrap(), Some(fee_entry(1, 67890)));
        assert_eq!(store.get(100).unwrap(), Some(fee_entry(100, u64::MAX)));
    }

    #[test]
    fn test_fee_rate_missing_block_returns_none() {
        let (store, _dir) = open_store();

        assert_eq!(store.get(2).unwrap(), None);
        assert_eq!(store.get(99999).unwrap(), None);
        // a fresh/empty slot (0) must not look like height 0
        assert_eq!(store.get(0).unwrap(), None);
    }

    #[test]
    fn test_fee_rate_max_height_saturates() {
        let (store, _dir) = open_store();

        // u32::MAX saturates instead of wrapping to the 0 sentinel
        store.save(fee_entry(u32::MAX, 777)).unwrap();
        assert_eq!(store.get(u32::MAX).unwrap(), Some(fee_entry(u32::MAX, 777)));

        // the adjacent unsaved slot stays empty
        assert_eq!(store.get(u32::MAX - 1).unwrap(), None);
    }

    #[test]
    fn test_fee_rate_slot_encode_decode() {
        let entry = FeeRateEntry {
            height: 42,
            fee_rate: fr(1500),
        };

        // roundtrip
        let buf = entry.encode();
        assert_eq!(FeeRateEntry::decode(buf, 42), Some(entry));

        // a slot only matches its own height
        assert_eq!(FeeRateEntry::decode(buf, 43), None);

        // a zero-initialized slot is the "empty" sentinel, even for height 0
        let empty = DiskFeeRateEntry::read_from_bytes(&[0u8; FeeRateEntry::ENCODED_SIZE]).unwrap();
        assert_eq!(FeeRateEntry::decode(empty, 0), None);

        // saturating tag keeps u32::MAX distinct from the sentinel
        let max = FeeRateEntry {
            height: u32::MAX,
            fee_rate: fr(7),
        }
        .encode();
        assert_eq!(
            FeeRateEntry::decode(max, u32::MAX),
            Some(FeeRateEntry {
                height: u32::MAX,
                fee_rate: fr(7),
            })
        );
    }

    #[test]
    fn test_fee_rate_overwrite() {
        let (store, _dir) = open_store();

        store.save(fee_entry(5, 111)).unwrap();
        store.save(fee_entry(5, 222)).unwrap();
        assert_eq!(store.get(5).unwrap(), Some(fee_entry(5, 222)));
    }

    #[test]
    fn test_fee_rate_stale_slot_is_rejected() {
        let (store, _dir) = open_store();

        store.save(fee_entry(5, 999)).unwrap();

        // same slot, one window later: the height tag no longer matches
        let h = W + 5;
        assert_eq!(store.get(h).unwrap(), None);

        store.save(fee_entry(h, 111)).unwrap();
        assert_eq!(store.get(h).unwrap(), Some(fee_entry(h, 111)));
    }
}
