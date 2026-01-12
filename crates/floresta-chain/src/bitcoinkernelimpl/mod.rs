//! Bitcoin Kernel-based chainstate implementation for Floresta
//!
//! This module provides an alternative chainstate backend using the bitcoin-kernel library,
//! which leverages Bitcoin Core's consensus engine.
mod blockchaininterface;

pub use blockchaininterface::BitcoinKernelChainstate;
