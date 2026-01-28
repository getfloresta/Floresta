# `uptime`

Returns the number of seconds that the Floresta node has been running.

## Usage

### Synopsis

```bash
floresta-cli uptime
```

### Examples

```bash
floresta-cli uptime
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

- (numeric) The number of seconds since the node was started.

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- Useful for monitoring node stability and uptime.
- The counter starts when florestad is launched.
- Can be used in scripts to detect recent restarts.
- Does not persist across restarts.
