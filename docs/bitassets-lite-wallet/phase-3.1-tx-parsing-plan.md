# Phase 3.1: Transaction Parsing and Version Handling for BitAssets Lite Client

**Branch**: `bitassets-lite-signet`  
**Date**: 2026-05-18  
**Status**: Plan + First Incremental Implementation (stub + tests)

## Executive Summary

Floresta already fully supports deserializing and processing Bitcoin transactions with arbitrary version numbers, including the v10/v11/v12 range used by plain-bitassets for asset issuance, transfers, DEX orders, and auctions. No changes to the parsing layer are required for correctness or to prevent crashes during chain sync on a bitassets-enabled signet.

This document captures the focused exploration of `crates/floresta-chain/src/pruned_utreexo/` (and related modules), identifies safe extension points, and defines the minimal first deliverable: a set of unit tests exercising high-version transactions through the existing consensus paths.

## Code Exploration Results (Primary Locations)

### Transaction Decoding Sites
- **Sole source of truth**: `bitcoin` crate v0.32 (workspace dep).
  - Used via `bitcoin::consensus::deserialize`, `deserialize_hex`, `deserialize_partial`.
  - Locations: 
    - `floresta-chain/src/pruned_utreexo/consensus.rs` (verify_block_transactions, check_transaction_context_free, tests)
    - `floresta-chain/src/pruned_utreexo/udata.rs` (leaf data roundtrips)
    - `floresta-chain/src/pruned_utreexo/partial_chain.rs`, `flat_chain_store.rs`
    - `floresta-wire/src/p2p_wire/{transport.rs, peer.rs, node/blocks.rs}`
    - `floresta-node/src/json_rpc/server.rs` (raw tx RPC)
- `bitcoin::Transaction { version: bitcoin::transaction::Version(pub i32), ... }` — the deserializer reads a 4-byte little-endian signed integer with **no range restriction**.
- Result: `Version(10)`, `Version(11)`, `Version(12)` deserialize and serialize without error or data loss.

### Version Checks and Consensus Paths
- `Consensus::check_transaction_context_free` (consensus.rs:454): only checks `!input.is_empty()`, `!output.is_empty()`, non-null prevouts for non-coinbase, script sizes, total output value ≤ MAX_MONEY. **Zero references to `transaction.version`**.
- `Consensus::verify_transaction`, `verify_block_transactions`, `verify_block_transactions_swiftsync`, `check_block`: same — structural + economic checks only.
- `check_merkle_root`, weight limits, BIP34, witness commitment: independent of tx version.
- `UtxoData` and accumulator update (`Consensus::update_acc`): operate on `TxOut` only.
- No version gating anywhere that would reject v > 2.

### Utreexo Accumulator / UTXO Representation
- `UtxoData` (pruned_utreexo/mod.rs:376): `{ txout: TxOut, is_coinbase, creation_height, creation_time }`.
- `CompactLeafData` (udata.rs:105): header_code + amount + ScriptPubKeyKind. Reconstructs standard `TxOut`.
- Leaf hashing for the Stump accumulator (via rustreexo) commits to the serialized UTXO + metadata; **tx version is not part of any leaf**.
- `update_acc` (consensus.rs) only adds/removes leaves for outputs/inputs.

### Other Related Modules
- `extensions.rs`: only BIP30 special cases. Natural home for future `BitAssetsExt` trait.
- `error.rs`: `TransactionError` / `BlockValidationErrors` have no version variants.
- Mempool, wire, RPC: all delegate to `bitcoin::Transaction`; no filters on version.

## Precise Answers to the Four Questions

**1. What minimal changes are needed in the parsing layer?**

- **Zero code changes** to parsing or deserialization.
- The existing `bitcoin::consensus` paths are already sufficient and forward-compatible.
- The "stub" implementation for 3.1 consists solely of **new unit tests** (no production logic) that:
  - Construct `Transaction` values with `version: Version(10/11/12)`.
  - Round-trip via serialization.
  - Exercise `check_transaction_context_free` and higher-level block validation helpers.
- Future (post 3.1, behind feature flag): introduce a thin `BitAssetTransactionView<'a>` or detection function (`fn is_bitasset_tx(tx: &Transaction) -> bool { matches!(tx.version.0, 10..=12) }`) in a new module. Never alter the hot consensus paths.

