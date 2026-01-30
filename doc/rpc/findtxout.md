# `findtxout`

Searches for a specific UTXO in the blockchain using compact block filters.

## Usage

### Synopsis

```bash
floresta-cli findtxout <txid> <vout> <script> [height_hint]
```

### Examples

```bash
floresta-cli findtxout "abc123..." 0 "76a914..."
floresta-cli findtxout "abc123..." 1 "76a914..." 700000
```

## Arguments

- `txid` - (string, required) The transaction ID containing the output.
- `vout` - (numeric, required) The output index (0-based).
- `script` - (string, required) The scriptPubKey of the output as a hex-encoded string.
- `height_hint` - (numeric, optional, default=0) A block height hint to start searching from. Providing an accurate hint speeds up the search significantly.

## Returns

### Ok Response

A JSON object containing:

- `value` - (numeric) The value of the UTXO in satoshis.
- `scriptPubKey` - (object) Information about the output script.

Returns an empty object if the UTXO doesn't exist or has been spent.

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- This method requires compact block filters to be enabled (`blockfilters=1`).
- Unlike `gettxout`, this searches the entire blockchain, not just cached outputs.
- The `height_hint` greatly improves performance by limiting the search range.
- Useful for finding arbitrary UTXOs without having them pre-cached.
- The search can be slow without a height hint, especially for old transactions.
