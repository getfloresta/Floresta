# Phase 3.3: BitAsset Validation Rules Stub (Issuance, Controllers, OP_SPLIT, DEX/Auction Classification)

**Branch**: `bitassets-lite-signet`  
**Date**: 2026-05-18  
**Status**: Plan + First Minimal Implementation + Tests

## Executive Summary

Phase 3.3 initiates the **bitasset protocol interpretation layer** for the lite client. Building directly on the proven high-version transaction acceptance (3.1) and the orthogonal `BitAssetIndex` observer (3.2), we introduce the first semantic validation/ classification primitives under the existing `bitassets` feature gate.

The deliverable consists of:
- `BitAssetTxKind` (and supporting `BitAssetValidationError`) types.
- A pure `classify_bitasset_transaction` + `validate_bitasset_transaction` pair that recognizes the primary operations (v10 issuance with controller detection, v11 DEX claim, v12 auction bid) using the transaction version and output structure.
- A lightweight `transaction_contains_op_split` scanner as the entry point for future OP_SPLIT semantics.
- Updated documentation and a few observability counters in the existing `BitAssetIndex`.
- Unit tests exercising realistic (minimal) example transactions for each class.

Everything is **strictly additive, feature-gated, and side-effect free** with respect to the core Bitcoin consensus paths, `Consensus`, `UtxoData`, the Utreexo Stump, and normal mainnet / non-bitasset signet operation. No new crates, no file creation for Rust sources, no mutation of validation logic.

This lays the foundation for Phase 3.4+ richer work: asset-quantity conservation checks (once `wants_spent_utxos` is enabled), controller lifecycle state, full DEX/auction matching rules, and per-asset amount fields in `AssetUtxo`.

## Background: Protocol Rules (from Authoritative Spec)

The governing design is Paul Sztorc’s “BitAssets – A Digital Assets Sidechain” (truthcoin.info/blog/bit-assets/, 2018) as implemented by the plain-bitassets Rust node on the dedicated drivechain signet.

Relevant rules for a lite-client validator (interpretation + structural checks, not full script execution):
- **v10 CreateAsset**: Exactly one new asset per such transaction. First output is the controller (exactly 1 indivisible “asset unit” for mutable/issuable assets). Second output is the genesis supply (arbitrary quantity). Any further outputs are BTC carrier / change. For immutable crypto-collectibles the controller is effectively burned (sent to unspendable). Asset metadata (ticker 7B + tagline 50B + payload hash 32B) is part of the creation but not yet parsed here.
- **Controller outputs**: Grant the holder the right to issue more supply, edit metadata/tagline, or transfer control. Later validation will enforce that only controller spends can perform admin actions.
- **OP_SPLIT (opcode 90 / 0x5a in the sidechain numbering)**: Used **only for output-type inference** when a transaction moves multiple asset types or splits outputs of the same asset. It appears as a prefix byte in the corresponding output script; it is not a general spending constraint. The lite validator only needs to *detect* its presence for now.
- **v11 Claim DEX order**: References a prior placed order (via an extra 36-byte slot in the transaction header) and “transcribes” the requested output to the seller while delivering the offered asset to the buyer.
- **v12+ Auction bids/settlements**: Dutch (and other) auctions for permissionless crowdsales/liquidations. Use unique 4-byte auction IDs and a standardized 8-byte price format (“dust per asset unit”).
- Conservation: For ordinary transfers the sum of asset quantities (the repurposed 8-byte value fields) of each asset type must balance between inputs and outputs. Issuance/destruction is only allowed via controller-authorized v10 or equivalent admin paths.
- The lite client never runs the sidechain’s full script interpreter; it trusts Utreexo + Bitcoin consensus for block validity and only adds the orthogonal asset-state machine on top.

The current `BitAssetIndex` still uses the Phase 3.2 heuristic (txid of any v10–v12 tx as `AssetId`). Future work will use the new classifier to distinguish true issuance events and propagate asset identity across spends.

## Design Principles (Unchanged from 3.1/3.2)

