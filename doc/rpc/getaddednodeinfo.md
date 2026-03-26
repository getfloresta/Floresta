# `getaddednodeinfo`

Returns information about the given added node, or all added nodes.

## Usage

### Synopsis

```bash
floresta-cli getaddednodeinfo [node]
```

### Examples

```bash
floresta-cli getaddednodeinfo
floresta-cli getaddednodeinfo 192.168.0.1:8333
```

## Arguments

`node` - (string, optional) If provided, return information about this specific node. Can be an `ip:port` or just an `ip`.

## Returns

### Ok response

```json
[
  {
    "addednode": "192.168.0.1:8333",
    "connected": true,
    "addresses": [
      {
        "address": "192.168.0.1:8333",
        "connected": "outbound"
      }
    ]
  }
]
```

- `addednode` - (string) The node address in `ip:port` format, as provided to `addnode`.
- `connected` - (boolean) Whether the node is currently connected.
- `addresses` - (json array) A list of addresses with connection direction info. Empty when not connected.
  - `address` - (string) The address in `ip:port` format.
  - `connected` - (string) Connection direction: `"outbound"` when connected.

## Notes

- Only nodes added via `addnode add` are listed. Nodes connected with `addnode onetry` are not included.
- The `addresses` array is empty when `connected` is `false`.
