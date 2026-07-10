# `findtxout`

Finds a specific unspent transaction output (UTXO) in the chain.

This is a Floresta flavored RPC and is not part of the Bitcoin Core RPC spec. Unlike
`gettxout`, which only looks at outputs already cached by the wallet, `findtxout` uses
block filters to scan the chain for an output that the wallet doesn't know about yet. If
the output is found it is cached, so subsequent calls to `gettxout` will return it too.

## Usage

### Synopsis

```
floresta-cli findtxout <txid> <vout> <script> [<height_hint>]
```

### Examples

```bash
# Look for output 0 of a transaction, providing its scriptPubKey hex
floresta-cli findtxout aa5f3068b53941915d82be382f2b35711305ec7d454a34ca69f8897510db7ab8 0 0014c664139327b98043febeab6434eba89bb196d1af

# Same lookup, but start scanning from block height 800000 to speed things up
floresta-cli findtxout aa5f3068b53941915d82be382f2b35711305ec7d454a34ca69f8897510db7ab8 0 0014c664139327b98043febeab6434eba89bb196d1af 800000
```

## Arguments

`txid` - (string, required) The transaction id that created the output.

`vout` - (numeric, required) The index of the output within that transaction.

`script` - (string, required) The scriptPubKey of the output, hex encoded. This is what gets
matched against the block filters, so it must be correct for the output to be found.

`height_hint` - (numeric, optional, default=0) The block height to start scanning from. Providing
a height close to where the output was created avoids scanning the whole chain and makes the
lookup much faster.

## Returns

### Ok Response

If the output is found, an object describing it is returned. This is a variant of the
[`gettxout`](gettxout.md) response, currently returning just:

- `value` - (numeric) The output value in satoshis.
- `script_pubkey` - (string) The output scriptPubKey, hex encoded.

In the future `findtxout` will return the same response shape as `gettxout` directly.

### Error Enum `JsonRpcError`

- `InInitialBlockDownload` - The node is still doing its initial block download, so there are no
filters to scan yet.
- `NoBlockFilters` - Block filters are not enabled. You must start the node with `blockfilters=1`
to use this method.
- `Filters` - Something went wrong while matching against the block filters.
- `Node` - A node level error happened while fetching a candidate block.
- `BlockNotFound` - A matched block could not be located in the chain.

## Notes

- If the output can't be found (it doesn't exist or has already been spent), an empty object `{}`
is returned.
- You must enable block filters by setting the `blockfilters=1` option, otherwise this method
returns `NoBlockFilters`.
- Passing a good `height_hint` matters a lot for performance, since without it the scan starts
from the beginning of the chain.
- Related methods: `gettxout`, which returns an output only if it is already in the wallet cache.
