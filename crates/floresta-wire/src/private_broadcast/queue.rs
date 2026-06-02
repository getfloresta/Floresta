// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bitcoin::Transaction;
use bitcoin::Txid;
use bitcoin::Wtxid;
use bitcoin::hashes::Hash;

use super::INITIAL_STALE_DURATION;
use super::STALE_DURATION;
use crate::node_context::PeerId;

/// Per-peer send/ack timestamps for one in-flight private broadcast.
#[derive(Debug, Clone)]
pub struct PeerSendInfo {
    /// The address of the peer.
    pub address: String,
    /// The timestamp when the transaction was sent to the peer.
    pub sent: u64,
    /// The timestamp when the transaction was received from the peer.
    pub received: Option<u64>,
}

/// Snapshot of one transaction moving through the private-broadcast queue.
#[derive(Debug, Clone)]
pub struct TxBroadcastInfo {
    /// The transaction.
    pub tx: Transaction,
    /// The transaction ID.
    pub txid: Txid,
    /// The transaction wtxid.
    pub wtxid: Wtxid,
    /// The timestamp when the transaction was added to the queue.
    pub time_added: u64,
    /// The peers that the transaction has been sent to.
    pub peers: Vec<PeerSendInfo>,
}

/// Per-peer delivery state for one in-flight private broadcast transaction.
///
/// Created each time a peer is picked to receive the transaction. Records when
/// that peer was selected and, once known, when they acknowledged receipt (UNIX seconds).
#[derive(Debug)]
struct SendStatus {
    /// The peer that the transaction was sent to.
    peer_id: PeerId,
    /// The address of the peer that the transaction was sent to.
    address: String,
    /// UNIX time (seconds) when this peer was selected to receive the tx.
    picked_unix: u64,
    /// UNIX time (seconds) when the peer acknowledged receipt, if known.
    confirmed_unix: Option<u64>,
}

/// One transaction in the private-broadcast queue and all peers targeted so far.
///
/// Holds the transaction body, when it was enqueued, and a growing list of
/// per-peer pick/ack timestamps as broadcast retries reach more peers.
#[derive(Debug)]
struct TxSendStatus {
    /// The transaction.
    tx: Transaction,
    /// UNIX time (seconds) when the transaction was added to the queue.
    time_added_unix: u64,
    /// The send statuses of the transaction.
    send_statuses: Vec<SendStatus>,
}

/// Priority of a transaction in the queue.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Priority {
    /// The number of times the transaction was picked for sending.
    num_picked: usize,
    /// The number of peers that have confirmed receipt of the transaction.
    num_confirmed: usize,
    /// Most recent pick time across peers (UNIX seconds).
    last_picked: Option<u64>,
    /// Most recent peer ack time (UNIX seconds).
    last_confirmed: Option<u64>,
}

/// In-memory queue of transactions awaiting or undergoing Tor private broadcast.
#[derive(Debug, Default)]
pub struct PrivateBroadcast {
    /// The mutex protecting the private broadcast queue.
    mutex: Mutex<HashMap<Wtxid, TxSendStatus>>,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn num_confirmed(sent_to: &[SendStatus]) -> usize {
    sent_to
        .iter()
        .filter(|s| s.confirmed_unix.is_some())
        .count()
}

/// Derives [`Priority`] from a transaction's per-peer send history.
///
/// One transaction may be sent to many peers over time. The queue needs a
/// single measure of broadcast progress to rank in-flight work when choosing
/// the next outbound send and when applying stale timeouts.
fn derive_priority(sent_to: &[SendStatus]) -> Priority {
    let mut p = Priority {
        num_picked: sent_to.len(),
        last_picked: None,
        num_confirmed: num_confirmed(sent_to),
        last_confirmed: None,
    };
    for status in sent_to {
        match p.last_picked {
            None => p.last_picked = Some(status.picked_unix),
            Some(t) if status.picked_unix > t => p.last_picked = Some(status.picked_unix),
            _ => {}
        }
        if let Some(confirmed) = status.confirmed_unix {
            match p.last_confirmed {
                None => p.last_confirmed = Some(confirmed),
                Some(t) if confirmed > t => p.last_confirmed = Some(confirmed),
                _ => {}
            }
        }
    }
    p
}

/// Determines whether a queued transaction should be reconsidered for rebroadcast at `now_unix`.
fn tx_is_stale(time_added_unix: u64, send_statuses: &[SendStatus], now_unix: u64) -> bool {
    let p = derive_priority(send_statuses);
    if p.num_confirmed == 0 {
        time_added_unix.saturating_add(INITIAL_STALE_DURATION.as_secs()) < now_unix
    } else {
        p.last_confirmed
            .expect("num_confirmed > 0 implies last_confirmed")
            .saturating_add(STALE_DURATION.as_secs())
            < now_unix
    }
}

/// Compares queue entries for `pick_tx_for_send` by [`Priority`] only (Bitcoin Core `PickTxForSend`).
fn compare_tx_for_send(
    a: &(&Wtxid, &TxSendStatus),
    b: &(&Wtxid, &TxSendStatus),
) -> std::cmp::Ordering {
    derive_priority(&a.1.send_statuses).cmp(&derive_priority(&b.1.send_statuses))
}

impl PrivateBroadcast {
    /// Adds a transaction to the private broadcast queue.
    ///
    /// Returns `false` if the transaction is already in the queue.
    pub fn add(&self, tx: Transaction) -> bool {
        let wtxid = tx.compute_wtxid();
        let mut guard = self.mutex.lock().expect("private broadcast lock poisoned");
        if guard.contains_key(&wtxid) {
            return false;
        }
        guard.insert(
            wtxid,
            TxSendStatus {
                tx,
                time_added_unix: unix_now(),
                send_statuses: Vec::new(),
            },
        );
        true
    }

