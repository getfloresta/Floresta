// TODO: remove once we use this module for SwiftSync
#![allow(dead_code, unused_imports)]

use std::collections::BTreeMap;
use std::time::Duration;
use std::time::Instant;

use rustreexo::node_hash::AccumulatorHash;
use rustreexo::node_hash::BitcoinNodeHash;
use rustreexo::proof::Proof;
use rustreexo::stump::Stump;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::info;

/// Utreexo additions stored without empty hashes.
///
/// During SwiftSync, spent TXOs are represented by empty hashes in the dense form. The sparse
/// form omits those hashes while they aren't needed to save significant memory. Once the full
/// Utreexo additions are needed, the omitted `BitcoinNodeHash::Empty` variants are restored.
pub struct SparseUtreexoAdds {
    /// Total number of additions, including omitted empty hashes.
    len: usize,

    /// Non-empty hashes and their positions in the full list.
    entries: Vec<(u32, BitcoinNodeHash)>,
}

impl SparseUtreexoAdds {
    /// Returns the number of non-empty addition hashes, i.e., the UTXO count.
    fn utxo_count(&self) -> usize {
        self.entries.len()
    }

    /// Builds sparse additions by omitting empty hashes from a full list.
    ///
    /// Panics if a position exceeds `u32::MAX`. This cannot happen for block outputs.
    pub fn new(dense: Vec<BitcoinNodeHash>) -> Self {
        let len = dense.len();
        let entries = dense
            .into_iter()
            .enumerate()
            .filter(|(_, hash)| !hash.is_empty())
            .map(|(i, hash)| {
                let index = u32::try_from(i).expect("block output positions fit in a u32");
                (index, hash)
            })
            .collect();

        Self { len, entries }
    }

    /// Rebuilds the full addition list by restoring the omitted empty hashes.
    pub fn into_dense(self) -> Vec<BitcoinNodeHash> {
        let mut dense = vec![BitcoinNodeHash::Empty; self.len];

        for (index, hash) in self.entries {
            let index = usize::try_from(index).expect("usize is at least 32 bits");

            // Indices come from the original dense list, so they are always < `self.len`
            dense[index] = hash;
        }

        dense
    }
}

/// Handle for interacting with a running [`StumpUpdater`] task.
///
/// The caller must send exactly one update for each height in `initial_height + 1..=stop_height`.
/// Sending stale, duplicate, or out-of-range heights, or dropping `tx` before `stop_height` is
/// reached, is invalid usage and will close `done` without a result.
pub struct StumpUpdaterHandle {
    /// Sender side for feeding `(height, update_data)` into the updater task.
    pub tx: mpsc::UnboundedSender<(u32, SparseUtreexoAdds)>,

    /// Receiver for the final accumulator at `stop_height`.
    pub done: oneshot::Receiver<Stump>,
}

/// The `StumpUpdater` struct is responsible for managing the state and updates for an utreexo
/// [`Stump`] accumulator, applying updates sequentially.
///
/// This type enables out-of-order block processing, since we decouple accumulator updates from
/// block processing. It will cache all the data needed to update the accumulator (proofless, so
/// just additions with implicit deletion) and consume it sequentially.
///
/// The channel will be used to send the final accumulator to the consumer.
pub struct StumpUpdater {
    /// The accumulator for `last_height`.
    last_acc: Stump,

    /// The last height we have processed. This is always incremented by 1, iff we have the update
    /// data for the next height.
    last_height: u32,

    /// Pending additions to apply to the accumulator at each height.
    pending_updates: BTreeMap<u32, SparseUtreexoAdds>,

    // Telemetry fields
    /// Total time spent applying accumulator updates.
    total_time: Duration,

    /// Number of UTXO hashes currently cached in pending updates.
    cached_utxos: usize,

    /// Maximum number of UTXO hashes cached at once.
    max_cached_utxos: usize,
}

