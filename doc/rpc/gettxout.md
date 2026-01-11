# `gettxout`

Returns details about an unspent transaction output.

## Usage

### Synopsis

```bash
floresta-cli gettxout <txid> <n> (include_mempool)
```

### Examples

```bash
# gettxout with a txid, vout 0
floresta-cli gettxout aa5f3068b53941915d82be382f2b35711305ec7d454a34ca69f8897510db7ab8 0
```

## Arguments

`txid` - (string, required) The transaction id.

`n` - (numeric, required) vout number.

`include_mempool` - (not implemented, the argument is available to maintain API consistency but it value will be ignored).

## Returns

### Ok Response

* `bestblock`: (string) The hash of the block at the tip of the chain;
* `confirmations`: (numeric) The number of confirmations;
* `value` : (numeric) The transaction value in BTC;
* `scriptPubKey` : (json object)
  * `asm` : (string) The assembled scriptPubKey
  * `hex` : (string) The raw scriptPubKey
  * `type` : (string) The type, eg pubkeyhash
  * `addresses` : (string) bitcoin addresses
* `coinbase` : (boolean) Coinbase or not.

### Error Enum `CommandError`

* `JsonRpcError::BlockNotFound`
* `JsonRpcError::InvalidScript`
* `JsonRpcError::InvalidDescriptor`
* `JsonRpcError::Encode`

## Notes

* This `rpc` isn't fully implemented mostly because we need to implement a mempool. For more information see [RPC Saga](https://github.com/orgs/getfloresta/projects/5):

* The API accept the `include_mempool` argument but, for now, it does nothing.
