# `getnodeaddresses`

Return known peer addresses from the node's address manager. These can be used to find new peers in the network.

## Usage

### Synopsis

```text
floresta-cli getnodeaddresses [count] [network]
```

### Examples

```bash
floresta-cli getnodeaddresses
floresta-cli getnodeaddresses 10
floresta-cli getnodeaddresses 0
floresta-cli getnodeaddresses 10 ipv4
floresta-cli getnodeaddresses 0 ipv6
floresta-cli getnodeaddresses 0 onion
```

## Arguments

- `count` - (numeric, optional, default=1) The maximum number of addresses to return. Pass `0` to return all known addresses.

- `network` - (string, optional, default="all" networks) Only return addresses from this network. One of: `ipv4`, `ipv6`, `onion`, `i2p`, `cjdns`.

## Returns

### Ok Response

A JSON array of address objects:

- `time` - (numeric) Unix timestamp of when this address was last seen
- `services` - (numeric) Service flags advertised by this peer
- `address` - (string) The address of the peer (IP address or `.onion` hostname)
- `port` - (numeric) The port number of the peer
- `network` - (string) The network the address belongs to (`ipv4`, `ipv6`, or `onion`)

### Error Response

- `InvalidParameterType` - A parameter is invalid.
- `Node` - Failed to retrieve addresses from the address manager

## Notes

- Results are shuffled randomly before being returned, making it harder to fingerprint or eclipse the node.
- Addresses are returned from the full address manager, including untried and recently-failed peers. Only banned addresses are excluded. This matches Bitcoin Core's behavior of returning all "not terrible" addresses.
- The `onion` filter returns Tor v3 (`.onion`) addresses. The `i2p` and `cjdns` filters are accepted but currently return an empty list, since Floresta does not yet support connections over those networks.
