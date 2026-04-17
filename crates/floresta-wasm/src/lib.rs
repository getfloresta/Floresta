// SPDX-License-Identifier: MIT OR Apache-2.0

//! # Floresta WASM
//!
//! WebAssembly bindings for Floresta's core chain validation and watch-only wallet.
//!
//! This crate compiles the WASM-compatible subset of Floresta — primarily the chain
//! validation engine ([`floresta_chain`]) and the in-memory watch-only wallet
//! ([`floresta_watch_only`]) — and exposes them to JavaScript via `wasm-bindgen`.
//!
//! ## What works in WASM
//!
//! * Header validation and chain state management
//! * Block connection and consensus checks (pure-Rust path, no `bitcoinkernel`)
//! * Utreexo accumulator
//! * Watch-only wallet with in-memory database
//!
//! ## What does **not** work in WASM
//!
//! * P2P networking (TCP sockets) — use a JS-side transport instead
//! * Filesystem-backed storage — data lives in memory (persist via IndexedDB from JS)
//! * `libbitcoinkernel` C++ FFI — disabled automatically

mod memory_chain_store;

use std::sync::Arc;

use bitcoin::consensus::deserialize;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use floresta_chain::AssumeValidArg;
use floresta_chain::BlockchainInterface;
use floresta_chain::ChainState;
use floresta_chain::pruned_utreexo::chain_state_builder::ChainStateBuilder;
use floresta_chain::pruned_utreexo::UpdatableChainstate;
use wasm_bindgen::prelude::*;

pub use crate::memory_chain_store::MemoryChainStore;

// ---------------------------------------------------------------------------
// Error helper
// ---------------------------------------------------------------------------

fn to_js_err<E: std::fmt::Debug>(e: E) -> JsError {
    JsError::new(&format!("{e:?}"))
}

fn str_to_js_err(e: String) -> JsError {
    JsError::new(&e)
}

// ---------------------------------------------------------------------------
// WasmChainState — the main JS-facing type
// ---------------------------------------------------------------------------

/// A lightweight Bitcoin chain-state that runs entirely in WASM.
///
/// Validates headers and blocks using Floresta's consensus engine (pure Rust,
/// no `libbitcoinkernel`).  Storage is kept in memory; callers can serialise
/// and persist externally (e.g. IndexedDB).
#[wasm_bindgen]
pub struct WasmChainState {
    inner: Arc<ChainState<MemoryChainStore>>,
}

#[wasm_bindgen]
impl WasmChainState {
    /// Create a new chain-state for the given network.
    ///
    /// `network` must be one of: `"bitcoin"`, `"testnet"`, `"signet"`, `"regtest"`.
    #[wasm_bindgen(constructor)]
    pub fn new(network: &str) -> Result<WasmChainState, JsError> {
        let net = parse_network(network).map_err(str_to_js_err)?;
        let params = floresta_chain::ChainParams::from(net);
        let store = MemoryChainStore::new();

        let chain = ChainStateBuilder::new()
            .with_chainstore(store)
            .with_chain_params(params)
            .toggle_ibd(true)
            .build()
            .map_err(to_js_err)?;

        Ok(Self {
            inner: Arc::new(chain),
        })
    }

    /// Create a chain-state with an assume-valid block hash for faster IBD.
    ///
    /// When set, script validation is skipped for blocks at or below the
    /// specified hash, greatly speeding up initial sync.
    #[wasm_bindgen(js_name = "newWithAssumeValid")]
    pub fn new_with_assume_valid(
        network: &str,
        assume_valid_hex: &str,
    ) -> Result<WasmChainState, JsError> {
        let net = parse_network(network).map_err(str_to_js_err)?;
        let params = floresta_chain::ChainParams::from(net);
        let store = MemoryChainStore::new();

        let av_hash: BlockHash = assume_valid_hex.parse().map_err(to_js_err)?;
        let av = AssumeValidArg::UserInput(av_hash);

        let chain = ChainStateBuilder::new()
            .with_chainstore(store)
            .with_chain_params(params)
            .with_assume_valid(av, net)
            .toggle_ibd(true)
            .build()
            .map_err(to_js_err)?;

        Ok(Self {
            inner: Arc::new(chain),
        })
    }

    // -- Queries -----------------------------------------------------------

    /// Returns the height of the best known chain tip.
    #[wasm_bindgen(js_name = "getBestBlockHeight")]
    pub fn get_best_block_height(&self) -> Result<u32, JsError> {
        let (height, _) = self.inner.get_best_block().map_err(to_js_err)?;
        Ok(height)
    }

    /// Returns the hash of the best known chain tip as a hex string.
    #[wasm_bindgen(js_name = "getBestBlockHash")]
    pub fn get_best_block_hash(&self) -> Result<String, JsError> {
        let (_, hash) = self.inner.get_best_block().map_err(to_js_err)?;
        Ok(hash.to_string())
    }

    /// Returns the block hash at the given height as a hex string.
    #[wasm_bindgen(js_name = "getBlockHash")]
    pub fn get_block_hash(&self, height: u32) -> Result<String, JsError> {
        let hash = self.inner.get_block_hash(height).map_err(to_js_err)?;
        Ok(hash.to_string())
    }

    /// Returns true if the node is in Initial Block Download mode.
    #[wasm_bindgen(js_name = "isInIbd")]
    pub fn is_in_ibd(&self) -> bool {
        self.inner.is_in_ibd()
    }

