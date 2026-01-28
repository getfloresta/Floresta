# `ping`

Sends a ping message to all connected peers to check if they are still alive.

## Usage

### Synopsis

```bash
floresta-cli ping
```

### Examples

```bash
floresta-cli ping
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

- json null

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- Requests that each peer sends a `pong` response.
- Useful for testing connectivity with peers.
- Does not return the ping times; it only initiates the ping.
- Peers that don't respond may be disconnected.
- Use `getpeerinfo` to see detailed peer connection status.
