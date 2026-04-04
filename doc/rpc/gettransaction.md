# `gettransaction`

Returns detailed information about a wallet transaction, assuming it is cached by the watch-only wallet.

## Usage

### Synopsis

```bash
floresta-cli gettransaction <txid> [verbose]
```

### Examples

```bash
# Get transaction details with verbose output (default behavior)
floresta-cli gettransaction 4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b

# Explicitly request verbose output
floresta-cli gettransaction 4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b true
```

## Arguments

`txid` - (string, required) The transaction id (txid) of the transaction to retrieve.

`verbose` - (boolean, optional, default=true) If true, returns a JSON object with detailed transaction information. If false, returns the same detailed JSON object.

## Returns

### Ok Response

* `in_active_chain`: (boolean) Whether this transaction is in the active chain.
* `hex`: (string) The hex-encoded raw transaction.
* `txid`: (string) The transaction id (same as provided).
* `hash`: (string) The transaction hash (differs from txid for witness transactions).
* `size`: (numeric) The serialized transaction size in bytes.
* `vsize`: (numeric) The virtual transaction size (differs from size for segwit transactions).
* `weight`: (numeric) The transaction's weight (between vsize*4-3 and vsize*4).
* `version`: (numeric) The transaction version.
* `locktime`: (numeric) The transaction locktime.
* `vin`: (array) Array of transaction inputs:
  * `txid`: (string) The transaction id of the previous output.
  * `vout`: (numeric) The output index of the previous output.
  * `script_sig`: (object) The script signature:
    * `asm`: (string) The assembly representation of the script.
    * `hex`: (string) The hex-encoded script.
  * `sequence`: (numeric) The script sequence number.
  * `witness`: (array) Array of hex-encoded witness data (if any).
* `vout`: (array) Array of transaction outputs:
  * `value`: (numeric) The value in satoshis.
  * `n`: (numeric) The output index.
  * `script_pub_key`: (object) The script public key:
    * `asm`: (string) The assembly representation of the script.
    * `hex`: (string) The hex-encoded script.
    * `req_sigs`: (numeric) The required number of signatures.
    * `type`: (string) The type (e.g., pubkeyhash, scripthash, witness_v0_keyhash).
    * `address`: (string) The Bitcoin address (if available).
* `blockhash`: (string) The hash of the block containing this transaction.
* `confirmations`: (numeric) The number of confirmations. 0 if unconfirmed.
* `blocktime`: (numeric) The block time expressed in UNIX epoch time.
* `time`: (numeric) Same as blocktime.

### Error Enum `JsonRpcError`

* `JsonRpcError::TxNotFound` - The transaction was not found in the watch-only wallet cache.

## Notes

- This RPC only returns transactions that have been cached by the watch-only wallet. To cache transactions, you must first load a descriptor using `loaddescriptor` and optionally trigger a rescan with `rescanblockchain`.
- The `verbose` parameter is accepted for API compatibility, but the detailed JSON response is always returned regardless of its value.