    /// Removes a transaction from the private broadcast queue.
    ///
    /// Called from `p2p_wire` on clearnet P2P echo, stale reattempt give-up, etc. Returns the
    /// number of peers that had already confirmed receipt, if the wtxid was present.
    pub fn remove(&self, tx: &Transaction) -> Option<usize> {
        let wtxid = tx.compute_wtxid();
        let mut guard = self.mutex.lock().expect("private broadcast lock poisoned");
        let status = guard.remove(&wtxid)?;
        Some(num_confirmed(&status.send_statuses))
    }

    /// Aborts queued transactions matching `id` (txid or wtxid).
    ///
    /// Returns each removed transaction with the number of peers that had already
    /// confirmed receipt before removal.
    pub fn abort_by_id(&self, id: [u8; 32]) -> Vec<(Transaction, usize)> {
        let mut guard = self.mutex.lock().expect("private broadcast lock poisoned");
        let keys: Vec<Wtxid> = guard
            .iter()
            .filter(|(_, state)| {
                state.tx.compute_txid().as_byte_array() == &id
                    || state.tx.compute_wtxid().as_byte_array() == &id
            })
            .map(|(w, _)| *w)
            .collect();
        keys.into_iter()
            .filter_map(|w| {
                guard
                    .remove(&w)
                    .map(|s| (s.tx, num_confirmed(&s.send_statuses)))
            })
            .collect()
    }

    /// Checks if a transaction is in the private broadcast queue.
    ///
    /// Returns `true` if the transaction is in the queue.
    pub fn contains_tx(&self, tx: &Transaction) -> bool {
        self.mutex
            .lock()
            .expect("private broadcast lock poisoned")
            .contains_key(&tx.compute_wtxid())
    }

    /// Checks if there are any pending transactions in the private broadcast queue.
    ///
    /// Returns `true` if there are any pending transactions.
    pub fn have_pending_transactions(&self) -> bool {
        !self
            .mutex
            .lock()
            .expect("private broadcast lock poisoned")
            .is_empty()
    }

    /// Picks a transaction for sending to a peer.
    ///
    /// Returns the transaction if it was picked for sending.
    pub fn pick_tx_for_send(&self, peer_id: PeerId, peer_address: String) -> Option<Transaction> {
        let mut guard = self.mutex.lock().expect("private broadcast lock poisoned");
        let wtxid = *guard.iter().min_by(compare_tx_for_send)?.0;

        let state = guard.get_mut(&wtxid)?;
        state.send_statuses.push(SendStatus {
            peer_id,
            address: peer_address,
            picked_unix: unix_now(),
            confirmed_unix: None,
        });
        Some(state.tx.clone())
    }

    /// Gets the transaction assigned to a peer.
    ///
    /// Returns their transaction if it is still in the queue.
    pub fn get_tx_for_peer(&self, peer_id: PeerId) -> Option<Transaction> {
        let guard = self.mutex.lock().expect("private broadcast lock poisoned");
        for state in guard.values() {
            if state.send_statuses.iter().any(|s| s.peer_id == peer_id) {
                return Some(state.tx.clone());
            }
        }
        None
    }

