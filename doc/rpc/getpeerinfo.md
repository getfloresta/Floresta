# `getpeerinfo`

Returns general information about the peers we are currently connected to.

## Usage

### Synopsis

```bash
floresta-cli getpeerinfo
```

### Examples

```bash
# Get information about connected peers
floresta-cli getpeerinfo
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

Returns a JSON array of objects, each representing a connected peer with the following fields:

- `id` - (numeric) This peer's unique identifier in the node's peer manager. This is useful for commands like `disconnectnode` which can target a peer by its ID.
- `address` - (string) The network address and port for this peer (e.g., "192.168.1.5:8333"). This helps identify where the connection is established.
- `services` - (string) A 16-character hexadecimal bitfield with the services this peer advertises (e.g., "0000000000000c09"). This indicates what capabilities the peer supports and what data we can request from them.
- `servicesnames` - (array of strings) Human-readable names for the recognized services this peer advertises (e.g., "NETWORK", "WITNESS", "P2P_V2"). Unknown service bits are still represented in `services`.
- `user_agent` - (string) The User Agent string representing the client software and version being used by the peer (e.g., `/Satoshi-26.0/`). Useful for identifying the software distribution on the network.
- `initial_height` - (numeric) The block height this peer reported when the connection was first established. This may differ from the current chain tip if the peer has not announced new blocks since connecting.
- `kind` - (string) The connection type of this peer. Possible values are "feeler" (short-lived connections to test address validity), "regular" (standard persistent P2P connection), "extra", or "manual".
- `state` - (string) The current state of this peer. Can be "Ready" (fully handshaked and active), "Awaiting" (still establishing connection), or "Banned" (connection rejected/dropped).
- `transport_protocol` - (string) The transport protocol used to communicate with the peer (e.g., "V1" or "V2").

### Error Enum

* `JsonRpcError::Node` - If there is an internal node error preventing the retrieval of peer information (e.g., "Failed to get peer information").

## Notes

- This RPC method has a direct equivalent in Bitcoin Core. However, Floresta's `getpeerinfo` is a more lightweight version that currently returns a subset of essential connection and state information, whereas Bitcoin Core provides extensive additional telemetry (like bytes sent/received, ping times, etc.).
