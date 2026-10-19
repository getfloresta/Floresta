// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rolling window of per-block fee rates used for fee estimation.
//!
//! `FeeEstimation` is the pure domain policy: it keeps the last `window`
//! per-block fee rates and answers `estimate` with a median.
//! [`FeeRateStore`] is the outbound port used to persist those rates, and
//! [`BlockFeeEstimator`] is the application service that turns block-connected
//! events into updates of the window.

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

use bitcoin::FeeRate;
use spin::RwLock;
use tracing::warn;

use super::chain_state::BlockConnected;
use super::chain_state::BlockConsumer;
use crate::DatabaseError;

const FEE_ESTIMATION_WINDOW: usize = 1008;
const FEE_ESTIMATION_SHORT: usize = 6; // target 1
const FEE_ESTIMATION_MEDIUM: usize = 30; // target 10

/// Tunable parameters of the fee estimation policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeEstimationPolicy {
    /// Number of per-block fee rates kept in the rolling window.
    pub window: usize,
    /// Window size used to answer short-horizon targets (1 block).
    pub short: usize,
    /// Window size used to answer medium-horizon targets (up to 10 blocks).
    pub medium: usize,
    /// Lower bound applied to every estimate.
    pub min: FeeRate,
}

impl Default for FeeEstimationPolicy {
    fn default() -> Self {
        Self {
            window: FEE_ESTIMATION_WINDOW,
            short: FEE_ESTIMATION_SHORT,
            medium: FEE_ESTIMATION_MEDIUM,
            min: FeeRate::BROADCAST_MIN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeRateEntry {
    pub height: u32,
    pub fee_rate: FeeRate,
}

/// Read-side port used by protocol adapters (Electrum, RPC) to answer fee queries.
pub trait FeeEstimator: Send + Sync + 'static {
    /// Returns the estimated fee rate to be included within `target` blocks.
    fn estimate_fee(&self, target: usize) -> FeeRate;
}

/// Narrow outbound port for persisting per-block fee rates.
pub trait FeeRateStore: Send + Sync + 'static {
    type Error: DatabaseError;
    fn save(&self, entry: FeeRateEntry) -> Result<(), Self::Error>;
    fn get(&self, height: u32) -> Result<Option<FeeRateEntry>, Self::Error>;
}

pub(crate) struct FeeEstimation {
    policy: FeeEstimationPolicy,
    rates: VecDeque<FeeRateEntry>,
    estimates: (FeeRate, FeeRate, FeeRate),
}

impl FeeEstimation {
    pub(crate) fn new(policy: FeeEstimationPolicy) -> Self {
        Self {
            policy,
            rates: VecDeque::new(),
            estimates: (policy.min, policy.min, policy.min),
        }
    }

    /// reorg guard(drop above height) + push + trim(window) + estimates recalc
    pub(crate) fn push(&mut self, height: u32, rate: Option<FeeRate>) {
        //drop above height
        while self.rates.back().is_some_and(|e| e.height >= height) {
            self.rates.pop_back();
        }
        if let Some(fee_rate) = rate {
            self.rates.push_back(FeeRateEntry { height, fee_rate });
        }

        //trim by window
        while self.rates.len() > self.policy.window {
            self.rates.pop_front();
        }

        let rates: Vec<FeeRate> = self.rates.iter().map(|e| e.fee_rate).collect();

        if !rates.is_empty() {
            self.estimates = (
                Self::median(&rates[rates.len().saturating_sub(self.policy.short)..]),
                Self::median(&rates[rates.len().saturating_sub(self.policy.medium)..]),
                Self::median(rates.as_slice()),
            )
        }
    }

    pub(crate) fn estimate(&self, target: usize) -> FeeRate {
        let rate = match target {
            0..=1 => self.estimates.0,
            2..=10 => self.estimates.1,
            _ => self.estimates.2,
        };
        rate.max(self.policy.min)
    }

    /// Iterates over the window in chronological order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = FeeRateEntry> + '_ {
        self.rates.iter().copied()
    }

    pub(crate) fn median(data: &[FeeRate]) -> FeeRate {
        if data.is_empty() {
            return FeeRate::ZERO;
        }
        let mut sorted = data.to_vec();
        sorted.sort_unstable();
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 0 {
            FeeRate::from_sat_per_kwu(u64::midpoint(
                sorted[mid - 1].to_sat_per_kwu(),
                sorted[mid].to_sat_per_kwu(),
            ))
        } else {
            sorted[mid]
        }
    }
}

/// Application service maintaining a persisted rolling window of per-block fee
/// rates. Subscribe it to a [`ChainState`](crate::ChainState) to feed it with
/// connected blocks, and query it through the [`FeeEstimator`] port.
///
/// While the node is in initial block download it only buffers rates in memory;
/// the window is written once, when IBD finishes. Afterwards every block is
/// persisted as it is connected.
pub struct BlockFeeEstimator<S: FeeRateStore> {
    state: RwLock<FeeEstimation>,
    store: S,
    /// When true, each new rate is written through to the store immediately.
    /// It is false while the node is in IBD.
    live: AtomicBool,
}