    /// Marks a peer as having confirmed receipt of their transaction.
    ///
    /// Updates the peer's send status to indicate they have confirmed receipt.
    pub fn peer_confirmed_receipt(&self, peer_id: PeerId) {
        let mut guard = self.mutex.lock().expect("private broadcast lock poisoned");
        for state in guard.values_mut() {
            for status in &mut state.send_statuses {
                if status.peer_id == peer_id {
                    status.confirmed_unix = Some(unix_now());
                }
            }
        }
    }

    /// Checks if the given peer has confirmed receipt of their transaction.
    ///
    /// Returns `true` if the peer has confirmed receipt of their transaction.
    pub fn did_peer_confirm_receipt(&self, peer_id: PeerId) -> bool {
        let guard = self.mutex.lock().expect("private broadcast lock poisoned");
        guard.values().any(|state| {
            state
                .send_statuses
                .iter()
                .any(|s| s.peer_id == peer_id && s.confirmed_unix.is_some())
        })
    }

    /// Gets all transactions that are stale and should be rebroadcast.
    ///
    /// Returns a list of transactions that are stale and should be rebroadcast.
    pub fn get_stale(&self) -> Vec<Transaction> {
        let now_unix = unix_now();
        let guard = self.mutex.lock().expect("private broadcast lock poisoned");
        let mut stale = Vec::new();
        for state in guard.values() {
            if tx_is_stale(state.time_added_unix, &state.send_statuses, now_unix) {
                stale.push(state.tx.clone());
            }
        }
        stale
    }

