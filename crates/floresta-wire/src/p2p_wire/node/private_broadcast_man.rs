// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`UtreexoNode`] hooks for Tor private-broadcast outbounds.
//!
//! Wires the queue and connection budget in [`crate::private_broadcast`] into peer
//! lifecycle: opening [`ConnectionKind::PrivateBroadcast`] dials, assigning transactions
//! after verack, and tearing down peers once receipt is confirmed or the connection
//! ages out. See [`crate::private_broadcast`] for the full protocol and constants.

use std::time::Duration;
use std::time::Instant;

use bitcoin::Transaction;
use floresta_chain::ChainBackend;
use tracing::debug;
use tracing::warn;

use super::ConnectionKind;
use super::NodeRequest;
use super::PeerStatus;
use super::UtreexoNode;
use crate::node_context::NodeContext;
use crate::p2p_wire::error::WireError;
use crate::private_broadcast::NUM_PRIVATE_BROADCAST_PER_TX;
use crate::private_broadcast::PRIVATE_BROADCAST_MAX_CONNECTION_LIFETIME;

impl<T, Chain> UtreexoNode<Chain, T>
where
    T: 'static + Default + NodeContext,
    Chain: ChainBackend + 'static,
    WireError: From<Chain::Error>,
{
    /// Counts live [`ConnectionKind::PrivateBroadcast`] peers.
    ///
    /// Includes peers still in handshake and those in [`PeerStatus::Ready`]. Used with
    /// [`crate::private_broadcast::PrivateBroadcastConnector::can_open_more`] to enforce
    /// [`crate::private_broadcast::MAX_PRIVATE_BROADCAST_CONNECTIONS`].
    pub(crate) fn count_private_broadcast_peers(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.kind == ConnectionKind::PrivateBroadcast)
            .count()
    }

    /// Opens one outbound Tor private-broadcast connection.
    ///
    /// Picks a random onion tx-recipient via
    /// [`crate::address_man::AddressMan::select_for_private_broadcast`] and dials it over
    /// SOCKS5 as [`ConnectionKind::PrivateBroadcast`]. Returns
    /// [`WireError::NoAddressesAvailable`] when the address manager has no eligible Tor v3
    /// peers.
    pub(crate) fn open_private_broadcast_connection(&mut self) -> Result<String, WireError> {
        let Some((_peer_id, peer_address)) = self.address_man.select_for_private_broadcast() else {
            return Err(WireError::NoAddressesAvailable);
        };
        let target = peer_address.to_string();
        let allow_v1_fallback = self.config.allow_v1_fallback;
        self.open_connection(
            ConnectionKind::PrivateBroadcast,
            peer_address,
            allow_v1_fallback,
        )?;
        Ok(target)
    }

    /// Opens as many private-broadcast outbounds as the connector budget allows.
    ///
    /// No-op when [`crate::p2p_wire::UtreexoNodeConfig::private_broadcast`] is disabled or
    /// no SOCKS5 proxy is configured. Otherwise drains
    /// [`crate::private_broadcast::PrivateBroadcastConnector::num_to_open`] until the global
    /// connection cap is reached or addrman has no onion peers left.
    /// Other dial errors are propagated to the caller.
    pub(crate) fn try_open_private_broadcast_connections(&mut self) -> Result<(), WireError> {
        if !self.config.private_broadcast {
            return Ok(());
        }
        if self.socks5.is_none() {
            if self.private_broadcast_connector.num_to_open() > 0
                && self.last_private_broadcast_unreachable_warn.elapsed() > Duration::from_secs(5)
            {
                warn!("Unable to open private-broadcast connections: no Tor proxy is configured");
                self.last_private_broadcast_unreachable_warn = Instant::now();
            }
            return Ok(());
        }

        while self.private_broadcast_connector.num_to_open() > 0
            && self
                .private_broadcast_connector
                .can_open_more(self.count_private_broadcast_peers())
        {
            match self.open_private_broadcast_connection() {
                Ok(target) => {
                    let remaining = self.private_broadcast_connector.num_to_open_sub(1);
                    debug!(
                        "Socket connected to {}; remaining connections to open: {remaining}",
                        target
                    );
                }
                Err(WireError::NoAddressesAvailable) => {
                    debug!(
                        "Connections needed for private broadcast but addrman has no eligible Tor v3 peers, will retry"
                    );
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Enqueues a transaction for Tor private broadcast and schedules outbound dials.
    ///
    /// Adds `tx` to [`crate::private_broadcast::PrivateBroadcast`]. On first enqueue for
    /// that wtxid, increments the connector budget by
    /// [`crate::private_broadcast::NUM_PRIVATE_BROADCAST_PER_TX`]. Duplicate submissions
    /// of the same transaction are ignored.
    pub(crate) fn schedule_private_broadcast_for_tx(&mut self, tx: Transaction) {
        let txid = tx.compute_txid();
        let wtxid = tx.compute_wtxid();
        if self.tx_for_private_broadcast.add(tx) {
            debug!(
                "Requesting {NUM_PRIVATE_BROADCAST_PER_TX} new connections due to txid={txid}, wtxid={wtxid}"
            );
            self.private_broadcast_connector
                .num_to_open_add(NUM_PRIVATE_BROADCAST_PER_TX);
        } else {
            debug!(
                "Ignoring unnecessary request to schedule an already scheduled transaction: txid={txid}, wtxid={wtxid}"
            );
        }
    }

    /// Assigns a queued transaction to a private-broadcast peer after verack.
    ///
    /// Called from [`super::peer_man`] when a [`ConnectionKind::PrivateBroadcast`] peer
    /// enters [`PeerStatus::Ready`]. Uses
    /// [`crate::private_broadcast::PrivateBroadcast::pick_tx_for_send`] to bind the next
    /// in-flight transaction to this peer and sends [`NodeRequest::PrivateBroadcastInv`].
    /// If the queue is empty, sends [`NodeRequest::Shutdown`] instead. Returns `Ok(())`
    /// immediately when the peer id is no longer in the node's peer map.
    pub(crate) fn on_private_broadcast_peer_ready(&mut self, peer: u32) -> Result<(), WireError> {
        let (peer_log, address) = {
            let Some(p) = self.peers.get(&peer) else {
                return Ok(());
            };
            (
                peer_log(peer, &p.address.to_string()),
                p.address.to_string(),
            )
        };

        if let Some(tx) = self
            .tx_for_private_broadcast
            .pick_tx_for_send(peer, address)
        {
            let txid = tx.compute_txid();
            let wtxid = tx.compute_wtxid();
            let wtxid_suffix = format!(", wtxid={wtxid}");
            debug!(
                "P2P handshake completed, sending INV for txid={txid}{wtxid_suffix}, {peer_log}"
            );
            self.send_to_peer(peer, NodeRequest::PrivateBroadcastInv(tx))?;
        } else {
            debug!(
                "Disconnecting: no more transactions for private broadcast (connected in vain), {peer_log}"
            );
            self.send_to_peer(peer, NodeRequest::Shutdown)?;
        }
        Ok(())
    }

    /// Records receipt and shuts down a private-broadcast peer after `pong`.
    ///
    /// Invoked when the peer task reports [`crate::p2p_wire::peer::PeerMessages::PrivateBroadcastConfirmed`]
    /// (probe `ping` answered). Marks the peer's delivery in
    /// [`crate::private_broadcast::PrivateBroadcast::peer_confirmed_receipt`] and sends
    /// [`NodeRequest::Shutdown`].
    pub(crate) fn on_private_broadcast_confirmed(&mut self, peer: u32) -> Result<(), WireError> {
        if let Some(p) = self.peers.get(&peer) {
            let peer_log = peer_log(peer, &p.address.to_string());
            debug!(
                "Got a PONG (the transaction will probably reach the network), marking for disconnect, {peer_log}"
            );
        }
        self.tx_for_private_broadcast.peer_confirmed_receipt(peer);
        self.send_to_peer(peer, NodeRequest::Shutdown)?;
        Ok(())
    }

    /// Updates private-broadcast state after a dedicated peer disconnects.
    ///
    /// Called from [`super::peer_man`] for every [`ConnectionKind::PrivateBroadcast`]
    /// disconnect. Matches Bitcoin Core `FinalizeNode`: peers that already confirmed
    /// receipt are unchanged; otherwise, if the queue still has pending work, schedules
    /// one replacement outbound via the connector. Per-peer send history in
    /// [`crate::private_broadcast::PrivateBroadcast`] is left intact (Core does not erase
    /// failed picks on disconnect).
    pub(crate) fn on_private_broadcast_disconnect(&mut self, peer: u32, peer_log: &str) {
        if self.tx_for_private_broadcast.did_peer_confirm_receipt(peer) {
            return;
        }

        let remaining = self.private_broadcast_connector.num_to_open();
        if remaining == 0 {
            debug!(
                "Private-broadcast peer disconnected before send completed ({peer_log}); no replacement dial needed"
            );
        } else {
            debug!(
                "Private-broadcast peer disconnected before send completed ({peer_log}); will retry to another peer; remaining connections to open: {remaining}"
            );
        }

        if self.tx_for_private_broadcast.have_pending_transactions() {
            self.private_broadcast_connector.num_to_open_add(1);
        }
    }

    /// Disconnects private-broadcast peers that stalled after verack.
    ///
    /// Targets [`ConnectionKind::PrivateBroadcast`] peers in [`PeerStatus::Ready`] that have
    /// not confirmed receipt (see
    /// [`crate::private_broadcast::PrivateBroadcast::did_peer_confirm_receipt`]) and have been
    /// ready longer than [`crate::private_broadcast::PRIVATE_BROADCAST_MAX_CONNECTION_LIFETIME`].
    /// Each match receives [`NodeRequest::Shutdown`].
    pub(crate) fn disconnect_stale_private_broadcast_peers(&mut self) -> Result<(), WireError> {
        let now = Instant::now();
        let timeout_secs = PRIVATE_BROADCAST_MAX_CONNECTION_LIFETIME.as_secs();
        let to_disconnect: Vec<(u32, String)> = self
            .peers
            .iter()
            .filter_map(|(peer_id, p)| {
                if p.kind == ConnectionKind::PrivateBroadcast
                    && p.state == PeerStatus::Ready
                    && !self
                        .tx_for_private_broadcast
                        .did_peer_confirm_receipt(*peer_id)
                    && p.ready_since.is_some_and(|t| {
                        now.duration_since(t) > PRIVATE_BROADCAST_MAX_CONNECTION_LIFETIME
                    })
                {
                    Some((*peer_id, peer_log(*peer_id, &p.address.to_string())))
                } else {
                    None
                }
            })
            .collect();
        for (peer, peer_log) in to_disconnect {
            debug!(
                "Disconnecting: did not complete the transaction send within {timeout_secs} seconds, {peer_log}"
            );
            let _ = self.send_to_peer(peer, NodeRequest::Shutdown);
        }
        Ok(())
    }

    /// Re-schedules stale queued transactions for another round of Tor outbounds.
    ///
    /// No-op when private broadcast is disabled. Otherwise collects entries from
    /// [`crate::private_broadcast::PrivateBroadcast::get_stale`] (see
    /// [`crate::private_broadcast::INITIAL_STALE_DURATION`] and
    /// [`crate::private_broadcast::STALE_DURATION`]). Stale transactions that still pass
    /// local mempool validation each add one connection to the connector budget; invalid
    /// ones are removed from the queue. Invoked periodically from [`super::running_ctx`]
    /// on [`crate::private_broadcast::REATTEMPT_INTERVAL_BASE`].
    pub(crate) async fn reattempt_private_broadcast(&mut self) {
        if !self.config.private_broadcast {
            return;
        }
        let stale = self.tx_for_private_broadcast.get_stale();
        if stale.is_empty() {
            return;
        }

        let mut num_for_rebroadcast = 0usize;
        for tx in stale {
            let txid = tx.compute_txid();
            let wtxid = tx.compute_wtxid();
            let valid = {
                let mut guard = self.mempool.lock().await;
                guard.validate_transaction(&tx)
            };
            if valid.is_ok() {
                debug!("Reattempting broadcast of stale txid={txid} wtxid={wtxid}");
                num_for_rebroadcast += 1;
            } else {
                debug!("Giving up broadcast attempts for txid={txid} wtxid={wtxid}: {valid:?}");
                self.tx_for_private_broadcast.remove(&tx);
            }
        }
        if num_for_rebroadcast > 0 {
            self.private_broadcast_connector
                .num_to_open_add(num_for_rebroadcast);
        }
    }

    /// Removes a queued transaction when it appears on the public P2P network.
    ///
    /// Called from [`super::peer_man`] on every clearnet `tx`. No-op when private broadcast is disabled or the wtxid is not in
    /// [`crate::private_broadcast::PrivateBroadcast`]. Otherwise drops the queue entry and
    /// reduces the connector budget by the number of
    /// [`crate::private_broadcast::NUM_PRIVATE_BROADCAST_PER_TX`] outbounds that were still
    /// unacknowledged.
    pub(crate) fn handle_private_broadcast_echo(&mut self, tx: &Transaction) {
        if !self.config.private_broadcast {
            return;
        }
        if let Some(num_confirmed) = self.tx_for_private_broadcast.remove(tx) {
            debug!(
                "private broadcast tx {} echoed; confirmed on {} peers",
                tx.compute_txid(),
                num_confirmed
            );
            if NUM_PRIVATE_BROADCAST_PER_TX > num_confirmed {
                self.private_broadcast_connector
                    .num_to_open_sub(NUM_PRIVATE_BROADCAST_PER_TX - num_confirmed);
            }
        }
    }

    /// Aborts private broadcast for a transaction identified by txid or wtxid.
    ///
    /// Removes matching entries via [`crate::private_broadcast::PrivateBroadcast::abort_by_id`]
    /// and subtracts from the connector budget any outbound slots that were still
    /// unacknowledged. Used by [`super::user_req`] for RPC abort.
    pub(crate) fn abort_private_broadcast(&mut self, id: [u8; 32]) -> Vec<Transaction> {
        let removed = self.tx_for_private_broadcast.abort_by_id(id);
        let mut connections_cancelled = 0usize;
        let mut txs = Vec::new();
        for (tx, acks) in removed {
            txs.push(tx);
            if NUM_PRIVATE_BROADCAST_PER_TX > acks {
                connections_cancelled += NUM_PRIVATE_BROADCAST_PER_TX - acks;
            }
        }
        if connections_cancelled > 0 {
            self.private_broadcast_connector
                .num_to_open_sub(connections_cancelled);
        }
        txs
    }
}

pub(super) fn peer_log(peer: u32, address: &str) -> String {
    format!("peer={peer} addr={address}")
}
