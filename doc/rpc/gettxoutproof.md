# `gettxoutproof`

Returns the Merkle proof that one or more transactions were included in a block.

## Usage

### Synopsis

```bash
floresta-cli gettxoutproof <txids> [blockhash]
```

### Examples

```bash
floresta-cli gettxoutproof '["txid1", "txid2"]'
floresta-cli gettxoutproof '["abc123..."]' "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
```

## Arguments

- `txids` - (json array, required) A JSON array of transaction IDs to prove. Each element should be a valid transaction ID (64-character hex string).
- `blockhash` - (string, optional) The block hash to look for the transactions in. If not specified, Floresta will search in the most recent blocks.

## Returns

### Ok Response

- `proof` - (string) A hex-encoded Merkle proof string that proves the transactions were included in the specified block.

### Error Enum `CommandError`

Returns `null` if the proof cannot be generated (e.g., transaction not found).

## Notes

- The proof can be verified using `verifytxoutproof` on Bitcoin Core or compatible implementations.
- All transactions must be in the same block.
- This is useful for SPV (Simplified Payment Verification) proofs.