**2. How do we safely extend the Utreexo accumulator / UTXO set for per-asset data without breaking normal Bitcoin rules?**

- **Do not touch the accumulator or `UtxoData` for asset data.**
  - Any mutation of leaf format or addition of asset fields to `UtxoData` would change the Utreexo root for every bitasset-signet block, breaking proof compatibility with other Utreexo nodes and the core Bitcoin commitment.
- Safe approach (lite-client friendly):
  - Keep the primary `ChainState` / Stump **100% Bitcoin-pure**.
  - Introduce an **orthogonal, optional `AssetState`** (or `BitAssetIndex`) that a `BlockConsumer` implementation populates asynchronously when it observes v10+ transactions with recognized asset output patterns.
  - Asset UTXOs are still represented in the normal accumulator (their `TxOut` value is the Bitcoin dust or carrier value); extra semantics (asset ID, amount, controller pubkey, state machine) live in the side index keyed by `OutPoint`.
  - This guarantees that normal mainnet / testnet / regular signet sync is bit-identical and unaffected. Bitasset tracking can be feature-gated (`#[cfg(feature = "bitassets")]`).

**3. Where should asset-specific validation (OP_SPLIT, controller outputs, etc.) live for a lite client (proof verification focused)?**

- **Strictly outside `floresta-chain`'s `Consensus` and `ChainState` validation.**
- Recommended home (for Phase 3.x+):
  - A new `floresta-bitassets` crate (or extension inside `floresta-watch-only` / wallet).
  - Implements `BlockConsumer`.
  - On `on_block`, iterates `block.txdata`; for those with `version.0 >= 10`, performs custom interpretation of scripts / witness data (OP_SPLIT semantics, controller checks, auction state transitions, etc.).
  - Relies on the fact that the block has **already** passed Utreexo proof verification + Bitcoin consensus in the lite client. Only additional asset rules are applied.
  - For full "lite" spirit: many asset rules can be proven via additional inclusion proofs or SPV-style commitments rather than re-executing every script.
- This keeps the core lite client small, auditable, and unchanged for plain Bitcoin usage. Asset validation errors are reported to the wallet layer, not as `BlockchainError::TransactionError`.

**4. What unit / property tests must be added first?**

- **Immediate (this phase)**:
  - In `crates/floresta-chain/src/pruned_utreexo/consensus.rs` (inside `mod tests`):
    - `test_high_version_bitasset_txs_roundtrip`: serialize/deserialize v10, v11, v12; assert `version` preserved.
    - `test_high_version_txs_pass_context_free_checks`: build minimal valid (non-coinbase) txs with v10/v11/v12 using existing `txin!`/`txout!` + `build_tx` generalized, call `Consensus::check_transaction_context_free`, assert `Ok(out_value)`.
    - `test_high_version_tx_in_block_validation`: construct a 2-tx block (coinbase + v12 spend) and exercise `verify_block_transactions` / `validate_block_no_acc` with mocked `UtxoData`.
  - Verify no panic and that the returned values are identical to equivalent v2 transactions (structure only).
- **Also**:
  - Ensure all existing tests (including vendored `tx_valid.json` / `tx_invalid.json` which contain low-version cases) continue to pass.
  - (Optional stretch) Quickcheck/fuzz property: any `Version(v)` where `v >= 1` never triggers a version-dependent early return in the current validation functions.
- These tests act as the "canary" and living documentation that the parsing layer is ready for bitassets.

## Risks & Constraints Compliance

- Lite-client priority: proof verification remains the job of Utreexo + existing `Consensus`; asset rules are post-filters.
- Minimal & incremental: only additive tests in this PR. No behavior change, no new dependencies, no risk to mainnet/signet IBD.
- Non-breaking: all changes are in test code under `#[cfg(test)]`.

## Next Steps (After Architect Approval)

- Implement the tests described above on this branch.
- `cargo check`, `cargo test --package floresta-chain`, `cargo clippy`.
- Append `Assisted-by: Grok 4.3 (xAI) <version>` trailer to the commit.
- Provide architect with: summary, full diff, and this plan file for review before Phase 3.2 (asset detection stub / index).

---

*This plan was produced after exhaustive targeted searches and file reads across the pruned_utreexo consensus paths.*

