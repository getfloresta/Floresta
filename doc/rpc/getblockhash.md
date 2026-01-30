# `getblockhash`

Returns the hash of the block at the specified height in the most-work fully-validated chain.

## Usage

### Synopsis

```bash
floresta-cli getblockhash <height>
```

### Examples

```bash
# Get the genesis block hash (height 0)
floresta-cli getblockhash 0
```

## Arguments

`height` - (numeric, required) The height (block number) of the block whose hash you want to retrieve. Must be a non-negative integer within the current blockchain height.

## Returns

### Ok Response

- `blockhash` - (string) The block hash in hexadecimal format (64 characters).

### Error Enum `CommandError`

* `JsonRpcError::InvalidHeight` - If the specified height is beyond the current blockchain height or is invalid.
* `JsonRpcError::Chain` - If there's an error accessing blockchain data.