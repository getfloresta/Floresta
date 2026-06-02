// SPDX-License-Identifier: MIT OR Apache-2.0

//! Private transaction broadcast over Tor.
//!
//! Mirrors Bitcoin Core's `PrivateBroadcast`. Floresta dials random Tor v3 **tx-recipients** from
//! the addrman over SOCKS5. With `config.private_broadcast` enabled, [`crate::node_interface::UserRequest::SendTransaction`]
//! enqueues the tx in [`PrivateBroadcast`] instead of the public mempool relay path.
//! Each new entry schedules [`NUM_PRIVATE_BROADCAST_PER_TX`] such outbounds, capped by
//! [`MAX_PRIVATE_BROADCAST_CONNECTIONS`]. [`PrivateBroadcastConnector`] counts how
//! many connections still need to open; [`crate::address_man::AddressMan::select_for_private_broadcast`]
//! picks the next onion peer. One connection, end to end:
//!
//! ```text
//! tx-sender >--- connect -------> tx-recipient
//! tx-sender >--- VERSION -------> tx-recipient   (anonymous VERSION; see [`PRIVATE_BROADCAST_USER_AGENT`])
//! tx-sender <--- VERSION -------< tx-recipient
//! tx-sender <--- VERACK --------< tx-recipient
//! tx-sender >--- VERACK --------> tx-recipient
//! tx-sender >--- INV -----------> tx-recipient
//! tx-sender <--- GETDATA -------< tx-recipient
//! tx-sender >--- TX ------------> tx-recipient
//! tx-sender >--- PING ----------> tx-recipient
//! tx-sender <--- PONG ----------< tx-recipient
//! tx-sender disconnects
//! ```
//!
//! On Tor private-broadcast outbounds, tx-sender sends a minimal `version` message
//! (no services, zero time/height, empty addrs, decoy user agent) so recipients cannot
//! fingerprint this client. Regular P2P peers still get the normal version.
//!
//! The handshake is a trimmed P2P version exchange: optional recipient features are
//! ignored, but the recipient must advertise `relay`. After `verack`, tx-sender
//! announces the tx with `inv`; tx-recipient pulls it with `getdata`; tx-sender
//! delivers `tx` and probes with `ping`. `pong` counts as receipt in
//! [`PrivateBroadcast`], then the outbound peer is shut down. Wire handling lives in
//! `p2p_wire`; this module holds only the queue and connection budget.
//!
//! A tx leaves the queue when it is echoed back on the ordinary P2P network (mempool
//! acceptance elsewhere) or when RPC abort removes it. Entries that miss acks within
//! [`INITIAL_STALE_DURATION`] / [`STALE_DURATION`] are reconsidered on
//! [`REATTEMPT_INTERVAL_BASE`] if mempool validation still passes. Peers that never
//! finish the flow are dropped after [`PRIVATE_BROADCAST_MAX_CONNECTION_LIFETIME`].
//!
//! ## Logging
//!
//! Events are emitted with [`tracing`] at `debug`/`warn`, like the rest of `floresta-wire`.
//! Use `--debug` or `RUST_LOG` to enable them. For private-broadcast only (without full
//! wire noise), for example:
//!
//! ```text
//! RUST_LOG=floresta_wire::p2p_wire::node::private_broadcast_man=debug,floresta_wire::p2p_wire::peer=debug,info
//! ```

mod connector;
mod queue;

pub use connector::PrivateBroadcastConnector;
pub use queue::PeerSendInfo;
pub use queue::PrivateBroadcast;
pub use queue::TxBroadcastInfo;

/// User agent sent on private-broadcast outbounds only.
///
/// Bitcoin Core uses this exact string on `IsPrivateBroadcastConn()` ([discussion](https://github.com/bitcoin/bitcoin/pull/27509#discussion_r1214671917)).
/// We reuse it so Tor recipients cannot distinguish Floresta from Core private-broadcast
/// senders and cannot link the connection to the configured user agent on clearnet peers.
pub const PRIVATE_BROADCAST_USER_AGENT: &str = "/pynode:0.0.1/";

/// Outbound private-broadcast connections opened per submitted transaction.
pub const NUM_PRIVATE_BROADCAST_PER_TX: usize = 3;

/// Maximum simultaneous `ConnectionKind::PrivateBroadcast` peers.
pub const MAX_PRIVATE_BROADCAST_CONNECTIONS: usize = 64;

/// Disconnect private-broadcast peers that do not finish the send/ack flow in time.
pub const PRIVATE_BROADCAST_MAX_CONNECTION_LIFETIME: std::time::Duration =
    std::time::Duration::from_secs(3 * 60);

/// Interval between stale-queue scans.
pub const REATTEMPT_INTERVAL_BASE: std::time::Duration = std::time::Duration::from_secs(2 * 60);

/// If a transaction is not sent to any peer for this duration, consider it stale.
pub const INITIAL_STALE_DURATION: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// After the last peer ack, wait this long before considering the tx stale again.
pub const STALE_DURATION: std::time::Duration = std::time::Duration::from_secs(60);
