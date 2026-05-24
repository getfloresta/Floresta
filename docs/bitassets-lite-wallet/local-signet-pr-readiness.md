# Local Signet PR-Readiness Notes

## Current Architecture

The issue #28 path now uses a native Floresta BitAssets wallet instead of the old delegated Electrum write path:

- `plain-bitassets` remains the sidechain authority and exposes script-hash lite-wallet updates, proof refs, and raw authorized transaction broadcast.
- `florestad --features bitassets` owns BitAssets seed persistence, deterministic addresses, watched script hashes, wallet UTXOs, proof-verified sync, local signing, and native transaction construction.
- QUIC lite-wallet subscription is the primary live update path; `get_lite_wallet_update` JSON-RPC polling remains the recovery/debug mirror.
- Existing Electrum `blockchain.asset.*` methods remain available for compatibility, but they are no longer the main wallet architecture.

The native wallet file is stored separately from the Electrum cache as `bitassets-wallet.json` under the Floresta data directory. It persists owned addresses, script hashes, wallet UTXOs, last sidechain tip, and proof refs.

## Public Floresta Surface

Start `florestad` with:

```bash
--enable-bitassets
--bitassets-rpc-url http://127.0.0.1:6004
--enable-bitassets-native-wallet
--bitassets-lite-wallet-quic-url 127.0.0.1:6104
--bitassets-wallet-create
```

The native JSON-RPC methods are:

- `bitassets_getnewaddress`
- `bitassets_walletinfo`
- `bitassets_sync`
- `bitassets_listutxos`
- `bitassets_getbalance`
- `bitassets_transfer`
- `bitassets_reserve`
- `bitassets_register`
- `bitassets_amm_mint`
- `bitassets_amm_swap`
- `bitassets_amm_burn`
- `bitassets_dutch_auction_create`
- `bitassets_dutch_auction_bid`
- `bitassets_dutch_auction_collect`

Constructor V1 accepts only `fee_sats = 0`; nonzero fees fail explicitly until Bitcoin UTXO fee selection is added.

## Mobile Embedding Surface

`crates/floresta-bitassets-wallet` is now the reusable wallet boundary for daemon and mobile callers. It still reuses the proven native wallet implementation, but exports a mobile-safe facade:

- `mobile::EmbeddedBitAssetsWallet` accepts JSON-shaped params matching the Floresta JSON-RPC methods.
- `mobile_ffi` exposes a C ABI for iOS and JNI exports for Android.
- The crate builds as `rlib`, `staticlib`, and `cdylib` so RedWallet can link it directly.
- `include/floresta_bitassets_wallet.h` documents the C ABI used by Swift or C/C++ bridges.

Build example:

```bash
cd /Users/lukekensik/drivechain-wallet-dev/floresta-bitassets
./scripts/build-bitassets-wallet-mobile.sh aarch64-apple-ios-sim
```

Android targets require the Android Rust targets plus an NDK linker environment before producing `.so` files for RedWallet `jniLibs`.

## Canonical Live Smoke

The local smoke harness lives in `local-dev`, which is not a git repository here, so it should be referenced from PR notes rather than committed in this branch:

Full local signet proof-backed Electrum/cache smoke:

```bash
cd /Volumes/T705/code/drivechain-wallet-dev/local-dev
DOCKER_BUILDKIT=0 \
BITASSETS_PLATFORM=linux/arm64 \
BITASSETS_BUILD_PLATFORM=linux/arm64 \
RESET_STACK=1 \
RESET_VOLUMES=1 \
REBUILD_BITASSETS=0 \
BITASSETS_IMAGE=local/plain-bitassets:codex-proof \
./scripts/pr-ready-bitassets-smoke.sh
```

Latest passing proof-backed Electrum/cache result:

```json
{
  "mainchain_height": 113,
  "bitassets_sidechain_height": 5,
  "sidechain_activation_height": 108,
  "asset_id": "1f7d29cb94f4678610ce298f1d91f07ed1b36201de54e9e43ac67a2d546e287e",
  "reserve_tx": "fc15abb51d0c503a7f47cedc8b8e84be367f5230426bc4b283abd5b132799afd",
  "register_tx": "7f8ff6a7e5e5d0ecd2ad37dfcde0deb58205138d06a3b2cf84fc2e765a4b4d17",
  "transfer_tx": "6cba6f2825f7253ca82b49c3ab7747c701497abcb00131fb9feccfa1daf3883a",
  "floresta_wallet_transfer_tx": "b65d4d890f7693ebd947efe49292a8f5ece0f66753afb75f4c4db189b8b980cb",
  "checks": [
    {"mode": "rpc-refresh", "balance": 1000, "utxos": 2, "history": 2},
    {"mode": "rpc-refresh-wallet-transfer", "balance": 1000, "utxos": 3, "history": 3},
    {"mode": "persisted-cache", "balance": 1000, "utxos": 3, "history": 3}
  ],
  "persisted_cache": "/tmp/floresta-bitassets-signet-smoke/signet/bitassets-index.json"
}
```

The restart half of the smoke logged `Loaded 3 persisted plain-bitassets sidechain UTXO(s)` from the persisted cache file before the `persisted-cache` Electrum assertion passed.