impl StumpUpdater {
    pub fn spawn(initial_acc: Stump, initial_height: u32, stop_height: u32) -> StumpUpdaterHandle {
        assert!(
            initial_height < stop_height,
            "initial `StumpUpdater` height must be less than `stop_height`",
        );

        let (tx, rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = oneshot::channel();

        // Initial state and empty updates cache
        let updater = Self {
            last_acc: initial_acc,
            last_height: initial_height,
            pending_updates: BTreeMap::new(),
            total_time: Duration::ZERO,
            cached_utxos: 0,
            max_cached_utxos: 0,
        };

        tokio::task::spawn_blocking(move || {
            let result = updater.run(rx, stop_height);
            let _ = done_tx.send(result);
        });

        StumpUpdaterHandle { tx, done: done_rx }
    }

    /// Queues one future update, rejecting stale, out-of-range, or duplicate heights.
    fn queue_update(&mut self, height: u32, update: SparseUtreexoAdds, stop_height: u32) {
        let last_height = self.last_height;

        // Sanity check: we shouldn't receive updates for already-processed heights
        if height <= last_height || height > stop_height {
            panic!("got update height {height}, but last={last_height}, stop={stop_height}");
        }

        let new_utxos = update.utxo_count();

        // When we insert the new pending update, it shouldn't be duplicated
        if self.pending_updates.insert(height, update).is_some() {
            panic!("duplicate update data at height {height}");
        }

        self.cached_utxos += new_utxos;
        self.max_cached_utxos = self.max_cached_utxos.max(self.cached_utxos);
    }

    fn run(
        mut self,
        mut rx: mpsc::UnboundedReceiver<(u32, SparseUtreexoAdds)>,
        stop_height: u32,
    ) -> Stump {
        while self.last_height < stop_height {
            // Wait until a new state update arrives
            let Some((height, update)) = rx.blocking_recv() else {
                panic!(
                    "updater channel closed at height {} before {stop_height}",
                    self.last_height,
                )
            };

            self.queue_update(height, update, stop_height);
            self.try_next();
        }

        // If we exit the while loop, we have reached the stop height
        info!(
            "Stump updater took {:.3}s to apply all accumulator updates. Maximum cached UTXO hashes: {}.",
            self.total_time.as_secs_f64(),
            self.max_cached_utxos,
        );

        self.last_acc
    }

    /// Loops over all pending updates that we can sequentially apply, consuming the data and
    /// updating `last_acc` and `last_height`.
    ///
    /// Returns on the first missing update data that is next in the sequence.
    fn try_next(&mut self) {
        loop {
            let next_height = self.last_height + 1;

            // Since `pending_updates` is ordered by height, the first entry is the only
            // update that can advance the accumulator. If it is not `next_height`,
            // there is a gap, so we must wait for more update data.
            let adds = match self.pending_updates.first_entry() {
                Some(entry) if *entry.key() == next_height => entry.remove(),
                _ => break,
            };

            // Update the count by removing the UTXOs we consumed from the cache
            self.cached_utxos -= adds.utxo_count();
            let adds = adds.into_dense();

            let start = Instant::now();
            let new_acc = self
                .last_acc
                .modify(&adds, &[], &Proof::default())
                .expect("addition-only stump modification cannot fail");
            self.total_time += start.elapsed();

            self.last_acc = new_acc;
            self.last_height = next_height;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bitcoin::Block;
    use bitcoin::consensus::encode::deserialize_hex;
    use floresta_chain::proof_util;
    use floresta_common::acchashes;
    use floresta_common::assert_err;
    use tokio::time::timeout;

    use super::*;

    fn hash(value: u8) -> BitcoinNodeHash {
        BitcoinNodeHash::Some([value; 32])
    }

    fn empty_adds() -> SparseUtreexoAdds {
        SparseUtreexoAdds::new(Vec::new())
    }

    async fn assert_worker_closed(done: oneshot::Receiver<Stump>) {
        let done_result = timeout(Duration::from_secs(1), done).await.unwrap();
        assert_err!(done_result);
    }

    #[test]
    fn sparse_utreexo_adds_round_trip_to_dense() {
        let dense = vec![
            BitcoinNodeHash::Empty,
            hash(1),
            BitcoinNodeHash::Empty,
            hash(2),
        ];

        let sparse = SparseUtreexoAdds::new(dense.clone());

        assert_eq!(sparse.into_dense(), dense);
    }

    #[tokio::test]
    async fn run_applies_sparse_additions() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 0, 1);
        let dense = vec![
            BitcoinNodeHash::Empty,
            hash(1),
            BitcoinNodeHash::Empty,
            hash(2),
        ];

        tx.send((1, SparseUtreexoAdds::new(dense))).unwrap();

        let final_acc = timeout(Duration::from_secs(1), done)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(final_acc.leaves, 4);
    }

    #[tokio::test]
    async fn run_closes_done_if_channel_closes_before_stop_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 0, 1);

