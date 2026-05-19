# Local Signet PR-Readiness Notes

## Current Architecture

The bitassets lite-wallet path is intentionally split into three layers:

- `plain-bitassets` remains the sidechain authority for asset state and exposes proof-oriented archive context through `get_transaction_proof`.
- `florestad --features bitassets` imports the sidechain UTXO view through JSON-RPC, records compact proof provenance, persists it in `bitassets-index.json`, and can delegate wallet-write actions to the configured sidechain RPC.
- `floresta-electrum` serves asset wallet views and delegated wallet actions through `blockchain.asset.*` methods.

Mainchain Utreexo changes are out of scope for this path. The relevant proof and archive material belongs to the bitassets sidechain. Floresta uses mainchain connectivity for Drivechain context and uses sidechain RPC/proof metadata for asset provenance.

## Proof Status

Electrum asset UTXOs and history entries include:

- `proof`: either `sidechain_rpc_proof` or `trusted_snapshot`
- `block_hash`: sidechain block containing the asset transaction, when known
- `sidechain_height`: sidechain block height, when proof RPC supplied it
- `bmm_inclusions`: mainchain blocks committing to the sidechain block
- `best_main_verification`: best mainchain verification hash reported by the sidechain node

`sidechain_rpc_proof` means Floresta received sidechain archive context from `get_transaction_proof`. It does not mean Floresta has independently verified the full sidechain state machine. Full verifier-backed persistence is a later design step.

## Electrum Wallet Surface

When built with `--features bitassets` and started with `--enable-bitassets`, Floresta exposes:

- `blockchain.asset.list`
- `blockchain.asset.get_balance`
- `blockchain.asset.listunspent`
- `blockchain.asset.get_history`

When `--bitassets-rpc-url` is also configured, Floresta exposes delegated wallet actions:

- `blockchain.asset.get_new_address`
- `blockchain.asset.reserve`
- `blockchain.asset.register`
- `blockchain.asset.transfer`

The write methods intentionally use the plain-bitassets sidechain wallet/RPC for construction, signing, and broadcast in this v1 scope. Floresta then observes the resulting sidechain state through its refresh loop and persists the compact proof-backed view.

## Canonical Local Validation

From the Mac local signet:

```bash
cd ~/drivechain-wallet-dev/local-dev
BITASSETS_IMAGE=local/plain-bitassets:codex-proof \
  ./scripts/pr-ready-bitassets-smoke.sh
```

The smoke:

- checks available disk before doing heavy work
- optionally rebuilds patched bitassets with `REBUILD_BITASSETS=1`
- optionally resets the stack with `RESET_STACK=1` or `RESET_STACK=1 RESET_VOLUMES=1`
- creates/loads the local mainchain miner wallet after a clean reset
- activates sidechain ID 4 from `local-dev/activate-plain-bitassets-id4.json` when needed
- funds the enforcer wallet through dynamic wallet-owned coinbase recipients
- mines bitassets sidechain blocks through BMM
- creates/registers/transfers a fresh asset
- verifies `get-transaction-proof` for the transfer and `null` for an unknown txid
- starts Floresta in live sidechain RPC mode
- verifies asset Electrum balance/UTXOs/history with `sidechain_rpc_proof`
- initiates an additional asset transfer through Floresta Electrum and verifies the refreshed wallet state
- restarts Floresta without sidechain RPC and verifies persisted-cache service

Latest Mac validation:

```bash
RESET_STACK=1 RESET_VOLUMES=1 REBUILD_BITASSETS=0 MIN_FREE_GB=6 \
BITASSETS_IMAGE=local/plain-bitassets:codex-proof \
EXPECTED_PROOF_STATUS=sidechain_rpc_proof \
./scripts/pr-ready-bitassets-smoke.sh
```

This passed from fresh compose volumes, activated sidechain ID 4, mined sidechain blocks through BMM, created/transferred a fresh asset, and verified both live RPC refresh and persisted-cache service. The canonical Floresta proof smoke also passed again after restarting mainchain, enforcer, and bitassets.

Latest wallet-action smoke:

```bash
BITASSETS_IMAGE=local/plain-bitassets:codex-proof \
EXPECTED_PROOF_STATUS=sidechain_rpc_proof \
./scripts/floresta-bitassets-electrum-smoke-test.sh
```

```json
{"mode":"rpc-refresh","asset_id":"9b2b37b359067deb1414d05a2ecc6ff94016e5a78d788af5285472db0a169fb0","balance":1000,"utxos":2,"history":2}
{"mode":"rpc-refresh-wallet-transfer","asset_id":"9b2b37b359067deb1414d05a2ecc6ff94016e5a78d788af5285472db0a169fb0","balance":1000,"utxos":3,"history":3}
{"mode":"persisted-cache","asset_id":"9b2b37b359067deb1414d05a2ecc6ff94016e5a78d788af5285472db0a169fb0","balance":1000,"utxos":3,"history":3}
```

## Required Rust Checks

```bash
cd ~/drivechain-wallet-dev/plain-bitassets
cargo check -p plain_bitassets_app_rpc_api -p plain_bitassets_app_cli -p plain_bitassets_app
cargo test -p plain_bitassets_app_rpc_api

cd ~/drivechain-wallet-dev/floresta-bitassets
cargo fmt --check
cargo check -p florestad --features bitassets
cargo test -p floresta-chain --features bitassets --lib -- --quiet
cargo test -p floresta-electrum --features bitassets --lib -- --quiet
cargo test -p floresta-node --features bitassets --lib -- --quiet
```

## Known Limits

- Apple Silicon runs the LayerTwo Docker images under amd64 emulation. Use native amd64 for final heavy validation if Docker or disk pressure becomes unstable.
- The local enforcer may return a `broadcast deposit transaction failed` response after the BMM request has already been persisted. The patched local `plain-bitassets` miner treats that Mac/QEMU-specific response as non-fatal and confirms the BMM after the paired L1 block is mined.
- The v1 Floresta cache stores compact proof refs, not full proof bundles.
- Floresta exposes delegated wallet-write methods for address creation, reservation, registration, and transfer. Transaction construction/signing/broadcast remains delegated to plain-bitassets for this PR-ready increment rather than implementing an independent Floresta signer.
