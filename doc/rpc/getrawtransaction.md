# `getrawtransaction`


Returns detailed data about a specified transaction.

As it is, `getrawtransaction`  only returns the given transaction when its cached by the watch-only wallet. And you can control the amount of information you want to be returned using the `verbose` argum.

## Arguments

* `txid` - (string, required) The transaction id

* `verbose` - (boolean, optional) When set, returns more human-readable and detailed data from the specified transaction, otherwise it will just return the hex encoded transaction.

## Returns

### Response (without `verbose`)

- The serialized transaction as a hex-encoded string.

### Response (when `verbose` is set)

```json5
{
    "in_active_chain": true,        // (boolean) Whether specified block is in the active chain or not (only present with explicit "blockhash" argument)

    "hex": "str",                   // (string) The serialized, hex-encoded data for 'txid'

    "txid": "str",                  // (string) The transaction id (same as provided)
    
    "hash": "str",                  // (string) The transaction hash (differs from txid for witness transactions)
    
    "size": 123,                    // (numeric) The serialized transaction size
    
    "vsize": 123,                   // (numeric) The virtual transaction size (differs from size for witness transactions)
    
    "weight": 123,                  // (numeric) The transaction's weight (between vsize*4-3 and vsize*4)
    
    "version": 123,                 // (numeric) The version
    
    "locktime": 123,                // (numeric) The lock time
    
    "vin": [                        // (array)
        {                           // (object)
            "txid": "str",          // (string) The transaction id

            "vout": 123,            // (numeric) The output number

            "script_sig": {         // (object) The script
                "asm": "str",       // (string) asm

                "hex": "str",       // (string) hex
            },

            "sequence": 123,        // (numeric) The script sequence number

            "witness": [            // (array)
                "str"               // hex-encoded witness data (if any)
            ],
        }
    ],

    "vout": [                       // (array)
        {                           // (object)
            "value": 123,           // (numeric) The value in BTC

            "n": 123,               // (numeric) index

            "script_pub_key":  {    // (object)
                "asm": "str",       // (string) the asm

                "hex": "str",       // (string) the hex

                "req_sigs": 123,    // (numeric) The required sigs

                "type_": "str",     // (string) The type, eg 'pubkeyhash'

                "address": "str",   // (string) bitcoin address
            }
        }
    ],

    "blockhash": "str",             // (string) the block hash

    "confirmations": 123,           // (numeric) The confirmations

    "blocktime": 123,               // (numeric) The block time expressed in UNIX epoch time

    "time": 123                     // (numeric) Same as "blocktime"
}
```

### Error Enum 

- `TxNotFound`
- `InvalidHash`

## Usage Examples

```bash
floresta-cli getrawtransaction <txid> [true|false]
```

## Notes

- This function only works for transactions that are cached inside the watch-only wallet.