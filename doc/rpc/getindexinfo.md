# `getindexinfo`

Returns the status of one or all available indices currently running on the node.

## Usage

### Synopsis

```bash
floresta-cli getindexinfo [<index_name>]
```

### Examples

```bash
# Get status of all available indices
floresta-cli getindexinfo

# Get status of the block filter index specifically
floresta-cli getindexinfo block_filter
```

## Arguments

| Name         | Type   | Required | Description                                                                                       |
| ------------ | ------ | -------- | ------------------------------------------------------------------------------------------------- |
| `index_name` | string | No       | Filter results for an index with a specific name. If omitted, all available indices are returned. |

## Returns

### Ok Response

Returns a JSON object where each key is an index name and each value contains:

- `synced` - (boolean) Whether the index is fully synced to the chain tip
- `best_block_height` - (numeric) The block height to which the index is synced

```json
{
  "block_filter": {
    "synced": true,
    "best_block_height": 850000
  },
  "backfill": {
    "synced": false,
    "best_block_height": 450000
  }
}
```

If no indices are enabled (e.g., florestad was started with `--no-cfilters` and without `--backfill`), an empty object `{}` is returned.

If `index_name` is specified but does not match any available index, an empty object `{}` is returned.

### Error Enum

- `JsonRpcError::Filters` - If there is an error querying the block filter storage.
- `JsonRpcError::Chain` - If there is an error querying the chain state.

## Available Indices

| Name           | Description                                                                                                      | Always present |
| -------------- | ---------------------------------------------------------------------------------------------------------------- | -------------- |
| `block_filter` | BIP157/BIP158 compact block filter index. Only present when block filters are enabled (without `--no-cfilters`). | No             |
| `backfill`     | Historical block validation after assume-utreexo IBD. Only present when backfill is enabled (`--backfill`).      | No             |

## Notes

- This RPC method is modeled after Bitcoin Core's `getindexinfo`. Bitcoin Core tracks different indices (txindex, coinstatsindex, blockfilterindex) that do not apply to Floresta's architecture.
- The `block_filter` index is only present when compact block filters are enabled. It is considered synced when the filter height has caught up with the chain tip height.
- The `backfill` index tracks the progress of historical block validation that runs after an assume-utreexo initial sync. It is considered synced once all assumed blocks have been fully validated.
