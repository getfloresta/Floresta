# `getblockheader`

Retrieve information about a specific block header by its hash. The verbosity parameter determines the format of the returned data.

## Usage

### Synopsis

```bash
floresta-cli getblockheader <blockhash> [verbosity]
```

### Examples

```bash
# Returns a JSON object with detailed block header information (default verbosity = true)
floresta-cli getblockheader "000000000000000000007ae6247b184396b8a1a292b8435508f448669ead45a6"

# Returns a serialized, hex-encoded string of the block header data (verbosity = false)
floresta-cli getblockheader "000000000000000000007ae6247b184396b8a1a292b8435508f448669ead45a6" false

# Returns a JSON object with detailed block header information (verbosity = true)
floresta-cli getblockheader "000000000000000000007ae6247b184396b8a1a292b8435508f448669ead45a6" true
```

## Arguments

- `blockhash` - (string, required) The block hash.
- `verbosity` - (bool, optional, default=true)
  - `false`: Returns a serialized, hex-encoded string of the block header data.
  - `true`: Returns a JSON object with detailed block header information.

## Returns

### Ok Response (for verbosity = false)

- `"hex"` - (string) A serialized, hex-encoded string of the block header data.

### Ok Response (for verbosity = true)

Return JSON object
- `confirmations` - (numeric) The number of confirmations.
- `height` - (numeric) The block height or index.
- `version` - (numeric) The block version.
- `versionHex` - (string) The block version formatted in hexadecimal.
- `merkleroot` - (string) The merkle root.
- `time` - (numeric) The block time expressed in UNIX epoch time.
- `mediantime` - (numeric) The median block time expressed in UNIX epoch time.
- `nonce` - (numeric) The nonce.
- `bits` - (string) Compact representation of the block difficulty target.
- `target` - (string) The difficulty target.
- `difficulty` - (numeric) The difficulty.
- `chainwork` - (string) Expected number of hashes required to produce the chain up to this block (in hex).
- `nTx` - (numeric) The number of transactions in the block.
- `previousblockhash` - (string, optional) The hash of the previous block.
- `nextblockhash` - (string, optional) The hash of the next block.

### Error Enum `CommandError`

* `JsonRpcError::ChainWorkOverflow` - Overflow occurred while calculating accumulated chain work
* `JsonRpcError::BlockNotFound` - The requested block hash was not found in the blockchain
* `JsonRpcError::Chain` - If there's an error accessing blockchain data

## Notes

- To retrieve block hashes, you can use the `getblockhash` RPC to obtain the hash of a specific block by its height, or the `getbestblockhash` RPC to get the hash of the latest known block. These hashes can then be used with the `getblockheader` RPC to retrieve detailed block information.
- **In regtest**, the difficulty value may not match real-world conditions due to easier mining.