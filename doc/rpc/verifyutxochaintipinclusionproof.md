# `verifyutxochaintipinclusionproof`

Verifies a Utreexo accumulator inclusion proof for the chain tip.

## Usage

### Synopsis

```bash
floresta-cli verifyutxochaintipinclusionproof <proof> [verbosity]
```

### Examples

```bash
# Verify a valid proof
floresta-cli verifyutxochaintipinclusionproof "7be8d37f896c5679fc370f78618352758f8313267f1ad3348e32fa7d7fdb411501000678463e22379a90e33854981d711045382f5a897d578944a32625a4676472b87ea7b27aa09d99d88888dc70af5cf032afd4d3b92a77db2501ff133a46bf11780afe345534cc16c23330a6eb00f8bcc8fef9c2c48237bf6ce35cb9474a6ca93556e6b07d99729145a9eff0b40ab0401c0b38136b20498753e109fcdc869587b4565236f49e081ba525b9e100d0a73d6c734a4b7c89dcff97646f5f30f95974aaefdef19c978c859a62f3885a22424c34e657fd8170a63b13a72e3202b044d65399010000006e6d53328389a2ebf90239ee0a9884fd46fa03c471bebf37cde90f6dd49812c9"

# Verify an invalid proof (returns false)
floresta-cli verifyutxochaintipinclusionproof "7be8d37f896c5679fc370f78618352758f8313267f1ad3348e32fa7d7fdb411501000678463e22379a90e33854981d711045382f5a897d578944a32625a4676472b87ea7b27aa09d99d88888dc70af5cf032afd4d3b92a77db2501ff133a46bf11780afe345534cc16c23330a6eb00f8bcc8fef9c2c48237bf6ce35cb9474a6ca93556e6b07d99729145a9eff0b40ab0401c0b38136b20498753e109fcdc869587b4565236f49e081ba525b9e100d0a73d6c734a4b7c89dcff97646f5f30f95974aaefdef19c978c859a62f3885a22424c34e657fd8170a63b13a72e3202b044d65399010000006e6d53328389a2ebf90239ee0a9884fd46fa03c471bebf37cde90f6dd4981210"

# Verify with detailed output (verbosity 1 — returns JSON object)
floresta-cli verifyutxochaintipinclusionproof "7be8d37f...c9" 1
```

## Arguments

`proof` - (String, required) Hex-encoded proof from `utreexod` `proveutxochaintipinclusion` RPC.

`verbosity` - (u32, optional, default=0) Level of detail in the response:
- `0`: Returns `true` or `false`.
- `1`: Returns a JSON object with `valid`, `proved_at_hash`, `targets`, `num_proof_hashes`, `proof_hashes` and `hashes_proven`.

## Returns

### Ok Response
### Verbosity 0 (default)
- `true|false` - (boolean) Returns `true` if the proof is cryptographically valid, `false` if the proof is invalid but well-formed.

### Verbosity 1
```json
{
  "valid": true,
  "proved_at_hash": "6d11188b...",
  "targets": [0],
  "num_proof_hashes": 3,
  "proof_hashes": ["a6b5a202...", "4bc79324...", "590d644e..."],
  "hashes_proven": ["e1e92857..."]
}
```

### Error Enum `CommandError`

* `JsonRpcError::InvalidHex` - The proof is not valid hexadecimal.
* `JsonRpcError::Decode` - Malformed proof: too large, too short, truncated data, invalid varints, or trailing bytes.
* `JsonRpcError::InvalidProof` - Well-formed proof but invalid: stale (wrong block height) or cryptographic verification failed.
* `JsonRpcError::InvalidVerbosityLevel` - Verbosity value is not 0 or 1.

## Notes

- The proof must be generated at the **exact same block height** as the current chain tip. Stale proofs will return `InvalidProof` error.
- This RPC validates that UTXOs are included in the Utreexo accumulator at the current chain tip using cryptographic proof verification.
- To generate a proof, use the `proveutxochaintipinclusion` RPC from `utreexod` with transaction IDs and output index.
- Related RPCs: `getbestblockhash` (to verify current chain tip hash).