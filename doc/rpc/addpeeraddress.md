# `addpeeraddress`

Add an IP address and port to the node's address manager.

## Usage

### Synopsis

```text
floresta-cli addpeeraddress <address> [port] [tried]
```

### Examples

```bash
floresta-cli addpeeraddress 192.168.0.1
floresta-cli addpeeraddress 192.168.0.1 8333
floresta-cli addpeeraddress 192.168.0.1 8333 true
floresta-cli addpeeraddress "2001:db8::1" 8333 false
```

## Arguments

- `address` - (string, required) The IPv4 or IPv6 address of the peer.

- `port` - (numeric, optional, default=network default port) The port number the peer listens on. Defaults to the network's P2P port (e.g. 8333 for mainnet).

- `tried` - (boolean, optional, default=false) If `true`, the address is added directly to the _tried_ table, indicating a previously successful connection.

## Returns

### Ok Response

A JSON object with a single field:

- `success` - (boolean) `true` if the address was accepted by the address manager, `false` otherwise (e.g. the address is not routable or the address manager is full).

### Error Response

- `Node` - Failed to communicate with the address manager

## Notes

- Only IPv4 and IPv6 addresses are accepted; Tor, I2P, and CJDNS connections are not supported by floresta yet.
- Adding an address here does not immediately open a connection to the peer. Use `addnode` to establish a persistent connection.
