# `abortprivatebroadcast`

Abort private broadcast for a transaction identified by txid or wtxid.

## Usage

### Synopsis

```
floresta-cli abortprivatebroadcast <txid|wtxid>
```

### Examples

```bash
floresta-cli abortprivatebroadcast 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

## Arguments

`id` - (string, required) The transaction id or witness transaction id (hex).

## Returns

### Ok Response

A JSON object with a `removed_transactions` array. Each element is an object with:

- `txid` - (string) The transaction id
- `wtxid` - (string) The witness transaction id
- `hex` - (string) The serialized transaction

### Error Response

- `InvalidHex` - The id is not valid hex or not a recognized txid/wtxid
- `TxNotFound` - No queued transaction matches the id
- `Node` - Failed to abort private broadcast on the node

## Notes

- Removes the transaction from the private-broadcast queue and cancels Tor outbound slots that have not yet confirmed receipt.
- Does not remove a transaction from the public mempool if it was already accepted through another path.
- For inspecting the queue, see [`getprivatebroadcastinfo`](getprivatebroadcastinfo.md).
