# `getnodeaddresses`

Return known peer addresses from the node's address manager. These can be used to find new peers in the network.

## Usage

### Synopsis

```
floresta-cli getnodeaddresses [count]
```

### Examples

```bash
floresta-cli getnodeaddresses
floresta-cli getnodeaddresses 10
floresta-cli getnodeaddresses 0
floresta-cli getnodeaddresses 10 ipv4
floresta-cli getnodeaddresses 0 onion
```

## Arguments

- `count` - (numeric, optional, default=1) The maximum number of addresses to return. Pass `0` to return all known addresses.

- `network` - (string, optional, default=all networks) Only return addresses from this network. One of: `ipv4`, `ipv6`, `onion`, `i2p`, `cjdns`.

## Returns

### Ok Response

A JSON array of address objects:

- `time` - (numeric) Unix timestamp of when this address was last seen
- `services` - (numeric) Service flags advertised by this peer
- `address` - (string) The IP address of the peer
- `port` - (numeric) The port number of the peer

### Error Response

- `Node` - Failed to retrieve addresses from the address manager

## Notes

- Addresses are sourced from the internal address manager and reflect peers that have been seen on the network.
- Passing `count=0` returns all known addresses.
- **Known limitation**: Tor and I2P addresses are serialized as debug-formatted strings rather than proper address representations. Bitcoin Core serializes these correctly. This will be improved in a future change.
