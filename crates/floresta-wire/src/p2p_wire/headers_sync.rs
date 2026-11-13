// SPDX-License-Identifier: MIT OR Apache-2.0

//! Header pre-synchronization and checkpoint-range verification.
//!
//! PRESYNC validates proof of work, difficulty transitions, and continuity without writing
//! headers to disk. It retains one full header checkpoint every 20,000 blocks. Once a peer
//! demonstrates the network's minimum chainwork, adjacent checkpoints become independent ranges.
//!
//! REDOWNLOAD assigns those ranges to multiple peers. Each range is buffered and validated in
//! memory until its ending checkpoint hash is reproduced. The caller then accepts the range into
//! the chain in height order through `accept_header`.

use bitcoin::BlockHash;
use bitcoin::CompactTarget;
use bitcoin::Network;
use bitcoin::Target;
use bitcoin::Work;
use bitcoin::block::Header;
use bitcoin::consensus::params::Params;
use tracing::warn;

/// Distance between full hashes retained during PRESYNC.
///
/// This is also the size of one REDOWNLOAD range, about 1.6 MB of headers held in memory until
/// the range completes. Smaller ranges keep memory low and let fast peers take more of the work,
/// so a slow peer only delays one small piece.
pub(crate) const CHECKPOINT_INTERVAL: u32 = 20_000;

/// Generous memory bound: 24,000 hashes cover 480 million headers.
const MAX_CHECKPOINTS: usize = 24_000;

/// Stateless check that `new_bits` is a permitted successor to `old_bits` at `height`.
///
/// Mirrors Bitcoin Core's `PermittedDifficultyTransition` (pow.cpp): at a retarget
/// boundary the target may move by at most 4x in either direction (capped at the
/// network's proof-of-work limit); between boundaries it must not change at all.
/// Needs no timestamps, so it can run on headers that are not yet in storage.
fn permitted_difficulty_transition(
    params: &Params,
    height: u32,
    old_bits: CompactTarget,
    new_bits: CompactTarget,
) -> bool {
    // Min-difficulty networks (testnet3/4, regtest) may reset to the limit at any height.
    if params.allow_min_difficulty_blocks {
        return true;
    }

    if u64::from(height) % params.difficulty_adjustment_interval() != 0 {
        return old_bits == new_bits;
    }

    let old_target = Target::from_compact(old_bits);
    let observed = Target::from_compact(new_bits);
    let max_target = Target::from_compact(
        old_target
            .max_transition_threshold(params)
            .to_compact_lossy(),
    );
    if observed > max_target {
        return false;
    }

    let min_target = Target::from_compact(old_target.min_transition_threshold().to_compact_lossy());
    observed >= min_target
}

/// The phase of a peer's header pre-synchronization state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresyncPhase {
    /// Validating headers without writing them to disk and recording checkpoints.
    Presync,
    /// Minimum work was demonstrated; checkpoint ranges are ready for parallel redownload.
    Redownload,
    /// The checkpoint memory bound was exceeded. This is not peer misbehavior.
    Aborted,
}

/// A full hash retained at a known height during PRESYNC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeaderCheckpoint {
    pub height: u32,
    pub hash: BlockHash,
    pub bits: CompactTarget,
}

/// An independently verifiable range between two PRESYNC checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeaderRange {
    pub start: HeaderCheckpoint,
    pub end: HeaderCheckpoint,
}

impl HeaderRange {
    pub fn header_count(&self) -> u32 {
        self.end.height - self.start.height
    }
}

/// Result of processing one PRESYNC response.
#[derive(Debug, Default)]
pub struct ProcessingResult {
    /// `false` for invalid proof of work, difficulty, or continuity.
    pub success: bool,
}

/// Result of processing a response for one checkpoint range.
#[derive(Debug)]
pub(crate) enum RangeProgress {
    /// The range is valid so far but needs another response.
    InProgress,
    /// The complete range reproduced its ending checkpoint.
    Complete(Vec<Header>),
    /// The response violated the range's continuity, difficulty, PoW, or endpoint.
    Invalid,
}

/// Per-peer state for the disk-free PRESYNC pass.
#[derive(Debug, Clone)]
pub struct HeadersSyncState {
    state: PresyncPhase,
    pub start_hash: BlockHash,
    start_bits: CompactTarget,
    consensus_params: Params,
    minimum_chain_work: Work,
    /// Total chain work up to the last validated header, starting from the start header's own
    /// total work like Core, not from zero.
    presync_work: Work,
    presync_height: u32,
    presync_last_header: Option<Header>,
    checkpoint_interval: u32,
    max_checkpoints: usize,
    checkpoints: Vec<HeaderCheckpoint>,
}

