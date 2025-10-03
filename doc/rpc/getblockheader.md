# `getblockheader`

Returns the block header for the given block hash.

This method returns the block header for the given block hash, as defined in the Bitcoin protocol specification.

A raw header contains, in hex format, the block's version, the previous block hash, the merkle root, the timestamp, the difficulty target, and the nonce.

A verbose header will show additional information about the block/blockheader: the blockhash itself, the number of confirmations, height, version (and its hexadecimal format),
mediantime, chainwork, number of transactions, the previousblockhash and nextblockhash.

If `verbose` is false (default), the command will return a string that is serialized, hex-encoded data for blockheader `hash`.

If `verbose` is true, returns an Object with information about blockheader `hash`.

## Usage

### Synopsis

```bash
floresta-cli getblockheader <hash> [true|false]
```

### Examples

```bash
floresta-cli getblockheader 00000000ba63ae2eeb3d2371708291b90507c3317ef957f6fcba3811cc4fe0cc
floresta-cli getblockheader 00000000ba63ae2eeb3d2371708291b90507c3317ef957f6fcba3811cc4fe0cc true
floresta-cli getblockheader 00000000ba63ae2eeb3d2371708291b90507c3317ef957f6fcba3811cc4fe0cc false
```

## Arguments

`hex` - (string, required) The block hash

`verbose` - (boolean, optional) true for a json object, false for the hex-encoded data

## Returns

### Ok Response

* `verbose = false`: (`serde_json::Value<String>`) -  A serialized hexadecimal string in the format `<version><previousblockhash><merkleroot><time><bits><nonce>`;
* `verbose = true`: (`serde_json::Value<floresta_rpc::rpc_types::GetBlockResVerbose>`) with the format:

```json5
{
  "hash": "hex",                  // (string) the block hash (same as provided)
  "confirmations" : n,            // (numeric) The number of confirmations, or -1 if the block is not on the main chain
  "height" : n,                   // (numeric) The block height or index
  "version" : n,                  // (numeric) The block version
  "versionHex" : "hex",           // (string) The block version formatted in hexadecimal
  "merkleroot" : "hex",           /// (string) The merkle root
  "time" : xxx,                   // (numeric) The block time expressed in UNIX epoch time
  "mediantime" : xxx,             // (numeric) The median block time expressed in UNIX epoch time
  "nonce" : n,                    // (numeric) The nonce
  "bits" : "hex",                 // (string) The bits
  "difficulty" : n,               // (numeric) The difficulty
  "chainwork" : "hex",            // (string) Expected number of hashes required to produce the current chain
  "nTx" : n,                      // (numeric) The number of transactions in the block
  "previousblockhash" : "hex",    // (string) The hash of the previous block
  "nextblockhash" : "hex"         // (string) The hash of the next block
}
```

### Error 

Any of the error types on `rpc_types::Error`.

## Notes

Will print the hexadecimal format unless the parameter `verbose` is specified to `true`, else will print a json string.
