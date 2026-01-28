# `disconnectnode`

Immediately disconnect from a peer given its address or node id.

## Usage

### Synopsis

```bash
floresta-cli disconnectnode <ip:[port]> <nodeid>
```

### Examples

```bash
floresta-cli disconnectnode 1.2.3.4:8333 
floresta-cli disconnectnode "" 0
```

## Arguments

- `address` - (string, optional, default=fallback to nodeid) The IP address/port of the node

- `nodeid` - (numeric, optional, default=fallback to address) The node ID (see `getpeerinfo` for node IDs)

## Returns

### Ok response

- json null

### Error response

- `InvalidAddress` - The peer address format is invalid 
- `PeerNotFound` - No peer found with the specified address or node ID
- `InvalidDisconnectNodeCommand` - Invalid command usage (either both address and nodeid were provided, or neither was provided)
- `Node` - Failed to disconnect from the peer

## Notes

If referencing a node by its address, the nodeid argument is optional.
If referencing a node by its nodeid, the address MUST be an empty string.
