# `gettransaction`

Get detailed information about in-wallet transaction.

## Usage

### Synopsis

```bash
floresta-cli gettransaction <txid> (verbose)
```

### Examples

```bash
# gettransaction from txid, verbose 
floresta-cli gettransaction aa5f3068b53941915d82be382f2b35711305ec7d454a34ca69f8897510db7ab8 true
```

## Arguments

`txid` - (string, required) The transaction id.

`verbose` - (boolean, optional) Whether to include a decoded field containing the decoded transaction

## Returns

### Ok Response (for verbosity = 0)

* `"hex"` - (string) A serialized, hex-encoded string of the transaction data.

### Ok Response (for verbosity = 1)

Return Json object

* `txid` - (string) The transaction id.
* `hash` - (string) The transaction hash (differs from txid for witness transactions).
* `version` - (numeric) The version.
* `size` - (numeric) The transaction size.
* `vsize` - (numeric) The virtual transaction size (differs from size for witness transactions).
* `weight` - (numeric) The transaction's weight.
* `locktime` - (numeric) The lock time.
* `vin` - (json array) An array of transaction inputs:
  * `txid` - (string) The transaction id.
  * `vout` - (numeric) The output number.
  * `script_sig` - (json object) The script:
    * `asm` - (string) symbolic decoded instruction of the script.
    * `hex` - (string) script hex.
  * `sequence` - (numeric) The script sequence number.
  * `witness` - (json array) Hex-encoded witness data (if any).
* `vout` - (json array) An array of transaction outputs:
  * `value` - (numeric) The value in BTC.
  * `n` - (numeric) index.
  * `script_pub_key` - (json object):
    * `asm` - (string) the symbolic decoded instruction of the script.
    * `hex` - (string) the script hex.
    * `req_sigs` - (numeric) The required signatures.
    * `type` - (string) The type, eg 'pubkeyhash'.
    * `address` - (string) bitcoin address.

### Error Enum `CommandError`

* `JsonRpcError::TxNotFound`
