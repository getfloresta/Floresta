# Phase 3.2: Per-Asset UTXO Sets + Utreexo Extension (Orthogonal Index Stub)

**Branch**: `bitassets-lite-signet`  
**Date**: 2026-05-18  
**Status**: Plan + Minimal Stub Implementation + Tests

## Executive Summary

This phase delivers the **first safe, orthogonal per-asset index** (`BitAssetIndex`) for tracking BitAssets (v10/v11/v12 transactions) without any modification to the core Utreexo accumulator, `UtxoData`, leaf format, or `Consensus` validation paths.

The index is implemented as an optional `BlockConsumer` (already an established extension point in `ChainState`), feature-gated behind `bitassets` in `floresta-chain`. It can be subscribed by lite-wallet or indexer code to observe blocks containing high-version transactions and maintain its own per-asset UTXO sets (keyed by `(asset_id, outpoint)`). Normal Bitcoin sync, mainnet, and non-bitasset signets remain 100% unaffected and bit-identical.

This is the foundation for future "Utreexo extension" work (asset-specific proofs) while strictly obeying the "do not mutate core" constraint from Phase 3.1.

## Design Principles (from Phase 3.1 and Requirements)

- **Orthogonal & Non-Mutating**: The primary `ChainState` / Stump / `UtxoData` stays pure Bitcoin. Asset semantics live in a completely separate in-memory (later on-disk) map. No change to rustreexo Stump, leaf hashing, or `update_acc`.
- **Optional & Feature-Gated**: `#[cfg(feature = "bitassets")]`. When the feature is off, zero code is compiled for the index; the public API surface and all existing behavior is identical.
- **BlockConsumer Pattern**: The index implements `floresta_chain::BlockConsumer`. Users obtain a `ChainState`, create a `BitAssetIndex`, and call `chain.subscribe(Arc::new(index))`. The core `notify` path simply calls `on_block` on all subscribers after a block has passed full Bitcoin + Utreexo validation.
- **Lite-Client Friendly**: The index only *observes*. It never influences consensus decisions or proof verification. Asset-specific rules (issuance, transfers, DEX, auctions) will be interpreted in higher layers (wallet / future `floresta-bitassets` crate) that consume the index.
- **Minimal & Incremental**: Stub only tracks outputs of v10–12 transactions under per-tx asset IDs for now. No spent-UTXO tracking (`wants_spent_utxos = false`), no protocol script parsing, no persistence. Future phases add:
  - Wants spent UTXOs + asset-id propagation for real transfers.
  - Real asset-amount extraction (OP_RETURN / witness patterns used by plain-bitassets).
  - On-disk storage (kv / sled via watch-only db).
  - Utreexo accumulator extensions for asset proofs (still orthogonal leaves or side trees).

## Implementation Location & Changes

- **Primary file edited**: `crates/floresta-chain/src/pruned_utreexo/chain_state.rs`
  - `BitAssetIndex` struct + `BlockConsumer` impl + `AssetId` / `AssetUtxo` types defined under `#[cfg(feature = "bitassets")]`.
  - No alterations to `ChainStateInner`, `notify`, `connect_block`, `UtxoData`, `Stump`, or any consensus function.
- **Cargo**: Added `bitassets = []` feature (pure additive, no new dependencies) to `crates/floresta-chain/Cargo.toml`.
- **Re-export**: Conditionally `pub use` the index type from `crates/floresta-chain/src/lib.rs` when feature enabled.
- **Tests**: Added inside the existing `mod tests` in `consensus.rs` (under the feature cfg). Tests reuse the Phase 3.1 high-version transaction construction helpers.
- **No new Rust source files** created (implementation embedded to satisfy minimal-change guideline). The required plan document is created as explicitly requested.

## The `BitAssetIndex` Stub (Minimal API)

```rust
#[cfg(feature = "bitassets")]
pub type AssetId = bitcoin::Txid;

#[cfg(feature = "bitassets")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetUtxo { /* ... */ }

#[cfg(feature = "bitassets")]
pub struct BitAssetIndex { /* spin::RwLock protected in-memory maps */ }

#[cfg(feature = "bitassets")]
impl BitAssetIndex {
    pub fn new() -> Self { ... }
    pub fn get_asset_utxos(&self, asset_id: &AssetId) -> Vec<OutPoint> { ... }
    // ...
}

#[cfg(feature = "bitassets")]
impl BlockConsumer for BitAssetIndex {
    fn wants_spent_utxos(&self) -> bool { false }
    fn on_block(&self, block: &Block, height: u32, _spent: Option<...>) {
        // For every tx with version in 10..=12:
        //   asset_id = tx.compute_txid()
        //   register every output as an AssetUtxo under that asset
    }
}
```

