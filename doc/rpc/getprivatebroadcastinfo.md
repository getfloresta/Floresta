# `getprivatebroadcastinfo`

Return a snapshot of transactions in the private-broadcast queue.

## Usage

### Synopsis

```
floresta-cli getprivatebroadcastinfo
```

### Examples

```bash
floresta-cli getprivatebroadcastinfo
```

## Arguments

None.

## Returns

### Ok Response

A JSON object with a `transactions` array. Each element is an object with:

- `txid` - (string) The transaction id
- `wtxid` - (string) The witness transaction id
- `hex` - (string) The serialized transaction
- `time_added` - (numeric) UNIX time when the transaction was enqueued
- `peers` - (array) Per-peer relay state. Each entry contains:
  - `address` - (string) The onion peer address
  - `sent` - (numeric) UNIX time when the transaction was assigned to this peer
  - `received` - (numeric, optional) UNIX time when receipt was confirmed

### Error Response

- `Node` - Failed to retrieve the private-broadcast queue from the node

## Notes

- Only useful when the node was started with `--private-broadcast` and a SOCKS5 proxy; otherwise the queue is typically empty.
- For cancelling a queued broadcast, see [`abortprivatebroadcast`](abortprivatebroadcast.md).
