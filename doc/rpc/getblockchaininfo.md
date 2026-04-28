# `getblockchaininfo`

Returns general information about the current state of the blockchain, including block height, validation progress, and network difficulty.

## Usage

### Synopsis

```bash
floresta-cli getblockchaininfo
```

### Examples

```bash
# Get comprehensive blockchain information
floresta-cli getblockchaininfo
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

Returns a JSON object with the following fields:

- `bestblockhash` - (string) The hash of the best (most-work) block we know about. This is the latest block in the most PoW chain, which may or may not be fully validated yet.

- `blocks` - (numeric) The height of the most-work fully-validated chain. During IBD, this number will be smaller than `headers`; after IBD completes, it should be equal to `headers`.

- `initialblockdownload` - (boolean) Whether the node is currently in Initial Block Download (IBD) mode.

- `headers` - (numeric) The count of headers we have validated. During IBD, this number will be larger than `blocks`; after IBD completes, it should be equal to `blocks`.

- `chainwork` - (string) Total amount of work in the active chain, in hexadecimal.

- `time` - (numeric) The UNIX timestamp for the latest block, as reported by the block's header.

- `chain` - (string) A short string representing the blockchain network (e.g., "bitcoin", "testnet", "signet").

- `verificationprogress` - (numeric) The validation progress as a decimal between 0 and 1. A value of 0 means no blocks have been validated, while 1 means all blocks are validated (headers == blocks).

- `difficulty` - (numeric) The current network difficulty. On average, miners need to make `difficulty` hashes before finding one that solves a block's Proof-of-Work.

- `mediantime` - (numeric) The median block time of the last 11 blocks, expressed in UNIX epoch time.

- `bits` - (string) nBits: compact representation of the block difficulty target, in hexadecimal.

- `target` - (string) The difficulty target, in hexadecimal.

- `pruned` - (boolean) Whether the blocks are subject to pruning. Always `true` for Floresta since it does not store full blocks.

- `pruneheight` - (numeric) Height of the last pruned block plus one. In Floresta, always equals `blocks + 1` since every validated block is immediately pruned.

- `automatic_pruning` - (boolean) Whether automatic pruning is enabled. Always `true` for Floresta.

- `prune_target_size` - (numeric) Target on-disk size used for pruning, in bytes. Always `0` in Floresta since blocks are not retained on disk.

- `signet_challenge` - (string or null) The block challenge used on signet networks. Always `null` in Floresta; not yet exposed on signet.

- `warnings` - (array of strings) Any network and blockchain warnings.

- `size_on_disk` - (numeric) The total size, in bytes, of the chain-store files persisted by Floresta. See the note below on what this value represents.


### Error Enum `CommandError`

- `JsonRpcError::ChainWorkOverflow` - Overflow occurred while calculating accumulated chain work
- `JsonRpcError::BlockNotFound` - The requested block hash was not found in the blockchain
- `JsonRpcError::Chain` - If there's an error accessing blockchain data.

## Notes

- During IBD, some features may be limited.
- `pruned`, `automatic_pruning`, and `prune_target_size` are hardcoded (`true`, `true`, `0`) because Floresta does not store raw blocks or undo files locally.
- `pruneheight` always equals `blocks + 1` since pruning is immediate. Detect pruning via `pruned`; do not use `pruneheight` to decide which blocks to fetch.
- `warnings` is hardcoded to `[]`. Floresta does not currently pipe network or node warnings here.
- `signet_challenge` is hardcoded to `null`. Floresta does not currently expose the signet challenge script.
- `blocks`, `headers`, `difficulty`, `mediantime`, `bits`, `target`, and `chainwork` are dynamically calculated and behave identically to Bitcoin Core.
- `size_on_disk` reports the **total allocated capacity** of Floresta's memory-mapped files, not the amount actually written. A fresh node may report gigabytes even with only the genesis block. On filesystems that support sparse files, physical usage (reported by `du`) will be much smaller. This differs from Bitcoin Core, which starts near zero and grows as `blk*.dat` files are written.