```bash
cd /Users/lukekensik/drivechain-wallet-dev/local-dev
PREPARE_STACK=0 \
BITASSETS_IMAGE=local/plain-bitassets:codex-proof \
BITASSETS_QUIC_URL=127.0.0.1:6104 \
BMM_MINE_ATTEMPTS=8 \
BMM_REQUEST_SETTLE_SECS=40 \
BITASSETS_MINE_TIMEOUT=120 \
./scripts/floresta-bitassets-native-wallet-smoke.sh
```

The smoke proves all native wallet flows against Docker signet using QUIC-driven wallet updates and a restart persistence check:

- native address creation
- reserve/register for two assets
- transfer
- AMM mint/swap/burn
- Dutch auction create/bid/collect
- direct plain-bitassets transaction shape checks
- persisted Floresta native wallet balances after restart

The post-rebase validation run used stack preparation and longer Mac/QEMU wait windows:

```bash
PREPARE_STACK=1 \
BITASSETS_IMAGE=local/plain-bitassets:codex-proof \
BITASSETS_QUIC_URL=127.0.0.1:6104 \
BMM_MINE_ATTEMPTS=12 \
BMM_REQUEST_SETTLE_SECS=40 \
BITASSETS_MINE_TIMEOUT=180 \
WALLET_WAIT_SECS=240 \
QUIC_WAIT_SECS=90 \
./scripts/floresta-bitassets-native-wallet-smoke.sh
```

Latest passing result:

```json
{
  "mode": "native-wallet",
  "asset_a": "7c7bc226ca3a53bc549cdb17c6b7002fc2c56c2086e48579598ff6a950ea482f",
  "asset_b": "993f25719b66763ffcb36683b58cfa0edd42a9defa0ddb2d3bdd920f5d732c58",
  "txids": {
    "transfer": "2c50b836c2d49441112060a0a4bc6e6ba0d34a211fc1d61c5b4dcc3a45eeebe1",
    "reserve_a": "0a057b47396b4541821ac896713bfe33c0e6cc3aced6166bdf2039a9b8b9082b",
    "register_a": "ead5fc378486d91ab32e3e1dcb4e277c18f51db55cdb051ecd1ab642040b8221",
    "reserve_b": "772a996b6f957bcaab427ccc853e54e569a6df33a5e62ad162056c09305fa885",
    "register_b": "bb48994f50d81c0f5eb325b6227009958415ef08674dedc6a2cb34d4a32eeda9",
    "amm_mint": "d9e8a63e925631a6e7a991fc7a643043e0dda4a17214f8d291f59636062e0bc7",
    "amm_swap": "4dafb6dbb72638e480cece9d774526364c63f688d65c74eb2d3dce0e6a624cc4",
    "amm_burn": "e3480003519a2141945fbf5c6242dc1150c61c6fdc0341b1c055f75ab9731d5f",
    "dutch_auction_create": "9e294e6f60c705e7d7f197ca6c85792c2bedfd1436ad18362523fda086204e63",
    "dutch_auction_bid": "2b34ef4cde69036fcbf22f93b9cc13e5c5d8ae610d60f6d0daac38438f18decc",
    "dutch_auction_collect": "de5d48259183581b9d2ad4b25e928f21c162d134308527dfdc977106242d7c90"
  },
  "final_balances": {
    "7c7bc226ca3a53bc549cdb17c6b7002fc2c56c2086e48579598ff6a950ea482f": 9090,
    "993f25719b66763ffcb36683b58cfa0edd42a9defa0ddb2d3bdd920f5d732c58": 9106,
    "control:7c7bc226ca3a53bc549cdb17c6b7002fc2c56c2086e48579598ff6a950ea482f": 1,
    "control:993f25719b66763ffcb36683b58cfa0edd42a9defa0ddb2d3bdd920f5d732c58": 1,
    "lp:7c7bc226ca3a53bc549cdb17c6b7002fc2c56c2086e48579598ff6a950ea482f:993f25719b66763ffcb36683b58cfa0edd42a9defa0ddb2d3bdd920f5d732c58": 900
  }
}
```

## Required Rust Checks

```bash
cd /Users/lukekensik/drivechain-wallet-dev/floresta-bitassets
cargo check -p florestad --features bitassets
cargo test -p floresta-node --features bitassets --lib -- --quiet
cargo test -p floresta-electrum --features bitassets --lib -- --quiet
```

The coordinating `plain-bitassets` PR should also report:

```bash
cd /Users/lukekensik/drivechain-wallet-dev/plain-bitassets
cargo check -p plain_bitassets_app_rpc_api -p plain_bitassets_app_cli -p plain_bitassets_app
cargo test -p plain_bitassets --lib -- --quiet
cargo test -p plain_bitassets_app --bin plain_bitassets_app -- --quiet
```

## PR Draft Notes

Suggested title:

```text
Add native BitAssets lite wallet with QUIC sync and constructors
```

Summary bullets:

- Adds a Floresta-owned native BitAssets wallet with seed persistence, deterministic plain-bitassets-compatible address derivation, script-hash watches, proof-verified sync, and local signing.
- Adds native JSON-RPC wallet methods for address creation, sync, balances, UTXOs, transfer, reserve/register, AMM, and Dutch auction flows.
- Starts a QUIC subscription task when configured and falls back to explicit JSON-RPC sync for recovery/debug.
- Preserves existing delegated Electrum asset methods for compatibility.

Known limits for reviewers:

- Sidechain Bitcoin fees are intentionally zero-fee only in this PR.
- Compact filters and stronger privacy are out of scope for issue #28 closure.
- QUIC uses newline-delimited JSON messages for this PR-ready protocol pass.
