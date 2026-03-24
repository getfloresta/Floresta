use std::collections::BTreeMap;

use rustreexo::node_hash::BitcoinNodeHash;
use rustreexo::proof::Proof;
use rustreexo::stump::Stump;
use rustreexo::stump::StumpError;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

/// Utreexo additions stored without empty hashes.
///
/// During SwiftSync, spent TXOs are represented by empty hashes, so omitting them saves memory.
pub struct SparseUtreexoAdds {
    /// Total number of additions, including omitted empty hashes.
    len: usize,

    /// Non-empty hashes and their positions in the full list.
    entries: Vec<(u32, BitcoinNodeHash)>,
}

impl SparseUtreexoAdds {
    /// Builds sparse additions by omitting empty hashes from a full list.
    ///
    /// Panics if a position exceeds `u32::MAX`. This cannot happen for block outputs.
    pub fn new(dense: Vec<BitcoinNodeHash>) -> Self {
        let len = dense.len();
        let entries = dense
            .into_iter()
            .enumerate()
            .filter_map(|(index, hash)| {
                if hash == BitcoinNodeHash::Empty {
                    return None;
                }

                let index = u32::try_from(index).expect("block output positions fit in a u32");

                Some((index, hash))
            })
            .collect();

        Self { len, entries }
    }

    /// Rebuilds the full list by restoring the omitted empty hashes.
    pub fn into_dense(self) -> Vec<BitcoinNodeHash> {
        let mut dense = vec![BitcoinNodeHash::Empty; self.len];

        for (index, hash) in self.entries {
            dense[index as usize] = hash;
        }

        dense
    }
}

/// Pending additions, deletions, and proof for a single accumulator update.
pub struct StumpUpdate {
    pub adds: SparseUtreexoAdds,
    pub deletes: Vec<BitcoinNodeHash>,
    pub proof: Proof,
}

pub type StumpResult = Result<Stump, StumpError>;

/// Handle for interacting with a running [`StumpUpdater`] task.
///
/// The caller must send exactly one update for each height in `initial_height + 1..=stop_height`.
/// Sending stale, duplicate, or out-of-range heights, or dropping `tx` before `stop_height` is
/// reached, is invalid usage and will close `done` without a result.
pub struct StumpUpdaterHandle {
    /// Sender side for feeding `(height, update_data)` into the updater task.
    pub tx: mpsc::UnboundedSender<(u32, StumpUpdate)>,

    /// Receiver for the final accumulator at `stop_height`, or any early update error.
    pub done: oneshot::Receiver<StumpResult>,
}

/// The `StumpUpdater` struct is responsible for managing the state and updates for an utreexo
/// [`Stump`] accumulator, applying updates sequentially.
///
/// This type enables out-of-order block processing, since we decouple accumulator updates from
/// block processing. It will cache all the data needed to update the accumulator (adds, deletes,
/// proofs) and consume it sequentially.
///
/// The channel will be used to send the final accumulator to the consumer, if successful, or to
/// notify accumulator update failures.
pub struct StumpUpdater {
    /// The accumulator for `last_height`.
    last_acc: Stump,

    /// The last height we have processed. This is always incremented by 1, iff we have the update
    /// data for the next height.
    last_height: u32,

    /// Pending additions, deletions, and proofs to apply to the accumulator, mapped to the height
    /// at which they must be applied.
    pending_updates: BTreeMap<u32, StumpUpdate>,
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
        };

        tokio::task::spawn_blocking(move || {
            let result = updater.run(rx, stop_height);
            let _ = done_tx.send(result);
        });

        StumpUpdaterHandle { tx, done: done_rx }
    }

    /// Queues one future update, rejecting stale, out-of-range, or duplicate heights.
    fn queue_update(&mut self, height: u32, update: StumpUpdate, stop_height: u32) {
        let last_height = self.last_height;

        // Sanity check: we shouldn't receive updates for already-processed heights
        if height <= last_height || height > stop_height {
            panic!("got update height {height}, but last={last_height}, stop={stop_height}");
        }

        // When we insert the new pending update, it shouldn't be duplicated
        if self.pending_updates.insert(height, update).is_some() {
            panic!("duplicate update data at height {height}");
        }
    }

    fn run(
        mut self,
        mut rx: mpsc::UnboundedReceiver<(u32, StumpUpdate)>,
        stop_height: u32,
    ) -> StumpResult {
        while self.last_height < stop_height {
            // Wait until a new state update arrives
            let Some((height, update)) = rx.blocking_recv() else {
                panic!(
                    "updater channel closed at height {} before {stop_height}",
                    self.last_height,
                )
            };

            self.queue_update(height, update, stop_height);
            self.try_next()?;
        }

        // If we exit the while loop, we have reached the stop height
        Ok(self.last_acc)
    }

    /// Loops over all pending updates that we can sequentially apply, consuming the data and
    /// updating `last_acc` and `last_height`.
    ///
    /// Returns on the first missing update data that is next in the sequence, or on update errors.
    fn try_next(&mut self) -> Result<(), StumpError> {
        loop {
            let next_height = self.last_height + 1;

            // Since `pending_updates` is ordered by height, the first entry is the only
            // update that can advance the accumulator. If it is not `next_height`,
            // there is a gap, so we must wait for more update data.
            let StumpUpdate {
                adds,
                deletes,
                proof,
            } = match self.pending_updates.first_entry() {
                Some(entry) if *entry.key() == next_height => entry.remove(),
                _ => break,
            };

            // Fill the spent TXO positions with empty hashes
            let adds = adds.into_dense();

            self.last_acc = self.last_acc.modify(&adds, &deletes, &proof)?;
            self.last_height = next_height;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use floresta_common::assert_err;
    use tokio::time::timeout;

    use super::*;

    fn hash(value: u8) -> BitcoinNodeHash {
        BitcoinNodeHash::Some([value; 32])
    }

    fn dummy_update() -> StumpUpdate {
        StumpUpdate {
            adds: SparseUtreexoAdds::new(Vec::new()),
            deletes: Vec::new(),
            proof: Proof::default(),
        }
    }

    async fn assert_worker_closed(done: oneshot::Receiver<StumpResult>) {
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

        tx.send((
            1,
            StumpUpdate {
                adds: SparseUtreexoAdds::new(dense),
                deletes: Vec::new(),
                proof: Proof::default(),
            },
        ))
        .unwrap();

        let final_acc = timeout(Duration::from_secs(1), done)
            .await
            .unwrap()
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

        tx.send((10, dummy_update())).unwrap();

        assert_worker_closed(done).await;
    }

    #[tokio::test]
    async fn run_closes_done_if_height_is_lower_than_last_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 10, 12);

        tx.send((9, dummy_update())).unwrap();

        assert_worker_closed(done).await;
    }

    #[tokio::test]
    async fn run_closes_done_if_height_is_above_stop_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 10, 12);

        tx.send((13, dummy_update())).unwrap();

        assert_worker_closed(done).await;
    }

    #[tokio::test]
    async fn run_closes_done_on_duplicate_height() {
        let StumpUpdaterHandle { tx, done } = StumpUpdater::spawn(Stump::new(), 0, 3);

        tx.send((2, dummy_update())).unwrap();
        tx.send((2, dummy_update())).unwrap();

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
}
