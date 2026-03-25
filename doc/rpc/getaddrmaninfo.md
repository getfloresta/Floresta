# `getaddrmaninfo`

Return statistics from the node's address manager, broken down by network type.

## Usage

### Synopsis

```
floresta-cli getaddrmaninfo
```

### Examples

```bash
floresta-cli getaddrmaninfo
```

## Arguments

None.

## Returns

### Ok Response

A JSON object with one key per network plus an `all_networks` summary. Each value is an object with:

- `total` - (numeric) Total number of addresses known for this network
- `new` - (numeric) Number of addresses that have never been tried
- `tried` - (numeric) Number of addresses that have been successfully connected to

Top-level keys:

- `all_networks` - Aggregate counts across all networks
- `ipv4` - IPv4 addresses
- `ipv6` - IPv6 addresses
- `onion` - Tor v3 addresses
- `i2p` - I2P addresses
- `cjdns` - CJDNS addresses

### Error Response

- `Node` - Failed to retrieve statistics from the address manager

## Notes

- An address is counted as `tried` if its state is `Tried` or `Connected`; all other states count as `new`.
- `total` equals `new + tried` for each network.
