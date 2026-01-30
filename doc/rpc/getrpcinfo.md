# `getrpcinfo`

Returns information about the RPC server.

## Usage

### Synopsis

```bash
floresta-cli getrpcinfo
```

### Examples

```bash
floresta-cli getrpcinfo
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

A JSON object containing:

- `active_commands` - (json array) List of currently executing RPC commands.
  - `method` - (string) The name of the RPC command.
  - `duration` - (numeric) Running time in microseconds.
- `logpath` - (string) The complete file path to the debug log.

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- Useful for debugging and monitoring RPC server performance.
- The `active_commands` array shows commands currently being processed.
- Long-running commands may indicate performance issues.
- The `logpath` helps locate debug information for troubleshooting.
