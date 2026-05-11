# `invalidateblock`

Permanently marks a block as invalid, as if it violated a consensus rule. All descendants of the block are also marked invalid and the tip rolls back to the parent of the invalidated block.

## Usage

### Synopsis

```bash
floresta-cli invalidateblock <blockhash>
```

### Examples

```bash
# Invalidate a specific block
floresta-cli invalidateblock "00000000000000000002a7c4c1e48d76c5a37902165a270156b7a8d72f9a68c5"
```

## Arguments

`blockhash` - (string, required) The hash of the block to mark as invalid, in hexadecimal format (64 characters).

## Returns

### Ok Response

Returns `null` on success.

### Error Enum `CommandError`

* `JsonRpcError::BlockNotFound` - If the specified block hash is not found in the blockchain or the invalidation fails.

## Notes

- This is a hidden RPC in Bitcoin Core, intended for testing and debugging.
- The accumulator state is not modified; only the best known block and validation index are rolled back.
- All blocks from the invalidated height up to the current tip are marked as `InvalidChain`.
- There is currently no `reconsiderblock` equivalent to undo this operation.
