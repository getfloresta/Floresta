# `getaddednodeinfo`

Return information about nodes that were manually added via `addnode`.

## Usage

### Synopsis

```
floresta-cli getaddednodeinfo
```

### Examples

```bash
floresta-cli getaddednodeinfo
```

## Arguments

None.

## Returns

### Ok Response

A JSON array of objects, one per added node:

- `addednode` - (string) The address of the node in `ip:port` format
- `connected` - (boolean) Whether the node is currently connected

### Error Response

- `Node` - Failed to retrieve added node information

## Notes

- Only nodes explicitly added via `addnode add` appear here; peers discovered automatically are not included.
- A node can be in the list but not currently connected (e.g. if the connection failed or was dropped).