- **Orthogonal & Non-Mutating**: Zero edits to `floresta-chain` consensus functions, `update_acc`, leaf format, or `ChainState` control flow.
- **Feature-Gated & Dead-Code Safe**: Entirely behind `#[cfg(feature = "bitassets")]`. Default builds are identical to before.
- **Lite-Client Friendly**: Pure functions on already-validated `&Transaction`. No network, no database, minimal CPU. Classification results can later be fed into the index or a higher wallet layer.
- **Incremental & Reviewable**: The smallest useful surface that demonstrates the intended architecture for protocol rules.
- **Reuses Existing Test Infrastructure**: All new tests live in the `mod tests` of `consensus.rs` and reuse the `build_tx_with_version`, `txout!`, `make_minimal_high_version_tx*` helpers introduced in 3.1.

## Implementation Location & Changes

- **Primary file for types & logic**: `crates/floresta-chain/src/pruned_utreexo/chain_state.rs`
  - New Phase 3.3 subsection appended after the existing `BitAssetIndex` / `BlockConsumer` implementation (around line 291).
  - New public (under cfg) items:
    - `BitAssetTxKind`
    - `BitAssetValidationError`
    - `classify_bitasset_transaction`
    - `validate_bitasset_transaction`
    - `transaction_contains_op_split`
  - Minor enhancement to `BitAssetIndexInner` (new counter) and `process_high_version_tx` to call the classifier (demonstrates integration without changing behavior).
  - Documentation comments updated on `AssetUtxo`.
- **Re-exports**: Extend the conditional export in `crates/floresta-chain/src/lib.rs`.
- **Tests**: Added inside the existing `mod tests` block in `crates/floresta-chain/src/pruned_utreexo/consensus.rs` (under `#[cfg(feature = "bitassets")]`), immediately after the Phase 3.2 index tests. They construct representative v10/v11/v12 transactions and assert correct classification + error paths.
- **No new Rust source files**. The plan document is the only new file (explicitly requested by the phase mission).

## The Minimal API (First Cut)

```rust
#[cfg(feature = "bitassets")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BitAssetTxKind {
    /// v10 issuance / creation transaction.
    /// `has_controller` distinguishes mutable tokens (controller output with qty 1)
    /// from immutable collectibles (controller burned / absent in first position).
    Issuance { has_controller: bool, genesis_amount: u64 },

    /// v11 transaction claiming a previously placed DEX order.
    DexClaim,

    /// v12 (and higher) auction bid or settlement transaction.
    AuctionBid,

    /// Any other high-version transaction (future protocol extensions).
    GenericHighVersion,
}

#[cfg(feature = "bitassets")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BitAssetValidationError {
    /// A v10 transaction must have at least the controller + genesis outputs.
    IssuanceRequiresAtLeastTwoOutputs,
    // Future variants: ConservationFailure, InvalidControllerAction, ...
}

#[cfg(feature = "bitassets")]
pub fn classify_bitasset_transaction(tx: &Transaction) -> BitAssetTxKind {
    match tx.version.0 {
        10 => {
            let has_controller = tx.output.len() >= 2 && tx.output[0].value.to_sat() == 1;
            let genesis_amount = tx.output.get(1).map(|o| o.value.to_sat()).unwrap_or(0);
            BitAssetTxKind::Issuance { has_controller, genesis_amount }
        }
        11 => BitAssetTxKind::DexClaim,
        12 => BitAssetTxKind::AuctionBid,
        _ if (10..=12).contains(&tx.version.0) => BitAssetTxKind::GenericHighVersion,
        _ => BitAssetTxKind::GenericHighVersion, // defensive
    }
}

#[cfg(feature = "bitassets")]
pub fn validate_bitasset_transaction(tx: &Transaction) -> Result<BitAssetTxKind, BitAssetValidationError> {
    let kind = classify_bitasset_transaction(tx);
    if let BitAssetTxKind::Issuance { .. } = kind {
        if tx.output.len() < 2 {
            return Err(BitAssetValidationError::IssuanceRequiresAtLeastTwoOutputs);
        }
    }
    Ok(kind)
}

/// Returns true if any output script in the transaction contains the protocol’s
/// OP_SPLIT marker (sidechain opcode 90 / 0x5a used for multi-asset type inference).
/// This is a pure byte scan; no script execution is performed.
#[cfg(feature = "bitassets")]
pub fn transaction_contains_op_split(tx: &Transaction) -> bool {
    const OP_SPLIT: u8 = 0x5a; // protocol-specific marker per the BitAssets design
    tx.output.iter().any(|out| out.script_pubkey.as_bytes().contains(&OP_SPLIT))
}
```

