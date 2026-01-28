# `getpeerinfo`

Returns information about the connected peers.

## Usage

### Synopsis

```bash
floresta-cli getpeerinfo
```

### Examples

```bash
floresta-cli getpeerinfo
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

An array of JSON objects, one for each connected peer:

- `addr` - (string) The IP address and port of the peer.
- `services` - (string) The services offered by the peer.
- `version` - (numeric) The peer's protocol version.
- `subver` - (string) The peer's user agent string.
- `inbound` - (boolean) Whether the connection is inbound.
- `startingheight` - (numeric) The peer's block height when we connected.
- `banscore` - (numeric) The peer's ban score.
- `synced_headers` - (numeric) The last header we have in common with this peer.
- `synced_blocks` - (numeric) The last block we have in common with this peer.
- `connection_type` - (string) Type of connection (e.g., "outbound-full-relay").
- `transport_protocol_type` - (string) The P2P transport protocol (v1 or v2).

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- Use this to monitor your node's network connectivity.
- A healthy node should have multiple peers.
- The `banscore` increases when peers send invalid data.
- v2 transport provides encrypted P2P communication (BIP324).
