# `getroots`

Returns the current Utreexo accumulator roots for the UTXO set.

## Usage

### Synopsis

```bash
floresta-cli getroots
```

### Examples

```bash
floresta-cli getroots
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

- (json array of strings) An array of hex-encoded Utreexo root hashes.

```json
[
  "abc123def456...",
  "789ghi012jkl..."
]
```

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- Utreexo is a hash-based dynamic accumulator for the Bitcoin UTXO set.
- The roots allow proving that a UTXO exists without storing the entire UTXO set.
- The number of roots is logarithmic in the number of UTXOs.
- These roots are essential for Utreexo proof verification.
- This is what makes Floresta a lightweight node - it stores only these roots instead of the full UTXO set.
