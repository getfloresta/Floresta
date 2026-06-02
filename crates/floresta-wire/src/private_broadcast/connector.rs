// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use super::MAX_PRIVATE_BROADCAST_CONNECTIONS;

/// Tracks the pending-open budget for private-broadcast outbound connections.
///
/// Active peer counts are supplied by the caller when checking capacity.
#[derive(Debug, Default)]
pub struct PrivateBroadcastConnector {
    num_to_open: AtomicUsize,
}

impl PrivateBroadcastConnector {
    /// Returns the number of connections that need to be opened.
    pub fn num_to_open(&self) -> usize {
        self.num_to_open.load(Ordering::Acquire)
    }

    /// Adds `n` to the number of connections that need to be opened.
    pub fn num_to_open_add(&self, n: usize) {
        self.num_to_open.fetch_add(n, Ordering::AcqRel);
    }

    /// Subtracts up to `n`, returning the value remaining after the operation.
    pub fn num_to_open_sub(&self, n: usize) -> usize {
        let mut current = self.num_to_open();
        loop {
            let new_value = current.saturating_sub(n);
            match self.num_to_open.compare_exchange_weak(
                current,
                new_value,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return new_value,
                Err(actual) => current = actual,
            }
        }
    }

    /// Checks if more private-broadcast connections can be opened.
    ///
    /// `active_connections` is the caller's count of connected private-broadcast peers.
    /// Returns true when that count is below [`MAX_PRIVATE_BROADCAST_CONNECTIONS`] and
    /// the pending-open budget is still positive.
    pub fn can_open_more(&self, active_connections: usize) -> bool {
        active_connections < MAX_PRIVATE_BROADCAST_CONNECTIONS && self.num_to_open() > 0
    }
}