impl HeadersSyncState {
    /// Creates a PRESYNC state rooted at a header already known by the chain backend.
    ///
    /// `start_work` is the total chain work up to `start`, so a node that already holds part of
    /// the chain only has to be shown the remaining work.
    pub fn new(
        start: HeaderCheckpoint,
        start_work: Work,
        minimum_chain_work: Work,
        network: Network,
    ) -> Self {
        Self {
            state: PresyncPhase::Presync,
            start_hash: start.hash,
            start_bits: start.bits,
            consensus_params: Params::from(network),
            minimum_chain_work,
            presync_work: start_work,
            presync_height: start.height,
            presync_last_header: None,
            checkpoint_interval: CHECKPOINT_INTERVAL,
            max_checkpoints: MAX_CHECKPOINTS,
            checkpoints: vec![start],
        }
    }

    pub fn phase(&self) -> &PresyncPhase {
        &self.state
    }

    #[cfg(test)]
    pub(crate) fn presync_height(&self) -> u32 {
        self.presync_height
    }

    fn record_checkpoint(&mut self, checkpoint: HeaderCheckpoint) -> bool {
        if self
            .checkpoints
            .last()
            .is_some_and(|last| last.height == checkpoint.height)
        {
            return true;
        }
        if self.checkpoints.len() >= self.max_checkpoints {
            warn!(
                height = checkpoint.height,
                "Peer chain exceeded presync checkpoint cap; aborting presync"
            );
            self.state = PresyncPhase::Aborted;
            return false;
        }
        self.checkpoints.push(checkpoint);
        true
    }

    /// Validates one batch and records a full hash at each checkpoint boundary.
    pub fn process_presync(&mut self, headers: &[Header]) -> ProcessingResult {
        if self.state != PresyncPhase::Presync {
            return ProcessingResult { success: false };
        }

        for header in headers {
            let expected_prev = self
                .presync_last_header
                .as_ref()
                .map(Header::block_hash)
                .unwrap_or(self.start_hash);
            if header.prev_blockhash != expected_prev {
                return ProcessingResult { success: false };
            }

            let previous_bits = self
                .presync_last_header
                .as_ref()
                .map(|header| header.bits)
                .unwrap_or(self.start_bits);
            let Some(next_height) = self.presync_height.checked_add(1) else {
                return ProcessingResult { success: false };
            };
            if !permitted_difficulty_transition(
                &self.consensus_params,
                next_height,
                previous_bits,
                header.bits,
            ) || header.validate_pow(header.target()).is_err()
            {
                return ProcessingResult { success: false };
            }

            self.presync_work = self.presync_work + header.work();
            self.presync_height = next_height;
            self.presync_last_header = Some(*header);

            if next_height % self.checkpoint_interval == 0
                && !self.record_checkpoint(HeaderCheckpoint {
                    height: next_height,
                    hash: header.block_hash(),
                    bits: header.bits,
                })
            {
                return ProcessingResult { success: true };
            }
        }

        if self.presync_work >= self.minimum_chain_work {
            if let Some(last) = self.presync_last_header {
                if !self.record_checkpoint(HeaderCheckpoint {
                    height: self.presync_height,
                    hash: last.block_hash(),
                    bits: last.bits,
                }) {
                    return ProcessingResult { success: true };
                }
                self.state = PresyncPhase::Redownload;
                return ProcessingResult { success: true };
            }
        }

        ProcessingResult { success: true }
    }

    /// Returns adjacent checkpoint ranges after PRESYNC demonstrates minimum work.
    pub(crate) fn redownload_ranges(&self) -> Option<Vec<HeaderRange>> {
        (self.state == PresyncPhase::Redownload).then(|| {
            self.checkpoints
                .windows(2)
                .map(|window| HeaderRange {
                    start: window[0],
                    end: window[1],
                })
                .collect()
        })
    }

    pub fn next_locator_hash(&self) -> Option<BlockHash> {
        (self.state == PresyncPhase::Presync)
            .then(|| self.presync_last_header.as_ref().map(Header::block_hash))
            .flatten()
    }

    #[cfg(test)]
    fn with_checkpoint_interval(mut self, interval: u32) -> Self {
        assert!(interval > 0);
        self.checkpoint_interval = interval;
        self
    }

    #[cfg(test)]
    fn with_max_checkpoints(mut self, maximum: usize) -> Self {
        self.max_checkpoints = maximum;
        self
    }
}

/// Buffers and verifies one range before it is written at its known height.
#[derive(Debug, Clone)]
pub(crate) struct HeadersRangeDownload {
    range: HeaderRange,
    consensus_params: Params,
    next_height: u32,
    next_hash: BlockHash,
    previous_bits: CompactTarget,
    headers: Vec<Header>,
}