impl<S: FeeRateStore> BlockFeeEstimator<S> {
    /// Rebuilds the in-memory window from the store, then returns the service.
    pub fn load(store: S, policy: FeeEstimationPolicy, tip: u32) -> Result<Self, S::Error> {
        let mut state = FeeEstimation::new(policy);
        let start = tip.saturating_sub(policy.window as u32 - 1);

        for height in start..=tip {
            if let Some(entry) = store.get(height)? {
                state.push(entry.height, Some(entry.fee_rate));
            }
        }

        Ok(Self {
            state: RwLock::new(state),
            store,
            // Default to write-through; `on_ibd_changed(true)` turns it off.
            live: AtomicBool::new(true),
        })
    }

    /// Records the average fee rate of the block at `height`.
    pub(crate) fn record(&self, height: u32, rate: Option<FeeRate>) {
        self.state.write().push(height, rate);

        if self.live.load(Ordering::Relaxed) {
            self.persist(height, rate);
        }
    }

    fn persist(&self, height: u32, rate: Option<FeeRate>) {
        if let Some(fee_rate) = rate {
            let entry = FeeRateEntry { height, fee_rate };
            if let Err(e) = self.store.save(entry) {
                warn!("Failed to persist fee rate for height {height}: {e}");
            }
        }
    }

    /// Writes the whole in-memory window to the store in a single pass.
    fn flush(&self) {
        let entries: Vec<FeeRateEntry> = self.state.read().entries().collect();
        for entry in entries {
            if let Err(e) = self.store.save(entry) {
                warn!("Failed to flush fee rate window: {e}");
            }
        }
    }
}

impl<S: FeeRateStore> FeeEstimator for BlockFeeEstimator<S> {
    fn estimate_fee(&self, target: usize) -> FeeRate {
        self.state.read().estimate(target)
    }
}

impl<S: FeeRateStore> BlockConsumer for BlockFeeEstimator<S> {
    fn wants_spent_utxos(&self) -> bool {
        false
    }

    fn wants_block_stats(&self) -> bool {
        true
    }

    fn on_connected(&self, evt: BlockConnected<'_>) {
        let Some(stats) = evt.stats else {
            return;
        };
        self.record(evt.height, stats.avg_fee_rate());
    }

