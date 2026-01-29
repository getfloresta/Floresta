# `getblockchaininfo`

Returns general information about the current state of the blockchain, including block height, validation progress, network difficulty, and utreexo accumulator statistics.

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

- `best_block` - (string) The hash of the best (most-work) block we know about. This is the latest block in the most PoW chain, which may or may not be fully validated yet.

- `height` - (numeric) The depth of the most-work chain we know about, representing the total number of blocks.

- `ibd` - (boolean) Whether the node is currently in Initial Block Download (IBD) mode.

- `validated` - (numeric) How many blocks have been fully validated so far. During IBD, this number will be smaller than `height`. After IBD completes, it should equal `height`.

- `latest_work` - (string) The work performed by the last block. This is the estimated number of hashes the miner had to perform before mining that block, on average.

- `latest_block_time` - (numeric) The UNIX timestamp for the latest block, as reported by the block's header.

- `leaf_count` - (numeric) The number of leaves in the utreexo accumulator. This should be equal to the number of UTXOs (equivalent to Bitcoin Core's `gettxoutsetinfo` result).

- `root_count` - (numeric) The number of roots in the utreexo accumulator.

- `root_hashes` - (array of strings) The actual hex-encoded roots of the utreexo accumulator.

- `chain` - (string) A short string representing the blockchain network (e.g., "main", "test", "signet", "regtest").

- `progress` - (numeric) The validation progress as a decimal between 0 and 1. A value of 0 means no blocks have been validated, while 1 means all blocks are validated (validated == height).

- `difficulty` - (numeric) The current network difficulty. On average, miners need to make `difficulty` hashes before finding one that solves a block's Proof-of-Work.

### Error Enum `CommandError`

* `JsonRpcError::ChainWorkOverflow`
* `JsonRpcError::BlockNotFound`
* `JsonRpcError::Chain`

## Notes

- During IBD, some features may be limited.