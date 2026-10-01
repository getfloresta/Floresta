# `getchaintips`

Return information about all known tips in the block tree, including the main chain as well as orphaned branches.

## Usage

### Synopsis

```bash
floresta-cli getchaintips
```

### Examples

```bash
# List all chain tips
floresta-cli getchaintips
```

## Returns

### Ok Response

Returns a JSON array of objects, each describing one chain tip:

- `height` - (numeric) The height of the chain tip.

- `hash` - (string) The block hash of the tip.

- `branchlen` - (numeric) The number of blocks between the tip and the fork point with the main chain. Zero for the active chain tip.

- `status` - (string) The status of the chain tip. One of:
  - `"active"` — This is the tip of the active best chain, which is certainly valid.
  - `"valid-headers"` — The headers for this branch are valid, but the full blocks have not been validated. In Floresta, this is the only status used for fork tips (see Notes).

### Error Enum

- `JsonRpcError::Chain` — If there is an error accessing chain state data.
- `JsonRpcError::BlockNotFound` — If a tip's block header could not be found while computing branch length.

## Notes

- **Incompatibility with Bitcoin Core**: Floresta's `getchaintips` uses the same JSON response shape as Bitcoin Core, but the `status` field is limited to two values: `"active"` and `"valid-headers"`. The following Bitcoin Core statuses are never returned by Floresta:
  - `"valid-fork"` — Floresta never fully validates fork blocks (only the active chain's blocks are validated), so this status cannot be determined.
  - `"headers-only"` — Floresta does not track this status option because headers only is yet the default behavior.
  - `"invalid"` — Floresta does not persist metadata about branches containing invalid blocks yet.
- `branchlen` is computed by walking back through fork tip headers until reaching a block that belongs to the main chain.
- **Fork tip retention**: Floresta prunes alternative tips that fall too far behind or have too little work compared to the active chain. As a result, `getchaintips` may return fewer fork tips than Bitcoin Core, which retains all known tips in its block index.