    /// Gets a snapshot of all transactions in the private broadcast queue.
    ///
    /// Returns a list of transactions in the queue, including the transaction ID,
    /// wtxid, timestamp when added, and the peers that the transaction has been
    /// sent to.
    pub fn get_broadcast_info(&self) -> Vec<TxBroadcastInfo> {
        let guard = self.mutex.lock().expect("private broadcast lock poisoned");
        guard
            .values()
            .map(|state| {
                let peers = state
                    .send_statuses
                    .iter()
                    .map(|status| PeerSendInfo {
                        address: status.address.clone(),
                        sent: status.picked_unix,
                        received: status.confirmed_unix,
                    })
                    .collect();
                TxBroadcastInfo {
                    txid: state.tx.compute_txid(),
                    wtxid: state.tx.compute_wtxid(),
                    tx: state.tx.clone(),
                    time_added: state.time_added_unix,
                    peers,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::Transaction;
    use bitcoin::absolute::LockTime;
    use bitcoin::transaction::Version;

    use super::*;

    /// Synthetic UNIX clock (seconds) for stale/priority unit tests.
    const NOW: u64 = 21_000_000;

    fn tx(n_sequence: u32) -> Transaction {
        Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![bitcoin::TxIn {
                previous_output: bitcoin::OutPoint::null(),
                script_sig: bitcoin::ScriptBuf::new(),
                sequence: bitcoin::Sequence(n_sequence),
                witness: bitcoin::Witness::default(),
            }],
            output: vec![],
        }
    }

    fn send_status(peer_id: PeerId, picked_unix: u64, confirmed_unix: Option<u64>) -> SendStatus {
        SendStatus {
            peer_id,
            address: format!("peer{peer_id}"),
            picked_unix,
            confirmed_unix,
        }
    }

    fn queue_with_picked_tx(peer_id: PeerId, n_sequence: u32) -> (PrivateBroadcast, Transaction) {
        let pb = PrivateBroadcast::default();
        let tx = tx(n_sequence);
        assert!(pb.add(tx.clone()));
        assert!(
            pb.pick_tx_for_send(peer_id, format!("peer{peer_id}:8333"))
                .is_some()
        );
        (pb, tx)
    }

    #[test]
    fn test_tx_is_stale_unacked_after_initial_timeout() {
        let time_added = NOW - (INITIAL_STALE_DURATION.as_secs() + 1);
        assert!(tx_is_stale(time_added, &[], NOW));
    }

    #[test]
    fn test_tx_is_stale_unacked_before_initial_timeout() {
        assert!(!tx_is_stale(NOW, &[], NOW));
    }

    #[test]
    fn test_tx_is_stale_acked_after_rebroadcast_timeout() {
        let last_confirmed = NOW - (STALE_DURATION.as_secs() + 1);
        let statuses = vec![send_status(1, last_confirmed, Some(last_confirmed))];
        assert!(tx_is_stale(last_confirmed, &statuses, NOW));
    }

    #[test]
    fn test_tx_is_stale_acked_before_rebroadcast_timeout() {
        let statuses = vec![send_status(1, NOW, Some(NOW))];
        assert!(!tx_is_stale(NOW, &statuses, NOW));
    }

    #[test]
    fn test_derive_priority_orders_by_picks_then_confirms() {
        let two_picks = vec![send_status(1, NOW, None), send_status(2, NOW, None)];
        let one_pick_one_confirm = vec![send_status(3, NOW, Some(NOW))];
        assert!(
            derive_priority(&two_picks) > derive_priority(&one_pick_one_confirm),
            "more picks: one confirmation does not win"
        );

        let one_pick_unconfirmed = vec![send_status(4, NOW, None)];
        let one_pick_confirmed = vec![send_status(5, NOW, Some(NOW))];
        assert!(
            derive_priority(&one_pick_confirmed) > derive_priority(&one_pick_unconfirmed),
            "equal picks: more confirmations win"
        );
    }

    #[test]
    fn test_add() {
        let pb = PrivateBroadcast::default();
        let tx = tx(1);

        let first_add = pb.add(tx.clone());

        assert!(first_add);

        let duplicate_add = pb.add(tx);

        assert!(!duplicate_add);
    }

    #[test]
    fn test_contains_tx() {
        let pb = PrivateBroadcast::default();
        let tx = tx(1);

        let absent = !pb.contains_tx(&tx);

        assert!(absent);

        assert!(pb.add(tx.clone()));

        let present = pb.contains_tx(&tx);

        assert!(present);
    }

    #[test]
    fn test_have_pending_transactions() {
        let pb = PrivateBroadcast::default();

        let empty = !pb.have_pending_transactions();

        assert!(empty);

        assert!(pb.add(tx(1)));

        let pending = pb.have_pending_transactions();

        assert!(pending);
    }

    #[test]
    fn test_abort_by_id() {
        let pb = PrivateBroadcast::default();
        let wrong_id = [0u8; 32];

        let empty_abort = pb.abort_by_id(wrong_id);

        assert!(empty_abort.is_empty());

        let tx = tx(1);
        assert!(pb.add(tx.clone()));

        let no_match_abort = pb.abort_by_id(wrong_id);

        assert!(no_match_abort.is_empty());

        let txid_abort = pb.abort_by_id(*tx.compute_txid().as_byte_array());

        assert_eq!(txid_abort.len(), 1);
        assert_eq!(txid_abort[0].0.compute_wtxid(), tx.compute_wtxid());

        assert!(pb.add(tx.clone()));

        let wtxid_abort = pb.abort_by_id(*tx.compute_wtxid().as_byte_array());

        assert_eq!(wtxid_abort.len(), 1);
        assert_eq!(wtxid_abort[0].0.compute_wtxid(), tx.compute_wtxid());
    }

    #[test]
    fn test_pick_tx_for_send() {
        let pb = PrivateBroadcast::default();

        let empty_pick = pb.pick_tx_for_send(1, "peer1:8333".into());

        assert!(empty_pick.is_none());

        let tx = tx(1);
        assert!(pb.add(tx.clone()));

        let picked = pb.pick_tx_for_send(1, "peer1:8333".into());

        assert_eq!(
            picked.expect("queued tx should be picked").compute_wtxid(),
            tx.compute_wtxid()
        );
    }

    #[test]
    fn test_pick_tx_for_send_prefers_least_in_flight_tx() {
        let pb = PrivateBroadcast::default();
        let tx_a = tx(1);
        let tx_b = tx(2);
        assert!(pb.add(tx_a.clone()));
        assert!(pb.add(tx_b.clone()));

        let first = pb
            .pick_tx_for_send(1, "peer1:8333".into())
            .expect("non-empty queue should yield a tx for peer 1");
        let second = pb
            .pick_tx_for_send(2, "peer2:8333".into())
            .expect("queue should still have a tx for peer 2");
        assert_ne!(first.compute_wtxid(), second.compute_wtxid());

        pb.peer_confirmed_receipt(1);
        let third = pb
            .pick_tx_for_send(3, "peer3:8333".into())
            .expect("less in-flight tx should be pickable for peer 3");
        assert_eq!(
            second.compute_wtxid(),
            third.compute_wtxid(),
            "still prefer the tx with fewer picks and confirmations"
        );
    }

    #[test]
    fn test_get_tx_for_peer() {
        let pb = PrivateBroadcast::default();

        let missing = pb.get_tx_for_peer(1);

        assert!(missing.is_none());

        let tx = tx(1);
        assert!(pb.add(tx.clone()));
        assert!(pb.pick_tx_for_send(1, "peer1:8333".into()).is_some());

        let assigned = pb.get_tx_for_peer(1);

        assert_eq!(
            assigned
                .expect("picked peer should have a tx")
                .compute_wtxid(),
            tx.compute_wtxid()
        );
    }

    #[test]
    fn test_did_peer_confirm_receipt() {
        let pb = PrivateBroadcast::default();
        let tx = tx(1);
        assert!(pb.add(tx));
        assert!(pb.pick_tx_for_send(1, "peer1:8333".into()).is_some());

        let unconfirmed = !pb.did_peer_confirm_receipt(1);

        assert!(unconfirmed);

        pb.peer_confirmed_receipt(1);

        let confirmed = pb.did_peer_confirm_receipt(1);

        assert!(confirmed);
    }

    #[test]
    fn test_get_broadcast_info() {
        let pb = PrivateBroadcast::default();

        let empty_info = pb.get_broadcast_info();

        assert!(empty_info.is_empty());

        let tx = tx(1);
        assert!(pb.add(tx.clone()));
        assert!(pb.pick_tx_for_send(1, "peer1:8333".into()).is_some());
        assert!(pb.pick_tx_for_send(2, "peer2:8333".into()).is_some());
        pb.peer_confirmed_receipt(1);

        let info = pb.get_broadcast_info();

        assert_eq!(
            (
                info.len(),
                info[0].peers.len(),
                info[0].peers[0].received.is_some(),
                info[0].peers[1].received.is_none(),
            ),
            (1, 2, true, true)
        );
    }

    #[test]
    fn test_remove() {
        let pb = PrivateBroadcast::default();
        let tx = tx(1);

        let missing = pb.remove(&tx);

        assert_eq!(missing, None);

        assert!(pb.add(tx.clone()));
        assert!(pb.pick_tx_for_send(1, "peer1:8333".into()).is_some());

        let removed = pb.remove(&tx);

        assert_eq!(removed, Some(0));
    }

    #[test]
    fn test_remove_confirmed_count() {
        let (pb, picked_tx) = queue_with_picked_tx(1, 1);
        pb.peer_confirmed_receipt(1);
        assert!(pb.did_peer_confirm_receipt(1));
        assert_eq!(pb.remove(&picked_tx), Some(1));
        assert!(!pb.have_pending_transactions());

        let pb = PrivateBroadcast::default();
        let tx1 = tx(1);
        let tx2 = tx(2);
        assert!(pb.add(tx1.clone()));
        assert!(pb.add(tx2.clone()));
        let t1 = pb
            .pick_tx_for_send(1, "peer1:8333".into())
            .expect("peer 1 should get a tx");
        let t2 = pb
            .pick_tx_for_send(2, "peer2:8333".into())
            .expect("peer 2 should get a tx");
        assert_ne!(t1.compute_wtxid(), t2.compute_wtxid());
        pb.peer_confirmed_receipt(1);
        pb.peer_confirmed_receipt(2);
        assert_eq!(pb.remove(&t1), Some(1));
        assert_eq!(pb.remove(&t2), Some(1));
        assert!(!pb.have_pending_transactions());
    }

    /// Bitcoin Core keeps failed private-broadcast picks in `send_statuses` on disconnect.
    #[test]
    fn failed_peer_pick_retained_after_disconnect_like_bitcoin_core() {
        let pb = PrivateBroadcast::default();
        let tx = tx(1);
        assert!(pb.add(tx.clone()));
        assert!(pb.pick_tx_for_send(1, "peer1:8333".into()).is_some());

        assert!(pb.get_tx_for_peer(1).is_some());
        assert!(!pb.did_peer_confirm_receipt(1));

        let info = pb.get_broadcast_info();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].peers.len(), 1);
        assert_eq!(info[0].peers[0].address, "peer1:8333");
    }
}
