# `findtxout`

Searches for a specific unspent transaction output (UTXO) using compact block filters.

## Usage

### Synopsis

```bash
floresta-cli findtxout <txid> <vout> <script> <height_hint>
```

### Examples

```bash
# Look for the output 0 of a given transaction, locking script and a height hint to start the search from
floresta-cli findtxout aa5f3068b53941915d82be382f2b35711305ec7d454a34ca69f8897510db7ab8 0 76a914...88ac 800000
```

## Arguments

`txid` - (string, required) The id of the transaction containing the output.

`vout` - (numeric, required) The index of the output to look for.

`script` - (string, required) The hex-encoded `scriptPubKey` that locks the output, used to match against the compact block filters.

`height_hint` - (numeric, required) A block height to start the filter search from. Providing a height close to when the output was created speeds up the search.

## Returns

### Ok Response

* `value` - (numeric) The output value, in satoshis;
* `script_pubkey` - (string) The hex-encoded locking script of the output.

Returns an empty object if the output could not be found.

### Error Enum `CommandError`

* `JsonRpcError::InvalidScript`
* `JsonRpcError::InInitialBlockDownload`
* `JsonRpcError::NoBlockFilters`
* `JsonRpcError::Filters`
* `JsonRpcError::Node`

## Notes

* This command requires block filters to be enabled, by setting the `blockfilters=1` option in the configuration.
* If the node is still in the initial block download, the search can't be performed since the local filter index isn't complete yet, and `JsonRpcError::InInitialBlockDownload` is returned.
* Floresta first checks its own wallet cache for the output before falling back to a filter-based search.