    fn on_ibd_changed(&self, in_ibd: bool) {
        if in_ibd {
            // IBD: keep the window in memory, skip the per-block store writes.
            self.live.store(false, Ordering::Relaxed);
        } else {
            // Synced: enable write-through first so a concurrent `record` is
            // not lost, then flush the accumulated window once.
            self.live.store(true, Ordering::Relaxed);
            self.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::FeeRate;

    use super::FeeEstimation;
    use super::FeeEstimationPolicy;

    fn fr(n: u64) -> FeeRate {
        FeeRate::from_sat_per_kwu(n)
    }

    #[test]
    fn test_median() {
        let cases: [(&[u64], u64); 6] = [
            (&[], 0),
            (&[42], 42),
            (&[1, 3, 2], 2),
            (&[1, 4, 2, 3], 2),
            (&[7, 7, 7], 7),
            (&[u64::MAX / 2, u64::MAX / 2 + 2], u64::MAX / 2 + 1),
        ];

        for (input, expected) in cases {
            let data: Vec<FeeRate> = input.iter().copied().map(fr).collect();
            assert_eq!(
                FeeEstimation::median(&data),
                fr(expected),
                "input = {input:?}"
            );
        }
    }

    #[test]
    fn test_median_empty() {
        let data: &[FeeRate] = &[];
        assert_eq!(FeeEstimation::median(data), FeeRate::ZERO)
    }

    #[test]
    fn test_median_large_values_even() {
        let large = u64::MAX / 2;
        assert_eq!(
            FeeEstimation::median(&[fr(large), fr(large + 2)]),
            fr(large + 1)
        );
    }

    #[test]
    fn test_median_large_values_odd() {
        let large = u64::MAX / 2;
        assert_eq!(
            FeeEstimation::median(&[fr(large), fr(large + 2), fr(large + 1)]),
            fr(large + 1)
        );
    }

    #[test]
    fn test_estimate_fee() {
        let mut w = FeeEstimation::new(FeeEstimationPolicy::default());
        w.estimates = (fr(300), fr(400), fr(500));

        let cases: [(usize, FeeRate); 7] = [
            (0, fr(300)),
            (1, fr(300)),
            (2, fr(400)),
            (5, fr(400)),
            (10, fr(400)),
            (11, fr(500)),
            (1000, fr(500)),
        ];

        for (target, expected) in cases {
            assert_eq!(w.estimate(target), expected, "target = {target}");
        }
    }

    #[test]
    fn test_estimate_fee_floors_at_broadcast_min() {
        let mut w = FeeEstimation::new(FeeEstimationPolicy::default());

        w.estimates = (FeeRate::ZERO, FeeRate::ZERO, FeeRate::ZERO);
        assert_eq!(w.estimate(1), FeeRate::BROADCAST_MIN);
        assert_eq!(w.estimate(10), FeeRate::BROADCAST_MIN);
        assert_eq!(w.estimate(100), FeeRate::BROADCAST_MIN);
    }

    #[test]
    fn test_update_fee_estimation() {
        let mut w = FeeEstimation::new(FeeEstimationPolicy::default());

        for h in 0..=59u32 {
            w.push(h, Some(fr(h as u64)));
        }

        let (t0, t1, t2) = w.estimates;
        assert_eq!(t0, fr(56));
        assert_eq!(t1, fr(44));
        assert_eq!(t2, fr(29));
    }
}

#[cfg(test)]
mod estimator_tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use bitcoin::FeeRate;

    use super::BlockConsumer;
    use super::BlockFeeEstimator;
    use super::FeeEstimationPolicy;
    use super::FeeEstimator;
    use super::FeeRateEntry;
    use super::FeeRateStore;
    use crate::DatabaseError;

    #[derive(Debug)]
    struct TestError;

    impl core::fmt::Display for TestError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "test store error")
        }
    }

    impl DatabaseError for TestError {}

    #[derive(Default)]
    struct MemoryStore {
        entries: Mutex<BTreeMap<u32, FeeRateEntry>>,
    }

    impl FeeRateStore for MemoryStore {
        type Error = TestError;

        fn save(&self, entry: FeeRateEntry) -> Result<(), Self::Error> {
            self.entries.lock().unwrap().insert(entry.height, entry);
            Ok(())
        }

        fn get(&self, height: u32) -> Result<Option<FeeRateEntry>, Self::Error> {
            Ok(self.entries.lock().unwrap().get(&height).copied())
        }
    }

    fn fr(n: u64) -> FeeRate {
        FeeRate::from_sat_per_kwu(n)
    }

    #[test]
    fn floors_below_min_and_persists() {
        // A rate below the policy floor is raised to `min`.
        let est =
            BlockFeeEstimator::load(MemoryStore::default(), FeeEstimationPolicy::default(), 0)
                .unwrap();
        est.record(1, Some(fr(100)));
        assert_eq!(est.estimate_fee(1), FeeRate::BROADCAST_MIN);

        // A rate above the floor is used as-is and persisted.
        let est =
            BlockFeeEstimator::load(MemoryStore::default(), FeeEstimationPolicy::default(), 0)
                .unwrap();
        est.record(1, Some(fr(1000)));
        assert_eq!(est.estimate_fee(1), fr(1000));
        assert_eq!(
            est.store.get(1).unwrap(),
            Some(FeeRateEntry {
                height: 1,
                fee_rate: fr(1000)
            })
        );
    }

    #[test]
    fn empty_window_floors_at_policy_min() {
        let store = MemoryStore::default();
        let est = BlockFeeEstimator::load(store, FeeEstimationPolicy::default(), 0).unwrap();

        assert_eq!(est.estimate_fee(1), FeeRate::BROADCAST_MIN);
        assert_eq!(est.estimate_fee(100), FeeRate::BROADCAST_MIN);
    }

    #[test]
    fn buffers_during_ibd_and_flushes_on_done() {
        let est =
            BlockFeeEstimator::load(MemoryStore::default(), FeeEstimationPolicy::default(), 0)
                .unwrap();

        // Enter IBD: rates update the in-memory window but are not persisted.
        est.on_ibd_changed(true);
        for h in 1..=5u32 {
            est.record(h, Some(fr(1000)));
        }
        assert_eq!(est.estimate_fee(1), fr(1000));
        assert_eq!(est.store.get(1).unwrap(), None);
        assert_eq!(est.store.get(5).unwrap(), None);

        // IBD done: the whole window is flushed once.
        est.on_ibd_changed(false);
        for h in 1..=5u32 {
            assert_eq!(
                est.store.get(h).unwrap(),
                Some(FeeRateEntry {
                    height: h,
                    fee_rate: fr(1000)
                })
            );
        }

        // After IBD, new rates are written through immediately.
        est.record(6, Some(fr(2000)));
        assert_eq!(
            est.store.get(6).unwrap(),
            Some(FeeRateEntry {
                height: 6,
                fee_rate: fr(2000)
            })
        );
    }

    #[test]
    fn load_recovers_window_from_store() {
        let store = MemoryStore::default();
        for h in 10..=20u32 {
            store
                .save(FeeRateEntry {
                    height: h,
                    fee_rate: fr(500),
                })
                .unwrap();
        }

        let est = BlockFeeEstimator::load(store, FeeEstimationPolicy::default(), 20).unwrap();
        assert_eq!(est.estimate_fee(1), fr(500));
    }
}
