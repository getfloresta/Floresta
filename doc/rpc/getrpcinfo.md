# `getrpcinfo`

Returns information about the running RPC server.

## Usage

### Synopsis

```bash
floresta-cli getrpcinfo
```

### Examples

```bash
# Get the current state of the RPC server
floresta-cli getrpcinfo
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

* `active_commands` - (json array) All commands currently being processed by the server;
  * `method` - (string) The name of the RPC command;
  * `duration` - (numeric) How long the command has been running for, in microseconds.
* `logpath` - (string) The full path to the node's debug log file.

### Error Enum `CommandError`

This command typically does not return any specific logic errors under normal operation.

## Notes

* `active_commands` only lists commands that are still in flight at the moment this RPC is called, so an empty array is the common case for a node that isn't under heavy RPC load.
* `logpath` is useful for locating the debug log without having to know the node's data directory layout in advance.
