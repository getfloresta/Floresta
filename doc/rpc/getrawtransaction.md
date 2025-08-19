# `getrawtransaction`

Return the raw transaction data.

## Arguments

* `txid` - (string, required) The transaction id

* `verbose` - (boolean, optional) If false, return a string, otherwise return a json object

## Returns

### Response (if `verbose` is not set or set to false)

- The serialized, hex-encoded data for `txid`

### Response (if `verbose` is set to true)

- `in_active_chain`   - (boolean) Whether specified block is in the active chain or not (only present with explicit "blockhash" argument)

- `hex`               - (string) The serialized, hex-encoded data for 'txid'

- `txid`              - (string) The transaction id (same as provided)

- `hash`              - (string) The transaction hash (differs from txid for witness transactions)

- `size`              - (numeric) The serialized transaction size

- `vsize`             - (numeric) The virtual transaction size (differs from size for witness transactions)

- `weight`            - (numeric) The transaction's weight (between vsize*4-3 and vsize*4)

- `version`           - (numeric) The version

- `locktime`          - (numeric) The lock time

- `vin`               - (object array)

    - `txid`            - (string) The transaction id

    - `vout`            - (numeric) The output number

    - `script_sig`      - (object) The script

        - `asm`             - (string) asm

        - `hex`             - (string) hex

    - `sequence`        - (numeric) The script sequence number

    - `witness`         - (strings array) hex-encoded witness data (if any)

- `vout`              - (object array)

    - `value`           - (numeric) The value in BTC

    - `n`               - (numeric) index

    - `script_pub_key`  - (object)

        - `asm`             - (string) the asm

        - `hex`             - (string) the hex

        - `req_sigs`        - (numeric) The required sigs

        - `type_`           - (string) The type, eg 'pubkeyhash'

        - `address`         - (string) bitcoin address

- `blockhash`         - (string) the block hash

- `confirmations`     - (numeric) The confirmations

- `blocktime`         - (numeric) The block time expressed in UNIX epoch time

- `time`              - (numeric) Same as "blocktime"

### Error Enum `TxNotFound`

- "Transaction not found"

### Error Enum `InvalidHash`

- "Invalid hash"

## Usage Examples

```bash
floresta-cli getrawtransaction <txid> [true|false]
```

## Notes

- This function only works for in-wallet transactions.