    /// Returns the height up to which blocks have been fully validated.
    #[wasm_bindgen(js_name = "getValidationIndex")]
    pub fn get_validation_index(&self) -> Result<u32, JsError> {
        self.inner.get_validation_index().map_err(to_js_err)
    }

    // -- Header acceptance -------------------------------------------------

    /// Accept a raw block header (80 bytes, hex-encoded).
    ///
    /// This extends the header chain without full block validation.  Returns
    /// the resulting best-chain height.
    #[wasm_bindgen(js_name = "acceptHeader")]
    pub fn accept_header(&self, header_hex: &str) -> Result<u32, JsError> {
        let bytes = hex_decode(header_hex).map_err(str_to_js_err)?;
        let header: bitcoin::block::Header = deserialize(&bytes).map_err(to_js_err)?;
        self.inner.accept_header(header).map_err(to_js_err)?;
        let (height, _) = self.inner.get_best_block().map_err(to_js_err)?;
        Ok(height)
    }

    /// Accept multiple raw block headers (concatenated 80-byte headers, hex-encoded).
    #[wasm_bindgen(js_name = "acceptHeaders")]
    pub fn accept_headers(&self, headers_hex: &str) -> Result<u32, JsError> {
        let bytes = hex_decode(headers_hex).map_err(str_to_js_err)?;
        if bytes.len() % 80 != 0 {
            return Err(JsError::new("headers length must be a multiple of 80 bytes"));
        }
        for chunk in bytes.chunks(80) {
            let header: bitcoin::block::Header = deserialize(chunk).map_err(to_js_err)?;
            self.inner.accept_header(header).map_err(to_js_err)?;
        }
        let (height, _) = self.inner.get_best_block().map_err(to_js_err)?;
        Ok(height)
    }

    // -- Block connection --------------------------------------------------

    /// Connect (validate and apply) a full serialized block (hex-encoded).
    ///
    /// The block's header must have been accepted first via `acceptHeader`.
    /// On success returns the new validation index height.
    #[wasm_bindgen(js_name = "connectBlock")]
    pub fn connect_block(&self, block_hex: &str) -> Result<u32, JsError> {
        let bytes = hex_decode(block_hex).map_err(str_to_js_err)?;
        let block: Block = deserialize(&bytes).map_err(to_js_err)?;
        self.inner.connect_block(&block, Default::default(), Default::default(), Default::default()).map_err(to_js_err)?;
        self.inner.get_validation_index().map_err(to_js_err)
    }

    // -- State toggling ----------------------------------------------------

    /// Toggle Initial Block Download mode on or off.
    #[wasm_bindgen(js_name = "toggleIbd")]
    pub fn toggle_ibd(&self, ibd: bool) {
        self.inner.toggle_ibd(ibd);
    }

    /// Flush any cached state.  No-op for the in-memory store, but kept for
    /// API compatibility.
    pub fn flush(&self) -> Result<(), JsError> {
        self.inner.flush().map_err(to_js_err)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_network(s: &str) -> Result<Network, String> {
    match s {
        "bitcoin" | "mainnet" => Ok(Network::Bitcoin),
        "testnet" | "testnet3" => Ok(Network::Testnet),
        "signet" => Ok(Network::Signet),
        "regtest" => Ok(Network::Regtest),
        _ => Err(format!(
            "unknown network \"{s}\"; expected bitcoin, testnet, signet, or regtest"
        )),
    }
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("hex string has odd length".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests (native)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use floresta_chain::BlockchainInterface;
    use floresta_chain::ChainStore;

    use super::memory_chain_store::MemoryChainStore;

    #[test]
    fn memory_chain_store_basic() {
        let store = MemoryChainStore::new();
        assert!(store.load_height().unwrap().is_none());
        assert!(store.check_integrity().is_ok());
    }

    #[test]
    fn create_regtest_chainstate() {
        let params = floresta_chain::ChainParams::from(bitcoin::Network::Regtest);
        let store = MemoryChainStore::new();
        let chain = floresta_chain::pruned_utreexo::chain_state_builder::ChainStateBuilder::new()
            .with_chainstore(store)
            .with_chain_params(params)
            .toggle_ibd(true)
            .build()
            .expect("should build regtest chainstate");
        let (height, _) = chain.get_best_block().unwrap();
        assert_eq!(height, 0);
        assert!(chain.is_in_ibd());
    }

    #[test]
    fn create_signet_chainstate() {
        let params = floresta_chain::ChainParams::from(bitcoin::Network::Signet);
        let store = MemoryChainStore::new();
        let chain = floresta_chain::pruned_utreexo::chain_state_builder::ChainStateBuilder::new()
            .with_chainstore(store)
            .with_chain_params(params)
            .toggle_ibd(true)
            .build()
            .expect("should build signet chainstate");
        let (height, _) = chain.get_best_block().unwrap();
        assert_eq!(height, 0);
    }

    #[test]
    fn parse_network_variants() {
        use super::parse_network;
        assert!(parse_network("bitcoin").is_ok());
        assert!(parse_network("mainnet").is_ok());
        assert!(parse_network("testnet").is_ok());
        assert!(parse_network("signet").is_ok());
        assert!(parse_network("regtest").is_ok());
        assert!(parse_network("fakenet").is_err());
    }

    #[test]
    fn hex_decode_works() {
        use super::hex_decode;
        assert_eq!(hex_decode("deadbeef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
        assert!(hex_decode("xyz").is_err());
        assert!(hex_decode("0").is_err()); // odd length
    }
}
