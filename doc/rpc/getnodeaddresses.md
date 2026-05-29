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
```

## Arguments

- `count` - (numeric, optional, default=1) The maximum number of addresses to return. Pass `0` to return all known addresses.

- `network` - (string, optional, default="all" networks) Only return addresses from this network. One of: `ipv4`, `ipv6`.

## Returns

### Ok Response

A JSON array of address objects:

- `time` - (numeric) Unix timestamp of when this address was last seen
- `services` - (numeric) Service flags advertised by this peer
- `address` - (string) The IP address of the peer
- `port` - (numeric) The port number of the peer
- `network` - (string) The network the address belongs to (`ipv4` or `ipv6`)

### Error Response

- `InvalidParameterType` - A parameter is invalid.
- `Node` - Failed to retrieve addresses from the address manager

## Notes

- This command only supports networks that Floresta can currently connect to: `ipv4` and `ipv6`.
- The addresses are fetched from an internal index of the Address Manager for good peers, those that have had their connections validated. Because of this, we can't return Cjdns, I2P, or Tor addresses, even if they somehow end up in the Address Manager.
