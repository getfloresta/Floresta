// SPDX-License-Identifier: MIT OR Apache-2.0

//! A no-std Single Producer, Single Consumer channel for unidirectional message exchange between
//! modules. This module don't use anything from the standard lib and can be easily used in no-std
//! environments. We only use mem::take from [core].

use core::mem::take;

use crate::prelude::Vec;

/// A (Send + Sync) single producer, single consumer channel to notify modules about things.
/// The api is super minimalistic to reduce external dependencies, including from the std-lib
///
/// One notable difference from the standard mspc channel is that this channel's ends are't
/// two different types, while this is possible, there's no reason to do that. Specially
/// considering that to get a good compile-time asurance that both ends will not be shared, the
/// channel must not be [Send], but this is one of the main requirements to use this channel in
/// async code. Moreover, if two worker threads are meant to be identical threads balancing their
/// work, it might be beneficial to use this same channel as a de-facto single producer, multiple
/// consumer channel for work distribution.
/// # Example
/// ```
/// use floresta_common::spsc;
/// let channel = spsc::Channel::new();
///
/// // Send something
/// channel.send(1);
/// // Read the same thing back
/// assert_eq!(channel.recv().next(), Some(1));
/// ```
#[derive(Debug, Default)]
pub struct Channel<T> {
    /// The data pending for read
    content: spin::Mutex<Vec<T>>,
}

impl<T> Channel<T> {
    /// Creates a new channel
    ///
    /// # Example
    /// ```
    /// use floresta_common::spsc;
    /// let channel = spsc::Channel::new();
    ///
    /// channel.send(1);
    /// assert_eq!(channel.recv().next(), Some(1));
    /// ```
    pub fn new() -> Self {
        Channel {
            content: spin::Mutex::new(Vec::new()),
        }
    }
    /// Sends some data through a channel
    ///
    /// # Example
    /// ```
    /// use floresta_common::spsc;
    /// let channel = spsc::Channel::new();
    ///
    /// channel.send(1);
    /// assert_eq!(channel.recv().next(), Some(1));
    /// ```
    pub fn send(&self, data: T) {
        self.content.lock().push(data);
    }
    /// Reads from a channel
    ///
    /// This method returns an iterator over all elements inside a [Channel]
    pub fn recv(&self) -> RecvIter<T> {
        let inner = take(&mut *self.content.lock());
        RecvIter { inner }
    }
}

/// An iterator issued every time someone calls `recv`.
///
/// This iterator takes all items available for reading in a channel
/// and lets the consumer iterate over them, without acquiring the lock
/// every time (the mutex is only locked when `recv` is called).
///
/// # Example
/// ```
/// use floresta_common::spsc;
/// let channel = spsc::Channel::new();
///
/// channel.send(0);
/// channel.send(1);
///
/// for (i, el) in channel.recv().enumerate() {
///     assert_eq!(i, el);
/// }
/// // A second read should create an empty iterator
/// assert_eq!(channel.recv().next(), None);
/// ```
pub struct RecvIter<T> {
    inner: Vec<T>,
}

impl<T> Iterator for RecvIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.is_empty() {
            return None;
        }
        Some(self.inner.remove(0))
    }
}
