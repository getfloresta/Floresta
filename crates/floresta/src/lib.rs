// SPDX-License-Identifier: MIT OR Apache-2.0

#![doc = include_str!("../README.md")]

/// Chain state, validation, accumulator, and block-processing primitives.
#[cfg(feature = "chain")]
pub use floresta_chain as chain;
/// Shared primitives used across Floresta crates.
#[cfg(feature = "common")]
pub use floresta_common as common;
/// Compact Block Filter storage and querying.
#[cfg(feature = "compact-filters")]
pub use floresta_compact_filters as compact_filters;
/// Domain traits and types for building node components.
#[cfg(feature = "domain")]
pub use floresta_domain as domain;
/// Electrum server implementation backed by Floresta.
#[cfg(feature = "electrum")]
pub use floresta_electrum as electrum;
/// Transaction mempool and policy logic.
#[cfg(feature = "mempool")]
pub use floresta_mempool as mempool;
/// Prometheus metrics registry and exporter.
#[cfg(feature = "metrics")]
pub use floresta_metrics as metrics;
/// High-level full node orchestration.
#[cfg(feature = "node")]
pub use floresta_node as node;
/// RPC method traits, data types, and JSON-RPC client.
#[cfg(feature = "rpc")]
pub use floresta_rpc as rpc;
/// Watch-only wallet indexing and storage.
#[cfg(feature = "watch-only")]
pub use floresta_watch_only as watch_only;
/// Bitcoin P2P networking and wire protocol.
#[cfg(feature = "wire")]
pub use floresta_wire as wire;
