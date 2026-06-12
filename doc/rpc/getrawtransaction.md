# `getrawtransaction`

Returns a transaction that is cached in the wallet. This method retrieves transactions
that have been tracked and indexed by the wallet during blockchain synchronization or
through explicit tracking. The response format can be controlled via the verbosity parameter.

## Usage

### Synopsis

```
floresta-cli getrawtransaction <txid> [<verbosity>]
```

### Examples

```bash
# Get transaction as serialized hex string
floresta-cli getrawtransaction "d7d7558342e3aff8fc4ae0a84bc7d19c0add4dfa1a3c0fa56cf66dc4f2400145"

# Get transaction as detailed JSON object
floresta-cli getrawtransaction "d7d7558342e3aff8fc4ae0a84bc7d19c0add4dfa1a3c0fa56cf66dc4f2400145" 1

# Default behavior (hex format)
floresta-cli getrawtransaction "d7d7558342e3aff8fc4ae0a84bc7d19c0add4dfa1a3c0fa56cf66dc4f2400145" 0
```

## Arguments

-`txid` - (string, required) The transaction ID to retrieve.
- `verbosity` - (numeric, optional, default=1)
  * `0`: Returns transaction as a raw serialized hexadecimal string.
  * `1`: Returns transaction as a detailed JSON object with decoded fields.

## Returns

### Ok Response (verbosity = 0)

- `"hex"` - (string) A serialized, hex-encoded string of the transaction data.

### Ok Response (verbosity = 1)

- `in_active_chain` - (boolean) Whether the transaction is in the active blockchain
- `blockhash` - (string, optional) The block hash containing this transaction
- `confirmations` - (numeric, optional) Number of confirmations (0 if in mempool)
- `blocktime` - (numeric, optional) Block creation time as Unix timestamp
- `time` - (numeric, optional) Transaction time as Unix timestamp
- `hex` - (string) The serialized, hex-encoded data for 'txid'
- `txid` - (string) The transaction id (same as provided)
- `hash` - (string) The transaction hash (differs from txid for witness transactions)
- `size` - (numeric) The transaction size in bytes
- `vsize` - (numeric) The virtual transaction size (differs from size for witness transactions)
- `weight` - (numeric) The transaction's weight (between vsize*4-3 and vsize*4)
- `version` - (numeric) The transaction version
- `locktime` - (numeric) The block height or timestamp at which transaction is final
- `vin` - (array) Array of transaction inputs
  * `coinbase` - (string) The hex-encoded coinbase script data. **Only if coinbase transaction**
  * `txid` - (string) The previous transaction ID. **Only if non-coinbase transaction**
  * `vout` - (numeric) The output index in the previous transaction. **Only if non-coinbase transaction**
  * `script_sig` - (object) Contains the scriptSig. **Only if non-coinbase transaction**
    - `hex` - (string) The script hex
  * `sequence` - (numeric) The sequence number
  * `txinwitness` - (array, optional) Witness stack
    - `hex` - (string) hex-encoded witness data (if any)
- `vout` - (array) Array of transaction outputs
  * `value` - (numeric) Amount in btc
  * `n` - (numeric) The output index
  * `script_pub_key` - (object) Contains the scriptPubKey
    - `asm` - (string) Disassembly of the output script
    - `hex` - (string) The raw output script bytes, hex-encoded
    - `type` - (string) The type of script (pubkey, pubkeyhash, multisig, etc.)
    - `address` - (string, optional) The Bitcoin address (if applicable)

### Error Enum `JsonRpcError`

- `TxNotFound` - The requested transaction is not found in the wallet cache
- `InvalidVerbosityLevel` - Verbosity parameter is not 0 or 1
- `MissingParameter` - The txid parameter was not provided
- `InvalidParameterType` - The txid is not a valid hex string
- `Decode` - Error during transaction decoding or serialization
