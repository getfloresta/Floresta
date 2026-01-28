# `sendrawtransaction`

Submits a raw transaction to the Bitcoin network.

## Usage

### Synopsis

```bash
floresta-cli sendrawtransaction <tx>
```

### Examples

```bash
floresta-cli sendrawtransaction "0100000001..."
```

## Arguments

- `tx` - (string, required) The raw transaction as a hex-encoded string. Must be a valid, fully signed Bitcoin transaction.

## Returns

### Ok Response

- `txid` - (string) The transaction ID (txid) of the submitted transaction, as a hex-encoded string.

### Error Enum `CommandError`

Returns an error if:
- The transaction is malformed or invalid.
- The transaction fails consensus rules.
- The transaction double-spends an existing transaction.
- The transaction's inputs are already spent.

## Notes

- The transaction must be fully signed before submission.
- You can create raw transactions using Bitcoin libraries or other wallet software.
- Once broadcast, the transaction cannot be easily reversed.
- The transaction will be relayed to connected peers for inclusion in a block.