impl HeadersRangeDownload {
    pub fn new(range: HeaderRange, network: Network) -> Self {
        Self {
            range,
            consensus_params: Params::from(network),
            next_height: range.start.height,
            next_hash: range.start.hash,
            previous_bits: range.start.bits,
            headers: Vec::with_capacity(range.header_count() as usize),
        }
    }

    pub fn range(&self) -> HeaderRange {
        self.range
    }

    pub fn next_locator_hash(&self) -> BlockHash {
        self.next_hash
    }

    pub fn stop_hash(&self) -> BlockHash {
        self.range.end.hash
    }

    pub fn process(&mut self, incoming: &[Header]) -> RangeProgress {
        if incoming.is_empty() {
            return RangeProgress::Invalid;
        }

        for header in incoming {
            let Some(height) = self.next_height.checked_add(1) else {
                return RangeProgress::Invalid;
            };
            if height > self.range.end.height
                || header.prev_blockhash != self.next_hash
                || !permitted_difficulty_transition(
                    &self.consensus_params,
                    height,
                    self.previous_bits,
                    header.bits,
                )
                || header.validate_pow(header.target()).is_err()
            {
                return RangeProgress::Invalid;
            }

            self.next_height = height;
            self.next_hash = header.block_hash();
            self.previous_bits = header.bits;
            self.headers.push(*header);
        }

        if self.next_height == self.range.end.height {
            if self.next_hash != self.range.end.hash {
                return RangeProgress::Invalid;
            }
            return RangeProgress::Complete(std::mem::take(&mut self.headers));
        }

        RangeProgress::InProgress
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::TxMerkleNode;
    use bitcoin::block::Version;
    use bitcoin::hashes::Hash;

    use super::*;

    const EASY_BITS: u32 = 0x207f_ffff;
    const ANCIENT_TIMESTAMP: u32 = 1_000_000;

    fn easy_bits() -> CompactTarget {
        CompactTarget::from_consensus(EASY_BITS)
    }

    fn make_header_with_bits(prev: BlockHash, time: u32, bits: CompactTarget) -> Header {
        (0u32..=u32::MAX)
            .map(|nonce| Header {
                version: Version::from_consensus(1),
                prev_blockhash: prev,
                merkle_root: TxMerkleNode::all_zeros(),
                time,
                bits,
                nonce,
            })
            .find(|header| header.validate_pow(header.target()).is_ok())
            .expect("easy target should produce a valid nonce")
    }

    fn make_header(prev: BlockHash, time: u32) -> Header {
        make_header_with_bits(prev, time, easy_bits())
    }

    fn no_work() -> Work {
        Work::from_be_bytes([0; 32])
    }

    fn unreachable_work() -> Work {
        Work::from_be_bytes([0xff; 32])
    }

    /// A PRESYNC state starting at height 0 from an all-zeros hash with easy bits.
    fn presync_from_zero(start_work: Work, minimum_chain_work: Work) -> HeadersSyncState {
        let start = HeaderCheckpoint {
            height: 0,
            hash: BlockHash::all_zeros(),
            bits: easy_bits(),
        };
        HeadersSyncState::new(start, start_work, minimum_chain_work, Network::Regtest)
    }

    fn make_chain(prev: BlockHash, time: u32, count: usize) -> Vec<Header> {
        let mut headers = Vec::with_capacity(count);
        let mut prev = prev;
        for offset in 0..count {
            let header = make_header(prev, time + offset as u32);
            prev = header.block_hash();
            headers.push(header);
        }
        headers
    }

    #[test]
    fn difficulty_transition_off_boundary_requires_unchanged_bits() {
        let params = Params::from(Network::Bitcoin);
        let old = CompactTarget::from_consensus(0x1b04_04cb);
        let other = CompactTarget::from_consensus(0x1b04_04cc);
        assert!(permitted_difficulty_transition(&params, 1, old, old));
        assert!(!permitted_difficulty_transition(&params, 1, old, other));
    }

    #[test]
    fn difficulty_transition_at_boundary_allows_at_most_four_x() {
        let params = Params::from(Network::Bitcoin);
        let old = CompactTarget::from_consensus(0x1b04_04cb);
        let target = Target::from_compact(old);
        let easiest = target.max_transition_threshold(&params).to_compact_lossy();
        let hardest = target.min_transition_threshold().to_compact_lossy();
        assert!(permitted_difficulty_transition(&params, 2016, old, easiest));
        assert!(permitted_difficulty_transition(&params, 2016, old, hardest));
        assert!(!permitted_difficulty_transition(
            &params,
            2016,
            old,
            CompactTarget::from_consensus(0x1c04_04cb)
        ));
    }

    #[test]
    fn min_difficulty_networks_allow_difficulty_changes() {
        let params = Params::from(Network::Regtest);
        assert!(permitted_difficulty_transition(
            &params,
            1,
            CompactTarget::from_consensus(0x1b04_04cb),
            CompactTarget::from_consensus(0x1c04_04cb)
        ));
    }

    #[test]
    fn presync_rejects_broken_continuity() {
        let mut state = presync_from_zero(no_work(), unreachable_work());
        let wrong = make_header(BlockHash::from_byte_array([1; 32]), ANCIENT_TIMESTAMP);
        assert!(!state.process_presync(&[wrong]).success);
    }

    #[test]
    fn presync_stays_in_memory_below_minimum_work() {
        let mut state = presync_from_zero(no_work(), unreachable_work());
        let headers = make_chain(BlockHash::all_zeros(), ANCIENT_TIMESTAMP, 5);
        let result = state.process_presync(&headers);
        assert!(result.success);
        assert_eq!(state.phase(), &PresyncPhase::Presync);
        assert_eq!(state.presync_height(), 5);
    }

    #[test]
    fn presync_counts_work_from_the_start_header() {
        // The node already holds a lot of work, so only three more headers are needed. Counting
        // from zero would leave presync stuck below the threshold forever.
        let mut held = [0u8; 32];
        held[16] = 1;
        let start_work = Work::from_be_bytes(held);
        let headers = make_chain(BlockHash::all_zeros(), ANCIENT_TIMESTAMP, 3);
        let added = headers
            .iter()
            .fold(no_work(), |work, header| work + header.work());
        let mut state = presync_from_zero(start_work, start_work + added);

        assert!(state.process_presync(&headers[..2]).success);
        assert_eq!(state.phase(), &PresyncPhase::Presync);
        assert!(state.process_presync(&headers[2..]).success);
        assert_eq!(state.phase(), &PresyncPhase::Redownload);
    }

    #[test]
    fn presync_builds_adjacent_checkpoint_ranges() {
        let headers = make_chain(BlockHash::all_zeros(), ANCIENT_TIMESTAMP, 7);
        let mut state = presync_from_zero(no_work(), no_work()).with_checkpoint_interval(3);

        let result = state.process_presync(&headers);
        assert!(result.success);
        assert_eq!(state.phase(), &PresyncPhase::Redownload);

        let ranges = state.redownload_ranges().unwrap();
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].start.height, 0);
        assert_eq!(ranges[0].end.height, 3);
        assert_eq!(ranges[1].start.height, 3);
        assert_eq!(ranges[1].end.height, 6);
        assert_eq!(ranges[2].start.height, 6);
        assert_eq!(ranges[2].end.height, 7);
        assert_eq!(ranges[2].end.hash, headers[6].block_hash());
    }

    #[test]
    fn presync_aborts_at_checkpoint_limit() {
        let headers = make_chain(BlockHash::all_zeros(), ANCIENT_TIMESTAMP, 2);
        let mut state = presync_from_zero(no_work(), unreachable_work())
            .with_checkpoint_interval(1)
            .with_max_checkpoints(2);

        let result = state.process_presync(&headers);
        assert!(result.success);
        assert_eq!(state.phase(), &PresyncPhase::Aborted);
    }

    #[test]
    fn range_download_completes_across_batches() {
        let genesis = HeaderCheckpoint {
            height: 0,
            hash: BlockHash::all_zeros(),
            bits: easy_bits(),
        };
        let headers = make_chain(genesis.hash, ANCIENT_TIMESTAMP, 5);
        let range = HeaderRange {
            start: genesis,
            end: HeaderCheckpoint {
                height: 5,
                hash: headers[4].block_hash(),
                bits: headers[4].bits,
            },
        };
        let mut download = HeadersRangeDownload::new(range, Network::Regtest);

        assert!(matches!(
            download.process(&headers[..2]),
            RangeProgress::InProgress
        ));
        match download.process(&headers[2..]) {
            RangeProgress::Complete(completed) => assert_eq!(completed, headers),
            other => panic!("expected completed range, got {other:?}"),
        }
    }

    #[test]
    fn range_download_rejects_wrong_checkpoint_endpoint() {
        let genesis = HeaderCheckpoint {
            height: 0,
            hash: BlockHash::all_zeros(),
            bits: easy_bits(),
        };
        let expected = make_chain(genesis.hash, ANCIENT_TIMESTAMP, 2);
        let mut alternate = expected.clone();
        alternate[1] = make_header(expected[0].block_hash(), ANCIENT_TIMESTAMP + 10);
        let range = HeaderRange {
            start: genesis,
            end: HeaderCheckpoint {
                height: 2,
                hash: expected[1].block_hash(),
                bits: expected[1].bits,
            },
        };

        assert!(matches!(
            HeadersRangeDownload::new(range, Network::Regtest).process(&alternate),
            RangeProgress::Invalid
        ));
    }
}
