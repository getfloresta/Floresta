# `getblockheader`

Returns the block header for a given block hash.

## Usage

### Synopsis

```bash
floresta-cli getblockheader <hash>
```

### Examples

```bash
floresta-cli getblockheader "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
```

## Arguments

- `hash` - (string, required) The block hash (64-character hex string) of the block whose header you want to retrieve.

## Returns

### Ok Response

A JSON object containing the block header fields:

- `version` - (numeric) The block version.
- `prev_blockhash` - (string) The hash of the previous block.
- `merkle_root` - (string) The Merkle root of all transactions in the block.
- `time` - (numeric) The block timestamp (Unix epoch time).
- `bits` - (string) The difficulty target in compact format.
- `nonce` - (numeric) The nonce used for proof-of-work.

### Error Enum `CommandError`

Returns an error if the block hash is not found.

## Notes

- Block headers are 80 bytes and contain essential metadata about the block.
- The header does not include transaction data, only the Merkle root.
- This is useful for light clients that need to verify the chain without downloading full blocks.
- The `prev_blockhash` field links blocks together to form the blockchain.
