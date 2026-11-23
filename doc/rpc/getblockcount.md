# `getblockcount`

Returns the height of the most-work fully-validated chain. The genesis block has height 0.

While the node is still syncing, this can be lower than the best known header
height. It always matches the `blocks` field of `getblockchaininfo`; the header
height is its `headers` field.

## Usage

### Synopsis

floresta-cli getblockcount

### Examples

```bash
floresta-cli getblockcount
```

## Returns

### Ok Response

- `height` - (numeric) The current block count
