# `getaddednodeinfo`

Return information about nodes that were manually added.

## Usage

### Synopsis

```text
floresta-cli getaddednodeinfo [node]
```

### Examples

```bash
floresta-cli getaddednodeinfo
floresta-cli getaddednodeinfo 192.168.0.1:8333
```

## Arguments

- `node` - (string, optional) If provided, return information only about this added node. The value should be an IP address or `ip:port`. If only an IP is given, port 8333 is assumed.

## Returns

### Ok Response

A JSON array of objects, one per added node:

- `addednode` - (string) The address of the node in `ip:port` format
- `connected` - (boolean) Whether the node is currently connected
- `addresses` - (array, only when connected) Connection details:
  - `address` - (string) The peer address in `ip:port` format
  - `connected` - (string) The connection direction (`"outbound"`)

### Error Response

- `Node` - Failed to retrieve added node information
- `InvalidAddress` - The provided node address could not be parsed

## Notes

- Only manually nodes added will appear here; peers discovered automatically are not included.
- Floresta does not accept inbound connections yet, so the `connected` field in the `addresses` array is always `"outbound"`.