The index’s `process_high_version_tx` is lightly extended to record the kind (for test observability) while preserving the Phase 3.2 asset_id heuristic.

## Unit Tests Added

All under `#[cfg(all(test, feature = "bitassets"))]` in `consensus.rs`:

- `test_classify_v10_issuance_with_controller`: build v10 tx with outputs (value=1, value=1_000_000, BTC change); assert `Issuance { has_controller: true, ... }`.
- `test_classify_v10_collectible_style`: v10 with first output value != 1 or only one asset output; assert the flag correctly reflects “no controller”.
- `test_classify_v11_and_v12`: assert DexClaim / AuctionBid.
- `test_validate_rejects_malformed_v10`: v10 with <2 outputs yields the specific error.
- `test_op_split_detector`: construct tx whose second output script begins with OP_SPLIT byte; assert the scanner returns true. Also negative case.
- `test_validation_integration_with_index`: feed a mixed block (v10 issuance + v12) to a fresh `BitAssetIndex`; verify both the existing counters and that the new classification path was exercised (via an added diagnostic method or by extending existing stats).

All pre-existing tests (Phases 3.1/3.2 and the full consensus suite) continue to pass with the feature both enabled and disabled.

## Risks, Constraints & Compliance

- **Never mutate core consensus / accumulator**: 100% satisfied. New code lives in a completely separate module section and is never called from `Consensus` or `ChainState::connect_block`.
- **Minimal & incremental**: ~80–120 lines of new code + comments + tests. One new public enum, three pure functions, one counter, one re-export line.
- **Feature gating & dead code**: Verified by conditional compilation and by running the full test matrix with and without `--features bitassets`.
- **Lite-client priority**: No script verification, no signature checks, no assumption of full UTXO history beyond what the index already receives via the consumer contract. All heavy lifting remains with the sidechain full nodes or future proof-carrying extensions.
- **Future compatibility**: The classification is intentionally coarse; richer rules (quantity vectors per asset, controller ownership, auction price curves, OP_SPLIT-driven multi-type splitting) are explicitly left for later phases once the index can request spent UTXOs and store per-output asset metadata.

## Build & Verification Commands (Executed in This Phase)

```bash
cargo check -p floresta-chain --features bitassets
cargo test -p floresta-chain --features bitassets -- --quiet
cargo clippy -p floresta-chain --features bitassets -- -D warnings

# Also verify the feature-off path remains pristine
cargo test -p floresta-chain -- --quiet
cargo clippy -p floresta-chain -- -D warnings
```

## Next Steps (Post Phase 3.3)

- Enable `wants_spent_utxos = true` on `BitAssetIndex` (behind a second fine-grained cfg or same feature) and begin propagating asset identity + quantities across spends.
- Store richer `AssetUtxo` records containing the asset-specific amount and a `BitAssetTxKind` tag.
- Implement basic conservation rule inside the validator (sum of input quantities per asset == sum of output quantities, except for controller-authorized issuance).
- Begin parsing the 89-byte creation metadata payload for ticker/tagline (still inside the feature).
- Consider a thin `BitAssetsExt` trait on `Transaction` (in `extensions.rs`) for ergonomic calls from wallet code.
- Design (still orthogonal) Utreexo extension proofs for compact per-asset inclusion / exclusion.

## Files Changed / Added

- `docs/bitassets-lite-wallet/phase-3.3-validation-rules-plan.md` (this document)
- `crates/floresta-chain/src/pruned_utreexo/chain_state.rs` (new types, functions, integration into index, doc updates)
- `crates/floresta-chain/src/lib.rs` (extended conditional re-export)
- `crates/floresta-chain/src/pruned_utreexo/consensus.rs` (new classification + validation tests)

---

*This phase follows the exact incremental, safety-first, reviewable spirit of Phases 3.1 and 3.2. The implementation is deliberately the smallest surface that makes “validation rules for bitassets” a living, testable concept inside the lite client while guaranteeing zero impact on ordinary Bitcoin operation.*

*Assisted-by: Grok 4.3 (xAI) – plan authoring and initial implementation*
