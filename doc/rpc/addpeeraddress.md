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
floresta-cli addpeeraddress "example7abcdefg.onion" 8333
```

## Arguments

- `address` - (string, required) The IP address or onion address of the peer. Accepts IPv4 (`192.168.0.1`), IPv6 (`2001:db8::1`), or Tor v3 onion addresses. The port may be embedded in the address string (e.g. `192.168.0.1:8333` or `[2001:db8::1]:8333`), in which case the separate `port` parameter is not needed.

- `port` - (numeric, optional, default=network default port) The port number the peer listens on. Defaults to the network's P2P port (e.g. 8333 for mainnet). If a port is already embedded in the `address` string, this parameter should be omitted.

- `tried` - (boolean, optional, default=false) If `true`, the address is added directly to the _tried_ table, indicating a previously successful connection.

## Returns

### Ok Response

A JSON object with a single field:

- `success` - (boolean) `true` if the address was accepted by the address manager, `false` otherwise (e.g. the address is not routable or the address manager is full).

### Error Response

- `Node` - Failed to communicate with the address manager

## Notes

- IPv4, IPv6, and Tor v3 onion addresses are accepted. I2P and CJDNS are not currently supported.
- Adding an address here does not immediately open a connection to the peer. Use `addnode` to establish a persistent connection.
- There are corner cases where the command runs fine but the address can be denied. Internally on the Address Manager, we insert address with an method
  called `push_addresses`, which may deny some addresses. For further details, check the source code or the method documentation if its available.
