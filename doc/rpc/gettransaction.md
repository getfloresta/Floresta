# `gettransaction`

Returns a transaction from the watch-only wallet cache.

## Usage

### Synopsis

```bash
floresta-cli gettransaction <txid> [verbose]
```

### Examples

```bash
floresta-cli gettransaction "abc123def456..."
floresta-cli gettransaction "abc123def456..." true
```

## Arguments

- `txid` - (string, required) The transaction ID (txid) of the transaction to retrieve.
- `verbose` - (boolean, optional, default=false) If true, returns a JSON object with detailed transaction information. If false, returns the raw transaction hex.

## Returns

### Ok Response (verbose=false)

- (string) The raw transaction as a hex-encoded string.

### Ok Response (verbose=true)

- `txid` - (string) The transaction ID.
- `hash` - (string) The transaction hash.
- `size` - (numeric) The transaction size in bytes.
- `vsize` - (numeric) The virtual transaction size.
- `version` - (numeric) The transaction version.
- `locktime` - (numeric) The transaction locktime.
- `vin` - (json array) Array of input objects.
- `vout` - (json array) Array of output objects.

### Error Enum `CommandError`

Returns an error if the transaction is not found in the wallet cache.

## Notes

- This method only returns transactions that are cached by the watch-only wallet.
- To have transactions cached, you must first load descriptors using `loaddescriptor`.
- For transactions not in the cache, you may need to rescan the blockchain.