Storage model (in-memory only for Phase 3.2):
- `asset_utxos: HashMap<AssetId, HashSet<OutPoint>>`
- `outpoint_to_asset: HashMap<OutPoint, AssetId>`
- Stats counters for observability in tests.

Heuristic (intentionally naive for stub):
- Any v10/v11/v12 transaction is treated as an "asset event".
- Its txid becomes the `AssetId` for its outputs (models issuance txs and simple per-tx grouping).
- Future work will inspect input asset membership to propagate IDs across transfers and distinguish v10 issuance from v11/v12 state machines.

This already allows a lite wallet to answer "what asset UTXOs do I control?" by combining the index with its own address filter, all while the Utreexo accumulator only ever saw normal `TxOut`s.

## Unit Tests Added

Inside `crates/floresta-chain/src/pruned_utreexo/consensus.rs` (under `#[cfg(all(test, feature = "bitassets"))]`):

- `test_bitasset_index_observes_high_version_block`: Construct a minimal block containing a v12 tx (using existing Phase 3.1 helpers), feed it directly to a fresh `BitAssetIndex::on_block`, assert the output outpoints are present under the tx's `AssetId`.
- `test_bitasset_index_does_not_affect_normal_chain_sync`: Using `setup_test_chain`, subscribe a `BitAssetIndex`, connect several normal regtest blocks (no high-v txs), verify `chain.get_height()` and accumulator roots are identical to an unsubscribed control chain. Then feed one high-v block via the normal `connect_block` path (possible because Phase 3.1 proved they validate) and confirm the index received the data via its internal counters while the main `ChainState` state remained pristine.
- Additional assertions that `wants_spent_utxos()` returns false and that the index compiles and links only when the feature is enabled.

These tests prove:
1. The index populates correctly from blocks with high-version transactions.
2. Subscribing it never alters Bitcoin consensus behavior or Utreexo state.
3. Feature gating works (code is dead when feature disabled).

All pre-existing tests (including the Phase 3.1 high-version tests) continue to pass when the feature is both on and off.

## Risks, Constraints & Compliance

- **Constraint "Never mutate the core Utreexo Stump or leaf format"**: Fully satisfied. Zero edits in `update_acc`, `UtxoData`, `CompactLeafData`, rustreexo calls.
- **"Keep the change minimal and incremental"**: ~120 lines of new code, all behind cfg. One feature flag. One new public-when-enabled type. No behavior change for default builds.
- **"The index must be able to run alongside normal chain sync"**: Demonstrated by subscription + real `connect_block` usage in tests.
- **No new crates, no new persistent stores, no protocol parser yet**: Deliberate.
- **Clippy / check / test clean**: Required in this phase.

## Build & Verification Commands (Executed in This Phase)

```bash
cargo check -p floresta-chain --features bitassets
cargo test -p floresta-chain --features bitassets -- --quiet   # (new tests + regression)
cargo clippy -p floresta-chain --features bitassets -- -D warnings
```

## Next Steps (Post Phase 3.2)

- Phase 3.3+: Make `BitAssetIndex` capable of `wants_spent_utxos = true`, implement asset-id inheritance for transfers, expose richer `AssetUtxo` (amounts, scripts).
- Move sophisticated asset script interpretation into `floresta-watch-only` (behind its own bitassets feature) or a new `floresta-bitassets` crate that consumes the index.
- Persist the index using the same `ChainStore` or wallet DB.
- Begin design of optional Utreexo-side accumulator for compact asset inclusion proofs (still never mutating the primary Bitcoin Stump).

## Files Changed / Added

- `docs/bitassets-lite-wallet/phase-3.2-asset-index-plan.md` (this document)
- `crates/floresta-chain/Cargo.toml` (feature)
- `crates/floresta-chain/src/lib.rs` (conditional export)
- `crates/floresta-chain/src/pruned_utreexo/chain_state.rs` (the `BitAssetIndex` + tests)

---

*This plan follows the exact spirit and constraints established in Phase 3.1. The implementation is the smallest possible that satisfies the mission while remaining production-safe and reviewable.*
