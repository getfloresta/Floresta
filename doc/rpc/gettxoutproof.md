# `gettxoutproof`

Returns a hex-encoded Merkle proof showing that one or more transactions were included in a block.

## Usage

### Synopsis

```bash
floresta-cli gettxoutproof <txids> [blockhash]
```

### Examples

```bash
# Get proof for a single transaction (block is looked up automatically)
floresta-cli gettxoutproof '["txid1"]'

# Get proof for multiple transactions in a specific block
floresta-cli gettxoutproof '["txid1", "txid2"]' 000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f
```

## Arguments

`txids` - (JSON array of strings, required) The transaction IDs to prove. Must be a JSON array of hex-encoded transaction IDs. All transactions must be in the same block.

`blockhash` - (string, optional) The hash of the block in which to look for the transactions. If omitted, the block is determined from the first transaction ID using the watch-only wallet cache.

## Returns

### Ok Response

- (string) A hex-encoded serialized MerkleBlock structure. This proof can be verified by any node to confirm that the transactions were included in the specified block.

### Error Enum

- `JsonRpcError::TxNotFound` - If one or more of the specified transaction IDs are not found in the block.
- `JsonRpcError::BlockNotFound` - If the specified block hash does not correspond to a known block.
- `JsonRpcError::Chain` - If there is an error accessing blockchain data.

## Notes

- All transaction IDs must belong to the same block. If a `blockhash` is not provided, the block is inferred from the first transaction ID.
- The returned proof should be compatible with Bitcoin Core's `verifytxoutproof` RPC.