        drop(tx);

        assert_worker_closed(done).await;
    }

    #[tokio::test]
    async fn run_closes_done_if_height_is_equal_to_last_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 10, 12);

        tx.send((10, empty_adds())).unwrap();

        assert_worker_closed(done).await;
    }

    #[tokio::test]
    async fn run_closes_done_if_height_is_lower_than_last_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 10, 12);

        tx.send((9, empty_adds())).unwrap();

        assert_worker_closed(done).await;
    }

    #[tokio::test]
    async fn run_closes_done_if_height_is_above_stop_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 10, 12);

        tx.send((13, empty_adds())).unwrap();

        assert_worker_closed(done).await;
    }

    #[tokio::test]
    async fn run_closes_done_on_duplicate_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 0, 3);

        tx.send((2, empty_adds())).unwrap();
        tx.send((2, empty_adds())).unwrap();

        assert_worker_closed(done).await;
    }

    #[test]
    #[should_panic]
    fn spawn_panics_if_initial_height_is_not_below_stop_height() {
        for h in 0..5 {
            let _ = StumpUpdater::spawn(Stump::new(), h, 5);
        }

        let _ = StumpUpdater::spawn(Stump::new(), 5, 5);
    }

    #[test]
    #[should_panic]
    fn spawn_panics_if_initial_height_is_above_stop_height() {
        let _ = StumpUpdater::spawn(Stump::new(), 6, 5);
    }

    /// Builds a mainnet stump from additions delivered in reverse height order.
    async fn run_mainnet_additions(stop_height: u32, empty_add_height: Option<u32>) -> Stump {
        let blocks: Vec<Block> =
            include_str!("../../../floresta-chain/testdata/mainnet_blocks.txt")
                .lines()
                .take(stop_height as usize + 1)
                .map(|block| deserialize_hex(block).unwrap())
                .collect();

        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 0, stop_height);

        // Send every update in reverse order
        for height in (1..=stop_height).rev() {
            let block = &blocks[height as usize];
            let mut adds = proof_util::get_block_adds(block, height, block.block_hash());

            if empty_add_height == Some(height) {
                assert_eq!(adds.len(), 1, "this test assumes a single spent TXO");
                adds[0] = BitcoinNodeHash::Empty;
            }

            tx.send((height, SparseUtreexoAdds::new(adds))).unwrap();
        }

        timeout(Duration::from_secs(5), done)
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn mainnet_additions_only_out_of_order() {
        // Height 170 contains the first spend, so stop at 169
        let actual = run_mainnet_additions(169, None).await;

        let expected = Stump {
            leaves: 169,
            roots: acchashes![
                "69482b799cf46ed514b01ce0573730a89c537018636b8c52a8864d5968b917f3",
                "53c92fa0792c9af1c19793b1149e7fe209c69b320ea054338f53f8fd8535f2e8",
                "6096c8421c1f86a9caa26e972dccdb964e280164fb060a576d51f5844e259569",
                "fd46029ebb0c19e2d468a9b24d20519c64ccc342e6a32b95c86a57489b6d2504",
            ]
            .to_vec(),
        };

        assert_eq!(actual, expected);
    }

    // TODO: uncomment when rustreexo supports implicit deletion
    /*
    #[tokio::test]
    async fn mainnet_additions_with_implicit_deletion_out_of_order() {
        // Block 9's sole addition is spent at height 170
        let actual = run_mainnet_additions(175, Some(9)).await;

        let expected = Stump {
            leaves: 177,
            roots: acchashes![], // TODO: expected roots at height 175
        };

        assert_eq!(actual, expected);
    }
    */
}
